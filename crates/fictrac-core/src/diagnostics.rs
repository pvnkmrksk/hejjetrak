//! FicTrac-shaped diagnostic views, produced from actual frames and accepted orientation.
use crate::{Config, Geometry, GrayFrame, Result, TrackRow};
use glam::{DQuat, DVec3};
use std::f64::consts::{PI, TAU};

// A cell must have a substantial accepted history before contradictory votes
// acquire extra inertia. At 100 FPS this requires at least ten seconds of
// actual visibility; newly learned surfaces keep upstream's consolidation.
const MATURE_TEACHING_FRAMES: u16 = 1024;
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
    learning_hits: Vec<u16>,
    last_learning_epoch: Vec<u32>,
    learning_epoch: u32,
    resilient_learning: bool,
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
        let resilient_learning = config.spherical_map.as_ref().is_some_and(|m| m.resilient);
        Ok(Self {
            config,
            basis,
            points,
            map: vec![128; map_width * map_height],
            learning_hits: vec![0; map_width * map_height],
            last_learning_epoch: vec![0; map_width * map_height],
            learning_epoch: 0,
            resilient_learning,
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
        self.learning_hits.fill(0);
        self.last_learning_epoch.fill(0);
        self.learning_epoch = 0;
    }
    fn begin_learning_frame(&mut self) {
        self.learning_epoch = if let Some(next) = self.learning_epoch.checked_add(1) {
            next
        } else {
            self.last_learning_epoch.fill(0);
            1
        };
    }
    fn learn_site(&mut self, index: usize, observation: u8) {
        let first_hit = self.learning_hits[index] == 0;
        let mut mature = false;
        if self.resilient_learning {
            if self.last_learning_epoch[index] != self.learning_epoch {
                self.learning_hits[index] = if self.learning_hits[index] == u16::MAX {
                    // Retain mature inertia while preserving its modulo-16
                    // update phase beyond 65535 observations. Saturating on an
                    // ineligible remainder would freeze this cell forever.
                    u16::MAX - 15
                } else {
                    self.learning_hits[index] + 1
                };
                self.last_learning_epoch[index] = self.learning_epoch;
            }
            // Count distinct teaching frames, not colliding ROI samples. New
            // surface retains its original first-hit bonus and early learning;
            // contradictory evidence against a long-observed consolidated
            // cell acquires sixteen times the inertia. Agreement consolidates fast;
            // near-neutral evidence remains free to resolve either way.
            let hits = self.learning_hits[index];
            let period = (1 + hits.saturating_sub(1) / 64).min(16);
            mature = hits >= MATURE_TEACHING_FRAMES;
            let contrary = (self.map[index] >= 160 && observation == 0)
                || (self.map[index] <= 96 && observation == 255);
            if mature && !first_hit && contrary && !hits.is_multiple_of(period) {
                return;
            }
        }
        self.map[index] = match self.map[index] {
            0 => 0,
            255 => 255,
            128 if !mature || first_hit => {
                if observation == 255 {
                    192
                } else {
                    64
                }
            }
            value => {
                if observation == 255 {
                    value.saturating_add(1)
                } else {
                    value.saturating_sub(1)
                }
            }
        };
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
                if x > 0 && x + 1 < n && y > 0 && y + 1 < n {
                    blur[y * n + x] = median9([
                        grey[(y - 1) * n + x - 1],
                        grey[(y - 1) * n + x],
                        grey[(y - 1) * n + x + 1],
                        grey[y * n + x - 1],
                        grey[y * n + x],
                        grey[y * n + x + 1],
                        grey[(y + 1) * n + x - 1],
                        grey[(y + 1) * n + x],
                        grey[(y + 1) * n + x + 1],
                    ]);
                    continue;
                }
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
        let mut horizontal = vec![Extrema::EMPTY; n * n];
        let mut prefix = vec![Extrema::EMPTY; n + 2 * rad];
        let mut suffix = prefix.clone();
        for y in 0..n {
            extrema_windows(
                n,
                rad,
                &mut prefix,
                &mut suffix,
                |x| {
                    let i = y * n + x;
                    if self.points[i].is_none() {
                        Extrema::EMPTY
                    } else {
                        Extrema {
                            min: blur[i],
                            max: if blur[i] < 255 { blur[i] } else { 0 },
                        }
                    }
                },
                |x, value| horizontal[y * n + x] = value,
            );
        }
        let mut roi = vec![128; n * n];
        for x in 0..n {
            extrema_windows(
                n,
                rad,
                &mut prefix,
                &mut suffix,
                |y| horizontal[y * n + x],
                |y, bounds| {
                    let i = y * n + x;
                    if self.points[i].is_none() {
                        return;
                    }
                    let value = if self.ratio * (f64::from(grey[i]) - f64::from(bounds.min))
                        <= f64::from(bounds.max) - f64::from(grey[i])
                    {
                        0
                    } else {
                        255
                    };
                    roi[i] = value;
                },
            );
        }
        roi
    }
    pub fn update(&mut self, frame: &GrayFrame, row: Option<&TrackRow>) -> DebugViews {
        let roi = self.prepare(frame);
        self.update_prepared(frame, row, roi, true)
    }
    /// The atlas estimator and its committed map must use the same thresholded
    /// observation. Reuse it rather than sampling/filtering the image twice.
    pub(crate) fn update_prepared(
        &mut self,
        frame: &GrayFrame,
        row: Option<&TrackRow>,
        roi: Vec<u8>,
        allow_learning: bool,
    ) -> DebugViews {
        let n = self.size;
        let mw = self.map_width;
        let mh = self.map_height;
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
        let teach = allow_learning && row.is_none_or(|r| r.accepted);
        if teach && self.resilient_learning {
            self.begin_learning_frame();
        }
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
                if teach {
                    self.learn_site(mi, value);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Extrema {
    min: u8,
    max: u8,
}
impl Extrema {
    const EMPTY: Self = Self { min: 255, max: 0 };
    fn join(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}
/// Prefix/suffix blocks give each clipped window in constant work, independent
/// of its radius. Padding with the identity preserves masked-border semantics.
fn extrema_windows(
    n: usize,
    rad: usize,
    prefix: &mut [Extrema],
    suffix: &mut [Extrema],
    value: impl Fn(usize) -> Extrema,
    mut emit: impl FnMut(usize, Extrema),
) {
    let extent = n + 2 * rad;
    let window = 2 * rad + 1;
    let sample = |i: usize| {
        if i >= rad && i - rad < n {
            value(i - rad)
        } else {
            Extrema::EMPTY
        }
    };
    for i in 0..extent {
        prefix[i] = if i % window == 0 {
            sample(i)
        } else {
            prefix[i - 1].join(sample(i))
        };
    }
    for i in (0..extent).rev() {
        suffix[i] = if i + 1 == extent || (i + 1) % window == 0 {
            sample(i)
        } else {
            suffix[i + 1].join(sample(i))
        };
    }
    for i in 0..n {
        emit(i, suffix[i].join(prefix[i + 2 * rad]));
    }
}
/// Median selection network: only the fifth order statistic is needed.
/// The 512 binary inputs exhaust its threshold cases (zero-one principle).
fn median9(mut p: [u8; 9]) -> u8 {
    for (a, b) in [
        (1, 2),
        (4, 5),
        (7, 8),
        (0, 1),
        (3, 4),
        (6, 7),
        (1, 2),
        (4, 5),
        (7, 8),
        (0, 3),
        (5, 8),
        (4, 7),
        (3, 6),
        (1, 4),
        (2, 5),
        (4, 7),
        (4, 2),
        (6, 4),
        (4, 2),
    ] {
        let lo = p[a].min(p[b]);
        let hi = p[a].max(p[b]);
        p[a] = lo;
        p[b] = hi;
    }
    p[4]
}

#[cfg(test)]
mod learning_tests {
    use super::*;
    use crate::{MapSettings, Tracker};

    fn diagnostics(resilient: bool) -> Result<Diagnostics> {
        Diagnostics::new(
            Config {
                spherical_map: Some(MapSettings {
                    resilient,
                    ..MapSettings::default()
                }),
                ..Config::default()
            },
            6,
            1.25,
            0.25,
        )
    }

    #[test]
    fn mature_unsaturated_map_resists_300_accepted_contrary_images() -> Result<()> {
        let mut protected = diagnostics(true)?;
        let mut legacy = diagnostics(false)?;
        // A long-observed boundary cell can remain unsaturated. Saturated
        // cells are already immutable in both policies; exercise the vulnerable
        // partially consolidated evidence instead of only rejected images.
        protected.map.fill(200);
        legacy.map.fill(200);
        protected.learning_hits.fill(1600);
        let base = GrayFrame::new(160, 160, vec![0.5; 160 * 160], 0.0)?;
        let mut row = Tracker::new(Config::default())?.process(base.clone())?;
        assert!(row.accepted);
        let roi = vec![0; protected.size * protected.size];
        let mut observed = Vec::new();
        for frame in 0..300 {
            row.frame = frame;
            row.timestamp_ms = frame as f64 * 10.0;
            let image = base.clone().with_timestamp(row.timestamp_ms)?;
            let views = protected.update_prepared(&image, Some(&row), roi.clone(), true);
            legacy.update_prepared(&image, Some(&row), roi.clone(), true);
            if frame == 299 {
                observed = views
                    .instant
                    .iter()
                    .enumerate()
                    .filter_map(|(index, value)| (*value != 128).then_some(index))
                    .collect();
            }
        }
        assert!(!observed.is_empty());
        assert!(observed.iter().all(|index| legacy.map[*index] == 0));
        let preserved = observed
            .iter()
            .filter(|index| protected.map[**index] > 128)
            .count();
        assert!(preserved * 100 >= observed.len() * 95);
        assert!(
            observed
                .iter()
                .all(|index| protected.learning_hits[*index] == 1900),
            "ROI collisions must not count as multiple teaching frames"
        );
        Ok(())
    }

    #[test]
    fn new_surface_and_legacy_updates_remain_fast_and_learning_history_resets() -> Result<()> {
        let mut protected = diagnostics(true)?;
        protected.begin_learning_frame();
        protected.learn_site(0, 255);
        assert_eq!(protected.map[0], 192);
        protected.learn_site(0, 255);
        assert_eq!(protected.map[0], 193);
        assert_eq!(protected.learning_hits[0], 1);
        protected.map[1] = 128;
        protected.learning_hits[1] = 10;
        protected.learn_site(1, 255);
        assert_eq!(
            protected.map[1], 192,
            "young evidence retains legacy consolidation"
        );
        protected.map[1] = 128;
        protected.learning_hits[1] = 1600;
        protected.begin_learning_frame();
        protected.learn_site(1, 255);
        assert_eq!(
            protected.map[1], 129,
            "mature neutral evidence is not unseen"
        );
        protected.map[2] = 0;
        protected.map[3] = 255;
        protected.learn_site(2, 255);
        protected.learn_site(3, 0);
        assert_eq!(&protected.map[2..4], &[0, 255]);
        protected.reset_history();
        assert_eq!(protected.learning_epoch, 0);
        assert!(protected.learning_hits.iter().all(|hits| *hits == 0));
        assert!(
            protected
                .last_learning_epoch
                .iter()
                .all(|epoch| *epoch == 0)
        );

        let mut legacy = diagnostics(false)?;
        legacy.map[0] = 128;
        legacy.learning_hits[0] = 1000;
        legacy.learn_site(0, 255);
        assert_eq!(legacy.map[0], 192);
        legacy.learn_site(0, 0);
        assert_eq!(legacy.map[0], 191);
        legacy.map[1] = 128;
        legacy.learning_hits[1] = 1000;
        legacy.learn_site(1, 0);
        assert_eq!(legacy.map[1], 64);
        Ok(())
    }

    #[test]
    fn young_boundary_cells_retain_legacy_learning_before_later_maturity() -> Result<()> {
        let mut protected = diagnostics(true)?;
        let mut legacy = diagnostics(false)?;
        for age in [1, 10, 63, 64, 512, MATURE_TEACHING_FRAMES - 2] {
            protected.map[0] = 200;
            legacy.map[0] = 200;
            protected.learning_hits[0] = age;
            protected.begin_learning_frame();
            protected.learn_site(0, 0);
            legacy.learn_site(0, 0);
            assert_eq!(protected.map[0], legacy.map[0], "young boundary age {age}");
        }
        protected.map[0] = 200;
        protected.learning_hits[0] = MATURE_TEACHING_FRAMES;
        protected.begin_learning_frame();
        protected.learn_site(0, 0);
        assert_eq!(
            protected.map[0], 200,
            "mature contrary vote waits for its phase"
        );
        Ok(())
    }

    #[test]
    fn mature_counter_wrap_keeps_periodic_learning_alive() -> Result<()> {
        let mut protected = diagnostics(true)?;
        protected.map[0] = 200;
        protected.learning_hits[0] = u16::MAX - 1;
        for _ in 0..34 {
            protected.begin_learning_frame();
            protected.learn_site(0, 0);
        }
        assert_eq!(protected.map[0], 197);
        assert!(protected.learning_hits[0] >= u16::MAX - 15);
        Ok(())
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    #[test]
    fn median_network_exhausts_binary_threshold_cases() {
        for bits in 0..512 {
            let values = std::array::from_fn(|i| if bits & (1 << i) != 0 { 255 } else { 0 });
            let mut sorted = values;
            sorted.sort_unstable();
            assert_eq!(median9(values), sorted[4]);
        }
    }
    #[test]
    fn blocked_extrema_match_scalar_windows_with_masks_and_saturation() {
        for n in [1, 2, 3, 15, 60, 300] {
            let values: Vec<_> = (0..n)
                .map(|i| {
                    if i % 7 == 0 {
                        Extrema::EMPTY
                    } else {
                        let v = (i * 73 % 256) as u8;
                        Extrema {
                            min: v,
                            max: if v < 255 { v } else { 0 },
                        }
                    }
                })
                .collect();
            for rad in [0, 1, n / 2, n] {
                let mut prefix = vec![Extrema::EMPTY; n + 2 * rad];
                let mut suffix = prefix.clone();
                let mut result = vec![Extrema::EMPTY; n];
                extrema_windows(
                    n,
                    rad,
                    &mut prefix,
                    &mut suffix,
                    |i| values[i],
                    |i, v| result[i] = v,
                );
                for (i, v) in result.iter().enumerate() {
                    let expected = values[i.saturating_sub(rad)..=(i + rad).min(n - 1)]
                        .iter()
                        .copied()
                        .fold(Extrema::EMPTY, Extrema::join);
                    assert_eq!(*v, expected, "n={n} radius={rad} at {i}");
                }
            }
        }
    }
}
