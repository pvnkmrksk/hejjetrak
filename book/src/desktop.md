# The tracking window

The regular view keeps experiment monitoring separate from configuration. It shows the source around the ball, the contact trace/path and a small toolbar. Images use uniform scaling: a crop may change the visible context, but it does not stretch the source aspect ratio.

![Compact native preview](images/preview.png)

## Everyday controls

- **Reset tracking** starts a fresh pose/map/path epoch while retaining source time and integrated displacement. Motion during rejected intervals remains missing. Reset does not rewind a video.
- **Record** starts or stops a take immediately. The adjacent selection chooses data only, raw source, processed diagnostics or both videos. Configured `save_raw` and `save_debug` still apply when recording starts from config.
- **Source** opens the camera list and video selection. Drag-and-drop selects a video without a shell command.
- **Settings** opens a separate native inspector. Calibration, exact config text, numerical options and output destinations live there.
- **Diagnostics** reveals the traditional six-panel view. Full-source view and calibration guides are separate display selections; they do not change tracking.

Hover icon controls for their action and the status strip for timing/quality details. As the preview shrinks, controls progressively reduce to Reset and Record, then Reset alone at the smallest 96×96 preview. The **Full image** source-context button switches between a full image and the cropped ball without changing aspect ratio or numerical geometry. Native title bars identify the VR name, actual applied source, available camera name and active UDP/ZMQ ports (not an unapplied draft); window size and location can be controlled from the command line for tiled rigs.

![Actual tiny native sample preview](images/tiny-preview.png)

A tiny sample preview keeps its essential control visible; expanding the window restores the additional toolbar controls.

## Diagnostic views

![Computed diagnostic panels](images/diagnostics.png)

The mosaic shows the input, thresholded sphere ROI, warped difference, instantaneous map, accumulated confidence map and flat path. These are derived from the actual tracker state. They are diagnostic measurements, not an independent physical-ground-truth check. Rejected/recovering frames keep their error/quality decision; a frozen displayed path does not mean measured zero velocity.

## Timing readouts

`ms track` is the mean shared-engine call duration, including ROI preparation and numerical map work. It excludes camera waiting, pixel conversion, engine construction, recording and preview publication. Worker processing cost includes more surrounding work. Delivered FPS measures arrivals/processed delivery, not numerical capacity in isolation. The mean restarts after a calibration or tracker reset.

A source running at 30 Hz can remain on schedule while taking several milliseconds of CPU per frame. Conversely, a fast tracking call cannot prevent drops caused by a saturated camera bus or encoder. Use the reported capture skips and output queue counters alongside latency. Details are in [tracking performance](tracking-performance.md) and [measurement semantics](output.md).

## Settings and state

The inspector groups source/camera, calibration, geometry, tracking/quality, outputs and raw configuration. Canonical config text preserves comments, unknown fields, formatting and line endings. Field and graphical edits share undo/redo. Saving is explicit. An external disk conflict is reported rather than silently overwriting either copy.

Numerical edits create a new tracking epoch without seeking the unchanged video source. A recording is closed rather than mixing two calibrations in one epoch. Reopen a source or rewind explicitly when a fresh timeline is wanted.

## Monitoring without busy work

A paced source waits on command-interruptible deadlines rather than repeatedly polling for its next frame. Paused/end-of-file workers block with an interruptible health heartbeat. Each file decoder queue and staged frame are bounded; this does not skip file frames or change their timestamps.

Numerical processing and quality decisions still happen for every processed frame. Preview/diagnostic snapshots are published on the existing roughly 30 ms display cadence, rather than cloning the full view at a 120/240 Hz camera's numerical rate. Recording borrows current full numerical views for every recorded frame, so display coalescing does not decimate scientific output or recorded diagnostic data. Independent sessions use independent workers; the normal fitter does not fan out into a large thread pool.

Native status exposes `resources.preview_updates`, `resources.idle_waits` and `resources.pacing_waits` to distinguish publication work from waiting. These counters are not CPU or energy measurements. Resource claims require the measured source/rate/recording mode and machine; a GPU used for display does not imply a GPU-based tracker.
