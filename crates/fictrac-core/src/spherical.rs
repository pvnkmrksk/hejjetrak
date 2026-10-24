//! FicTrac-style accumulated atlas, bounded local fitting and deterministic global recovery.
use crate::{Config, DebugViews, Diagnostics, Error, GrayFrame, MapSettings, Result, TrackRow};
use bobyqa::{Bobyqa, Config as SolverConfig};
use glam::{DQuat, DVec3};
use std::f64::consts::{PI, TAU};

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
    solver: Bobyqa,
    settings: MapSettings,
    bound: f64,
    orientation: DQuat,
    guess: DVec3,
    ready: bool,
    clean_map: bool,
    template: Vec<u8>,
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
        solver_config.rho_begin = config.max_rotation * 0.5;
        solver_config.rho_end = settings.tolerance;
        solver_config.max_fun = settings.max_evaluations;
        let solver =
            Bobyqa::new(3, solver_config).map_err(|e| Error::Config(format!("BOBYQA: {e}")))?;
        let template = sampler.map.clone();
        Ok(Self {
            sampler,
            solver,
            settings,
            bound: config.max_rotation,
            orientation: DQuat::IDENTITY,
            guess: DVec3::ZERO,
            ready: false,
            clean_map: true,
            template,
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
        self.views = None;
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
        self.views = None;
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
        let roi = self.sampler.prepare(frame);
        let sites = &self.sampler.points;
        let basis = self.sampler.basis;
        let count = sites.iter().flatten().count();
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
            sites,
            roi: &roi,
            map: &self.sampler.map,
            basis,
            width: self.sampler.map_width,
            height: self.sampler.map_height,
            count,
        };
        let orientation = self.orientation;
        let mut x = self.guess.to_array();
        let lower = x.map(|v| v - self.bound);
        let upper = x.map(|v| v + self.bound);
        let error = if self.ready {
            self.solver
                .minimize(
                    |x| {
                        objective.cost(
                            DQuat::from_scaled_axis(DVec3::new(x[0], x[1], x[2])) * orientation,
                            1,
                        )
                    },
                    &mut x,
                    &lower,
                    &upper,
                )
                .f
        } else {
            1e12
        };
        let local_good = acceptable(error, self.settings.max_error);
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
        if row.accepted {
            let absolute = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
            self.orientation = if legacy {
                absolute
            } else {
                self.sampler.basis * absolute * self.sampler.basis.inverse()
            };
            self.ready = true;
            self.clean_map = false;
        }
        self.views = Some(self.sampler.update(frame, Some(row)));
    }
}
fn acceptable(error: f64, threshold: f64) -> bool {
    error.is_finite() && error < 1e11 && (threshold < 0.0 || error <= threshold)
}
struct AtlasObjective<'a> {
    sites: &'a [Option<(DVec3, [f64; 2])>],
    roi: &'a [u8],
    map: &'a [u8],
    basis: DQuat,
    width: usize,
    height: usize,
    count: usize,
}
impl AtlasObjective<'_> {
    fn cost(&self, rotation: DQuat, stride: usize) -> f64 {
        let inverse = rotation.inverse();
        let mut squared = 0.0;
        let mut count = 0;
        let mut good = 0;
        for i in (0..self.sites.len()).step_by(stride) {
            let Some((p, _)) = self.sites[i] else {
                continue;
            };
            count += 1;
            let value = self.map[map_index(inverse * (self.basis * p), self.width, self.height)];
            if value == 128 {
                continue;
            }
            let d = f64::from(self.roi[i]) - f64::from(value);
            squared += d * d;
            good += 1;
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
