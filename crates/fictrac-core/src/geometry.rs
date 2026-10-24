use crate::{Config, Error, Result, Sphere};
use glam::{DMat3, DVec3};

#[derive(Clone, Debug)]
pub struct Geometry {
    pub(crate) config: Config,
    focal: f64,
    centre: DVec3,
    radius: f64,
}
impl Geometry {
    pub fn new(config: Config) -> Result<Self> {
        config.validate()?;
        let focal = if config.fisheye {
            config.height as f64 / config.vfov_deg.to_radians()
        } else {
            config.height as f64 * 0.5 / (config.vfov_deg.to_radians() * 0.5).tan()
        };
        let centre = DVec3::from_array(config.sphere.centre);
        let radius = config.sphere.angular_radius.sin();
        Ok(Self {
            config,
            focal,
            centre,
            radius,
        })
    }
    pub(crate) fn ray(&self, x: f64, y: f64) -> DVec3 {
        let dx = x - self.config.width as f64 * 0.5;
        let dy = y - self.config.height as f64 * 0.5;
        if self.config.fisheye {
            let r = dx.hypot(dy);
            let angle = r / self.focal;
            let factor = if r > 1e-12 {
                angle.sin() / r
            } else {
                1.0 / self.focal
            };
            DVec3::new(dx * factor, dy * factor, angle.cos())
        } else {
            DVec3::new(dx, dy, self.focal).normalize()
        }
    }
    /// Unit sphere surface point in camera axes, relative to ball centre.
    pub fn surface(&self, x: f64, y: f64) -> Option<DVec3> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= self.config.width as f64
            || y >= self.config.height as f64
        {
            return None;
        }
        let ray = self.ray(x, y);
        let dot = ray.dot(self.centre);
        let disc = dot * dot + self.radius * self.radius - 1.0;
        if disc <= 0.0 {
            return None;
        }
        let t = dot - disc.sqrt();
        Some((ray * t - self.centre) / self.radius)
    }
    pub fn project(&self, p: DVec3) -> Option<[f64; 2]> {
        let v = self.centre + p * self.radius;
        if v.z <= 0.0 || !v.is_finite() || p.dot(v) >= -0.08 {
            return None;
        }
        self.project_ray(v)
    }
    pub fn project_ray(&self, v: DVec3) -> Option<[f64; 2]> {
        if !v.is_finite() || v.z <= 0.0 {
            return None;
        }
        let factor = if self.config.fisheye {
            let r = v.x.hypot(v.y);
            if r > 1e-12 {
                self.focal * r.atan2(v.z) / r
            } else {
                self.focal / v.z
            }
        } else {
            self.focal / v.z
        };
        Some([
            v.x * factor + self.config.width as f64 * 0.5,
            v.y * factor + self.config.height as f64 * 0.5,
        ])
    }
    pub fn usable(&self, x: f64, y: f64) -> bool {
        !self
            .config
            .ignore_polygons
            .iter()
            .any(|p| inside_polygon(x, y, p))
    }
    /// Fit a small circle to calibrated camera rays. At least three rim points.
    pub fn fit_sphere(
        width: usize,
        height: usize,
        vfov_deg: f64,
        points: &[[f64; 2]],
    ) -> Result<Sphere> {
        Self::fit_sphere_model(width, height, vfov_deg, points, false)
    }
    pub fn fit_sphere_model(
        width: usize,
        height: usize,
        vfov_deg: f64,
        points: &[[f64; 2]],
        fisheye: bool,
    ) -> Result<Sphere> {
        let base = Self::new(Config {
            fisheye,
            width,
            height,
            vfov_deg,
            ..Config::default()
        })?;
        if !(3..=4096).contains(&points.len())
            || points.iter().any(|p| {
                !p[0].is_finite()
                    || !p[1].is_finite()
                    || p[0] < 0.0
                    || p[1] < 0.0
                    || p[0] >= width as f64
                    || p[1] >= height as f64
            })
        {
            return Err(Error::Config(
                "provide 3..4096 finite rim points inside the image".into(),
            ));
        }
        let mut a = DMat3::ZERO;
        let mut b = DVec3::ZERO;
        for p in points {
            let r = base.ray(p[0], p[1]);
            a += outer(r, r);
            b += r;
        }
        let axis = solve(a, b)
            .ok_or_else(|| Error::Config("rim points are collinear or degenerate".into()))?;
        if axis.length() <= 1.0 {
            return Err(Error::Config("rim points do not define a sphere".into()));
        }
        let sphere = Sphere {
            centre: axis.normalize().to_array(),
            angular_radius: (1.0 / axis.length()).acos(),
        };
        Config {
            width,
            height,
            vfov_deg,
            sphere: sphere.clone(),
            ..Config::default()
        }
        .validate()?;
        Ok(sphere)
    }
}
pub(crate) fn outer(a: DVec3, b: DVec3) -> DMat3 {
    DMat3::from_cols(a * b.x, a * b.y, a * b.z)
}
pub(crate) fn solve(a: DMat3, b: DVec3) -> Option<DVec3> {
    let det = a.determinant();
    if !det.is_finite()
        || det.abs() <= 1e-12 * a.x_axis.length() * a.y_axis.length() * a.z_axis.length()
    {
        return None;
    }
    let x = a.inverse() * b;
    x.is_finite().then_some(x)
}
fn inside_polygon(x: f64, y: f64, p: &[[f64; 2]]) -> bool {
    let Some(mut prev) = p.last() else {
        return false;
    };
    let mut inside = false;
    for cur in p {
        if (cur[1] > y) != (prev[1] > y)
            && x < (prev[0] - cur[0]) * (y - cur[1]) / (prev[1] - cur[1]) + cur[0]
        {
            inside = !inside;
        }
        prev = cur;
    }
    inside
}
