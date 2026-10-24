//! Pure-Rust OpenDML AVI envelope with 64-bit super indexes and RIFF/AVIX rollover.
use crate::{Error, Result};
use oxideav_avi::muxer::{AviKind, AviMuxOptions, AviMuxer, RiffSegmentLimit, open_avi};
use oxideav_core::{
    CodecId, CodecParameters, CodecTag, Muxer, Packet, Rational, StreamInfo, TimeBase,
};
use std::{
    fs::{File, OpenOptions},
    path::Path,
};
const MAX_FRAMES: u64 = 1_000_000;
// One million 32-million-pixel RGB frames occupy <96 TB. At 1 GiB/RIFF,
// this reservation covers every segment within the recorder's existing bounds.
const SUPER_INDEX_CAPACITY: usize = 131_072;
pub(crate) struct AviMux {
    muxer: AviMuxer,
    durable: File,
    time_base: TimeBase,
    frames: u64,
    finished: bool,
}
impl AviMux {
    pub fn create(
        path: &Path,
        width: usize,
        height: usize,
        fps: f64,
        fourcc: [u8; 4],
        raw: bool,
    ) -> Result<Self> {
        Self::create_inner(
            path,
            width,
            height,
            fps,
            fourcc,
            raw,
            RiffSegmentLimit::OneGiB,
            SUPER_INDEX_CAPACITY,
        )
    }
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "integration rollover tests compile this private module directly"
    )]
    pub(crate) fn create_small_segments(
        path: &Path,
        width: usize,
        height: usize,
        fps: f64,
        fourcc: [u8; 4],
        raw: bool,
    ) -> Result<Self> {
        Self::create_inner(
            path,
            width,
            height,
            fps,
            fourcc,
            raw,
            RiffSegmentLimit::Bytes(4096),
            16,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn create_inner(
        path: &Path,
        width: usize,
        height: usize,
        fps: f64,
        fourcc: [u8; 4],
        raw: bool,
        limit: RiffSegmentLimit,
        reserve: usize,
    ) -> Result<Self> {
        if !(16..=8192).contains(&width)
            || !(16..=8192).contains(&height)
            || width.checked_mul(height).is_none_or(|n| n > 32_000_000)
            || !fps.is_finite()
            || !(0.1..=1000.0).contains(&fps)
            || (!raw && fourcc.iter().any(|v| !v.is_ascii_graphic() && *v != b' '))
        {
            return Err(Error::Video(
                "invalid OpenDML recording dimensions, frame rate or codec FourCC".into(),
            ));
        }
        let rate = (fps * 1000.0).round() as i64;
        let time_base = TimeBase::new(1000, rate);
        let mut params =
            CodecParameters::video(CodecId::new(if raw { "rgb24" } else { "avi_encoded" }));
        params.width = Some(width as u32);
        params.height = Some(height as u32);
        params.frame_rate = Some(Rational::new(rate, 1000));
        params.tag = Some(CodecTag::Fourcc(if raw { [0; 4] } else { fourcc }));
        let stream = StreamInfo {
            index: 0,
            time_base,
            duration: None,
            start_time: Some(0),
            params,
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        let durable = file.try_clone()?;
        let options = AviMuxOptions::new().with_super_index_capacity(reserve);
        let mut muxer = open_avi(Box::new(file), &[stream], AviKind::OpenDml(limit), options)
            .map_err(av_error)?;
        muxer.write_header().map_err(av_error)?;
        Ok(Self {
            muxer,
            durable,
            time_base,
            frames: 0,
            finished: false,
        })
    }
    pub fn write_packet(&mut self, bytes: &[u8], key: bool) -> Result<()> {
        if self.finished {
            return Err(Error::Video(
                "OpenDML recording has already finished".into(),
            ));
        }
        if self.frames >= MAX_FRAMES {
            return Err(Error::Video(
                "OpenDML recording reached one million frames; begin a new recording".into(),
            ));
        }
        if bytes.len() > 128_000_000 {
            return Err(Error::Video("AVI packet exceeds 128 MB".into()));
        }
        let mut packet =
            Packet::new(0, self.time_base, bytes.to_vec()).with_pts(self.frames as i64);
        packet.duration = Some(1);
        packet.flags.keyframe = key;
        self.muxer.write_packet(&packet).map_err(av_error)?;
        self.frames += 1;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Err(Error::Video(
                "OpenDML recording has already finished".into(),
            ));
        }
        self.muxer.write_trailer().map_err(av_error)?;
        if self.muxer.truncated_super_index_segments() != 0 {
            return Err(Error::Video(
                "OpenDML super-index capacity exhausted; incomplete recording was not promoted"
                    .into(),
            ));
        }
        self.durable.sync_all()?;
        self.finished = true;
        Ok(())
    }
}
fn av_error(error: oxideav_core::Error) -> Error {
    Error::Video(format!("Rust OpenDML AVI muxer: {error}"))
}
