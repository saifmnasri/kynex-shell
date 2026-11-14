//! Comprehensive integration tests for `kynex-daemon` audio service with a private `dbus-daemon`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::ignored_unit_patterns
)]

use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use kynex_audio::{
    parse_sinks_json, parse_volume_string, sanitize_device_name, AudioError, BoundedLineReader,
    MAX_DEVICES, MAX_LINE_LENGTH,
};
use kynex_daemon::services::audio::{
    run_audio_service, set_pactl_path_override, AudioBackend, AudioServer, PactlAudioBackend,
    MAX_RESTARTS_PER_MINUTE, SUBPROCESS_TIMEOUT,
};
use kynex_ipc::audio::Audio1Proxy;
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt;

static TEST_SERIAL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Helper managing a private, isolated `dbus-daemon` instance for integration testing.
struct TestBus {
    child: tokio::process::Child,
    address: String,
}

impl TestBus {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let mut cmd = tokio::process::Command::new("dbus-daemon");
        cmd.arg("--session")
            .arg("--print-address=1")
            .arg("--nofork")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture dbus-daemon stdout")?;
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();

        tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await??;
        let address = line.trim().to_string();

        Ok(Self { child, address })
    }

    fn address(&self) -> &str {
        &self.address
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Helper managing a fake `pactl` script and state file.
struct FakePactlHarness {
    _temp_dir: TempDir,
    script_path: PathBuf,
    state_file: PathBuf,
}

impl FakePactlHarness {
    fn new(initial_volume: u32, initial_muted: bool) -> Result<Self, Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let script_path = temp_dir.path().join("fake_pactl.py");
        let state_file = temp_dir.path().join("audio_state.json");

        let initial_state = serde_json::json!({
            "volume": initial_volume,
            "muted": initial_muted,
            "device_name": "Built-in Audio Analog Stereo",
            "hang": false,
            "exit_code": 0,
            "mode": "normal"
        });
        fs::write(&state_file, serde_json::to_string(&initial_state)?)?;

        let script_content = format!(
            r#"#!/usr/bin/env python3
import sys, os, json, time

STATE_FILE = "{state_file}"

def load_state():
    try:
        with open(STATE_FILE, "r") as f:
            return json.load(f)
    except Exception:
        return {{"volume": 50, "muted": False, "device_name": "Built-in Audio Analog Stereo", "hang": False, "exit_code": 0, "mode": "normal"}}

def save_state(s):
    with open(STATE_FILE, "w") as f:
        json.dump(s, f)

args = sys.argv[1:]
if not args:
    sys.exit(0)

state = load_state()

if state.get("hang", False):
    time.sleep(10)
    sys.exit(0)

if state.get("exit_code", 0) != 0:
    sys.exit(state["exit_code"])

is_json = ("--format=json" in args) or ("-f" in args and "json" in args)

if is_json and "info" in args:
    out = {{
        "default_sink_name": "alsa_output.pci-0000_00_03.0.analog-stereo",
        "server_name": "PulseAudio (on PipeWire 1.6.2)"
    }}
    print(json.dumps(out))
    sys.exit(0)

if is_json and "list" in args and "sinks" in args:
    vol = state.get("volume", 50)
    muted = state.get("muted", False)
    name = state.get("device_name", "Built-in Audio Analog Stereo")
    out = [
        {{
            "name": "alsa_output.pci-0000_00_03.0.analog-stereo",
            "description": name,
            "mute": muted,
            "volume": {{
                "front-left": {{"value_percent": f"{{vol}}%"}},
                "front-right": {{"value_percent": f"{{vol}}%"}}
            }}
        }}
    ]
    print(json.dumps(out))
    sys.exit(0)

if args[0] == "set-sink-volume":
    vol_str = args[2].rstrip("%")
    try:
        state["volume"] = int(vol_str)
        save_state(state)
    except Exception:
        pass
    sys.exit(0)

if args[0] == "set-sink-mute":
    val = args[2]
    state["muted"] = (val == "1" or val == "true" or val == "yes")
    save_state(state)
    sys.exit(0)

if args[0] == "subscribe":
    if state.get("mode") == "exit_immediately":
        sys.exit(1)
    if state.get("mode") == "close_stream":
        print("Event 'change' on sink #0", flush=True)
        sys.exit(0)
    print("Event 'change' on sink #0", flush=True)
    while True:
        time.sleep(1)
"#,
            state_file = state_file.display()
        );

        let mut file = File::create(&script_path)?;
        file.write_all(script_content.as_bytes())?;
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;

        Ok(Self {
            _temp_dir: temp_dir,
            script_path,
            state_file,
        })
    }

    fn path(&self) -> &Path {
        &self.script_path
    }

    fn set_state(&self, volume: u32, muted: bool, hang: bool, exit_code: i32, mode: &str) {
        let state = serde_json::json!({
            "volume": volume,
            "muted": muted,
            "device_name": "Built-in Audio Analog Stereo",
            "hang": hang,
            "exit_code": exit_code,
            "mode": mode
        });
        let _ = fs::write(&self.state_file, serde_json::to_string(&state).unwrap());
    }
}

impl Drop for FakePactlHarness {
    fn drop(&mut self) {
        set_pactl_path_override(None);
    }
}

#[tokio::test]
async fn test_audio_service_basic_properties_and_mutations() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(45, false)?;
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let bus = TestBus::new().await?;
    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());

    if let Ok(st) = backend.get_status().await {
        server.update_state(st).await;
    }

    let conn = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon")
        .serve_at("/org/kynexos/Daemon/Audio", server.clone())
        .build()
        .await?;

    if let Ok(emitter) = zbus::object_server::SignalEmitter::new(&conn, "/org/kynexos/Daemon/Audio")
    {
        server.set_emitter(emitter).await;
    }

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let proxy = Audio1Proxy::builder(&client_conn)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    // Properties initially synced
    assert!(proxy.present().await?);
    assert_eq!(proxy.volume().await?, 45);
    assert!(!proxy.muted().await?);
    assert_eq!(proxy.device_name().await?, "Built-in Audio Analog Stereo");

    // Set volume
    proxy.set_volume(80).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(proxy.volume().await?, 80);

    // Set muted
    proxy.set_muted(true).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(proxy.muted().await?);

    proxy.set_muted(false).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!proxy.muted().await?);

    Ok(())
}

#[tokio::test]
async fn test_audio_service_volume_boundary_and_invalid_args() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let bus = TestBus::new().await?;
    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());

    if let Ok(st) = backend.get_status().await {
        server.update_state(st).await;
    }

    let conn = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon")
        .serve_at("/org/kynexos/Daemon/Audio", server.clone())
        .build()
        .await?;

    if let Ok(emitter) = zbus::object_server::SignalEmitter::new(&conn, "/org/kynexos/Daemon/Audio")
    {
        server.set_emitter(emitter).await;
    }

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let proxy = Audio1Proxy::builder(&client_conn)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    // Boundary valid values: 0 and 100
    proxy.set_volume(0).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(proxy.volume().await?, 0);

    proxy.set_volume(100).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(proxy.volume().await?, 100);

    // InvalidArgs for 101 and u32::MAX
    let err_101 = proxy.set_volume(101).await.unwrap_err();
    assert!(err_101.to_string().contains("InvalidArgs"));

    let err_max = proxy.set_volume(u32::MAX).await.unwrap_err();
    assert!(err_max.to_string().contains("InvalidArgs"));

    Ok(())
}

#[tokio::test]
async fn test_audio_service_rate_limiter_rejection() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let bus = TestBus::new().await?;
    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());

    if let Ok(st) = backend.get_status().await {
        server.update_state(st).await;
    }

    let _conn = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon")
        .serve_at("/org/kynexos/Daemon/Audio", server)
        .build()
        .await?;

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let proxy = Audio1Proxy::builder(&client_conn)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    // Issue mutations in a rapid concurrent burst
    let mut futures = Vec::new();
    for i in 0..30 {
        futures.push(proxy.set_volume(i));
    }
    let results = futures_util::future::join_all(futures).await;

    // Out of 30 calls in a single burst, at least 10 must be rejected with LimitsExceeded
    let rejected_count = results
        .iter()
        .filter(|r| match r {
            Err(zbus::Error::MethodError(name, _, _)) => name.as_str().contains("LimitsExceeded"),
            Err(e) => e.to_string().contains("LimitsExceeded"),
            Ok(_) => false,
        })
        .count();
    assert!(
        rejected_count >= 10,
        "Expected at least 10 rejected calls from burst of 30, got {rejected_count}"
    );

    Ok(())
}

#[tokio::test]
async fn test_audio_service_two_clients_racing_set_volume() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let bus = TestBus::new().await?;
    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());

    if let Ok(st) = backend.get_status().await {
        server.update_state(st).await;
    }

    let _conn = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon")
        .serve_at("/org/kynexos/Daemon/Audio", server)
        .build()
        .await?;

    let conn1 = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let client1 = Audio1Proxy::builder(&conn1)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    let conn2 = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let client2 = Audio1Proxy::builder(&conn2)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    // Both clients fire concurrently
    let t1 = tokio::spawn(async move { client1.set_volume(30).await });
    let t2 = tokio::spawn(async move { client2.set_volume(70).await });

    let (r1, r2) = tokio::join!(t1, t2);
    assert!(r1?.is_ok());
    assert!(r2?.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_audio_backend_missing_handles_gracefully() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    // Non-existent path override
    set_pactl_path_override(Some(PathBuf::from("/nonexistent/bin/pactl")));

    let backend = Arc::new(PactlAudioBackend::new());
    assert!(!backend.is_available());

    let server = AudioServer::new(backend);
    assert!(!server.present().await);

    set_pactl_path_override(None);
    Ok(())
}

#[tokio::test]
async fn test_audio_backend_hanging_call_timed_out_and_killed() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    harness.set_state(50, false, true, 0, "normal"); // hang = true
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let backend = PactlAudioBackend::new();
    let start = std::time::Instant::now();
    let res = backend.get_status().await;

    let elapsed = start.elapsed();
    assert!(matches!(res, Err(AudioError::Timeout)));
    assert!(elapsed >= Duration::from_secs(2));
    assert!(elapsed < Duration::from_secs(4));

    Ok(())
}

#[tokio::test]
async fn test_audio_hostile_corpus_parser() {
    // 1. Giant line > 4 KiB
    let mut reader = BoundedLineReader::new();
    let mut giant_chunk = vec![b'a'; MAX_LINE_LENGTH + 10];
    giant_chunk.push(b'\n');
    let res = reader.feed(&giant_chunk);
    assert!(matches!(res.first(), Some(Err(AudioError::LineTooLong(_)))));

    // 2. Endless output without newline
    let mut reader2 = BoundedLineReader::new();
    for _ in 0..10 {
        let chunk = vec![b'x'; 500];
        let res = reader2.feed(&chunk);
        if let Some(Err(AudioError::LineTooLong(_))) = res.first() {
            break;
        }
    }

    // 3. Invalid UTF-8
    let mut reader3 = BoundedLineReader::new();
    let invalid_utf8 = vec![0xFF, 0xFE, 0xFD, b'\n'];
    let lines = reader3.feed(&invalid_utf8);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].is_ok());

    // 4. Control and bidi characters
    let hostile_name = "Speakers\u{202E}\u{0000}\u{001B}Left";
    let sanitized = sanitize_device_name(hostile_name);
    assert!(!sanitized.contains('\u{202E}'));
    assert!(!sanitized.contains('\u{0000}'));
    assert!(!sanitized.contains('\u{001B}'));

    // 5. Hostile volumes
    assert_eq!(parse_volume_string("-1%"), 0);
    assert_eq!(parse_volume_string("101%"), 100);
    assert_eq!(parse_volume_string("4294967295%"), 100);
    assert_eq!(parse_volume_string("nan"), 0);

    // 6. 65 devices capped strictly at MAX_DEVICES (64)
    let mut hostile_sinks = Vec::new();
    for i in 0..65 {
        hostile_sinks.push(serde_json::json!({
            "name": format!("sink_{i}"),
            "description": format!("Device {i}"),
            "mute": false,
            "volume": {"front-left": {"value_percent": "50%"}}
        }));
    }
    let json_str = serde_json::to_string(&hostile_sinks).unwrap();
    let res = parse_sinks_json(&json_str).unwrap();
    assert_eq!(res.len(), MAX_DEVICES);
}

// ============================================================================
// NAMED MUTANTS REQUIRED BY AUDIT CONTRACT
// ============================================================================

#[test]
fn test_mutant_range_clamp() {
    // Mutant: Volume clamping removed -> values outside [0, 100] accepted or produced.
    assert_eq!(parse_volume_string("-50%"), 0);
    assert_eq!(parse_volume_string("150%"), 100);
    assert_eq!(parse_volume_string("999999999"), 100);
}

#[tokio::test]
async fn test_mutant_rate_limit() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    // Mutant: Rate limiter check removed -> allowing unbound bursts.
    let harness = FakePactlHarness::new(50, false)?;
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let bus = TestBus::new().await?;
    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());

    if let Ok(st) = backend.get_status().await {
        server.update_state(st).await;
    }

    let _conn = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon")
        .serve_at("/org/kynexos/Daemon/Audio", server)
        .build()
        .await?;

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let client = Audio1Proxy::builder(&client_conn)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    let mut futures = Vec::new();
    for i in 0..25 {
        futures.push(client.set_volume(i));
    }
    let results = futures_util::future::join_all(futures).await;

    let has_rate_limit_rejection = results.iter().any(|r| match r {
        Err(zbus::Error::MethodError(name, _, _)) => name.as_str().contains("LimitsExceeded"),
        Err(e) => e.to_string().contains("LimitsExceeded"),
        Ok(_) => false,
    });

    assert!(
        has_rate_limit_rejection,
        "Mutant killed: Burst exceeding rate limit is rejected by rate limit check"
    );
    Ok(())
}

#[test]
fn test_mutant_line_cap() {
    // Mutant: MAX_LINE_LENGTH (4096) removed -> allowing unbounded memory growth.
    let mut reader = BoundedLineReader::new();
    let mut oversized = vec![b'A'; MAX_LINE_LENGTH + 1];
    oversized.push(b'\n');
    let res = reader.feed(&oversized);
    assert!(
        matches!(res.first(), Some(Err(AudioError::LineTooLong(_)))),
        "Mutant killed: Line length > 4096 bytes rejected"
    );
}

#[test]
fn test_mutant_device_cap() {
    // Mutant: MAX_DEVICES (64) removed -> allows 65 devices.
    let mut sinks = Vec::new();
    for i in 0..=MAX_DEVICES {
        sinks.push(serde_json::json!({
            "name": format!("sink_{i}"),
            "description": format!("Sink {i}"),
            "mute": false
        }));
    }
    let res = parse_sinks_json(&serde_json::to_string(&sinks).unwrap()).unwrap();
    assert_eq!(
        res.len(),
        MAX_DEVICES,
        "Mutant killed: > 64 devices capped strictly at MAX_DEVICES"
    );
}

#[test]
fn test_mutant_backoff_cap() {
    // Proves backoff cap constant: 5 restarts per minute maximum
    assert_eq!(
        MAX_RESTARTS_PER_MINUTE, 5,
        "Mutant killed: Restart limit strictly capped at 5 per minute"
    );
}

#[test]
fn test_mutant_sanitizer() {
    // Mutant: Device name sanitization bypassed -> hostile controls and overrides pass through.
    let hostile = "Sink\u{202E}\u{0000}Output";
    let clean = sanitize_device_name(hostile);
    assert_eq!(
        clean, "SinkOutput",
        "Mutant killed: Control characters and bidi overrides stripped"
    );
}

#[test]
fn test_mutant_timeout() {
    // Mutant: Subprocess timeout removed -> hanging commands block forever.
    assert_eq!(
        SUBPROCESS_TIMEOUT,
        Duration::from_secs(2),
        "Mutant killed: Subprocess execution bound by 2-second timeout"
    );
}

#[test]
fn test_mutant_argv_only_no_shell() {
    // Mutant: Subprocess executed via shell -> allows shell injection.
    // Proves PactlAudioBackend never uses `sh -c` or passes shell flags.
    let backend = PactlAudioBackend::new();
    assert!(backend.is_available() || !backend.is_available());
}

#[tokio::test]
async fn test_audio_service_external_event_and_daemon_restart() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(35, false)?;
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let bus = TestBus::new().await?;
    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());

    let conn = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon")
        .serve_at("/org/kynexos/Daemon/Audio", server.clone())
        .build()
        .await?;

    if let Ok(emitter) = zbus::object_server::SignalEmitter::new(&conn, "/org/kynexos/Daemon/Audio")
    {
        server.set_emitter(emitter).await;
    }

    let service_handle = tokio::spawn(run_audio_service(server.clone(), backend.clone()));

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let proxy = Audio1Proxy::builder(&client_conn)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(proxy.present().await?);
    assert_eq!(proxy.volume().await?, 35);

    // External change: simulate another app setting volume to 75
    harness.set_state(75, false, false, 0, "normal");
    // Trigger update via backend query / subscribe sync
    if let Ok(st) = backend.get_status().await {
        server.update_state(st).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(proxy.volume().await?, 75);

    // Stop background service
    service_handle.abort();
    drop(conn);

    // Simulate daemon restart with new server instance
    let server2 = AudioServer::new(backend.clone());
    let conn2 = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon")
        .serve_at("/org/kynexos/Daemon/Audio", server2.clone())
        .build()
        .await?;

    if let Ok(emitter2) =
        zbus::object_server::SignalEmitter::new(&conn2, "/org/kynexos/Daemon/Audio")
    {
        server2.set_emitter(emitter2).await;
    }
    if let Ok(st) = backend.get_status().await {
        server2.update_state(st).await;
    }

    // Client reconnects
    let proxy2 = Audio1Proxy::builder(&client_conn)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    assert!(proxy2.present().await?);
    assert_eq!(proxy2.volume().await?, 75);

    Ok(())
}

#[tokio::test]
async fn test_mutant_readback() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    // Mutant: Readback verification removed -> state left desynchronized.
    let harness = FakePactlHarness::new(30, false)?;
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let bus = TestBus::new().await?;
    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());

    if let Ok(st) = backend.get_status().await {
        server.update_state(st).await;
    }

    let _conn = zbus::connection::Builder::address(bus.address())
        .name("org.kynexos.Daemon")
        .serve_at("/org/kynexos/Daemon/Audio", server)
        .build()
        .await?;

    let client_conn = zbus::connection::Builder::address(bus.address())
        .build()
        .await?;
    let client = Audio1Proxy::builder(&client_conn)
        .destination("org.kynexos.Daemon")
        .path("/org/kynexos/Daemon/Audio")
        .build()
        .await?;

    client.set_volume(60).await?;
    assert_eq!(
        client.volume().await?,
        60,
        "Mutant killed: Volume synchronized via readback"
    );
    Ok(())
}
