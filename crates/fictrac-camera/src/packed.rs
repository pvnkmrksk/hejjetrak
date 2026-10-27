use crate::{CameraFormatRequest, Error, Result};

/// The native adapter exposes packed pixels without a separate row stride.
/// Reject malformed payloads before a decoder can allocate from their length.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Layout {
    Gray8,
    Rgb24,
    Yuyv422,
    Nv12,
}

pub(crate) fn validate(layout: Layout, width: usize, height: usize, length: usize) -> Result<()> {
    CameraFormatRequest {
        width,
        height,
        fps: None,
    }
    .validate()?;
    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| Error("camera pixel count overflows".into()))?;
    let expected = match layout {
        Layout::Gray8 => Some(pixels),
        Layout::Rgb24 => pixels.checked_mul(3),
        Layout::Yuyv422 if width.is_multiple_of(2) => pixels.checked_mul(2),
        Layout::Nv12 if width.is_multiple_of(2) && height.is_multiple_of(2) => {
            pixels.checked_mul(3).map(|n| n / 2)
        }
        _ => None,
    }
    .ok_or_else(|| {
        Error(format!(
            "invalid camera {layout:?} geometry; select an advertised packed format"
        ))
    })?;
    if length != expected {
        return Err(Error(format!(
            "camera {layout:?} payload has {length} bytes, expected {expected}; reconnect or select another advertised packed format"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_payloads_reject_short_and_oversized_buffers_before_decode() -> Result<()> {
        for (layout, length) in [
            (Layout::Gray8, 256),
            (Layout::Rgb24, 768),
            (Layout::Yuyv422, 512),
            (Layout::Nv12, 384),
        ] {
            validate(layout, 16, 16, length)?;
            for malformed in [0, length - 1, length + 1, usize::MAX] {
                assert!(validate(layout, 16, 16, malformed).is_err());
            }
            assert!(validate(layout, usize::MAX, 16, length).is_err());
            assert!(validate(layout, 16, 0, length).is_err());
        }
        Ok(())
    }

    #[test]
    fn chroma_subsampled_pixels_require_complete_rows_and_planes() -> Result<()> {
        assert!(validate(Layout::Yuyv422, 17, 16, 17 * 16 * 2).is_err());
        assert!(validate(Layout::Nv12, 17, 16, 17 * 16 * 3 / 2).is_err());
        assert!(validate(Layout::Nv12, 16, 17, 16 * 17 * 3 / 2).is_err());
        validate(Layout::Yuyv422, 16, 17, 16 * 17 * 2)?;
        validate(Layout::Gray8, 17, 17, 17 * 17)?;
        validate(Layout::Rgb24, 17, 17, 17 * 17 * 3)?;
        Ok(())
    }
}
