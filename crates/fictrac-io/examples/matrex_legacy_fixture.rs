//! Deterministic, explicitly synthetic output fixture for unchanged MATREX players.
//! Uses the production OutputSinks; never a Python publisher or replacement receiver.
use fictrac_core::TrackRow;
use fictrac_io::{ConfigDocument, OutputSinks};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "Usage: matrex_legacy_fixture PORT single|topic_json SECONDS NEW_OUTPUT_DIRECTORY"
                .into(),
        );
    }
    let port: u16 = args[0].parse()?;
    if port == 0 || !matches!(args[1].as_str(), "single" | "topic_json") {
        return Err("Use a nonzero port and single or topic_json framing".into());
    }
    let seconds: f64 = args[2].parse()?;
    if !seconds.is_finite() || !(2.0..=300.0).contains(&seconds) {
        return Err("Fixture duration must be finite and between 2 and 300 seconds".into());
    }
    let directory = PathBuf::from(&args[3]);
    fs::create_dir(&directory)?;
    let mut rows = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("rows.jsonl"))?,
    );
    let mut events = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("events.jsonl"))?,
    );
    let text = format!(
        "sock_output:n\nzmq_host:127.0.0.1\nzmq_port:{port}\nzmq_queue_capacity:256\nzmq_output_mode:legacy_matrex_vr\nzmq_framing:{}\nzmq_topic:FT\n",
        args[1]
    );
    let document = ConfigDocument::parse(text.clone())?;
    fs::write(directory.join("config.txt"), text)?;
    let mut sinks = Some(OutputSinks::from_config(&document)?);
    let started = Instant::now();
    let gap_start = seconds * 0.45;
    let gap_end = gap_start + 1.0;
    let mut disconnected = false;
    let mut reconnected = false;
    let frames = (seconds * 30.0).ceil() as u64;
    let mut source_rows = 0_u64;
    for frame in 0..frames {
        let t = frame as f64 / 30.0;
        if let Some(wait) = Duration::from_secs_f64(t).checked_sub(started.elapsed()) {
            thread::sleep(wait);
        }
        if !disconnected && t >= gap_start {
            let status = sinks.take().map(|value| value.status());
            serde_json::to_writer(
                &mut events,
                &json!({"event":"publisher_closed","seconds":t,"status":status}),
            )?;
            writeln!(events)?;
            disconnected = true;
        }
        if disconnected && !reconnected && t >= gap_end {
            sinks = Some(OutputSinks::from_config(&document)?);
            serde_json::to_writer(
                &mut events,
                &json!({"event":"publisher_rebound","seconds":t}),
            )?;
            writeln!(events)?;
            reconnected = true;
        }
        let accepted = frame % 90 != 89;
        let position = if accepted {
            [0.2 * t + 0.03 * t.sin(), -0.12 * t + 0.02 * t.cos()]
        } else {
            [9999.0, -9999.0]
        };
        let row = TrackRow {
            frame,
            sequence: frame,
            delta_camera: [0.0; 3],
            error: 0.0,
            delta_lab: [0.0; 3],
            absolute_camera: [0.0; 3],
            absolute_lab: [0.0; 3],
            position,
            heading: (5.4 + 0.11 * t + 0.02 * (0.3 * t).sin()).rem_euclid(std::f64::consts::TAU),
            direction: 0.0,
            speed: 0.0,
            integrated: [100.0 + t, -100.0 - t],
            timestamp_ms: t * 1000.0,
            delta_ms: 1000.0 / 30.0,
            accepted,
            status: if accepted { "tracked" } else { "rejected" },
            samples: 100,
        };
        let published = sinks.is_some();
        if let Some(output) = &mut sinks {
            let errors = output.write(&row);
            if !errors.is_empty() {
                return Err(format!("Production output failed: {errors:?}").into());
            }
        }
        serde_json::to_writer(
            &mut rows,
            &json!({"seconds":t,"publisher_active":published,"row":row}),
        )?;
        writeln!(rows)?;
        rows.flush()?;
        source_rows += 1;
    }
    let status = sinks.as_ref().map(OutputSinks::status);
    serde_json::to_writer(
        &mut events,
        &json!({"event":"complete","seconds":started.elapsed().as_secs_f64(),"source_rows":source_rows,"status":status,"synthetic":true}),
    )?;
    writeln!(events)?;
    rows.flush()?;
    events.flush()?;
    Ok(())
}
