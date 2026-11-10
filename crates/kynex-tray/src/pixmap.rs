//! IconPixmap validation, checked arithmetic, size selection,
//! and network byte order ARGB to RGBA pixel conversion.

use crate::error::TrayError;
use crate::limits::{MAX_PIXMAP_BYTES, MAX_PIXMAP_DIMENSION};

/// Raw pixmap tuple as transmitted over D-Bus: `(width, height, data)`.
pub type RawPixmapTuple = (i32, i32, Vec<u8>);

/// A validated, converted system tray icon pixmap in RGBA format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPixmap {
    /// Pixmap width in pixels (1..=64).
    pub width: i32,
    /// Pixmap height in pixels (1..=64).
    pub height: i32,
    /// Pixel data converted to RGBA8888 byte format (`width * height * 4` bytes).
    pub rgba_data: Vec<u8>,
}

/// Validates an individual candidate pixmap tuple.
///
/// # Invariants Enforced
/// - `width > 0 && width <= MAX_PIXMAP_DIMENSION (64)`
/// - `height > 0 && height <= MAX_PIXMAP_DIMENSION (64)`
/// - Checked multiplication: `width * height * 4` must not overflow.
/// - Exact length: `data.len() == width * height * 4`.
/// - Size cap: `data.len() <= MAX_PIXMAP_BYTES (64 KiB)`.
///
/// Returns [`ValidatedPixmap`] with bytes converted from ARGB to RGBA,
/// or [`TrayError::InvalidPixmap`] on validation failure.
///
/// # Errors
///
/// Returns an error if dimensions are non-positive, exceed 64x64, arithmetic overflows,
/// total payload exceeds 64 KiB, or buffer length does not match expected byte count.
pub fn validate_and_convert_pixmap(
    width: i32,
    height: i32,
    argb_data: &[u8],
) -> Result<ValidatedPixmap, TrayError> {
    if width <= 0 || width > MAX_PIXMAP_DIMENSION {
        return Err(TrayError::InvalidPixmap(format!(
            "Width {width} out of valid bounds (1..={MAX_PIXMAP_DIMENSION})"
        )));
    }
    if height <= 0 || height > MAX_PIXMAP_DIMENSION {
        return Err(TrayError::InvalidPixmap(format!(
            "Height {height} out of valid bounds (1..={MAX_PIXMAP_DIMENSION})"
        )));
    }

    let w_usize = usize::try_from(width)
        .map_err(|_| TrayError::InvalidPixmap("Negative width converted to usize".to_string()))?;
    let h_usize = usize::try_from(height)
        .map_err(|_| TrayError::InvalidPixmap("Negative height converted to usize".to_string()))?;

    let pixel_count = w_usize
        .checked_mul(h_usize)
        .ok_or_else(|| TrayError::InvalidPixmap("Width * height overflow".to_string()))?;

    let expected_bytes = pixel_count
        .checked_mul(4)
        .ok_or_else(|| TrayError::InvalidPixmap("Pixel count * 4 overflow".to_string()))?;

    if expected_bytes > MAX_PIXMAP_BYTES {
        return Err(TrayError::InvalidPixmap(format!(
            "Payload size {expected_bytes} exceeds cap of {MAX_PIXMAP_BYTES} bytes"
        )));
    }

    if argb_data.len() != expected_bytes {
        return Err(TrayError::InvalidPixmap(format!(
            "Data length mismatch: expected {expected_bytes} bytes, got {}",
            argb_data.len()
        )));
    }

    // Convert network byte order ARGB32 (byte 0: A, byte 1: R, byte 2: G, byte 3: B)
    // to standard RGBA (byte 0: R, byte 1: G, byte 2: B, byte 3: A).
    let mut rgba_data = Vec::with_capacity(expected_bytes);
    for chunk in argb_data.chunks_exact(4) {
        let a = chunk[0];
        let r = chunk[1];
        let g = chunk[2];
        let b = chunk[3];
        rgba_data.push(r);
        rgba_data.push(g);
        rgba_data.push(b);
        rgba_data.push(a);
    }

    Ok(ValidatedPixmap {
        width,
        height,
        rgba_data,
    })
}

/// Evaluates a list of candidate pixmap tuples `a(iiay)` from a StatusNotifierItem
/// and selects the best candidate whose dimensions are `<= 64x64`.
///
/// "Best" candidate is defined as the largest valid candidate (by pixel area `width * height`).
/// Any candidate violating security invariants (overflow, truncated, negative, or > 64 KiB)
/// is rejected.
///
/// Returns `Ok(Some(best_pixmap))` if at least one valid candidate `<= 64x64` exists,
/// `Ok(None)` if the input list is empty or contains no candidates `<= 64x64`,
/// or `Err(TrayError)` if any candidate contains adversarial corruptions (e.g. data length mismatch).
///
/// # Errors
///
/// Returns an error if any entry <= 64x64 has corrupted data lengths or arithmetic overflows.
pub fn select_best_pixmap(
    pixmaps: &[RawPixmapTuple],
) -> Result<Option<ValidatedPixmap>, TrayError> {
    if pixmaps.is_empty() {
        return Ok(None);
    }

    let mut best: Option<ValidatedPixmap> = None;
    let mut best_area = 0;

    for (w, h, data) in pixmaps {
        // Skip entries exceeding max dimension (e.g. 128x128 or 256x256 provided by some apps)
        if *w > MAX_PIXMAP_DIMENSION || *h > MAX_PIXMAP_DIMENSION {
            continue;
        }

        // Validate candidate
        let validated = validate_and_convert_pixmap(*w, *h, data)?;
        let area = validated.width * validated.height;
        if area > best_area {
            best_area = area;
            best = Some(validated);
        }
    }

    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_pixmap_conversion() {
        // 2x2 ARGB image
        // Pixel 0: A=255, R=10, G=20, B=30
        // Pixel 1: A=128, R=40, G=50, B=60
        // Pixel 2: A=0,   R=70, G=80, B=90
        // Pixel 3: A=64,  R=100, G=110, B=120
        let argb = vec![
            255, 10, 20, 30, //
            128, 40, 50, 60, //
            0, 70, 80, 90, //
            64, 100, 110, 120, //
        ];

        let res = validate_and_convert_pixmap(2, 2, &argb);
        assert!(res.is_ok());
        let pixmap = res.unwrap_or_else(|_| unreachable!());
        assert_eq!(pixmap.width, 2);
        assert_eq!(pixmap.height, 2);
        assert_eq!(
            pixmap.rgba_data,
            vec![
                10, 20, 30, 255, //
                40, 50, 60, 128, //
                70, 80, 90, 0, //
                100, 110, 120, 64, //
            ]
        );
    }

    #[test]
    fn test_lying_data_length() {
        let truncated = vec![255, 10, 20];
        let res = validate_and_convert_pixmap(2, 2, &truncated);
        assert!(matches!(res, Err(TrayError::InvalidPixmap(_))));

        let excess = vec![0u8; 20];
        let res2 = validate_and_convert_pixmap(2, 2, &excess);
        assert!(matches!(res2, Err(TrayError::InvalidPixmap(_))));
    }

    #[test]
    fn test_dimension_bounds() {
        let res_zero = validate_and_convert_pixmap(0, 0, &[]);
        assert!(matches!(res_zero, Err(TrayError::InvalidPixmap(_))));

        let res_neg = validate_and_convert_pixmap(-1, 10, &[]);
        assert!(matches!(res_neg, Err(TrayError::InvalidPixmap(_))));

        let res_large = validate_and_convert_pixmap(65, 64, &[]);
        assert!(matches!(res_large, Err(TrayError::InvalidPixmap(_))));
    }

    #[test]
    fn test_select_best_pixmap() {
        let p16 = (16, 16, vec![0u8; 16 * 16 * 4]);
        let p32 = (32, 32, vec![1u8; 32 * 32 * 4]);
        let p128 = (128, 128, vec![2u8; 128 * 128 * 4]);

        let list = vec![p16, p32, p128];
        let best = select_best_pixmap(&list).unwrap_or(None);
        assert!(best.is_some());
        let chosen = best.unwrap_or_else(|| unreachable!());
        assert_eq!(chosen.width, 32);
        assert_eq!(chosen.height, 32);
    }
}
