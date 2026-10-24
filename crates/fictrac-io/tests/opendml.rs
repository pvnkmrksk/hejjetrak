use fictrac_io::{Error, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "../src/avi_segments.rs"]
mod avi_segments;
static NEXT: AtomicU64 = AtomicU64::new(0);
fn file() -> Result<(PathBuf, File)> {
    let p = std::env::temp_dir().join(format!(
        "fictrac-opendml-{}-{}.avi",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    Ok((
        p.clone(),
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(p)?,
    ))
}
fn riff(file: &mut File, form: &[u8; 4], size: u32) -> Result<()> {
    file.write_all(b"RIFF")?;
    file.write_all(&size.to_le_bytes())?;
    file.write_all(form)?;
    Ok(())
}
#[test]
fn scans_every_continuation_without_losing_ranges() -> Result<()> {
    let (path, mut f) = file()?;
    riff(&mut f, b"AVI ", 4)?;
    f.write_all(b"JUNK\x03\0\0\0abc\0")?;
    riff(&mut f, b"AVIX", 4)?;
    riff(&mut f, b"AVIX", 4)?;
    f.seek(SeekFrom::Start(5))?;
    let ranges = avi_segments::riff_segments(&mut f)?;
    assert_eq!(ranges, vec![(12, 12), (36, 36), (48, 48)]);
    assert_eq!(f.stream_position()?, 5);
    drop(f);
    fs::remove_file(path)?;
    Ok(())
}
// POSIX seek creates holes without allocating the 4 GiB logical extent.
// Windows needs an explicit filesystem sparse-file control, outside this adapter.
#[cfg(unix)]
#[test]
fn sparse_continuation_past_four_gib_retains_full_u64_offsets() -> Result<()> {
    let (path, mut f) = file()?;
    // A legal largest even first RIFF, sparse on disk: no huge payload allocation.
    let first_size = u32::MAX - 1;
    riff(&mut f, b"AVI ", first_size)?;
    let second_offset = u64::from(first_size) + 8;
    f.seek(SeekFrom::Start(second_offset))?;
    riff(&mut f, b"AVIX", 4)?;
    let ranges = avi_segments::riff_segments(&mut f)?;
    assert_eq!(
        ranges,
        vec![
            (12, second_offset),
            (second_offset + 12, second_offset + 12)
        ]
    );
    assert!(ranges[1].0 > u64::from(u32::MAX));
    drop(f);
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn malformed_segment_boundaries_are_rejected_without_moving_cursor() -> Result<()> {
    for bytes in [
        b"RIFF\x04\0\0\0AVIX".to_vec(),
        b"RIFF\x05\0\0\0AVI x".to_vec(),
        b"RIFF\xff\xff\xff\xffAVI ".to_vec(),
        b"RIFF\x04\0\0\0AVI RIFF\x04\0\0\0AVI ".to_vec(),
        b"RIFF\x03\0\0\0AVI ".to_vec(),
        b"RIFF\x04\0\0\0AVI trailing".to_vec(),
    ] {
        let (path, mut f) = file()?;
        f.write_all(&bytes)?;
        f.seek(SeekFrom::Start(2))?;
        assert!(avi_segments::riff_segments(&mut f).is_err());
        assert_eq!(f.stream_position()?, 2);
        drop(f);
        fs::remove_file(path)?;
    }
    Ok(())
}
#[test]
fn segment_count_is_bounded() -> Result<()> {
    let (path, mut f) = file()?;
    riff(&mut f, b"AVI ", 4)?;
    f.write_all(&b"RIFF\x04\0\0\0AVIX".repeat(131_072))?;
    assert!(avi_segments::riff_segments(&mut f).is_err());
    drop(f);
    fs::remove_file(path)?;
    Ok(())
}

#[path = "../src/avi_mux.rs"]
mod avi_mux;
#[test]
fn forced_opendml_rollover_round_trips_every_raw_frame_and_timestamp() -> Result<()> {
    let (path, f) = file()?;
    drop(f);
    fs::remove_file(&path)?;
    let (w, h) = (32, 16);
    let mut mux = avi_mux::AviMux::create_small_segments(&path, w, h, 29.97, [0; 4], true)?;
    for i in 0..12u8 {
        // Raw AVI carries bottom-up BGR; use uniform frames so only packet order matters.
        mux.write_packet(&vec![i * 10; w * h * 3], true)?;
    }
    mux.finish()?;
    assert!(mux.finish().is_err());
    assert!(mux.write_packet(&[0; 4], true).is_err());
    drop(mux);
    let mut f = File::open(&path)?;
    let ranges = avi_segments::riff_segments(&mut f)?;
    assert!(
        ranges.len() > 1,
        "tiny limit must create RIFF AVIX continuations"
    );
    let mut source = fictrac_io::VideoSource::open(&path)?;
    let mut count = 0;
    while let Some(frame) = source.next_frame()? {
        assert!(
            frame
                .pixels()
                .iter()
                .all(|p| (*p - f32::from(count as u8 * 10) / 255.0).abs() < 1e-6)
        );
        assert!((frame.timestamp_ms() - count as f64 * 1000.0 / 29.97).abs() < 1e-7);
        count += 1;
    }
    assert_eq!(count, 12);
    drop(source);
    drop(f);
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn production_opendml_header_retains_complete_super_index() -> Result<()> {
    let (path, f) = file()?;
    drop(f);
    fs::remove_file(&path)?;
    let mut mux = avi_mux::AviMux::create(&path, 16, 16, 30.0, [0; 4], true)?;
    mux.write_packet(&vec![77; 16 * 16 * 3], true)?;
    mux.finish()?;
    drop(mux);
    let data = fs::read(&path)?;
    assert!(data.windows(4).any(|v| v == b"indx"));
    assert!(data.windows(4).any(|v| v == b"ix00"));
    assert!(data.windows(4).any(|v| v == b"dmlh"));
    let mut source = fictrac_io::VideoSource::open(&path)?;
    assert!(source.next_frame()?.is_some());
    assert!(source.next_frame()?.is_none());
    drop(source);
    fs::remove_file(path)?;
    Ok(())
}
#[cfg(unix)]
#[test]
fn real_video_frames_after_four_gib_are_not_lost_or_truncated() -> Result<()> {
    let (path, f) = file()?;
    drop(f);
    fs::remove_file(&path)?;
    let mut mux = avi_mux::AviMux::create(&path, 16, 16, 30.0, [0; 4], true)?;
    mux.write_packet(&vec![31; 16 * 16 * 3], true)?;
    mux.finish()?;
    drop(mux);
    let mut f = OpenOptions::new().read(true).write(true).open(&path)?;
    let original_end = f.metadata()?.len();
    let first_size = u32::MAX - 1;
    let continuation = u64::from(first_size) + 8;
    // Extend the first RIFF using one sparse JUNK chunk, not billions of zero chunks.
    f.seek(SeekFrom::Start(4))?;
    f.write_all(&first_size.to_le_bytes())?;
    f.seek(SeekFrom::Start(original_end))?;
    f.write_all(b"JUNK")?;
    let junk_size = u32::try_from(continuation - original_end - 8)
        .map_err(|_| Error::Video("test JUNK extent overflow".into()))?;
    f.write_all(&junk_size.to_le_bytes())?;
    f.seek(SeekFrom::Start(continuation))?;
    let packet_size = 16 * 16 * 3u32;
    let movi_size = 4 + 8 + packet_size;
    riff(&mut f, b"AVIX", 4 + 8 + movi_size)?;
    f.write_all(b"LIST")?;
    f.write_all(&movi_size.to_le_bytes())?;
    f.write_all(b"movi")?;
    f.write_all(b"00db")?;
    f.write_all(&packet_size.to_le_bytes())?;
    f.write_all(&vec![219; packet_size as usize])?;
    f.flush()?;
    drop(f);
    let mut source = fictrac_io::VideoSource::open(&path)?;
    let first = source
        .next_frame()?
        .ok_or_else(|| Error::Video("first sparse AVI frame missing".into()))?;
    let second = source
        .next_frame()?
        .ok_or_else(|| Error::Video("continuation sparse AVI frame missing".into()))?;
    assert!((first.pixels()[0] - 31.0 / 255.0).abs() < 1e-6);
    assert!((second.pixels()[0] - 219.0 / 255.0).abs() < 1e-6);
    assert!((second.timestamp_ms() - 1000.0 / 30.0).abs() < 1e-9);
    assert!(source.next_frame()?.is_none());
    drop(source);
    fs::remove_file(path)?;
    Ok(())
}
