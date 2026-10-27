use super::*;
use nokhwa::{
    Camera as NativeCamera,
    pixel_format::RgbFormat,
    utils::{
        ApiBackend, CameraControl, CameraIndex, ControlValueDescription, ControlValueSetter,
        FrameFormat, KnownCameraControl, KnownCameraControlFlag, RequestedFormat,
        RequestedFormatType,
    },
};
use std::time::Instant;
pub fn request_permission() -> Result<()> {
    Ok(())
}
pub fn list_cameras() -> Result<Vec<CameraInfo>> {
    let devices = nokhwa::query(ApiBackend::Auto).map_err(|error| Error(error.to_string()))?;
    #[cfg(target_os = "linux")]
    {
        crate::identity::indexed_camera_infos(
            devices
                .iter()
                .map(|camera| (camera.index().as_string(), camera.human_name())),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        devices
            .iter()
            .enumerate()
            .map(|(ordinal, camera)| {
                Ok(CameraInfo {
                    id: u32::try_from(ordinal).map_err(|_| {
                        Error("too many cameras to represent their native ordinal".into())
                    })?,
                    name: camera.human_name(),
                })
            })
            .collect()
    }
}
pub struct Camera {
    camera: NativeCamera,
    start: Instant,
    clock: crate::clock::CaptureClock,
    count: u64,
    read_failed: bool,
}
impl Camera {
    /// Identity reported by the opened backend, not an inferred enumeration path.
    pub fn source_label(&self) -> String {
        let info = self.camera.info();
        #[cfg(target_os = "linux")]
        if let Ok(devices) = nokhwa::query(ApiBackend::Video4Linux)
            && let Some(device) = devices
                .iter()
                .find(|device| device.index() == self.camera.index())
            && let Some(path) = device.description().strip_prefix("Video4Linux Device @ ")
            && !path.is_empty()
        {
            return format!("{} ({path})", info.human_name());
        }
        #[cfg(target_os = "windows")]
        if !info.misc().is_empty() {
            return format!("{} ({})", info.human_name(), info.misc());
        }
        format!(
            "{} ({:?} camera {})",
            info.human_name(),
            self.camera.backend(),
            self.camera.index().as_string()
        )
    }
    pub fn open(id: u32, fps: f64) -> Result<Self> {
        if !fps.is_finite() || (fps > 0.0 && !(0.1..=1000.0).contains(&fps)) {
            return Err(Error(
                "camera fps must be finite and 0.1..1000, or nonpositive for automatic".into(),
            ));
        }
        Self::open_internal(
            id,
            CameraFormatRequest {
                width: 640,
                height: 480,
                fps: (fps > 0.0).then_some(fps),
            },
            false,
        )
    }
    pub fn open_with_format(id: u32, request: CameraFormatRequest) -> Result<Self> {
        request.validate()?;
        Self::open_internal(id, request, true)
    }
    fn open_internal(id: u32, request: CameraFormatRequest, exact: bool) -> Result<Self> {
        let requested_fps = request.fps.unwrap_or(30.0);
        if request.fps.is_some() && requested_fps.fract() != 0.0 {
            return Err(Error(
                "this Nokhwa backend supports integer camera FPS only; choose an advertised rate"
                    .into(),
            ));
        }
        // Bootstrap without requiring YUYV: mono sensors and MJPEG-only UVC
        // devices may not advertise that encoding, or the default 640x480 size.
        // Select the actual bounded format from the complete advertised list
        // before opening the capture stream or allocating image buffers.
        let format = RequestedFormat::with_formats(
            RequestedFormatType::None,
            &[
                FrameFormat::YUYV,
                FrameFormat::RAWRGB,
                FrameFormat::RAWBGR,
                FrameFormat::GRAY,
                FrameFormat::NV12,
                FrameFormat::MJPEG,
            ],
        );
        let mut camera = NativeCamera::new(CameraIndex::Index(id), format)
            .map_err(|e| camera_error(id, "open", e))?;
        let formats = camera
            .compatible_camera_formats()
            .map_err(|e| camera_error(id, "enumerate formats for", e))?
            .into_iter()
            .filter(|f| supported_format(f.format()))
            .collect::<Vec<_>>();
        let candidates = formats
            .iter()
            .map(|f| crate::selection::FormatCandidate {
                width: f.width() as usize,
                height: f.height() as usize,
                fps: f64::from(f.frame_rate()),
                compressed: f.format() == FrameFormat::MJPEG,
            })
            .collect::<Vec<_>>();
        let selected = crate::selection::select_format(&candidates, &request, exact)
            .and_then(|index| formats.get(index).copied())
            .ok_or_else(|| Error("camera does not advertise a bounded format matching the requested dimensions/FPS; use automatic settings or query camera_status formats".into()))?;
        camera
            .set_camera_requset(RequestedFormat::with_formats(
                RequestedFormatType::Exact(selected),
                &[selected.format()],
            ))
            .map_err(|e| camera_error(id, "select the format for", e))?;
        let actual = camera.camera_format();
        CameraFormatRequest {
            width: actual.width() as usize,
            height: actual.height() as usize,
            fps: Some(f64::from(actual.frame_rate())),
        }
        .validate()?;
        if !supported_format(actual.format()) {
            return Err(Error(
                "camera selected an unsupported pixel format; query camera_status formats".into(),
            ));
        }
        camera
            .open_stream()
            .map_err(|e| camera_error(id, "start", e))?;
        let actual = camera
            .refresh_camera_format()
            .map_err(|e| Error(format!("cannot verify opened camera format: {e}")))?;
        CameraFormatRequest {
            width: actual.width() as usize,
            height: actual.height() as usize,
            fps: Some(f64::from(actual.frame_rate())),
        }
        .validate()?;
        if exact
            && (actual.width() as usize != request.width
                || actual.height() as usize != request.height
                || request
                    .fps
                    .is_some_and(|fps| f64::from(actual.frame_rate()) != fps))
        {
            return Err(Error("camera negotiated a format different from the explicit request; choose another advertised format".into()));
        }
        Ok(Self {
            camera,
            start: Instant::now(),
            clock: crate::clock::CaptureClock::default(),
            count: 0,
            read_failed: false,
        })
    }
    pub fn capabilities(&mut self) -> Result<CameraCapabilities> {
        let active = self
            .camera
            .refresh_camera_format()
            .map_err(|e| Error(format!("cannot read actual camera format: {e}")))?;
        let mut warnings = Vec::new();
        let formats = match self.camera.compatible_camera_formats() {
            Ok(formats) => formats
                .into_iter()
                .filter(|f| supported_format(f.format()))
                .filter_map(|f| {
                    let request = CameraFormatRequest {
                        width: f.width() as usize,
                        height: f.height() as usize,
                        fps: Some(f64::from(f.frame_rate())),
                    };
                    request.validate().ok()?;
                    Some(CameraFormatCapability {
                        width: request.width,
                        height: request.height,
                        pixel_format: format!("{:?}", f.format()),
                        frame_rates: vec![FrameRateRange {
                            min_fps: f64::from(f.frame_rate()),
                            max_fps: f64::from(f.frame_rate()),
                        }],
                    })
                })
                .collect(),
            Err(e) => {
                warnings.push(format!("cannot enumerate supported formats: {e}"));
                Vec::new()
            }
        };
        let exposure = self.exposure_control();
        let exposure_unit = if cfg!(target_os = "windows") {
            "log2(seconds)"
        } else if exposure
            .as_ref()
            .is_ok_and(|c| c.control() == KnownCameraControl::Other(0x009a0902))
        {
            "100 microseconds"
        } else {
            "driver units"
        };
        let exposure_cap = numeric_capability(exposure.as_ref().ok(), exposure_unit);
        let gain = numeric_capability(
            self.camera
                .camera_control(KnownCameraControl::Gain)
                .ok()
                .as_ref(),
            "driver units",
        );
        #[cfg_attr(target_os = "windows", allow(unused_mut))]
        let mut exposure_mode = exposure.as_ref().ok().and_then(|c| {
            if c.flag().contains(&KnownCameraControlFlag::Automatic) {
                Some(ExposureMode::Automatic)
            } else if c.flag().contains(&KnownCameraControlFlag::Manual) {
                Some(ExposureMode::Locked)
            } else {
                None
            }
        });
        #[cfg_attr(target_os = "windows", allow(unused_mut))]
        let mut supported_exposure_modes = Vec::new();
        #[cfg(target_os = "linux")]
        if let Ok(control) = self
            .camera
            .camera_control(KnownCameraControl::Other(0x009a0901))
        {
            if let ControlValueSetter::Integer(value) | ControlValueSetter::EnumValue(value) =
                control.value()
            {
                exposure_mode = match value {
                    1 => Some(ExposureMode::Locked),
                    0 | 3 => Some(ExposureMode::Automatic),
                    _ => None,
                };
            }
            if linux_exposure_setter(&control, ExposureMode::Locked).is_some() {
                supported_exposure_modes.push(ExposureMode::Locked);
            }
            if linux_exposure_setter(&control, ExposureMode::Automatic).is_some() {
                supported_exposure_modes.push(ExposureMode::Automatic);
            }
        }
        #[cfg(target_os="windows")]
        warnings.push("Media Foundation/Nokhwa exposes current exposure mode but cannot change automatic/manual flags; numeric exposure/gain remain available when reported writable".into());
        Ok(CameraCapabilities {
            backend: if cfg!(target_os = "windows") {
                "Media Foundation"
            } else {
                "V4L2"
            }
            .into(),
            current_format: CameraFormatRequest {
                width: active.width() as usize,
                height: active.height() as usize,
                fps: Some(f64::from(active.frame_rate())),
            },
            formats,
            exposure_mode,
            supported_exposure_modes,
            exposure: exposure_cap,
            gain,
            warnings,
        })
    }
    fn exposure_control(&self) -> std::result::Result<CameraControl, nokhwa::NokhwaError> {
        #[cfg(target_os = "linux")]
        if let Ok(c) = self
            .camera
            .camera_control(KnownCameraControl::Other(0x009a0902))
        {
            return Ok(c);
        }
        self.camera.camera_control(KnownCameraControl::Exposure)
    }
    pub fn set_control(&mut self, request: CameraControlRequest) -> Result<CameraCapabilities> {
        let (control, value) = match request {
            CameraControlRequest::ExposureMode { mode } => {
                #[cfg(target_os = "windows")]
                {
                    let _ = mode;
                    return Err(Error("Nokhwa Media Foundation cannot change automatic/manual flags; select a numeric exposure/gain control if writable".into()));
                }
                #[cfg(target_os = "linux")]
                {
                    let control = self
                        .camera
                        .camera_control(KnownCameraControl::Other(0x009a0901))
                        .map_err(|e| {
                            Error(format!(
                                "camera has no V4L2 automatic-exposure control: {e}"
                            ))
                        })?;
                    let setter = linux_exposure_setter(&control, mode).ok_or_else(|| {
                        Error(format!("camera does not advertise writable {mode:?} exposure; query camera_status supported_exposure_modes"))
                    })?;
                    (control, setter)
                }
            }
            CameraControlRequest::Exposure { value } => {
                let control = self
                    .exposure_control()
                    .map_err(|e| Error(format!("camera exposure control unavailable: {e}")))?;
                numeric_capability(Some(&control), "native exposure units").validate(value)?;
                let setter = numeric_setter(&control, value)?;
                (control, setter)
            }
            CameraControlRequest::Gain { value } => {
                let control = self
                    .camera
                    .camera_control(KnownCameraControl::Gain)
                    .map_err(|e| Error(format!("camera gain unavailable: {e}")))?;
                numeric_capability(Some(&control), "driver units").validate(value)?;
                let setter = numeric_setter(&control, value)?;
                (control, setter)
            }
        };
        self.camera
            .set_camera_control(control.control(), value.clone())
            .map_err(|e| Error(format!("camera rejected control: {e}")))?;
        let actual = self
            .camera
            .camera_control(control.control())
            .map_err(|e| Error(format!("cannot read back camera control: {e}")))?;
        if actual.value() != value {
            return Err(Error("camera control readback differs from request; query camera_status for actual settings".into()));
        }
        self.capabilities()
    }
    pub fn next_frame(&mut self) -> Result<Option<CameraFrame>> {
        if self.read_failed {
            return Err(Error(
                "camera stream previously failed; reconnect before reading another frame".into(),
            ));
        }
        let id = self.camera.index().as_string();
        let frame = match self.camera.frame() {
            Ok(frame) => frame,
            Err(error) => {
                // A timed-out V4L MMAP queue already has a pending buffer;
                // retrying would enqueue it again. Reconnect creates a new queue.
                self.read_failed = true;
                return Err(camera_error(id, "read from", error));
            }
        };
        CameraFormatRequest {
            width: frame.resolution().width() as usize,
            height: frame.resolution().height() as usize,
            fps: None,
        }
        .validate()?;
        let width = frame.resolution().width() as usize;
        let height = frame.resolution().height() as usize;
        let encoding = frame.source_frame_format();
        let packed_layout = match encoding {
            FrameFormat::GRAY => Some(crate::packed::Layout::Gray8),
            FrameFormat::RAWRGB | FrameFormat::RAWBGR => Some(crate::packed::Layout::Rgb24),
            FrameFormat::YUYV => Some(crate::packed::Layout::Yuyv422),
            FrameFormat::NV12 => Some(crate::packed::Layout::Nv12),
            FrameFormat::MJPEG => None,
        };
        if let Some(layout) = packed_layout {
            crate::packed::validate(layout, width, height, frame.buffer().len())?;
        }
        let rgb = if encoding == FrameFormat::MJPEG {
            crate::jpeg::decode(frame.buffer(), width, height)?
        } else {
            frame
                .decode_image::<RgbFormat>()
                .map_err(|e| Error(format!("cannot convert camera pixels: {e}")))?
                .into_raw()
        };
        if width.checked_mul(height).and_then(|n| n.checked_mul(3)) != Some(rgb.len()) {
            return Err(Error("camera RGB buffer length differs from its negotiated dimensions; select another advertised format".into()));
        }
        let gray = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| {
                (0.299 * f32::from(c[0]) + 0.587 * f32::from(c[1]) + 0.114 * f32::from(c[2]))
                    / 255.0
            })
            .collect();
        let timestamp = self.clock.timestamp(
            frame.capture_timestamp(),
            self.start.elapsed().as_secs_f64() * 1000.0,
        )?;
        self.count = self.count.saturating_add(1);
        Ok(Some(CameraFrame {
            gray: GrayFrame::new(width, height, gray, timestamp)
                .map_err(|e| Error(e.to_string()))?,
            rgb,
            sequence: self.count,
        }))
    }
}

fn camera_error(
    id: impl std::fmt::Display,
    operation: &str,
    error: impl std::fmt::Display,
) -> Error {
    let access = if cfg!(target_os = "linux") {
        "Check /dev/video device access (usually the video group), the USB connection, and close other camera apps before Connect again."
    } else {
        "Allow desktop camera access in system privacy settings, check the USB connection, and close other camera apps before Connect again."
    };
    Error(format!("cannot {operation} camera {id}: {error}. {access}"))
}

fn supported_format(format: FrameFormat) -> bool {
    matches!(
        format,
        FrameFormat::YUYV
            | FrameFormat::RAWRGB
            | FrameFormat::RAWBGR
            | FrameFormat::GRAY
            | FrameFormat::NV12
            | FrameFormat::MJPEG
    )
}
fn numeric_capability(control: Option<&CameraControl>, unit: &str) -> NumericControlCapability {
    let Some(c) = control else {
        return NumericControlCapability::unsupported(
            unit,
            "camera does not expose this control through the native backend",
        );
    };
    let (value, min, max, step) = match *c.description() {
        ControlValueDescription::IntegerRange {
            value,
            min,
            max,
            step,
            ..
        } => (
            value as f64,
            Some(min as f64),
            Some(max as f64),
            step as f64,
        ),
        ControlValueDescription::FloatRange {
            value,
            min,
            max,
            step,
            ..
        } => (value, Some(min), Some(max), step),
        ControlValueDescription::Integer { value, step, .. } => {
            (value as f64, None, None, step as f64)
        }
        ControlValueDescription::Float { value, step, .. } => (value, None, None, step),
        _ => {
            return NumericControlCapability::unsupported(
                unit,
                "camera control is not a numeric control",
            );
        }
    };
    if !value.is_finite()
        || min.is_some_and(|v| !v.is_finite())
        || max.is_some_and(|v| !v.is_finite())
        || min.zip(max).is_some_and(|(min, max)| min > max)
    {
        return NumericControlCapability::unsupported(
            unit,
            "camera supplied invalid numeric control metadata",
        );
    }
    let writable = control_writable(c);
    NumericControlCapability {
        value: Some(value).filter(|v| v.is_finite()),
        minimum: min,
        maximum: max,
        step: Some(step).filter(|v| v.is_finite() && *v > 0.0),
        unit: unit.into(),
        writable,
        unsupported_reason: (!writable)
            .then(|| "camera control is inactive, automatic, disabled or read-only".into()),
    }
}
fn control_writable(c: &CameraControl) -> bool {
    c.active()
        && !c.flag().iter().any(|f| {
            matches!(
                f,
                KnownCameraControlFlag::ReadOnly
                    | KnownCameraControlFlag::Disabled
                    | KnownCameraControlFlag::Automatic
            )
        })
}
fn numeric_setter(control: &CameraControl, value: f64) -> Result<ControlValueSetter> {
    match control.value() {
        ControlValueSetter::Integer(_)
            if value.fract() == 0.0 && value >= i64::MIN as f64 && value < i64::MAX as f64 =>
        {
            Ok(ControlValueSetter::Integer(value as i64))
        }
        ControlValueSetter::Float(_) => Ok(ControlValueSetter::Float(value)),
        _ => Err(Error(
            "camera requires an exactly representable native numeric control value".into(),
        )),
    }
}
#[cfg(target_os = "linux")]
fn linux_exposure_setter(
    control: &CameraControl,
    mode: ExposureMode,
) -> Option<ControlValueSetter> {
    if !control_writable(control) {
        return None;
    }
    let current = match control.value() {
        ControlValueSetter::Integer(value) | ControlValueSetter::EnumValue(value) => value,
        _ => return None,
    };
    let candidates = match mode {
        ExposureMode::Locked => [1, 1],
        // Preserve the camera's existing automatic mode when possible. UVC
        // cameras commonly advertise only manual=1 and aperture-priority=3;
        // the missing menu entry 0 is not implied by its numeric range.
        ExposureMode::Automatic if current == 0 || current == 3 => [current, current],
        ExposureMode::Automatic => [3, 0],
    };
    candidates.into_iter().find_map(|value| {
        let setter = if matches!(control.description(), ControlValueDescription::Enum { .. }) {
            ControlValueSetter::EnumValue(value)
        } else {
            ControlValueSetter::Integer(value)
        };
        control
            .description()
            .verify_setter(&setter)
            .then_some(setter)
    })
}

#[cfg(all(test, target_os = "linux"))]
mod exposure_tests {
    use super::*;

    #[test]
    fn sparse_uvc_exposure_menu_never_selects_an_unadvertised_entry() {
        let control = CameraControl::new(
            KnownCameraControl::Other(0x009a0901),
            "Exposure mode".into(),
            ControlValueDescription::Enum {
                value: 1,
                possible: vec![1, 3],
                default: 3,
            },
            vec![],
            true,
        );
        assert_eq!(
            linux_exposure_setter(&control, ExposureMode::Automatic),
            Some(ControlValueSetter::EnumValue(3))
        );
        assert_eq!(
            linux_exposure_setter(&control, ExposureMode::Locked),
            Some(ControlValueSetter::EnumValue(1))
        );
        let manual_only = CameraControl::new(
            KnownCameraControl::Other(0x009a0901),
            "Exposure mode".into(),
            ControlValueDescription::Enum {
                value: 1,
                possible: vec![1],
                default: 1,
            },
            vec![],
            true,
        );
        assert_eq!(
            linux_exposure_setter(&manual_only, ExposureMode::Automatic),
            None
        );
    }
}
