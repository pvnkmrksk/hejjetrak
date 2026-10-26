//! Pure Rust H264/MPEG4 Visual/MJPEG and uncompressed AVI/MP4 recording.
use crate::{Error, Result};
use image::codecs::jpeg::JpegEncoder;
use oxideav_core::{
    CodecId, CodecParameters, Encoder, Frame, PixelFormat, Rational, VideoFrame, VideoPlane,
};
use oxideav_h264::h264_encoder::H264CodecEncoder;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingCodec {
    H264,
    H264Mp4,
    Xvid,
    Mpg4,
    Mjpeg,
    Raw,
}
impl RecordingCodec {
    pub fn from_legacy(value: Option<&str>) -> Result<Self> {
        match value.unwrap_or("h264").to_ascii_lowercase().as_str() {
            "h264" => Ok(Self::H264),
            "h264_mp4" => Ok(Self::H264Mp4),
            "xvid" => Ok(Self::Xvid),
            "mpg4" => Ok(Self::Mpg4),
            "mjpg" | "mjpeg" => Ok(Self::Mjpeg),
            "raw" => Ok(Self::Raw),
            other => Err(Error::Video(format!(
                "vid_codec {other:?} has no Rust encoder; supported: h264, h264_mp4, xvid, mpg4, mjpg, raw"
            ))),
        }
    }
    /// Match upstream AVI by default; h264_mp4 explicitly selects MP4.
    pub fn extension(self) -> &'static str {
        if matches!(self, Self::H264Mp4 | Self::Mpg4) {
            "mp4"
        } else {
            "avi"
        }
    }
}
pub struct VideoRecorder {
    width: usize,
    height: usize,
    frames: u64,
    inner: Recording,
    destination: PathBuf,
    partial: PathBuf,
    h264_layout: Option<crate::h264_crop::H264Layout>,
}
enum Recording {
    Mp4 {
        encoder: Box<dyn Encoder>,
        muxer: crate::mp4_recording::Mp4Recording,
    },
    EncodedAvi {
        encoder: Box<dyn Encoder>,
        avi: AviRecorder,
    },
    Avi(AviRecorder),
}
impl VideoRecorder {
    pub fn create(
        path: &Path,
        width: usize,
        height: usize,
        fps: f64,
        codec: RecordingCodec,
    ) -> Result<Self> {
        validate(width, height, fps)?;
        if path.exists() {
            return Err(Error::Video(format!(
                "recording output {} already exists",
                path.display()
            )));
        }
        let mut partial_name = path.as_os_str().to_os_string();
        partial_name.push(".partial");
        let partial = PathBuf::from(partial_name);
        let h264_layout = if matches!(codec, RecordingCodec::H264 | RecordingCodec::H264Mp4) {
            Some(crate::h264_crop::H264Layout::new(width, height)?)
        } else {
            None
        };
        let inner = if matches!(
            codec,
            RecordingCodec::H264
                | RecordingCodec::H264Mp4
                | RecordingCodec::Xvid
                | RecordingCodec::Mpg4
        ) {
            let mpeg4 = matches!(codec, RecordingCodec::Xvid | RecordingCodec::Mpg4);
            let mut parameters =
                CodecParameters::video(CodecId::new(if mpeg4 { "mpeg4video" } else { "h264" }));
            parameters.width = Some(h264_layout.map(|l| l.coded_width).unwrap_or(width) as u32);
            parameters.height = Some(h264_layout.map(|l| l.coded_height).unwrap_or(height) as u32);
            parameters.pixel_format = Some(PixelFormat::Yuv420P);
            let rate = (fps * 1000.0).round() as i64;
            parameters.frame_rate = Some(Rational::new(rate, 1000).reduced());
            let encoder: Box<dyn Encoder> = if mpeg4 {
                parameters.options.insert("qp", "4");
                parameters.options.insert("gop-size", "30");
                oxideav_mpeg4video::encoder::make_encoder(&parameters).map_err(codec_error)?
            } else {
                parameters.options.insert(
                    "format",
                    if codec == RecordingCodec::H264 {
                        "annexb"
                    } else {
                        "avcc"
                    },
                );
                parameters.options.insert("qp", "20");
                // Intra-only High avoids the baseline session's exhaustive
                // inter-frame RDO. Independent frame decoding also bounds
                // recovery after an interrupted camera recording.
                parameters.options.insert("profile", "high");
                parameters.options.insert("intra_4x4", "false");
                parameters.options.insert("gop", "1");
                Box::new(H264CodecEncoder::new(&parameters).map_err(codec_error)?)
            };
            if matches!(codec, RecordingCodec::H264 | RecordingCodec::Xvid) {
                let avi = AviRecorder::create(&partial, width, height, fps, codec)?;
                Recording::EncodedAvi { encoder, avi }
            } else {
                let config = if let Some(layout) = h264_layout {
                    layout.patch_avcc(&encoder.output_params().extradata)?
                } else {
                    encoder.output_params().extradata.clone()
                };
                let muxer = crate::mp4_recording::Mp4Recording::create_with_codec(
                    &partial,
                    width,
                    height,
                    rate as u32,
                    config,
                    mpeg4,
                )?;
                Recording::Mp4 { encoder, muxer }
            }
        } else {
            Recording::Avi(AviRecorder::create(&partial, width, height, fps, codec)?)
        };
        Ok(Self {
            width,
            height,
            frames: 0,
            inner,
            destination: path.to_path_buf(),
            partial,
            h264_layout,
        })
    }
    pub fn write_rgb(&mut self, rgb: &[u8]) -> Result<()> {
        if rgb.len() != self.width * self.height * 3 {
            return Err(Error::Video(
                "recording RGB frame length does not match configured dimensions".into(),
            ));
        }
        if self.frames >= 1_000_000 {
            return Err(Error::Video(
                "recording reached one million frames; start a new recording segment".into(),
            ));
        }
        let padded = self
            .h264_layout
            .filter(|layout| layout.coded_width != self.width || layout.coded_height != self.height)
            .map(|layout| layout.pad_rgb(rgb))
            .transpose()?;
        let encoded_rgb = padded.as_deref().unwrap_or(rgb);
        let encoded_width = self
            .h264_layout
            .map(|l| l.coded_width)
            .unwrap_or(self.width);
        let encoded_height = self
            .h264_layout
            .map(|l| l.coded_height)
            .unwrap_or(self.height);
        match &mut self.inner {
            Recording::Avi(avi) => avi.write_rgb(rgb)?,
            Recording::EncodedAvi { encoder, avi } => {
                let mut frame = rgb_to_yuv420(
                    encoded_rgb,
                    encoded_width,
                    encoded_height,
                    self.frames as i64,
                );
                if encoder.codec_id().as_str() == "mpeg4video" {
                    frame.pts = None;
                }
                encoder
                    .send_frame(&Frame::Video(frame))
                    .map_err(codec_error)?;
                drain_avi(encoder, avi, self.h264_layout.as_ref())?;
            }
            Recording::Mp4 { encoder, muxer } => {
                let mut frame = rgb_to_yuv420(
                    encoded_rgb,
                    encoded_width,
                    encoded_height,
                    self.frames as i64,
                );
                if encoder.codec_id().as_str() == "mpeg4video" {
                    frame.pts = None;
                }
                encoder
                    .send_frame(&Frame::Video(frame))
                    .map_err(codec_error)?;
                drain(encoder, muxer)?;
            }
        }
        self.frames += 1;
        Ok(())
    }
    /// Must be called to write MP4 sample tables / AVI index and close cleanly.
    pub fn finish(mut self) -> Result<()> {
        self.finalize()?;
        drop(self.inner);
        std::fs::hard_link(&self.partial, &self.destination)?;
        std::fs::remove_file(&self.partial)?;
        Ok(())
    }
    /// Finalize a recorded prefix while retaining its explicit `.partial` name.
    /// Used when optional live video cannot keep up with numerical acquisition.
    pub fn finish_partial(mut self) -> Result<()> {
        self.finalize()
    }
    fn finalize(&mut self) -> Result<()> {
        match &mut self.inner {
            Recording::Avi(avi) => avi.finish()?,
            Recording::EncodedAvi { encoder, avi } => {
                encoder.flush().map_err(codec_error)?;
                drain_avi(encoder, avi, self.h264_layout.as_ref())?;
                avi.finish()?;
            }
            Recording::Mp4 { encoder, muxer } => {
                encoder.flush().map_err(codec_error)?;
                drain(encoder, muxer)?;
                muxer.finish()?;
            }
        }
        Ok(())
    }
    pub fn frame_count(&self) -> u64 {
        self.frames
    }
}
fn codec_error(error: oxideav_core::Error) -> Error {
    Error::Video(format!("Rust recording codec: {error}"))
}
fn drain(
    encoder: &mut Box<dyn Encoder>,
    muxer: &mut crate::mp4_recording::Mp4Recording,
) -> Result<()> {
    loop {
        match encoder.receive_packet() {
            Ok(packet) => muxer.write_packet(&packet)?,
            Err(oxideav_core::Error::NeedMore | oxideav_core::Error::Eof) => return Ok(()),
            Err(error) => return Err(codec_error(error)),
        }
    }
}
fn drain_avi(
    encoder: &mut Box<dyn Encoder>,
    avi: &mut AviRecorder,
    layout: Option<&crate::h264_crop::H264Layout>,
) -> Result<()> {
    loop {
        match encoder.receive_packet() {
            Ok(packet) => {
                let data = if let Some(layout) = layout {
                    layout.patch_annex_b(&packet.data)?
                } else {
                    packet.data
                };
                avi.write_encoded(&data, packet.flags.keyframe)?;
            }
            Err(oxideav_core::Error::NeedMore | oxideav_core::Error::Eof) => return Ok(()),
            Err(error) => return Err(codec_error(error)),
        }
    }
}
fn validate(width: usize, height: usize, fps: f64) -> Result<()> {
    if !(16..=8192).contains(&width)
        || !(16..=8192).contains(&height)
        || width.checked_mul(height).is_none_or(|n| n > 32_000_000)
    {
        return Err(Error::Video(
            "recording dimensions must be 16..8192 and at most 32 million pixels".into(),
        ));
    }
    if !fps.is_finite() || !(0.1..=1000.0).contains(&fps) {
        return Err(Error::Video(
            "recording fps must be finite in 0.1..1000".into(),
        ));
    }
    Ok(())
}
fn rgb_to_yuv420(rgb: &[u8], w: usize, h: usize, pts: i64) -> VideoFrame {
    let mut y = vec![0; w * h];
    let cw = w.div_ceil(2);
    let ch = h.div_ceil(2);
    let mut u = vec![0; cw * ch];
    let mut v = vec![0; cw * ch];
    for row in 0..h {
        for col in 0..w {
            let i = (row * w + col) * 3;
            let r = rgb[i] as i32;
            let g = rgb[i + 1] as i32;
            let b = rgb[i + 2] as i32;
            y[row * w + col] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
        }
    }
    for row in (0..h).step_by(2) {
        for col in (0..w).step_by(2) {
            let mut r = 0;
            let mut g = 0;
            let mut b = 0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let i = ((row + dy).min(h - 1) * w + (col + dx).min(w - 1)) * 3;
                    r += rgb[i] as i32;
                    g += rgb[i + 1] as i32;
                    b += rgb[i + 2] as i32;
                }
            }
            r = (r + 2) / 4;
            g = (g + 2) / 4;
            b = (b + 2) / 4;
            let i = (row / 2) * cw + col / 2;
            u[i] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            v[i] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
    VideoFrame {
        pts: Some(pts),
        planes: vec![
            VideoPlane { stride: w, data: y },
            VideoPlane {
                stride: cw,
                data: u,
            },
            VideoPlane {
                stride: cw,
                data: v,
            },
        ],
    }
}

/// OpenDML wrapper; frame encoding stays in Rust and RIFF/AVIX index handling
/// belongs to the bounded container adapter.
struct AviRecorder {
    muxer: crate::avi_mux::AviMux,
    width: usize,
    height: usize,
    codec: RecordingCodec,
}
impl AviRecorder {
    fn create(
        path: &Path,
        width: usize,
        height: usize,
        fps: f64,
        codec: RecordingCodec,
    ) -> Result<Self> {
        validate(width, height, fps)?;
        let fourcc = match codec {
            RecordingCodec::H264 => *b"H264",
            RecordingCodec::Xvid => *b"XVID",
            RecordingCodec::Mjpeg => *b"MJPG",
            RecordingCodec::Raw => *b"DIB ",
            _ => {
                return Err(Error::Video(
                    "MP4 codec requested through AVI adapter".into(),
                ));
            }
        };
        let muxer = crate::avi_mux::AviMux::create(
            path,
            width,
            height,
            fps,
            fourcc,
            codec == RecordingCodec::Raw,
        )?;
        Ok(Self {
            muxer,
            width,
            height,
            codec,
        })
    }
    pub fn write_rgb(&mut self, rgb: &[u8]) -> Result<()> {
        if rgb.len() != self.width * self.height * 3 {
            return Err(Error::Video("AVI RGB dimensions mismatch".into()));
        }
        let mut packet = Vec::new();
        if self.codec == RecordingCodec::Mjpeg {
            JpegEncoder::new_with_quality(&mut packet, 95)
                .encode(
                    rgb,
                    self.width as u32,
                    self.height as u32,
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| Error::Video(format!("JPEG recording: {e}")))?;
        } else {
            let stride = (self.width * 3 + 3) & !3;
            packet.resize(stride * self.height, 0);
            for y in 0..self.height {
                for x in 0..self.width {
                    let src = (y * self.width + x) * 3;
                    let dst = (self.height - 1 - y) * stride + x * 3;
                    packet[dst..dst + 3].copy_from_slice(&[rgb[src + 2], rgb[src + 1], rgb[src]]);
                }
            }
        }
        self.write_encoded(&packet, true)
    }

    fn write_encoded(&mut self, packet: &[u8], key: bool) -> Result<()> {
        self.muxer.write_packet(packet, key)
    }
    fn finish(&mut self) -> Result<()> {
        self.muxer.finish()
    }
}
