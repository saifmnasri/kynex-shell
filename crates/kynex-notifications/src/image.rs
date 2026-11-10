//! Validation and representation of raw embedded notification `image-data`.

use crate::error::NotificationError;
use crate::limits::{MAX_IMAGE_BYTES, MAX_IMAGE_HEIGHT, MAX_IMAGE_WIDTH};

/// Validated, safely bounded raw image data extracted from the `image-data` hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImageData {
    /// Width of the image in pixels (1..=256).
    pub width: i32,
    /// Height of the image in pixels (1..=256).
    pub height: i32,
    /// Distance in bytes between the starts of consecutive rows.
    pub rowstride: i32,
    /// Whether the image includes an alpha channel (`channels == 4`).
    pub has_alpha: bool,
    /// Bits per color sample (strictly 8).
    pub bits_per_sample: i32,
    /// Number of color channels (3 for RGB, 4 for RGBA).
    pub channels: i32,
    /// Raw pixel bytes in RGB or RGBA order.
    pub data: Vec<u8>,
}

/// Strictly validates raw notification image data according to Freedesktop spec
/// and security bounds.
///
/// # Errors
///
/// Returns [`NotificationError::InvalidImageData`] if dimensions, channels, rowstride,
/// or data byte lengths do not match spec requirements, or [`NotificationError::LimitsExceeded`]
/// if total payload exceeds 256 KiB.
#[allow(clippy::too_many_arguments)]
pub fn validate_image_data(
    width: i32,
    height: i32,
    rowstride: i32,
    has_alpha: bool,
    bits_per_sample: i32,
    channels: i32,
    data: &[u8],
) -> Result<RawImageData, NotificationError> {
    if width <= 0 || width > MAX_IMAGE_WIDTH {
        return Err(NotificationError::InvalidImageData(
            "Width must be between 1 and 256",
        ));
    }
    if height <= 0 || height > MAX_IMAGE_HEIGHT {
        return Err(NotificationError::InvalidImageData(
            "Height must be between 1 and 256",
        ));
    }
    if bits_per_sample != 8 {
        return Err(NotificationError::InvalidImageData(
            "Bits per sample must be strictly 8",
        ));
    }
    if channels != 3 && channels != 4 {
        return Err(NotificationError::InvalidImageData(
            "Channels must be 3 (RGB) or 4 (RGBA)",
        ));
    }
    if has_alpha != (channels == 4) {
        return Err(NotificationError::InvalidImageData(
            "Alpha flag is inconsistent with channel count",
        ));
    }

    let width_u =
        u32::try_from(width).map_err(|_| NotificationError::InvalidImageData("Negative width"))?;
    let height_u = u32::try_from(height)
        .map_err(|_| NotificationError::InvalidImageData("Negative height"))?;
    let rowstride_u = u32::try_from(rowstride)
        .map_err(|_| NotificationError::InvalidImageData("Negative rowstride"))?;
    let channels_u = u32::try_from(channels)
        .map_err(|_| NotificationError::InvalidImageData("Negative channels"))?;

    let min_rowstride = width_u
        .checked_mul(channels_u)
        .ok_or(NotificationError::InvalidImageData("Rowstride overflow"))?;

    if rowstride_u < min_rowstride {
        return Err(NotificationError::InvalidImageData(
            "Rowstride cannot be less than width * channels",
        ));
    }

    let full_len = rowstride_u
        .checked_mul(height_u)
        .ok_or(NotificationError::InvalidImageData("Image size overflow"))?;

    let min_len = height_u
        .checked_sub(1)
        .and_then(|h_minus_1| h_minus_1.checked_mul(rowstride_u))
        .and_then(|prev| prev.checked_add(min_rowstride))
        .ok_or(NotificationError::InvalidImageData("Image size overflow"))?;

    if data.len() > MAX_IMAGE_BYTES {
        return Err(NotificationError::LimitsExceeded(
            "Image byte payload exceeds 256 KiB limit",
        ));
    }

    let data_len_u32 = u32::try_from(data.len())
        .map_err(|_| NotificationError::LimitsExceeded("Data length overflow"))?;

    if data_len_u32 != full_len && data_len_u32 != min_len {
        return Err(NotificationError::InvalidImageData(
            "Data length does not match expected rowstride and dimensions",
        ));
    }

    Ok(RawImageData {
        width,
        height,
        rowstride,
        has_alpha,
        bits_per_sample,
        channels,
        data: data.to_vec(),
    })
}
