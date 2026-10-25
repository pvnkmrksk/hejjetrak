//! Bounded, confirmed reacquisition; uncertain frames never teach the atlas.
use crate::MapSettings;
use crate::spherical::AtlasObjective;
use bobyqa::{Bobyqa, Config as SolverConfig};
use glam::{DQuat, DVec3};
use std::f64::consts::TAU;

const MAX_TRUSTED_POSES: usize = 256;
const POSE_SEPARATION: f64 = 0.08;
const MAX_RECOVERY_EVALUATIONS: usize = 768;

#[derive(Default)]
pub(crate) struct Recovery {
    pub lost_frames: u32,
    pending: Option<DQuat>,
    attempts: u64,
    error_mean: f64,
    edge_mean: f64,
    // Unit ROI poses learned only from accepted frames. This small spatial bank
    // revisits known views instead of expecting a coarse uniform SO(3) cover to
    // land near an earlier appearance after a long occlusion.
    trusted: Vec<DQuat>,
    next_trusted: usize,
}
impl Recovery {
    pub fn reject(&mut self) {
        self.lost_frames = self.lost_frames.saturating_add(1);
        self.pending = None;
    }
    pub fn limit(&self, settings: &MapSettings) -> f64 {
        let adaptive = if self.error_mean > 0.0 {
            (self.error_mean * 1.8 + 2000.0).clamp(10000.0, 18000.0)
        } else {
            18000.0
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
    pub fn observe(&mut self, error: f64, edge: f64, pose: DQuat) {
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
        let near = (POSE_SEPARATION * 0.5).cos();
        if self
            .trusted
            .iter()
            .any(|saved| saved.dot(pose).abs() >= near)
        {
            return;
        }
        if self.trusted.len() < MAX_TRUSTED_POSES {
            self.trusted.push(pose);
        } else {
            self.trusted[self.next_trusted] = pose;
            self.next_trusted = (self.next_trusted + 1) % MAX_TRUSTED_POSES;
        }
    }
    /// Returns an absolute ROI pose only after two consecutive matching frames.
    /// No relative motion across the rejected interval can be inferred here.
    pub fn reacquire(
        &mut self,
        objective: &AtlasObjective<'_>,
        previous: DQuat,
        bound: f64,
        settings: &MapSettings,
    ) -> (Option<DQuat>, f64) {
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
            let (pose, error) = refine(objective, start, bound, settings, &mut budget, 64);
            if error.is_finite() && error <= limit {
                if confirm {
                    self.pending = None;
                    self.lost_frames = 0;
                    return (Some(pose), error);
                }
                self.pending = Some(pose);
                return (None, error);
            }
        }
        self.pending = None;
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
            objective.coarse_cost(pose, 128)
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
                objective.local_cost(DQuat::from_scaled_axis(DVec3::new(x[0], x[1], x[2])) * start)
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
        let known = recovery.trusted.clone();
        recovery.reject();
        assert_eq!(recovery.trusted, known);
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
