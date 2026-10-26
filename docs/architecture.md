# Architecture

The engine and native frontends are separate. [PhotoCraft's architecture](https://github.com/storytold/photocraft/blob/main/docs/architecture.md) and [contributor guide](https://github.com/storytold/photocraft/blob/main/AGENTS.md) informed the headless-first boundaries and validation workflow.

`fictrac-core` owns validated images, perspective/equidistant camera geometry, rim fitting, SO(3) estimation and integration, and numerical debug views. It depends on glam, serde, thiserror and the pure Rust bobyqa crate. No frontend, OS capture, file, network or codec dependency enters core.

`fictrac-io` owns lossless legacy document edits/history, strict JSON, H.264 MP4/MOV decoding through OxideAV, Y4M, data/SVG output and reference comparison. Optional RGB decoding supplies OpenCV-style integer grayscale or selected RGB channel for the legacy threshold pipeline. The raw-luma decoder oracle remains unchanged.

`fictrac-camera` wraps native OS APIs. macOS AVFoundation delivers BGRA on a serial dispatch queue; a bounded latest-frame slot copies pixels before unlocking the native buffer and retains capture PTS/sequence. The adapter prefers a supported 640×480 native format and supplies explicit CoreVideo output dimensions: the session preset alone did not constrain delivered pixels on the tested Mac. Exact rational frame durations preserve supported FPS endpoints. Transitional frames with different dimensions are filtered before publishing, without advancing the delivered sequence. See [Apple’s output-settings contract](https://developer.apple.com/documentation/avcapturevideodataoutput/videosettings). The sole application unsafe exception is documented OS FFI in `macos.rs`; there are no C/C++ source files. Windows/Linux use Nokhwa's native Media Foundation/V4L backends. Linux MJPEG camera buffers are decoded in Rust; metadata-only V4L nodes are excluded from camera choices. Available native capture timestamps are normalized once to the session clock, preserving queued-frame intervals and acquisition gaps; host delivery time is the fallback when the backend supplies no capture timestamp. A delivered-frame counter alone cannot establish absence of sensor or driver frame loss.

`fictrac` is the CLI frontend. `fictrac-desktop` uses egui/eframe and wgpu (Metal, DX12, Vulkan/GLES) in native windows, with native file dialogs. Each session owns its config document, undo history and worker. Worker commands are bounded, snapshots expose only the latest image/state, and capture/decoding/fitting do not run on the render thread. Invalid numerical settings retain a responsive video preview with an error. Configuration changes reset the tracking epoch; recordings start new data/quality/config files so calibrations do not silently mix.

## Legacy spherical-map estimation

Recorded-video frontends use a one-slot bounded decode producer, overlapping
codec work and tracking without skipping frames. The synchronous adapter
remains available for oracles. Core frames share validated immutable pixels;
clock edits do not copy images. Map fitting reuses each prepared observation
for its atlas/diagnostic commit and avoids unused pairwise pyramids.
See [measured performance](performance.md).

Legacy `.txt` configs default to map fitting. Image rays are recentered into FicTrac's ROI basis, sampled onto a square angular ROI and intersected with the sphere. Exclusion polygons mask source pixels. Local masked minima/maxima, `thr_ratio`, `thr_win_pc` and a median filter create a binary ROI. Visible surface points are projected onto a persistent equal-area sphere map. Initial pixels receive the original confidence bonus; subsequent binary evidence updates its saturated values.

For each new frame, pure Rust PRIMA BOBYQA minimizes mean squared binary-map error against the accumulated atlas within `guess ± opt_bound` on each axis. `opt_tol`, `opt_max_evals`, `opt_max_err` and `q_factor` control the healthy local fit. A filtered previous delta supplies the next guess. The fitted map and the displayed accumulated map are the same state. The optimizer differs from upstream NLopt; compatibility is assessed against fixed rotation/trajectory gates, not numerical identity.

Spherical-map settings default to `rust_resilient : y`. v0.1.3 source adds a robust accepted-motion-rate gate and short post-recovery atlas-learning probation, without loosening the previous image-error ceiling. A genuine acceleration can establish a fresh rate epoch only after stronger four-image confirmation; see [recovery](recovery.md) for bootstrap and three/eight-row teaching deferrals. Unreliable images freeze the learned atlas, pose, path and integrated movement. Uniform/low-texture observations and an adaptive complete-score ceiling supplement an explicit `opt_max_err`, even when that legacy ceiling is negative. Accepted-only error/edge history and a bank of up to 256 known views guide bounded reacquisition. Searches are throttled, spend at most `min(opt_global_evals, 768)` objective evaluations per recovery call and require confirmation before publishing `relocalized` (ordinary second-image confirmation; stronger four-image confirmation for a new acceleration epoch). Its relative delta is zero: hidden motion is never reconstructed. `recovering` is unaccepted and remains in all-frame quality/numeric evidence. This is an intentional quality/recovery policy extension, not an unchanged upstream acceptance rule. See [recovery](recovery.md).

Resilient mode suppresses destructive `max_bad_frames` auto-clearing and can reacquire without `opt_do_global`. Setting `rust_resilient : n` restores the legacy permissive quality/reset policy: optional deterministic SO(3) covering and multistart BOBYQA search after failed local fitting or loaded-map reset, and the configured bad-frame reset threshold. This differs from upstream stochastic CRS2. Explicit reset preserves source count/time and heading-independent integrated displacement while resetting pose/path/heading and sequence. Imported confidence maps are validated before mutation; unaccepted frames do not teach the atlas.

Current source also recognizes an acceleration ramp supported by three accepted
velocity vectors and a fourth score-verified image, retains confirmed visible
motion as the post-recovery predictor, and preserves spatial coverage in the
256-view reference bank. Mature partial atlas cells acquire up to 16 times the
inertia against contradictory accepted votes, while new surface and agreeing
evidence keep learning promptly. The legacy opt-out remains unchanged. Exact
policy, counter bounds and imported-map age limitations are in
[recovery](recovery.md); [VR3 evidence](vr3-validation.md) distinguishes
increment agreement, complete trajectories and post-hand recovery.

The explicit pairwise alternative (`rust_engine : pairwise`) uses a three-level luminance pyramid, bilinear warps, Huber residuals, finite-difference Jacobian and bounded Gauss–Newton/line search. It retains its own synthetic and reference regression tests; its error scale differs from map fitting.

Resilient local map search evaluates the complete ROI throughout the configured
`opt_max_evals` budget, reducing sampled-objective bias on blurred motion.
Legacy mode retains deterministic subset candidate generation, followed by
complete candidate/predictor verification reserved within that same budget;
small ROIs/budgets keep complete search. Atlas
projection caches unit sites and uses bounded atan interpolation with precise
cell-boundary fallback. Threshold extrema use linear prefix/suffix passes.
[Tracking-stage measurements](tracking-performance.md) record latency and the
resulting trajectory tradeoff. Both legacy and resilient recovery verify accepted
candidates on the complete ROI. The healthy-fit scoring and original regression
gates are retained; resilient mode intentionally adds the separate guard above.

Both estimates feed the same quaternion and path integrator. Camera physical coordinates are right/down/forward. Upstream's legacy exported “camera” deltas actually use a recentered ROI basis; the importer preserves that convention, then applies the stored `c2a_r` exactly as upstream does. Modern JSON can select physical axes. Lab forward/side motion are `(delta_lab.y, -delta_lab.x)` and heading changes by `-delta_lab.z`. Legacy path integration uses upstream’s four midpoint substeps; modern JSON uses an analytic constant-velocity arc. Position is in sphere radians.

## Calibration and file state

Rim fitting solves a calibrated angular circle in ray space. Saved `roi_c`/`roi_r` take precedence over historical `roi_circ`, matching legacy behavior. A graphical rim refit updates all three atomically. Square calibration solves a planar homography with FicTrac's unit square/corner ordering and extracts an orthonormal pose. Forward dragging preserves the existing down axis.

Graphical calibration is a thin adapter over shared rim/exclusion/square operations. Calibrate again pauses the source and immediately opens an empty Ball draft, then advances through Exclusions and Lab. Rim drafts preview a fitted outline from three marks and never connect rim points with straight lines; lab-square drafts preview from four corners. Continue validates the complete candidate before committing one undoable layer and opening the next draft. Keep saved skips a layer without changing its fields; cancellation discards the current draft and restores playback from wizard entry. Add this region commits one exclusion polygon and keeps drawing active for the next. Saved-point review and editing remain secondary inspector controls. Diagnostics renders the full input and offers direct layer redraw and complete config access. Saved calibration layers respect Show outline even in diagnostics; an active calibration draft remains visible while editing. A 240-pixel precision loupe magnifies the current view 4× and retains geometry/mark labels. Hit-testing converts through the actual crop/scale back to source coordinates. Preview zoom, full-image display and window size never alter the numerical frame.

Resolution portability is explicit. `resolution_reference` retains ordinary legacy pixel fields and records source dimensions, configured FOV/projection and normalized extension copies. `resolution_apply` validates the reference and regenerates only legacy pixel marks for identical aspect ratio and configured FOV/projection. Angular `roi_c`/`roi_r` and `c2a_r`/`c2a_t` stay exactly as written. Stale marks or incompatible framing fail atomically. Metadata-free legacy files are not silently normalized; optical changes and undocumented camera crops still need physical review/refitting.

Raw configuration text is canonical. Editing one field changes only that field's value, retaining unknown data and unrelated formatting. Graphical edits use the same operation and history. Disk reload occurs only when clean; conflicts remain visible. Corner-only configurations use the same square solver as the GUI and agent frontend. Saved `c2a_r` retains precedence. Pinhole and equidistant fisheye square calibration are tested on XY/YZ/XZ planes with the original unit-square scale.

## Bounds and failure semantics

Core/I/O/GUI forbid unsafe and deny panic/unwrap/expect/todo paths. Native capture validates dimensions, stride and buffer lengths. Configuration validates finite values and bounds before mutation. Images are capped at 32 million pixels and 8192 per axis; camera copies at 8 million pixels; configs at 1 MB. Calibration supports at most 4096 rim points and 64 polygons of 256 vertices. The pairwise sampler caps sites per level; map quality and optimization budgets are bounded.

Rejected/recovering frames freeze pose/path and reset prediction; their motion is missing, not measured zero. No interpolation invents motion across an unaccepted interval. Confirmed resilient or optional legacy global recovery resumes from an absolute pose; its relative motion is explicitly zero. Application bounds do not establish that all third-party codec parsers are fuzz-safe. See the validation evidence and explicit parity gaps before interpreting results.

## Command and output interfaces

`fictrac-control` owns synchronous recorded-video sessions and typed requests/responses. JSONL and MCP stdio call the same controller; no shell expressions or document instructions are evaluated. Configuration edits validate before committing, retain decoder position when the input is unchanged, and restart only the numerical epoch. FPS changes preserve a monotonic emitted clock. Requests, queues, sessions and retained rows are bounded. Native GUI JSONL is a thin asynchronous adapter over the sessions actually displayed in OS viewports. Queued acknowledgments are not completion: status exposes frames, revision, remaining advance count, recording state and notices.

`fictrac-io` additionally owns PNG sphere templates, OS serial/UDP sinks, pure Rust H.264 AVI/MP4, XVID AVI, MPEG-4 MP4 and MJPEG/raw AVI recording, segmented OpenDML AVI input/output and shared calibrated diagnostic rendering. Both the headless and native frontends call these implementations. Video outputs begin as `.partial` and are promoted only after encoder/container finalization without overwriting existing files. Source FPS for recording is metadata or inferred from prefetched PTS; prefetched frames and RGB are retained. No guessed 30 fps substitution is used. H.264 recording uses 8-bit High, QP20, GOP1, Intra4×4 disabled and deblocking retained; independent frames avoid the prior encoder’s expensive inter-frame search, with a measured payload-size tradeoff. Encoder selection does not change the shared estimator; decoder pixel oracles remain unchanged. Current candidate-search and resilient acceptance policies are described separately above.

Legacy `.dat` and wire sinks omit unaccepted rows (`rejected` or `recovering`), preserving the accepted-only wire format. The resilient guard can intentionally reject observations that a permissive upstream configuration would accept. Accepted legacy rows report elapsed time since the previous accepted row, spanning rejected intervals; rejected all-frame rows retain their processed-frame interval. Companion all-frame numeric data and quality logs retain every measurement decision. File recording errors preserve the last committed row in status, report failure and retain incomplete artifacts. Desktop worker shutdown joins the processing thread so ordinary close finalizes active recordings.

## Compact native frontend

The default viewport presents a cropped ball preview with uniform pixel scaling, measured contact trace and path inset. Config lives in an optional separate native viewport; diagnostics and calibration guides are independent view flags. Camera enumeration is cached and explicitly refreshable. Ordinary startup selects the first enumerated camera, with a temporary calibrated sample fallback after failure; explicit config selection remains authoritative. Automatic fallback retains rig output settings without writing the source file. Native viewport placement accepts logical size/position and a bounded 16-cell grid.

At small sizes, toolbar controls reduce to essentials; below 160 pixels wide or 120 pixels high, toolbar/status chrome is hidden and context-menu controls remain available. Fonts are not shrunk; the minimum viewport is 96×96 logical pixels. Full-image/cropped-ball display stays independent of calibration. Compact native titles prioritize VR, applied ZMQ port and camera index/video basename. Large widths add UDP and full actual source identity. Source labels use actual OS identifiers where exposed and report an honest backend/name/index fallback rather than inventing a Linux device node.

Each session uses one sequential numerical worker. Source deadlines and paused/EOF waits block interruptibly, so commands wake them without a high-frequency polling loop. A bounded staged frame survives a pacing interruption. Numeric and quality recording retain every processed frame. Preview/diagnostic publication follows an independent 1–60 FPS display setting (30 by default), with immediate step/pause/EOF publication. This setting never edits source FPS or resets the estimator. Unchanged images avoid repeated texture uploads, parsed calibration is cached and hidden guides are not built. Contact/pose history uses bounded deques rather than shifting all samples. No GPU numerical kernel or nested tracking thread pool has been added. Independent sessions can run on separate cores; their actual aggregate CPU/RSS is measured separately in [resource use](resources.md).

Numeric counters and media/output health publish approximately every 30 ms
independently of those image updates, reusing existing immutable preview Arcs.
A slow display setting therefore does not make native control status stale.

Raw and diagnostic video have independent bounded Rust encoder workers; the
diagnostic worker owns its calibrated remap and dynamic composition. Recorded
files use lossless queue backpressure. Camera sessions use nonblocking submission:
an overloaded optional stream closes as an explicit partial prefix while numeric
recording, tracking and live output continue. Completion, queued/encoded counts,
rates and warnings distinguish media from processing. The recording speed/size
choice changes the effective recording-document codec, leaving source geometry
and the canonical configuration untouched. No external media subprocess is used.
Linux live encoder workers additionally preserve a 512 MiB filesystem reserve
with safe Rust `statvfs` checks before the first frame and every 100 ms. Storage
checks never run on the numerical worker. Low space or a failed probe uses the
same explicit partial-file failure path; offline and other-platform behavior
remain scoped separately in [output semantics](output.md).

Camera capabilities and setters are typed DTOs in fictrac-camera. Format changes validate against advertised formats, update rust_camera_width/rust_camera_height/src_fps and reopen acquisition. Opted-in resolution metadata can adapt legacy pixel marks only after compatible-reference validation; other formats require calibration review. Exposure/gain setters run on the worker with native readback. Unsupported values never become fabricated controls. GUI view/camera operations are discoverable native agent actions. Transient recording options affect an effective recording-document copy, preserving canonical config and tracker state.

OutputSinks owns the shared transport/payload reconfiguration transaction: unchanged healthy actors are reused; changed outputs reopen with rollback. ZMQ transport has its own bounded Rust actor with staged ZMTP 2/3 negotiation, per-peer queues and deadlines, so a slow peer cannot block numerical processing or healthy subscribers. Explicit MATREX JSON framing lives entirely in I/O, derived from the exact native FT row; the shared core and recorded numeric data retain their coordinate and unit conventions. Fixed ROI-to-atlas geometry is cached per calibration. Resilient local fitting uses the complete ROI; legacy candidate generation retains subset sampling with complete final scores reserved in the configured budget. Decoder pixel oracles and declared healthy-trajectory regression gates remain fixed; the default resilient acceptance guard is a separately documented behavior change with a legacy opt-out.
