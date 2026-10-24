use crate::Result;
use fictrac_core::TrackRow;
use std::io::Write;
/// Original 25-column field ordering. Error metric and estimator differ; see docs/parity.md.
pub fn write_dat(out: &mut impl Write, r: &TrackRow) -> Result<()> {
    let mut values = Vec::with_capacity(25);
    values.push(r.frame as f64);
    values.extend(r.delta_camera);
    values.push(r.error);
    values.extend(r.delta_lab);
    values.extend(r.absolute_camera);
    values.extend(r.absolute_lab);
    values.extend(r.position);
    values.extend([r.heading, r.direction, r.speed]);
    values.extend(r.integrated);
    values.extend([
        r.timestamp_ms,
        r.sequence as f64,
        r.delta_ms,
        r.timestamp_ms,
    ]);
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            write!(out, ", ")?;
        }
        if i == 0 {
            write!(out, "{}", r.frame)?;
        } else if i == 22 {
            write!(out, "{}", r.sequence)?;
        } else {
            write!(out, "{v:.12}")?;
        }
    }
    writeln!(out)?;
    Ok(())
}
pub fn write_path_svg(out: &mut impl Write, points: &[[f64; 2]]) -> Result<()> {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for p in points {
        for i in 0..2 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    if points.is_empty() {
        min = [0.0; 2];
        max = [0.0; 2];
    }
    let span = (max[0] - min[0]).max(max[1] - min[1]).max(0.1);
    let mid = [(max[0] + min[0]) * 0.5, (max[1] + min[1]) * 0.5];
    writeln!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="800" viewBox="0 0 800 800"><rect width="800" height="800" fill="#101b27"/><text x="32" y="42" fill="#eef4fa" font-family="sans-serif" font-size="22">FicTrac Rust · fictive path</text><text x="32" y="72" fill="#99adbf" font-family="sans-serif" font-size="14">East → · North ↑ · coordinates in sphere radians</text><path fill="none" stroke="#55ddbf" stroke-width="2" d=""##
    )?;
    for (i, p) in points.iter().enumerate() {
        let x = 400.0 + (p[1] - mid[1]) * 640.0 / span;
        let y = 420.0 - (p[0] - mid[0]) * 640.0 / span;
        write!(out, "{} {x:.3} {y:.3} ", if i == 0 { "M" } else { "L" })?;
    }
    writeln!(
        out,
        r##""/><text x="32" y="770" fill="#99adbf" font-family="sans-serif" font-size="14">Scale by ball radius for physical distance. Inspect quality.csv before use.</text></svg>"##
    )?;
    Ok(())
}
