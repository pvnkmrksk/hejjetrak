# Native camera capabilities and controls

The camera crate exposes serializable Rust values, independent of the desktop and agent frontends. `Camera::capabilities()` queries the live device; `set_control(CameraControlRequest)` validates support/range/step, performs the operation on the owning capture worker and reads the actual state back. `open_with_format(id, CameraFormatRequest)` selects an explicitly supported resolution and rate. Invalid dimensions, nonfinite values, unsupported formats and unsupported hardware controls return errors. Format changes reopen capture and start a distinct calibration epoch. Legacy pixel marks require review/refit for the new geometry; an explicitly captured resolution reference can scale them for matching aspect ratio, configured FOV and projection. This does not detect physical lens changes or undocumented sensor crops, and metadata-free legacy files are never silently normalized.

`Camera::open(id, fps)` retains the default preference for a bounded format near 640×480. Explicit format requests use exact advertised dimensions instead of silently choosing another resolution. Automatic FPS is `null` in the shared request. Windows/Linux choose from the complete advertised set, including mono and MJPEG-only devices; they do not require a YUYV mode or an exact 640×480 default size. JPEG decoding uses Rust code with checked image dimensions and allocation limits. Capture is bounded to 8 million pixels and 8192 per axis. The macOS adapter sets native CoreVideo output dimensions, filters transitional buffers and applies exact rational frame duration after the session configuration commits. No Rust image scaling conceals a negotiated size.

Capabilities contain the configured output dimensions/rate, bounded supported formats and frame-rate ranges, supported exposure modes, numeric exposure/gain metadata, and device-specific warnings. Numeric controls expose `value`, `minimum`, `maximum`, `step`, `unit`, `writable` and `unsupported_reason`. Missing bounds remain unknown. Read-only, inactive, disabled and automatic numeric controls cannot be set through this API. A successful setter returns readback; accepting a GUI worker command into its queue is separately an asynchronous acknowledgment.

Linux enumeration now excludes metadata-only UVC nodes and preserves the actual
image-device indices. The supplied four Arducam image sources are 0/2/4/6.
Exposure menus preserve their advertised entries instead of converting sparse
menus into continuous ranges. In particular, a device advertising only manual
`1` and aperture-priority automatic `3` must never receive unsupported entry `0`.
The automatic setter preserves the current automatic mode where possible and
verifies the selected entry through native readback.

When the native backend supplies capture timestamps, the adapter now preserves
their relative intervals, anchored once to session elapsed time. Draining queued
images after an encoding or rendering stall no longer assigns them the short
delivery intervals. A backend without capture timestamps retains the host
elapsed-time clock. A backward/discontinuous capture clock produces an actionable
reconnect error instead of silently changing measured time. Linux delivered
sequence numbers still count images received by the adapter; their zero-drop
counter does not establish that the sensor/USB driver lost no images.

## Platform boundaries

- **macOS / AVFoundation:** supported automatic and locked exposure modes are queried through `isExposureModeSupported:`. A camera may expose neither mode. The macOS SDK marks numerical `exposureDuration`, `ISO` and `setExposureModeCustomWithDuration:ISO:completionHandler:` unavailable on macOS; numerical exposure/ISO are consequently explicit unsupported capabilities. The API does not call unavailable selectors or emulate manual exposure in image processing. See Apple's [device exposure documentation](https://developer.apple.com/documentation/avfoundation/avcapturedevice/exposureduration) and the availability annotations in `AVCaptureDevice.h` in the installed macOS SDK.
- **Windows / Media Foundation via Nokhwa:** advertised uncompressed and MJPEG formats and numeric exposure/gain are exposed where the driver permits them. Exposure uses native `log2(seconds)` units; gain uses driver units. Nokhwa's current backend preserves the existing automatic/manual flag when setting a value and does not expose a flag-changing setter. Exposure mode changes therefore return an actionable unsupported error. The dependency currently omits exposure bounds on some controls; missing bounds are reported as unknown, with final validation delegated to the driver. See [Nokhwa's camera API](https://docs.rs/nokhwa/latest/nokhwa/struct.Camera.html) and [Microsoft's exposure contract](https://learn.microsoft.com/en-us/windows/win32/api/strmif/ne-strmif-cameracontrolproperty).
- **Linux / V4L2 via Nokhwa:** the standard [`GREY` mono format](https://docs.kernel.org/userspace-api/media/v4l/pixfmt-yuv-luma.html) is supported, alongside YUYV, RGB/BGR, NV12 and MJPEG. A stalled MMAP frame read fails after two seconds so the source can close and automatic startup can fall back to the sample. Reconnect starts a fresh stream; timed-out MMAP queues are not retried. Standard absolute exposure is preferred when exposed (`100 microseconds` per native tick); older exposure and gain controls retain driver units. Standard auto/manual exposure is available only when reported by the device. Unavailable or inactive controls remain unsupported. Nokhwa's format abstraction supports integer FPS; fractional explicit FPS requests return an error instead of rounding. See the [V4L2 camera controls](https://docs.kernel.org/userspace-api/media/v4l/ext-ctrls-camera.html).

## If the preview does not start

Select the named camera and check its negotiated dimensions/rate in diagnostics.
Use automatic settings first; explicit format requests must match an advertised
mode. A Linux stream that supplies no frames reports an error and can be
reconnected; ordinary startup can use the sample instead.
The two-second limit bounds frame polling; device setup and shutdown still
depend on the operating system's camera driver responding.

A camera already owned by another rig is labelled with that rig in the chooser.
Selecting it reports the conflict before changing the working preview. Selecting
the current camera retains its existing capture/tracking worker. A failed open
without an image displays its actual camera error and reconnect advice.

An occupied live-output port no longer prevents preview, tracking or recording.
The error remains visible and actual output status stays disabled. Stop an old
Python republisher or choose a free port, then Apply to reconnect; sending an
edit to the worker is not yet confirmation that the port opened.

## Ubuntu 22.04 Arducam exposure check — 2026-10-26

The local C++ `CVSource.cpp` forces MJPEG, 320×240 and 100 FPS; it does not set an
explicit exposure duration. The current VR1–VR4 configuration files select that
same format. A native VR3 probe verified automatic → manual → automatic control
and readback, then restored the original automatic mode and exposure value.
Each stage retained all 169–178 processed images as accepted measurements.

| Exposure setting | Full-image mean luma / 255 | Luma standard deviation / 255 | Median map MSE |
|---|---:|---:|---:|
| Original automatic | 75.75 | 55.62 | 4441.63 |
| Nominal 1 ms | 31.84 | 11.14 | 6705.71 |
| Nominal 2 ms | 40.53 | 21.40 | 6504.66 |
| Nominal 5 ms | 61.75 | 42.75 | 4416.16 |
| Nominal 10 ms | 76.37 | 55.96 | 3954.12 |

Durations use the driver's reported 100-microsecond absolute-exposure units;
they are not an independent shutter-timing measurement. The scene was not a
controlled repeated motion, so the table compares signal/contrast and control
functionality, not physical motion accuracy. Very short exposure reduces signal
under the present lighting. The rig configs therefore retain the original
exposure policy; the recorded-video benchmark tests existing blur directly.
The private evidence is `runs/exposure-vr3-v2/report.json`, with native snapshots,
all-frame data and the restored readback.

## Historical Mac evidence

`runs/camera-controls-native/` contains metadata from the built-in FaceTime HD camera on this Mac, using the pure Rust `camera_probe` example in an ad hoc signed local app. No camera pixels were stored. The device advertises seven bounded formats with 15–30 fps ranges. Explicit 640×480/30 and 1280×720/15 were opened and delivered actual frames; status readback and errors are retained in the JSON reports. This built-in camera exposes no supported automatic/locked exposure modes through AVFoundation, so both mode requests and numerical exposure/ISO requests were rejected without interrupting frame delivery. This is evidence of honest unsupported behavior, not evidence of working manual exposure on another camera. Concurrent builds can cause dropped delivered intervals; these probes are configuration/functionality checks, not camera throughput benchmarks.

Native preflight CI passes on Windows, Mac and Ubuntu **22.04/24.04** userspace. The Linux builds exercise the shared camera-format/JPEG tests and the vendored canonical `GREY` mapping regression. A single Ubuntu 22.04-built package has also been installed and run on both Ubuntu versions: the installed and portable CLIs process all 300 reference frames, native Xvfb/Mesa previews visibly render the supplied sample, and startup with an unavailable camera falls back to that sample. [The preflight source, exact ABI, numerical checks and screenshot evidence](benchmarks/ubuntu22-compatibility-20261026.json) record the scope. These container checks use software graphics and no V4L hardware; physical Windows/Linux camera and GPU-driver behavior remains untested here. The macOS capture implementation is unchanged by this Linux compatibility fix. Camera controls never establish physical sphere-tracking accuracy.
