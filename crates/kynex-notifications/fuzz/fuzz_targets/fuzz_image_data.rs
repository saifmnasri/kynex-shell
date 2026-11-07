#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 21 {
        return;
    }
    let width = i32::from_le_bytes(data[0..4].try_into().unwrap());
    let height = i32::from_le_bytes(data[4..8].try_into().unwrap());
    let rowstride = i32::from_le_bytes(data[8..12].try_into().unwrap());
    let has_alpha = data[12] != 0;
    let bits_per_sample = i32::from_le_bytes(data[13..17].try_into().unwrap());
    let channels = i32::from_le_bytes(data[17..21].try_into().unwrap());
    let image_bytes = &data[21..];

    if let Ok(valid) = kynex_notifications::validate_image_data(
        width,
        height,
        rowstride,
        has_alpha,
        bits_per_sample,
        channels,
        image_bytes,
    ) {
        assert!(valid.width >= 1 && valid.width <= 256);
        assert!(valid.height >= 1 && valid.height <= 256);
        assert_eq!(valid.bits_per_sample, 8);
        assert!(valid.channels == 3 || valid.channels == 4);
        assert!(valid.data.len() <= 256 * 1024);
    }
});

