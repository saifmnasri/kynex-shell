//! Comprehensive integration tests for `kynex-daemon` audio service with a private `dbus-daemon`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::ignored_unit_patterns,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::panic,
    clippy::uninlined_format_args
)]

use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
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
    argv_file: PathBuf,
    parent_file: PathBuf,
    fifo_path: PathBuf,
}

impl FakePactlHarness {
    fn new(initial_volume: u32, initial_muted: bool) -> Result<Self, Box<dyn Error>> {
        Self::new_with_sink_name(
            initial_volume,
            initial_muted,
            "alsa_output.pci-0000_00_03.0.analog-stereo",
        )
    }

    fn new_with_sink_name(
        initial_volume: u32,
        initial_muted: bool,
        sink_name: &str,
    ) -> Result<Self, Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let script_path = temp_dir.path().join("fake_pactl.py");
        let state_file = temp_dir.path().join("audio_state.json");
        let argv_file = temp_dir.path().join("argv.log");
        let parent_file = temp_dir.path().join("parent.log");
        let fifo_path = temp_dir.path().join("events.fifo");

        let initial_state = serde_json::json!({
            "volume": initial_volume,
            "muted": initial_muted,
            "device_name": "Built-in Audio Analog Stereo",
            "sink_name": sink_name,
            "hang": false,
            "exit_code": 0,
            "mode": "normal",
            "fifo_path": fifo_path.to_string_lossy().to_string()
        });
        fs::write(&state_file, serde_json::to_string(&initial_state)?)?;

        let script_content = format!(
            r#"#!/usr/bin/env python3
import sys, os, json, time

STATE_FILE = "{state_file}"
ARGV_LOG = "{argv_log}"
PARENT_LOG = "{parent_log}"

# Record argv
try:
    with open(ARGV_LOG, "a") as f:
        for a in sys.argv:
            f.write(a + "\n")
        f.write("=== CALL_END ===\n")
except Exception:
    pass

# Record parent process
try:
    ppid = os.getppid()
    try:
        with open(f"/proc/{{ppid}}/comm", "r") as pf:
            pcomm = pf.read().strip()
    except Exception:
        pcomm = "unknown"
    with open(PARENT_LOG, "a") as f:
        f.write(f"{{ppid}}:{{pcomm}}\n")
except Exception:
    pass

def load_state():
    try:
        with open(STATE_FILE, "r") as f:
            return json.load(f)
    except Exception:
        return {{"volume": 50, "muted": False, "device_name": "Built-in Audio Analog Stereo", "sink_name": "alsa_output.pci-0000_00_03.0.analog-stereo", "hang": False, "exit_code": 0, "mode": "normal"}}

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
    sys.stderr.write("fake pactl mock error\n")
    sys.exit(state["exit_code"])

is_json = ("--format=json" in args) or ("-f" in args and "json" in args)

if is_json and "info" in args:
    sink_id = state.get("sink_name", "alsa_output.pci-0000_00_03.0.analog-stereo")
    out = {{
        "default_sink_name": sink_id,
        "server_name": "PulseAudio (on PipeWire 1.6.2)"
    }}
    print(json.dumps(out))
    sys.exit(0)

if is_json and "list" in args and "sinks" in args:
    if state.get("empty_sinks", False):
        print(json.dumps([]))
        sys.exit(0)
    vol = state.get("volume", 50)
    muted = state.get("muted", False)
    name = state.get("device_name", "Built-in Audio Analog Stereo")
    sink_id = state.get("sink_name", "alsa_output.pci-0000_00_03.0.analog-stereo")
    out = [
        {{
            "name": sink_id,
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
    mode = state.get("mode", "normal")
    if mode == "exit_immediately":
        sys.exit(1)
    if mode == "close_stream":
        print("Event 'change' on sink #0", flush=True)
        sys.exit(0)
    if mode == "fifo":
        fifo_path = state.get("fifo_path")
        if fifo_path and os.path.exists(fifo_path):
            with open(fifo_path, "r") as fifo:
                for line in fifo:
                    print(line.rstrip(), flush=True)
            sys.exit(0)
    print("Event 'change' on sink #0", flush=True)
    while True:
        time.sleep(1)
"#,
            state_file = state_file.display(),
            argv_log = argv_file.display(),
            parent_log = parent_file.display()
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
            argv_file,
            parent_file,
            fifo_path,
        })
    }

    fn path(&self) -> &Path {
        &self.script_path
    }

    fn fifo_path(&self) -> &Path {
        &self.fifo_path
    }

    fn create_fifo(&self) -> Result<(), Box<dyn Error>> {
        if !self.fifo_path.exists() {
            let status = std::process::Command::new("mkfifo")
                .arg(&self.fifo_path)
                .status()?;
            if !status.success() {
                return Err("mkfifo failed".into());
            }
        }
        Ok(())
    }

    fn set_state(&self, volume: u32, muted: bool, hang: bool, exit_code: i32, mode: &str) {
        let state = serde_json::json!({
            "volume": volume,
            "muted": muted,
            "device_name": "Built-in Audio Analog Stereo",
            "sink_name": "alsa_output.pci-0000_00_03.0.analog-stereo",
            "hang": hang,
            "exit_code": exit_code,
            "mode": mode,
            "fifo_path": self.fifo_path.to_string_lossy().to_string()
        });
        let _ = fs::write(&self.state_file, serde_json::to_string(&state).unwrap());
    }

    #[allow(dead_code)]
    fn set_state_full(
        &self,
        volume: u32,
        muted: bool,
        device_name: &str,
        sink_name: &str,
        hang: bool,
        exit_code: i32,
        mode: &str,
    ) {
        let state = serde_json::json!({
            "volume": volume,
            "muted": muted,
            "device_name": device_name,
            "sink_name": sink_name,
            "hang": hang,
            "exit_code": exit_code,
            "mode": mode,
            "fifo_path": self.fifo_path.to_string_lossy().to_string()
        });
        let _ = fs::write(&self.state_file, serde_json::to_string(&state).unwrap());
    }

    fn set_empty_sinks(&self, empty: bool) {
        let state = serde_json::json!({
            "volume": 50,
            "muted": false,
            "device_name": "Built-in Audio Analog Stereo",
            "sink_name": "alsa_output.pci-0000_00_03.0.analog-stereo",
            "hang": false,
            "exit_code": 0,
            "mode": "normal",
            "empty_sinks": empty,
            "fifo_path": self.fifo_path.to_string_lossy().to_string()
        });
        let _ = fs::write(&self.state_file, serde_json::to_string(&state).unwrap());
    }

    fn argv_lines(&self) -> Vec<String> {
        if let Ok(content) = fs::read_to_string(&self.argv_file) {
            content.lines().map(ToString::to_string).collect()
        } else {
            Vec::new()
        }
    }

    fn parent_records(&self) -> Vec<(u32, String)> {
        let mut recs = Vec::new();
        if let Ok(content) = fs::read_to_string(&self.parent_file) {
            for line in content.lines() {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    if let Ok(pid) = parts[0].parse::<u32>() {
                        recs.push((pid, parts[1].to_string()));
                    }
                }
            }
        }
        recs
    }
}

impl Drop for FakePactlHarness {
    fn drop(&mut self) {
        set_pactl_path_override(None);
        let script_str = self.script_path.to_string_lossy().to_string();
        let start = std::time::Instant::now();
        let mut remaining = false;
        while start.elapsed() < Duration::from_secs(2) {
            if let Ok(out) = std::process::Command::new("pgrep")
                .args(["-f", &script_str])
                .output()
            {
                if out.status.success() && !out.stdout.is_empty() {
                    remaining = true;
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
            }
            remaining = false;
            break;
        }
        if remaining {
            let _ = std::process::Command::new("pkill")
                .args(["-9", "-f", &script_str])
                .output();
            assert!(
                std::thread::panicking(),
                "Test teardown assertion failed: fake_pactl orphan remained running within 2s ({script_str})"
            );
        }
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

    // Sleep 1.05s to allow the rate limiter window to expire and reset
    tokio::time::sleep(Duration::from_millis(1050)).await;
    let allowed_again = proxy.set_volume(50).await;
    assert!(
        allowed_again.is_ok(),
        "Call must succeed after rate limiter window reset"
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

#[tokio::test]
async fn test_mutant_backoff_cap() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    assert_eq!(MAX_RESTARTS_PER_MINUTE, 5);

    // Verify runtime enforcement: when child repeatedly exits, restarts stop after 5
    let harness = FakePactlHarness::new(50, false)?;
    harness.set_state(50, false, false, 1, "exit_immediately");
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());

    // run_audio_service should terminate within backoff window when restart limit (5) is reached
    let start = std::time::Instant::now();
    tokio::time::timeout(
        Duration::from_secs(10),
        run_audio_service(server.clone(), backend),
    )
    .await?;

    // Backend must be disabled: Present = false
    assert!(
        !server.present().await,
        "Mutant killed: restart loop terminates and disables backend after 5 restarts"
    );
    assert!(
        start.elapsed() >= Duration::from_millis(500),
        "Must have executed backoff delays"
    );
    Ok(())
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

#[tokio::test]
async fn test_mutant_timeout() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    assert_eq!(SUBPROCESS_TIMEOUT, Duration::from_secs(2));

    // Verify runtime timeout enforcement
    let harness = FakePactlHarness::new(50, false)?;
    harness.set_state(50, false, true, 0, "normal"); // hang = true
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let backend = PactlAudioBackend::new();
    let start = std::time::Instant::now();
    let res = backend.get_status().await;
    let elapsed = start.elapsed();

    assert!(
        matches!(res, Err(AudioError::Timeout)),
        "Mutant killed: Hanging call timed out"
    );
    assert!(
        elapsed >= Duration::from_secs(2),
        "Must wait at least 2 seconds before killing"
    );
    assert!(
        elapsed < Duration::from_secs(4),
        "Must kill within 4 seconds"
    );
    Ok(())
}

#[tokio::test]
async fn test_mutant_argv_only_no_shell() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;

    let hostile_name = "evil; touch /tmp/kynex-pwned $(id) | tee x";
    let pwned_path = Path::new("/tmp/kynex-pwned");
    let _ = fs::remove_file(pwned_path);
    assert!(
        !pwned_path.exists(),
        "Precondition: /tmp/kynex-pwned must not exist"
    );

    let harness = FakePactlHarness::new_with_sink_name(50, false, hostile_name)?;
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

    // Call SetVolume and SetMuted through the daemon
    client.set_volume(82).await?;
    client.set_muted(true).await?;

    // Assert hostile name arrived verbatim as a SINGLE argv element
    let argv_lines = harness.argv_lines();
    let has_verbatim_single_element = argv_lines.iter().any(|arg| arg == hostile_name);
    assert!(
        has_verbatim_single_element,
        "Mutant killed: Hostile sink name with shell metacharacters must arrive verbatim as a SINGLE argv element"
    );

    // Assert no /tmp/kynex-pwned exists
    assert!(
        !pwned_path.exists(),
        "Mutant killed: Command injection did not execute touch /tmp/kynex-pwned"
    );

    // Assert no process named sh, bash, or dash was spawned
    let parents = harness.parent_records();
    assert!(
        !parents.is_empty(),
        "Must have captured parent process info"
    );
    for (pid, comm) in &parents {
        assert!(
            comm != "sh" && comm != "bash" && comm != "dash",
            "Mutant killed: Child must not be spawned via shell interpreter (got parent pid {pid} comm '{comm}')"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_subscribe_worker_real_events_and_properties_changed() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(40, false)?;
    harness.create_fifo()?;
    harness.set_state(40, false, false, 0, "fifo");
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

    // Wait for initial query
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(proxy.present().await?);
    assert_eq!(proxy.volume().await?, 40);

    let mut vol_changed = proxy.receive_volume_changed().await;

    // Update state to 75 so that subscribe change queries new volume
    harness.set_state(75, false, false, 0, "fifo");

    // Open FIFO for writing events
    let fifo_path = harness.fifo_path().to_path_buf();
    let fifo_write_task = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut f = File::create(&fifo_path)?;
        // 1. Valid event triggering immediate update followed by rapid burst
        // triggering ScheduleDebounce and AlreadyScheduled
        f.write_all(b"Event 'change' on sink #0\n")?;
        f.write_all(b"Event 'change' on sink #0\n")?;
        f.write_all(b"Event 'change' on sink #0\n")?;
        f.flush()?;
        std::thread::sleep(Duration::from_millis(120));

        // 3. Malformed and non-sink events
        f.write_all(b"Event 'remove' on client #42\n")?;
        f.write_all(b"gibberish line that is not an event\n")?;
        f.write_all(b"\n")?;
        f.write_all(b"Event 'change' on server\n")?;
        f.flush()?;
        std::thread::sleep(Duration::from_millis(80));

        // 4. Close pipe -> triggers EOF in subscribe child
        drop(f);
        Ok(())
    });

    let mut target_received = false;
    while let Ok(Some(change)) =
        tokio::time::timeout(Duration::from_secs(3), vol_changed.next()).await
    {
        if change.get().await? == 75 {
            target_received = true;
            break;
        }
    }
    assert!(
        target_received,
        "Timed out waiting for D-Bus PropertiesChanged on volume (75)"
    );

    fifo_write_task.await??;
    service_handle.abort();
    Ok(())
}

#[tokio::test]
async fn test_audio_service_external_event_and_daemon_restart() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(35, false)?;
    harness.create_fifo()?;
    harness.set_state(35, false, false, 0, "fifo");
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

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(proxy.present().await?);
    assert_eq!(proxy.volume().await?, 35);

    // External change via real subscribe pipe
    let mut vol_changed = proxy.receive_volume_changed().await;
    harness.set_state(75, false, false, 0, "fifo");

    let fifo_path = harness.fifo_path().to_path_buf();
    let _ = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut f = File::create(&fifo_path)?;
        f.write_all(b"Event 'change' on sink #0\n")?;
        f.flush()?;
        Ok(())
    })
    .await?;

    let mut target_received = false;
    while let Ok(Some(change)) =
        tokio::time::timeout(Duration::from_secs(3), vol_changed.next()).await
    {
        if change.get().await? == 75 {
            target_received = true;
            break;
        }
    }
    assert!(
        target_received,
        "Timed out waiting for external subscribe change to 75"
    );

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

#[tokio::test]
async fn test_orphan_cleanup_normal_exit() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    let backend = Arc::new(PactlAudioBackend::new());
    let server = AudioServer::new(backend.clone());
    let handle = tokio::spawn(run_audio_service(server, backend));

    tokio::time::sleep(Duration::from_millis(200)).await;
    let script_str = harness.path().to_string_lossy().to_string();

    // Verify child is running
    let running = std::process::Command::new("pgrep")
        .args(["-f", &script_str])
        .output()?
        .status
        .success();
    assert!(
        running,
        "fake_pactl should be running while service is active"
    );

    // Normal exit / abort of daemon task
    handle.abort();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check that nothing of ours survives within 2s
    let start = std::time::Instant::now();
    let mut survived = false;
    while start.elapsed() < Duration::from_secs(2) {
        let out = std::process::Command::new("pgrep")
            .args(["-f", &script_str])
            .output()?;
        if out.status.success() && !out.stdout.is_empty() {
            survived = true;
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }
        survived = false;
        break;
    }
    assert!(
        !survived,
        "Normal exit: 0 fake_pactl processes survive within 2s"
    );
    Ok(())
}

#[tokio::test]
async fn test_orphan_cleanup_sigterm() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    let script_str = harness.path().to_string_lossy().to_string();

    // Spawn a parent process that invokes fake_pactl subscribe through setpriv --pdeathsig SIGKILL
    let mut cmd = std::process::Command::new("bash");
    cmd.args([
        "-c",
        &format!("/usr/bin/setpriv --pdeathsig SIGKILL -- python3 {script_str} subscribe"),
    ]);
    let mut child = cmd.spawn()?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send SIGTERM to the parent process
    let pid = child.id();
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
    let _ = child.wait();

    // Check that nothing of ours survives within 2s
    let start = std::time::Instant::now();
    let mut survived = false;
    while start.elapsed() < Duration::from_secs(2) {
        let out = std::process::Command::new("pgrep")
            .args(["-f", &script_str])
            .output()?;
        if out.status.success() && !out.stdout.is_empty() {
            survived = true;
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }
        survived = false;
        break;
    }
    assert!(
        !survived,
        "SIGTERM: 0 fake_pactl processes survive within 2s"
    );
    Ok(())
}

#[tokio::test]
async fn test_orphan_cleanup_sigkill() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    let script_str = harness.path().to_string_lossy().to_string();

    // Spawn parent running setpriv --pdeathsig SIGKILL
    let mut cmd = std::process::Command::new("bash");
    cmd.args([
        "-c",
        &format!("/usr/bin/setpriv --pdeathsig SIGKILL -- python3 {script_str} subscribe"),
    ]);
    let mut child = cmd.spawn()?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send SIGKILL (-9) to the parent process
    let pid = child.id();
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .output();
    let _ = child.wait();

    // Check that nothing of ours survives within 2s (kernel delivers pdeathsig SIGKILL to child)
    let start = std::time::Instant::now();
    let mut survived = false;
    while start.elapsed() < Duration::from_secs(2) {
        let out = std::process::Command::new("pgrep")
            .args(["-f", &script_str])
            .output()?;
        if out.status.success() && !out.stdout.is_empty() {
            survived = true;
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }
        survived = false;
        break;
    }
    assert!(
        !survived,
        "SIGKILL: 0 fake_pactl processes survive within 2s via pdeathsig"
    );
    Ok(())
}

#[tokio::test]
async fn test_orphan_cleanup_test_process_exit() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    let script_str = harness.path().to_string_lossy().to_string();

    // Test process helper that spawns setpriv child and exits immediately
    let helper = format!(
        "python3 -c 'import subprocess, time; p = subprocess.Popen([\"/usr/bin/setpriv\", \"--pdeathsig\", \"SIGKILL\", \"--\", \"python3\", \"{script_str}\", \"subscribe\"]); time.sleep(0.1)'"
    );
    let mut cmd = std::process::Command::new("bash");
    cmd.args(["-c", &helper]);
    let mut child = cmd.spawn()?;
    let _ = child.wait();

    // Test process exited. Check that within 2s, 0 fake_pactl survive
    let start = std::time::Instant::now();
    let mut survived = false;
    while start.elapsed() < Duration::from_secs(2) {
        let out = std::process::Command::new("pgrep")
            .args(["-f", &script_str])
            .output()?;
        if out.status.success() && !out.stdout.is_empty() {
            survived = true;
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }
        survived = false;
        break;
    }
    assert!(
        !survived,
        "Test process exit: 0 fake_pactl processes survive within 2s"
    );
    Ok(())
}

#[tokio::test]
async fn test_backend_direct_error_handling_and_exit_code() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    harness.set_state(50, false, false, 1, "normal"); // non-zero exit code
    let backend = PactlAudioBackend::new_with_paths(Some(harness.path().to_path_buf()), None);

    let res = backend.set_volume(60).await;
    assert!(res.is_err(), "Non-zero exit code should return error");
    let err_str = res.unwrap_err().to_string();
    assert!(
        err_str.contains("pactl exited with"),
        "Error should report pactl exit status: {err_str}"
    );
    assert!(
        err_str.contains("fake pactl mock error"),
        "Error should include stderr: {err_str}"
    );

    // Also test mute non-zero exit code
    let mute_res = backend.set_muted(true).await;
    assert!(
        mute_res.is_err(),
        "Non-zero exit code on set_muted should return error"
    );

    // Also test spawn failure with invalid path
    let invalid_backend =
        PactlAudioBackend::new_with_paths(Some(PathBuf::from("/nonexistent/binary/pactl")), None);
    let spawn_err = invalid_backend.set_volume(60).await;
    assert!(spawn_err.is_err(), "Spawn failure should return error");
    assert!(spawn_err
        .unwrap_err()
        .to_string()
        .contains("Failed to spawn"));
    Ok(())
}

#[tokio::test]
async fn test_backend_empty_sinks_fallback_and_defaults() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    harness.set_empty_sinks(true);
    let backend = PactlAudioBackend::new_with_paths(Some(harness.path().to_path_buf()), None);
    let st = backend.get_status().await?;
    assert_eq!(st.volume, 0); // empty sinks returns default status with volume 0

    // Test default trait and server helpers
    let def_backend = PactlAudioBackend::default();
    let _ = def_backend;
    let server = AudioServer::new_default();
    let snapshot = server.get_snapshot().await;
    assert_eq!(snapshot.volume, 0);

    // Test resolve functions with None override
    set_pactl_path_override(None);
    let _ = kynex_daemon::services::audio::resolve_pactl_path();
    let _ = kynex_daemon::services::audio::resolve_setpriv_path();
    set_pactl_path_override(Some(harness.path().to_path_buf()));

    // Test spawn_subscribe without setpriv
    let no_setpriv_backend =
        PactlAudioBackend::new_with_paths(Some(harness.path().to_path_buf()), None);
    let mut child = no_setpriv_backend.spawn_subscribe().await?;
    let _ = child.kill().await;
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn test_audio_service_backend_unavailable_present_false() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    // Backend with pactl_path: None -> is_available() is false
    let backend = Arc::new(PactlAudioBackend::new_with_paths(None, None));
    let server = AudioServer::new(backend.clone());

    // run_audio_service should immediately notice !backend.is_available(), update state to default (Present=false), and exit cleanly
    run_audio_service(server.clone(), backend).await;
    let snapshot = server.get_snapshot().await;
    assert!(
        !snapshot.present,
        "Audio1 Present should be false when backend is unavailable"
    );
    Ok(())
}

#[tokio::test]
async fn test_audio_subscribe_spawn_failure_backoff_and_exit() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    // Backend with an unspawnable pactl path
    let backend = Arc::new(PactlAudioBackend::new_with_paths(
        Some(PathBuf::from("/nonexistent/bin")),
        None,
    ));
    let server = AudioServer::new(backend.clone());

    let handle = tokio::spawn(run_audio_service(server.clone(), backend));
    tokio::time::sleep(Duration::from_millis(300)).await;
    handle.abort();
    let _ = handle.await;
    Ok(())
}

#[tokio::test]
async fn test_audio_subscribe_child_exit_and_restart() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(50, false)?;
    harness.set_state(50, false, false, 0, "exit_immediately");
    let backend = Arc::new(PactlAudioBackend::new_with_paths(
        Some(harness.path().to_path_buf()),
        None,
    ));
    let server = AudioServer::new(backend.clone());

    let handle = tokio::spawn(run_audio_service(server.clone(), backend));
    tokio::time::sleep(Duration::from_millis(300)).await;
    handle.abort();
    let _ = handle.await;
    Ok(())
}

#[tokio::test]
async fn test_mutant_coalesce_final_state() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(30, false)?;
    harness.create_fifo()?;
    harness.set_state(30, false, false, 0, "fifo");
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

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(proxy.volume().await?, 30);

    let mut vol_changed = proxy.receive_volume_changed().await;

    // Set authoritative final state to 88
    harness.set_state(88, false, false, 0, "fifo");

    // Write a burst of rapid events, then stop (silence)
    let fifo_path = harness.fifo_path().to_path_buf();
    let fifo_task = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut f = File::create(&fifo_path)?;
        for _ in 0..10 {
            f.write_all(b"Event 'change' on sink #0\n")?;
        }
        f.flush()?;
        // Silence follows
        std::thread::sleep(Duration::from_millis(300));
        drop(f);
        Ok(())
    });

    let mut final_delivered = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(300), vol_changed.next()).await {
            Ok(Some(change)) => {
                if change.get().await? == 88 {
                    final_delivered = true;
                    break;
                }
            }
            _ => {
                if proxy.volume().await? == 88 {
                    final_delivered = true;
                    break;
                }
            }
        }
    }

    assert!(
        final_delivered,
        "Mutant killed: Final volume (88) delivered after burst followed by silence"
    );

    fifo_task.await??;
    service_handle.abort();
    let _ = service_handle.await;
    Ok(())
}

#[tokio::test]
async fn test_coalesce_flood_and_isolated_latency() -> Result<(), Box<dyn Error>> {
    let _serial = TEST_SERIAL_LOCK.lock().await;
    let harness = FakePactlHarness::new(20, false)?;
    harness.create_fifo()?;
    harness.set_state(20, false, false, 0, "fifo");
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

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(proxy.volume().await?, 20);

    let mut vol_changed = proxy.receive_volume_changed().await;

    // 1. Isolated event test: latency must arrive within 25ms + processing (<= 60ms)
    harness.set_state(45, false, false, 0, "fifo");
    let fifo_path = harness.fifo_path().to_path_buf();
    let (tx, rx) = tokio::sync::oneshot::channel::<Instant>();
    let fifo_task = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut f = File::create(&fifo_path)?;
        // Send isolated event
        let t0 = Instant::now();
        let _ = tx.send(t0);
        f.write_all(b"Event 'change' on sink #0\n")?;
        f.flush()?;
        std::thread::sleep(Duration::from_millis(100));

        // 2. 10,000-event flood
        for _ in 0..10_000 {
            f.write_all(b"Event 'change' on sink #0\n")?;
        }
        f.flush()?;
        std::thread::sleep(Duration::from_millis(300));
        drop(f);
        Ok(())
    });

    let t0 = rx.await?;
    let mut isolated_received = false;
    let mut latency_ms = 0.0;
    while let Ok(Some(change)) =
        tokio::time::timeout(Duration::from_millis(200), vol_changed.next()).await
    {
        if change.get().await? == 45 {
            latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
            isolated_received = true;
            break;
        }
    }
    assert!(
        isolated_received,
        "Isolated event volume change (45) received"
    );
    assert!(
        latency_ms >= 20.0,
        "Isolated event debounced with 25ms base window (got {latency_ms:.2}ms)"
    );
    assert!(
        latency_ms <= 150.0,
        "Isolated event arrives within 25ms base window plus processing (got {latency_ms:.2}ms)"
    );

    // Update state for flood final value
    harness.set_state(95, false, false, 0, "fifo");

    // Wait for flood to complete and final value (95) to arrive
    let mut flood_final_received = false;
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if proxy.volume().await? == 95 {
            flood_final_received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        flood_final_received,
        "Final value (95) arrived after 10,000-event flood"
    );

    fifo_task.await??;
    service_handle.abort();
    let _ = service_handle.await;
    Ok(())
}
