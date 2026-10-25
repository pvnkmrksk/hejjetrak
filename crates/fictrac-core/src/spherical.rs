//! FicTrac-style accumulated atlas, bounded local fitting and deterministic global recovery.
use crate::{Config, DebugViews, Diagnostics, Error, GrayFrame, MapSettings, Result, TrackRow};
use bobyqa::{Bobyqa, Config as SolverConfig};
use glam::{DMat3, DQuat, DVec3};
use std::f64::consts::{PI, TAU};
use std::sync::OnceLock;

/// Equal-area atlas in FicTrac ROI/material axes. Value 128 denotes unseen surface.
#[derive(Clone, Debug, PartialEq)]
pub struct SphereTemplate {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}
impl SphereTemplate {
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Result<Self> {
        let template = Self {
            width,
            height,
            pixels,
        };
        template.validate()?;
        Ok(template)
    }
    pub fn validate(&self) -> Result<()> {
        if self.width < 30
            || self.width > 900
            || self.height.checked_mul(2) != Some(self.width)
            || self.width.checked_mul(self.height) != Some(self.pixels.len())
            || self.pixels.iter().filter(|p| **p != 128).take(32).count() < 32
        {
            return Err(Error::Config("sphere template must be a 2:1 equal-area image, 30..900 pixels wide, with at least 32 observed pixels".into()));
        }
        Ok(())
    }
}
pub(crate) struct MapTracker {
    sampler: Diagnostics,
    // Fixed ROI-to-atlas rays: preserve the exact per-site quaternion transform,
    // computing it once per calibration instead of every optimizer evaluation.
    atlas_sites: Vec<Option<DVec3>>,
    active_sites: Vec<(DVec3, usize)>,
    prepared_roi: Option<Vec<u8>>,
    prepared_edge: f64,
    previous_time: Option<f64>,
    prepared_delta_ms: f64,
    solver: Bobyqa,
    local_stride: usize,
    settings: MapSettings,
    bound: f64,
    orientation: DQuat,
    guess: DVec3,
    ready: bool,
    clean_map: bool,
    template: Vec<u8>,
    recovery: crate::resilience::Recovery,
    pub views: Option<DebugViews>,
}
pub(crate) struct Estimate {
    pub rotation: DVec3,
    /// A recovered absolute physical-camera pose; relocalisation never invents relative motion.
    pub absolute: Option<DQuat>,
    pub error: f64,
    pub accepted: bool,
    pub samples: usize,
    pub status: &'static str,
}
impl MapTracker {
    pub fn new(config: &Config, settings: MapSettings) -> Result<Self> {
        let sampler = Diagnostics::new(
            config.clone(),
            settings.quality,
            settings.threshold_ratio,
            settings.threshold_window,
        )?;
        let mut solver_config = SolverConfig::new(3);
        let count = sampler.points.iter().flatten().count();
        // Search a spatially distributed subset, then score both the candidate
        // and predictor on every site. Reserve those two evaluations in the
        // original budget. Small ROIs/budgets retain the complete objective.
        let local_stride = if settings.max_evaluations >= 16 {
            count.div_ceil(1024).max(1)
        } else {
            1
        };
        solver_config.rho_begin = config.max_rotation * 0.5;
        solver_config.rho_end = settings.tolerance;
        solver_config.max_fun = settings.max_evaluations - if local_stride > 1 { 2 } else { 0 };
        let solver =
            Bobyqa::new(3, solver_config).map_err(|e| Error::Config(format!("BOBYQA: {e}")))?;
        let template = sampler.map.clone();
        let atlas_sites: Vec<_> = sampler
            .points
            .iter()
            .map(|site| site.map(|(p, _)| sampler.basis * p))
            .collect();
        let active_sites = atlas_sites
            .iter()
            .enumerate()
            .filter_map(|(index, point)| point.map(|p| (p.normalize(), index)))
            .collect();
        Ok(Self {
            sampler,
            atlas_sites,
            active_sites,
            prepared_roi: None,
            prepared_edge: 0.0,
            previous_time: None,
            prepared_delta_ms: 0.0,
            solver,
            local_stride,
            settings,
            bound: config.max_rotation,
            orientation: DQuat::IDENTITY,
            guess: DVec3::ZERO,
            ready: false,
            clean_map: true,
            template,
            recovery: crate::resilience::Recovery::default(),
            views: None,
        })
    }
    pub fn reset(&mut self) {
        if !self.settings.global_search {
            self.sampler.map.clone_from(&self.template);
            self.clean_map = true;
        }
        self.orientation = DQuat::IDENTITY;
        self.guess = DVec3::ZERO;
        self.ready = false;
        self.sampler.reset_history();
        self.prepared_roi = None;
        self.views = None;
        self.recovery = crate::resilience::Recovery::default();
        self.previous_time = None;
        self.prepared_delta_ms = 0.0;
    }
    pub fn load_template(&mut self, template: SphereTemplate) -> Result<()> {
        template.validate()?;
        if template.width != self.sampler.map_width || template.height != self.sampler.map_height {
            return Err(Error::Config(format!(
                "sphere template must be {} by {} pixels for q_factor {}",
                self.sampler.map_width, self.sampler.map_height, self.settings.quality
            )));
        }
        self.template = template.pixels;
        self.sampler.map.clone_from(&self.template);
        self.clean_map = false;
        self.ready = false;
        self.orientation = DQuat::IDENTITY;
        self.guess = DVec3::ZERO;
        self.sampler.reset_history();
        self.prepared_roi = None;
        self.views = None;
        self.recovery = crate::resilience::Recovery::default();
        self.previous_time = None;
        self.prepared_delta_ms = 0.0;
        Ok(())
    }
    pub fn template(&self) -> SphereTemplate {
        SphereTemplate {
            width: self.sampler.map_width,
            height: self.sampler.map_height,
            pixels: self.sampler.map.clone(),
        }
    }
    pub fn estimate(&mut self, frame: &GrayFrame) -> Estimate {
        self.prepared_delta_ms = self
            .previous_time
            .map_or(0.0, |previous| frame.timestamp_ms() - previous);
        self.previous_time = Some(frame.timestamp_ms());
        let roi = self.sampler.prepare(frame);
        self.prepared_edge = if self.settings.resilient {
            crate::resilience::edge_density(&roi, self.sampler.size)
        } else {
            0.0
        };
        let estimate = self.estimate_roi(&roi, self.prepared_edge);
        self.prepared_roi = Some(roi);
        estimate
    }
    fn estimate_roi(&mut self, roi: &[u8], edge: f64) -> Estimate {
        let basis = self.sampler.basis;
        let count = self.active_sites.len();
        let failure = |error| Estimate {
            rotation: DVec3::ZERO,
            absolute: None,
            error,
            accepted: false,
            samples: count,
            status: "rejected",
        };
        let black = roi.iter().filter(|v| **v == 0).count();
        let white = roi.iter().filter(|v| **v == 255).count();
        if black < 8 || white < 8 {
            self.guess = DVec3::ZERO;
            if self.settings.resilient && self.ready {
                self.recovery.reject();
            }
            return failure(65025.0);
        }
        if self.settings.resilient && self.ready && !self.recovery.texture_ok(edge) {
            self.guess = DVec3::ZERO;
            self.recovery.reject();
            return failure(65025.0);
        }
        if !self.ready && (!self.settings.global_search || self.clean_map) {
            return Estimate {
                rotation: DVec3::ZERO,
                absolute: None,
                error: 0.0,
                accepted: true,
                samples: count,
                status: "initializing",
            };
        }
        let objective = AtlasObjective {
            sites: &self.atlas_sites,
            active_sites: &self.active_sites,
            roi,
            map: &self.sampler.map,
            width: self.sampler.map_width,
            height: self.sampler.map_height,
            count,
            local_stride: self.local_stride,
        };
        let orientation = self.orientation;
        if self.settings.resilient && self.recovery.lost_frames > 0 {
            let (pose, error) = self.recovery.reacquire(
                &objective,
                orientation,
                self.bound,
                &self.settings,
                self.prepared_delta_ms,
            );
            self.guess = DVec3::ZERO;
            return Estimate {
                rotation: DVec3::ZERO,
                absolute: pose.map(|p| basis.inverse() * p * basis),
                error: if error.is_finite() { error } else { 1e12 },
                accepted: pose.is_some(),
                samples: count,
                status: if pose.is_some() {
                    "relocalized"
                } else {
                    "recovering"
                },
            };
        }
        let mut x = self.guess.to_array();
        let lower = x.map(|v| v - self.bound);
        let upper = x.map(|v| v + self.bound);
        let error = if self.ready {
            let seed = x;
            let error = self
                .solver
                .minimize(
                    |x| {
                        objective.local_cost(
                            DQuat::from_scaled_axis(DVec3::new(x[0], x[1], x[2])) * orientation,
                        )
                    },
                    &mut x,
                    &lower,
                    &upper,
                )
                .f;
            if objective.local_stride > 1 {
                let (checked, error) = objective.checked_candidate(seed, x, orientation);
                x = checked;
                error
            } else {
                error
            }
        } else {
            1e12
        };
        let score_good = acceptable(
            error,
            if self.settings.resilient {
                self.recovery.limit(&self.settings)
            } else {
                self.settings.max_error
            },
        );
        let local_good = score_good
            && (!self.settings.resilient
                || self
                    .recovery
                    .motion_ok(DVec3::from_array(x).length(), self.prepared_delta_ms));
        if self.settings.resilient && !local_good {
            if score_good {
                self.recovery.reject_motion_candidate(
                    DQuat::from_scaled_axis(DVec3::from_array(x)) * orientation,
                );
            } else {
                self.recovery.reject();
            }
            self.guess = DVec3::ZERO;
            return failure(if error.is_finite() { error } else { 1e12 });
        }
        if self.settings.global_search && (!local_good || !self.ready) {
            let (absolute, global_error) = global_search(&objective, orientation, &self.settings);
            let accepted = acceptable(global_error, self.settings.max_error);
            self.guess = DVec3::ZERO;
            return Estimate {
                rotation: DVec3::ZERO,
                absolute: accepted.then(|| basis.inverse() * absolute * basis),
                error: global_error,
                accepted,
                samples: count,
                status: if accepted { "relocalized" } else { "rejected" },
            };
        }
        let delta = if local_good {
            DVec3::from_array(x)
        } else {
            DVec3::ZERO
        };
        self.guess = if local_good {
            delta * 0.9 + self.guess * 0.1
        } else {
            DVec3::ZERO
        };
        Estimate {
            rotation: basis.inverse() * delta,
            absolute: None,
            error: if error.is_finite() { error } else { 1e12 },
            accepted: local_good,
            samples: count,
            status: if local_good { "tracked" } else { "rejected" },
        }
    }
    pub fn commit(&mut self, frame: &GrayFrame, row: &TrackRow, legacy: bool) {
        let allow_learning =
            row.accepted && (!self.settings.resilient || self.recovery.allow_learning(row.status));
        if row.accepted {
            let absolute = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
            self.orientation = if legacy {
                absolute
            } else {
                self.sampler.basis * absolute * self.sampler.basis.inverse()
            };
            self.ready = true;
            self.clean_map = false;
            if self.settings.resilient && row.status == "tracked" {
                self.recovery.observe_motion(
                    DVec3::from_array(row.delta_camera).length(),
                    self.prepared_delta_ms,
                );
            }
            if self.settings.resilient && allow_learning && self.prepared_roi.is_some() {
                self.recovery
                    .observe(row.error, self.prepared_edge, self.orientation);
            }
        }
        self.views = Some(if let Some(roi) = self.prepared_roi.take() {
            self.sampler
                .update_prepared(frame, Some(row), roi, allow_learning)
        } else {
            self.sampler.update(frame, Some(row))
        });
    }
}
fn acceptable(error: f64, threshold: f64) -> bool {
    error.is_finite() && error < 1e11 && (threshold < 0.0 || error <= threshold)
}
pub(crate) struct AtlasObjective<'a> {
    sites: &'a [Option<DVec3>],
    active_sites: &'a [(DVec3, usize)],
    roi: &'a [u8],
    map: &'a [u8],
    width: usize,
    height: usize,
    count: usize,
    local_stride: usize,
}
impl AtlasObjective<'_> {
    pub(crate) fn checked_candidate(
        &self,
        seed: [f64; 3],
        candidate: [f64; 3],
        orientation: DQuat,
    ) -> ([f64; 3], f64) {
        let candidate_error = self.cost(
            DQuat::from_scaled_axis(DVec3::from_array(candidate)) * orientation,
            1,
        );
        let seed_error = self.cost(
            DQuat::from_scaled_axis(DVec3::from_array(seed)) * orientation,
            1,
        );
        if seed_error < candidate_error {
            (seed, seed_error)
        } else {
            (candidate, candidate_error)
        }
    }
    pub(crate) fn cost(&self, rotation: DQuat, stride: usize) -> f64 {
        self.evaluate(rotation, stride, false)
    }
    pub(crate) fn local_cost(&self, rotation: DQuat) -> f64 {
        self.evaluate(rotation, self.local_stride, true)
    }
    pub(crate) fn coarse_cost(&self, rotation: DQuat, target: usize) -> f64 {
        self.evaluate(
            rotation,
            self.active_sites.len().div_ceil(target).max(1),
            true,
        )
    }
    fn evaluate(&self, rotation: DQuat, stride: usize, compact: bool) -> f64 {
        // One transform per candidate, rather than expanding quaternion-vector
        // products for every ROI site. Dense sites retain original pixel order.
        let inverse = DMat3::from_quat(rotation.inverse());
        let projection = AtlasProjection::new(self.width, self.height);
        let mut squared = 0.0;
        let mut count = 0;
        let mut good = 0;
        let mut observe = |p, i| {
            count += 1;
            let value = self.map[projection.index(inverse * p)];
            if value == 128 {
                return;
            }
            let d = f64::from(self.roi[i]) - f64::from(value);
            squared += d * d;
            good += 1;
        };
        if stride == 1 || compact {
            for &(point, index) in self.active_sites.iter().step_by(stride) {
                observe(point, index);
            }
        } else {
            // Global coarse search keeps its original lattice, including mask
            // holes; compacting before stepping would change its candidates.
            for i in (0..self.sites.len()).step_by(stride) {
                if let Some(point) = self.sites[i] {
                    observe(point.normalize(), i);
                }
            }
        }
        if good * 4 <= count || count == 0 {
            1e12
        } else {
            squared / good as f64
        }
    }
}
/// Deterministic uniform SO(3) covering followed by bounded BOBYQA multistart.
/// Unlike upstream's stochastic NLopt CRS2 this produces reproducible recovery.
fn global_search(
    objective: &AtlasObjective<'_>,
    previous: DQuat,
    settings: &MapSettings,
) -> (DQuat, f64) {
    let budget = settings.global_evaluations;
    let coarse = (budget / 3).clamp(85, 32768);
    let stride = objective.count.div_ceil(768).max(1);
    let mut candidates = vec![(objective.cost(previous, stride), previous)];
    for i in 0..coarse {
        let q = if i == 0 {
            DQuat::IDENTITY
        } else {
            // Irrational rotations give a fixed low-discrepancy Shoemake covering.
            let a = (i as f64 + 0.5) / coarse as f64;
            let b = (i as f64 * 0.754_877_666_246_692_7).fract() * TAU;
            let c = (i as f64 * 0.569_840_290_998_053_2).fract() * TAU;
            DQuat::from_xyzw(
                (1.0 - a).sqrt() * b.sin(),
                (1.0 - a).sqrt() * b.cos(),
                a.sqrt() * c.sin(),
                a.sqrt() * c.cos(),
            )
        };
        let score = objective.cost(q, stride);
        if candidates.len() < 128 || score < candidates[candidates.len() - 1].0 {
            candidates.push((score, q));
            candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
            candidates.truncate(128);
        }
    }
    let mut best = (previous, objective.cost(previous, 1));
    let refinements = candidates.len().min((budget - coarse - 2) / 8);
    candidates.truncate(refinements);
    let per = ((budget - coarse - 2) / refinements).clamp(8, 1000);
    for (_, start) in candidates {
        let mut cfg = SolverConfig::new(3);
        cfg.rho_begin = 0.08;
        cfg.rho_end = settings.tolerance;
        cfg.max_fun = per;
        let Ok(mut solver) = Bobyqa::new(3, cfg) else {
            continue;
        };
        let mut delta = [0.0; 3];
        let outcome = solver.minimize(
            |x| {
                objective.cost(
                    DQuat::from_scaled_axis(DVec3::new(x[0], x[1], x[2])) * start,
                    1,
                )
            },
            &mut delta,
            &[-0.35; 3],
            &[0.35; 3],
        );
        if outcome.f < best.1 {
            best = (
                DQuat::from_scaled_axis(DVec3::from_array(delta)) * start,
                outcome.f,
            );
        }
    }
    best
}
pub(crate) fn map_index(p: DVec3, width: usize, height: usize) -> usize {
    let p = p.normalize();
    let lon = p.x.atan2(p.z);
    let x = (((-lon - PI) / TAU).rem_euclid(1.0) * width as f64).floor() as usize;
    let y = ((p.y + 1.0) * 0.5 * height as f64).floor() as usize;
    y.min(height - 1) * width + x.min(width - 1)
}

/// Unit-sphere projection with bounded atan interpolation. The interpolation
/// error on [0,1] is at most max|atan''|/(8*1024²) < 7.8e-8 radians.
/// Recompute precisely when that error could change the discrete map cell.
/// Points are normalized once at calibration; rotations preserve their norm.
struct AtlasProjection {
    width: usize,
    height: usize,
    atan: &'static [f64; 1025],
}
impl AtlasProjection {
    fn new(width: usize, height: usize) -> Self {
        static ATAN: OnceLock<[f64; 1025]> = OnceLock::new();
        Self {
            width,
            height,
            atan: ATAN.get_or_init(|| std::array::from_fn(|i| (i as f64 / 1024.0).atan())),
        }
    }
    #[inline]
    fn index(&self, p: DVec3) -> usize {
        let ax = p.x.abs();
        let az = p.z.abs();
        let major = ax.max(az);
        if major == 0.0 {
            return map_index(p, self.width, self.height);
        }
        let scaled = ax.min(az) / major * 1024.0;
        let i = (scaled as usize).min(1023);
        let mut lon = self.atan[i] + (scaled - i as f64) * (self.atan[i + 1] - self.atan[i]);
        if ax > az {
            lon = PI * 0.5 - lon;
        }
        if p.z < 0.0 {
            lon = PI - lon;
        }
        if p.x < 0.0 {
            lon = -lon;
        }
        let x = (PI - lon) * (self.width as f64 / TAU);
        let y = (p.y + 1.0) * (self.height as f64 * 0.5);
        // 1e-7 bounds interpolation plus roundoff. Latitude needs only a small
        // roundoff guard because immutable sites were normalized beforehand.
        if (x - x.round()).abs() <= self.width as f64 / TAU * 1e-7 + 1e-9
            || (y - y.round()).abs() <= 1e-9
        {
            return map_index(p, self.width, self.height);
        }
        (y as usize).min(self.height - 1) * self.width + (x as usize).min(self.width - 1)
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    #[test]
    fn reset_and_template_import_clear_motion_and_quality_history() -> Result<()> {
        let config = Config::default();
        let settings = MapSettings::default();
        let mut tracker = MapTracker::new(&config, settings.clone())?;
        for _ in 0..32 {
            tracker.recovery.observe_motion(0.006, 10.0);
            tracker.recovery.observe(2000.0, 0.1, DQuat::IDENTITY);
        }
        assert!(!tracker.recovery.motion_ok(0.1, 10.0));
        assert_eq!(tracker.recovery.limit(&settings), 10000.0);
        tracker.previous_time = Some(200.0);
        tracker.reset();
        assert!(tracker.recovery.motion_ok(0.1, 10.0));
        assert_eq!(tracker.recovery.limit(&settings), 18000.0);
        assert_eq!(tracker.previous_time, None);
        for _ in 0..32 {
            tracker.recovery.observe_motion(0.006, 10.0);
        }
        assert!(!tracker.recovery.motion_ok(0.1, 10.0));
        let template = SphereTemplate::new(
            tracker.sampler.map_width,
            tracker.sampler.map_height,
            vec![0; tracker.sampler.map.len()],
        )?;
        tracker.load_template(template)?;
        assert!(tracker.recovery.motion_ok(0.1, 10.0));
        assert_eq!(tracker.previous_time, None);
        Ok(())
    }

    #[test]
    fn sampled_blind_spot_cannot_hide_error_from_final_quality_decision() {
        let point = DVec3::new(0.17, 0.28, 0.95).normalize();
        let sites = vec![Some(point); 2048];
        let active_sites: Vec<_> = (0..sites.len()).map(|i| (point, i)).collect();
        let roi: Vec<_> = (0..sites.len())
            .map(|i| if i % 3 == 0 { 0 } else { 255 })
            .collect();
        let map = vec![0; 180 * 90];
        let objective = AtlasObjective {
            sites: &sites,
            active_sites: &active_sites,
            roi: &roi,
            map: &map,
            width: 180,
            height: 90,
            count: sites.len(),
            local_stride: 3,
        };
        assert_eq!(objective.local_cost(DQuat::IDENTITY), 0.0);
        let (_, error) = objective.checked_candidate([0.0; 3], [0.0; 3], DQuat::IDENTITY);
        let expected =
            roi.iter().filter(|v| **v == 255).count() as f64 * 65025.0 / roi.len() as f64;
        assert_eq!(error, expected);
        assert!(!acceptable(error, 12000.0));
    }
    #[test]
    fn interpolated_projection_matches_precise_bins_including_boundaries() {
        for width in [30, 180, 900] {
            let height = width / 2;
            let projection = AtlasProjection::new(width, height);
            for lat in 0..=100 {
                let y = -1.0 + 2.0 * lat as f64 / 100.0;
                let radius = (1.0 - y * y).max(0.0).sqrt();
                for cell in 0..width {
                    for offset in [-1e-8, 0.0, 1e-8, 0.37, 0.99999] {
                        let lon = PI - (cell as f64 + offset) * TAU / width as f64;
                        let point = DVec3::new(lon.sin() * radius, y, lon.cos() * radius);
                        assert_eq!(
                            projection.index(point),
                            map_index(point, width, height),
                            "width={width} lat={lat} cell={cell} offset={offset}"
                        );
                    }
                }
            }
        }
    }
}
