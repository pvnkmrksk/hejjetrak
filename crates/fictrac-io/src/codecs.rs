//! Shared Rust codec dispatch; H.264 error accounting remains explicit.
use crate::{Error, Result};
use oxideav_core::{
    CodecId, CodecInfo, CodecParameters, CodecRegistry, CodecTag, Decoder, Frame, Packet,
};
use oxideav_h264::h264_decoder::H264CodecDecoder;
pub(crate) enum VideoDecoder {
    H264(Box<H264CodecDecoder>),
    Mpeg4(Box<dyn Decoder>),
}
impl VideoDecoder {
    pub fn new(params: &CodecParameters) -> Result<Self> {
        match params.codec_id.as_str() {
            "h264" => {
                let mut decoder = Box::new(H264CodecDecoder::new(params.codec_id.clone()));
                if !params.extradata.is_empty() {
                    decoder
                        .consume_extradata(&params.extradata)
                        .map_err(codec_error)?;
                }
                if decoder
                    .active_sps()
                    .is_some_and(|s| s.bit_depth_luma_minus8 != 0)
                {
                    return Err(Error::Video("only 8-bit H264 input is supported".into()));
                }
                Ok(Self::H264(decoder))
            }
            "mpeg4video" => Ok(Self::Mpeg4(Box::new(Mpeg4Decoder::new(params)?))),
            name => Err(Error::Video(format!(
                "no Rust video adapter for codec {name}"
            ))),
        }
    }
    pub fn send_packet(&mut self, packet: &Packet) -> oxideav_core::Result<()> {
        match self {
            Self::H264(d) => d.send_packet(packet),
            Self::Mpeg4(d) => d.send_packet(packet),
        }
    }
    pub fn receive_frame(&mut self) -> oxideav_core::Result<Frame> {
        match self {
            Self::H264(d) => d.receive_frame(),
            Self::Mpeg4(d) => d.receive_frame(),
        }
    }
    pub fn flush(&mut self) -> oxideav_core::Result<()> {
        match self {
            Self::H264(d) => d.flush(),
            Self::Mpeg4(d) => d.flush(),
        }
    }
    pub fn decode_error_count(&self) -> usize {
        match self {
            Self::H264(d) => d.decode_error_count() as usize,
            Self::Mpeg4(_) => 0,
        }
    }
    pub fn validate_frame(
        &self,
        frame: &oxideav_core::VideoFrame,
        width: usize,
        height: usize,
    ) -> Result<()> {
        if let Self::H264(d) = self
            && d.active_sps().is_some_and(|s| {
                s.bit_depth_luma_minus8 != 0
                    || s.cropped_dimensions() != (width as u32, height as u32)
            })
        {
            return Err(Error::Video(
                "decoded H264 dimensions or bit depth differ from stream header".into(),
            ));
        }
        let plane = frame
            .planes
            .first()
            .ok_or_else(|| Error::Video("decoder returned no luma plane".into()))?;
        if plane.stride != width || plane.data.len() != width * height {
            return Err(Error::Video(
                "decoded video dimensions differ from stream header".into(),
            ));
        }
        Ok(())
    }
}
pub(crate) fn register_mpeg4(registry: &mut CodecRegistry) {
    registry.register(
        CodecInfo::new(CodecId::new("mpeg4video"))
            .decoder(oxideav_mpeg4video::decoder::make_decoder)
            .tags([
                CodecTag::fourcc(b"XVID"),
                CodecTag::fourcc(b"DIVX"),
                CodecTag::fourcc(b"DX50"),
                CodecTag::fourcc(b"FMP4"),
                CodecTag::fourcc(b"MP4V"),
                CodecTag::fourcc(b"M4S2"),
                CodecTag::mp4_object_type(0x20),
            ]),
    );
}
fn codec_error(error: oxideav_core::Error) -> Error {
    Error::Video(format!("Rust video decoder: {error}"))
}

/// The codec's registry bridge exposes macroblock padding as visible pixels.
/// Crop only after verifying the VOL's actual dimensions against the container.
struct Mpeg4Decoder {
    inner: oxideav_mpeg4video::decoder::Mpeg4VideoDecoder,
    ready: std::collections::VecDeque<Frame>,
    width: usize,
    height: usize,
    flushed: bool,
    id: CodecId,
}
impl Mpeg4Decoder {
    fn new(params: &CodecParameters) -> Result<Self> {
        let width = params
            .width
            .ok_or_else(|| Error::Video("MPEG4 stream width missing".into()))?
            as usize;
        let height = params
            .height
            .ok_or_else(|| Error::Video("MPEG4 stream height missing".into()))?
            as usize;
        super::video::validate_size(width, height)?;
        let mut decoder = Self {
            inner: oxideav_mpeg4video::decoder::Mpeg4VideoDecoder::new(),
            ready: std::collections::VecDeque::new(),
            width,
            height,
            flushed: false,
            id: params.codec_id.clone(),
        };
        if !params.extradata.is_empty() {
            let frames = decoder
                .inner
                .decode(&params.extradata)
                .map_err(|e| Error::Video(format!("MPEG4 configuration: {e}")))?;
            decoder.enqueue(frames).map_err(codec_error)?;
        }
        Ok(decoder)
    }
    fn enqueue(
        &mut self,
        frames: Vec<oxideav_mpeg4video::DecodedFrame>,
    ) -> oxideav_core::Result<()> {
        if let Some(vol) = self.inner.vol()
            && (usize::from(vol.width) != self.width || usize::from(vol.height) != self.height)
        {
            return Err(oxideav_core::Error::invalid(
                "MPEG4 VOL dimensions differ from container",
            ));
        }
        for frame in frames {
            let cw = self.width.div_ceil(2);
            let ch = self.height.div_ceil(2);
            let stride = frame.width();
            let rows = frame.height();
            if stride < self.width
                || rows < self.height
                || stride > self.width + 15
                || rows > self.height + 15
            {
                return Err(oxideav_core::Error::invalid(
                    "MPEG4 decoded geometry exceeds macroblock padding",
                ));
            }
            let crop = |pixels: &[u8],
                        stride: usize,
                        width: usize,
                        height: usize|
             -> oxideav_core::Result<oxideav_core::VideoPlane> {
                if stride < width || pixels.len() < stride * height {
                    return Err(oxideav_core::Error::invalid(
                        "MPEG4 decoded plane is truncated",
                    ));
                }
                let mut data = Vec::with_capacity(width * height);
                for y in 0..height {
                    data.extend_from_slice(&pixels[y * stride..y * stride + width]);
                }
                Ok(oxideav_core::VideoPlane {
                    stride: width,
                    data,
                })
            };
            let video = oxideav_core::VideoFrame {
                pts: frame.pts().or(frame.pts_ticks()),
                planes: vec![
                    crop(frame.luma_samples(), stride, self.width, self.height)?,
                    crop(frame.cb_samples(), stride.div_ceil(2), cw, ch)?,
                    crop(frame.cr_samples(), stride.div_ceil(2), cw, ch)?,
                ],
            };
            self.ready.push_back(Frame::Video(video));
        }
        Ok(())
    }
}
impl Decoder for Mpeg4Decoder {
    fn codec_id(&self) -> &CodecId {
        &self.id
    }
    fn send_packet(&mut self, packet: &Packet) -> oxideav_core::Result<()> {
        let frames = self
            .inner
            .decode_with_pts(&packet.data, packet.pts)
            .map_err(|e| oxideav_core::Error::invalid(format!("MPEG4 decode: {e}")))?;
        self.enqueue(frames)
    }
    fn receive_frame(&mut self) -> oxideav_core::Result<Frame> {
        self.ready.pop_front().ok_or(if self.flushed {
            oxideav_core::Error::Eof
        } else {
            oxideav_core::Error::NeedMore
        })
    }
    fn flush(&mut self) -> oxideav_core::Result<()> {
        let frames = self.inner.flush();
        self.enqueue(frames)?;
        self.flushed = true;
        Ok(())
    }
}
