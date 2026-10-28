//! Deterministic image-space occluders over a known material-space sphere.
//! Shared by the audit driver and focused numerical regression tests.
use fictrac_core::{Config, Geometry, GrayFrame, MapSettings, SphereTemplate, Tracker};
use glam::{DQuat, DVec3};
use serde::Serialize;
use std::f64::consts::{PI, TAU};

pub type TestResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Case {
    pub warmup: usize,
    pub stationary: bool,
    pub covered_percent: usize,
    pub pattern: usize,
    pub acceleration: bool,
    pub return_to_known: bool,
}

#[derive(Debug, Serialize)]
pub struct Audit {
    pub case: Case,
    pub initial_coverage: f64,
    pub masked_measured: usize,
    pub masked_inaccurate: usize,
    pub masked_component_rms: Option<[f64; 3]>,
    pub masked_atlas_changed_rows: usize,
    pub rejected_mutations: usize,
    pub clear_measured: usize,
    pub clear_inaccurate: usize,
    pub clear_component_rms: Option<[f64; 3]>,
    pub first_accurate_clear_frame: Option<usize>,
    pub stable_clear_frame: Option<usize>,
    pub final_coverage: f64,
    pub atlas_wrong_fraction: f64,
    pub atlas_wrong_before_mask: Option<f64>,
    pub atlas_wrong_after_mask: Option<f64>,
    pub masked_max_pose_error: f64,
    pub final_reported_pose_error: Option<f64>,
}

pub struct Scene {
    pub config: Config,
    surface: Vec<Option<DVec3>>,
    texture: SphereTemplate,
    ranked_pixels: Vec<usize>,
}

pub fn atlas_index(p: DVec3, width: usize, height: usize) -> usize {
    let x = (((-p.x.atan2(p.z) - PI) / TAU).rem_euclid(1.0) * width as f64).floor();
    let y = ((p.y + 1.0) * 0.5 * height as f64).floor();
    (y as usize).min(height - 1) * width + (x as usize).min(width - 1)
}

impl Scene {
    pub fn new(floor: f64) -> TestResult<Self> {
        let config = Config {
            spherical_map: Some(MapSettings {
                resilient_error_floor: floor,
                ..MapSettings::default()
            }),
            legacy_roi_coordinates: true,
            ..Config::default()
        };
        let geometry = Geometry::new(config.clone())?;
        let mut surface = Vec::new();
        for y in 0..config.height {
            for x in 0..config.width {
                surface.push(geometry.surface(x as f64, y as f64));
            }
        }
        let mut ranked_pixels: Vec<_> = surface
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.map(|_| i))
            .collect();
        // A contiguous left-to-right panel; coverage is measured on actual
        // sphere pixels rather than on the image bounding rectangle.
        ranked_pixels.sort_by_key(|i| (i % config.width, i / config.width));
        let (width, height) = (180, 90);
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let py = (y as f64 + 0.5) / height as f64 * 2.0 - 1.0;
                let lon = -(x as f64 + 0.5) / width as f64 * TAU - PI;
                let r = (1.0 - py * py).sqrt();
                let p = DVec3::new(r * lon.sin(), py, r * lon.cos());
                let v = p.dot(DVec3::new(17., 11., 7.)).sin()
                    + 0.7 * p.dot(DVec3::new(-9., 23., 15.)).cos()
                    + 0.5 * p.dot(DVec3::new(31., -13., 19.)).sin();
                pixels.push(if v > 0.0 { 255 } else { 0 });
            }
        }
        Ok(Self {
            config,
            surface,
            texture: SphereTemplate::new(width, height, pixels)?,
            ranked_pixels,
        })
    }

    pub fn render(&self, q: DQuat, frame: usize, case: Option<Case>) -> TestResult<GrayFrame> {
        let inverse = q.inverse();
        let mut pixels: Vec<f32> = self
            .surface
            .iter()
            .map(|point| {
                point.map_or(0.12, |p| {
                    if self.texture.pixels[atlas_index(inverse * p, 180, 90)] == 255 {
                        0.95
                    } else {
                        0.05
                    }
                })
            })
            .collect();
        if let Some(case) = case {
            let target = self.ranked_pixels.len() * case.covered_percent / 100;
            let phase = frame.saturating_sub(case.warmup).min(59);
            let count = if case.pattern == 5 {
                target * phase.min(30) / 30
            } else {
                target
            };
            let offset = if case.pattern == 4 {
                phase * (self.ranked_pixels.len() - target) / 59
            } else {
                0
            };
            for &i in self.ranked_pixels.iter().skip(offset).take(count) {
                pixels[i] = match case.pattern {
                    0 | 4 => 0.0,
                    1 => 1.0,
                    2 => 0.1 + 0.8 * (i / self.config.width) as f32 / self.config.height as f32,
                    _ => {
                        let noise = (i as u64 * 73).wrapping_mul(6364136223846793005);
                        (noise >> 56) as f32 / 255.0
                    }
                };
            }
        }
        Ok(GrayFrame::new(
            self.config.width,
            self.config.height,
            pixels,
            frame as f64 * 10.0,
        )?)
    }

    pub fn audit(
        &self,
        case: Case,
        mut observe: impl FnMut(usize, &str, DVec3, &fictrac_core::TrackRow) -> TestResult<()>,
    ) -> TestResult<Audit> {
        let mut tracker = Tracker::new(self.config.clone())?;
        let mut q = DQuat::IDENTITY;
        let axis = DVec3::new(1.0, -0.3, 0.2).normalize();
        let mut initial_pose = None;
        for i in 0..case.warmup {
            if !case.stationary && i > 0 {
                q = DQuat::from_scaled_axis(axis * 0.02) * q;
            }
            let row = tracker.process(self.render(q, i, None)?)?;
            if row.status == "initializing" {
                initial_pose = Some(q);
            }
        }
        let initial = tracker.sphere_template().ok_or("missing initial atlas")?;
        let known_pose = q;
        let coverage = |map: &SphereTemplate| {
            map.pixels.iter().filter(|p| **p != 128).count() as f64 / map.pixels.len() as f64
        };
        let mut result = Audit {
            case,
            initial_coverage: coverage(&initial),
            masked_measured: 0,
            masked_inaccurate: 0,
            masked_component_rms: None,
            masked_atlas_changed_rows: 0,
            rejected_mutations: 0,
            clear_measured: 0,
            clear_inaccurate: 0,
            clear_component_rms: None,
            first_accurate_clear_frame: None,
            stable_clear_frame: None,
            final_coverage: 0.,
            atlas_wrong_fraction: 0.,
            atlas_wrong_before_mask: initial_pose.map(|q| self.atlas_wrong(&initial, q)),
            atlas_wrong_after_mask: None,
            masked_max_pose_error: 0.,
            final_reported_pose_error: None,
        };
        let mut mask_squared = [0.; 3];
        let mut clear_squared = [0.; 3];
        let mut streak = 0usize;
        for j in 0..240 {
            let rate = if case.acceleration {
                0.012 + 0.053 * ((j as f64 - 10.0) / 15.0).clamp(0., 1.)
            } else {
                0.012
            };
            let delta = axis * rate;
            let previous_q = q;
            q = if j == 60 && case.return_to_known {
                known_pose
            } else {
                DQuat::from_scaled_axis(delta) * q
            };
            let delta = (q * previous_q.inverse()).to_scaled_axis();
            let before = tracker.sphere_template().ok_or("missing pre-frame atlas")?;
            let masked = j < 60;
            let row =
                tracker.process(self.render(q, case.warmup + j, masked.then_some(case))?)?;
            if row.status == "initializing" {
                initial_pose = Some(q);
            }
            let after = tracker
                .sphere_template()
                .ok_or("missing post-frame atlas")?;
            if !row.accepted && before != after {
                result.rejected_mutations += 1;
            }
            if masked && before != after {
                result.masked_atlas_changed_rows += 1;
            }
            if masked
                && row.accepted
                && let Some(origin) = initial_pose
            {
                let actual = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
                let expected = q * origin.inverse();
                result.masked_max_pose_error = result
                    .masked_max_pose_error
                    .max((actual * expected.inverse()).to_scaled_axis().length());
            }
            if j == 59 {
                result.atlas_wrong_after_mask = initial_pose.map(|q| self.atlas_wrong(&after, q));
            }
            if let Some(origin) = initial_pose {
                let actual = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
                result.final_reported_pose_error = Some(
                    (actual * (q * origin.inverse()).inverse())
                        .to_scaled_axis()
                        .length(),
                );
            }
            let measured = row.status == "tracked";
            let error = DVec3::from_array(row.delta_camera) - delta;
            if measured {
                let (count, bad, squared) = if masked {
                    (
                        &mut result.masked_measured,
                        &mut result.masked_inaccurate,
                        &mut mask_squared,
                    )
                } else {
                    (
                        &mut result.clear_measured,
                        &mut result.clear_inaccurate,
                        &mut clear_squared,
                    )
                };
                *count += 1;
                *bad += usize::from(error.abs().max_element() >= 0.020);
                for (axis, value) in squared.iter_mut().enumerate() {
                    *value += error[axis].powi(2);
                }
            }
            if !masked {
                if measured && error.abs().max_element() < 0.020 {
                    result.first_accurate_clear_frame.get_or_insert(j - 60);
                    streak += 1;
                    if streak == 10 {
                        result.stable_clear_frame.get_or_insert(j - 69);
                    }
                } else {
                    streak = 0;
                }
            }
            observe(
                case.warmup + j,
                if masked { "masked" } else { "clear" },
                delta,
                &row,
            )?;
        }
        result.masked_component_rms = (result.masked_measured > 0)
            .then(|| mask_squared.map(|x| (x / result.masked_measured as f64).sqrt()));
        result.clear_component_rms = (result.clear_measured > 0)
            .then(|| clear_squared.map(|x| (x / result.clear_measured as f64).sqrt()));
        let map = tracker.sphere_template().ok_or("missing final atlas")?;
        result.final_coverage = coverage(&map);
        if let Some(origin) = initial_pose {
            result.atlas_wrong_fraction = self.atlas_wrong(&map, origin);
        }
        Ok(result)
    }

    fn atlas_wrong(&self, map: &SphereTemplate, origin: DQuat) -> f64 {
        let mut wrong = 0usize;
        let mut known = 0usize;
        for y in 0..map.height {
            for x in 0..map.width {
                let actual = map.pixels[y * map.width + x];
                if actual == 128 {
                    continue;
                }
                let py = (y as f64 + 0.5) / map.height as f64 * 2.0 - 1.0;
                let lon = -(x as f64 + 0.5) / map.width as f64 * TAU - PI;
                let r = (1.0 - py * py).sqrt();
                let p = origin.inverse() * DVec3::new(r * lon.sin(), py, r * lon.cos());
                let expected = self.texture.pixels[atlas_index(p, 180, 90)];
                wrong += usize::from((actual > 128) != (expected > 128));
                known += 1;
            }
        }
        wrong as f64 / known.max(1) as f64
    }
}
