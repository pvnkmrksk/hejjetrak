//! FicTrac-style accumulated atlas, bounded local fitting and deterministic global recovery.
use crate::diagnostics::LearningPolicy;
use crate::diagnostics::TemporalEvidence;
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
    temporal_solver: Bobyqa,
    local_stride: usize,
    settings: MapSettings,
    bound: f64,
    orientation: DQuat,
    guess: DVec3,
    ready: bool,
    clean_map: bool,
    template: Vec<u8>,
    temporal_map: Vec<u8>,
    temporal_valid: bool,
    prepared_temporal: Option<TemporalEvidence>,
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
#[derive(Clone, Copy)]
enum CapturePrediction {
    Normal,
    Extended,
    Ambiguous,
}
impl MapTracker {
    pub fn temporal_evidence(&self) -> Option<&TemporalEvidence> {
        self.prepared_temporal.as_ref()
    }
    pub fn new(config: &Config, settings: MapSettings) -> Result<Self> {
        let sampler = Diagnostics::new(
            config.clone(),
            settings.quality,
            settings.threshold_ratio,
            settings.threshold_window,
        )?;
        let mut solver_config = SolverConfig::new(3);
        let count = sampler.points.iter().flatten().count();
        // Resilient fitting uses the complete objective throughout local search:
        // a sampled objective can bias blurred-image increments even when its
        // final score is verified. Legacy mode retains sampled generation and
        // reserves two full verification evaluations in the original budget.
        let local_stride = if !settings.resilient && settings.max_evaluations >= 16 {
            count.div_ceil(1024).max(1)
        } else {
            1
        };
        solver_config.rho_begin = config.max_rotation * 0.5;
        solver_config.rho_end = settings.tolerance;
        solver_config.max_fun = settings.max_evaluations - if local_stride > 1 { 2 } else { 0 };
        let solver =
            Bobyqa::new(3, solver_config).map_err(|e| Error::Config(format!("BOBYQA: {e}")))?;
        let mut temporal_config = SolverConfig::new(3);
        temporal_config.rho_begin =
            (config.max_rotation * 0.5).min(0.04f64.max(settings.tolerance * 2.));
        temporal_config.rho_end = settings.tolerance;
        temporal_config.max_fun = settings.max_evaluations;
        let temporal_solver = Bobyqa::new(3, temporal_config)
            .map_err(|e| Error::Config(format!("temporal BOBYQA: {e}")))?;
        let template = sampler.map.clone();
        let temporal_map = vec![128; template.len()];
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
            temporal_solver,
            local_stride,
            settings,
            bound: config.max_rotation,
            orientation: DQuat::IDENTITY,
            guess: DVec3::ZERO,
            ready: false,
            clean_map: true,
            template,
            temporal_map,
            temporal_valid: false,
            prepared_temporal: None,
            recovery: crate::resilience::Recovery::default(),
            views: None,
        })
    }
    pub fn reset(&mut self) {
        self.temporal_valid = false;
        self.prepared_temporal = None;
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
        self.temporal_valid = false;
        self.prepared_temporal = None;
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
        self.prepared_temporal = None;
        let previous_interval_ms = self.prepared_delta_ms;
        self.prepared_delta_ms = self
            .previous_time
            .map_or(0.0, |previous| frame.timestamp_ms() - previous);
        let mut prediction = CapturePrediction::Normal;
        if self.settings.resilient && previous_interval_ms > 0.0 {
            let scale = self.prepared_delta_ms / previous_interval_ms;
            if scale > 1.8 {
                prediction = CapturePrediction::Extended;
            }
            let predicted = if scale > 1.8 {
                self.recovery
                    .recent_motion_prediction(self.prepared_delta_ms)
                    .unwrap_or(self.guess * scale)
            } else if scale < 1.0 / 1.8 {
                self.guess * scale
            } else {
                self.guess
            };
            // Keep the whole bounded search on the principal rotation branch.
            // A longer interval is not permission to infer an unseen full turn.
            self.guess =
                if predicted.is_finite() && predicted.length() + self.bound * 3.0f64.sqrt() < PI {
                    predicted
                } else {
                    prediction = CapturePrediction::Ambiguous;
                    DVec3::ZERO
                };
        }
        self.previous_time = Some(frame.timestamp_ms());
        let roi = self.sampler.prepare(frame);
        self.prepared_edge = if self.settings.resilient {
            crate::resilience::edge_density(&roi, self.sampler.size)
        } else {
            0.0
        };
        let estimate = self.estimate_roi(&roi, self.prepared_edge, prediction);
        self.prepared_roi = Some(roi);
        estimate
    }
    fn estimate_roi(&mut self, roi: &[u8], edge: f64, prediction: CapturePrediction) -> Estimate {
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
            if self.settings.resilient && self.clean_map {
                let informative = self
                    .sampler
                    .informative_sites(roi)
                    .iter()
                    .filter(|site| **site)
                    .count();
                if informative * 2 < count {
                    return failure(65025.0);
                }
            }
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
        if self.settings.resilient && matches!(prediction, CapturePrediction::Ambiguous) {
            self.recovery.reject();
            return failure(objective.cost(orientation, 1));
        }
        let extended_interval = matches!(prediction, CapturePrediction::Extended);
        if self.settings.resilient && self.recovery.lost_frames > 0 {
            let (pose, error) = self.recovery.reacquire(
                &objective,
                orientation,
                self.bound,
                &self.settings,
                self.prepared_delta_ms,
            );
            // Confirmation observes real motion between visible images. Keep
            // that increment only as the next solver seed; the recovery row
            // still exports zero relative motion across the missing interval.
            self.guess = pose.as_ref().map_or(DVec3::ZERO, |p| p.increment);
            return Estimate {
                rotation: DVec3::ZERO,
                absolute: pose.as_ref().map(|p| basis.inverse() * p.pose * basis),
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
        let mut error = if self.ready {
            let seed = x;
            let fitted = minimize_local(
                &mut self.solver,
                &objective,
                seed,
                orientation,
                self.bound,
                self.settings.resilient && extended_interval,
            );
            x = fitted.0;
            let error = fitted.1;
            if (self.settings.resilient && extended_interval) || objective.local_stride > 1 {
                let (checked, error) = objective.checked_candidate(seed, x, orientation);
                x = checked;
                error
            } else {
                error
            }
        } else {
            1e12
        };
        let limit = if self.settings.resilient {
            self.recovery.limit(&self.settings)
        } else {
            self.settings.max_error
        };
        // An appearance change can bias the atlas objective before its score
        // fails outright. Refine against the last accepted observation only
        // when the usual fit or motion consistency is uncertain. No reference
        // survives a rejection, and both fits retain the configured window.
        let motion = DVec3::from_array(x);
        if self.settings.resilient
            && self.temporal_valid
            && self.recovery.stable_motion_history()
            && (!acceptable(error, limit)
                || (!self
                    .recovery
                    .motion_ok(motion.length(), self.prepared_delta_ms)
                    && !self
                        .recovery
                        .accepted_ramp_ok(motion, self.prepared_delta_ms)))
        {
            let original_evidence = objective.temporal_evidence(
                DQuat::from_scaled_axis(motion) * orientation,
                &self.temporal_map,
                self.sampler.size,
            );
            let temporal_objective = AtlasObjective {
                map: &self.temporal_map,
                local_stride: 1,
                ..objective
            };
            let prediction = self.guess.to_array();
            let (seed, _) = temporal_objective.checked_candidate(prediction, x, orientation);
            let lower = prediction.map(|value| value - self.bound);
            let upper = prediction.map(|value| value + self.bound);
            let mut refined = seed;
            self.temporal_solver.minimize(
                |candidate| {
                    temporal_objective.local_cost(
                        DQuat::from_scaled_axis(DVec3::new(
                            candidate[0],
                            candidate[1],
                            candidate[2],
                        )) * orientation,
                    )
                },
                &mut refined,
                &lower,
                &upper,
            );
            let (refined, _) = temporal_objective.checked_candidate(seed, refined, orientation);
            let rotation = DQuat::from_scaled_axis(DVec3::from_array(refined)) * orientation;
            let evidence =
                objective.temporal_evidence(rotation, &self.temporal_map, self.sampler.size);
            let improved = evidence.verified
                && evidence.binary_mse.is_some_and(|score| {
                    original_evidence
                        .binary_mse
                        .is_none_or(|old| score < old * 0.9)
                });
            if improved {
                x = refined;
                error = objective.cost(rotation, 1);
                self.prepared_temporal = Some(evidence);
            } else {
                self.prepared_temporal = Some(original_evidence);
            }
        }
        let strict_score_good = acceptable(
            error,
            if self.settings.resilient {
                self.recovery.limit(&self.settings)
            } else {
                self.settings.max_error
            },
        );
        let motion = DVec3::from_array(x);
        if !strict_score_good
            && self.settings.resilient
            && self.temporal_valid
            && self.prepared_temporal.is_none()
        {
            self.prepared_temporal = Some(objective.temporal_evidence(
                DQuat::from_scaled_axis(motion) * orientation,
                &self.temporal_map,
                self.sampler.size,
            ));
        }
        // A quiet startup must not permanently force the learned atlas's
        // appearance ceiling onto a genuinely moving ball. An independent
        // consecutive-image check may support a fit against an informative
        // persistent atlas despite changed appearance. Explicit user caps
        // still win, and this cannot authorize a motion-history outlier.
        let score_good = strict_score_good
            || (self.settings.resilient
                && self.temporal_valid
                && self.prepared_temporal.as_ref().is_some_and(|evidence| {
                    self.recovery
                        .temporal_score_ok(error, &self.settings, evidence)
                }));
        let local_good = score_good
            && (!self.settings.resilient
                || self
                    .recovery
                    .motion_ok(motion.length(), self.prepared_delta_ms)
                || if self
                    .recovery
                    .accepted_ramp_ok(motion, self.prepared_delta_ms)
                {
                    self.recovery.begin_ramp_epoch();
                    true
                } else {
                    false
                });
        if self.settings.resilient && !local_good {
            if score_good {
                self.recovery.reject_motion_candidate(
                    DQuat::from_scaled_axis(DVec3::from_array(x)) * orientation,
                    DVec3::from_array(x),
                    self.prepared_delta_ms,
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
        self.temporal_valid = false;
        let learning = if !row.accepted {
            LearningPolicy::Frozen
        } else if self.settings.resilient {
            self.recovery.learning_policy(row.status)
        } else {
            LearningPolicy::Full
        };
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
                self.recovery.observe_tracked_motion(
                    if legacy {
                        DVec3::from_array(row.delta_camera)
                    } else {
                        self.sampler.basis * DVec3::from_array(row.delta_camera)
                    },
                    self.prepared_delta_ms,
                );
            }
            if self.settings.resilient
                && learning == LearningPolicy::Full
                && self.prepared_roi.is_some()
            {
                self.recovery
                    .observe(row.error, self.prepared_edge, self.orientation);
            }
        }
        self.views = Some(if let Some(roi) = self.prepared_roi.take() {
            if row.accepted && self.settings.resilient {
                self.temporal_map.fill(128);
                let inverse = DMat3::from_quat(self.orientation.inverse());
                let projection =
                    AtlasProjection::new(self.sampler.map_width, self.sampler.map_height);
                for &(point, index) in &self.active_sites {
                    self.temporal_map[projection.index(inverse * point)] = roi[index];
                }
                self.temporal_valid = true;
            }
            self.sampler.update_prepared(
                frame,
                Some(row),
                roi,
                if self.settings.resilient && learning == LearningPolicy::Full {
                    LearningPolicy::Guarded
                } else {
                    learning
                },
            )
        } else {
            self.sampler.update(frame, Some(row))
        });
    }
}
fn acceptable(error: f64, threshold: f64) -> bool {
    error.is_finite() && error < 1e11 && (threshold < 0.0 || error <= threshold)
}
fn minimize_local(
    solver: &mut Bobyqa,
    objective: &AtlasObjective<'_>,
    seed: [f64; 3],
    orientation: DQuat,
    bound: f64,
    smooth: bool,
) -> ([f64; 3], f64) {
    let mut x = seed;
    let lower = seed.map(|v| v - bound);
    let upper = seed.map(|v| v + bound);
    let outcome = solver.minimize(
        |x| {
            let rotation = DQuat::from_scaled_axis(DVec3::new(x[0], x[1], x[2])) * orientation;
            if smooth {
                objective.smooth_cost(rotation)
            } else {
                objective.local_cost(rotation)
            }
        },
        &mut x,
        &lower,
        &upper,
    );
    (x, outcome.f)
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
    fn smooth_cost(&self, rotation: DQuat) -> f64 {
        let inverse = DMat3::from_quat(rotation.inverse());
        let mut squared = 0.0;
        let mut good = 0usize;
        for &(point, index) in self.active_sites {
            let p = inverse * point;
            let x = (PI - p.x.atan2(p.z)) * (self.width as f64 / TAU) - 0.5;
            let y = ((p.y + 1.0) * (self.height as f64 * 0.5) - 0.5)
                .clamp(0.0, (self.height - 1) as f64);
            let fx = x.floor();
            let fy = y.floor();
            let x0 = fx.rem_euclid(self.width as f64) as usize;
            let x1 = (x0 + 1) % self.width;
            let y0 = fy as usize;
            let y1 = (y0 + 1).min(self.height - 1);
            let tx = x - fx;
            let ty = y - fy;
            let mut value = 0.0;
            let mut weight = 0.0;
            for (cell, w) in [
                (y0 * self.width + x0, (1.0 - tx) * (1.0 - ty)),
                (y0 * self.width + x1, tx * (1.0 - ty)),
                (y1 * self.width + x0, (1.0 - tx) * ty),
                (y1 * self.width + x1, tx * ty),
            ] {
                if self.map[cell] != 128 {
                    value += f64::from(self.map[cell]) * w;
                    weight += w;
                }
            }
            if weight >= 0.5 {
                squared += (f64::from(self.roi[index]) - value / weight).powi(2);
                good += 1;
            }
        }
        if good * 4 <= self.active_sites.len() {
            1e12
        } else {
            squared / good as f64
        }
    }
    fn temporal_evidence(&self, rotation: DQuat, map: &[u8], size: usize) -> TemporalEvidence {
        let inverse = DMat3::from_quat(rotation.inverse());
        let projection = AtlasProjection::new(self.width, self.height);
        let mut count = [0usize; 36];
        let mut squared = [0.0; 36];
        let mut persistent_known = 0usize;
        let (mut image_sum, mut map_sum, mut map_squared) = (0., 0., 0.);
        for &(point, index) in self.active_sites {
            let projected = projection.index(inverse * point);
            let persistent = self.map[projected];
            if persistent != 128 {
                persistent_known += 1;
                image_sum += f64::from(self.roi[index]);
                map_sum += f64::from(persistent);
                map_squared += f64::from(persistent).powi(2);
            }
            let value = map[projected];
            if value == 128 {
                continue;
            }
            let tile = ((index / size) * 6 / size).min(5) * 6 + ((index % size) * 6 / size).min(5);
            count[tile] += 1;
            squared[tile] += (f64::from(self.roi[index]) - f64::from(value)).powi(2);
        }
        let known: usize = count.iter().sum();
        let total: f64 = squared.iter().sum();
        let supported: usize = count
            .iter()
            .zip(squared)
            .filter_map(|(&count, error)| {
                (count >= 8 && error <= 16_000.0 * count as f64).then_some(count)
            })
            .sum();
        let verified = known * 2 >= self.active_sites.len()
            && known >= 32
            && total <= 10_000.0 * known as f64
            && supported * 100 >= known * 85;
        TemporalEvidence {
            known_samples: known,
            possible_samples: self.active_sites.len(),
            binary_mse: (known > 0).then(|| total / known as f64),
            spatially_supported_fraction: supported as f64 / known.max(1) as f64,
            verified,
            persistent_known_samples: persistent_known,
            persistent_chance_mse: (persistent_known > 0).then(|| {
                let count = persistent_known as f64;
                // ROI observations are binary, hence E[I²] = 255 E[I].
                // E[(I-M)²] under independence retains the actual marginal
                // contrast and atlas confidence instead of assuming 50/50.
                ((255. * image_sum + map_squared) / count
                    - 2. * (image_sum / count) * (map_sum / count))
                    .max(0.)
            }),
        }
    }
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
    fn confirmed_visible_motion_seeds_next_fit_without_exporting_gap_motion() -> Result<()> {
        let config = Config::default();
        let settings = MapSettings::default();
        let mut tracker = MapTracker::new(&config, settings)?;
        let width = tracker.sampler.map_width;
        let height = tracker.sampler.map_height;
        for y in 0..height {
            for x in 0..width {
                let py = (y as f64 + 0.5) * 2.0 / height as f64 - 1.0;
                let longitude = PI - (x as f64 + 0.5) * TAU / width as f64;
                let radius = (1.0 - py * py).sqrt();
                let point = DVec3::new(radius * longitude.sin(), py, radius * longitude.cos());
                let value = point.dot(DVec3::new(17.0, 11.0, 7.0)).sin()
                    + 0.7 * point.dot(DVec3::new(-9.0, 23.0, 15.0)).cos();
                tracker.sampler.map[y * width + x] = if value > 0.0 { 255 } else { 0 };
            }
        }
        let visible_increment = DVec3::new(0.19, -0.04, 0.02);
        let held = DQuat::from_scaled_axis(DVec3::new(0.3, 0.1, -0.2));
        let current = DQuat::from_scaled_axis(visible_increment) * held;
        let mut roi = vec![128; tracker.sampler.size * tracker.sampler.size];
        for &(point, index) in &tracker.active_sites {
            roi[index] = tracker.sampler.map[map_index(current.inverse() * point, width, height)];
        }
        tracker.ready = true;
        tracker.prepared_delta_ms = 10.0;
        tracker
            .recovery
            .reject_motion_candidate(held, visible_increment, 10.0);
        let map = tracker.sampler.map.clone();
        let estimate = tracker.estimate_roi(&roi, 0.1, CapturePrediction::Normal);
        assert_eq!(estimate.status, "relocalized");
        assert!(estimate.accepted);
        assert_eq!(estimate.rotation, DVec3::ZERO);
        assert!(estimate.absolute.is_some_and(|found| {
            (found * current.inverse()).to_scaled_axis().length() < 0.001
        }));
        assert!(
            (tracker.guess - visible_increment).length() < 0.001,
            "confirmed visible motion was discarded: {:?}",
            tracker.guess
        );
        assert_eq!(tracker.sampler.map, map);
        Ok(())
    }

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
    fn temporal_support_requires_distributed_agreement_and_enough_known_surface() {
        let size = 60usize;
        let mut sites = Vec::new();
        let mut active = Vec::new();
        let mut map = vec![128; 180 * 90];
        let mut roi = Vec::new();
        let projection = AtlasProjection::new(180, 90);
        for y in 0..size {
            for x in 0..size {
                let py = -0.8 + 1.6 * y as f64 / size as f64;
                let lon = -2.8 + 5.6 * x as f64 / size as f64;
                let radius = (1. - py * py).sqrt();
                let point = DVec3::new(radius * lon.sin(), py, radius * lon.cos());
                let value = if (x + y) % 2 == 0 { 0 } else { 255 };
                map[projection.index(point)] = value;
                sites.push(Some(point));
                active.push((point, roi.len()));
                roi.push(value);
            }
        }
        let evidence = |roi: &[u8], map: &[u8]| {
            AtlasObjective {
                sites: &sites,
                active_sites: &active,
                roi,
                map,
                width: 180,
                height: 90,
                count: active.len(),
                local_stride: 1,
            }
            .temporal_evidence(DQuat::IDENTITY, map, size)
        };
        let matched = evidence(&roi, &map);
        assert!(matched.verified);
        assert_eq!(matched.persistent_known_samples, active.len());
        assert!(
            matched
                .persistent_chance_mse
                .is_some_and(|value| (value - 65025. / 2.).abs() < 1e-9)
        );
        let mut distributed = roi.clone();
        for i in (0..distributed.len()).step_by(20) {
            distributed[i] = 255 - distributed[i];
        }
        assert!(
            evidence(&distributed, &map).verified,
            "small distributed threshold differences are supported"
        );
        let mut localized = roi.clone();
        for y in (0..size).step_by(3) {
            for x in 0..12 {
                localized[y * size + x] = 255 - localized[y * size + x];
            }
        }
        let localized = evidence(&localized, &map);
        assert!(
            localized.binary_mse.is_some_and(|score| score < 10_000.),
            "fixture must defeat a scalar-only check"
        );
        assert!(
            !localized.verified,
            "localized inconsistent region bypassed spatial check"
        );
        let sparse: Vec<_> = map
            .iter()
            .enumerate()
            .map(|(i, v)| if i % 5 == 0 { *v } else { 128 })
            .collect();
        assert!(
            !evidence(&roi, &sparse).verified,
            "unknown surface cannot establish independent support"
        );
        let unknown = evidence(&roi, &vec![128; map.len()]);
        assert!(!unknown.verified);
        assert_eq!(unknown.persistent_known_samples, 0);
        assert_eq!(unknown.persistent_chance_mse, None);
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
