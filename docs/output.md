# Output contract

Default resilient local search evaluates **all valid ROI sites** throughout
`opt_max_evals`, including blurred/high-speed candidate fitting. With
`rust_resilient : n`, accelerated legacy candidate generation samples a
deterministic subset (at most about 1024 sites for ordinary budgets), then
scores the selected candidate and prediction on all valid sites within that
same budget. The complete score drives the reported map MSE and
`opt_max_err` decision in both modes. Small ROIs/budgets retain complete local
search. Legacy global recovery retains its existing search. Default resilient recovery
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

`quality.csv` always accompanies the data. `initializing` is the first reference frame and has zero motion; `tracked` is an accepted relative estimate; `relocalized` is an accepted absolute-pose recovery with zero relative motion; `rejected` freezes integrated pose and writes zero deltas only to all-frame evidence; `recovering` is also rejected (`accepted = false`) while an absolute-pose candidate is being sought or awaiting consecutive-image confirmation (ordinary two-image confirmation, or the stronger four-image acceleration path). Both remain in all-frame data/quality and count as rejected in the CLI summary. Legacy `.dat` and wire outputs omit rejected frames. A reset preserves integrated displacement and total source count while resetting pose/path/heading and sequence. `accepted` is true for initialization, meaning the frame initialized successfully, not that it contains a motion measurement. The error score on an attempted rejected/recovering fit is its complete-ROI binary map MSE before zeroing motion. Textureless or insufficient-texture images rejected before optimization use the maximum binary error 65025; insufficient atlas support uses the finite failure score 1e12. These scores are quality evidence, not measured movement. Frozen position across a rejected interval represents missing measurement, not proof that the animal stood still.

The v0.1.3 resilient policy also checks accepted-motion consistency using recent
accepted rates, and temporarily defers atlas teaching after relocalization. A
candidate rejected by the motion guard retains its actual complete image MSE,
which can be low; acceptance is not inferred from column 5 alone. Ordinary relocalization
and the following three accepted tracked rows do not teach atlas/error/edge/
known-view histories. Recovery after a four-image consistent acceleration
confirmation starts a fresh motion-rate epoch and defers teaching for eight
following accepted tracked rows. Accepted motion/output semantics stay as
specified above. See [recovery](recovery.md) for bootstrap, timing and probation
boundaries. `rust_resilient : n` retains the older policy.

The standard `.dat` format has no validity field, so use quality.csv alongside it. Pairwise mode advances its reference image on a rejection; map mode leaves integrated orientation unchanged and does not contaminate the atlas with a rejected image. Prediction resets to zero. Default resilient mode protects the learned atlas instead of applying `max_bad_frames` auto-reset, and requires confirmed plausible absolute matches before `relocalized` (ordinary two-image confirmation, or the v0.1.3 four-image acceleration path). Its adaptive accepted-history MSE ceiling supplements any explicit `opt_max_err`; set `rust_resilient : n` for the legacy quality/reset behavior. Bounded reacquisition or the legacy optional global relocalisation can resume, but cannot reconstruct the lost interval. Absolute pose and path therefore omit that motion.

MP4 B-frames are decoded into presentation order and input timestamps are used. Non-monotonic or missing decoded PTS fail the run. CLI tracking uses input timestamps. The desktop additionally supports an explicit `src_fps` override and camera capture timestamps; it never guesses an unknown video rate. Y4M timestamps use its rational frame-rate header. A max-frames run records only the processed prefix. Metadata may report fps=null when nominal frame rate is absent even though presentation timestamps are valid.

Outputs require a new directory and use create-new files. Missing parents are created automatically; an existing run directory is still refused. Data and quality files are buffered, flushed and synced before promotion from `.partial`. SVG and JSON are flushed before promotion. Completion promotes data, all-frame data, quality, SVG and JSON files individually, so a filesystem failure during final renaming could leave a mixture of final and partial artifacts; this is not an atomic directory transaction. A filesystem/power failure or interrupted process can leave partial files. The command returns an error on observed I/O failures and never overwrites a prior run.

The SVG preview bounds its path memory at 100000 points by decimating older points. Full numeric data is always streamed without decimation. Its coordinates are labelled in sphere radians; the plot automatically fits its view.

Desktop recording uses epoch-numbered data, quality and config files in a new directory. Applying settings resets the estimator and starts a new epoch. The screen path can decimate older points without changing streamed numeric data. The legacy `save_raw`/`save_debug` parameters encode real videos: default H.264 AVI, XVID AVI, MPEG-4 MP4, MJPEG/raw AVI or explicit H.264 MP4 (`vid_codec : h264_mp4`). Shared calibrated debug recording includes input/rim/exclusions/axes/contact trace and actual maps/path. H.264 uses High profile with independent frames (GOP1); compressed payloads may be larger than inter-frame encoding. Source FPS comes from metadata or prefetched PTS; no frames/RGB are dropped by inference. Encoded files remain `.partial` until finalization and never overwrite another recording. H.264 encoding preserves even display dimensions through standard SPS cropping; odd 4:2:0 H.264 dimensions fail explicitly. Segmented OpenDML AVI preserves indexes and frames across 4 GiB boundaries. UDP sends the upstream `FT, ` prefix and a 25-column row, without delivery guarantees.

Recorded video uses a constant frame rate: explicit `src_fps` takes priority, then the camera or file's reported nominal rate. A timestamp interval supplies the rate only when nominal metadata is unavailable. A single-frame desktop recording without either rate retains the 25 FPS fallback. Selecting the recording rate does not rewrite numeric or quality timestamps; an explicit `src_fps` override retains its existing retiming behavior. Constant-rate video does not preserve capture jitter or gaps from dropped frames.
The display FPS setting only limits preview publication; it never decimates
processed rows or changes the recording's declared source FPS. Transient
`recording_tuning.encoding` chooses `configured`, `mjpeg` or `raw`, with the
effective codec written into that take's config copy. The canonical rig config
and numerical epoch remain unchanged.

Raw and debug video use separate bounded encoder workers. File replay waits for
space so a completed video contains every processed image. Camera acquisition
uses nonblocking submission: if an optional stream cannot keep up, its existing
FIFO prefix is finalized under the `.partial` filename and that stream stops.
Numeric/quality files, the other video stream and live output continue. A partial
prefix is never promoted or described as a complete take. `raw-status.json`,
`debug-status.json` and final `media-status.json` retain requested/state flags,
numeric/submitted/encoded counts, queue limits, source-frame/timestamp endpoints,
the applied overflow policy, container finalization and warnings. Inspect them alongside all-frame data;
camera delivered counters alone do not establish sensor-level completeness.

For live native Stop/rotation, numeric flush/fsync and encoder drains run in a
single owned background finalizer per camera. The bound is two retired takes
(queued plus running) and the active take; a capacity rejection retains the
active take without creating a new directory or resetting tracking. Files are
ready for verification only after that take's atomic `completion.json`, rather
than a changed active-directory acknowledgment or `recording=false`. The marker
records `directory`, terminal `state`, `numeric_error`, `media_recording`,
`media_warnings` and `completion_error`. `complete` means numeric commit and
finalization finished; optional media can remain explicitly partial/failed.
Inspect its stream states and encoded counts before treating video as complete.
A numeric finalization failure produces a `failed` marker; inability to commit
the marker is exposed in native `recording_finalizations` status. Its recent
history is bounded to 16 entries. Graceful shutdown drains owned jobs after
acquisition stops. File replay finalization and configuration-epoch changes
remain synchronous. Abrupt process termination/power loss can leave a take
without a completion marker.

On Linux, live video workers check available output-filesystem space before
their first frame and at most every 100 ms. Below 512 MiB, or if the check fails,
that optional stream stops as a failed partial prefix, preserving space for
numeric evidence. File replay and other OSes retain ordinary filesystem error
handling. The reserve is best effort with concurrent writers, quotas and delayed
allocation; it does not provide indefinite recording capacity.

Media status `processing_fps` measures renderer/encoder capacity during work,
not recorded cadence. Encoded count divided by elapsed recording time is an
average cadence, including pauses. Native status reports preview and numerical
processing rates separately. Progress counters are sampled independently and
can briefly reflect different instants while workers are active; final status
and independently decoded frame counts establish completed-file alignment.

Serial and UDP receive the same accepted-only newline-terminated `FT, ` plus full 25-column row, including the independent sequence counter. Serial defaults to 115200 baud, 8-N-1, no flow control. Output failures remain visible and do not manufacture measurements. Headless summary separates tracked, relocalized, initializing and rejected counts. Native GUI JSONL acknowledgments are queued; inspect actual revision/frame/advance/recording status before reporting completion.

## Simultaneous UDP and ZeroMQ PUB

HejjéTrac can publish the same accepted measurement over UDP and ZeroMQ at the same time. Set the ordinary `sock_host`/`sock_port` for UDP and these additional legacy-text fields for ZeroMQ:

```text
zmq_host           : 127.0.0.1
zmq_port           : 9871
zmq_queue_capacity : 256
```

`zmq_port : -1` or `0` disables the publisher. `zmq_host` is an address **on the publishing computer**, defaulting to `127.0.0.1`. `localhost` and numeric IPv4/IPv6 interface addresses are accepted. An explicit `0.0.0.0` or `*` binds all IPv4 interfaces, while `::` binds IPv6 interfaces; no DNS resolution occurs. For a remote receiver, bind a local LAN interface or wildcard and have the receiver connect to the publishing computer's LAN IP. Do not bind the receiver's IP. The TCP endpoint is a **PUB bind**, with one publisher per unused port; stop that rig's Python republisher before reusing its bind port. The default `zmq_output_mode : native_ft` sends exactly one UTF-8 frame: the identical newline-terminated `FT, ` prefix and complete 25-column row used for UDP and serial. In native mode there is no multipart topic, JSON wrapper, added column or coordinate conversion. The explicit `legacy_matrex_vr` mode below changes only ZeroMQ; UDP/serial retain their FT rows. A subscriber can filter on the byte prefix `FT, `. Rejected measurements remain in `all-frames.dat` and `quality.csv` and are omitted from both live transports, preserving the legacy decision.

The publisher implements the TCP PUB/SUB subset of ZMTP entirely in Rust; it does not link libzmq or launch another process. It negotiates ZMTP 2.0 for the original NetMQ 4.0.0.1 DLL and NULL-authenticated ZMTP 3 with modern subscribers. The `zeromq` crate is an independent test subscriber, not the production publisher. A dedicated worker owns the socket. Tracking submits one complete, atomically framed message with a nonblocking bounded queue operation. Capacity defaults to 256 rows and accepts 1–4096; a row is limited to 16 KiB. A full queue discards that live row and reports the drop while numeric data recording continues. UDP and ZeroMQ are independent best-effort outputs: one output's failure does not turn a measurement into zero motion or stop the numerical engine.

The output status includes the endpoint, queue capacity/current length, enqueued rows, completed PUB dispatch attempts (`published`), application/per-peer queue discards (`dropped`), failure count, worker state and last error. `zmq_payload` reports the applied mode, framing and topic. Native FT always has effective single framing, even if a previous JSON framing choice remains stored. `published` does **not** mean a subscriber received or acknowledged a row. Like normal ZeroMQ PUB/SUB, subscribers joining late miss earlier messages; subscription setup takes time, and there is no replay. Subscriber or network losses below the application queue are not included in `dropped`. The independently sampled counters can briefly reflect different instants.

Each subscriber has a 16-message queue and a 25 ms write deadline. A stalled or malformed peer is closed independently, so a cancelled partial frame is never reused and healthy peers continue. Handshakes have a two-second deadline, with at most 64 peers including pending greetings. Control frames are bounded to 4096 bytes and subscriptions to 64 prefixes of at most 256 bytes. Messages queued before subscription are discarded. Shutdown drains the application queue for up to 50 ms, then gives peer tasks up to another 50 ms, with a further 50 ms runtime teardown bound. Rows remaining in the application queue are counted as discarded; losses inside a peer/network are not completely measurable. Neither closing nor a completed run guarantees subscriber delivery. A port already in use fails configuration with the exact endpoint and a request to choose another port.

Configuration edits reuse healthy output handles when transport settings are unchanged. Changing an output releases exclusive handles before reopening them. If the new settings fail, the previous settings are restored and the edit reports the failure; restoring a PUB starts a fresh socket and counters, so subscribers must reconnect and can miss rows. If restoration also fails, the error explicitly identifies the rollback failure and outputs remain disabled until valid settings are reapplied. Numeric tracking state is independent of this output transaction.

The root `VR1.txt`–`VR4.txt` files use the supplied 2026-10-26 calibrations,
320×240/100-fps capture and camera IDs `0, 2, 4, 6`. UDP ports `1111`–`1114`
feed the setup's existing external republishers; direct PUB is disabled to
avoid colliding with them. These are device-specific saved calibrations; check
source dimensions and geometry for the actual setup. Opening another instance
cycles existing VR/BR configs through slots 1–16, skipping absent files and
already-used sources. Thus opening four from VR3 selects VR3, VR4, VR1, VR2.
When no saved sibling is available, the copy increments its numeric camera ID,
enabled UDP/PUB ports and `rust_instance_name`; a video path stays a video path
and disabled outputs stay disabled. Copies have no saved path, so saving cannot
overwrite the parent configuration. Port or label-number overflow fails explicitly.

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


## Legacy MATREX VR JSON

This explicit mode reproduces the [Python republisher](https://github.com/pvnkmrksk/fictrac/blob/10fc4472af1fc9062b9bf5a92738e835b98cc3ec/VR_array/socket_zmq_republisher.py), without a Python runtime or republisher process:

```text
zmq_host        : 127.0.0.1
zmq_port        : 9871
zmq_output_mode : legacy_matrex_vr
zmq_framing     : topic_json
zmq_topic       : FT
```

With the complete comma-split wire row, **including `FT` at index 0**:

| JSON field | Value |
|---|---|
| `x` | `float(fields[15])` |
| `y` | `float(fields[16])` |
| `z` | `0.0` |
| `yaw` | `math.degrees(float(fields[17]))` |
| `pitch`, `roll` | `0.0` |

Conversion reads the already formatted FT row, preserving its decimal rounding before degrees conversion. Key order, separators, signed zeros and float notation match `json.dumps`. For a stationary zero pose the payload is:

```json
{"x": 0.0, "y": 0.0, "z": 0.0, "yaw": 0.0, "pitch": 0.0, "roll": 0.0}
```

Positions remain cumulative **sphere radians**, on the legacy axes. There is no radius multiplication, axis swap, reset offset or inferred movement across missing frames. Unity applies the radius. Only accepted, entirely finite measurements are published; overflow during yaw conversion also rejects the live JSON message. Recording and quality evidence remain independent of subscribers.

`zmq_framing : single` sends one UTF-8 JSON frame, matching Python's `send_string(json.dumps(...))`. `topic_json` sends exactly two frames in **one multipart message**, `[topic, JSON]`, for old paired-read receivers. The default topic is `FT`; the old MATREX players subscribe to all topics. A modern single-frame JSON subscriber should subscribe to the empty prefix or `{`, rather than the native `FT, ` prefix. Defaults remain `native_ft`, `single` and `FT`.

See [MATREX VR compatibility](matrex-vr.md) for the unchanged-player tests and the v0.1.0 lifecycle limitation.

## MATREX VR connection

[MATREX VR](https://github.com/pvnkmrksk/matrexVR) is the companion Unity experiment renderer. Its [input guide](https://pvnkmrksk.github.io/matrexVR/manual/input-contracts.html) maps native FT columns 15/16/17 (forward, east and heading in radians) to its walking rig and explains sphere-radius conversion. The native decoder/transport path belongs to current source after its immutable 0.3 release; use the contract for the exact renderer build rather than treating the old JSON adapter and an FT row as the same payload.

A successful connection does not turn rejected/missing motion into a measurement. Keep the all-frame/quality evidence described above. [Daari Deepa](https://github.com/pvnkmrksk/trajectory-dashboard) supplies downstream trajectory analysis; [MATREX recording and replay](https://pvnkmrksk.github.io/matrexVR/manual/recording-pipeline.html) explains its saved experiment bundle.
