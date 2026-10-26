# The tracking window

The regular view keeps experiment monitoring separate from configuration. It shows the source around the ball, the contact trace/path and a small toolbar. Images use uniform scaling: a crop may change the visible context, but it does not stretch the source aspect ratio.

![Compact native preview](images/preview.png)

## Everyday controls

- **Reset tracking** starts a fresh pose/map/path epoch while retaining source time and integrated displacement. Motion during rejected intervals remains missing. Reset does not rewind a video.
- **Record** starts or stops a take immediately. The adjacent selection chooses data only, raw source, processed diagnostics or both videos. Configured `save_raw` and `save_debug` still apply when recording starts from config.
- **Source** opens the camera list and video selection. Drag-and-drop selects a video without a shell command.
- **Settings** opens a separate native inspector. Calibration, exact config text, numerical options and output destinations live there.
- **More** in current source opens the rig manager, with per-rig config views, **Start all** and grid arrangement. **More → Appearance** offers System, Light and Dark themes; **About HejjéTrac** gives author and license details.
- **Diagnostics** shows the complete input with the saved calibration, followed by the traditional numerical panels. **Redraw ball**, **Redraw exclusions** and **Redraw lab** replace a single layer. **Full configuration…** opens the exact current config text.

Hover the bundled Lucide icons for their actions. Small windows hide secondary
controls; narrower than 160 pixels or shorter than 120, the preview hides its
entire toolbar/status instead of shrinking fonts. Right-click the image for
Reset, recording, Settings or Calibrate again. **Full image** switches context
without changing aspect ratio or numerical geometry.

Compact titles show VR name, applied ZMQ port and `Camera 0`/video basename.
Larger windows add UDP and the full actual source identity; unapplied drafts do
not relabel running outputs. Closing any individual window leaves the remaining
sessions running. Explicit Quit closes the application. See [several rigs](rigs.md)
for placement and output-isolation details.

![Actual tiny native sample preview](images/tiny-preview.png)

This actual v0.1.3 capture shows the image-only tiny preview. Controls remain available through its context menu.

## Diagnostic views

![Computed diagnostic panels](images/diagnostics.png)

This diagnostic capture is from v0.1.2; the numerical panel layout is retained. The mosaic shows the input, thresholded sphere ROI, warped difference, instantaneous map, accumulated confidence map and flat path. These are derived from the actual tracker state. They are diagnostic measurements, not an independent physical-ground-truth check. Rejected/recovering frames keep their error/quality decision; a frozen displayed path does not mean measured zero velocity.

## Timing readouts

`ms track` is the mean shared-engine call duration, including ROI preparation and numerical map work. It excludes camera waiting, pixel conversion, engine construction, recording and preview publication. Worker processing cost includes more surrounding work. Delivered FPS measures arrivals/processed delivery, not numerical capacity in isolation. The mean restarts after a calibration or tracker reset.

A source running at 30 Hz can remain on schedule while taking several milliseconds of CPU per frame. Conversely, a fast tracking call cannot prevent drops caused by a saturated camera bus or encoder. Use the reported capture skips and output queue counters alongside latency. Details are in [tracking performance](tracking-performance.md) and [measurement semantics](output.md).

## Settings and state

The inspector groups source/camera, calibration, geometry, tracking/quality, outputs and raw configuration. Canonical config text preserves comments, unknown fields, formatting and line endings. Field and graphical edits share undo/redo. Saving is explicit. An external disk conflict is reported rather than silently overwriting either copy.

Numerical edits create a new tracking epoch without seeking the unchanged video source. A recording is closed rather than mixing two calibrations in one epoch. Reopen a source or rewind explicitly when a fresh timeline is wanted.

## MATREX VR output

In **Settings → Output & recording → ZeroMQ publisher**, enable the publisher,
choose its port, then select **Native FicTrac (FT)** or **Legacy MATREX VR (JSON)**.
Native FT remains the default. JSON can use a single frame matching Python, or
**Topic + JSON** for old paired-read Unity receivers; the topic is editable.
The status tooltip reports the applied mode rather than an unapplied draft.

Legacy x/y stay in sphere radians and yaw is in degrees; Unity handles radius
scaling. UDP/serial retain FT rows in either mode. Save your rig config for future
launches. See [MATREX compatibility and validation](matrex-vr.md).

## Monitoring without busy work

A paced source waits on command-interruptible deadlines rather than repeatedly polling for its next frame. Paused/end-of-file workers block with an interruptible health heartbeat. Each file decoder queue and staged frame are bounded; this does not skip file frames or change their timestamps.

Numerical processing and quality decisions still happen for every processed frame. Preview/diagnostic snapshots are published on the existing roughly 30 ms display cadence, rather than cloning the full view at a 120/240 Hz camera's numerical rate. Recording borrows current full numerical views for every recorded frame, so display coalescing does not decimate scientific output or recorded diagnostic data. Independent sessions use independent workers; the normal fitter does not fan out into a large thread pool.

Native status exposes `resources.preview_updates`, `resources.idle_waits` and `resources.pacing_waits` to distinguish publication work from waiting. These counters are not CPU or energy measurements. Resource claims require the measured source/rate/recording mode and machine; a GPU used for display does not imply a GPU-based tracker.
