use fictrac_io::VideoSource;
use std::path::PathBuf;

#[test]
fn sample_rgb_matches_original_scalar_conversion_full_video()
-> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample.mp4");
    let mut source = VideoSource::open(&path)?;
    source.enable_color();
    let mut hash = 0xcbf29ce484222325u64;
    let mut frames = 0;
    while let Some(frame) = source.next_frame()? {
        let rgb = source.take_rgb().ok_or("missing sample RGB")?;
        assert_eq!(rgb.len(), frame.width() * frame.height() * 3);
        for byte in rgb {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        frames += 1;
    }
    assert_eq!(frames, 300);
    // Captured from the original scalar conversion before its optimization;
    // this supplements, and does not replace, the independent luma oracle.
    assert_eq!(hash, 0x7c112d90f2f74beb);
    Ok(())
}
