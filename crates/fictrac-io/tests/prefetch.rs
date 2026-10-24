use fictrac_io::VideoSource;
use std::{fs, path::PathBuf, sync::mpsc, time::Duration};

#[test]
fn bounded_pipeline_preserves_full_video_pixels_pts_rgb_and_rate_inference()
-> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample.mp4");
    let mut source = VideoSource::open(&path)?;
    source.enable_color();
    let mut source = source.prefetch()?;
    let mut luma_hash = 0xcbf29ce484222325u64;
    let mut rgb_hash = luma_hash;
    let mut frames = 0;
    while let Some(frame) = source.next_frame()? {
        assert!((frame.timestamp_ms() - frames as f64 * 1000.0 / 30.0).abs() < 1e-7);
        if frames == 100 {
            // Looking ahead must preserve the current RGB as well as both
            // prefetched frames, even when the bounded producer queue is full.
            assert!((source.recording_fps()? - 30.0).abs() < 1e-7);
        }
        let rgb = source.take_rgb().ok_or("missing prefetched RGB")?;
        for value in frame.pixels() {
            let byte = (value * 255.0).round() as u8;
            luma_hash = (luma_hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        for byte in rgb {
            rgb_hash = (rgb_hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        frames += 1;
    }
    assert_eq!(frames, 300);
    assert_eq!(rgb_hash, 0x7c112d90f2f74beb);
    assert_eq!(luma_hash, 0x7e8d27f58358523f);
    assert!(source.next_frame()?.is_none());
    Ok(())
}

fn fixture(
    label: &str,
    frames: usize,
    truncated: bool,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "hejjetrak-prefetch-{label}-{}-{stamp}.y4m",
        std::process::id()
    ));
    let mut bytes = b"YUV4MPEG2 W16 H16 F30:1 Ip Cmono\n".to_vec();
    for frame in 0..frames {
        bytes.extend_from_slice(b"FRAME\n");
        bytes.extend(std::iter::repeat_n(frame as u8, 256));
    }
    if truncated {
        bytes.extend_from_slice(b"FRAME\nshort");
    }
    fs::write(&path, bytes)?;
    Ok(path)
}

#[test]
fn closing_a_paused_full_queue_joins_the_decoder_without_deadlock()
-> Result<(), Box<dyn std::error::Error>> {
    let path = fixture("drop", 20, false)?;
    let source = VideoSource::open(&path)?.prefetch()?;
    std::thread::sleep(Duration::from_millis(30));
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        drop(source);
        let _ = sender.send(());
    });
    receiver.recv_timeout(Duration::from_secs(3))?;
    worker.join().map_err(|_| "drop test worker failed")?;
    fs::remove_file(path)?;
    Ok(())
}

#[test]
fn decode_errors_follow_all_previous_frames_and_never_become_eof_silently()
-> Result<(), Box<dyn std::error::Error>> {
    let path = fixture("error", 2, true)?;
    let mut source = VideoSource::open(&path)?.prefetch()?;
    assert_eq!(
        source
            .next_frame()?
            .ok_or("missing first frame")?
            .timestamp_ms(),
        0.0
    );
    assert!(
        (source
            .next_frame()?
            .ok_or("missing second frame")?
            .timestamp_ms()
            - 1000.0 / 30.0)
            .abs()
            < 1e-7
    );
    assert!(source.next_frame().is_err());
    // A later request must not interpret a failed decoder as clean EOF and
    // accidentally finalize an incomplete recording as a successful take.
    assert!(source.next_frame().is_err());
    drop(source);
    fs::remove_file(path)?;
    Ok(())
}
