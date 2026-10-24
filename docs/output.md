# Output contract

`tracking.dat` has comma-separated accepted rows in legacy mode, matching upstream rejection omission; modern JSON mode retains every row. `all-frames.dat` and `quality.csv` always preserve every processed frame and decision. Data files have no header. Floating-point values use twelve decimal places; frame/sequence fields are integers.

| Columns (1-based) | Meaning |
|---|---|
| 1 | Frame counter; legacy mode starts at 0, modern physical-axis mode at 1 |
| 2–4 | Delta rotation, exported camera/ROI axis-angle, radians |
| 5 | Map mode: binary atlas mean squared error (0–65025 on a supported fit). Pairwise mode: normalized luminance RMSE. No cross-engine threshold equivalence |
| 6–8 | Delta rotation, lab axis-angle, radians |
| 9–11 | Accumulated camera axis-angle rotation, radians |
| 12–14 | Lab orientation vector for `R_camera_to_lab × R_absolute_camera`, matching upstream convention |
| 15–16 | Integrated world forward/east path coordinates in sphere radians |
| 17 | Animal heading in [0, 2π), radians |
| 18 | Instantaneous body movement direction in [0, 2π), radians |
| 19 | Body translational rotation magnitude, radians/frame |
| 20–21 | Integrated forward/side motion without heading, sphere radians |
| 22 | Presentation timestamp relative to first displayed video frame, milliseconds |
| 23 | Independent relative sequence counter, starts at 0 and restarts on pose/path reset; frame counter continues |
| 24 | Timestamp delta, milliseconds; zero for first frame. Accepted legacy rows span time since the previous accepted row; rejected evidence and modern rows use the processed-frame interval |
| 25 | Offline: same video-relative timestamp as column 22. Camera: native/host capture time as provided by its adapter |

Physical camera axes are right, down, forward. Legacy imports preserve upstream's recentered ROI basis for its exported “camera” rotations, then apply the saved `c2a_r` directly. This convention matters for off-axis balls; do not rotate the legacy calibration again. Lab axes: animal forward, animal right, animal down. World axes start aligned with lab. Plot column 16 horizontally and column 15 vertically to obtain east right/north up. Multiply position or speed by sphere radius for physical length. For physical velocity per second, also divide speed by column 24 / 1000. Never divide the first row's speed by its zero delta time.

`quality.csv` always accompanies the data. `initializing` is the first reference frame and has zero motion; `tracked` is an accepted relative estimate; `relocalized` is an accepted absolute-pose recovery with zero relative motion; `rejected` freezes integrated pose and writes zero deltas only to all-frame evidence. Legacy `.dat` and wire outputs omit rejected frames. A reset preserves integrated displacement and total source count while resetting pose/path/heading and sequence. `accepted` is true for initialization, meaning the frame initialized successfully, not that it contains a motion measurement. The error score on a rejected row describes the attempted fit before zeroing its motion. Frozen position across a rejected interval represents missing measurement, not proof that the animal stood still.

The standard `.dat` format has no validity field, so use quality.csv alongside it. Pairwise mode advances its reference image on a rejection; map mode leaves integrated orientation unchanged and does not contaminate the atlas with a rejected image. Prediction resets to zero. The next local estimate or optional global relocalisation can resume, but cannot reconstruct the lost interval. Absolute pose and path therefore omit that motion.

MP4 B-frames are decoded into presentation order and input timestamps are used. Non-monotonic or missing decoded PTS fail the run. CLI tracking uses input timestamps. The desktop additionally supports an explicit `src_fps` override and camera capture timestamps; it never guesses an unknown video rate. Y4M timestamps use its rational frame-rate header. A max-frames run records only the processed prefix. Metadata may report fps=null when nominal frame rate is absent even though presentation timestamps are valid.

Outputs require a new directory and use create-new files. Missing parents are created automatically; an existing run directory is still refused. Data and quality files are buffered, flushed and synced before promotion from `.partial`. SVG and JSON are flushed before promotion. Completion promotes data, all-frame data, quality, SVG and JSON files individually, so a filesystem failure during final renaming could leave a mixture of final and partial artifacts; this is not an atomic directory transaction. A filesystem/power failure or interrupted process can leave partial files. The command returns an error on observed I/O failures and never overwrites a prior run.

The SVG preview bounds its path memory at 100000 points by decimating older points. Full numeric data is always streamed without decimation. Its coordinates are labelled in sphere radians; the plot automatically fits its view.

Desktop recording uses epoch-numbered data, quality and config files in a new directory. Applying settings resets the estimator and starts a new epoch. The screen path can decimate older points without changing streamed numeric data. The legacy `save_raw`/`save_debug` parameters encode real videos: default H.264 AVI, XVID AVI, MPEG-4 MP4, MJPEG/raw AVI or explicit H.264 MP4 (`vid_codec : h264_mp4`). Shared calibrated debug recording includes input/rim/exclusions/axes/contact trace and actual maps/path. H.264 uses High profile with independent frames (GOP1); compressed payloads may be larger than inter-frame encoding. Source FPS comes from metadata or prefetched PTS; no frames/RGB are dropped by inference. Encoded files remain `.partial` until finalization and never overwrite another recording. H.264 encoding preserves even display dimensions through standard SPS cropping; odd 4:2:0 H.264 dimensions fail explicitly. Segmented OpenDML AVI preserves indexes and frames across 4 GiB boundaries. UDP sends the upstream `FT, ` prefix and a 25-column row, without delivery guarantees.

Serial and UDP receive the same accepted-only newline-terminated `FT, ` plus full 25-column row, including the independent sequence counter. Serial defaults to 115200 baud, 8-N-1, no flow control. Output failures remain visible and do not manufacture measurements. Headless summary separates tracked, relocalized, initializing and rejected counts. Native GUI JSONL acknowledgments are queued; inspect actual revision/frame/advance/recording status before reporting completion.
