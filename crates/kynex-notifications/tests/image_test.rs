//! Embedded raw `image-data` validation unit tests.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use kynex_notifications::{validate_image_data, NotificationError, MAX_IMAGE_BYTES};

#[test]
fn test_valid_rgb_image() {
    let width = 16;
    let height = 16;
    let channels = 3;
    let rowstride = width * channels; // 48
    let has_alpha = false;
    let bits_per_sample = 8;
    let data = vec![255u8; (width * height * channels) as usize];

    let result = validate_image_data(
        width,
        height,
        rowstride,
        has_alpha,
        bits_per_sample,
        channels,
        &data,
    );

    assert!(result.is_ok());
    let validated = result.expect("valid image");
    assert_eq!(validated.width, 16);
    assert_eq!(validated.height, 16);
    assert_eq!(validated.channels, 3);
    assert!(!validated.has_alpha);
}

#[test]
fn test_valid_rgba_image_with_padded_rowstride() {
    let width = 10;
    let height = 10;
    let channels = 4;
    let min_stride = width * channels; // 40
    let rowstride = 64; // padded to 64-byte alignment
    assert!(rowstride >= min_stride);
    let has_alpha = true;
    let bits_per_sample = 8;
    let data = vec![128u8; (rowstride * height) as usize];

    let result = validate_image_data(
        width,
        height,
        rowstride,
        has_alpha,
        bits_per_sample,
        channels,
        &data,
    );

    assert!(result.is_ok());
    let validated = result.expect("valid rgba image");
    assert_eq!(validated.width, 10);
    assert_eq!(validated.height, 10);
    assert_eq!(validated.channels, 4);
    assert!(validated.has_alpha);
}

#[test]
fn test_reject_dimensions_exceeding_max_256() {
    // Width 256 within bound passes
    let valid_width_buffer = vec![0u8; 256 * 10 * 3];
    assert!(
        validate_image_data(256, 10, 256 * 3, false, 8, 3, &valid_width_buffer).is_ok(),
        "Width 256 with exact-sized buffer must pass"
    );

    // Width 257 exceeds max width (256): must fail even with exact buffer
    let oversized_width_buffer = vec![0u8; 257 * 10 * 3];
    assert_eq!(
        validate_image_data(257, 10, 257 * 3, false, 8, 3, &oversized_width_buffer),
        Err(NotificationError::InvalidImageData(
            "Width must be between 1 and 256"
        )),
        "Width 257 must be rejected independently of buffer length"
    );

    // Height 257 exceeds max height (256): must fail even with exact buffer
    let oversized_height_buffer = vec![0u8; 10 * 257 * 3];
    assert_eq!(
        validate_image_data(10, 257, 10 * 3, false, 8, 3, &oversized_height_buffer),
        Err(NotificationError::InvalidImageData(
            "Height must be between 1 and 256"
        )),
        "Height 257 must be rejected independently of buffer length"
    );
}

#[test]
fn test_reject_zero_or_negative_dimensions() {
    let dummy_data = vec![0u8; 100];
    assert!(validate_image_data(0, 10, 0, false, 8, 3, &dummy_data).is_err());
    assert!(validate_image_data(10, 0, 30, false, 8, 3, &dummy_data).is_err());
    assert!(validate_image_data(-1, 10, 30, false, 8, 3, &dummy_data).is_err());
    assert!(validate_image_data(10, -1, 30, false, 8, 3, &dummy_data).is_err());
    assert!(validate_image_data(10, 10, -30, false, 8, 3, &dummy_data).is_err());
}

#[test]
fn test_reject_invalid_bits_per_sample() {
    let data = vec![0u8; 48];
    // 16 bits per sample must be rejected
    assert!(validate_image_data(4, 4, 12, false, 16, 3, &data).is_err());
    // 1 bit per sample rejected
    assert!(validate_image_data(4, 4, 12, false, 1, 3, &data).is_err());
}

#[test]
fn test_reject_invalid_channel_counts() {
    let data = vec![0u8; 100];
    // 1 channel (grayscale) or 5 channels rejected
    assert!(validate_image_data(4, 4, 4, false, 8, 1, &data).is_err());
    assert!(validate_image_data(4, 4, 20, true, 8, 5, &data).is_err());
}

#[test]
fn test_reject_inconsistent_alpha_flag() {
    let data = vec![0u8; 48];
    // 3 channels with has_alpha = true rejected
    assert!(validate_image_data(4, 4, 12, true, 8, 3, &data).is_err());
    // 4 channels with has_alpha = false rejected
    assert!(validate_image_data(4, 4, 16, false, 8, 4, &data).is_err());
}

#[test]
fn test_reject_rowstride_too_small() {
    let data = vec![0u8; 100];
    // width 10, channels 3 => min rowstride is 30. rowstride 20 must be rejected
    assert!(validate_image_data(10, 10, 20, false, 8, 3, &data).is_err());
}

#[test]
fn test_reject_mismatched_data_length() {
    let width = 10;
    let height = 10;
    let rowstride = 30;
    // Expected exact length is 300
    let short_data = vec![0u8; 299];
    let long_data = vec![0u8; 301];

    assert!(validate_image_data(width, height, rowstride, false, 8, 3, &short_data).is_err());
    assert!(validate_image_data(width, height, rowstride, false, 8, 3, &long_data).is_err());
}

#[test]
fn test_reject_total_size_exceeding_256_kib() {
    // 256x256 RGBA = 262,144 bytes = exactly 256 KiB (accepted)
    let max_data = vec![0u8; MAX_IMAGE_BYTES];
    let ok = validate_image_data(256, 256, 1024, true, 8, 4, &max_data);
    assert!(ok.is_ok());

    // 256 KiB + 1 byte (exceeds cap)
    let too_large_data = vec![0u8; MAX_IMAGE_BYTES + 1];
    let err = validate_image_data(256, 256, 1024, true, 8, 4, &too_large_data);
    assert!(matches!(err, Err(NotificationError::LimitsExceeded(_))));
}
