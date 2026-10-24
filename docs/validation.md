# Validation evidence — v0.3, 2026-10-24

Local execution: macOS 27.0.1, Apple Silicon, Rust 1.99.1, thin LTO release. Toolchain homes remain task-local under `/tmp`; no sudo, global toolchain changes, releases or external agent/model credentials were used. The user subsequently unlocked the Mac and authorized live-camera testing; the built-in camera was exercised as described below. Historical v0.1/v0.2 evidence is retained separately and must not be confused with this build.

## Final checks

- `cargo fmt --all -- --check`: pass.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: pass.
- `cargo test --workspace --locked --offline`: **106 passed**, zero failed: 25 core, 43 I/O, 17 desktop, 9 control-protocol, 8 CLI, 4 camera-buffer/rate tests.
- `cargo build --workspace --release --locked --offline`: pass.
- Windows MSVC desktop cross-target `cargo check`: pass.
- Linux full CLI/control/I/O cross-target `cargo check`: pass. Linux GUI/camera remains blocked on this Mac by absent Linux V4L headers; native Linux/Windows execution has not occurred here. Three-OS CI is configured, not remotely executed.
- Native loopback and serial pseudo-terminal tests require ordinary local sockets/PTY access, so the final suite ran outside the execution sandbox. No physical serial hardware was used.
- A transitive `block 0.1.6` future-compatibility notice remains in Cargo's report; application strict Clippy is clean.

One historical test required warnings for missing features in the original sample config. It now requires no warnings for the supported sample settings and separately asserts that an unknown future option remains warned. Numerical gates, decoder oracle constants and motion tolerances were not weakened. The complete final test log is `runs/camera-workspace-tests-v03.log`; the final diagnostic-only formatting cleanup was followed by another 21 camera/desktop tests, also passing, in `runs/camera-desktop-final-tests-v03.log`.

## Actual native GUI agent run

An external client launched the native release executable with `--control-stdio`, wrote JSONL requests through stdin, read structured replies from stdout, and polled actual worker state. **299 commands passed**, driving four real native OS windows. This was an application control API test, not pointer synthesis or a mocked GUI.

The run verified:

- full 300-frame sample in the displayed session; final endpoint agrees with the headless result;
- pause, exact multi-frame advances, queued advance accumulation and source-speed controls;
- invalid numerical edits leave config/history/revision unchanged;
- parameter edits, undo and explicit save;
- next-window copies with unchanged video paths and four native viewports;
- sphere map save, validated load, global relocalisation and zero invented relative delta;
- default H.264 raw/debug AVI recording, clean finalization, and reopening/decoding both generated videos;
- FPS changes 30→60→disabled without rewinding or backwards timestamps;
- actual calibrated numerical snapshots and safe explicit-discard shutdown.

Local evidence under `runs/native-agent-v03-final/`: `transcript.json`, `result.json`, `benchmark.json`, `native-state.json`, `native-window.png`, `full-video-snapshot/`, `recording/`, `sphere.png`, `schema.json` and a ready-to-use local `mcp-config.json`. The OS-window screenshot and computed six-panel diagnostic image were visually inspected. The screenshot was taken after a config edit reset its numerical epoch; the earlier full-video snapshot retains the complete tracked path.

The final native endpoint discrepancy is **0.0545253183 sphere rad**, matching the independent CLI run. The generated recordings contain five frames each. The source clock and recorded FPS are checked, not inferred from wall-clock display speed. Ordinary pointer/file-dialog interaction remains untested while the Mac is locked, but the actual native application was successfully driven through its control interface.

## Live built-in camera verification

The built-in FaceTime HD camera was tested through the same native AVFoundation adapter used by the GUI. The iPhone camera was enumerated but never opened. Initial testing exposed a rounded 30 fps interval outside the supported native boundary, unconstrained 1080p output despite a 640×480 preset, and slow inter-frame H.264 recording. The adapter now uses exact rational durations, explicitly sets native CoreVideo output dimensions, and rejects transitional frame sizes before publication. No Rust resize was introduced.

A dedicated debug probe captured 30 actual 640×480 frames in 1.183849 seconds including initial delivery. Metadata-only evidence is in `runs/camera-native-probe-v03/`. The final release GUI then passed **126 JSONL commands** over real native sessions: continuous capture, pause with stable counts, exact single step, parameter/config edits, recording start/stop, save, camera→video→camera source changes, reopening and clean shutdown.

Both finalized H.264 High AVI recordings contain **62 frames at 640×480, nominal 30 fps**. Independent FFprobe checks and full FFmpeg decodes passed. The 62 all-frame numeric rows match the 62 quality records and both media frame counts; every field is finite and timestamps strictly increase. The 61 capture intervals range 33.35–33.36 ms (29.9807 fps over the recorded span), with no skipped interval inside the recording. The session-wide 18 skipped capture frames include deliberate pauses/configuration interaction; they are not hidden as measured zero motion. Raw and stepped snapshots differ. Source reconnect delivered another 31 frames before normal closure.

**This camera faced a room, not a calibrated patterned sphere. All 62 recorded rows were marked rejected under the test error threshold; the accepted-only data file is empty.** The frozen path is missing measurement, not an accuracy or stationary-motion result. This verifies acquisition, controls, diagnostic rendering, recording and lifecycle only. Physical sphere tracking accuracy still needs a calibrated experimental setup.

Evidence is in `runs/camera-validation-v03-final/`: `result.json`, `transcript.json`, `native-window.png`, snapshots, saved config, numeric/quality logs and the two recordings. The actual application-rendered native window screenshot was visually inspected. An earlier live window was also inspected through the unlocked desktop; the Mac locked again before the final pointer check, so that check was not claimed. The native agent interface and app-rendered screenshot remained operational.

## Headless agent and MCP evidence

Nine Rust protocol tests exercise real commands, not just parsing. They process the full supplied sample through the agent controller and enforce the unchanged reference limits, initialize a real MCP session and invoke its tools, calibrate/edit/undo transactionally, operate four isolated sessions, save/reload templates, record data and media, preserve rejection evidence, reset counters and integrated displacement, verify source-position and FPS continuity, and compare paced/unpaced numerical outputs. CLI subprocess tests separately exercise the installed protocol entry points, legacy config invocation and config editing.

Native JSONL is intentionally asynchronous: queued acknowledgments are not completion. Status exposes the actual revision, frame count, remaining advance budget, recording state and notices. Headless commands are synchronous; real-time pacing is opt-in and does not change fitted numerical rows. Both calibration paths call core functions; all frontends use the same estimator, I/O sinks and codecs. See [agent-control.md](agent-control.md).

## Numerical recovery evidence

Six dedicated core regressions cover imported complete atlases, known large arbitrary pose, off-axis legacy coordinates, intervening flat occlusion followed by a large orientation jump, deterministic repeated global search, local motion after recovery, manual reset and automatic bad-frame thresholds. Pose error after global recovery must stay below 0.08 rad; resumed known relative motion must stay within 0.008 rad vector error, so a frozen zero estimate cannot pass. Relocalization reports zero relative delta and leaves path/integrated displacement unchanged across the missing interval. Failed frames must not contaminate the atlas. Wrong-size maps/settings are rejected before mutation.

The global search uses deterministic SO(3) covering plus multistart Rust BOBYQA, not upstream stochastic NLopt CRS2. The default global budget is 100,000 objective evaluations; these are occasional recovery operations, not the normal-frame cost. Tests demonstrate behavior on their known-motion fixtures, not universal recovery guarantees.

## Supplied 300-frame reference

The user's config/data/log are preserved verbatim under `examples/benchmark/`. The reference drove the correction of upstream's ROI-basis export convention in v0.2. Legacy integration now follows upstream’s four midpoint substeps, and accepted-row timing spans rejected intervals. These are tested without changing rotation gates or decoder oracle values. The new recovery/recording/agent features retain the global-disabled sample rotation estimates.

| Metric | v0.3 measured | Unchanged gate |
|---|---:|---:|
| Frames | 300: 1 init, 299 tracked, 0 rejected | 300 aligned |
| Delta X component RMSE | 0.008760805 rad | 0.020 rad |
| Delta Y component RMSE | 0.008292136 rad | 0.020 rad |
| Delta Z component RMSE | 0.014210732 rad | 0.020 rad |
| Euclidean path-point RMSE | 0.044806751 sphere rad | 0.075 sphere rad |
| Wrapped heading RMSE | 0.013184811 rad | 0.025 rad |
| Endpoint discrepancy | 0.054525318 sphere rad | 0.075 in regression test |
| Later video timestamp discrepancy | 3.46e-11 ms | 1 ms |

Final reference position `[3.674266189723,-2.6214661833162]`; candidate `[3.620053461636,-2.627296315711]`. Final heading reference `6.1845202293513`, candidate `6.177704995564` rad. Initialization is excluded from errors; heading wraps angular differences. The anomalous upstream first clock row is not compared as motion. Frame counts and subsequent video timestamps must align. `fictrac compare` returns nonzero when gates fail.

The original full-luma decoder oracle remains **33,177,600 bytes**, FNV-1a-64 **7e8d27f58358523f**, sample SHA-256 **afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad**. The AVI B-frame fixture additionally matches ten MP4 presentation frames byte-for-byte in RGB. Applications and tests never invoke FFmpeg; it was used only during development to create independent fixture/oracle evidence, with provenance recorded.

## Independent media interoperability

All five original recording codec settings plus explicit H.264 MP4 were executed through the release CLI against the real sample. Twelve raw/debug videos (three frames each) independently passed FFprobe codec/dimension/frame-count/FPS checks and a complete FFmpeg decode: H.264 AVI, XVID AVI, MPEG-4 MP4, MJPEG AVI, raw AVI and H.264 MP4. Every raw file is 384×288, every debug file is 640×480, and each retains 30 fps. Evidence is in `runs/codec-validation-v03-camera/interop.json`.

A separate 640×360 two-frame H.264 recording uses a coded height of 368 and normative SPS cropping. The independent decoder reports the original 640×360 display dimensions, two complete frames, 30 fps and retained edge pixels. The Rust tests additionally check cropped AVI/MP4 at 34×18 and odd-dimension MPEG-4 at 31×29. Seven OpenDML tests cover real frames on both sides of a sparse 4 GiB boundary, forced small-segment rollover, complete indexes, bounds and malformed segment failures. Sparse >4 GiB tests are Unix-only to avoid allocating 4 GiB physically on systems without sparse-file controls; ordinary rollover tests run on every platform.

FFmpeg/FFprobe are development interoperability tools only. No application command or Rust test executes them.

## H.264 recording CPU/quality tradeoff

The production encoder now uses High profile 100, QP20, GOP1, Intra4×4 disabled and deblocking retained. A retained Rust development benchmark (`crates/fictrac-io/examples/h264_probe.rs`) processed three captured-content frames serially per case, after compilers finished. Timing includes RGB→YUV420 conversion, encoder calls and packet drain; it excludes PNG reading/resizing, encoder initialization and disk writes. These short fixture measurements are not general camera throughput guarantees.

| Captured-content fixture | Previous baseline/GOP30 | Selected High/GOP1 |
|---|---:|---:|
|640×480, three frames|1.74 encode fps|121.51 encode fps|
|1920×1080, three frames|0.11 encode fps|18.73 encode fps|

Independent FFmpeg decoding passed every case. Selected luma RMSE was 0.775/255 at 640 and 0.683/255 at 1080, versus 0.749 and 0.666 previously; compressed AVC payloads grew 1.705× and 1.311× respectively. These sizes exclude fixed container/index overhead. The selected mode prioritizes predictable frame cost over inter-frame compression. Reports, per-frame timings, inputs and quality metrics are in `runs/h264-camera-bench/summary.json` and its case directories. The legacy codec selection remains H.264; no codec substitution or external runtime encoder was introduced.

## Serial release measurements

Measured serially after builds/tests/native runs completed:

| Case | Real H.264 sample, map backend | Generated known motion, pairwise backend |
|---|---:|---:|
| Size / frames | 384×288 / 300 | 160×160 / 120 |
| Accepted relative estimates | 299 | 119 |
| Rejections | 0 | 0 |
| Wall seconds | 3.81855 | 0.20046 |
| End-to-end fps | 78.56 | 598.62 |
| Tracking-call fps | 472.31 | 675.26 |
| Known-motion component RMSE | No physical ground truth | 0.00030405 rad |

Wall timing begins before input/config/first decode and includes processing, numeric/quality/SVG output; it ends before summary/final reporting. Tracking-call timing includes `Tracker::process` and numerical map updates, excluding decode/disk writes. These are single fixture measurements on this machine, not upstream comparisons or general throughput guarantees. Synthetic generation precedes timed tracking; delta `[0.006,-0.009,0.004]` passes the existing 0.003 rad/component gate.

Artifacts: `runs/benchmark-camera-v03/` contains full data, quality, map, summary, comparison JSON and overlay PNG/PDF. `runs/demo-camera-v03/` contains the generated Y4M, tracking and known-motion oracle. A local ad hoc signed development app is `target/local/FicTrac.app`.

## Remaining boundary

The implementation is not scientifically or byte-for-byte identical to FicTrac. Optimizers differ; the renderer shares geometry with the tracker; the upstream reference is not physical ground truth. All five original recorded-video codec choices are implemented. H.264 recording requires even display dimensions; unfamiliar media profiles and formats remain outside the tested coverage. Built-in Mac camera acquisition and recording are verified; physical sphere accuracy, iPhone/industrial cameras, physical serial devices, native Windows/Linux execution, broader media and experimental ground-truth validation remain unverified. See [parity.md](parity.md). No release was published.
