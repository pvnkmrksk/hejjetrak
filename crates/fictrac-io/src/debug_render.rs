//! Headless rendering of FicTrac's six-panel diagnostic screen.
use crate::{Error, Result};
use fictrac_core::DebugViews;
/// Returns RGB with dimensions `(4*cell, 3*cell)`. The data are computed views,
/// never screenshots, so headless agents and video recorders see the same values.
pub fn render_debug_rgb(
    rgb: &[u8],
    width: usize,
    height: usize,
    views: &DebugViews,
    path: &[[f64; 2]],
    cell: usize,
) -> Result<Vec<u8>> {
    render_debug_panels(rgb, width, height, views, path, cell, true)
}

fn render_debug_panels(
    rgb: &[u8],
    width: usize,
    height: usize,
    views: &DebugViews,
    path: &[[f64; 2]],
    cell: usize,
    draw_input: bool,
) -> Result<Vec<u8>> {
    if !(16..=512).contains(&cell)
        || !(16..=8192).contains(&width)
        || !(16..=8192).contains(&height)
        || width.checked_mul(height).is_none_or(|n| n > 32_000_000)
        || rgb.len() != width * height * 3
    {
        return Err(Error::Video(
            "invalid debug image dimensions or RGB storage".into(),
        ));
    }
    let roi = views.roi_size;
    if !(1..=300).contains(&roi)
        || views.roi.len() != roi * roi
        || views.diff.len() != roi * roi
        || views.map_width == 0
        || views.map_height == 0
        || views.map_width > 900
        || views.map_height > 450
        || views.instant.len() != views.map_width * views.map_height
        || views.accumulated.len() != views.instant.len()
    {
        return Err(Error::Video(
            "invalid numerical diagnostic view dimensions".into(),
        ));
    }
    if path.len() > 1_000_000 || path.iter().flatten().any(|v| !v.is_finite()) {
        return Err(Error::Video("invalid diagnostic path coordinates".into()));
    }
    let w = 4 * cell;
    let h = 3 * cell;
    let mut out = vec![0u8; w * h * 3];
    if draw_input {
        let source_x: Vec<_> = (0..2 * cell).map(|x| x * width / (2 * cell)).collect();
        for y in 0..2 * cell {
            let source_row = y * height / (2 * cell) * width;
            for (x, sx) in source_x.iter().enumerate() {
                let src = (source_row + sx) * 3;
                let dst = (y * w + x) * 3;
                out[dst..dst + 3].copy_from_slice(&rgb[src..src + 3]);
            }
        }
    }
    let panels = [
        (&views.roi, roi, roi, 2 * cell, 0, cell, cell),
        (&views.diff, roi, roi, 3 * cell, 0, cell, cell),
        (
            &views.instant,
            views.map_width,
            views.map_height,
            2 * cell,
            cell,
            2 * cell,
            cell,
        ),
        (
            &views.accumulated,
            views.map_width,
            views.map_height,
            2 * cell,
            2 * cell,
            2 * cell,
            cell,
        ),
    ];
    for (data, sw, sh, dx, dy, dw, dh) in panels {
        let source_x: Vec<_> = (0..dw).map(|x| x * sw / dw).collect();
        for y in 0..dh {
            let source_row = y * sh / dh * sw;
            for (x, sx) in source_x.iter().enumerate() {
                let v = data[source_row + sx];
                let dst = ((dy + y) * w + dx + x) * 3;
                out[dst..dst + 3].fill(v);
            }
        }
    }
    if !path.is_empty() {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in path {
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let spanx = (max[0] - min[0]).max(0.01);
        let spany = (max[1] - min[1]).max(0.01);
        let scale = ((cell as f64 - 16.0) / spanx).min((2.0 * cell as f64 - 16.0) / spany);
        let project = |p: [f64; 2]| {
            [
                ((p[1] - (min[1] + max[1]) * 0.5) * scale + cell as f64) as i32,
                ((p[0] - (min[0] + max[0]) * 0.5) * scale + 2.5 * cell as f64) as i32,
            ]
        };
        for pair in path.windows(2) {
            line(&mut out, w, h, project(pair[0]), project(pair[1]), [255; 3]);
        }
    }
    for (a, b) in [
        ([2 * cell, 0], [2 * cell, 3 * cell - 1]),
        ([0, 2 * cell], [4 * cell - 1, 2 * cell]),
        ([3 * cell, 0], [3 * cell, cell]),
        ([2 * cell, cell], [4 * cell - 1, cell]),
    ] {
        line(
            &mut out,
            w,
            h,
            [a[0] as i32, a[1] as i32],
            [b[0] as i32, b[1] as i32],
            [255; 3],
        );
    }
    for (text, x, y) in [
        ("INPUT IMAGE", 2, 2 * cell - 10),
        ("SPHERE ROI", 2 * cell + 2, cell - 10),
        ("WARPED DIFF", 3 * cell + 2, cell - 10),
        ("INSTANT MAP", 2 * cell + 2, 2 * cell - 10),
        ("FLAT PATH", 2, 3 * cell - 10),
        ("ACCUMULATED MAP", 2 * cell + 2, 3 * cell - 10),
    ] {
        label(&mut out, w, h, text, x, y);
    }
    Ok(out)
}
fn line(out: &mut [u8], w: usize, h: usize, a: [i32; 2], b: [i32; 2], color: [u8; 3]) {
    let mut x = a[0];
    let mut y = a[1];
    let dx = (b[0] - x).abs();
    let sx = if x < b[0] { 1 } else { -1 };
    let dy = -(b[1] - y).abs();
    let sy = if y < b[1] { 1 } else { -1 };
    let mut err = dx + dy;
    for _ in 0..=(dx - dy).min((w + h) as i32 * 2) {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let i = (y as usize * w + x as usize) * 3;
            out[i..i + 3].copy_from_slice(&color);
        }
        if x == b[0] && y == b[1] {
            break;
        }
        let e = 2 * err;
        if e >= dy {
            err += dy;
            x += sx;
        }
        if e <= dx {
            err += dx;
            y += sy;
        }
    }
}
fn label(out: &mut [u8], w: usize, h: usize, text: &str, x: usize, y: usize) {
    for (index, c) in text.chars().enumerate() {
        let bits = match c {
            'A' => [14, 17, 17, 31, 17, 17, 17],
            'C' => [14, 17, 16, 16, 16, 17, 14],
            'D' => [30, 17, 17, 17, 17, 17, 30],
            'E' => [31, 16, 16, 30, 16, 16, 31],
            'F' => [31, 16, 16, 30, 16, 16, 16],
            'G' => [14, 17, 16, 23, 17, 17, 14],
            'H' => [17, 17, 17, 31, 17, 17, 17],
            'I' => [14, 4, 4, 4, 4, 4, 14],
            'L' => [16, 16, 16, 16, 16, 16, 31],
            'M' => [17, 27, 21, 21, 17, 17, 17],
            'N' => [17, 25, 25, 21, 19, 19, 17],
            'O' => [14, 17, 17, 17, 17, 17, 14],
            'P' => [30, 17, 17, 30, 16, 16, 16],
            'R' => [30, 17, 17, 30, 20, 18, 17],
            'S' => [15, 16, 16, 14, 1, 1, 30],
            'T' => [31, 4, 4, 4, 4, 4, 4],
            'U' => [17, 17, 17, 17, 17, 17, 14],
            'W' => [17, 17, 17, 21, 21, 27, 17],
            _ => [0; 7],
        };
        for (yy, row) in bits.iter().enumerate() {
            for xx in 0..5 {
                let px = x + index * 6 + xx;
                let py = y + yy;
                if row & (1 << (4 - xx)) != 0 && px < w && py < h {
                    let i = (py * w + px) * 3;
                    out[i..i + 3].copy_from_slice(&[255, 255, 0]);
                }
            }
        }
    }
}

/// Reusable calibrated debug compositor. The camera/configuration and cell size
/// are immutable; dynamic RGB, numerical views, paths and poses stay per frame.
/// Recreate this object after changing the source geometry or calibration.
pub struct CalibratedDebugRenderer {
    config: fictrac_core::Config,
    cell: usize,
    source_remap: Vec<Option<RgbRemapSample>>,
}

/// Bilinear coordinates are invariant for a calibrated source. Keep the exact
/// f64 interpolation order of `sample_rgb`, including its final rounding.
struct RgbRemapSample {
    top: usize,
    bottom: usize,
    fx: f64,
    fy: f64,
}
impl RgbRemapSample {
    fn new(width: usize, height: usize, x: f64, y: f64) -> Option<Self> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= (width - 1) as f64
            || y >= (height - 1) as f64
        {
            return None;
        }
        let ix = x.floor() as usize;
        let iy = y.floor() as usize;
        Some(Self {
            top: (iy * width + ix) * 3,
            bottom: ((iy + 1) * width + ix) * 3,
            fx: x - ix as f64,
            fy: y - iy as f64,
        })
    }

    fn sample(&self, rgb: &[u8]) -> [u8; 3] {
        let mut color = [0; 3];
        for (channel, value) in color.iter_mut().enumerate() {
            let a = f64::from(rgb[self.top + channel]);
            let b = f64::from(rgb[self.top + 3 + channel]);
            let c = f64::from(rgb[self.bottom + channel]);
            let d = f64::from(rgb[self.bottom + 3 + channel]);
            *value = ((a * (1.0 - self.fx) + b * self.fx) * (1.0 - self.fy)
                + (c * (1.0 - self.fx) + d * self.fx) * self.fy)
                .round() as u8;
        }
        color
    }
}
impl CalibratedDebugRenderer {
    pub fn new(config: &fictrac_core::Config, cell: usize) -> Result<Self> {
        use glam::{DQuat, DVec3};
        if !(16..=512).contains(&cell) {
            return Err(Error::Video("debug cell size must be 16..512".into()));
        }
        let geometry = fictrac_core::Geometry::new(config.clone())?;
        let basis = DQuat::from_rotation_arc(DVec3::from_array(config.sphere.centre), DVec3::Z);
        let radians_per_pixel = 3.0 * config.sphere.angular_radius / (2.0 * cell as f64);
        let mut source_remap = Vec::with_capacity(4 * cell * cell);
        for y in 0..2 * cell {
            for x in 0..2 * cell {
                let dx = (x as f64 + 0.5 - cell as f64) * radians_per_pixel;
                let dy = (y as f64 + 0.5 - cell as f64) * radians_per_pixel;
                let angle = dx.hypot(dy);
                let factor = if angle > 1e-12 {
                    angle.sin() / angle
                } else {
                    1.0
                };
                let ray = basis.inverse() * DVec3::new(dx * factor, dy * factor, angle.cos());
                source_remap.push(geometry.project_ray(ray).and_then(|p| {
                    RgbRemapSample::new(config.width, config.height, p[0] - 0.5, p[1] - 0.5)
                }));
            }
        }
        Ok(Self {
            config: config.clone(),
            cell,
            source_remap,
        })
    }
    pub fn render(
        &self,
        rgb: &[u8],
        views: &DebugViews,
        path: &[[f64; 2]],
        poses: &[[f64; 3]],
    ) -> Result<Vec<u8>> {
        render_calibrated_debug_impl(
            rgb,
            &self.config,
            views,
            path,
            poses,
            self.cell,
            Some(&self.source_remap),
        )
    }
}

/// Full calibrated input remap and contact history, using the same ROI coordinate
/// convention as upstream. `poses` are accepted absolute_camera Rodrigues vectors,
/// oldest first, limited to the latest 1000 samples by the caller.
pub fn render_calibrated_debug_rgb(
    rgb: &[u8],
    config: &fictrac_core::Config,
    views: &DebugViews,
    path: &[[f64; 2]],
    poses: &[[f64; 3]],
    cell: usize,
) -> Result<Vec<u8>> {
    render_calibrated_debug_impl(rgb, config, views, path, poses, cell, None)
}

fn render_calibrated_debug_impl(
    rgb: &[u8],
    config: &fictrac_core::Config,
    views: &DebugViews,
    path: &[[f64; 2]],
    poses: &[[f64; 3]],
    cell: usize,
    source_remap: Option<&[Option<RgbRemapSample>]>,
) -> Result<Vec<u8>> {
    use glam::{DQuat, DVec3};
    config.validate()?;
    if poses.len() > 1000
        || poses
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 100.0)
    {
        return Err(Error::Video(
            "diagnostic pose history must contain at most 1000 finite rotations".into(),
        ));
    }
    // The calibrated input replaces this entire quadrant. Avoid computing a
    // nearest-neighbour source image that would be discarded immediately.
    let mut out = render_debug_panels(rgb, config.width, config.height, views, path, cell, false)?;
    let basis = DQuat::from_rotation_arc(DVec3::from_array(config.sphere.centre), DVec3::Z);
    let radians_per_pixel = 3.0 * config.sphere.angular_radius / (2.0 * cell as f64);
    let w = 4 * cell;
    let h = 3 * cell;
    if let Some(remap) = source_remap {
        if remap.len() != 4 * cell * cell {
            return Err(Error::Video("debug remap dimensions changed".into()));
        }
        for (y, row) in remap.chunks(2 * cell).enumerate() {
            for (x, point) in row.iter().enumerate() {
                let color = point.as_ref().map(|p| p.sample(rgb)).unwrap_or([0; 3]);
                let target = (y * w + x) * 3;
                out[target..target + 3].copy_from_slice(&color);
            }
        }
    } else {
        // Keep the direct projection path for one-off renders and as an
        // independent per-pixel oracle for the recording cache.
        let geometry = fictrac_core::Geometry::new(config.clone())?;
        for y in 0..2 * cell {
            for x in 0..2 * cell {
                let dx = (x as f64 + 0.5 - cell as f64) * radians_per_pixel;
                let dy = (y as f64 + 0.5 - cell as f64) * radians_per_pixel;
                let angle = dx.hypot(dy);
                let factor = if angle > 1e-12 {
                    angle.sin() / angle
                } else {
                    1.0
                };
                let ray = basis.inverse() * DVec3::new(dx * factor, dy * factor, angle.cos());
                let color = geometry
                    .project_ray(ray)
                    .and_then(|p| {
                        sample_rgb(rgb, config.width, config.height, p[0] - 0.5, p[1] - 0.5)
                    })
                    .unwrap_or([0; 3]);
                let target = (y * w + x) * 3;
                out[target..target + 3].copy_from_slice(&color);
            }
        }
    }
    let project = |v: DVec3| -> Option<[i32; 2]> {
        if !v.is_finite() || v.z <= 0.0 {
            return None;
        }
        let radius = v.x.hypot(v.y);
        let factor = if radius > 1e-12 {
            radius.atan2(v.z) / (radians_per_pixel * radius)
        } else {
            1.0 / (radians_per_pixel * v.z)
        };
        let x = v.x * factor + cell as f64 - 0.5;
        let y = v.y * factor + cell as f64 - 0.5;
        (x >= 0.0 && y >= 0.0 && x < (2 * cell) as f64 && y < (2 * cell) as f64)
            .then_some([x.round() as i32, y.round() as i32])
    };
    let rim_radius = config.sphere.angular_radius / radians_per_pixel;
    for i in 0..128 {
        let a = i as f64 * std::f64::consts::TAU / 128.;
        let b = (i + 1) as f64 * std::f64::consts::TAU / 128.;
        line(
            &mut out,
            w,
            h,
            [
                (cell as f64 + rim_radius * a.cos()) as i32,
                (cell as f64 + rim_radius * a.sin()) as i32,
            ],
            [
                (cell as f64 + rim_radius * b.cos()) as i32,
                (cell as f64 + rim_radius * b.sin()) as i32,
            ],
            [0, 255, 100],
        );
    }
    // Legacy c2a_r acts on ROI-space exports. JSON's physical convention includes Q.
    let lab = DQuat::from_scaled_axis(DVec3::from_array(config.camera_to_lab));
    let roi_from_lab = if config.legacy_roi_coordinates {
        lab.inverse()
    } else {
        basis * lab.inverse()
    };
    let sphere_radius = config.sphere.angular_radius.sin();
    for (axis, color) in [
        (DVec3::X, [255, 70, 70]),
        (DVec3::Y, [70, 255, 70]),
        (DVec3::Z, [70, 140, 255]),
    ] {
        let v = roi_from_lab * axis;
        let start = [cell as i32, cell as i32];
        if let Some(end) = project(DVec3::Z + v * sphere_radius * 0.7) {
            line(&mut out, w, h, start, end, color);
        }
    }
    if let Some(last) = poses.last() {
        let as_roi = |r: &[f64; 3]| {
            let rotation = DQuat::from_scaled_axis(DVec3::from_array(*r));
            if config.legacy_roi_coordinates {
                rotation
            } else {
                basis * rotation * basis.inverse()
            }
        };
        let current = as_roi(last);
        let up = roi_from_lab * (-DVec3::Z) * sphere_radius;
        let mut previous = None;
        for pose in poses.iter().rev() {
            let v = current * as_roi(pose).inverse() * up;
            let point = if v.z < 0.0 {
                project(DVec3::Z + v)
            } else {
                None
            };
            if let (Some(a), Some(b)) = (previous, point) {
                line(&mut out, w, h, a, b, [0, 255, 255]);
            }
            previous = point;
        }
    }
    label(&mut out, w, h, "INPUT IMAGE", 2, 2 * cell - 10);
    Ok(out)
}
fn sample_rgb(rgb: &[u8], width: usize, height: usize, x: f64, y: f64) -> Option<[u8; 3]> {
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x >= (width - 1) as f64
        || y >= (height - 1) as f64
    {
        return None;
    }
    let ix = x.floor() as usize;
    let iy = y.floor() as usize;
    let fx = x - ix as f64;
    let fy = y - iy as f64;
    let mut color = [0; 3];
    for (channel, value) in color.iter_mut().enumerate() {
        let a = f64::from(rgb[(iy * width + ix) * 3 + channel]);
        let b = f64::from(rgb[(iy * width + ix + 1) * 3 + channel]);
        let c = f64::from(rgb[((iy + 1) * width + ix) * 3 + channel]);
        let d = f64::from(rgb[((iy + 1) * width + ix + 1) * 3 + channel]);
        *value =
            ((a * (1.0 - fx) + b * fx) * (1.0 - fy) + (c * (1.0 - fx) + d * fx) * fy).round() as u8;
    }
    Some(color)
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use fictrac_core::Tracker;
    use std::path::PathBuf;

    #[test]
    fn cached_remap_matches_direct_projection_for_real_frames_and_dynamic_history() -> Result<()> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut source = crate::VideoSource::open(&root.join("examples/sample.mp4"))?;
        source.enable_color();
        let loaded = crate::load_config_text(
            include_str!("../../../examples/benchmark/config.txt"),
            source.info.width,
            source.info.height,
        )?;
        let mut frames = Vec::new();
        for _ in 0..3 {
            let frame = source
                .next_tracking_frame(None)?
                .ok_or_else(|| Error::Video("real renderer fixture ended early".into()))?;
            let rgb = source
                .take_rgb()
                .ok_or_else(|| Error::Video("real renderer fixture has no RGB".into()))?;
            frames.push((frame, rgb));
        }
        let mut off_axis = loaded.config.clone();
        off_axis.sphere.centre = glam::DVec3::new(0.12, -0.08, 1.0).normalize().to_array();
        let mut fisheye = off_axis.clone();
        fisheye.fisheye = true;
        fisheye.legacy_roi_coordinates = false;
        for (variant, config) in [loaded.config, off_axis, fisheye].iter().enumerate() {
            for cell in [64, 160] {
                let cached = CalibratedDebugRenderer::new(config, cell)?;
                let mut tracker = Tracker::new(config.clone())?;
                let mut path = Vec::new();
                let mut poses = Vec::new();
                for (index, (frame, rgb)) in frames.iter().enumerate() {
                    let row = tracker.process(frame.clone())?;
                    path.push(row.position);
                    if row.accepted {
                        poses.push(row.absolute_camera);
                    }
                    let views = tracker.debug_views().ok_or_else(|| {
                        Error::Video("real renderer fixture has no numerical views".into())
                    })?;
                    let direct =
                        render_calibrated_debug_rgb(rgb, config, views, &path, &poses, cell)?;
                    let actual = cached.render(rgb, views, &path, &poses)?;
                    let mismatch = actual.iter().zip(&direct).position(|(a, b)| a != b);
                    assert_eq!(actual.len(), direct.len());
                    assert_eq!(
                        mismatch, None,
                        "variant {variant}, cell {cell}, real frame {index}"
                    );
                    assert!(
                        cached
                            .render(&rgb[..rgb.len() - 1], views, &path, &poses)
                            .is_err()
                    );
                    assert!(
                        cached
                            .render(rgb, views, &path, &[[f64::NAN, 0.0, 0.0]])
                            .is_err()
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn cached_renderer_rejects_invalid_geometry_before_allocating_remap() {
        let config = fictrac_core::Config::default();
        for cell in [0, 15, 513, usize::MAX] {
            assert!(CalibratedDebugRenderer::new(&config, cell).is_err());
        }
        let invalid = fictrac_core::Config { width: 0, ..config };
        assert!(CalibratedDebugRenderer::new(&invalid, 160).is_err());
    }
}
