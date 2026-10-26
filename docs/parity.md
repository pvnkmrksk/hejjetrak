# Behavioral parity assessment

## Current Linux rig work — 2026-10-26

The local `codex/robust-multicamera` branch is based on the requested
`codex/ubuntu22-compat-ci` source. Its supplied VR3 recording has 5,157 images.
The revised resilient fitter accepts all 4,678 clear pre-hand images and all
410 images in the selected fast section. Increment RMS against the original
live data is 0.006677/0.006547/0.009413 rad; all rolling 100-image component
checks pass the unchanged 0.020 rad limit. The complete path still differs:
2.486 sphere-radian path RMS and 2.700 endpoint error over 273.950 travelled
sphere radians. Recovery occurs at image 4820, about 90–100 ms after the ball
becomes visible, with no invented motion across the obstruction. Original
post-hand output is unreliable and is not treated as recovery ground truth.
[Full trajectory, alignment and recovery evidence](vr3-validation.md).

On Ubuntu 22.04, this machine's Intel/NVIDIA graphics combination has passed
native single/four-camera resize, source-switch and ownership tests. A complete
native VR3 replay, including resizing during fast movement and the hand interval,
matches the headless run in all 25 numeric columns and every quality decision.
Four cameras sustain approximately 101 delivered frames/s each under the tested
ZMQ load, with matching recorded and received accepted rows. Saved VR3-start
Open-four checks retain four live previews in both flat and per-folder layouts.
[Native evidence and graphics limits](linux-native-validation.md).

These are measured improvements on this fixture and machine. They do not
establish numerical identity with FicTrac, arbitrary occlusion recovery, unlimited
camera capacity or immunity to OS/driver failures. Recording throughput and its
separate processing/preview/media boundaries are documented in
[recording performance](recording-performance.md).

The separate 300-frame sample also passes its original gates with the selected
complete-ROI policy: path RMS 0.044807 sphere rad, heading RMS 0.013185 rad and
component RMS .008761/.008292/.014211 rad. The 0.025594 sample path result in
the historical accelerated-search assessment below describes that earlier policy.

## Historical v0.1.3 release assessment

Published [v0.1.3](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.1.3)
passed all four [native release jobs](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40180700262),
with 173 workspace tests on Linux and both Macs, 170 platform-applicable tests
on Windows, and 1,353 decoder tests on each platform. The official Apple Silicon
archive passed strict signature/version checks, the complete sample comparison
and native preview/recording inspection. Native hover/loupe pointer verification
remains incomplete; physical camera and GUI coverage limits below still apply.

Current source also includes live calibration previews, a multi-rig manager, system/light/dark appearance and stricter invalid-input checks. The [polish validation](validation.md#final-polish--2026-10-25) records native Mac interaction checks and the unchanged numerical comparison gates.

Current source additionally uses the [accelerated local search](tracking-performance.md).
It changes candidate generation while retaining complete error/quality scoring,
coordinates, maps and the original reference gates. Its sample path RMS improves
to 0.025594 sphere radians; component RMS remains within 0.020 rad each.
Unrestricted numerical identity is not claimed. Default `rust_resilient : y`
additionally changes acceptance/recovery policy to preserve a learned atlas on
unreliable images; `rust_resilient : n` restores the older permissive/reset
behavior. The new adaptive guard is not described as an unchanged quality gate.
The fixed healthy-trajectory regression tolerances and complete fit scores remain
separate from this intentional policy extension.

Assessed against pvnkmrksk/fictrac commit `10fc4472af1fc9062b9bf5a92738e835b98cc3ec` and the supplied FicTrac 2.1.2 sample output. This version closes the previously missing operational features for the supported recorded-video workflow. **It does not establish unrestricted or numerical identity.**

| Capability | Implementation / evidence |
|---|---|
| Sphere tracking and six-panel display | Shared threshold/atlas fitter and actual numerical maps; native Metal screen inspected |
| Local optimizer | Pure Rust PRIMA BOBYQA; differs from upstream NLopt Powell implementation |
| Global search | Legacy mode uses deterministic SO(3) covering/BOBYQA multistart, differing from upstream stochastic CRS2. Default resilient mode additionally uses accepted-view seeds, throttled bounded recovery and complete final verification; no universal recovery guarantee |
| Templates | Real equal-area PNG confidence image import/export; matching dimensions validated atomically |
| Bad-frame/reset handling | Default resilient policy preserves atlas/pose/path on unreliable frames, rejects low texture/adaptive excessive complete-score errors even with negative `opt_max_err`, suppresses automatic atlas clearing and confirms recovery across two matches, or four good images for a consistent new-speed epoch. `max_bad_frames` auto-reset is the legacy `rust_resilient=n` policy. Explicit reset remains available |
| Missing motion | `rejected` and unaccepted `recovering` rows remain in quality/all-frame evidence; successful confirmed relocalisation sets absolute pose with zero relative motion. No hidden-interval trajectory is invented |
| Legacy axes and 25-column data | ROI export basis, original signs/units, principal Rodrigues orientation, independent sequence counter; supplied reference gates unchanged |
| Legacy accepted-only output | `.dat`, serial and UDP emit accepted rows; companion all-frame output prevents missing decisions from being hidden |
| Calibration | Shared rim/masks/forward/square and resolution-reference operations; guided active-step editor and 4× current-view cursor loupe; saved square slots preserve order. F reflected-seed alternate fit differs from NLopt and can report no alternate branch; saved angular/pose values retain precedence |
| Resolution portability | Explicit normalized extension reference can regenerate legacy pixel marks for matching aspect/FOV/projection; angular fits stay unchanged. Imported legacy files are never silently normalized; sensor crop/optical changes need review |
| Config/file state | Lossless comments/unknown fields/CRLF, raw/field synchronization, history, external conflict guard; transactional agent edits |
| Multi-instance | Independent native windows/headless sessions; VR/BR files, incremented camera IDs/ports, preserved video paths; explicit window size/position and up to 16 grid cells |
| Full CLI | Conventional commands plus legacy positional config invocation; structured status, schema, all numerical/calibration/recording operations through JSONL |
| AI agent control | JSONL/MCP share the numerical controller; native GUI JSONL controls displayed sessions, including calibration steps/edit/loupe/full-image choices. Historical v0.1.1 native video/camera/grid transcripts and current validation are distinguished in validation.md |
| Video inputs | H.264 MP4/MOV/AVI (including B-frame ordering), MPEG-4 Part 2 MP4/MOV/AVI, MJPEG/raw AVI and mono/420 Y4M |
| Video recording | All original codec settings: H.264 AVI default, XVID AVI, MPEG-4 MP4, MJPEG/raw AVI; explicit H.264 MP4; exact FPS/container tests, calibrated debug mosaic, safe partial finalization |
| Video codec boundaries | Segmented OpenDML AVI supports >4 GiB offsets; H.264 encodes even dimensions with SPS cropping; odd H.264 4:2:0 dimensions fail explicitly; exotic profiles/formats remain outside tested coverage |
| Serial/UDP | Native Rust serial2 and datagram sinks with original prefix/full row; actual OS pseudo-terminal and loopback tests; physical serial hardware untested |
| Camera | Built-in FaceTime HD camera: actual 640×480/30 and 1280×720/15 format/readback, pause/step/config, two 35-frame H.264 recordings, source switch and reopen; unsupported exposure/gain controls explain limitations; physical sphere accuracy/iPhone/industrial SDKs unverified |
| Native platforms | Published v0.1.3 passed all four native release jobs. Official Apple Silicon archive signature/version/sample and native preview/recording checks passed. Native hover/loupe pointer testing remains incomplete; Windows/Linux GUI appearance and camera hardware remain unverified |
| Packaging | Published v0.1.3 has five assets: two Mac ZIPs, Windows ZIP, Linux portable tar and Linux amd64 `.deb`, with dependency notices. The Linux package passed extraction, dependency/desktop-entry and CLI smoke validation on its build host; ABI requirements remain explicit. No Developer ID notarization or Windows Authenticode signing |
| Resource scheduling | Interruptible source deadlines/idle waits, bounded queues/staged frame, coalesced preview publication, unchanged per-frame numerical/recording evidence, cached geometry/textures and deque history. CPU/RSS methodology/results in resources.md; no wattage or sixteen high-rate camera capacity claim |

A protocol request accepted into a GUI worker queue is not proof the action completed. Clients poll revision/frame/advance/recording/notice state and validate files. Synchronous headless commands expose the same numerical operations; playback and OS-window scheduling differ deliberately.

The 300-frame sample benchmark checks aligned per-frame Rodrigues deltas, path and wrapped heading with unchanged declared tolerances. Neither that reference nor synthetic renderer tests provide physical ground truth for arbitrary experiments. Numerical optimizer differences, untested hardware, codec/container limits, timing and platform differences above prevent a claim of full drop-in parity. All unsupported codec requests fail explicitly rather than silently substituting another codec.

## Compact desktop and transport additions

Performance has a separate [same-machine assessment](performance.md): bounded
decode/fit overlap and exact-work elimination improve the sample 1.91× over
v0.1.1. Pure Rust decoding remains the limiting stage; full active-throughput
parity with C++ has not been demonstrated. Decoder pixel oracles and the declared healthy-trajectory regression gates
remain unchanged. Resilient mode intentionally adds an adaptive acceptance guard
and suppresses legacy destructive auto-reset; see [recovery](recovery.md).

The default preview hides config and diagnostics and can shrink to 96×96 logical pixels; v0.1.3 hides tiny-preview chrome and retains context-menu controls instead of shrinking fonts. A full-image toggle restores context without aspect distortion. A separate native inspector exposes the same canonical config and shared-core calibration actions. Source images and diagnostic panels use uniform scaling and letterboxing; cropping uses the calibrated pinhole/fisheye projection. Native pointer regressions cover active-tool isolation, nearest-point deletion and preserved square corner order. Camera controls are capability-based: exact formats/rates and readback are implemented, but support for manual exposure/gain depends on the backend and device. See camera-controls.md.

ZeroMQ is a pure Rust localhost PUB with a bounded worker queue, alongside UDP/serial. PUB startup/subscriber reconnect loss and overload are observable, not guaranteed delivery. Native accepted-row wire compatibility is verified independently with modern subscribers; full rejected-frame evidence remains in files. The explicit legacy MATREX JSON mode and ZMTP 2 support are tested against the old NetMQ 4.0.0.1 DLL and unchanged MATREX VR players; [compatibility evidence and receiver lifecycle limits](matrex-vr.md) distinguish functional checks from clean teardown. Busy-port edits roll back outputs while retaining the numerical source state.

## v0.1.2 intentional differences

The protected-atlas policy is a deliberate improvement over an unrestricted
upstream behavioral clone. Default recovery retains accepted history, rejects
unreliable observations and requires confirmation. The accepted-view bank is
bounded to 256 poses; recovery spends at most `min(opt_global_evals, 768)`
objective evaluations per call and is throttled between broader attempts.
`rust_resilient : n` is the opt-out for the old acceptance/reset policy. The
numeric columns, coordinate frames, configuration text preservation and
accepted-only wire framing are unchanged; which frames qualify for acceptance
can differ. [Recovery fixtures](recovery.md) cover long learned-map sequences,
240-frame obstructions, actual sample images and a large partial occlusion.
They do not establish physical live-ball or arbitrary-occluder accuracy.

The calibration frontend adds guided steps, a view-relative cursor loupe and
explicit resolution-reference metadata. These are thin shared-command adapters,
not a new coordinate system. Scheduling improvements reduce housekeeping rather
than dropping numerical frames to lower CPU. Platform-specific CPU/RSS evidence
and acquisition/recording limits are recorded separately from tracking-call
latency. GPU tracking, NVIDIA hardware, battery drain/wattage and modern Intel
laptop capacity have not been measured.

## v0.1.3 refinements

Saved calibration is reviewed read-only before Keep, Edit or Redraw. The larger
loupe and small-preview chrome do not change source coordinates or font sizes.
Closing the initial native window leaves other sessions running. Concise titles
prioritize VR, applied ZMQ port and camera index/video basename; larger windows
also show UDP and the full identity.

Resilient fitting additionally bounds accepted motion using robust recent
accepted rates, never relaxes the previous image-error ceiling, and delays atlas
teaching after recovery. Ordinary confirmation uses two images/three following
teaching-probation rows; a consistent acceleration change requires four good
images, restarts accepted-motion history and defers teaching for eight rows. A low-MSE
motion outlier can therefore remain rejected with its real score intact. The
legacy opt-out remains `rust_resilient : n`. See [recovery](recovery.md) for exact
bootstrap/probation limits and the inherent ambiguity of a plausible lookalike.
These are intentional acceptance changes; the declared reference gates, pixel
oracles, coordinate conventions and numeric/wire columns are not redefined.
