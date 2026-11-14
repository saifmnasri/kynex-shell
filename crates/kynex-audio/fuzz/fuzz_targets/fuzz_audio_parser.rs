#![no_main]

use kynex_audio::{
    compute_master_volume, parse_server_info_json, parse_sinks_json, parse_subscribe_line,
    parse_volume_string, resolve_audio_status, sanitize_device_name, BoundedLineReader,
    ServerInfo, MAX_DEVICES, MAX_DEVICE_NAME_CHARS, MAX_VOLUME, MIN_VOLUME,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // 1. Fuzz BoundedLineReader
    let mut reader = BoundedLineReader::new();
    let _ = reader.feed(data);
    let _ = reader.flush();

    // 2. Fuzz string sanitization
    let s = String::from_utf8_lossy(data);
    let sanitized = sanitize_device_name(&s);
    assert!(sanitized.chars().count() <= MAX_DEVICE_NAME_CHARS);

    // 3. Fuzz volume parser
    let vol = parse_volume_string(&s);
    assert!(vol >= MIN_VOLUME && vol <= MAX_VOLUME);

    // 4. Fuzz JSON parsers
    let parsed_server = parse_server_info_json(&s);
    let parsed_sinks = parse_sinks_json(&s);

    if let Ok(ref sinks) = parsed_sinks {
        assert!(sinks.len() <= MAX_DEVICES);
        for sink in sinks {
            assert!(sink.volume <= MAX_VOLUME);
            assert!(sink.description.chars().count() <= MAX_DEVICE_NAME_CHARS);
        }

        let dummy_server = parsed_server.unwrap_or(ServerInfo {
            default_sink_name: String::new(),
        });
        let status = resolve_audio_status(&dummy_server, sinks);
        assert!(status.volume <= MAX_VOLUME);
        assert!(status.device_name.chars().count() <= MAX_DEVICE_NAME_CHARS);
    }

    // 5. Fuzz subscribe parser
    let _ = parse_subscribe_line(&s);

    // 6. Fuzz compute_master_volume with arbitrary channel numbers
    let channels: Vec<u32> = data.iter().map(|&b| b as u32).collect();
    let master = compute_master_volume(&channels);
    assert!(master >= MIN_VOLUME && master <= MAX_VOLUME);
});
