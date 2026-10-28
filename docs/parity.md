# Behavioral parity assessment

## Live capture intervals and rim accuracy — 2026-10-28

The live 47° follow-up separates a small source-rim calibration error from the
predictor's handling of irregular capture intervals. A refit at the same FOV
restores stable complete-video agreement with FicTrac. Time-aware bounded
prediction then tracks all 4407 increments in a real-video fixture with 57
controlled three-image omissions; all full/rolling component comparisons pass
the existing 0.020 rad limit. Known-motion gap accuracy retains the 0.008 rad
limit. No claim of physical ground truth or measurable hidden full turns is
made. [Evidence and limitations](take-validation.md#live-47-follow-up-2026-10-28).

The Linux user also reports that **Save and close** can repeat its confirmation
instead of exiting. This report remains a separate GUI issue; the tracking
change does not claim to fix it. The live config was saved with the ordinary
Save action and verified clean on disk and in session state.

## New supplied 17°/47° takes — 2026-10-27

The new 6075-image 47° take reproduces a large fast-motion failure with the
previous defaults and unchanged saved calibration. Current source adds bounded
previous-accepted-image refinement, a persistent-marginal null-match guard for
verified fits above the base 18000 MSE ceiling, and reuses already established
four-image motion confirmation during acceleration probation. These are Rust
engine changes; source calibration and numeric tracking settings are retained.

The final replay measures 6070 increments, with three unaccepted images and one
zero-delta recovery during hand contact. All 3413 clear reference-defined fast
images are tracked and every clear rolling 100-image component check passes the
unchanged 0.020 rad limit. The unexcluded worst window is still 0.020179 rad;
it is retained alongside hand annotations selected from input images. Neither
FicTrac nor the Rust output supplies physical ground truth. The 17° take has
7205 numeric rows but only 64 recorded images, so it cannot establish performance
on its missing high-speed interval. Existing full 17° VR3 gates still pass.

A short native 47° take contains 1038/1038 tracked/encoded rows at 100.9 delivered
frames/s; its observed motion is narrower than the supplied recording's full
speed range. A separate complete native 6075-image replay agrees with CLI in all
25 numeric columns and shared quality fields. [Methods, configs, live scope,
continued occlusion limits and reproduction](take-validation.md) retain exact
source/binary/input hashes and the difference between reference agreement and
physical accuracy. No installed application or rig file is replaced.

## Earlier partial-atlas/default follow-up — 2026-10-27

Shared defaults retain quality six and the existing prediction window/error
floor, increase the local budget to 100, and add guarded seeding plus bounded
independent temporal appearance verification. Explicit old config values remain
respected. The [masking assessment](masking-validation.md) contains 240
known-motion cases, 72 real-image panel controls, full missing-data evidence and
remaining textured-obstruction/cold-start ambiguity. These are acceptance and
learning extensions, not upstream numerical identity or universal occlusion
parity. Optical and lab-axis calibration remain necessary.

With calibration only, the supplied video now has 3552 measured relative rows,
ten unaccepted images and two zero-delta relocalizations. There is still 120 ms
of unmeasured motion. Same-config original FicTrac component RMS disagreement
is 0.005836/0.004236/0.008116 rad and complete path RMS is 0.378505 sphere rad;
neither program supplies physical ground truth. The final full VR3 replay
retains all 4678 pre-hand and all 410 fast-section measurements and passes every
unchanged rolling/reference gate. Rejected seeding prototypes that broke those
gates are not retained. Native/CLI full-video numeric outputs agree exactly.

## Initial supplied 47-degree raw-video experiment — 2026-10-27

Current source permits first-hit teaching of unseen surface after strong
acceleration confirmation while preserving existing atlas cells during
probation. An explicit per-video `rust_resilient_error_floor` can raise the
adaptive MSE ceiling's lower bound from its unchanged default 10000 through
18000; the upper ceiling, explicit error hard cap and other guards remain.
These are documented acceptance/learning choices, not numerical identity with
upstream. On the supplied 3,565-image clip, the dedicated fitted config leaves
three unaccepted images and one zero-delta recovery instead of main's 2,202
unaccepted images. Same-config FicTrac comparison, complete trajectory
discrepancy, the missing interval and unverified lab axes remain explicit in
[the raw-video validation](raw-video-validation.md). No physical ground truth
or general scientific-parity claim is available for this recording.

## Published v0.3.2 graphics patch

[v0.3.2](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.3.2), source
`93f973b1b1dc6c582f52f076dede0e272ebc9784`, adds automatic surface-compatible
OpenGL/Vulkan selection on X11/XWayland. All four native package jobs pass.
The original Debian/portable packages pass normal GL rendering, forced-GL failure
controls and automatic Vulkan rendering on Ubuntu 22.04/24.04 using Xvfb/Mesa.
All ten previews were inspected. [Validation](validation.md#v032-published-release--2026-10-27)
records exact package identities, the separate QA-parser correction and hardware
limits. Numerical tracking and camera behavior retain the v0.3.1 baseline below.


## Published v0.3.1 baseline

[v0.3.1](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.3.1), source
`99d4f5711584dd211f8b8c16e52bbbd357119a2f`, publishes the Linux four-camera
improvements together with the v0.2.3 recording-clock and saved-default fixes.
All four native package builds pass; the official archives and the same actual
Debian/portable packages pass source, numeric and visible-preview checks on
Ubuntu 22.04 and 24.04. The Debian-container correction preserves every installed
file and binary. [Published validation](validation.md#v031-published-release--2026-10-27)
separates application source, packaging tools, fixture checks and physical-hardware
boundaries. Historical assessments below retain their original version scope.


## Linux rig improvements — 2026-10-26–27

The `codex/robust-multicamera` branch was based on the
`codex/ubuntu22-compat-ci` source and is included in v0.3.1. Its supplied VR3 recording has 5,157 images.
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

Daily controls now provide direct calibration status, selective saved-layer
review/redraw and a one-row toolbar. These are native adapters to the shared
commands, without new numerical calibration algorithms. Full-video
[body movement and derivative comparisons](vr3-motion-structure.md) distinguish
heading drift, time-base jitter and weak low-motion agreement from overall
motion structure. Sustained one/four-camera CPU cases and their exact scope are
recorded in [validation](validation.md).

Live recording Stop/replacement now finalizes old takes on one bounded worker
per camera. Completion is explicit in status and `completion.json`; active
capture does not wait for the old encoder drain/fsync. Initializing a new take
and changing configuration epochs remain synchronous. Independent acquisition
clock checks are required: consecutive saved frame IDs alone missed the
multi-second gaps in the earlier rotation preflight. The
[overnight protocol and actual run reports](overnight-validation.md) distinguish
that failed preflight, the revised live test and any eventual long run.

The completed **6 h 12 min** four-camera soak exits cleanly with **8,998,214**
aligned processed rows and exact accepted wire data. All requested source MJPEG
movies complete, but four debug probes remain partial and two approximately
65-second storage/verification fallbacks intentionally omit video while keeping
data. A heavy debug probe briefly reduces capture throughput; delayed control
acknowledgments can extend requested probe duration. Those remaining recording
and scheduling limits prevent an unrestricted 100-FPS recording claim.
[Final overnight evidence](benchmarks/linux-overnight-20261027.json).

The separate 300-frame sample also passes its original gates with the selected
complete-ROI policy: path RMS 0.044807 sphere rad, heading RMS 0.013185 rad and
component RMS .008761/.008292/.014211 rad. The 0.025594 sample path result in
the historical accelerated-search assessment below describes that earlier policy.

## Historical v0.2.3 release assessment

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

The v0.2.3 source uses the [accelerated local search](tracking-performance.md).
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

ZeroMQ is a pure Rust TCP PUB with a bounded worker queue, alongside UDP/serial. It defaults to enabled on `0.0.0.0:9871` for localhost and IPv4 LAN receivers; explicit interface binds or disabling remain supported without changing the wire payload. PUB startup/subscriber reconnect loss and overload are observable, not guaranteed delivery. Native accepted-row wire compatibility is verified independently with modern subscribers; full rejected-frame evidence remains in files. The explicit legacy MATREX JSON mode and ZMTP 2 support are tested against the old NetMQ 4.0.0.1 DLL and unchanged MATREX VR players; [compatibility evidence and receiver lifecycle limits](matrex-vr.md) distinguish functional checks from clean teardown. Busy-port edits roll back outputs while retaining the numerical source state.

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
