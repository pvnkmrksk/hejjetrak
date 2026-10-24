//! Local hardware diagnostic; never opens a shell or stores camera pixels.
use std::{process::ExitCode, time::Instant};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let fps = std::env::args()
        .nth(1)
        .map(|s| s.parse::<f64>())
        .transpose()?
        .unwrap_or(30.0);
    for camera in fictrac_camera::list_cameras()? {
        println!("{}: {}", camera.id, camera.name);
    }
    fictrac_camera::request_permission()?;
    let mut camera = fictrac_camera::Camera::open(0, fps)?;
    let started = Instant::now();
    let mut frames = 0;
    let mut first = None;
    let mut last = None;
    while frames < 30 && started.elapsed().as_secs() < 10 {
        if let Some(frame) = camera.next_frame()? {
            frames += 1;
            if first.is_none() {
                println!(
                    "actual={}x{} first_pts_ms={}",
                    frame.gray.width(),
                    frame.gray.height(),
                    frame.gray.timestamp_ms()
                );
                first = Some(frame.gray.timestamp_ms());
            }
            last = Some(frame.gray.timestamp_ms());
        }
    }
    if frames != 30 {
        return Err(format!("only {frames} matching frames in ten seconds").into());
    }
    println!(
        "frames={frames} wall_seconds={:.6} pts_span_ms={:.6}",
        started.elapsed().as_secs_f64(),
        last.unwrap_or(0.0) - first.unwrap_or(0.0)
    );
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("camera probe: {e}");
            ExitCode::FAILURE
        }
    }
}
