use fictrac_core::{Config, Tracker, synthetic};
use fictrac_io::{VideoSource, load_config, write_dat};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> std::io::Result<Self> {
        let p = std::env::temp_dir().join(format!(
            "fictrac-io-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p)?;
        Ok(Self(p))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

#[test]
fn sample_decoder_matches_independent_full_luma_oracle() -> Result {
    // FNV-1a of every presentation-ordered luma byte from FFmpeg extractplanes=y.
    // See docs/validation.md for the exact command and input SHA-256.
    let mut video = VideoSource::open(&example("sample.mp4"))?;
    let mut hash = 0xcbf29ce484222325u64;
    let mut count = 0;
    while let Some(frame) = video.next_frame()? {
        assert_eq!(frame.width(), 384);
        assert_eq!(frame.height(), 288);
        assert!((frame.timestamp_ms() - count as f64 * 1000.0 / 30.0).abs() < 1e-6);
        for p in frame.pixels() {
            hash = (hash ^ u64::from((p * 255.0).round() as u8)).wrapping_mul(0x100000001b3);
        }
        count += 1;
    }
    assert_eq!(count, 300);
    assert_eq!(
        hash, 0x7e8d27f58358523f,
        "decoder differs from independent oracle"
    );
    Ok(())
}
#[test]
fn imports_original_sample_calibration() -> Result {
    let loaded = load_config(&example("upstream-config.txt"), 384, 288)?;
    assert!(loaded.config.sphere.angular_radius > 0.1);
    assert!(loaded.config.sphere.angular_radius < 0.2);
    assert_eq!(loaded.config.ignore_polygons.len(), 2);
    assert_eq!(
        loaded.config.camera_to_lab,
        [0.722445, -0.131314, -0.460878]
    );
    assert!(
        loaded.warnings.is_empty(),
        "all sample settings are now supported: {:?}",
        loaded.warnings
    );
    let text = std::fs::read_to_string(example("upstream-config.txt"))?
        + "\nfuture_unrecognized_option : y\n";
    let extended = fictrac_io::load_config_text(&text, 384, 288)?;
    assert_eq!(extended.warnings.len(), 1);
    assert!(extended.warnings[0].contains("future_unrecognized_option"));
    Ok(())
}
#[test]
fn dat_has_25_finite_columns_and_correct_timing() -> Result {
    let c = Config::default();
    let mut tracker = Tracker::new(c.clone())?;
    tracker.process(synthetic::render(&c, [0.0; 3], 0.0)?)?;
    let row = tracker.process(synthetic::render(&c, [0.0, 0.01, 0.0], 20.0)?)?;
    let mut output = vec![];
    write_dat(&mut output, &row)?;
    let line = String::from_utf8(output)?;
    let values: Vec<f64> = line
        .trim()
        .split(',')
        .map(|v| v.trim().parse())
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(values.len(), 25);
    assert!(values.iter().all(|v| v.is_finite()));
    assert_eq!(values[0], 2.0);
    assert_eq!(values[21], 20.0);
    assert_eq!(values[23], 20.0);
    Ok(())
}
#[test]
fn rejects_truncated_y4m_and_bad_headers() -> Result {
    let temp = Temp::new()?;
    for (i, data) in [
        b"YUV4MPEG2 W16 H16 F30:1 Ip Cmono\nFRAME\nshort".as_slice(),
        b"YUV4MPEG2 W16 H16 F0:0 Ip Cmono\n",
        b"YUV4MPEG2 W999999999 H16 F30:1 Ip Cmono\n",
        b"YUV4MPEG2 W16 H16 F30:1 Ip C420p10\n",
    ]
    .iter()
    .enumerate()
    {
        let p = temp.0.join(format!("bad-{i}.y4m"));
        fs::write(&p, data)?;
        match VideoSource::open(&p) {
            Err(_) => (),
            Ok(mut video) => assert!(video.next_frame().is_err()),
        }
    }
    Ok(())
}
#[test]
fn rejects_corrupt_mp4_without_panicking() -> Result {
    let temp = Temp::new()?;
    let p = temp.0.join("bad.mp4");
    fs::write(&p, b"\0\0\0\x18ftypisom\0\0\0\0isom")?;
    assert!(VideoSource::open(&p).is_err());
    Ok(())
}
#[test]
fn config_rejects_unknown_json_fields_and_duplicate_legacy_keys() -> Result {
    let temp = Temp::new()?;
    let p = temp.0.join("config.json");
    fs::write(&p, "{\"widht\":160}")?;
    assert!(load_config(&p, 160, 160).is_err());
    fs::write(&p, "vfov : 45\nvfov : 90\n")?;
    assert!(load_config(&p, 160, 160).is_err());
    Ok(())
}
#[test]
fn y4m_round_trip_preserves_luma_and_fractional_timing() -> Result {
    let temp = Temp::new()?;
    let p = temp.0.join("test.y4m");
    let mut bytes = b"YUV4MPEG2 W16 H16 F30000:1001 Ip C420jpeg\n".to_vec();
    for _ in 0..3 {
        bytes.extend(b"FRAME\n");
        bytes.extend([64u8; 256]);
        bytes.extend([128u8; 128]);
    }
    fs::write(&p, bytes)?;
    let mut source = VideoSource::open(&p)?;
    for i in 0..3 {
        let frame = source.next_frame()?.ok_or("frame missing")?;
        assert!((frame.timestamp_ms() - i as f64 * 1001.0 / 30.0).abs() < 1e-9);
        assert!(
            frame
                .pixels()
                .iter()
                .all(|p| (*p - 64.0 / 255.0).abs() < 1e-6)
        );
    }
    assert!(source.next_frame()?.is_none());
    Ok(())
}
