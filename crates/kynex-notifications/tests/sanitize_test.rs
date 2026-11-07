//! Sanitization, markup stripping, entity decoding, and limit validation unit tests.
#![allow(clippy::panic)]

use kynex_notifications::{
    sanitize_action_label, sanitize_app_name, sanitize_body, sanitize_summary, sanitize_text,
    truncate_to_char_boundary, validate_action_key, validate_and_sanitize_actions,
    NotificationError, MAX_APP_NAME_BYTES, MAX_BODY_BYTES, MAX_SUMMARY_BYTES,
};

#[test]
fn test_html_tag_stripping() {
    assert_eq!(sanitize_text("<b>Bold</b>", 100), "Bold");
    assert_eq!(
        sanitize_text("<a href=\"https://example.com\">Click here</a>", 100),
        "Click here"
    );
    assert_eq!(sanitize_text("<img src=\"file:///etc/passwd\">", 100), "");
    assert_eq!(
        sanitize_text("Text with <i>italic</i> and <u>underline</u>.", 100),
        "Text with italic and underline."
    );
    assert_eq!(
        sanitize_text("<div class=\"box\"><p>Paragraph</p></div>", 100),
        "Paragraph"
    );
}

#[test]
fn test_script_and_style_removal() {
    // Both tag and its internal body are eliminated
    assert_eq!(
        sanitize_text("<script>alert('XSS');</script>Normal text", 100),
        "Normal text"
    );
    assert_eq!(
        sanitize_text(
            "<SCRIPT TYPE=\"text/javascript\">var x = 1;</SCRIPT>Hello",
            100
        ),
        "Hello"
    );
    assert_eq!(
        sanitize_text("<style>body { display: none; }</style>Content", 100),
        "Content"
    );
}

#[test]
fn test_entity_decoding() {
    assert_eq!(sanitize_text("A &amp; B", 100), "A & B");
    assert_eq!(sanitize_text("&lt;escaped tag&gt;", 100), "<escaped tag>");
    assert_eq!(sanitize_text("&quot;Quoted&quot;", 100), "\"Quoted\"");
    assert_eq!(sanitize_text("&apos;Single&apos;", 100), "'Single'");
    assert_eq!(sanitize_text("Non&nbsp;breaking", 100), "Non breaking");
    // Decimal numeric entity: 65 = 'A'
    assert_eq!(sanitize_text("&#65;&#66;&#67;", 100), "ABC");
    // Hex numeric entity: 0x41 = 'A'
    assert_eq!(sanitize_text("&#x41;&#x42;&#x43;", 100), "ABC");
}

#[test]
fn test_control_character_filtering() {
    // ASCII control chars (0x00 - 0x1F except \n and \t, and 0x7F) are dropped
    let input = "Hello\x00\x01\x02\x07\x08World\x7F!\nLine 2\tTabbed";
    let sanitized = sanitize_text(input, 100);
    assert_eq!(sanitized, "HelloWorld!\nLine 2\tTabbed");
}

#[test]
fn test_bidi_override_filtering() {
    // Directional overrides (RLO, LRO, PDF, isolates) dropped to prevent UI text inversion
    let input = "filename\u{202E}cod.exe"; // RLO spoofing
    let sanitized = sanitize_text(input, 100);
    assert_eq!(sanitized, "filenamecod.exe");

    let isolates = "user\u{2066}evil\u{2069}domain";
    assert_eq!(sanitize_text(isolates, 100), "userevildomain");
}

#[test]
fn test_utf8_char_boundary_truncation() {
    // '🦀' is 4 bytes: [0xF0, 0x9F, 0xA6, 0x80]
    let ferris = "🦀🦀🦀";
    assert_eq!(ferris.len(), 12);

    // Truncating to 5 bytes should not split the second crab; slices cleanly to 4 bytes (1 crab)
    assert_eq!(truncate_to_char_boundary(ferris, 5), "🦀");
    assert_eq!(truncate_to_char_boundary(ferris, 8), "🦀🦀");
    assert_eq!(truncate_to_char_boundary(ferris, 11), "🦀🦀");
    assert_eq!(truncate_to_char_boundary(ferris, 12), "🦀🦀🦀");
    assert_eq!(truncate_to_char_boundary(ferris, 15), "🦀🦀🦀");
    assert_eq!(truncate_to_char_boundary(ferris, 0), "");
}

#[test]
fn test_field_length_caps() {
    let huge_text = "A".repeat(10_000);

    let app_name = sanitize_app_name(&huge_text);
    assert!(app_name.len() <= MAX_APP_NAME_BYTES);
    assert_eq!(app_name.len(), 64);

    let summary = sanitize_summary(&huge_text);
    assert!(summary.len() <= MAX_SUMMARY_BYTES);
    assert_eq!(summary.len(), 256);

    let body = sanitize_body(&huge_text);
    assert!(body.len() <= MAX_BODY_BYTES);
    assert_eq!(body.len(), 4096);

    let label = sanitize_action_label(&huge_text);
    assert!(label.len() <= 64);
}

#[test]
fn test_action_key_validation() {
    // Valid keys
    assert!(validate_action_key("default").is_ok());
    assert!(validate_action_key("open-app_1.0").is_ok());

    // Empty key rejected
    assert!(validate_action_key("").is_err());

    // Key exceeding 32 chars rejected
    let long_key = "a".repeat(33);
    assert_eq!(
        validate_action_key(&long_key),
        Err(NotificationError::LimitsExceeded(
            "Action key exceeds 32 characters"
        ))
    );

    // Disallowed characters rejected
    assert!(validate_action_key("has space").is_err());
    assert!(validate_action_key("key/with/slashes").is_err());
    assert!(validate_action_key("quote\"key").is_err());
}

#[test]
fn test_actions_list_validation() {
    // Valid action pairs
    let raw = vec![
        "default".to_string(),
        "<b>View</b>".to_string(),
        "dismiss".to_string(),
        "Dismiss".to_string(),
    ];
    let Ok(sanitized) = validate_and_sanitize_actions(&raw) else {
        panic!("valid actions");
    };
    assert_eq!(sanitized.len(), 2);
    assert_eq!(sanitized[0], ("default".to_string(), "View".to_string()));
    assert_eq!(sanitized[1], ("dismiss".to_string(), "Dismiss".to_string()));

    // Odd length rejected
    let odd = vec!["only_key".to_string()];
    assert!(validate_and_sanitize_actions(&odd).is_err());

    // Exceeding MAX_ACTIONS (8 pairs = 16 elements): 9 pairs = 18 elements
    let mut too_many = Vec::new();
    for i in 0..18 {
        too_many.push(format!("item_{i}"));
    }
    assert_eq!(
        validate_and_sanitize_actions(&too_many),
        Err(NotificationError::LimitsExceeded(
            "Actions count exceeds maximum of 8"
        ))
    );
}
