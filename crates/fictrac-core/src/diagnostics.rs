//! FicTrac-shaped diagnostic views, produced from actual frames and accepted orientation.
use crate::{Config, Geometry, GrayFrame, Result, TrackRow};
use glam::{DQuat, DVec3};
use std::f64::consts::{PI, TAU};
#[derive(Clone, Debug)]
pub struct DebugViews {
    pub roi: Vec<u8>,
    pub diff: Vec<u8>,
    pub instant: Vec<u8>,
    pub accumulated: Vec<u8>,
    pub roi_size: usize,
    pub map_width: usize,
    pub map_height: usize,
}
pub struct Diagnostics {
    config: Config,
    pub(crate) basis: DQuat,
    pub(crate) points: Vec<Option<(DVec3, [f64; 2])>>,
    pub(crate) map: Vec<u8>,
    previous: Option<GrayFrame>,
    pub(crate) size: usize,
    pub(crate) map_width: usize,
    pub(crate) map_height: usize,
    ratio: f64,
    window: usize,
}
impl Diagnostics {
    pub fn new(config: Config, quality: usize, ratio: f64, window_fraction: f64) -> Result<Self> {
        let g = Geometry::new(config.clone())?;
        let basis = DQuat::from_rotation_arc(DVec3::from_array(config.sphere.centre), DVec3::Z);
        let size = (quality.clamp(1, 30) * 10).min(config.width);
        // Upstream clamps ROI width to source width, then derives the even-width atlas.
        let map_height = size * 3 / 2;
        let map_width = map_height * 2;
        let radius = config.sphere.angular_radius;
        let mut points = vec![];
        for y in 0..size {
            for x in 0..size {
                let dx = (x as f64 + 0.5 - size as f64 * 0.5) * 2.0 * radius / size as f64;
                let dy = (y as f64 + 0.5 - size as f64 * 0.5) * 2.0 * radius / size as f64;
                let theta = dx.hypot(dy);
                let factor = if theta > 1e-10 {
                    theta.sin() / theta
                } else {
                    1.0
                };
                let ray = basis.inverse() * DVec3::new(dx * factor, dy * factor, theta.cos());
                let xy = g
                    .project_ray(ray)
                    .map(|p| [p[0] - 0.5, p[1] - 0.5])
                    .unwrap_or([-1.0, -1.0]);
                let p = if theta < radius * 0.975 && g.usable(xy[0], xy[1]) {
                    let c = DVec3::from_array(config.sphere.centre);
                    let dot = ray.dot(c);
                    let r = config.sphere.angular_radius.sin();
                    let disc = dot * dot + r * r - 1.0;
                    (disc > 0.0).then(|| ((ray * (dot - disc.sqrt()) - c) / r, xy))
                } else {
                    None
                };
                points.push(p);
            }
        }
        let window =
            ((window_fraction.clamp(0.0, 1.0) * size as f64).round() as usize | 1).min(size | 1);
        Ok(Self {
            config,
            basis,
            points,
            map: vec![128; map_width * map_height],
            previous: None,
            size,
            map_width,
            map_height,
            ratio: ratio.clamp(0.01, 100.0),
            window,
        })
    }
    pub(crate) fn reset_history(&mut self) {
        self.previous = None;
    }
    pub(crate) fn prepare(&self, frame: &GrayFrame) -> Vec<u8> {
        let n = self.size;
        let grey: Vec<_> = self
            .points
            .iter()
            .map(|p| {
                p.and_then(|(_, xy)| frame.sample(xy[0], xy[1]))
                    .map(|v| (v * 255.0).round() as u8)
                    .unwrap_or(128)
            })
            .collect();
        let mut blur = grey.clone();
        for y in 0..n {
            for x in 0..n {
                let mut values = [0u8; 9];
                let mut k = 0;
                for yy in y.saturating_sub(1)..=(y + 1).min(n - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(n - 1) {
                        values[k] = grey[yy * n + xx];
                        k += 1;
                    }
                }
                values[..k].sort_unstable();
                blur[y * n + x] = values[k / 2];
            }
        }
        let rad = self.window / 2;
        let mut hmin = vec![255; n * n];
        let mut hmax = vec![0; n * n];
        for y in 0..n {
            for x in 0..n {
                for xx in x.saturating_sub(rad)..=(x + rad).min(n - 1) {
                    let i = y * n + xx;
                    if self.points[i].is_none() {
                        continue;
                    }
                    let v = blur[i];
                    hmin[y * n + x] = hmin[y * n + x].min(v);
                    if v < 255 {
                        hmax[y * n + x] = hmax[y * n + x].max(v);
                    }
                }
            }
        }
        let mut roi = vec![128; n * n];
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                if self.points[i].is_none() {
                    continue;
                }
                let mut min = 255;
                let mut max = 0;
                for yy in y.saturating_sub(rad)..=(y + rad).min(n - 1) {
                    min = min.min(hmin[yy * n + x]);
                    max = max.max(hmax[yy * n + x]);
                }
                let value = if self.ratio * (f64::from(grey[i]) - f64::from(min))
                    <= f64::from(max) - f64::from(grey[i])
                {
                    0
                } else {
                    255
                };
                roi[i] = value;
            }
        }
        roi
    }
    pub fn update(&mut self, frame: &GrayFrame, row: Option<&TrackRow>) -> DebugViews {
        let n = self.size;
        let mw = self.map_width;
        let mh = self.map_height;
        let roi = self.prepare(frame);
        let mut diff = vec![0; n * n];
        let mut instant = vec![128; mw * mh];
        let orientation = row
            .map(|r| DQuat::from_scaled_axis(DVec3::from_array(r.absolute_camera)))
            .unwrap_or(DQuat::IDENTITY);
        let exported_basis = if self.config.legacy_roi_coordinates {
            self.basis
        } else {
            DQuat::IDENTITY
        };
        let orientation_cam = exported_basis.inverse() * orientation * exported_basis;
        let rotation = row
            .map(|r| exported_basis.inverse() * DVec3::from_array(r.delta_camera))
            .unwrap_or(DVec3::ZERO);
        let inverse_delta = DQuat::from_scaled_axis(-rotation);
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                let Some((p, _)) = self.points[i] else {
                    continue;
                };
                let value = roi[i];
                let previous_point = inverse_delta * p;
                let ray = self.basis
                    * (DVec3::from_array(self.config.sphere.centre)
                        + previous_point * self.config.sphere.angular_radius.sin());
                let radial = ray.x.hypot(ray.y);
                let factor = if radial > 1e-12 {
                    radial.atan2(ray.z) * n as f64
                        / (2.0 * self.config.sphere.angular_radius * radial)
                } else {
                    n as f64 / (2.0 * self.config.sphere.angular_radius * ray.z)
                };
                let xy = [
                    ray.x * factor + n as f64 * 0.5 - 0.5,
                    ray.y * factor + n as f64 * 0.5 - 0.5,
                ];
                if let Some(previous) = &self.previous
                    && let Some(v) = previous.sample(xy[0], xy[1])
                {
                    diff[i] = (f64::from(value) - v * 255.0).abs().min(255.0) as u8;
                }
                let p = self.basis * (orientation_cam.inverse() * p);
                let lon = p.x.atan2(p.z);
                let mx = ((-lon - PI) / TAU).rem_euclid(1.0) * mw as f64;
                let my = (p.y + 1.0) * 0.5 * mh as f64;
                let mi = (my.floor() as usize).min(mh - 1) * mw + (mx.floor() as usize).min(mw - 1);
                instant[mi] = value;
                if row.is_none_or(|r| r.accepted) {
                    self.map[mi] = match self.map[mi] {
                        0 => 0,
                        255 => 255,
                        128 => {
                            if value == 255 {
                                192
                            } else {
                                64
                            }
                        }
                        v => {
                            if value == 255 {
                                v.saturating_add(1)
                            } else {
                                v.saturating_sub(1)
                            }
                        }
                    };
                }
            }
        }
        self.previous = GrayFrame::new(
            n,
            n,
            roi.iter().map(|v| f32::from(*v) / 255.0).collect(),
            frame.timestamp_ms(),
        )
        .ok();
        DebugViews {
            roi,
            diff,
            instant,
            accumulated: self.map.clone(),
            roi_size: n,
            map_width: mw,
            map_height: mh,
        }
    }
}
