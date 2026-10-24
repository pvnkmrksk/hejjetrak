//! Bounded single-video AVI H.264, MJPEG and uncompressed 24-bit DIB input.
use crate::codecs::VideoDecoder;
use crate::{Error, Result, VideoInfo};
use oxideav_core::{CodecId, CodecParameters, Frame, Packet, TimeBase};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
pub(crate) struct Avi {
    file: File,
    width: usize,
    height: usize,
    top_down: bool,
    mjpeg: bool,
    fps: f64,
    frames: Vec<(u64, usize)>,
    count: usize,
    decoded: usize,
    decoder: Option<VideoDecoder>,
    flushed: bool,
    prefix: Vec<u8>,
}
#[derive(Default)]
struct Header {
    width: usize,
    height: usize,
    top_down: bool,
    compression: u32,
    bits: u16,
    video: bool,
    scale: u32,
    rate: u32,
    frames: Vec<(u64, usize)>,
    stream: usize,
    video_stream: Option<usize>,
    extradata: Vec<u8>,
    chunks: usize,
}
impl Avi {
    pub fn open(path: &Path) -> Result<(Self, VideoInfo)> {
        let mut file = File::open(path)?;
        let segments = crate::avi_segments::riff_segments(&mut file)?;
        let mut header = Header::default();
        for (start, end) in segments {
            scan(&mut file, start, end, 0, &mut header)?;
        }
        super::video::validate_size(header.width, header.height)?;
        if header.video_stream.is_none()
            || !matches!(header.bits, 0 | 24)
            || !matches!(
                header.compression,
                0 | 0x47504a4d
                    | 0x34363248
                    | 0x31435641
                    | 0x44495658
                    | 0x58564944
                    | 0x30355844
                    | 0x34504d46
                    | 0x5634504d
                    | 0x3253344d
            )
        {
            return Err(Error::Video(
                "AVI input supports H264/AVC1, XVID/DIVX/MPEG4, MJPG or uncompressed 24-bit DIB"
                    .into(),
            ));
        }
        if header.scale == 0 || header.rate == 0 {
            return Err(Error::Video("AVI missing valid frame rate".into()));
        }
        let fps = header.rate as f64 / header.scale as f64;
        if !(0.01..=100_000.).contains(&fps) {
            return Err(Error::Video("invalid AVI frame rate".into()));
        }
        if header.frames.is_empty() {
            return Err(Error::Video("AVI contains no video frames".into()));
        }
        let mjpeg = header.compression == 0x47504a4d;
        let avc = matches!(header.compression, 0x34363248 | 0x31435641);
        let mpeg4 = matches!(
            header.compression,
            0x44495658 | 0x58564944 | 0x30355844 | 0x34504d46 | 0x5634504d | 0x3253344d
        );
        let decoder = if avc || mpeg4 {
            let mut params =
                CodecParameters::video(CodecId::new(if avc { "h264" } else { "mpeg4video" }));
            params.width = Some(header.width as u32);
            params.height = Some(header.height as u32);
            if mpeg4 || header.extradata.first() == Some(&1) {
                params.extradata = header.extradata.clone();
            }
            Some(VideoDecoder::new(&params)?)
        } else {
            None
        };
        let info = VideoInfo {
            width: header.width,
            height: header.height,
            fps: Some(fps),
            codec: if decoder.is_some() {
                if avc {
                    "avi/h264 (pure Rust)".into()
                } else {
                    "avi/mpeg4video (pure Rust)".into()
                }
            } else if mjpeg {
                "avi/mjpg (pure Rust)".into()
            } else {
                "avi/raw (pure Rust)".into()
            },
            duration_ms: Some(header.frames.len() as f64 * 1000. / fps),
        };
        Ok((
            Self {
                file,
                width: header.width,
                height: header.height,
                top_down: header.top_down,
                mjpeg,
                fps,
                frames: header.frames,
                count: 0,
                decoded: 0,
                decoder,
                flushed: false,
                prefix: if mpeg4 || header.extradata.first() == Some(&1) {
                    Vec::new()
                } else {
                    header.extradata
                },
            },
            info,
        ))
    }
    pub fn next_rgb(&mut self) -> Result<Option<(Vec<u8>, f64)>> {
        if self.decoder.is_some() {
            return self.next_h264();
        }
        let Some((offset, size)) = self.frames.get(self.count).copied() else {
            return Ok(None);
        };
        self.file.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0; size];
        self.file.read_exact(&mut data)?;
        let rgb = if self.mjpeg {
            let mut decoder = image::ImageReader::with_format(
                std::io::Cursor::new(data),
                image::ImageFormat::Jpeg,
            );
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(self.width as u32);
            limits.max_image_height = Some(self.height as u32);
            limits.max_alloc = Some(256_000_000);
            decoder.limits(limits);
            let image = decoder
                .decode()
                .map_err(|e| Error::Video(format!("AVI JPEG frame {}: {e}", self.count)))?
                .into_rgb8();
            if image.dimensions() != (self.width as u32, self.height as u32) {
                return Err(Error::Video(
                    "AVI JPEG dimensions differ from stream header".into(),
                ));
            }
            image.into_raw()
        } else {
            let stride = (self.width * 3 + 3) & !3;
            if data.len() != stride * self.height {
                return Err(Error::Video("raw AVI frame storage mismatch".into()));
            }
            let mut rgb = vec![0; self.width * self.height * 3];
            for y in 0..self.height {
                let row = if self.top_down {
                    y
                } else {
                    self.height - 1 - y
                };
                for x in 0..self.width {
                    let src = row * stride + x * 3;
                    let dst = (y * self.width + x) * 3;
                    rgb[dst..dst + 3].copy_from_slice(&[data[src + 2], data[src + 1], data[src]]);
                }
            }
            rgb
        };
        let timestamp = self.count as f64 * 1000. / self.fps;
        self.count += 1;
        Ok(Some((rgb, timestamp)))
    }
    fn next_h264(&mut self) -> Result<Option<(Vec<u8>, f64)>> {
        let Some(decoder) = &mut self.decoder else {
            return Ok(None);
        };
        loop {
            match decoder.receive_frame() {
                Ok(Frame::Video(frame)) => {
                    decoder.validate_frame(&frame, self.width, self.height)?;
                    let rgb = super::video::video_rgb(&frame, self.width, self.height)?;
                    let timestamp = self.decoded as f64 * 1000.0 / self.fps;
                    self.decoded += 1;
                    return Ok(Some((rgb, timestamp)));
                }
                Ok(_) => {}
                Err(oxideav_core::Error::Eof) => return Ok(None),
                Err(oxideav_core::Error::NeedMore) => {
                    if let Some((offset, size)) = self.frames.get(self.count).copied() {
                        self.file.seek(SeekFrom::Start(offset))?;
                        let mut data = vec![0; size];
                        self.file.read_exact(&mut data)?;
                        if !self.prefix.is_empty() {
                            let mut prefixed = std::mem::take(&mut self.prefix);
                            prefixed.extend(data);
                            data = prefixed;
                        }
                        let packet = Packet::new(0, TimeBase::new(1, 1_000_000), data)
                            .with_pts((self.count as f64 * 1_000_000.0 / self.fps).round() as i64);
                        decoder.send_packet(&packet).map_err(av_error)?;
                        if decoder.decode_error_count() > 0 {
                            return Err(Error::Video(
                                "AVI video decoder reported a concealed decode error".into(),
                            ));
                        }
                        self.count += 1;
                    } else if !self.flushed {
                        decoder.flush().map_err(av_error)?;
                        self.flushed = true;
                    } else {
                        return Ok(None);
                    }
                }
                Err(error) => return Err(av_error(error)),
            }
        }
    }
}
fn le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
fn scan(
    file: &mut File,
    mut start: u64,
    end: u64,
    depth: usize,
    header: &mut Header,
) -> Result<()> {
    if depth > 5 {
        return Err(Error::Video("AVI chunk nesting exceeds limit".into()));
    }
    while start < end {
        header.chunks += 1;
        if header.chunks > 2_000_000 {
            return Err(Error::Video("AVI chunk count exceeds limit".into()));
        }
        if end - start < 8 {
            return Err(Error::Video("truncated AVI chunk header".into()));
        }
        file.seek(SeekFrom::Start(start))?;
        let mut chunk = [0; 8];
        file.read_exact(&mut chunk)?;
        let size = le(&chunk[4..]) as u64;
        let payload = start + 8;
        let next = payload
            .checked_add(size)
            .and_then(|v| v.checked_add(size % 2))
            .ok_or_else(|| Error::Video("AVI chunk size overflow".into()))?;
        if next > end {
            return Err(Error::Video(
                "AVI chunk extends beyond its container".into(),
            ));
        }
        match &chunk[..4] {
            b"LIST" => {
                if size < 4 {
                    return Err(Error::Video("invalid AVI LIST".into()));
                }
                let mut kind = [0; 4];
                file.read_exact(&mut kind)?;
                if matches!(&kind, b"hdrl" | b"strl" | b"movi" | b"rec ") {
                    if &kind == b"strl" {
                        header.video = false;
                    }
                    scan(file, payload + 4, payload + size, depth + 1, header)?;
                    if &kind == b"strl" {
                        header.stream += 1;
                    }
                }
            }
            b"strh" => {
                if size < 48 {
                    return Err(Error::Video("truncated AVI stream header".into()));
                }
                let mut bytes = [0; 48];
                file.read_exact(&mut bytes)?;
                if &bytes[..4] == b"vids" {
                    if header.video_stream.is_some() {
                        return Err(Error::Video(
                            "multiple AVI video streams are unsupported".into(),
                        ));
                    }
                    header.video = true;
                    header.video_stream = Some(header.stream);
                    header.scale = le(&bytes[20..24]);
                    header.rate = le(&bytes[24..28]);
                }
            }
            b"strf" if header.video => {
                if size < 40 {
                    return Err(Error::Video("truncated AVI bitmap header".into()));
                }
                let mut bytes = [0; 40];
                file.read_exact(&mut bytes)?;
                let w = le(&bytes[4..8]) as i32;
                let h = le(&bytes[8..12]) as i32;
                if w <= 0 || h == 0 || h == i32::MIN {
                    return Err(Error::Video("invalid AVI bitmap dimensions".into()));
                }
                header.width = w as usize;
                header.height = h.unsigned_abs() as usize;
                header.top_down = h < 0;
                header.bits = u16::from_le_bytes([bytes[14], bytes[15]]);
                header.compression = le(&bytes[16..20]
                    .iter()
                    .map(u8::to_ascii_uppercase)
                    .collect::<Vec<_>>());
                if size > 40 {
                    if size > 65536 {
                        return Err(Error::Video("AVI codec extradata exceeds 64KB".into()));
                    }
                    header.extradata = vec![0; (size - 40) as usize];
                    file.read_exact(&mut header.extradata)?;
                }
            }
            _ => {
                let id = &chunk[..4];
                if id[0].is_ascii_digit()
                    && id[1].is_ascii_digit()
                    && matches!(&id[2..], b"dc" | b"db")
                {
                    let stream = ((id[0] - b'0') * 10 + id[1] - b'0') as usize;
                    if Some(stream) == header.video_stream {
                        if size > 128_000_000 || header.frames.len() >= 1_000_000 {
                            return Err(Error::Video("AVI frame/packet limit exceeded".into()));
                        }
                        header.frames.push((payload, size as usize));
                    }
                }
            }
        }
        start = next;
    }
    Ok(())
}

fn av_error(error: oxideav_core::Error) -> Error {
    Error::Video(format!("AVI video decoder: {error}"))
}
