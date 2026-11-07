#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let sanitized = kynex_notifications::sanitize_body(s);

        // Invariant 1: Always bounded by MAX_BODY_BYTES
        assert!(sanitized.len() <= kynex_notifications::MAX_BODY_BYTES);

        // Invariant 2: Always valid UTF-8
        assert!(std::str::from_utf8(sanitized.as_bytes()).is_ok());

        // Invariant 3: Stripped of control characters (except \n, \t)
        assert!(!sanitized.chars().any(|c| (c < ' ' && c != '\t' && c != '\n') || c == '\x7F'));

        // Invariant 4: Stripped of bidi overrides and directional isolate characters
        assert!(!sanitized.chars().any(|c| matches!(
            c,
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
        )));

        // Invariant 5: Summary sanitizer is bounded by MAX_SUMMARY_BYTES
        let summary = kynex_notifications::sanitize_summary(s);
        assert!(summary.len() <= kynex_notifications::MAX_SUMMARY_BYTES);

        // Invariant 6: App name sanitizer is bounded by MAX_APP_NAME_BYTES
        let app_name = kynex_notifications::sanitize_app_name(s);
        assert!(app_name.len() <= kynex_notifications::MAX_APP_NAME_BYTES);
    }
});
