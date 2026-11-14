//! Unit and hostile integration tests for pure-Rust kynex-audio crate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use kynex_audio::{
    compute_master_volume, parse_server_info_json, parse_sinks_json, parse_subscribe_line,
    parse_volume_string, parse_volume_value, resolve_audio_status, sanitize_device_name,
    AudioError, AudioEvent, BoundedLineReader, CoalesceAction, EventCoalescer, ServerInfo,
    SinkInfo, MAX_DEVICES, MAX_DEVICE_NAME_CHARS, MAX_LINE_LENGTH,
};
use serde_json::json;

#[test]
fn test_sanitize_device_name_strips_controls_and_bidi() {
    // Contains LRO, RLO, LRI, RLI, PDI, ALM, NUL, ESC, BEL
    let hostile =
        "Built-in \u{202A}Audio\u{202E} \u{2066}Analog\u{2069}\u{0000}\u{001B}[31m \u{061C}Stereo";
    let sanitized = sanitize_device_name(hostile);
    assert_eq!(sanitized, "Built-in Audio Analog[31m Stereo");

    // Length capping
    let giant_name = "A".repeat(200);
    let capped = sanitize_device_name(&giant_name);
    assert_eq!(capped.chars().count(), MAX_DEVICE_NAME_CHARS);
    assert_eq!(capped.len(), 128);
}

#[test]
fn test_compute_master_volume_multi_channel_rule() {
    // Empty channels
    assert_eq!(compute_master_volume(&[]), 0);

    // Single channel
    assert_eq!(compute_master_volume(&[50]), 50);

    // Multi-channel average: left 40, right 60 -> 50
    assert_eq!(compute_master_volume(&[40, 60]), 50);

    // Rounding to nearest: 33 + 34 = 67 / 2 = 33.5 -> 34
    assert_eq!(compute_master_volume(&[33, 34]), 34);

    // Clamping to 0-100
    assert_eq!(compute_master_volume(&[0, 0]), 0);
    assert_eq!(compute_master_volume(&[100, 100]), 100);
    assert_eq!(compute_master_volume(&[200, 500]), 100);
}

#[test]
fn test_parse_volume_string_hostile_inputs() {
    assert_eq!(parse_volume_string("0%"), 0);
    assert_eq!(parse_volume_string("50%"), 50);
    assert_eq!(parse_volume_string("100%"), 100);
    assert_eq!(parse_volume_string("45.6%"), 46);
    assert_eq!(parse_volume_string("-5.2%"), 0);
    assert_eq!(parse_volume_string("150.8%"), 100);

    // Negative values clamp to 0
    assert_eq!(parse_volume_string("-1%"), 0);
    assert_eq!(parse_volume_string("-100"), 0);

    // Oversized values clamp to 100
    assert_eq!(parse_volume_string("101%"), 100);
    assert_eq!(parse_volume_string("4294967295%"), 100);
    assert_eq!(parse_volume_string("999999999999999999999"), 100);

    // NaN / inf strings
    assert_eq!(parse_volume_string("nan"), 0);
    assert_eq!(parse_volume_string("NaN%"), 0);
    assert_eq!(parse_volume_string("inf"), 0);
    assert_eq!(parse_volume_string("-inf"), 0);
    assert_eq!(parse_volume_string("not-a-number"), 0);
}

#[test]
fn test_parse_volume_value_json_representations() {
    assert_eq!(parse_volume_value(&json!(50)), 50);
    assert_eq!(parse_volume_value(&json!(-10)), 0);
    assert_eq!(parse_volume_value(&json!(150)), 100);
    assert_eq!(parse_volume_value(&json!({"value": 32768})), 50); // PA scale 65536 = 100%
    assert_eq!(parse_volume_value(&json!("75%")), 75);
    assert_eq!(parse_volume_value(&json!({"value_percent": "60%"})), 60);
    assert_eq!(parse_volume_value(&json!({"value": 65536})), 100);
    assert_eq!(parse_volume_value(&json!(72.4)), 72);
    assert_eq!(parse_volume_value(&json!(-5.5)), 0);
    assert_eq!(parse_volume_value(&json!(110.5)), 100);
    assert_eq!(parse_volume_value(&json!(null)), 0);
    assert_eq!(parse_volume_value(&json!(true)), 0);
    assert_eq!(parse_volume_value(&json!({})), 0);
    assert_eq!(parse_volume_value(&json!({"value": "80%"})), 80);
}

#[test]
fn test_parse_server_info_json() {
    let payload = json!({
        "default_sink_name": "alsa_output.pci-0000_00_03.0.analog-stereo",
        "server_name": "PulseAudio (on PipeWire 1.6.2)"
    })
    .to_string();

    let info = parse_server_info_json(&payload).expect("Valid server info");
    assert_eq!(
        info.default_sink_name,
        "alsa_output.pci-0000_00_03.0.analog-stereo"
    );

    // Missing default_sink_name field defaults to empty string
    let missing_field = json!({ "other": 123 }).to_string();
    let info2 = parse_server_info_json(&missing_field).expect("Parsed missing field");
    assert_eq!(info2.default_sink_name, "");

    // Malformed JSON error
    let err = parse_server_info_json("{bad json").unwrap_err();
    assert!(matches!(err, AudioError::JsonError(_)));
}

#[test]
fn test_parse_sinks_json_and_device_limit() {
    // Generate 70 devices (exceeding MAX_DEVICES = 64)
    let mut sinks_vec = Vec::new();
    for i in 0..70 {
        sinks_vec.push(json!({
            "index": i,
            "name": format!("sink_{i}"),
            "description": format!("Device \u{202A}{i}\u{202C}"),
            "mute": if i % 2 == 0 { "yes" } else { "no" },
            "volume": {
                "front-left": {"value_percent": format!("{}%", i.min(100))},
                "front-right": {"value_percent": format!("{}%", i.min(100))}
            }
        }));
    }

    let payload = serde_json::to_string(&sinks_vec).expect("Serialized sinks");
    let sinks = parse_sinks_json(&payload).expect("Parsed sinks");

    // Must be capped at exactly MAX_DEVICES (64)
    assert_eq!(sinks.len(), MAX_DEVICES);
    assert_eq!(sinks[0].index, 0);
    assert_eq!(sinks[0].description, "Device 0");
    assert!(sinks[0].muted);
    assert!(!sinks[1].muted);
    assert_eq!(sinks[63].index, 63);

    // Non-array json error
    let non_array = json!({"not": "array"}).to_string();
    let err = parse_sinks_json(&non_array).unwrap_err();
    assert!(matches!(err, AudioError::JsonError(_)));

    // Fallback to property description
    let prop_sink = json!([{
        "index": 1,
        "name": "raw_name",
        "properties": {
            "device.description": "Property Device"
        },
        "mute": true,
        "volume": 80
    }])
    .to_string();
    let parsed_prop = parse_sinks_json(&prop_sink).expect("Parsed prop sink");
    assert_eq!(parsed_prop[0].description, "Property Device");
    assert_eq!(parsed_prop[0].volume, 80);
}

#[test]
fn test_resolve_audio_status() {
    let server = ServerInfo {
        default_sink_name: "target_sink".to_string(),
    };

    let sinks = vec![
        SinkInfo {
            index: 1,
            name: "other_sink".to_string(),
            description: "Other".to_string(),
            muted: false,
            volume: 30,
            channel_volumes: vec![30],
        },
        SinkInfo {
            index: 2,
            name: "target_sink".to_string(),
            description: "Target Headphones".to_string(),
            muted: true,
            volume: 75,
            channel_volumes: vec![75],
        },
    ];

    let status = resolve_audio_status(&server, &sinks);
    assert!(status.present);
    assert_eq!(status.volume, 75);
    assert!(status.muted);
    assert_eq!(status.device_name, "Target Headphones");

    // Default sink name missing -> falls back to first sink
    let server_unknown = ServerInfo {
        default_sink_name: "nonexistent".to_string(),
    };
    let fallback_status = resolve_audio_status(&server_unknown, &sinks);
    assert_eq!(fallback_status.device_name, "Other");
    assert_eq!(fallback_status.volume, 30);

    // Empty sinks -> not present
    let empty_status = resolve_audio_status(&server, &[]);
    assert!(!empty_status.present);
    assert_eq!(empty_status.volume, 0);
}

#[test]
fn test_parse_subscribe_line() {
    assert_eq!(
        parse_subscribe_line("Event 'change' on sink #55"),
        Some(AudioEvent::SinkChanged(Some(55)))
    );
    assert_eq!(
        parse_subscribe_line("Event 'new' on sink"),
        Some(AudioEvent::SinkChanged(None))
    );
    assert_eq!(
        parse_subscribe_line("Event 'change' on server #0"),
        Some(AudioEvent::ServerChanged)
    );
    assert_eq!(
        parse_subscribe_line("Event 'change' on card #51"),
        Some(AudioEvent::Other("Event 'change' on card #51".to_string()))
    );
    assert_eq!(parse_subscribe_line("   "), None);
}

#[test]
fn test_bounded_line_reader_line_limit_and_overflow() {
    let mut reader = BoundedLineReader::new();

    // Normal line under 4 KiB
    let lines = reader.feed(b"Normal line 1\nSecond line\n");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].as_ref().unwrap(), "Normal line 1");
    assert_eq!(lines[1].as_ref().unwrap(), "Second line");

    // Flush empty reader
    assert!(reader.flush().is_none());

    // Flush partial line without newline
    let _ = reader.feed(b"Unterminated partial");
    let flushed = reader.flush();
    assert_eq!(flushed.unwrap().unwrap(), "Unterminated partial");

    // Massive line without newline (> 4096 bytes)
    let giant_chunk = vec![b'X'; MAX_LINE_LENGTH + 500];
    let res1 = reader.feed(&giant_chunk);
    assert_eq!(res1.len(), 0);

    // Another 10,000 bytes without newline - memory stays bounded!
    let second_giant_chunk = vec![b'Y'; 10_000];
    let res2 = reader.feed(&second_giant_chunk);
    assert_eq!(res2.len(), 0);

    // Flush in overflow state yields LineTooLong
    let overflow_flush = reader.flush();
    assert_eq!(
        overflow_flush.unwrap(),
        Err(AudioError::LineTooLong(MAX_LINE_LENGTH))
    );

    // Feed another giant line and terminate with newline
    let _ = reader.feed(&giant_chunk);
    let res3 = reader.feed(b"\nRecovered line\n");
    assert_eq!(res3.len(), 2);
    assert_eq!(res3[0], Err(AudioError::LineTooLong(MAX_LINE_LENGTH)));
    assert_eq!(res3[1].as_ref().unwrap(), "Recovered line");
}

#[test]
fn test_event_coalescer_timing_and_reset() {
    let mut coalescer = EventCoalescer::default();

    // First event at t=100ms -> immediate
    assert_eq!(
        coalescer.handle_event(100),
        CoalesceAction::TriggerImmediate
    );

    // Rapid second event at t=105ms -> schedule debounce for 20ms
    assert_eq!(
        coalescer.handle_event(105),
        CoalesceAction::ScheduleDebounce(20)
    );

    // Rapid third event at t=110ms -> already scheduled
    assert_eq!(
        coalescer.handle_event(110),
        CoalesceAction::AlreadyScheduled
    );

    // Debounced task runs at t=125ms
    coalescer.on_debounced_run(125);

    // Next event at t=160ms (> 25ms elapsed) -> immediate
    assert_eq!(
        coalescer.handle_event(160),
        CoalesceAction::TriggerImmediate
    );

    // Reset resets internal state
    coalescer.reset();
    assert_eq!(
        coalescer.handle_event(161),
        CoalesceAction::TriggerImmediate
    );
}
