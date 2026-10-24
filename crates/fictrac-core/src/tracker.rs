use crate::geometry::{outer, solve};
use crate::{Config, Error, Geometry, GrayFrame, Result};
use glam::{DMat3, DQuat, DVec3};
use serde::Serialize;
use std::f64::consts::TAU;

#[derive(Clone, Debug, Serialize)]
pub struct TrackRow {
    pub frame: u64,
    /// Consecutive epoch counter, reset independently of the source frame count.
    pub sequence: u64,
    pub delta_camera: [f64; 3],
    pub error: f64,
    pub delta_lab: [f64; 3],
    pub absolute_camera: [f64; 3],
    pub absolute_lab: [f64; 3],
    pub position: [f64; 2],
    pub heading: f64,
    pub direction: f64,
    pub speed: f64,
    pub integrated: [f64; 2],
    pub timestamp_ms: f64,
    pub delta_ms: f64,
    pub accepted: bool,
    pub status: &'static str,
    pub samples: usize,
}
#[derive(Clone)]
struct Site {
    p: DVec3,
    x: f64,
    y: f64,
}
struct Observation {
    p: DVec3,
    intensity: f64,
}

pub struct Tracker {
    geometry: Geometry,
    sites: Vec<Vec<Site>>,
    previous: Option<Vec<GrayFrame>>,
    last_time: Option<f64>,
    last_accepted_time: Option<f64>,
    count: u64,
    sequence: u64,
    bad_frames: u32,
    prediction: DVec3,
    orientation: DQuat,
    lab: DQuat,
    export_basis: DQuat,
    spherical: Option<crate::spherical::MapTracker>,
    position: [f64; 2],
    integrated: [f64; 2],
    heading: f64,
}
impl Tracker {
    pub fn new(config: Config) -> Result<Self> {
        let geometry = Geometry::new(config)?;
        let lab = DQuat::from_scaled_axis(DVec3::from_array(geometry.config.camera_to_lab));
        let export_basis = if geometry.config.legacy_roi_coordinates {
            DQuat::from_rotation_arc(DVec3::from_array(geometry.config.sphere.centre), DVec3::Z)
        } else {
            DQuat::IDENTITY
        };
        let spherical = geometry
            .config
            .spherical_map
            .clone()
            .map(|settings| crate::spherical::MapTracker::new(&geometry.config, settings))
            .transpose()?;
        let mut sites = vec![];
        for level in 0..3 {
            let scale = (1 << level) as f64;
            let mut points = vec![];
            for y in (1..(geometry.config.height >> level).saturating_sub(1))
                .step_by(geometry.config.sample_stride)
            {
                for x in (1..(geometry.config.width >> level).saturating_sub(1))
                    .step_by(geometry.config.sample_stride)
                {
                    let ox = (x as f64 + 0.5) * scale - 0.5;
                    let oy = (y as f64 + 0.5) * scale - 0.5;
                    if !geometry.usable(ox, oy) {
                        continue;
                    }
                    if let Some(p) = geometry.surface(ox, oy)
                        && p.dot(geometry.ray(ox, oy)) < -0.35
                    {
                        if points.len() >= 250_000 {
                            return Err(Error::Config(
                                "ROI exceeds 250000 samples; increase sample_stride".into(),
                            ));
                        }
                        points.push(Site {
                            p,
                            x: x as f64,
                            y: y as f64,
                        });
                    }
                }
            }
            sites.push(points);
        }
        if sites[0].len() < 32 {
            return Err(Error::Config(
                "visible sphere ROI has fewer than 32 samples".into(),
            ));
        }
        Ok(Self {
            geometry,
            sites,
            previous: None,
            last_time: None,
            last_accepted_time: None,
            count: 0,
            sequence: 0,
            bad_frames: 0,
            prediction: DVec3::ZERO,
            orientation: DQuat::IDENTITY,
            lab,
            export_basis,
            spherical,
            position: [0.0; 2],
            integrated: [0.0; 2],
            heading: 0.0,
        })
    }
    /// Reset pose, path and heading while retaining source count/time and integrated displacement.
    /// With global search enabled the atlas is retained for relocalisation, as in FicTrac.
    pub fn reset_tracking(&mut self) {
        self.prediction = DVec3::ZERO;
        self.orientation = DQuat::IDENTITY;
        self.position = [0.0; 2];
        self.heading = 0.0;
        self.previous = None;
        self.sequence = 0;
        self.bad_frames = 0;
        if let Some(map) = &mut self.spherical {
            map.reset();
        }
    }
    /// Import a validated ROI-coordinate equal-area map before fitting the next frame.
    pub fn load_sphere_template(&mut self, template: crate::SphereTemplate) -> Result<()> {
        let Some(map) = &mut self.spherical else {
            return Err(Error::Config(
                "sphere templates require the spherical-map estimator".into(),
            ));
        };
        map.load_template(template)?;
        self.prediction = DVec3::ZERO;
        self.orientation = DQuat::IDENTITY;
        self.position = [0.0; 2];
        self.heading = 0.0;
        self.previous = None;
        self.sequence = 0;
        self.bad_frames = 0;
        Ok(())
    }
    pub fn sphere_template(&self) -> Option<crate::SphereTemplate> {
        self.spherical.as_ref().map(|map| map.template())
    }
    pub fn debug_views(&self) -> Option<&crate::DebugViews> {
        self.spherical.as_ref().and_then(|m| m.views.as_ref())
    }
    pub fn process(&mut self, frame: GrayFrame) -> Result<TrackRow> {
        if frame.width() != self.geometry.config.width
            || frame.height() != self.geometry.config.height
        {
            return Err(Error::Frame(
                "frame dimensions differ from calibration".into(),
            ));
        }
        if self.last_time.is_some_and(|t| frame.timestamp_ms() <= t) {
            return Err(Error::Frame("timestamps must strictly increase".into()));
        }
        let ts = frame.timestamp_ms();
        let mut pyramid = vec![frame];
        for level in 0..2 {
            pyramid.push(pyramid[level].half());
        }
        let mut rotation = DVec3::ZERO;
        let mut error = 0.0;
        let mut accepted = true;
        let mut status = "initializing";
        let mut samples = self.sites[0].len();
        if let Some(map) = &mut self.spherical {
            let estimate = map.estimate(&pyramid[0]);
            rotation = estimate.rotation;
            error = estimate.error;
            accepted = estimate.accepted;
            samples = estimate.samples;
            status = estimate.status;
            if let Some(absolute) = estimate.absolute {
                self.orientation = absolute;
            }
        } else if let Some(previous) = &self.previous {
            rotation = self.prediction;
            let mut constrained = false;
            for level in (0..3).rev() {
                if self.sites[level].len() < 24 {
                    continue;
                }
                let observations: Vec<_> = self.sites[level]
                    .iter()
                    .filter_map(|s| {
                        Some(Observation {
                            p: s.p,
                            intensity: pyramid[level].sample(s.x, s.y)?,
                        })
                    })
                    .collect();
                let mean = observations.iter().map(|s| s.intensity).sum::<f64>()
                    / observations.len().max(1) as f64;
                let variance = observations
                    .iter()
                    .map(|s| (s.intensity - mean).powi(2))
                    .sum::<f64>()
                    / observations.len().max(1) as f64;
                if variance < 0.0002 {
                    continue;
                }
                let scale = (1 << level) as f64;
                for _ in 0..self.geometry.config.iterations {
                    let q = DQuat::from_scaled_axis(-rotation);
                    let eps = 1e-4;
                    let perturb = [DVec3::X, DVec3::Y, DVec3::Z]
                        .map(|v| DQuat::from_scaled_axis(-(rotation + eps * v)));
                    let mut h = DMat3::ZERO;
                    let mut b = DVec3::ZERO;
                    let mut used = 0;
                    for s in &observations {
                        let Some(value) = predict(&self.geometry, &previous[level], q, s.p, scale)
                        else {
                            continue;
                        };
                        let mut j = DVec3::ZERO;
                        let mut valid = true;
                        for axis in 0..3 {
                            if let Some(v) =
                                predict(&self.geometry, &previous[level], perturb[axis], s.p, scale)
                            {
                                j[axis] = (v - value) / eps;
                            } else {
                                valid = false;
                                break;
                            }
                        }
                        if !valid {
                            continue;
                        }
                        let residual = s.intensity - value;
                        let weight = (0.06 / residual.abs().max(0.06)).min(1.0);
                        h += outer(j, j) * weight;
                        b += j * (weight * residual);
                        used += 1;
                    }
                    if used < 24 {
                        break;
                    }
                    // Reject unobservable motion (a flat image or a one-dimensional pattern).
                    let Some(mut step) = solve(h, b) else {
                        break;
                    };
                    constrained = true;
                    step = step.clamp_length_max(0.10);
                    let old = cost(
                        &self.geometry,
                        &previous[level],
                        &observations,
                        rotation,
                        scale,
                    )
                    .0;
                    let mut improved = false;
                    for _ in 0..6 {
                        let candidate =
                            (rotation + step).clamp_length_max(self.geometry.config.max_rotation);
                        if cost(
                            &self.geometry,
                            &previous[level],
                            &observations,
                            candidate,
                            scale,
                        )
                        .0 < old
                        {
                            rotation = candidate;
                            improved = true;
                            break;
                        }
                        step *= 0.5;
                    }
                    if !improved || step.length() < 1e-5 {
                        break;
                    }
                }
            }
            let observations: Vec<_> = self.sites[0]
                .iter()
                .filter_map(|s| {
                    Some(Observation {
                        p: s.p,
                        intensity: pyramid[0].sample(s.x, s.y)?,
                    })
                })
                .collect();
            let (_, rmse, n) = cost(&self.geometry, &previous[0], &observations, rotation, 1.0);
            error = rmse;
            samples = n;
            accepted = constrained
                && error <= self.geometry.config.max_error
                && n * 4 >= observations.len() * 3
                && rotation.length() < self.geometry.config.max_rotation * 0.999;
            status = if accepted { "tracked" } else { "rejected" };
            if !accepted {
                rotation = DVec3::ZERO;
            }
        }
        self.prediction = rotation;
        let exported_rotation = self.export_basis * rotation;
        let delta_lab = self.lab * exported_rotation;
        if accepted {
            self.orientation = (DQuat::from_scaled_axis(rotation) * self.orientation).normalize();
            let vx = delta_lab.y;
            let vy = -delta_lab.x;
            // Integrate a constant body velocity exactly along the heading arc.
            let dh = -delta_lab.z;
            let half = dh * 0.5;
            let sinc = if half.abs() < 1e-8 {
                1.0 - half * half / 6.0
            } else {
                half.sin() / half
            };
            let (s, c) = (self.heading + half).sin_cos();
            if self.geometry.config.legacy_roi_coordinates {
                // Trackball::updatePath uses four midpoint substeps per image.
                for substep in 0..4 {
                    let (s, c) = (self.heading + (substep as f64 + 0.5) * dh / 4.0).sin_cos();
                    self.position[0] += (c * vx - s * vy) / 4.0;
                    self.position[1] += (s * vx + c * vy) / 4.0;
                }
            } else {
                self.position[0] += sinc * (c * vx - s * vy);
                self.position[1] += sinc * (s * vx + c * vy);
            }
            self.integrated[0] += vx;
            self.integrated[1] += vy;
            self.heading = (self.heading + dh).rem_euclid(TAU);
        }
        self.count += 1;
        let exported_orientation =
            self.export_basis * self.orientation * self.export_basis.inverse();
        let row = TrackRow {
            sequence: self.sequence,
            frame: if self.geometry.config.legacy_roi_coordinates {
                self.count - 1
            } else {
                self.count
            },
            delta_camera: exported_rotation.to_array(),
            error,
            delta_lab: delta_lab.to_array(),
            absolute_camera: principal_axis(exported_orientation).to_array(),
            absolute_lab: principal_axis(self.lab * exported_orientation).to_array(),
            position: self.position,
            heading: self.heading,
            direction: (-delta_lab.x).atan2(delta_lab.y).rem_euclid(TAU),
            speed: delta_lab.x.hypot(delta_lab.y),
            integrated: self.integrated,
            timestamp_ms: ts,
            delta_ms: if self.geometry.config.legacy_roi_coordinates && accepted {
                // Legacy logData writes only good rows and advances its clock only then.
                self.last_accepted_time.map_or(0.0, |t| ts - t)
            } else {
                self.last_time.map_or(0.0, |t| ts - t)
            },
            accepted,
            status,
            samples,
        };
        if let Some(map) = &mut self.spherical {
            map.commit(
                &pyramid[0],
                &row,
                self.geometry.config.legacy_roi_coordinates,
            );
        }
        self.previous = Some(pyramid);
        self.last_time = Some(ts);
        if accepted {
            self.last_accepted_time = Some(ts);
        }
        self.sequence += 1;
        self.bad_frames = if accepted {
            0
        } else {
            self.bad_frames.saturating_add(1)
        };
        let reset_due = self
            .geometry
            .config
            .spherical_map
            .as_ref()
            .is_some_and(|m| m.max_bad_frames >= 0 && self.bad_frames > m.max_bad_frames as u32);
        if reset_due {
            self.reset_tracking();
        }
        Ok(row)
    }
}
fn predict(g: &Geometry, frame: &GrayFrame, q: DQuat, p: DVec3, scale: f64) -> Option<f64> {
    let [x, y] = g.project(q * p)?;
    if !g.usable(x, y) {
        return None;
    }
    frame.sample((x + 0.5) / scale - 0.5, (y + 0.5) / scale - 0.5)
}
fn cost(
    g: &Geometry,
    frame: &GrayFrame,
    obs: &[Observation],
    r: DVec3,
    scale: f64,
) -> (f64, f64, usize) {
    let q = DQuat::from_scaled_axis(-r);
    let mut robust = 0.0;
    let mut square = 0.0;
    let mut n = 0;
    for s in obs {
        if let Some(v) = predict(g, frame, q, s.p, scale) {
            let d = (s.intensity - v).abs();
            square += d * d;
            robust += if d <= 0.06 {
                0.5 * d * d
            } else {
                0.06 * (d - 0.03)
            };
            n += 1;
        } else {
            // Fixed support penalty prevents an apparent improvement by dropping samples.
            robust += 0.02;
        }
    }
    (
        robust / obs.len().max(1) as f64,
        (square / n.max(1) as f64).sqrt(),
        n,
    )
}

fn principal_axis(q: DQuat) -> DVec3 {
    (if q.w < 0.0 { -q } else { q }).to_scaled_axis()
}
