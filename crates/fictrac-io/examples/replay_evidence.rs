//! Development replay with complete rows and independent temporal fit evidence.
use fictrac_core::Tracker;
use fictrac_io::{VideoSource, load_config};
use std::{fs, io::Write, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(4..=6).contains(&args.len()) {
        return Err(
            "usage: replay_evidence VIDEO CONFIG MAX_FRAMES NEW_JSONL [SOURCE_DAT_CLOCK] [OMIT_PERIOD]".into(),
        );
    }
    let omit_period = args.get(5).map(|s| s.parse::<usize>()).transpose()?;
    if omit_period.is_some_and(|period| period < 4) {
        return Err("omit period must be at least four source images".into());
    }
    let maximum = args[2].parse::<usize>()?;
    let timestamps = if let Some(path) = args.get(4) {
        Some(
            fs::read_to_string(path)?
                .lines()
                .take(maximum)
                .map(|line| {
                    line.split(',')
                        .nth(21)
                        .ok_or("clock row lacks timestamp column")?
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| "invalid clock timestamp")
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
    let mut source = VideoSource::open(Path::new(&args[0]))?;
    source.enable_color();
    let config = load_config(Path::new(&args[1]), source.info.width, source.info.height)?.config;
    let channel = config.spherical_map.as_ref().and_then(|m| m.channel);
    let mut tracker = Tracker::new(config)?;
    let mut output = std::io::BufWriter::new(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[3])?,
    );
    for index in 0..maximum {
        let Some(frame) = source.next_tracking_frame(channel)? else {
            break;
        };
        if omit_period.is_some_and(|period| index % period >= period - 3) {
            continue;
        }
        let frame = if let Some(timestamps) = &timestamps {
            frame.with_timestamp(
                *timestamps
                    .get(index)
                    .ok_or("clock has fewer rows than decoded video")?,
            )?
        } else {
            frame
        };
        let row = tracker.process(frame)?;
        writeln!(
            output,
            "{}",
            serde_json::to_string(
                &serde_json::json!({"source_frame_index":index,"row":row,"temporal":tracker.temporal_evidence()})
            )?
        )?;
    }
    Ok(())
}
