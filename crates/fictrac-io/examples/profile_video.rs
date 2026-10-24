//! Development profiler driver. No decoding subprocess or timing in the engine.
use fictrac_io::VideoSource;
use std::{hint::black_box, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(
        args.next()
            .ok_or("usage: profile_video INPUT [luma|rgb|tracking] [loops]")?,
    );
    let mode = args.next().unwrap_or_else(|| "luma".into());
    if !matches!(mode.as_str(), "luma" | "rgb" | "tracking") {
        return Err("mode must be luma, rgb or tracking".into());
    }
    let loops: usize = args.next().unwrap_or_else(|| "10".into()).parse()?;
    if !(1..=100).contains(&loops) || args.next().is_some() {
        return Err("loops must be 1..100; too many arguments are not supported".into());
    }
    let start = Instant::now();
    let mut frames = 0;
    let mut rgb_bytes = 0;
    for _ in 0..loops {
        let mut source = VideoSource::open(&input)?;
        if mode != "luma" {
            source.enable_color();
        }
        loop {
            let frame = if mode == "tracking" {
                source.next_tracking_frame(None)?
            } else {
                source.next_frame()?
            };
            let Some(frame) = frame else {
                break;
            };
            black_box(frame.pixels());
            if let Some(rgb) = source.take_rgb() {
                rgb_bytes += black_box(&rgb).len();
            }
            frames += 1;
        }
    }
    let seconds = start.elapsed().as_secs_f64();
    println!(
        "{}",
        serde_json::json!({"input":input,"mode":mode,"loops":loops,"frames":frames,"rgb_bytes":rgb_bytes,"seconds":seconds,"fps":frames as f64 / seconds})
    );
    Ok(())
}
