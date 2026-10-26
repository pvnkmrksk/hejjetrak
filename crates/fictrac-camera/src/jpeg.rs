use crate::{CameraFormatRequest, Error, Result};
use image::ImageDecoder;
use std::io::Cursor;

/// Decode a single UVC JPEG without Nokhwa's optional native mozjpeg backend.
/// Validate negotiated/header dimensions before allocating a decoded image.
pub(crate) fn decode(bytes: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    CameraFormatRequest {
        width,
        height,
        fps: None,
    }
    .validate()?;
    if bytes.is_empty() || bytes.len() > 32 * 1024 * 1024 {
        return Err(Error(
            "camera MJPEG payload is empty or exceeds 32 MiB".into(),
        ));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width as u32);
    limits.max_image_height = Some(height as u32);
    limits.max_alloc = Some(96 * 1024 * 1024);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| Error(format!("invalid camera MJPEG frame: {e}")))?;
    if decoder.dimensions() != (width as u32, height as u32) {
        return Err(Error("camera JPEG dimensions differ from its negotiated format; reconnect or select another advertised format".into()));
    }
    let image = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| Error(format!("cannot decode camera MJPEG frame: {e}")))?
        .into_rgb8();
    Ok(image.into_raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_rust_mjpeg_preserves_dimensions_and_rejects_mismatched_headers()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let pixels = vec![96u8; 16 * 16 * 3];
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95).encode(
            &pixels,
            16,
            16,
            image::ExtendedColorType::Rgb8,
        )?;
        let decoded = decode(&jpeg, 16, 16)?;
        assert_eq!(decoded.len(), pixels.len());
        assert!(decoded.iter().all(|v| v.abs_diff(96) <= 2));
        assert!(decode(&jpeg, 32, 16).is_err());
        assert!(decode(&jpeg, 0, 16).is_err());
        assert!(decode(b"not a JPEG", 16, 16).is_err());
        assert!(decode(&[], 16, 16).is_err());
        Ok(())
    }
}
