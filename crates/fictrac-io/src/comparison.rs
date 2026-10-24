//! Sequence-aligned comparison with a FicTrac reference; clocks/error scores are kept separate.
use crate::{Error, Result};
use serde::Serialize;
use std::io::BufRead;
#[derive(Debug, Serialize)]
pub struct Comparison {
    pub frames: usize,
    pub rotation_component_rmse_rad: [f64; 3],
    pub path_point_rmse_sphere_rad: f64,
    pub heading_rmse_rad: f64,
    pub endpoint_error_sphere_rad: f64,
    pub reference_final_position: [f64; 2],
    pub candidate_final_position: [f64; 2],
    pub timestamp_max_error_ms: f64,
}
impl Comparison {
    pub fn passes(&self, path_limit: f64, heading_limit: f64, rotation_limit: f64) -> bool {
        self.path_point_rmse_sphere_rad <= path_limit
            && self.heading_rmse_rad <= heading_limit
            && self
                .rotation_component_rmse_rad
                .iter()
                .all(|v| *v <= rotation_limit)
    }
}
fn rows(input: impl BufRead) -> Result<Vec<[f64; 25]>> {
    let mut rows = vec![];
    for line in input.lines().take(1_000_001) {
        if rows.len() >= 1_000_000 {
            return Err(Error::Config(
                "comparison is limited to one million frames".into(),
            ));
        }
        let line = line?;
        if line.len() > 8192 {
            return Err(Error::Config("data row exceeds 8192 bytes".into()));
        }
        let values: Vec<f64> = line
            .split(',')
            .map(|s| {
                s.trim()
                    .parse::<f64>()
                    .map_err(|_| Error::Config("non-numeric data column".into()))
            })
            .collect::<Result<_>>()?;
        let row: [f64; 25] = values
            .try_into()
            .map_err(|_| Error::Config("expected exactly 25 data columns".into()))?;
        if row.iter().any(|v| !v.is_finite()) {
            return Err(Error::Config("nonfinite data value".into()));
        }
        rows.push(row);
    }
    if rows.len() < 2 {
        return Err(Error::Config(
            "comparison requires at least two frames".into(),
        ));
    }
    for (i, row) in rows.iter().enumerate() {
        if (row[0] - rows[0][0] - i as f64).abs() > 1e-6 {
            return Err(Error::Config(
                "frame counters have gaps; align corresponding frames before comparison".into(),
            ));
        }
    }
    Ok(rows)
}
pub fn compare_dat(reference: impl BufRead, candidate: impl BufRead) -> Result<Comparison> {
    let reference = rows(reference)?;
    let candidate = rows(candidate)?;
    if reference.len() != candidate.len() {
        return Err(Error::Config(format!(
            "frame count mismatch: reference {}, candidate {}",
            reference.len(),
            candidate.len()
        )));
    }
    let mut rotation = [0.0; 3];
    let mut path = 0.0;
    let mut heading = 0.0;
    let mut timestamp_max: f64 = 0.0;
    for (r, c) in reference.iter().zip(&candidate).skip(1) {
        for axis in 0..3 {
            rotation[axis] += (r[axis + 1] - c[axis + 1]).powi(2);
        }
        path += (r[14] - c[14]).powi(2) + (r[15] - c[15]).powi(2);
        let d = r[16] - c[16];
        heading += d.sin().atan2(d.cos()).powi(2);
        timestamp_max = timestamp_max.max((r[21] - c[21]).abs());
    }
    // Exclude upstream's anomalous initial clock row, but require aligned displayed-video times.
    if timestamp_max > 1.0 {
        return Err(Error::Config(format!(
            "video timestamps are not aligned (max difference {timestamp_max} ms)"
        )));
    }
    let r = reference
        .last()
        .ok_or_else(|| Error::Config("empty reference".into()))?;
    let c = candidate
        .last()
        .ok_or_else(|| Error::Config("empty candidate".into()))?;
    let count = (reference.len() - 1) as f64;
    Ok(Comparison {
        frames: reference.len(),
        rotation_component_rmse_rad: rotation.map(|v| (v / count).sqrt()),
        path_point_rmse_sphere_rad: (path / count).sqrt(),
        heading_rmse_rad: (heading / count).sqrt(),
        endpoint_error_sphere_rad: (r[14] - c[14]).hypot(r[15] - c[15]),
        reference_final_position: [r[14], r[15]],
        candidate_final_position: [c[14], c[15]],
        timestamp_max_error_ms: timestamp_max,
    })
}
