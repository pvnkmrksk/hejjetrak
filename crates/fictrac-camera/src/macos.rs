//! Rust bindings to AVFoundation. All borrowed OS memory stays inside the callback.
use super::*;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_av_foundation::*;
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::*;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use std::time::Duration;

fn diagnostic(
    stage: &str,
    session: &AVCaptureSession,
    device: &AVCaptureDevice,
    output: &AVCaptureVideoDataOutput,
) {
    if std::env::var_os("FICTRAC_CAMERA_DIAGNOSTICS").is_none() {
        return;
    }
    // SAFETY: All arguments are live retained objects on the owning capture worker.
    // These read-only properties are available on the same OS versions as capture.
    unsafe {
        use std::io::Write;
        let dimensions = objc2_core_media::CMVideoFormatDescriptionGetDimensions(
            &device.activeFormat().formatDescription(),
        );
        let _ = writeln!(
            std::io::stderr(),
            "FicTrac camera {stage}: can640={} preset={} input={}x{} settings={:?}",
            session.canSetSessionPreset(AVCaptureSessionPreset640x480),
            session.sessionPreset(),
            dimensions.width,
            dimensions.height,
            output.videoSettings()
        );
    }
}
pub fn request_permission() -> Result<()> {
    // SAFETY: AVMediaTypeVideo is a constant framework string; the copied block captures nothing.
    unsafe {
        let media = AVMediaTypeVideo.ok_or_else(|| Error("AVMediaTypeVideo unavailable".into()))?;
        let block = block2::RcBlock::new(|_allowed: objc2::runtime::Bool| {});
        AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &block);
    }
    Ok(())
}
pub fn list_cameras() -> Result<Vec<CameraInfo>> {
    // SAFETY: Query uses a valid media type, and retained NSArray owns each device during iteration.
    #[allow(deprecated)]
    unsafe {
        let media = AVMediaTypeVideo.ok_or_else(|| Error("AVMediaTypeVideo unavailable".into()))?;
        let devices = AVCaptureDevice::devicesWithMediaType(media);
        Ok(devices
            .iter()
            .enumerate()
            .map(|(id, device)| CameraInfo {
                id: id as u32,
                name: device.localizedName().to_string(),
            })
            .collect())
    }
}
#[derive(Debug)]
struct DelegateState {
    latest: Arc<Latest>,
}
define_class!(
    // SAFETY: NSObject has no subclass invariants. Ivars are Send+Sync and there is no custom Drop.
    #[unsafe(super=NSObject)]
    #[ivars=DelegateState]
    struct CaptureDelegate;
    // SAFETY: NSObjectProtocol adds no invariants.
    unsafe impl NSObjectProtocol for CaptureDelegate {}
    // SAFETY: Selector, argument types and calling convention match AVFoundation's delegate protocol.
    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for CaptureDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn output(
            &self,
            _output: &AVCaptureOutput,
            sample: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            // SAFETY: AVFoundation lends a valid CMSampleBuffer for this callback. Pixels are copied
            // while locked, and no pointer/reference to its memory is published to another thread.
            unsafe {
                let Some(pixel) = sample.image_buffer() else {
                    return;
                };
                if CVPixelBufferGetPixelFormatType(&pixel) != kCVPixelFormatType_32BGRA {
                    return;
                }
                let flags = CVPixelBufferLockFlags::ReadOnly;
                if CVPixelBufferLockBaseAddress(&pixel, flags) != 0 {
                    return;
                }
                let width = CVPixelBufferGetWidth(&pixel);
                let height = CVPixelBufferGetHeight(&pixel);
                let stride = CVPixelBufferGetBytesPerRow(&pixel);
                let ptr = CVPixelBufferGetBaseAddress(&pixel);
                let length = stride.checked_mul(height);
                let time = sample.presentation_time_stamp();
                if !ptr.is_null()
                    && time.timescale > 0
                    && width.checked_mul(height).is_some_and(|n| n <= 8_000_000)
                    && let Some(length) = length.filter(|n| {
                        *n <= CVPixelBufferGetDataSize(&pixel) && *n <= isize::MAX as usize
                    })
                {
                    let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), length);
                    let timestamp = time.value as f64 * 1000.0 / f64::from(time.timescale);
                    if let Ok(frame) = from_bgra(width, height, stride, bytes, timestamp.max(0.0)) {
                        self.ivars().latest.publish(frame);
                    }
                }
                CVPixelBufferUnlockBaseAddress(&pixel, flags);
            }
        }
    }
);
pub struct Camera {
    session: Retained<AVCaptureSession>,
    output: Retained<AVCaptureVideoDataOutput>,
    _delegate: Retained<CaptureDelegate>,
    _queue: DispatchRetained<DispatchQueue>,
    latest: Arc<Latest>,
}
impl Camera {
    pub fn open(id: u32, fps: f64) -> Result<Self> {
        let requested = requested_interval(fps)?;
        let latest = Arc::new(Latest::default());
        // SAFETY: All session mutation is confined to this worker. Validated device index and
        // canAdd checks precede session mutation. Serial dispatch retains its delegate/queue.
        #[allow(deprecated)]
        unsafe {
            let media =
                AVMediaTypeVideo.ok_or_else(|| Error("video media type unavailable".into()))?;
            if AVCaptureDevice::authorizationStatusForMediaType(media)
                != AVAuthorizationStatus::Authorized
            {
                return Err(Error("camera access is not authorized; allow FicTrac in System Settings → Privacy & Security → Camera, then Connect again".into()));
            }
            let devices = AVCaptureDevice::devicesWithMediaType(media);
            let device = devices.iter().nth(id as usize).ok_or_else(|| {
                Error(format!(
                    "camera {id} is unavailable ({} detected)",
                    devices.len()
                ))
            })?;
            let input = AVCaptureDeviceInput::deviceInputWithDevice_error(&device)
                .map_err(|e| Error(e.to_string()))?;
            let session = AVCaptureSession::new();
            let output = AVCaptureVideoDataOutput::new();
            let value = NSNumber::numberWithUnsignedInt(kCVPixelFormatType_32BGRA);
            let key = NSString::from_str(&kCVPixelBufferPixelFormatTypeKey.to_string());
            let settings = NSDictionary::from_slices(&[&*key], &[value.as_ref()]);
            output.setVideoSettings(Some(&settings));
            output.setAlwaysDiscardsLateVideoFrames(true);
            let delegate = CaptureDelegate::alloc().set_ivars(DelegateState {
                latest: latest.clone(),
            });
            let delegate: Retained<CaptureDelegate> = msg_send![super(delegate), init];
            let queue = DispatchQueue::new("org.fictrac.capture", None);
            output.setSampleBufferDelegate_queue(
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(&queue),
            );
            if !session.canAddInput(&input) || !session.canAddOutput(&output) {
                return Err(Error("camera session cannot attach input/output".into()));
            }
            session.addInput(&input);
            session.addOutput(&output);
            diagnostic("attached-before-preset", &session, &device, &output);
            // Capability depends on attached input/output. An empty session may reject
            // this preset and otherwise retain its default high-resolution selection.
            if session.canSetSessionPreset(AVCaptureSessionPreset640x480) {
                session.setSessionPreset(AVCaptureSessionPreset640x480);
            }
            diagnostic("preset-applied", &session, &device, &output);
            let interval_for = |format: &AVCaptureDeviceFormat| {
                requested.and_then(|requested| {
                    format
                        .videoSupportedFrameRateRanges()
                        .iter()
                        .find_map(|range| {
                            let shortest = range.minFrameDuration();
                            let longest = range.maxFrameDuration();
                            supported_interval(
                                fps,
                                requested,
                                (shortest.value, shortest.timescale),
                                (longest.value, longest.timescale),
                            )
                        })
                })
            };
            let formats = device.formats();
            let selected = formats.iter().filter(|format| {
                let dimensions = objc2_core_media::CMVideoFormatDescriptionGetDimensions(&format.formatDescription());
                dimensions.width >= 16 && dimensions.height >= 16
                    && dimensions.width <= 8192 && dimensions.height <= 8192
                    && i64::from(dimensions.width) * i64::from(dimensions.height) <= 8_000_000
                    && (requested.is_none() || interval_for(format).is_some())
            }).min_by_key(|format| {
                let dimensions = objc2_core_media::CMVideoFormatDescriptionGetDimensions(&format.formatDescription());
                let dx = i64::from(dimensions.width) - 640;
                let dy = i64::from(dimensions.height) - 480;
                (dx * dx + dy * dy, i64::from(dimensions.width) * i64::from(dimensions.height))
            }).ok_or_else(|| Error(format!("camera {id} has no bounded native format supporting requested {fps} fps; use src_fps:-1 for automatic, or choose another camera")))?;
            let selected_dimensions = objc2_core_media::CMVideoFormatDescriptionGetDimensions(
                &selected.formatDescription(),
            );
            let duration = interval_for(&selected);
            // On macOS the output settings choose delivered pixel dimensions; a
            // session preset alone may leave the sensor's active format at full size.
            let width_key = NSString::from_str(&kCVPixelBufferWidthKey.to_string());
            let height_key = NSString::from_str(&kCVPixelBufferHeightKey.to_string());
            let width_value = NSNumber::numberWithInt(selected_dimensions.width);
            let height_value = NSNumber::numberWithInt(selected_dimensions.height);
            let settings = NSDictionary::from_slices(
                &[&*key, &*width_key, &*height_key],
                &[value.as_ref(), width_value.as_ref(), height_value.as_ref()],
            );
            output.setVideoSettings(Some(&settings));
            diagnostic("explicit-output-dimensions", &session, &device, &output);

            // macOS starts with a negotiated format before manual selection takes effect.
            // Filter those transitional buffers without exposing them or advancing sequence.
            latest.set_dimensions(
                selected_dimensions.width as usize,
                selected_dimensions.height as usize,
            )?;
            // Construct the owner before capture starts, so a failed start clears the
            // delegate and drains queued callbacks before releasing any retained object.
            let camera = Self {
                session,
                output,
                _delegate: delegate,
                _queue: queue,
                latest,
            };
            camera.session.startRunning();
            diagnostic(
                "started-before-explicit-format",
                &camera.session,
                &device,
                &camera.output,
            );
            if !camera.session.isRunning() {
                return Err(Error("AVFoundation capture session did not start; check camera availability and system camera permission".into()));
            }
            // Select the validated device format after macOS applies its startup preset.
            // The RAII owner stops/drains capture if configuring the running device fails.
            device
                .lockForConfiguration()
                .map_err(|e| Error(format!("cannot configure camera format/frame rate: {e}")))?;
            // The format comes from this device's immutable formats array, and the
            // requested interval was checked against that exact format before mutation.
            // Some macOS devices do not support InputPriority. The validated active
            // format is still selected directly, and verified after the session starts.
            camera.session.beginConfiguration();
            if camera
                .session
                .canSetSessionPreset(AVCaptureSessionPresetInputPriority)
            {
                camera
                    .session
                    .setSessionPreset(AVCaptureSessionPresetInputPriority);
            }
            device.setActiveFormat(&selected);
            diagnostic(
                "active-format-set-before-commit",
                &camera.session,
                &device,
                &camera.output,
            );
            if let Some(duration) = duration {
                // Preserve exact rational boundaries: 33333333/1e9 is faster than 30fps.
                let duration = objc2_core_media::CMTime::new(duration.0, duration.1);
                let active_max = device.activeVideoMaxFrameDuration();
                if duration.compare(active_max) > 0 {
                    device.setActiveVideoMaxFrameDuration(duration);
                    device.setActiveVideoMinFrameDuration(duration);
                } else {
                    device.setActiveVideoMinFrameDuration(duration);
                    device.setActiveVideoMaxFrameDuration(duration);
                }
            }
            device.unlockForConfiguration();
            camera.session.commitConfiguration();
            diagnostic(
                "committed-explicit-format",
                &camera.session,
                &device,
                &camera.output,
            );
            let active_dimensions = objc2_core_media::CMVideoFormatDescriptionGetDimensions(
                &device.activeFormat().formatDescription(),
            );
            if requested.is_some() && interval_for(&device.activeFormat()).is_none() {
                return Err(Error(format!(
                    "camera negotiated native {}x{} format does not support requested {fps} fps; use automatic fps",
                    active_dimensions.width, active_dimensions.height
                )));
            }
            Ok(camera)
        }
    }
    pub fn next_frame(&mut self) -> Result<Option<CameraFrame>> {
        self.latest.take(Duration::from_millis(100))
    }
}
impl Drop for Camera {
    fn drop(&mut self) {
        // SAFETY: Camera is dropped on its owning worker, never on the callback queue. Remove
        // the delegate before stopping; all retained objects stay alive through stopRunning.
        unsafe {
            self.output.setSampleBufferDelegate_queue(None, None);
            self.session.stopRunning();
            self._queue.exec_sync(|| {});
        }
    }
}
