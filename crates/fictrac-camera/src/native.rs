use super::*;
use nokhwa::{
    Camera as NativeCamera,
    pixel_format::RgbFormat,
    utils::{
        ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
        Resolution,
    },
};
use std::time::Instant;
pub fn request_permission() -> Result<()> {
    Ok(())
}
pub fn list_cameras() -> Result<Vec<CameraInfo>> {
    nokhwa::query(ApiBackend::Auto)
        .map(|list| {
            list.iter()
                .enumerate()
                .map(|(i, c)| CameraInfo {
                    id: i as u32,
                    name: c.human_name(),
                })
                .collect()
        })
        .map_err(|e| Error(e.to_string()))
}
pub struct Camera {
    camera: NativeCamera,
    start: Instant,
    count: u64,
}
impl Camera {
    pub fn open(id: u32, fps: f64) -> Result<Self> {
        let format = RequestedFormat::with_formats(
            RequestedFormatType::Closest(CameraFormat::new(
                Resolution::new(640, 480),
                FrameFormat::YUYV,
                if fps > 0.0 {
                    fps.clamp(1.0, 240.0) as u32
                } else {
                    30
                },
            )),
            &[
                FrameFormat::YUYV,
                FrameFormat::RAWRGB,
                FrameFormat::RAWBGR,
                FrameFormat::GRAY,
                FrameFormat::NV12,
            ],
        );
        let mut camera =
            NativeCamera::new(CameraIndex::Index(id), format).map_err(|e| Error(e.to_string()))?;
        camera.open_stream().map_err(|e| Error(e.to_string()))?;
        Ok(Self {
            camera,
            start: Instant::now(),
            count: 0,
        })
    }
    pub fn next_frame(&mut self) -> Result<Option<CameraFrame>> {
        let frame = self.camera.frame().map_err(|e| Error(e.to_string()))?;
        let rgb = frame
            .decode_image::<RgbFormat>()
            .map_err(|e| Error(e.to_string()))?;
        let width = rgb.width() as usize;
        let height = rgb.height() as usize;
        let gray = rgb
            .as_raw()
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| {
                (0.299 * f32::from(c[0]) + 0.587 * f32::from(c[1]) + 0.114 * f32::from(c[2]))
                    / 255.0
            })
            .collect();
        self.count += 1;
        Ok(Some(CameraFrame {
            gray: GrayFrame::new(
                width,
                height,
                gray,
                self.start.elapsed().as_secs_f64() * 1000.0,
            )
            .map_err(|e| Error(e.to_string()))?,
            rgb: rgb.into_raw(),
            sequence: self.count,
        }))
    }
}
