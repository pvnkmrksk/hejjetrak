# Behavioral parity assessment

Published baseline: [v0.2.3](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.2.3),
source `35859b3faf7ec7a394b6608730e20a78d6e5d9b8`, passes all four
[native package jobs and both Ubuntu runtime jobs](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40381876617).
The official Apple Silicon archive passes strict ad hoc signature/version/source
checks, the complete sample comparison and native preview inspection. The same
Linux packages pass installed/portable sample and visible Xvfb/Mesa preview
checks on Ubuntu 22.04 and 24.04. Native Windows visual checks, physical Linux
cameras and unrestricted physical-motion accuracy remain unverified. The
[published validation](validation.md#v023-published-release--2026-10-26) records
the v0.2.3 recording-rate correction, unchanged numeric timestamps and native
saved-default relaunch checks. The numerical engine is unchanged from v0.2.2.

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

## Current compatibility and recovery corrections

Published v0.2.2 corrects the Linux build baseline to Ubuntu 22.04 / glibc
2.35, with an ELF symbol-version guard for both executables. A single package
built on that baseline has been installed and run in Ubuntu 22.04 and 24.04
userspace: installed/portable CLIs process the full sample and pass the fixed
numerical comparison, and native Xvfb/Mesa windows show the sample and exercise
unavailable-camera fallback. The [final release evidence](benchmarks/release-v022-linux-20261026.json)
identifies the exact published source, package hashes and execution scope;
the [earlier preflight](benchmarks/ubuntu22-compatibility-20261026.json) remains
separate. These checks do not exercise physical V4L cameras. Windows/Linux capture supports validated
mono and pure Rust MJPEG decoding alongside the existing raw formats; V4L frame
waits are bounded. [Camera contracts and hardware limits](camera-controls.md)
remain explicit.

Fast-motion recovery now uses a held candidate's velocity only to propose the
next search seed. Complete ROI verification and all acceptance/learning rules
remain unchanged. A default-budget known-motion regression that previously
never resumed at 0.20 rad/frame now confirms on the fourth visible image for
sharp and simulated half-frame-exposure images, then measures all 86 remaining
relative deltas within the existing 0.008 rad/component gate. The actual-video
full/partial-obstruction oracle retains its exact prior errors and original
gates. These [controlled results](benchmarks/fast-motion-recovery-20261026.json)
do not establish a universal physical speed, shutter-exposure or occluder limit.
Final workspace/release verification of the combined changes is recorded in
[validation](validation.md). Live-camera accuracy remains provisional pending
correct ball-only calibration and independent physical reference measurements.

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
| Video recording | H.264 AVI default, XVID AVI, MPEG-4 MP4, MJPEG/raw AVI; explicit H.264 MP4; FPS/container tests, calibrated debug mosaic, safe partial finalization. v0.2.3 prefers explicit FPS, then source metadata, with interval inference only when unknown. Real-frame tests retain exact numeric/quality timestamps; encoded video remains constant-rate. See validation.md |
| Video codec boundaries | Segmented OpenDML AVI supports >4 GiB offsets; H.264 encodes even dimensions with SPS cropping; odd H.264 4:2:0 dimensions fail explicitly; exotic profiles/formats remain outside tested coverage |
| Serial/UDP | Native Rust serial2 and datagram sinks with original prefix/full row; actual OS pseudo-terminal and loopback tests; physical serial hardware untested |
| Camera | Historical FaceTime HD checks cover 640×480/30 and 1280×720/15 format/readback, pause/step/config, two 35-frame recordings, source switch and reopen. A v0.2.2 ~120 FPS USB-camera check maintains acquisition/preview for ~30 seconds, but its old calibration includes the holder and cannot establish tracking accuracy. Physical Linux V4L, iPhone and industrial SDKs remain unverified |
| Native platforms | Published v0.2.3 passes four native package jobs and four portability jobs. Official Apple Silicon signature/version/source/sample, 175-frame preview and saved-user-config relaunch checks pass; Ubuntu 22.04/24.04 installed/portable sample, Xvfb/Mesa previews and per-user default fallback pass. Native hover/loupe pointer testing remains incomplete; Windows visual checks, Linux hardware GPU/Wayland and physical V4L cameras remain unverified |
| Packaging | Published v0.2.3 has five assets: two Mac ZIPs, Windows ZIP, Linux portable tar and Linux amd64 `.deb`, with dependency notices. Actual Linux GUI/CLI ELF requirements stop at glibc 2.35; installed/portable binary hashes and full sample checks match on Ubuntu 22.04/24.04. Writable defaults live outside installed/signed resources. No Developer ID notarization or Windows Authenticode signing |
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
