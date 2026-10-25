//! Explicit resolution-aware editing; legacy angular geometry stays untouched.
use crate::{ConfigDocument, Error, Result};

const SIZE: &str = "rust_calibration_size";
const POINTS: [&str; 4] = ["roi_circ", "c2a_cnrs_xy", "c2a_cnrs_yz", "c2a_cnrs_xz"];

fn dimensions(width: usize, height: usize) -> Result<()> {
    if width < 2 || height < 2 || width.checked_mul(height).is_none_or(|n| n > 32_000_000) {
        return Err(Error::Config(
            "calibration dimensions must be at least 2×2 and at most 32 million pixels".into(),
        ));
    }
    Ok(())
}
fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?
        .replace('[', "{")
        .replace(']', "}"))
}
fn normalized(values: &[f64], width: usize, height: usize) -> Result<Vec<f64>> {
    if values.len() > 8192 || !values.len().is_multiple_of(2) {
        return Err(Error::Config(
            "calibration marks must contain at most 4096 x/y pairs".into(),
        ));
    }
    values.iter().enumerate().map(|(i, value)| {
        let extent = if i % 2 == 0 { width } else { height } as f64;
        if !value.is_finite() || *value < 0.0 || *value >= extent {
            return Err(Error::Config("calibration point is outside the reference frame; select its original dimensions or correct the point first".into()));
        }
        Ok(value / extent)
    }).collect()
}

/// Opt in, retaining pixel fields and adding normalized Rust extension metadata.
/// The returned document is complete before callers publish a history revision.
pub fn capture_calibration_reference(
    document: &ConfigDocument,
    width: usize,
    height: usize,
) -> Result<ConfigDocument> {
    dimensions(width, height)?;
    let mut next = document.clone();
    next.set(SIZE, &encode(&[width, height])?)?;
    next.set(
        "rust_calibration_vfov",
        &document.number("vfov", 45.0).to_string(),
    )?;
    next.set(
        "rust_calibration_fisheye",
        if document.boolean("fisheye") {
            "y"
        } else {
            "n"
        },
    )?;
    for key in POINTS {
        let values: Vec<f64> = if document.get(key).is_some() {
            document.vector(key)?
        } else {
            vec![]
        };
        next.set(
            &format!("rust_normalized_{key}"),
            &encode(&normalized(&values, width, height)?)?,
        )?;
    }
    let polygons: Vec<Vec<f64>> = if document.get("roi_ignr").is_some() {
        document.vector("roi_ignr")?
    } else {
        vec![]
    };
    if polygons.len() > 64 {
        return Err(Error::Config("at most 64 exclusion polygons".into()));
    }
    let polygons = polygons
        .iter()
        .map(|p| normalized(p, width, height))
        .collect::<Result<Vec<_>>>()?;
    next.set("rust_normalized_roi_ignr", &encode(&polygons)?)?;
    Ok(next)
}

/// Scale only saved pixel marks for an explicitly requested, same-FOV resize.
/// Aspect changes and stale metadata are rejected atomically; angular roi_c,
/// roi_r, c2a_r and c2a_t remain exactly as written in the legacy document.
pub fn rescale_calibration_reference(
    document: &ConfigDocument,
    width: usize,
    height: usize,
) -> Result<ConfigDocument> {
    dimensions(width, height)?;
    let [old_width, old_height]: [usize; 2] = document.vector(SIZE).map_err(|_| Error::Config("capture a calibration resolution reference before resizing; legacy files are never normalized automatically".into()))?;
    dimensions(old_width, old_height)?;
    if old_width as u128 * height as u128 != width as u128 * old_height as u128 {
        return Err(Error::Config("aspect ratio changed: automatic calibration scaling requires the same aspect ratio and optical field of view; recalibrate this source".into()));
    }
    if document.number("rust_calibration_vfov", f64::NAN) != document.number("vfov", 45.0)
        || document.boolean("rust_calibration_fisheye") != document.boolean("fisheye")
    {
        return Err(Error::Config("camera field of view/projection changed since calibration; recalibrate and capture a new resolution reference".into()));
    }
    let checked = capture_calibration_reference(document, old_width, old_height)?;
    for key in POINTS.into_iter().chain(["roi_ignr"]) {
        let name = format!("rust_normalized_{key}");
        if document.get(&name) != checked.get(&name) {
            return Err(Error::Config("calibration marks changed since their resolution reference was captured; capture the reference again before resizing".into()));
        }
    }
    let scale = |values: &[f64]| {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| v * if i % 2 == 0 { width } else { height } as f64)
            .collect::<Vec<_>>()
    };
    let mut next = document.clone();
    for key in POINTS {
        if document.get(key).is_some() {
            let values: Vec<f64> = document.vector(&format!("rust_normalized_{key}"))?;
            next.set(key, &encode(&scale(&values))?)?;
        }
    }
    if document.get("roi_ignr").is_some() {
        let polygons: Vec<Vec<f64>> = document.vector("rust_normalized_roi_ignr")?;
        next.set(
            "roi_ignr",
            &encode(&polygons.iter().map(|p| scale(p)).collect::<Vec<_>>())?,
        )?;
    }
    capture_calibration_reference(&next, width, height)
}
