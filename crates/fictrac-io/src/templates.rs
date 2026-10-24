//! Legacy equal-area sphere-map image interchange. Pixel values retain confidence.
use crate::{Error, Result};
use fictrac_core::SphereTemplate;
use image::{ImageFormat, ImageReader, Limits};
use std::{
    fs::OpenOptions,
    io::{BufReader, BufWriter},
    path::Path,
};
pub fn load_sphere_map(path: &Path) -> Result<SphereTemplate> {
    let file = std::fs::File::open(path)?;
    let mut reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|e| {
            Error::Video(format!(
                "cannot identify sphere template {}: {e}",
                path.display()
            ))
        })?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(900);
    limits.max_image_height = Some(450);
    limits.max_alloc = Some(16_000_000);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| {
            Error::Video(format!(
                "cannot decode sphere template {}: {e}",
                path.display()
            ))
        })?
        .to_luma8();
    Ok(SphereTemplate::new(
        image.width() as usize,
        image.height() as usize,
        image.into_raw(),
    )?)
}
/// Creates a new image, refusing to overwrite an existing experiment artifact.
/// PNG is recommended: JPEG changes confidence values through lossy compression.
pub fn save_sphere_map(path: &Path, template: &SphereTemplate) -> Result<()> {
    let valid = SphereTemplate::new(template.width, template.height, template.pixels.clone())?;
    let format = ImageFormat::from_path(path)
        .map_err(|e| Error::Video(format!("sphere-map extension: {e}")))?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Bmp | ImageFormat::Pnm
    ) {
        return Err(Error::Video(
            "save sphere maps as lossless .png, .bmp, or .pgm; JPEG would alter confidence values"
                .into(),
        ));
    }
    let image = image::GrayImage::from_raw(valid.width as u32, valid.height as u32, valid.pixels)
        .ok_or_else(|| Error::Video("invalid sphere template storage".into()))?;
    let mut out = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
    image
        .write_to(&mut out, format)
        .map_err(|e| Error::Video(format!("cannot encode sphere template: {e}")))?;
    std::io::Write::flush(&mut out)?;
    Ok(())
}
