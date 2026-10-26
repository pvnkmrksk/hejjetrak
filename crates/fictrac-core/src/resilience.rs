//! Bounded, confirmed reacquisition; uncertain frames never teach the atlas.
use crate::MapSettings;
use crate::spherical::AtlasObjective;
use bobyqa::{Bobyqa, Config as SolverConfig};
use glam::{DQuat, DVec3};
use std::f64::consts::TAU;

const MAX_TRUSTED_POSES: usize = 256;
const POSE_SEPARATION: f64 = 0.08;
const MAX_RECOVERY_EVALUATIONS: usize = 768;
const HISTORY: usize = 32;
const MIN_MOTION_HISTORY: usize = 8;
// Per-frame angular allowance, not an FPS-independent physical velocity or
// measured motion. It permits acceleration after rest or a small historical rate.
const MOTION_FLOOR_RADIANS: f64 = 0.04;

#[derive(Default)]
struct Samples {
    values: [f64; HISTORY],
    len: usize,
    next: usize,
}
impl Samples {
    fn push(&mut self, value: f64) {
        if !value.is_finite() || value < 0.0 {
            return;
        }
        self.values[self.next] = value;
        self.next = (self.next + 1) % HISTORY;
        self.len = (self.len + 1).min(HISTORY);
    }
    fn median_mad(&self) -> (f64, f64) {
        if self.len == 0 {
            return (0.0, 0.0);
        }
        let mut sorted = [0.0; HISTORY];
        sorted[..self.len].copy_from_slice(&self.values[..self.len]);
        sorted[..self.len].sort_unstable_by(f64::total_cmp);
        let median = sorted[self.len / 2];
        for (out, value) in sorted[..self.len].iter_mut().zip(&self.values[..self.len]) {
            *out = (*value - median).abs();
        }
        sorted[..self.len].sort_unstable_by(f64::total_cmp);
        (median, sorted[self.len / 2])
    }
}

#[derive(Default)]
pub(crate) struct Recovery {
    pub lost_frames: u32,
    pending: Option<DQuat>,
    pending_motion: Option<(DVec3, usize)>,
    attempts: u64,
    error_mean: f64,
    errors: Samples,
    motion_rates: Samples,
    recent_velocity: [DVec3; 3],
    recent_interval: [f64; 3],
    recent_motion_count: usize,
    probation: usize,
    edge_mean: f64,
    // Unit ROI poses learned only from accepted frames. This small spatial bank
    // revisits known views instead of expecting a coarse uniform SO(3) cover to
    // land near an earlier appearance after a long occlusion.
    trusted: Vec<DQuat>,
    // Exact nearest-neighbor similarities, updated incrementally. Coverage
    // replacement is normally linear; only neighbors of an evicted pose need
    // a bounded rescan of the bank.
    trusted_nearest: Vec<(usize, f64)>,
    next_trusted: usize,
}
pub(crate) struct RecoveredPose {
    pub pose: DQuat,
    /// Observed between consecutive clear confirmation images, never across
    /// the missing interval. This seeds the next fit without being integrated.
    pub increment: DVec3,
}
impl Recovery {
    pub fn reject(&mut self) {
        self.lost_frames = self.lost_frames.saturating_add(1);
        self.pending = None;
        self.pending_motion = None;
        self.recent_motion_count = 0;
    }
    pub fn reject_motion_candidate(&mut self, pose: DQuat, delta: DVec3, delta_ms: f64) {
        self.reject();
        if pose.is_finite() {
            self.pending = Some(pose);
            if delta.is_finite() && delta_ms.is_finite() && delta_ms > 0.0 {
                let rate = delta / delta_ms;
                if rate.is_finite() {
                    // This is a search proposal, not an accepted velocity or a
                    // confirmation sample. Three subsequent consistent visible
                    // increments are still required for a new rate epoch.
                    self.pending_motion = Some((rate, 0));
                }
            }
        }
    }
    fn predicted_pending_pose(&self, start: DQuat, bound: f64, delta_ms: f64) -> Option<DQuat> {
        let (rate, _) = self.pending_motion?;
        if !delta_ms.is_finite() || delta_ms <= 0.0 {
            return None;
        }
        let delta = rate * delta_ms;
        // Match the existing visible-confirmation increment bound. Do not
        // extrapolate an unbounded pose through a missed/long source interval.
        if !delta.is_finite() || delta.length() > bound * 3.0f64.sqrt() {
            return None;
        }
        Some(DQuat::from_scaled_axis(delta) * start)
    }
    pub fn limit(&self, settings: &MapSettings) -> f64 {
        let adaptive = if self.error_mean > 0.0 {
            (self.error_mean * 1.8 + 2000.0).clamp(10000.0, 18000.0)
        } else {
            18000.0
        };
        // Accepted-only robust history can tighten the old EMA ceiling, never
        // loosen it. A few marginal fits cannot ratchet the ceiling upwards.
        let adaptive = if self.errors.len >= MIN_MOTION_HISTORY {
            let (median, _) = self.errors.median_mad();
            adaptive.min((median * 1.8 + 2000.0).clamp(10000.0, 18000.0))
        } else {
            adaptive
        };
        if settings.max_error >= 0.0 {
            adaptive.min(settings.max_error)
        } else {
            adaptive
        }
    }
    pub fn texture_ok(&self, edge: f64) -> bool {
        self.edge_mean == 0.0 || edge >= self.edge_mean * 0.3
    }
    /// Recovery can measure without immediately rewriting long-lived evidence.
    pub fn allow_learning(&mut self, status: &str) -> bool {
        if status == "relocalized" {
            self.probation = self.probation.max(3);
            return false;
        }
        if self.probation > 0 {
            self.probation -= 1;
            return false;
        }
        true
    }
    pub fn motion_ok(&self, angle: f64, delta_ms: f64) -> bool {
        if !angle.is_finite() || angle < 0.0 || !delta_ms.is_finite() || delta_ms <= 0.0 {
            return false;
        }
        if self.motion_rates.len < MIN_MOTION_HISTORY {
            return true;
        }
        let (median, mad) = self.motion_rates.median_mad();
        angle <= MOTION_FLOOR_RADIANS.max((4.0 * median + 8.0 * mad) * delta_ms)
    }
    pub fn observe_motion(&mut self, angle: f64, delta_ms: f64) {
        if delta_ms.is_finite() && delta_ms > 0.0 && angle.is_finite() && angle >= 0.0 {
            self.motion_rates.push(angle / delta_ms);
        }
    }
    pub fn observe_tracked_motion(&mut self, delta: DVec3, delta_ms: f64) {
        self.observe_motion(delta.length(), delta_ms);
        if !delta.is_finite() || !delta_ms.is_finite() || delta_ms <= 0.0 {
            return;
        }
        self.recent_velocity.rotate_left(1);
        self.recent_interval.rotate_left(1);
        self.recent_velocity[2] = delta / delta_ms;
        self.recent_interval[2] = delta_ms;
        self.recent_motion_count = (self.recent_motion_count + 1).min(3);
    }
    /// Three already accepted, consistently accelerating visible measurements
    /// plus this complete-score-verified image can establish a new rate epoch
    /// without declaring an artificial gap in a continuous measured ramp.
    pub fn accepted_ramp_ok(&self, delta: DVec3, delta_ms: f64) -> bool {
        if self.recent_motion_count < 3
            || !delta.is_finite()
            || !delta_ms.is_finite()
            || delta_ms <= 0.0
        {
            return false;
        }
        let rates = self.recent_velocity.map(|v| v.length());
        let rise = [
            (rates[1] - rates[0]) / ((self.recent_interval[0] + self.recent_interval[1]) * 0.5),
            (rates[2] - rates[1]) / ((self.recent_interval[1] + self.recent_interval[2]) * 0.5),
        ];
        if rates[0] <= 0.0 || rise[0] <= 0.0 || rise[1] <= 0.0 {
            return false;
        }
        // Speed must rise on both accepted transitions at a consistent scale;
        // direction must remain supported by all three accepted observations.
        if rise[0].max(rise[1]) > rise[0].min(rise[1]) * 4.0
            || (0..2).any(|i| {
                self.recent_velocity[i].dot(self.recent_velocity[i + 1])
                    < 0.75 * rates[i] * rates[i + 1]
            })
        {
            return false;
        }
        let velocity = delta / delta_ms;
        let rate = velocity.length();
        let prediction =
            rates[2] + 2.0 * rise[0].max(rise[1]) * (self.recent_interval[2] + delta_ms) * 0.5;
        rate >= rates[2] * 0.65
            && rate <= prediction + (0.006 / delta_ms).max(prediction * 0.35)
            && velocity.dot(self.recent_velocity[2]) >= 0.75 * rate * rates[2]
    }
    pub fn begin_ramp_epoch(&mut self) {
        self.motion_rates = Samples::default();
        for velocity in self.recent_velocity {
            self.motion_rates.push(velocity.length());
        }
    }
    pub fn observe(&mut self, error: f64, edge: f64, pose: DQuat) {
        if error.is_finite() && (0.0..1e11).contains(&error) {
            self.errors.push(error);
        }
        if error > 0.0 && error < 1e11 {
            self.error_mean = if self.error_mean > 0.0 {
                self.error_mean * 0.95 + error * 0.05
            } else {
                error
            };
        }
        self.edge_mean = if self.edge_mean > 0.0 {
            self.edge_mean * 0.95 + edge * 0.05
        } else {
            edge
        };
        let nearest = self
            .trusted
            .iter()
            .enumerate()
            .map(|(index, saved)| (index, saved.dot(pose).abs()))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if nearest.is_some_and(|(_, similarity)| similarity >= (POSE_SEPARATION * 0.5).cos()) {
            return;
        }
        if self.trusted.len() < MAX_TRUSTED_POSES {
            let index = self.trusted.len();
            for (saved, neighbor) in self.trusted.iter().zip(&mut self.trusted_nearest) {
                let similarity = saved.dot(pose).abs();
                if similarity > neighbor.1 {
                    *neighbor = (index, similarity);
                }
            }
            self.trusted.push(pose);
            self.trusted_nearest
                .push(nearest.unwrap_or((usize::MAX, -1.0)));
        } else {
            // Evict a member of the closest existing pair only when this new
            // view is farther from the bank than that pair's separation. A
            // long recent cluster must not erase an isolated older viewpoint.
            let Some((replacement, similarity)) = self
                .trusted_nearest
                .iter()
                .enumerate()
                .cycle()
                .skip(self.next_trusted)
                .take(self.trusted.len())
                .max_by(|a, b| a.1.1.total_cmp(&b.1.1))
                .map(|(index, neighbor)| (index, neighbor.1))
            else {
                return;
            };
            if nearest.is_none_or(|(_, incoming)| incoming >= similarity) {
                return;
            }
            self.trusted[replacement] = pose;
            self.next_trusted = (replacement + 1) % MAX_TRUSTED_POSES;
            for index in 0..self.trusted.len() {
                if index == replacement || self.trusted_nearest[index].0 == replacement {
                    self.trusted_nearest[index] = self.nearest_to(index);
                } else {
                    let similarity = self.trusted[index].dot(pose).abs();
                    if similarity > self.trusted_nearest[index].1 {
                        self.trusted_nearest[index] = (replacement, similarity);
                    }
                }
            }
        }
    }
    fn nearest_to(&self, index: usize) -> (usize, f64) {
        self.trusted
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(other, pose)| (other, self.trusted[index].dot(*pose).abs()))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((usize::MAX, -1.0))
    }
    /// Returns an absolute ROI pose only after two consecutive matching frames.
    /// No relative motion across the rejected interval can be inferred here.
    pub fn reacquire(
        &mut self,
        objective: &AtlasObjective<'_>,
        previous: DQuat,
        bound: f64,
        settings: &MapSettings,
        delta_ms: f64,
    ) -> (Option<RecoveredPose>, f64) {
        self.lost_frames = self.lost_frames.saturating_add(1);
        // Candidate probes, local fitting, coarse probes and complete scoring
        // all consume the same cap. Recovery can never spend the original
        // potentially million-evaluation global budget on every missing frame.
        let mut budget = Budget::new(settings.global_evaluations.min(MAX_RECOVERY_EVALUATIONS));
        let limit = self.limit(settings);
        let start = self.pending.unwrap_or(previous);
        let seed_error = budget.full(objective, start);
        let confirm = self.pending.is_some();
        if confirm || seed_error <= limit * 1.2 || self.lost_frames.is_multiple_of(12) {
            // A sharp fast-moving ball can have a good first candidate while
            // violating its old slow-rate history. Starting every subsequent
            // fit at zero velocity can then lose that candidate permanently.
            // Compare the held-velocity proposal on the complete ROI, without
            // changing any score/motion/texture acceptance or learning rule.
            let fit_start = self
                .predicted_pending_pose(start, bound, delta_ms)
                .filter(|pose| budget.full(objective, *pose) < seed_error)
                .unwrap_or(start);
            let (mut pose, mut error) =
                refine(objective, fit_start, bound, settings, &mut budget, 64);
            if fit_start != start && error > limit {
                let fallback = refine(objective, start, bound, settings, &mut budget, 64);
                if fallback.1 < error {
                    (pose, error) = fallback;
                }
            }
            if error.is_finite() && error <= limit {
                let delta = (pose * start.inverse()).to_scaled_axis();
                if confirm && self.motion_ok(delta.length(), delta_ms) {
                    self.pending = None;
                    self.pending_motion = None;
                    self.lost_frames = 0;
                    return (
                        Some(RecoveredPose {
                            pose,
                            increment: delta,
                        }),
                        error,
                    );
                }
                if !confirm {
                    self.pending = Some(pose);
                    return (None, error);
                }
                // A rate change beyond old history needs independent temporal
                // evidence, never a single outlier or two unrelated poses.
                if delta_ms > 0.0 && delta.length() <= bound * 3.0f64.sqrt() {
                    let rate = delta / delta_ms;
                    let streak = if let Some((previous, count)) = self.pending_motion {
                        if (rate - previous).length()
                            <= (0.006 / delta_ms).max(rate.length() * 0.35)
                        {
                            count + 1
                        } else {
                            1
                        }
                    } else {
                        1
                    };
                    if streak >= 3 {
                        self.pending = None;
                        self.pending_motion = None;
                        self.lost_frames = 0;
                        // The newly confirmed speed starts a motion epoch; this
                        // does not modify atlas/error/view history or invent gap motion.
                        self.motion_rates = Samples::default();
                        self.probation = MIN_MOTION_HISTORY;
                        return (
                            Some(RecoveredPose {
                                pose,
                                increment: delta,
                            }),
                            error,
                        );
                    }
                    self.pending = Some(pose);
                    self.pending_motion = Some((rate, streak));
                    return (None, error);
                }
                // Inconsistent large motion is not confirmation. Neither this
                // pose nor its map is learned.
            }
        }
        self.pending = None;
        self.pending_motion = None;
        if self.lost_frames % 30 == 1 {
            let (pose, error) = search(
                objective,
                previous,
                &self.trusted,
                self.attempts,
                settings,
                &mut budget,
                limit,
            );
            self.attempts = self.attempts.saturating_add(1);
            if error.is_finite() && error <= limit {
                self.pending = Some(pose);
            }
            return (None, error);
        }
        (None, seed_error)
    }
}

/// Hard bound on actual objective evaluations, including final verification.
struct Budget {
    remaining: usize,
}
impl Budget {
    fn new(remaining: usize) -> Self {
        Self { remaining }
    }
    fn take(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
    fn full(&mut self, objective: &AtlasObjective<'_>, pose: DQuat) -> f64 {
        if self.take() {
            objective.cost(pose, 1)
        } else {
            1e12
        }
    }
    fn coarse(&mut self, objective: &AtlasObjective<'_>, pose: DQuat) -> f64 {
        if self.take() {
            // A sparse 128-site probe can rank a coincidental patch ahead of
            // the correct learned view on a real speckled ball. Recovery is
            // infrequent; spend more sites per bounded probe before complete
            // refinement, retaining the same total objective-evaluation cap.
            objective.coarse_cost(pose, 512)
        } else {
            1e12
        }
    }
}

fn refine(
    objective: &AtlasObjective<'_>,
    start: DQuat,
    bound: f64,
    settings: &MapSettings,
    budget: &mut Budget,
    cap: usize,
) -> (DQuat, f64) {
    let seed_error = budget.full(objective, start);
    let evaluations = budget
        .remaining
        .saturating_sub(1)
        .min(cap)
        .min(settings.max_evaluations);
    if evaluations < 8 {
        return (start, seed_error);
    }
    let mut cfg = SolverConfig::new(3);
    cfg.rho_begin = (bound * 0.5).min(0.08);
    cfg.rho_end = settings.tolerance;
    cfg.max_fun = evaluations;
    let Ok(mut solver) = Bobyqa::new(3, cfg) else {
        return (start, seed_error);
    };
    let mut delta = [0.0; 3];
    solver.minimize(
        |x| {
            if budget.take() {
                // Recovery is rare and can change the absolute reference; optimize
                // every ROI site, not only verify a sampled-search candidate.
                objective.cost(
                    DQuat::from_scaled_axis(DVec3::new(x[0], x[1], x[2])) * start,
                    1,
                )
            } else {
                1e12
            }
        },
        &mut delta,
        &[-bound; 3],
        &[bound; 3],
    );
    let pose = DQuat::from_scaled_axis(DVec3::from_array(delta)) * start;
    let error = budget.full(objective, pose);
    if seed_error < error {
        (start, seed_error)
    } else {
        (pose, error)
    }
}

fn add_candidate(candidates: &mut Vec<(f64, DQuat)>, score: f64, pose: DQuat) {
    if candidates.len() < 4 || score < candidates[candidates.len() - 1].0 {
        candidates.push((score, pose));
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        candidates.truncate(4);
    }
}

/// Prefer known accepted views. A rotating deterministic coarse cover remains
/// available for an unknown pose, within the same bounded evaluation allowance.
fn search(
    objective: &AtlasObjective<'_>,
    previous: DQuat,
    trusted: &[DQuat],
    attempt: u64,
    settings: &MapSettings,
    budget: &mut Budget,
    limit: f64,
) -> (DQuat, f64) {
    let mut best = (previous, budget.full(objective, previous));
    let mut candidates = vec![];
    let probes = trusted.len().min(budget.remaining / 2);
    for i in 0..probes {
        let index = (i + (attempt as usize % trusted.len()) * probes) % trusted.len();
        let pose = trusted[index];
        add_candidate(&mut candidates, budget.coarse(objective, pose), pose);
    }
    for (_, pose) in candidates {
        let result = refine(objective, pose, 0.25, settings, budget, 48);
        if result.1 < best.1 {
            best = result;
        }
        if best.1 <= limit {
            return best;
        }
    }
    let mut candidates = vec![];
    let coarse = 256.min(budget.remaining / 2);
    for i in 0..coarse {
        let phase = i as f64 + attempt as f64 * 256.0;
        let a = (i as f64 + 0.5) / coarse as f64;
        let b = (phase * 0.754_877_666_246_692_7).fract() * TAU;
        let c = (phase * 0.569_840_290_998_053_2).fract() * TAU;
        let pose = DQuat::from_xyzw(
            (1.0 - a).sqrt() * b.sin(),
            (1.0 - a).sqrt() * b.cos(),
            a.sqrt() * c.sin(),
            a.sqrt() * c.cos(),
        );
        add_candidate(&mut candidates, budget.coarse(objective, pose), pose);
    }
    for (_, pose) in candidates {
        let result = refine(objective, pose, 0.5, settings, budget, 48);
        if result.1 < best.1 {
            best = result;
        }
        if best.1 <= limit {
            break;
        }
    }
    best
}

pub(crate) fn edge_density(roi: &[u8], n: usize) -> f64 {
    let mut edges = 0usize;
    let mut pairs = 0usize;
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            if roi[i] == 128 {
                continue;
            }
            if x + 1 < n && roi[i + 1] != 128 {
                pairs += 1;
                edges += usize::from(roi[i] != roi[i + 1]);
            }
            if y + 1 < n && roi[i + n] != 128 {
                pairs += 1;
                edges += usize::from(roi[i] != roi[i + n]);
            }
        }
    }
    edges as f64 / pairs.max(1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_pose_bank_is_spatially_deduplicated_and_bounded() {
        let mut recovery = Recovery::default();
        recovery.observe(1000.0, 0.1, DQuat::IDENTITY);
        recovery.observe(1000.0, 0.1, -DQuat::IDENTITY);
        recovery.observe(1000.0, 0.1, DQuat::from_rotation_x(0.02));
        assert_eq!(recovery.trusted.len(), 1);
        for i in 0..2000 {
            let a = i as f64;
            recovery.observe(
                1000.0,
                0.1,
                DQuat::from_euler(glam::EulerRot::XYZ, a * 0.13, a * 0.19, a * 0.31),
            );
        }
        assert_eq!(recovery.trusted.len(), MAX_TRUSTED_POSES);
        assert_eq!(recovery.trusted_nearest.len(), MAX_TRUSTED_POSES);
        for index in 0..MAX_TRUSTED_POSES {
            assert_eq!(
                recovery.trusted_nearest[index].1,
                recovery.nearest_to(index).1
            );
        }
        let known = recovery.trusted.clone();
        recovery.reject();
        assert_eq!(recovery.trusted, known);
    }
    #[test]
    fn spatial_bank_retains_an_old_isolated_view_after_a_long_recent_cluster() {
        let mut recovery = Recovery::default();
        let old = DQuat::from_rotation_y(-2.5);
        recovery.observe(2000.0, 0.1, old);
        for index in 0..255 {
            recovery.observe(
                2000.0,
                0.1,
                DQuat::from_euler(
                    glam::EulerRot::XYZ,
                    (index % 17) as f64 * 0.09,
                    (index / 17) as f64 * 0.09,
                    0.0,
                ),
            );
        }
        assert_eq!(recovery.trusted.len(), MAX_TRUSTED_POSES);
        let original = recovery.trusted.clone();
        for index in 0..3000 {
            recovery.observe(
                2000.0,
                0.1,
                DQuat::from_euler(
                    glam::EulerRot::XYZ,
                    (index as f64 * 0.137).sin() * 0.8 + 0.8,
                    (index as f64 * 0.193).cos() * 0.8 + 0.8,
                    (index as f64 * 0.317).sin() * 0.4 + 0.5,
                ),
            );
        }
        assert_eq!(recovery.trusted.len(), MAX_TRUSTED_POSES);
        assert_ne!(
            recovery.trusted, original,
            "fixture must exercise replacement"
        );
        assert!(
            recovery
                .trusted
                .iter()
                .any(|pose| pose.dot(old).abs() > 1.0 - 1e-12)
        );
        for index in 0..MAX_TRUSTED_POSES {
            assert_eq!(
                recovery.trusted_nearest[index].1,
                recovery.nearest_to(index).1
            );
        }
    }
    #[test]
    fn objective_budget_cannot_overrun_its_configured_limit() {
        for limit in [0, 1, 8, 256, 768] {
            let mut budget = Budget::new(limit);
            for _ in 0..limit {
                assert!(budget.take());
            }
            assert!(!budget.take());
            assert!(!budget.take());
            assert_eq!(budget.remaining, 0);
        }
    }
}

#[cfg(test)]
mod motion_tests {
    use super::*;
    #[test]
    fn accepted_acceleration_ramp_needs_four_consistent_good_images() {
        let mut recovery = Recovery::default();
        for _ in 0..32 {
            recovery.observe_motion(0.005, 10.0);
        }
        let candidate = DVec3::X * 0.10;
        assert!(!recovery.motion_ok(candidate.length(), 10.0));
        for angle in [0.025, 0.050] {
            recovery.observe_tracked_motion(DVec3::X * angle, 10.0);
            assert!(!recovery.accepted_ramp_ok(candidate, 10.0));
        }
        recovery.observe_tracked_motion(DVec3::X * 0.075, 10.0);
        assert!(recovery.accepted_ramp_ok(candidate, 10.0));
        assert!(!recovery.accepted_ramp_ok(DVec3::Y * 0.10, 10.0));
        assert!(!recovery.accepted_ramp_ok(DVec3::X * 0.5, 10.0));
        assert!(!recovery.accepted_ramp_ok(DVec3::X * 0.01, 10.0));
        recovery.begin_ramp_epoch();
        assert!(recovery.motion_ok(candidate.length(), 10.0));
        recovery.reject();
        assert!(!recovery.accepted_ramp_ok(candidate, 10.0));

        // A lone textured jump after constant motion has no ramp evidence.
        let mut steady = Recovery::default();
        for _ in 0..32 {
            steady.observe_tracked_motion(DVec3::X * 0.005, 10.0);
        }
        assert!(!steady.accepted_ramp_ok(candidate, 10.0));
        assert!(!steady.motion_ok(candidate.length(), 10.0));
    }

    #[test]
    fn held_motion_only_predicts_search_and_does_not_count_as_confirmation() {
        let mut recovery = Recovery::default();
        for _ in 0..32 {
            recovery.observe_motion(0.005, 10.0);
        }
        let motion = DVec3::new(0.2, 0.0, 0.0);
        let pose = DQuat::from_scaled_axis(motion);
        let history = recovery.motion_rates.values;
        recovery.reject_motion_candidate(pose, motion, 10.0);
        assert_eq!(recovery.pending_motion, Some((motion / 10.0, 0)));
        assert_eq!(recovery.motion_rates.values, history);
        assert!(!recovery.motion_ok(motion.length(), 10.0));
        let predicted = recovery.predicted_pending_pose(pose, 0.35, 10.0);
        assert!(predicted.is_some_and(|next| {
            (next * DQuat::from_scaled_axis(motion * 2.0).inverse())
                .to_scaled_axis()
                .length()
                < 1e-12
        }));
        assert!(
            recovery
                .predicted_pending_pose(pose, 0.35, 1000.0)
                .is_none()
        );
        assert!(
            recovery
                .predicted_pending_pose(pose, 0.35, f64::NAN)
                .is_none()
        );
        recovery.reject();
        assert!(recovery.pending.is_none());
        assert!(recovery.pending_motion.is_none());
        assert_eq!(recovery.motion_rates.values, history);
    }

    #[test]
    fn robust_motion_gate_handles_bootstrap_acceleration_clock_changes_and_outliers() {
        let mut recovery = Recovery::default();
        assert!(
            recovery.motion_ok(0.15, 10.0),
            "new tracker has no motion evidence"
        );
        for _ in 0..32 {
            recovery.observe_motion(0.006, 10.0);
        }
        assert!(
            recovery.motion_ok(0.03, 10.0),
            "acceleration below the physical allowance"
        );
        assert!(!recovery.motion_ok(0.07, 10.0));
        assert!(
            recovery.motion_ok(0.024, 40.0),
            "same velocity at slower source rate"
        );
        assert!(
            recovery.motion_ok(0.006, 2.5),
            "floor does not invent a speed restriction after metadata FPS changes"
        );
        // A few accepted marginal history samples cannot shift the median/MAD.
        for _ in 0..4 {
            recovery.observe_motion(0.2, 10.0);
        }
        assert!(!recovery.motion_ok(0.07, 10.0));
        let history = recovery.motion_rates.values;
        recovery.reject();
        assert_eq!(recovery.motion_rates.values, history);
        assert!(!recovery.motion_ok(f64::NAN, 10.0));
        assert!(!recovery.motion_ok(0.01, 0.0));
    }
    #[test]
    fn recovered_pose_must_earn_learning_and_error_history_cannot_loosen_old_gate() {
        let mut recovery = Recovery::default();
        let settings = MapSettings::default();
        for _ in 0..32 {
            recovery.observe(2000.0, 0.1, DQuat::IDENTITY);
        }
        let old = recovery.limit(&settings);
        for _ in 0..4 {
            recovery.observe(17000.0, 0.1, DQuat::IDENTITY);
        }
        assert!(recovery.limit(&settings) <= old);
        assert!(!recovery.allow_learning("relocalized"));
        for _ in 0..3 {
            assert!(!recovery.allow_learning("tracked"));
        }
        assert!(recovery.allow_learning("tracked"));
    }
}
