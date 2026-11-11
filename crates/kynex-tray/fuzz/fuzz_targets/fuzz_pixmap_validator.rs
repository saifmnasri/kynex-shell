#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }
    let width = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let height = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let argb_bytes = &data[8..];

    if let Ok(valid) = kynex_tray::validate_and_convert_pixmap(width, height, argb_bytes) {
        assert!(valid.width >= 1 && valid.width <= 64);
        assert!(valid.height >= 1 && valid.height <= 64);
        assert_eq!(valid.rgba_data.len(), (valid.width as usize) * (valid.height as usize) * 4);
        assert!(valid.rgba_data.len() <= 64 * 1024);
    }
});

