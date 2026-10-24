//! Checked 64-bit traversal of AVI/OpenDML RIFF segments.
//! Each returned range excludes the RIFF/form header and trailing word padding.
use crate::{Error, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};
const MAX_SEGMENTS: usize = 131_072;
const MAX_TOP_LEVEL_CHUNKS: usize = 262_144;

/// Return `(payload_start, payload_end)` for `RIFF AVI ` and every `RIFF AVIX`.
/// Uses file offsets throughout, never buffers segment payloads, and restores the
/// caller's cursor even on malformed input. Top-level JUNK/PAD chunks are skipped.
pub(crate) fn riff_segments(file: &mut File) -> Result<Vec<(u64, u64)>> {
    let original = file.stream_position()?;
    let result = scan_segments(file);
    let restored = file.seek(SeekFrom::Start(original));
    match result {
        Err(error) => Err(error),
        Ok(segments) => {
            restored?;
            Ok(segments)
        }
    }
}
fn scan_segments(file: &mut File) -> Result<Vec<(u64, u64)>> {
    let total = file.metadata()?.len();
    let mut offset = 0u64;
    let mut chunks = 0usize;
    let mut segments = vec![];
    while offset < total {
        chunks += 1;
        if chunks > MAX_TOP_LEVEL_CHUNKS {
            return Err(Error::Video("AVI top-level chunk limit exceeded".into()));
        }
        if total - offset < 8 {
            return Err(Error::Video(
                "truncated AVI top-level RIFF/chunk header".into(),
            ));
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
        let payload = offset
            .checked_add(8)
            .ok_or_else(|| Error::Video("AVI top-level offset overflow".into()))?;
        let end = payload
            .checked_add(size)
            .ok_or_else(|| Error::Video("AVI RIFF size overflow".into()))?;
        let next = end
            .checked_add(size & 1)
            .ok_or_else(|| Error::Video("AVI RIFF padding overflow".into()))?;
        if next > total {
            return Err(Error::Video(
                "AVI RIFF/chunk extends beyond the file".into(),
            ));
        }
        match &header[..4] {
            b"RIFF" => {
                if size < 4 {
                    return Err(Error::Video("AVI RIFF is missing its form type".into()));
                }
                let mut form = [0; 4];
                file.read_exact(&mut form)?;
                let expected = if segments.is_empty() {
                    b"AVI "
                } else {
                    b"AVIX"
                };
                if &form != expected {
                    return Err(Error::Video(if segments.is_empty() {
                        "input is not RIFF AVI".into()
                    } else {
                        "OpenDML continuation must be RIFF AVIX".into()
                    }));
                }
                if segments.len() >= MAX_SEGMENTS {
                    return Err(Error::Video("AVI exceeds 131072 OpenDML segments".into()));
                }
                segments.push((payload + 4, end));
            }
            b"JUNK" | b"junk" | b"PAD " if !segments.is_empty() => {}
            _ => {
                return Err(Error::Video(
                    "unexpected top-level chunk outside AVI/OpenDML RIFF segments".into(),
                ));
            }
        }
        offset = next;
    }
    if segments.is_empty() {
        return Err(Error::Video("input contains no RIFF AVI segment".into()));
    }
    Ok(segments)
}
