# Output contract

The accelerated local search samples a deterministic subset of valid ROI sites
(at most about 1024 for ordinary budgets). The selected candidate and prediction
are then scored on **all valid sites**; the lower complete score drives the
reported map MSE and `opt_max_err` decision. These checks are reserved within
`opt_max_evals`. Small ROIs/budgets retain complete local search. Legacy global recovery retains its existing search. Default resilient recovery
uses bounded accepted-view seeds and a deterministic fallback cover, with
complete final scoring; see [recovery](recovery.md).
ROI resolution, map evidence, coordinate conventions and missing-motion rules
are unchanged. Trajectories can differ from the earlier full-site optimizer;
accuracy is assessed with the unchanged trajectory/rotation reference gates.

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

`quality.csv` always accompanies the data. `initializing` is the first reference frame and has zero motion; `tracked` is an accepted relative estimate; `relocalized` is an accepted absolute-pose recovery with zero relative motion; `rejected` freezes integrated pose and writes zero deltas only to all-frame evidence; `recovering` is also rejected (`accepted = false`) while an absolute-pose candidate is being sought or awaiting a second-frame confirmation. Both remain in all-frame data/quality and count as rejected in the CLI summary. Legacy `.dat` and wire outputs omit rejected frames. A reset preserves integrated displacement and total source count while resetting pose/path/heading and sequence. `accepted` is true for initialization, meaning the frame initialized successfully, not that it contains a motion measurement. The error score on an attempted rejected/recovering fit is its complete-ROI binary map MSE before zeroing motion. Textureless or insufficient-texture images rejected before optimization use the maximum binary error 65025; insufficient atlas support uses the finite failure score 1e12. These scores are quality evidence, not measured movement. Frozen position across a rejected interval represents missing measurement, not proof that the animal stood still.

The standard `.dat` format has no validity field, so use quality.csv alongside it. Pairwise mode advances its reference image on a rejection; map mode leaves integrated orientation unchanged and does not contaminate the atlas with a rejected image. Prediction resets to zero. Default resilient mode protects the learned atlas instead of applying `max_bad_frames` auto-reset, and requires two consecutive plausible absolute matches before `relocalized`. Its adaptive accepted-history MSE ceiling supplements any explicit `opt_max_err`; set `rust_resilient : n` for the legacy quality/reset behavior. Bounded reacquisition or the legacy optional global relocalisation can resume, but cannot reconstruct the lost interval. Absolute pose and path therefore omit that motion.

MP4 B-frames are decoded into presentation order and input timestamps are used. Non-monotonic or missing decoded PTS fail the run. CLI tracking uses input timestamps. The desktop additionally supports an explicit `src_fps` override and camera capture timestamps; it never guesses an unknown video rate. Y4M timestamps use its rational frame-rate header. A max-frames run records only the processed prefix. Metadata may report fps=null when nominal frame rate is absent even though presentation timestamps are valid.

Outputs require a new directory and use create-new files. Missing parents are created automatically; an existing run directory is still refused. Data and quality files are buffered, flushed and synced before promotion from `.partial`. SVG and JSON are flushed before promotion. Completion promotes data, all-frame data, quality, SVG and JSON files individually, so a filesystem failure during final renaming could leave a mixture of final and partial artifacts; this is not an atomic directory transaction. A filesystem/power failure or interrupted process can leave partial files. The command returns an error on observed I/O failures and never overwrites a prior run.

The SVG preview bounds its path memory at 100000 points by decimating older points. Full numeric data is always streamed without decimation. Its coordinates are labelled in sphere radians; the plot automatically fits its view.

Desktop recording uses epoch-numbered data, quality and config files in a new directory. Applying settings resets the estimator and starts a new epoch. The screen path can decimate older points without changing streamed numeric data. The legacy `save_raw`/`save_debug` parameters encode real videos: default H.264 AVI, XVID AVI, MPEG-4 MP4, MJPEG/raw AVI or explicit H.264 MP4 (`vid_codec : h264_mp4`). Shared calibrated debug recording includes input/rim/exclusions/axes/contact trace and actual maps/path. H.264 uses High profile with independent frames (GOP1); compressed payloads may be larger than inter-frame encoding. Source FPS comes from metadata or prefetched PTS; no frames/RGB are dropped by inference. Encoded files remain `.partial` until finalization and never overwrite another recording. H.264 encoding preserves even display dimensions through standard SPS cropping; odd 4:2:0 H.264 dimensions fail explicitly. Segmented OpenDML AVI preserves indexes and frames across 4 GiB boundaries. UDP sends the upstream `FT, ` prefix and a 25-column row, without delivery guarantees.

Serial and UDP receive the same accepted-only newline-terminated `FT, ` plus full 25-column row, including the independent sequence counter. Serial defaults to 115200 baud, 8-N-1, no flow control. Output failures remain visible and do not manufacture measurements. Headless summary separates tracked, relocalized, initializing and rejected counts. Native GUI JSONL acknowledgments are queued; inspect actual revision/frame/advance/recording status before reporting completion.

## Simultaneous UDP and ZeroMQ PUB

Hejjetrak can publish the same accepted measurement over UDP and ZeroMQ at the same time. Set the ordinary `sock_host`/`sock_port` for UDP and these additional legacy-text fields for ZeroMQ:

```text
zmq_host           : 127.0.0.1
zmq_port           : 9871
zmq_queue_capacity : 256
```

`zmq_port : -1` or `0` disables the publisher. `zmq_host` is a loopback bind address: `localhost`, `127.0.0.1`, other loopback IPs, or `::1`. The TCP endpoint is a **PUB bind**; subscribers connect to it. Each ZeroMQ message contains exactly one UTF-8 frame: the identical newline-terminated `FT, ` prefix and complete 25-column row used for UDP and serial. There is no multipart topic, JSON wrapper, added column or coordinate conversion. A subscriber can filter on the byte prefix `FT, `. Rejected measurements remain in `all-frames.dat` and `quality.csv` and are omitted from both live transports, preserving the legacy decision.

The publisher implements ZMTP entirely in Rust (`zeromq`); it does not link libzmq or launch another process. A dedicated worker owns the socket. Tracking submits the already formatted row with a nonblocking bounded queue operation. Capacity defaults to 256 rows and accepts 1–4096; a row is limited to 16 KiB. A full queue discards that live row and reports the drop while numeric data recording continues. UDP and ZeroMQ are independent best-effort outputs: one output's failure does not turn a measurement into zero motion or stop the numerical engine.

The output status includes the endpoint, queue capacity/current length, enqueued rows, completed PUB send attempts (`published`), application discards (`dropped`), failure count, worker state and last error. `published` does **not** mean a subscriber received or acknowledged a row. Like normal ZeroMQ PUB/SUB, subscribers joining late miss earlier messages; subscription setup takes time, and there is no replay. Subscriber or network losses below the application queue are not included in `dropped`. The independently sampled counters can briefly reflect different instants.

A transport send has a 25 ms deadline. A stalled send stops and closes the publisher, so a cancelled partial ZMTP frame is never reused; tracking continues and status identifies the failure. Reapply the output configuration to restart it. Closing a publisher drains queued rows for at most approximately 75 ms before discarding any remainder; runtime teardown has a further 50 ms bound. Neither closing nor a completed run guarantees subscriber delivery. A port already in use fails configuration with the exact endpoint and a request to choose another port.

Configuration edits reuse healthy output handles when transport settings are unchanged. Changing an output releases exclusive handles before reopening them. If the new settings fail, the previous settings are restored and the edit reports the failure; restoring a PUB starts a fresh socket and counters, so subscribers must reconnect and can miss rows. If restoration also fails, the error explicitly identifies the rollback failure and outputs remain disabled until valid settings are reapplied. Numeric tracking state is independent of this output transaction.

The root `VR1.txt`–`VR4.txt` files keep the original BR1–BR4 camera IDs (`0, 2, 4, 6`) and saved calibrations. Their UDP ports are `1111`–`1114` and PUB ports are `9871`–`9874`, all on localhost. These are device-specific saved calibrations, **not** a claim that a different camera or resolution is calibrated. Check source dimensions and draw/refit geometry for the actual setup. Opening another instance selects the next existing numbered configuration. When no next file exists, the copy increments its numeric camera ID, enabled UDP/PUB ports and `rust_instance_name`; a video path stays a video path and disabled outputs stay disabled. Copies have no saved path, so saving cannot overwrite the parent configuration. Port or label-number overflow fails explicitly.

For an optional external Python subscriber (Python/pyzmq is not an application dependency):

```python
import zmq

context = zmq.Context()
subscriber = context.socket(zmq.SUB)
subscriber.setsockopt_string(zmq.SUBSCRIBE, "FT, ")
subscriber.connect("tcp://127.0.0.1:9871")
while True:
    message = subscriber.recv_string()
    columns = message.removeprefix("FT, ").strip().split(",")
    assert len(columns) == 25
```
