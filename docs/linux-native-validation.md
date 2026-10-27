# Native Linux rigs, rendering and replay validation

These local checks ran on Ubuntu 22.04.5 x86-64, kernel 6.8.0-124, with an
Intel i7-6700K and an Intel HD 530 / NVIDIA GTX 1060 hybrid graphics setup.
The Rust release build uses thin LTO and one codegen unit. Exact commands,
binary/config hashes, bridge replies and captured images remain in the local
ignored `runs/` evidence directories; the numerical summary is retained in
`docs/benchmarks/linux-native-20261026.json`.

## Installed resize failure and tested graphics path

The installed older application panicked during resize at 21:56:07 local time
on 2026-10-26. The user journal reports `egui-wgpu 0.33.3` configuring a surface
that no longer advertises support for its selected adapter. This is a real
rendering failure, independently of the tracking engine and image geometry.

A GLES-only experiment under native Wayland also stalled. A debugger located
the main thread inside Mesa EGL presentation, waiting indefinitely in
`wl_display_dispatch_queue`; the camera worker continued running. Linux GLES in
this version of wgpu advertises FIFO presentation only, so merely requesting
non-vsync presentation does not fix that path.

The tested configuration uses **X11/XWayland with GLES**, reporting the actual
GPU as `NV136 (Gl, Other)`. In v0.3.1, when `DISPLAY` is available, ordinary startup
forces that configuration. Before creating windows or capture workers, the
same executable replaces its process image with a private environment that
sets `WAYLAND_DISPLAY` empty and removes `WAYLAND_SOCKET`. This aligns winit's
window connection with wgpu's independent EGL-display probing; the command
arguments and standard streams are retained. It adds no external decoder,
media runtime or unsafe application code.

Explicit `WINIT_UNIX_BACKEND=x11|wayland` and `WGPU_BACKEND` choices remain
available. Native Wayland-only installations retain automatic backend selection.
Those alternatives have not passed this machine's complete matrix, and forcing
the earlier driver paths can reproduce their failures. Other platforms retain
their existing graphics defaults. These tests establish behavior on this host,
not a guarantee for every display server or driver.

## Surface-compatible graphics fallback in current source

A later user report on the installed Debian package reached X11/XWayland but
failed before creating the tracking window: OpenGL was incompatible with the
window surface and Vulkan had not been requested. The same user confirmed that
`WGPU_BACKEND=vulkan hejjetrac` opened the application. The old forced-OpenGL
workaround was therefore too restrictive for that system.

Current source enumerates OpenGL and Vulkan for implicit X11 startup. A native
selector first checks the actual window surface and supported framebuffer
formats. Compatible OpenGL remains preferred, including over a discrete Vulkan
GPU, preserving the earlier tested hybrid-driver path. Vulkan is selected only
when there is no eligible OpenGL adapter. Power preference applies within the
preferred backend; explicit backend masks and adapter-name filters remain
explicit. Native Wayland and Mac/Windows defaults retain their existing paths.
No capture, calibration, numerical or recording behavior is changed.

Policy regressions cover compatibility, GL-first priority, Vulkan fallback,
explicit masks, adapter names and power preferences. The extended Linux package
harness checks baseline GL rendering, then disables EGL vendor discovery and
restricts Vulkan to Mesa Lavapipe. Forced GL must fail cleanly in that environment;
automatic startup must select Vulkan and render the patterned sample. The same
full-video, quality, numeric, frame-count and screenshot-texture checks apply.
Execution results are recorded separately after the candidate runs; software
rendering is not a physical hybrid-GPU or Wayland validation claim.

## Complete VR3 replay through the native application

The actual desktop JSONL bridge processed all **5,157** images from the supplied
320×240, 100-fps VR3 recording. Playback was source-paced. Ten real window-size
and diagnostic-view transitions included the high-speed interval and the hand
interval. The run finalized all numeric and quality files and closed normally.

Every one of the **25 numeric columns matched the same-build headless output
exactly**, including the entire trajectory, integrated displacement, wrapped
heading, clocks, fit errors and recovered motion. Every acceptance/status
decision also matched. Rotation-component RMSE, trajectory-point RMSE and
wrapped-heading RMSE between native and headless outputs were all zero; the
comparison gate was 1e-9, with no rows omitted.

The headless reference is the precise full-ROI, late-maturity candidate described
in [the VR3 accuracy assessment](vr3-validation.md). Exact native/headless
agreement confirms the GUI uses that engine; it does not turn the upstream
recording into physical ground truth or remove the separately reported
upstream trajectory differences. Native evidence is in
`runs/native-vr3-validated/`.

## Four physical cameras with ZeroMQ load

All four Arducam image devices opened at **320×240, nominal 100 fps**, using the
provided per-rig geometry and exclusions. Camera IDs remained 0, 2, 4 and 6.
Metadata-only UVC nodes are excluded from the camera chooser. Titles distinguish
VR1–VR4, the applied output port and the actual device identifier.

A separate native regression starts at **VR3**, then invokes the same
`spawn_from` path used by Open four rigs three times. Both `VR3/config.txt`
folder layouts and flat `VR3.txt` layouts select the saved rigs in the order
**VR3 → VR4 → VR1 → VR2**, opening camera nodes **4 → 6 → 0 → 2**. All four
sessions deliver actual images and tracking rows; the four source images were
inspected and the app closed normally. This avoids the previous fallback to a
metadata-only camera node after reaching VR4. Evidence is in
`runs/native-open-four-final/` and the reproduction script covers both layouts.

The actual native-window test exercised:

- selecting the already active camera without restarting its tracking epoch;
- rejecting a camera owned by another window while retaining the working source;
- switching every camera to video and back, with the other workers advancing;
- pausing one camera while the others continued;
- 30 size/diagnostic transitions per window, **120 transitions total**;
- holding all windows at minimum size for 15 seconds;
- 60 seconds of continued processing and data-only recording, followed by clean close.

Four independent publishers used private loopback ports 19871–19874, alongside
four healthy real-libzmq subscribers, four subscribed clients whose application
queues were never drained, and repeated TCP clients that withheld their ZMTP
greetings. The malformed clients made **178 connections** in total. The expected
handshake deadlines were reported; all publishers remained alive and reported
zero application-queue discards.

| Rig | Processed frames/s during the 60-second observation | Recorded/received exact accepted rows |
|---|---:|---:|
| VR1 | 100.926 | 6,077 / 6,077 |
| VR2 | 100.876 | 6,076 / 6,076 |
| VR3 | 100.926 | 6,077 / 6,077 |
| VR4 | 100.876 | 6,077 / 6,077 |

All **24,307** recorded rows in that observation were marked tracked and
accepted; all were received with exact native FT bytes, in order and without
duplicates. Numeric and all-frame quality counts agreed. This is an observed
short stress case, including independent subscribers and local system activity,
not an isolated encoder benchmark or an unlimited camera-capacity claim.
The native delivered-sequence counter cannot establish zero sensor/kernel frame
loss. Raw and diagnostic video encoders were disabled in this case; their load
is assessed separately. Evidence is in `runs/native-four-zmq-final/`.

## Diagnostic outlines and recording choices

The full Debug viewer now respects the calibration-outline checkbox. Actual
native framebuffer captures at the identical paused VR3 source frame show no
calibration guides when disabled and the saved rim, exclusions and axes when
enabled. Their numeric state is identical. The currently edited calibration
layer remains visible during calibration. This display choice does not change
the independent calibrated diagnostic-video renderer. Native captures are in
`runs/native-outlines/`; render regressions cover both preview and full Debug.

New sessions initialize their recording choices from `save_raw` and
`save_debug`. Opening a different config refreshes those choices. Ordinary
source/numerical/view edits preserve manual transient choices, without silently
rewriting the canonical recording flags.

## Display rate, codec choice and metadata freshness

The final guarded release, with a media queue capacity of eight, was checked on
2026-10-27 with the actual VR3 camera,
using separate display and recording controls in the native inspector. The
recording speed/size control selected raw AVI without editing the canonical
config; its UI label distinguished uncompressed files and disk bandwidth.

| Display limit | Measured image publications/s | Processed frames/s |
|---|---:|---:|
| 1 fps | 0.998 | 101.300 |
| 60 fps | 50.363 | 101.474 |

The source remained at nominal 100 fps. Frame/clock counters and media/output
status now publish independently at about 30-ms intervals, while source/debug
images and displayed-path snapshots retain their selected cadence. A real
300-frame regression verifies that metadata advances while the immutable image
and path snapshots remain unchanged between 1-fps image updates; all numerical
rows and the final source pixels are preserved.

The native run finalized **853** raw-video frames with **853** numeric and
quality rows; an independent complete decode confirmed the same count. Twelve
tiny/wide/portrait/normal transitions passed during that recording. A request
above the current framebuffer bound failed before changing the view. Native
framebuffer captures show both sliders, the selected codec and separate rates;
full Debug respects hidden calibration guides. Evidence is in
`runs/native-display-final-queue8/`, using desktop SHA-256
`2a34a9bc2da67cf5e8f9e4678276f08905eead41b3e1e8fa3b0d007f0c0e6ec0`.
This is a short functional case, separate
from the serial recording-capacity assessment.
The earlier guarded candidate (`65b69073…68f97`) independently passed 846
aligned frames; its separate results remain in the benchmark JSON and
`runs/native-display-final-metadata/` and are not presented as this final binary.

## Reproduce the native checks

Use new output directories. The optional load case requires PyZMQ as an
independent test client; the production application remains Rust.
The rig harness also accepts `--display-fps 1 15 30 60` to assign independent
limits to four windows and check that camera/source rates remain unchanged.

```sh
python3 scripts/test_native_replay.py \
  --config runs/vr3/precise-late-config.txt \
  --video /path/to/VR3/fictrac-raw-20261026_212053.avi \
  --headless runs/vr3/precise-late \
  --output runs/native-vr3-check

python3 scripts/test_native_rigs.py \
  --configs VR1.txt VR2.txt VR3.txt VR4.txt \
  --output runs/native-four-zmq-check \
  --seconds 60 --tiny-seconds 15 --resize-cycles 3 \
  --reconnect-cycles 1 --zmq-base-port 19871

python3 scripts/test_native_open_four.py \
  --rig-directory . --output runs/native-open-four-check

python3 scripts/test_native_display.py \
  --config VR3.txt --output runs/native-display-check --capture-inspector
```
