use crate::codecs::VideoDecoder;
use crate::{Error, Result};
use fictrac_core::GrayFrame;
use oxideav_core::{CodecRegistry, Demuxer, Frame, StreamInfo};
use serde::Serialize;
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};

#[derive(Debug, Clone, Serialize)]
pub struct VideoInfo {
    pub width: usize,
    pub height: usize,
    pub fps: Option<f64>,
    pub codec: String,
    pub duration_ms: Option<f64>,
}
pub struct VideoSource {
    pub info: VideoInfo,
    color: bool,
    last_rgb: Option<Vec<u8>>,
    inner: Source,
    prefetched: VecDeque<(GrayFrame, Option<Vec<u8>>)>,
}
enum Source {
    Mp4(Box<Mp4>),
    Y4m(Y4m),
    Avi(crate::avi::Avi),
}
struct Mp4 {
    demux: Box<dyn Demuxer>,
    decoder: VideoDecoder,
    stream: StreamInfo,
    flushed: bool,
    origin: Option<f64>,
    last: Option<f64>,
    width: usize,
    height: usize,
    color: bool,
    last_rgb: Option<Vec<u8>>,
}
struct Y4m {
    reader: BufReader<File>,
    width: usize,
    height: usize,
    fps: f64,
    chroma: usize,
    count: u64,
}
pub(crate) fn validate_size(width: usize, height: usize) -> Result<()> {
    if !(16..=8192).contains(&width)
        || !(16..=8192).contains(&height)
        || width.checked_mul(height).is_none_or(|n| n > 32_000_000)
    {
        return Err(Error::Video(
            "unsupported dimensions (max 32 million pixels)".into(),
        ));
    }
    Ok(())
}
impl VideoSource {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 9];
        file.read_exact(&mut magic)?;
        if &magic == b"YUV4MPEG2" {
            return Self::y4m(path);
        }
        if &magic[..4] == b"RIFF" {
            let (avi, info) = crate::avi::Avi::open(path)?;
            return Ok(Self {
                info,
                color: false,
                last_rgb: None,
                inner: Source::Avi(avi),
                prefetched: VecDeque::new(),
            });
        }
        let mut registry = CodecRegistry::new();
        oxideav_h264::register_codecs(&mut registry);
        crate::codecs::register_mpeg4(&mut registry);
        let demux =
            oxideav_mp4::demux::open(Box::new(File::open(path)?), &registry).map_err(av_error)?;
        let stream = demux
            .streams()
            .iter()
            .find(|s| matches!(s.params.codec_id.as_str(),"h264"|"mpeg4video"))
            .cloned()
            .ok_or_else(|| {
                Error::Video("supported inputs: H264 or MPEG4 MP4/MOV, H264/XVID/MJPEG/raw AVI and 8-bit mono/420 Y4M".into())
            })?;
        let width = stream
            .params
            .width
            .ok_or_else(|| Error::Video("missing video width".into()))?
            as usize;
        let height = stream
            .params
            .height
            .ok_or_else(|| Error::Video("missing video height".into()))?
            as usize;
        validate_size(width, height)?;
        if !stream.time_base.is_valid() || stream.time_base.seconds_of(1) <= 0.0 {
            return Err(Error::Video("invalid video time base".into()));
        }
        let decoder = VideoDecoder::new(&stream.params)?;
        let info = VideoInfo {
            width,
            height,
            fps: stream
                .params
                .frame_rate
                .map(|r| r.as_f64())
                .filter(|r| r.is_finite() && *r > 0.0),
            codec: format!("{} (pure Rust)", stream.params.codec_id),
            duration_ms: stream
                .duration
                .map(|d| stream.time_base.seconds_of(d) * 1000.0),
        };
        Ok(Self {
            color: false,
            last_rgb: None,
            info,
            prefetched: VecDeque::new(),
            inner: Source::Mp4(Box::new(Mp4 {
                demux,
                decoder,
                stream,
                flushed: false,
                origin: None,
                last: None,
                width,
                height,
                color: false,
                last_rgb: None,
            })),
        })
    }
    fn y4m(path: &Path) -> Result<Self> {
        let mut reader = BufReader::new(File::open(path)?);
        let header =
            bounded_line(&mut reader)?.ok_or_else(|| Error::Video("missing Y4M header".into()))?;
        let mut width = 0;
        let mut height = 0;
        let mut fps = 0.0;
        let mut chroma_tag = "420jpeg";
        for field in header.split_whitespace().skip(1) {
            if let Some(v) = field.strip_prefix('W') {
                width = v
                    .parse()
                    .map_err(|_| Error::Video("invalid Y4M width".into()))?;
            }
            if let Some(v) = field.strip_prefix('H') {
                height = v
                    .parse()
                    .map_err(|_| Error::Video("invalid Y4M height".into()))?;
            }
            if let Some(v) = field.strip_prefix('F') {
                let (n, d) = v
                    .split_once(':')
                    .ok_or_else(|| Error::Video("invalid frame rate".into()))?;
                let n: f64 = n
                    .parse()
                    .map_err(|_| Error::Video("invalid frame rate".into()))?;
                let d: f64 = d
                    .parse()
                    .map_err(|_| Error::Video("invalid frame rate".into()))?;
                fps = n / d;
            }
            if let Some(v) = field.strip_prefix('C') {
                chroma_tag = v;
            }
            if field.starts_with('I') && field != "Ip" {
                return Err(Error::Video("only progressive Y4M is supported".into()));
            }
        }
        validate_size(width, height)?;
        if !fps.is_finite() || !(0.01..=100_000.0).contains(&fps) {
            return Err(Error::Video("invalid Y4M frame rate".into()));
        }
        let chroma = match chroma_tag {
            "mono" => 0,
            "420" | "420jpeg" | "420mpeg2" | "420paldv" => {
                2 * width.div_ceil(2) * height.div_ceil(2)
            }
            _ => {
                return Err(Error::Video(
                    "only 8-bit mono and 420 Y4M are supported".into(),
                ));
            }
        };
        let info = VideoInfo {
            width,
            height,
            fps: Some(fps),
            codec: format!("y4m/{chroma_tag}"),
            duration_ms: None,
        };
        Ok(Self {
            color: false,
            last_rgb: None,
            info,
            prefetched: VecDeque::new(),
            inner: Source::Y4m(Y4m {
                reader,
                width,
                height,
                fps,
                chroma,
                count: 0,
            }),
        })
    }
    pub fn next_tracking_frame(&mut self, channel: Option<usize>) -> Result<Option<GrayFrame>> {
        self.enable_color();
        let Some(frame) = self.next_frame()? else {
            return Ok(None);
        };
        let rgb = self.last_rgb.as_deref();
        match rgb {
            Some(rgb) => rgb_tracking_frame(frame, rgb, channel).map(Some),
            None => Ok(Some(frame)),
        }
    }
    pub fn enable_color(&mut self) {
        self.color = true;
        if let Source::Mp4(m) = &mut self.inner {
            m.color = true;
        }
    }
    pub fn take_rgb(&mut self) -> Option<Vec<u8>> {
        self.last_rgb.take()
    }
    /// Resolve an actual recording rate. When container metadata omits it, infer
    /// the first available PTS interval without consuming either frame or RGB.
    /// No arbitrary fallback clock is substituted for unknown source timing.
    pub fn recording_fps(&mut self) -> Result<f64> {
        if let Some(fps) = self.info.fps {
            if fps.is_finite() && (0.1..=1000.0).contains(&fps) {
                return Ok(fps);
            }
            return Err(Error::Video("source recording fps is outside supported 0.1..1000; provide an explicit valid src_fps".into()));
        }
        self.enable_color();
        let current_rgb = self.last_rgb.take();
        let decoded = (|| -> Result<()> {
            while self.prefetched.len() < 2 {
                let Some(frame) = self.decode_frame()? else {
                    break;
                };
                let rgb = self.last_rgb.take();
                self.prefetched.push_back((frame, rgb));
            }
            Ok(())
        })();
        self.last_rgb = current_rgb;
        decoded?;
        if self.prefetched.len() < 2 {
            return Err(Error::Video("recording fps unavailable: source has fewer than two remaining timed frames; set src_fps explicitly".into()));
        }
        let delta = self.prefetched[1].0.timestamp_ms() - self.prefetched[0].0.timestamp_ms();
        let fps = 1000.0 / delta;
        if !fps.is_finite() || !(0.1..=1000.0).contains(&fps) {
            return Err(Error::Video("cannot infer a supported recording fps from source timestamps; set src_fps explicitly".into()));
        }
        self.info.fps = Some(fps);
        Ok(fps)
    }
    pub fn next_frame(&mut self) -> Result<Option<GrayFrame>> {
        if let Some((frame, rgb)) = self.prefetched.pop_front() {
            self.last_rgb = rgb;
            return Ok(Some(frame));
        }
        self.decode_frame()
    }
    fn decode_frame(&mut self) -> Result<Option<GrayFrame>> {
        self.last_rgb = None;
        match &mut self.inner {
            Source::Mp4(m) => {
                let frame = m.next_frame()?;
                self.last_rgb = m.last_rgb.take();
                Ok(frame)
            }
            Source::Avi(avi) => {
                let Some((rgb, timestamp)) = avi.next_rgb()? else {
                    return Ok(None);
                };
                let values = rgb
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|p| {
                        ((4899 * u32::from(p[0])
                            + 9617 * u32::from(p[1])
                            + 1868 * u32::from(p[2])
                            + 8192)
                            >> 14) as f32
                            / 255.0
                    })
                    .collect();
                let frame = GrayFrame::new(self.info.width, self.info.height, values, timestamp)?;
                if self.color {
                    self.last_rgb = Some(rgb);
                }
                Ok(Some(frame))
            }
            Source::Y4m(y) => {
                let frame = y.next_frame()?;
                if self.color {
                    self.last_rgb = frame.as_ref().map(|f| {
                        f.pixels()
                            .iter()
                            .flat_map(|p| [(p * 255.0).round() as u8; 3])
                            .collect()
                    });
                }
                Ok(frame)
            }
        }
    }
}
impl Mp4 {
    fn next_frame(&mut self) -> Result<Option<GrayFrame>> {
        loop {
            match self.decoder.receive_frame() {
                Ok(Frame::Video(v)) => {
                    let plane = v
                        .planes
                        .first()
                        .ok_or_else(|| Error::Video("missing luma plane".into()))?;
                    if plane.stride != self.width || plane.data.len() != self.width * self.height {
                        return Err(Error::Video(
                            "decoded frame geometry or bit depth changed".into(),
                        ));
                    }
                    if self.color {
                        self.last_rgb = Some(video_rgb(&v, self.width, self.height)?);
                    }
                    let pts = v.pts.ok_or_else(|| {
                        Error::Video("decoded frame has no presentation timestamp".into())
                    })?;
                    let timestamp = self.stream.time_base.seconds_of(pts) * 1000.0;
                    let origin = *self.origin.get_or_insert(timestamp);
                    let timestamp = timestamp - origin;
                    if !timestamp.is_finite() || self.last.is_some_and(|t| timestamp <= t) {
                        return Err(Error::Video(
                            "decoder emitted non-monotonic presentation timestamps".into(),
                        ));
                    }
                    self.last = Some(timestamp);
                    let pixels = plane.data.iter().map(|p| f32::from(*p) / 255.0).collect();
                    return Ok(Some(GrayFrame::new(
                        self.width,
                        self.height,
                        pixels,
                        timestamp,
                    )?));
                }
                Ok(_) => continue,
                Err(oxideav_core::Error::Eof) => return Ok(None),
                Err(oxideav_core::Error::NeedMore) => {
                    if self.flushed {
                        return Ok(None);
                    }
                    loop {
                        match self.demux.next_packet() {
                            Ok(packet) => {
                                if packet.stream_index != self.stream.index {
                                    continue;
                                }
                                self.decoder.send_packet(&packet).map_err(av_error)?;
                                if self.decoder.decode_error_count() > 0 {
                                    return Err(Error::Video(
                                        "video decoder reported a concealed decode error".into(),
                                    ));
                                }
                                break;
                            }
                            Err(oxideav_core::Error::Eof) => {
                                self.decoder.flush().map_err(av_error)?;
                                self.flushed = true;
                                break;
                            }
                            Err(e) => return Err(av_error(e)),
                        }
                    }
                }
                Err(e) => return Err(av_error(e)),
            }
        }
    }
}
impl Y4m {
    fn next_frame(&mut self) -> Result<Option<GrayFrame>> {
        let Some(line) = bounded_line(&mut self.reader)? else {
            return Ok(None);
        };
        if line != "FRAME" && !line.starts_with("FRAME ") {
            return Err(Error::Video("expected Y4M FRAME marker".into()));
        }
        let mut pixels = vec![0u8; self.width * self.height];
        self.reader.read_exact(&mut pixels)?;
        let copied = std::io::copy(
            &mut self.reader.by_ref().take(self.chroma as u64),
            &mut std::io::sink(),
        )?;
        if copied != self.chroma as u64 {
            return Err(Error::Video("truncated Y4M chroma".into()));
        }
        let timestamp = self.count as f64 * 1000.0 / self.fps;
        self.count += 1;
        Ok(Some(GrayFrame::new(
            self.width,
            self.height,
            pixels.into_iter().map(|p| f32::from(p) / 255.0).collect(),
            timestamp,
        )?))
    }
}
fn av_error(e: oxideav_core::Error) -> Error {
    Error::Video(e.to_string())
}
fn bounded_line(reader: &mut impl BufRead) -> Result<Option<String>> {
    let mut bytes = vec![];
    for _ in 0..4096 {
        let mut b = [0];
        if reader.read(&mut b)? == 0 {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(Error::Video("truncated header".into()))
            };
        }
        if b[0] == b'\n' {
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| Error::Video("non-UTF8 header".into()));
        }
        bytes.push(b[0]);
    }
    Err(Error::Video("header exceeds 4096 bytes".into()))
}
pub fn write_y4m_header(out: &mut impl Write, width: usize, height: usize, fps: u32) -> Result<()> {
    validate_size(width, height)?;
    if fps == 0 {
        return Err(Error::Video("fps must be positive".into()));
    }
    writeln!(out, "YUV4MPEG2 W{width} H{height} F{fps}:1 Ip Cmono")?;
    Ok(())
}
pub fn write_y4m_frame(out: &mut impl Write, frame: &GrayFrame) -> Result<()> {
    out.write_all(b"FRAME\n")?;
    let bytes: Vec<_> = frame
        .pixels()
        .iter()
        .map(|p| (p * 255.0).round() as u8)
        .collect();
    out.write_all(&bytes)?;
    Ok(())
}

pub(crate) fn video_rgb(
    frame: &oxideav_core::VideoFrame,
    width: usize,
    height: usize,
) -> Result<Vec<u8>> {
    let y = frame
        .planes
        .first()
        .ok_or_else(|| Error::Video("missing luma".into()))?;
    let mut rgb = Vec::with_capacity(width * height * 3);
    let uv = frame.planes.get(1).zip(frame.planes.get(2));
    for row in 0..height {
        for col in 0..width {
            let yy = f64::from(
                *y.data
                    .get(row * y.stride + col)
                    .ok_or_else(|| Error::Video("truncated luma".into()))?,
            );
            let (u, v) = if let Some((u, v)) = uv {
                if u.stride == 0 || v.stride == 0 {
                    return Err(Error::Video("invalid chroma stride".into()));
                }
                let chroma_height = u.data.len() / u.stride;
                let chroma_row = row * chroma_height / height;
                let chroma_col = col * u.stride / width;
                let index = chroma_row * u.stride + chroma_col;
                (
                    f64::from(
                        *u.data
                            .get(index)
                            .ok_or_else(|| Error::Video("truncated chroma".into()))?,
                    ) - 128.0,
                    f64::from(
                        *v.data
                            .get(index)
                            .ok_or_else(|| Error::Video("truncated chroma".into()))?,
                    ) - 128.0,
                )
            } else {
                (0.0, 0.0)
            };
            let l = 1.164383 * (yy - 16.0);
            rgb.extend(
                [
                    l + 1.596027 * v,
                    l - 0.391762 * u - 0.812968 * v,
                    l + 2.017232 * u,
                ]
                .map(|v| v.round().clamp(0.0, 255.0) as u8),
            );
        }
    }
    Ok(rgb)
}

/// Match OpenCV's integer BT.601 grayscale convention without depending on OpenCV.
pub fn rgb_tracking_frame(
    frame: GrayFrame,
    rgb: &[u8],
    channel: Option<usize>,
) -> Result<GrayFrame> {
    if rgb.len() != frame.width() * frame.height() * 3 || channel.is_some_and(|c| c > 2) {
        return Err(Error::Video("invalid tracking color buffer/channel".into()));
    }
    let pixels = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| {
            let value = if let Some(c) = channel {
                p[c]
            } else {
                ((4899 * u32::from(p[0]) + 9617 * u32::from(p[1]) + 1868 * u32::from(p[2]) + 8192)
                    >> 14) as u8
            };
            f32::from(value) / 255.0
        })
        .collect();
    Ok(GrayFrame::new(
        frame.width(),
        frame.height(),
        pixels,
        frame.timestamp_ms(),
    )?)
}
