# HejjéTrac validation evidence

The later user-requested [storage cleanup](storage-retention.md) archives full
numerical/log/failure evidence losslessly and prunes redundant movies. Retention
counts below describe what existed at each audit time; current locations and
restore commands are recorded separately.

## Completed four-camera overnight run — 2026-10-27

The packaged Linux build from `01180ef8dad1a9007bffa0eb791461d7d7b6830c`
ran from **02:48:27 to 09:00:16 Europe/Berlin**, a supervisor wall interval of
**6 h 11 min 48.471 s**. Native exit is **0**, the stop reason is the scheduled
deadline, all owned processes exited and both temporary sleep inhibitors were
released. No tracker reset/restart was used. The actual terminal classification
is **completed with warnings**, not unrestricted recording success.

All **8,998,214** processed numeric rows have aligned quality evidence. Per-rig
counts are **2,249,349 / 2,249,532 / 2,249,579 / 2,249,754**. Processed IDs are
continuous across all 365 recording groups, accepted FT bytes match independent
subscribers exactly, and all **1,460** recording directories have committed
completion markers without numeric/marker errors. Configuration hashes remain
unchanged. Four healthy subscribers, four undrained subscribers and repeated
incomplete greetings exercised the publishers throughout the run.

All **1,424 requested full-source MJPEG movies** independently decode and match
their processed rows. Across source and debug probes, **1,476/1,480** requested
movies complete. Four diagnostic movies remain explicit retained partials.
Two capacity-protection groups, **254 and 311**, intentionally save data without
video for about **65 seconds each**; all **52,514** rows from those intervals
remain. This is not continuous all-night video. Thirty-two full-source clips
remain on disk: all four rigs for each wall-clock hour 02–08 plus the final
short clips. Other successful test movies were pruned only after independent
verification, hashes and full-frame excerpts; partial movies were not deleted.

Whole-run source-clock rates are **100.910–100.915 rows/s per rig**. Under one
roughly 3.26-second simultaneous uncompressed source/debug probe, rates briefly
fell to **85.6–93.3 rows/s**, returning promptly to approximately 101 afterward.
Eleven capture-clock intervals exceed 50 ms; the maximum is **80 ms**. Thus
processed-row continuity does not prove zero sensor loss or uninterrupted
100-FPS capture under every diagnostic load. Later requested three-second
probes stretched to approximately eight seconds with approximately one-second
control acknowledgments, producing more video than the supervisor's short-probe
storage estimate. That timing and budget limitation remains unresolved.

Full-run relative-motion measurement fractions are **99.952% / 99.495% / 100% /
100%**. Relocalization/rejection is retained as unmeasured relative motion rather
than measured zero. VR2 acceptance improved from **99.588%** in the initial
source window to **99.898%** in the final one; its late fit-error distribution
remained stable. These are operational quality observations without physical
ground truth, not a new scientific-parity claim.

Native-process CPU averaged **36.781%** of this eight-logical-CPU machine over
the sampled interval, excluding the external verifier. This is a loaded soak,
not the isolated one/four-camera CPU benchmark. Arithmetic engine means are
**3.012 / 3.066 / 3.123 / 2.895 ms/frame**; camera acquisition/decode, pacing and
the rest of the process have different timing scopes. RSS peaks at **214.86 MiB**
and finishes at **197.23 MiB**, essentially flat over the final 30 minutes.
Final free space is **4.20 GiB**, the verification backlog is zero, and every
final recording commit was independently checked.

[Exact final analysis and limitations](benchmarks/linux-overnight-20261027.json)
retain per-rig quality, probe failures, control timing, storage fallback, CPU
boundaries, configuration hashes and a hash manifest of all verification reports.
The tested archive retains its original build commit; this later documentation
does not imply a rebuilt executable. The completed hourly monitor was removed;
the separate October 29 performance follow-up remains active.

## Daily controls, motion structure and sustained CPU — 2026-10-27

The final daily-control/async-finalization source passes formatting, strict
locked all-target workspace Clippy and **260 tests across 27 suites**, with
zero failures and zero ignored tests. The release build also passes; logs are
`runs/validation/buffered-final-{fmt,clippy,tests,release}.log`. The final
serial release demo and 300-image sample retain their unchanged accuracy gates.
All **5,157** VR3 image decisions, quality values and all **25** numeric columns
are byte-identical to the earlier fully gated result, including recovery.
`runs/buffered-release-oracles-20261027/report.json` records commands, hashes,
numerical comparisons and descriptive timings (22.695 seconds externally for
the full VR3 command). This is regression evidence, not a new general speed
claim. The platform/toolchain remain the native Linux host described below.

The daily toolbar now exposes reset, recording, camera, source/ball view,
normal/debug view, saved calibration review and configuration directly. The
record control changes from a red circle to a stop square. Calibration review
shows the saved ball, exclusion and laboratory-axis layers; an operator can
keep a valid layer, redraw one layer or enter the full guided sequence. Opening
and closing review preserves the configuration, playback clock and earlier view.
Saved geometry still requires visual inspection after the apparatus is bumped.

Native Linux visual verification covers 13 screenshots, including the toolbar,
configuration and individual calibration stages. A five-round native control
stress pass exercises 20 recording takes, all source/debug selections, saved
configuration, resets, rewind, invalid edits and calibration cancellation.
Independent full movie decoding matches every requested take's numeric and
quality rows. Source recordings retain the full image dimensions. Evidence:
`runs/daily-ui-release-final-20261027/report.json` and
`runs/daily-controls-release-final-20261027/report.json`; the corresponding development scripts are
`scripts/test_daily_ui_visual.py` and `scripts/test_daily_controls.py`.

[Whole-recording motion analysis](vr3-motion-structure.md) now includes complete
trajectories and derivatives rather than only the high-speed interval. Across
all 4,678 clear pre-hand images, heading-independent cumulative body movement
has combined RMS difference 0.029703 sphere radians from the supplied live
FicTrac data. Shared-capture-clock body-speed correlation is 0.990263 with RMS
ratio 0.999886. Low-motion framewise agreement and accumulated heading remain
weaker, and the raw world-path discrepancy is retained. Original post-hand
motion is assessed as an unreliable comparison; recovered motion is reported
separately, with no invented displacement during the occlusion.

Four serial native measurements use 60 measured seconds after five warmup
seconds, display at 30 FPS, and the supplied 320×240 camera configuration. One
camera uses 45.38% of one logical CPU with data only and 72.45% with complete
full-source MJPEG. Four cameras use 200.10% and 345.34%, respectively. On this
eight-logical-CPU machine those are 5.67% / 9.06% and 25.01% / 43.17% of the
logical total. Every camera delivers approximately 100.8–101.0 FPS. Engine
arithmetic means are 2.67 ms/frame for one camera and 2.86–3.33 ms/frame for
four, depending on mode. These are process CPU and wall-clock engine timings,
not physical-core headroom, power or GPU measurements. Small source-clock gaps
are reported rather than claiming zero sensor loss. The exact binary and all
interval boundaries are in
[the sustained CPU record](benchmarks/daily-cpu-sustained-20261027.json).

The [overnight supervisor](overnight-validation.md) retains independent numeric,
quality, wire and movie evidence while bounding media storage. Its actual
elapsed duration and health must be read from the run report; development
smokes and a scheduled monitor do not establish a completed overnight run.
Six supervisor regression tests additionally cover explicit completion,
missing-motion statistics, clock gaps, storage projection, process identity and
retention boundaries.

The final 90-second four-camera rotation preflight passes on the queue-32
binary: **35,374** consecutive numeric/quality rows, exact accepted FT bytes,
**28/28 complete videos** (20 full-source MJPEG, four uncompressed source and
four debug), 24 committed completion markers, zero verification backlog and a
clean native exit. Per-rig source-clock rates are 100.830–100.898 rows/s; maximum
capture intervals are 28.002 / 28.000 / 20.000 / 31.994 ms, including every file
boundary, with no interval above 50 ms. This establishes the measured rotation
improvement while retaining small capture gaps as diagnostics, not claiming
zero sensor loss. The earlier synchronous case reached 2.368 seconds; the first
async queue-eight repeat fixed acquisition but retained three partial movies.
Neither failed campaign was overwritten. [Final rotation evidence](benchmarks/linux-rotation-final-20261027.json).

## Linux four-camera and VR3 work — 2026-10-26–27

This local branch starts from `86b71d7d3f53e8a468c4728513479d07a2d3543a`
on `codex/ubuntu22-compat-ci`. Tests run natively on Ubuntu 22.04.5 x86-64,
Intel i7-6700K, with Intel HD 530 / NVIDIA GTX 1060 graphics and four
Arducam OV9281 cameras. Rust is 1.99.1; the release profile uses thin LTO,
one codegen unit and no target-CPU override. Original rig files and private
recordings are retained unchanged outside the repository.

`cargo fmt --all -- --check`, strict locked all-target workspace Clippy and
`cargo test --workspace --locked` pass: **252 tests across 27 suites**, zero
failures and zero ignored tests. The suite includes actual-video output,
unchanged decoder/numerical oracles, blurred known motion, recovery, native
frontend controls, transport and encoder-worker failure/overload checks.
Localhost access is required for the real transport tests. Raw final logs are
retained under `runs/validation/final-{fmt,clippy,tests}.log`.

The serial release hot-path checks pass: the 120-frame demo has component RMS
0.00030405 rad against its unchanged 0.003 gate; the 300-frame sample has path
RMS 0.044807 sphere rad and component RMS .008761/.008292/.014211 rad against
the unchanged 0.075-path/0.020-component gates. The full 5,157-frame VR3 pipeline
runs at 228.55 FPS (22.5636 s internally, approximately 22.565 s externally). The measured
same-AVI original FicTrac replay takes 19.8739 s externally, so this Rust run
takes about 13.5% longer. Internal original-stage timing has a different scope.
Separate decode and tracker timings, hashes, commands and CPU boundaries are
retained in [the final serial record](benchmarks/linux-tracking-final-queue8-20261027.json).

The supplied 5,157-image VR3 replay passes the unchanged 0.020-radian component
gates over the clear pre-hand interval, every rolling 100-image window and all
410 fast images. All 4,678 clear pre-hand and 337 clear post-hand images are
accepted. Recovery is confirmed at frame 4820, about 90–100 ms after visibility
returns, with zero relative motion across the missing interval. Complete-path
drift remains visible: 2.486 sphere-radian path RMS and 2.700 endpoint error.
[Accuracy, blur decoder oracle and complete trajectories](vr3-validation.md).

A source-paced native replay retains every quality decision and all 25 numeric
columns exactly as headless processing, including resizing during fast movement
and the hand interval. Four cameras pass 120 size/view transitions, minimum-size
holds, source reconnections, camera ownership checks and 60 seconds of loaded
ZeroMQ output at approximately 101 delivered frames/s each. An additional
VR3-start Open-four check verifies live images in both config-folder layouts.
The mixed-GPU resize failure and the tested default X11/GLES graphics path are
documented in [native validation](linux-native-validation.md), with platform
limits stated explicitly.

Rendering, compression, disk I/O and preview cadence are measured separately
from numerical tracking. [Recording evidence](recording-performance.md) retains
the slow synchronous baseline, exact-pixel remap improvements, actor overload
behavior, media completeness checks and source-clock limits. These measurements
do not prove absence of sensor/driver loss or unrestricted camera capacity.

Historical platform and release results below retain their original scope;
they are not evidence that this local Linux change ran natively on other OSes.

## v0.2 series local MATREX validation — 2026-10-26

On Apple M2 Pro / macOS 27.0.1 arm64 / Rust 1.99.1 (LLVM 23.1.1),
`cargo fmt --all -- --check`, locked strict all-target Clippy and the complete
locked workspace suite pass **208 tests across 27 suites**, zero failures.
Numerical reference tolerances and decoder-oracle constants remain unchanged.
Native FT remains the default; the new legacy MATREX mode changes ZeroMQ
serialization and transport, independently of the numerical engine and recording.

The unchanged MATREX VR **v0.1.0** and **v0.3.0** Mac players pass all four
version/framing functional cases, each with **four rigs and six complete scene
loads**. The final matrix contains **33,300 CSV observations**, including repeated
Unity render-frame observations, rather than a wire-packet throughput count.
Movement, heading wrap, legacy axes/units, Unity-only radius scaling, rejected-row
omission and publisher reconnect pass. Executables and managed assemblies,
including the original NetMQ 4.0.0.1 DLL, retain their verified hashes.
The old v0.1.0 receiver still emits **nine/twelve worker-stop timeout warnings**
for single/multipart framing and can report an archive-sharing error during
recording shutdown; its **strict clean-lifecycle result remains false**.
The v0.3.0 cases complete the tested lifecycle checks cleanly. These are headless
Mac player checks, not Windows/Linux player or unlimited scene-reload evidence.
[Exact unchanged-player results](benchmarks/matrex-legacy-20261026.json) retain
the functional and lifecycle decisions separately.

The exact historical NetMQ DLL passes Python-identical payload-byte and reconnect
checks in both framings; its .NET 7 cleanup still requires a bounded forced exit.
An independent modern libzmq subscriber receives **900/900 real sample rows**
exactly and in order: 300 native FT, 300 single JSON and 300 multipart JSON.
Native FT and UDP bytes remain unchanged. Actual-video tests retain all 300
numeric/quality rows with no subscriber and with an incomplete TCP greeting.
[Wire-oracle results](benchmarks/matrex-wire-20261026.json) and the
[MATREX setup and scope](matrex-vr.md) document the receiver and transport bounds.

Fresh v0.2.0 release binaries pass the **120-frame** known-motion demo at
component RMS **0.000304049695 rad**, against the unchanged **0.003 rad** gate.
The supplied **384×288, 300-frame** sample processes 299 tracked frames and one
initializing frame, with zero rejections. Path RMS is **0.0255938418 sphere rad**,
heading RMS **0.0136514303 rad**, and all rotation components pass the original
**0.020 rad** limit. The sample SHA-256 remains
`afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad`.

The fixtures ran serially with raw/debug video and wire outputs disabled, while
the native QA session was paused. The first workspace-output sample run took
**3.20373 seconds** in the CLI pipeline and **3.20983 seconds** externally; its
separate small accuracy comparison took **2.40367 seconds**. One documented
control run with outputs under `/tmp` took **0.79052 seconds / 379.50 frames/s**
in the sample pipeline and **0.79661 seconds** externally; comparison took
**0.00692 seconds**. Both runs produce byte-identical numeric outputs. The
control synthetic pipeline took **0.21155 seconds**, or **0.31204 seconds** for
the whole command including fixture generation and accuracy verification.
These descriptive validation runs show timing sensitivity to filesystem/cache
and process conditions; they do not establish a tracking or transport speed
change. Pipeline timing includes opening, decoding, tracking, ordinary numeric/
quality/SVG output and finalization. The release profile uses thin LTO and one
codegen unit without a target-CPU override.
[Both runs, commands, hashes, toolchain and timing boundaries](benchmarks/matrex-performance-20261026.json)
retain the complete evidence.

The final full suite and release build were repeated from the exact public tag
`a7382fe905f19166ee88c21959e1582e1bbf1356` in a verified temporary checkout
outside iCloud. All 405 tracked files matched their committed Git blobs. An
earlier workspace run encountered cloud-backed source-opening timeouts; the
unchanged tests pass in the isolated checkout, including the strict source-clock
and staged-frame assertions. No timing or numerical tolerance was relaxed.

The final native Mac desktop was visually checked after processing 120 sample
frames. Native FT defaults to a disabled single-frame publisher; selecting legacy
JSON exposes both framing choices and the topic. An occupied test port reports
its bind failure and restores the previous publisher. Correcting the port to
19871 applies successfully, clears the old error banner and preserves the paused
source counter. Mode controls, units and port fields render without clipping.
Raw native bridge and screenshot evidence is retained under the task's local
`runs/matrex-compat/` evidence folder; the release-CI/package results follow below
when completed.

### Unpublished v0.2.0 candidate

The immutable v0.2.0 source candidate remains at
`a7382fe905f19166ee88c21959e1582e1bbf1356`; its GitHub release was held in
draft. Linux native release validation passed all 208 workspace tests and
1,353 decoder tests. Three independent Windows jobs consistently failed the
same test fixture: the dummy malformed peer closed before reading the PUB
greeting, allowing Windows to report a normal connection reset before invalid
signature validation. The publisher remained healthy. The v0.2.1 test reads the
bounded greeting and keeps that peer alive through the original error-counter
assertion; no production tracking/transport code, assertion or numerical gate
was changed. [Candidate workflow](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40302522495).

## v0.1.4 published release — 2026-10-25

**[v0.1.4](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.1.4)** is
published as latest at **14:06:27 UTC**, from immutable source commit
`eed74b17dd29ed551140eab0d90c28c570308fcf`. All five expected platform assets
are present. The [four native release jobs](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40198640739)
pass formatting, strict Clippy, workspace tests, **1,353 decoder tests each**, the
120-frame known-motion demo and packaging. Linux and both Macs pass **188
workspace tests** each; Windows passes **185** applicable tests, with three
Unix-only cases excluded. All use Rust 1.99.1. The separate
[portability workflow](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40198640789)
also passes on all three operating systems.

The downloaded official Apple Silicon ZIP SHA-256 is
`ea22e1f7d4081475d7b6349518423dccdda14117fc55bea89a755f61b43b6692`,
matching GitHub's asset digest. Extraction outside the iCloud workspace passes
strict ad hoc signature verification, version, source-commit provenance and
legacy-alias checks. The bundled sample decodes and processes all **300 frames**;
its unchanged numerical reference comparison passes. The packaged app's
Calibrate again entry was visually checked. Complete live-circle pointer checks
were performed in the local native v0.1.4 build described below; repeating them
in the packaged preview hit a computer-use window-routing error. Native
Windows/Linux visual and physical-camera checks remain unperformed.

The release-source [website and manual](https://pvnkmrksk.github.io/hejjetrak/) pass
**1,558 local references across 36 HTML pages** and the download-controller
regressions. The subsequent documentation/evidence build passes **1,566 references**
across the same 36 pages. On the public GitHub README, one click on Download now opens the
website, selects v0.1.4 and completes the recommended Mac attachment without a
second download click. The **21,963,615-byte** browser download has the same
SHA-256 above. Other platform choices remain visible and the one-time URL intent
is consumed. Public autoplay remains muted and playing with readyState 4. GitHub
recognizes CITATION.cff and renders the citation control.

[CI hosts, complete native checks, asset digests and downloaded-app/browser evidence](benchmarks/release-v014-20261025.json)
record the execution. This does not establish notarization, Gatekeeper or
SmartScreen first-launch behavior. Source tags and prior releases were not moved.

## v0.1.4 local validation — 2026-10-25

On Apple M2 Pro / macOS 27.0.1 arm64 / Rust 1.99.1 (LLVM 23.1.1),
formatting, locked strict all-target Clippy and the complete locked workspace
suite pass **188 tests across 26 suites**, zero failures. Four package tests and
the download controller checks pass, including close-event races, new/legacy
asset selection, launch instructions and automatic-download defaults. Numerical
reference tolerances and decoder-oracle constants remain unchanged.

Actual native pointer checks verified Calibrate again starts rim redraw,
Diagnostics shows the full source and all saved geometry, and Full configuration
opens the separate editor. Two rim marks remain unconnected; the third shows a
circle immediately, a fourth refines it, and right-click removes a mark. Continue
advances to exclusions; Keep saved advances to lab orientation. Guidance stays
visible after the first click and the source image remains stationary. The
[calibration screenshot](images/calibration.png) shows the rebuilt native UI.
The magnifier's circle-only rendering is covered by render regressions; a native
hover-only magnifier check remains incomplete.

The release **120-frame** synthetic fixture retains component RMS
**0.000304049695 rad** against the **0.003 rad** gate. The supplied **300-frame**
video processes 299 tracked frames plus one initializing frame, with zero
rejections. Path RMS is **0.0255938418 sphere rad**, heading RMS **0.0136514303
rad**, and each rotation component passes the original **0.020 rad** gate.
Serial descriptive runs report 0.76094 seconds inside the sample CLI pipeline
and 0.76768 seconds external process wall time. These include source opening,
decoding, tracking, output and finalization; they are fixture checks on this
machine. [Inputs, hashes, toolchain, exact timings and results](benchmarks/polish-v014-20261025.json)
retain the evidence.

Browser checks at **1280 px** and **390 px** verify the landing page's muted
looping autoplay (readyState 4), no horizontal overflow, the light manual and
persistent reciprocal Website/GitHub links. The README download link carries
`?download=1`; controller tests verify one automatic attachment selection and
consumption of that intent. Unknown Mac hardware uses the clearly stated Apple
Silicon default, with Intel and other platforms visible. The published release and
official package are verified in the preceding section. This local
pass does not establish Windows/Linux GUI appearance or physical-camera capacity.

## Final polish — 2026-10-25

On the Apple M2 Pro / macOS 27.0.1 arm64 / Rust 1.99.1 (LLVM 23.1.1),
`cargo fmt --all -- --check`, locked strict all-target Clippy and the complete
locked workspace suite pass **184 tests across 26 suites**, with zero failures.
Two package-format tests, download platform/controller regressions and **1,379
internal website references across 34 HTML pages** also pass. No numerical
reference tolerance or decoder-oracle constant was changed.

The audit fixes invalid square marks outside the source image, uppercase legacy
`Y` flag interpretation, a backward-half-line sphere intersection at extreme
field of view, camera dimensions/timestamps before conversion, and bounded
comparison-row reads before allocation. Regression coverage includes an
independent visible-ray projection check and a real-video invalid-calibration
edit that preserves config, source position and the next numeric row.

The release **120-frame** known-motion demo passes component RMS
**0.000304049695 rad** against the unchanged **0.003 rad** gate. The supplied
**384×288, 300-frame** sample processes every frame: 299 tracked, one initializing,
zero rejected. Reference path RMS is **0.0255938418 sphere rad**, heading RMS
**0.0136514303 rad**; each rotation component remains below **0.020 rad**.
The unchanged sample SHA-256 is
`afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad`.

Cases ran serially with recording and wire outputs disabled. One descriptive
sample validation run reports **0.75128 seconds / 399.32 frames/s** inside the CLI
pipeline, including source opening, decoding, tracking, data/quality output and
finalization. External process wall time was **0.75685 seconds**. The synthetic
pipeline reports **0.20600 seconds**, while its whole command, including fixture
generation and accuracy verification, took **0.54101 seconds**. These are single
fixture checks, not a new performance comparison. The release profile uses thin
LTO and one codegen unit, without a target-CPU override. [Complete numerical
results, hashes, toolchain and timing boundaries](benchmarks/polish-20261025.json)
retain the evidence.

Native Mac pointer checks placed two rim marks with no fitted outline, then
confirmed the live outline on the third mark and refinement on the fourth.
Continue advanced to saved Exclusions. The final plain Continue label was
visually checked. Invalid drafts and cancellation from both running and paused
sources are covered by regressions. Light mode, the separate inspector, About
creator/attribution, full config disclosure and Start all were visually checked.
Open four rigs created four actual sample-video windows with distinct 2×2 grid
positions; the initially paused source retained frame 120 while the other three
processed all 300 frames with identical endpoints. After the focus correction, reopening the manager from VR4 brought it forward; Start all then completed all four sample sessions at the same endpoint. This checks video sessions,
not four physical cameras or sustained multi-camera capacity. Separate native
magnifier hover behavior was not completed in this pass; its layout/source
coordinate regressions remain in the suite.

Actual native sample playback supplies the README GIF, website MP4 and preview.
The website was inspected at **1280×900** and **390×844**, with no horizontal
overflow. Playback reached readyState 4 with no media error, and the download
chooser selected the published Mac asset, release version, size and SHA-256.
This does not re-test Gatekeeper or SmartScreen. Native Windows/Linux runtime,
camera and visual checks were not performed during this local pass.

The local Mac archive is ad hoc signed and its extracted signature and CLI
version are checked. It includes dependency notices and CITATION.cff. Publishing
and notarization remain separate release steps. The local toolchain emitted a
nonfatal missing-libLLVM warning while attempting its optional debug stripping;
the optimized binaries build and run. The existing `block` dependency also emits
a future-compiler compatibility notice; these environment/dependency warnings
were not suppressed.

## v0.1.3 release validation — 2026-10-25

On the same M2 Pro/Mac/toolchain below, formatting, strict locked all-target
Clippy and the complete locked workspace suite pass **173 tests**, zero failures
across 26 suites. The release workspace binaries/examples build successfully.
Linux package-format tests, download-selection tests and the static-site link
check also pass locally. The [v0.1.3 release workflow](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40180700262)
passes all four native jobs: Mac Apple Silicon, Mac Intel, Windows x64 and Linux
x64. Mac and Linux pass 173 workspace tests each; Windows passes 170 with
platform-specific exclusions. All pass the separate 1,353-test Rust decoder
suite. The [published release](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.1.3)
contains all five platform assets, including the Linux Debian/Ubuntu installer.
Its metadata, relative launcher links, extracted CLI, desktop entry and package
installation simulation pass on Ubuntu 24.04; glibc 2.39 or newer is required.
Native Windows/Linux camera hardware and GUI appearance remain unverified.

The official Apple Silicon ZIP SHA-256 is
`ba18e7856cec83d7b6ff5c25c9c775df5c383900ececb780d5f927966804aeb1`,
matching GitHub's asset digest. After extraction outside the workspace, strict
ad hoc signature verification, CLI version, bundled Lucide notice, all 300
sample frames and unchanged reference comparison pass. Its native preview and
red recording Stop/REC state were visually verified. This does not establish
notarization, Gatekeeper first-launch behavior or completion of the loupe check
below. [Release jobs, asset hashes and downloaded-app evidence](benchmarks/release-v013-20261025.json)
retain the exact checks.

The release 120-frame known-motion demo retains component RMS 0.000304049695 rad.
All 300 sample frames process, 299 tracked and zero rejected; all-frame numeric
bytes match the official v0.1.2 Mac run. Original path/heading/component gates
remain passing with path RMS 0.025593842 sphere rad and heading RMS 0.013651430 rad.
A serial predecoded benchmark with one warmup/five measured runs reports
**0.537 ms** median run-mean. Its boundary remains Tracker::process only, excluding
initialization, capture/decode/conversion, pacing, outputs and GUI.

Actual native checks verified tiny 96×96 image-only preview, source-paced normal
preview with Lucide icons, saved-ball geometry/aspect, and the inspector's Keep &
continue button. A native JSONL regression verifies tiny calibration expands to
640 px then restores 96 px without advancing its paused source. After the initial
window closes, another session records 100 additional numeric/quality frames;
a new window created afterward processes all 300 sample frames. Closing the
remaining sessions exits successfully. Point placement/deletion, transactional
Keep/Edit/Redraw, invalid-view atomicity and inspector/loupe placement pass the
regressions. The **native hover/loupe mouse check remains incomplete**: desktop
automation lost its window binding and reported noWindowsAvailable. No broader
physical interaction claim is made.

Focused actual-video recovery uses 80 black, 80 gradient and 80 unrelated-pattern
occluders after 900 learned frames. It recovers on the second clear image;
178 measured suffix rows have path RMS **0.0156229003** sphere rad and heading
RMS **0.0106615281** rad. The 70% partial-obstruction suffix has path RMS
**0.0154191124** and heading RMS **0.0105766746**. Every rotation component stays
below the unchanged 0.020 rad gate; path/heading limits remain 0.075/0.025.
Rare recovery now optimizes the complete ROI, addressing the sampled-refinement
heading regression without changing healthy-frame fitting. See the
[full recovery evidence](benchmarks/recovery-20261025.json).

Final binary `3ed79a4e06e2d08fce62f57ddd9a27c15f32d086fa547b63cc29daf0e762c05a`
sustains 120.12 FPS for one replay using **3.19% of total 12-core CPU capacity**
and 160.0 MiB median RSS. Sixteen replays with UDP/ZMQ sustain 119.97–120.12 FPS each,
using **22.65% total CPU** and 291.3 MiB combined RSS. All publishers remain alive
without application discards/failures. One paused replay uses 0.26% total CPU in
this case; prior idle variability remains unresolved. No physical 120 FPS camera,
GPU percentage or power measurement is implied. [Raw resource/profile/lifetime
results](benchmarks/resources-v013-20261025.json) retain boundaries and hashes.

## Resource, calibration and recovery pass — 2026-10-25

Local platform: Apple M2 Pro, 12 logical cores, 32 GiB RAM, macOS 27.0.1 arm64,
Rust 1.99.1 / LLVM 23.1.1. The complete locked workspace check passes **157 tests**,
with formatting and strict all-target Clippy. Coverage includes actual video/agent
operations, gapped Linux device-ID mapping, explicit resolution-reference edits,
interruptible source waits, immediate manual steps and running **120 → 30 → 120 FPS**
changes that retain timestamps and every recorded row. The published
[v0.1.2 release workflow](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40174402745)
passed all four native platform jobs and uploaded the four archives. The official
Mac archive passed ad hoc signature verification, the complete sample comparison
with unchanged RMS, and a native visual smoke test. These are v0.1.2 results;
v0.1.3 release results are recorded separately above.

The release known-motion demo processes **120 frames**, with rotation-component
RMS **0.000304049695 rad** against the unchanged **0.003** gate. The release sample
run processes **300 frames**: 299 tracked, zero rejected. Its all-frame numeric
bytes are identical to retained build `51dca65`. Reference path RMS is
**0.025593842 sphere rad**, heading RMS **0.013651430 rad**, and each rotation
component remains below **0.020 rad**. The serial predecoded profile (one warmup,
five measured runs) reports a median run-mean of **0.495 ms/frame**. Timing covers
ROI preparation, fitting, integration, atlas updates and numerical maps; it
excludes initialization, capture, decoding, conversion, pacing, file/wire output
and UI. Original Y/RGB decoder oracle constants remain unchanged and pass in the
full suite.

The long pure Rust monochrome replay contains **7,176 ping-pong frames** at
384×288, with 120 FPS metadata. Its full CLI run tracks 7,175 frames after
initialization, with zero rejected. Sixteen visible native replay sessions
sustain approximately **120 FPS each**, using **2.66 CPU cores** in aggregate;
enabling loopback UDP and ZMQ uses **2.92 cores**, with no application publisher
discards/failures. These 20-second process-resource cases test neither sixteen
physical cameras nor receiving applications. A source-paced **240 FPS** replay
also executes locally. See [complete measurements, unresolved idle variability
and limitations](resources.md).

The known-motion recovery regression learns **400 frames**, then rejects
**240 uniform, gradient or unrelated-pattern obstructions** without altering the
atlas/path. It confirms a known view under the minimum **256-evaluation**
recovery budget. Existing **0.08 rad** absolute-pose and **0.012 rad** next-motion
gates remain unchanged. The real-video case learns **900 forward/backward
frames**, inserts **240 occluders**, checks **180 clear frames**, adds **120 frames**
with a panel covering approximately **70%** of the visible ROI, then checks
another **180 clear frames**. Aligned measured-suffix limits remain **0.020 rad
per rotation component**, **0.075 sphere rad path RMS ** and **0.025 rad heading
RMS **. Missing gap motion is never reconstructed. See [recovery policy and
boundaries](recovery.md).

The final-source output-enabled spot check repeats sixteen sessions at approximately
120 FPS each, using 2.55 cores and 290.6 MiB median RSS. All 16 publishers remain
alive with no application discards/failures.

Native Mac previews, including 96×96 reset-only and 180×140 reset/record layouts,
and the guided inspector were visually inspected using the
public sample. The calibration canvas shows only its active geometry, and the
full-source view retains image proportions. Source-coordinate pointer, deletion,
corner-order and cropped-view transformations are additionally exercised by the
UI regressions. Fresh sample captures are included in the manual; no live-room
camera image is published. The actual built-in **640×480 / 30 FPS** camera
resource run has zero running capture skips, but its room view is not a
calibrated sphere.

## Tracking-stage refinement — 2026-10-25

Same local Mac/toolchain as below. Required formatting, strict locked all-target
Clippy and full locked workspace tests pass: **141 tests**, zero failures.
New checks cover precise/interpolated map-cell agreement, median threshold
cases, scalar-versus-linear extrema, full-resolution quality despite a sampled
blind spot, and a 120-frame known-motion trajectory. No existing tolerances or
decoder oracle constants changed.

The isolated predecoded-frame benchmark ran serially after builds/tests: three
alternating pairs, one warmup and five repetitions each. Mean tracking time is
**1.639 → 0.503 ms**, median P95 **1.900 → 0.581 ms**. It includes preparation,
fit, integration, atlas update and numerical maps; it excludes source decoding,
conversion, pacing, output, recording and UI. All 300 frames are processed,
299 tracked, zero rejected. Trajectory RMS against the original reference is
**0.025594 sphere rad** (previously 0.044807); heading RMS 0.013651 rad and all
rotation components remain under 0.020 rad. Endpoint error is 0.041252 sphere rad.
The full release CLI run independently repeats the same numeric results and
passes `compare` with its unchanged gates. Optimizer output intentionally need
not match previous bytes. [Full timing/accuracy record](tracking-performance.md).

The new 120-frame map test uses delta `[0.004,-0.007,0.003]` with a 50-evaluation
budget. Its path RMS is 0.009876 sphere rad, heading RMS 0.004819 rad and component
RMS `[0.003219,0.003285,0.005803]` rad. The release 120-frame pairwise synthetic
demo also retains component RMS 0.000304049695 rad against its original 0.003 gate.

Native camera measurements sample 100 distinct published frame timings after
20 warmup frames at actual 640×480/30: **3.045 → 1.398 ms mean**, with P95
4.969 → 2.183 ms. All sampled rows are tracked; running capture drops are zero.
These room-scene runs are not identical replay and do not establish physical
ball accuracy. The optimized app's arithmetic tracking mean was 1.237 ms over
all measured frames in that session. The UI now labels this mean separately
from full worker cost, and agent status exposes its sample count.

A **126-command native GUI run** verified four OS windows, full 300-frame sample,
new timing fields, config/history/clock preservation, map import/relocalization,
five raw/debug recording frames and decoding. The rendered diagnostic window
was visually inspected with the new `ms track` label. Local private evidence is
under ignored `runs/native-agent-tracking-stage-final/`; camera/paired reports
are retained in temporary task directories, with sanitized aggregate evidence
committed under `docs/benchmarks/tracking-20261025.json`.

This refinement's published cross-platform CI must be assessed separately from
the prior release evidence below. It does not claim 100-microsecond fitting or
sixteen-camera throughput.

## Performance refinement — 2026-10-25 (source after v0.1.1)

Local Apple M2 Pro / macOS 27.0.1 arm64, Rust 1.99.1, release thin LTO,
codegen-units=1. Required workspace formatting, strict all-target Clippy and
locked full tests passed: **136 tests**, zero failures. The separately pinned
Rust H.264 decoder library passed **1,353 tests**, including new independent
bit-reader/stop-bit oracles. No numerical tolerances, decoder constants,
optimizer budgets or supplied benchmark files changed.

The full 300-frame sample retains luma FNV64 `7e8d27f58358523f` and RGB FNV64
`7c112d90f2f74beb`, ordered presentation timestamps and recording-rate inference.
New bounded-pipeline tests cover full-queue shutdown and persistent decode
failures. Source-clock/config atomicity, all-frame quality evidence, recovery,
recording and real-video agent regressions remain passing.

Performance cases ran serially after the release build, without concurrent
builds/tests. Same-machine C++/Rust and retained v0.1.1 comparisons, exact timing
boundaries and remaining decoder gap are in [performance.md](performance.md).
The final alternating-pair repeat measured **211.50 → 411.05 fps**, with every
all-frame numerical row byte-identical and unchanged reference gates passing.
The release known-motion demo processed 120 frames of 160×160 video with delta
`[0.006,-0.009,0.004]`, zero rejected frames and component RMSE
**0.000304049695 rad** against the unchanged 0.003 rad gate. Its reported wall
time was 0.213823 s (561.21 fps); tracker-only throughput was 625.16 fps.

Actual native checks on the optimized executable:

- Four-window video agent run: **137 commands**, all 300 sample frames,
  endpoint discrepancy 0.054525318 sphere rad, pause/step, transactional config,
  preserved source clocks, template relocalization and five-frame raw/debug
  recording independently decoded. The rendered diagnostic screen was visually
  inspected; source/ROI aspect ratios and maps remained correct.
- Built-in camera run: **107 commands**, actual 640×480/30 and 1280×720/15
  format/readback, reopen, unsupported-control feedback, unchanged canonical
  config during recording, **33 matched all-frame/quality/raw/debug frames**.
  FFprobe and full independent decode passed. This room-scene test establishes
  acquisition/lifecycle, not physical sphere accuracy.
- Sixteen actual native windows completed 300 sample frames each, all with
  endpoint discrepancy 0.054525318 sphere rad and non-overlapping 4×4 native
  rectangles. This does not establish sixteen-camera sustained throughput.
- Local Mac archive assembly and strict ad hoc signature verification passed.
  The archive contains both the local decoder MIT license and patch provenance
  alongside registry dependency notices. This check did not publish a release.

Local evidence remains under ignored `runs/*foundation-final/` and
`runs/performance-foundation/`. Published v0.1.1 platform execution below is
historical; this refinement's cross-platform CI must be assessed separately.

Current release evidence is in **hejjetrak v0.1.1 refinement** below: 130 tests, full-video reference checks, native camera/recording checks, sixteen actual video windows and an extracted Mac bundle. The older sections retain the internal v0.3 checkpoint as history; their test counts and platform/release statements describe that checkpoint only.

## Internal v0.3 checkpoint — 2026-10-24

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

## hejjetrak v0.1.1 refinement

On 2026-10-25 the final local required check sequence passed: `cargo fmt --all -- --check`, strict workspace/all-target Clippy, and **130 Rust tests**. These include real egui pointer events for tool isolation and nearest-point deletion, exact square-corner slot replacement, uniform scaling of landscape/portrait/cropped input, native-worker busy-port rollback retaining its source/tracker, real codec recording selections, native view atomicity and camera numeric-value schemas. No scientific tolerances or decoder oracle constants were relaxed.

The actual compact native preview and its 240×180 minimum window were visually inspected on the sample; reset, record, source, diagnostics and settings remain accessible. A 168-command native GUI transcript exercised the separate inspector, diagnostic view, four independent native windows, complete 300-frame reference, config/history, recordings, source-clock continuity and template relocalization. Endpoint discrepancy remained 0.0545253183 sphere rad. Evidence: `runs/native-agent-v0.1.1/`, `runs/ui-v0.1.1/` and `runs/ui-tiny-v0.1.1/`.

The new native camera GUI passed **115 commands**: 640×480/30 capture, capabilities/unsupported-control behavior, transient raw+debug recording without config/revision changes, exact 1280×720/15 format/readback, invalid-format atomicity and 640×480/30 reopen. Both independently decoded H.264 recordings contain **35 frames** at 640×480 and 30 fps; numeric/quality counts agree. The scene was a room, not a calibrated ball; no physical tracking-accuracy claim follows. Metadata-only capability probes also independently captured both formats. Evidence: `runs/camera-gui-v0.1.1/` and `runs/camera-controls-native/`.

### Serial performance and exact-output optimization

Apple M2 Pro, macOS 27.0.1 arm64, Rust 1.99.1, release/thin LTO/codegen-units=1 with no target-cpu override. The unchanged 384×288, 300-frame sample has SHA256 `afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad`. Four alternating before/after pairs ran serially with no other task workloads; the first pair was warmup. Output was under `/tmp`, with raw/debug recording and wire sinks disabled. Values below are medians of the remaining three runs.

| Boundary | Retained pre-optimization build | Cached atlas rays + exact RGB conversion |
|---|---:|---:|
| CLI wall seconds, including open/decode, tracking, data/quality and finalization | 1.61120 | 1.43576 |
| End-to-end frames/s | 186.20 | 208.95 |
| Tracker::process frames/s, including numerical diagnostics | 469.94 | 547.14 |

Every run accepted all 300 frames and passed the unchanged reference gates. All four before/after `.dat` pairs were **byte-identical**. Full-video RGB FNV64 stayed `7c112d90f2f74beb`; the independent luma oracle stayed `7e8d27f58358523f`. The 120-frame known-motion release demo passed at rotation-component RMSE 0.0003040497 rad against its unchanged 0.003 gate (597.68 end-to-end fps for this separate 160×160 pairwise fixture). Core known-motion atlas/recovery tests also passed.

The supplied C++ log reports **517.25 fps**. The new Rust end-to-end result remains below that figure; the tracker-only number has a different timing boundary and does not establish a faster complete application. Hardware/build conditions of the supplied C++ run were not reproduced.

A separate serial experiment used one warmup/mode followed by five alternating full-video agent runs with outputs disabled or simultaneous UDP+ZMQ plus independent receivers. Median request/run-response wall time was **1.46040 s disabled** versus **1.49956 s enabled**, a descriptive **2.68% increase**, including subscriber CPU. All six enabled runs delivered exactly 300 UDP and 300 libzmq packets matching numeric rows, with zero publisher drops/failures. PUB delivery to arbitrary future subscribers is not guaranteed. `runs/zmq-output-benchmark/` contains the complete traces; `docs/benchmarks/v0.1.1.json` records summarized timings and boundaries.

### Final grid, startup and extracted-bundle checks

The final release source passed the required formatting, strict all-target Clippy and all 130 tests again, then built in release mode. Sixteen actual native windows, launched with `--windows 16 --grid 4x4`, each processed all 300 sample frames with source pacing enabled. The reported OS rectangles were distinct, non-overlapping 370×233 cells including decorations. Every session reached the same reference endpoint discrepancy of 0.0545253183 sphere rad. Final delivered-rate EWMAs ranged 29.98–30.57 fps and processing EWMAs 3.01–4.66 ms; these are snapshots of a ten-second recorded-video case, not a sustained sixteen-camera benchmark. File sources skipped no frames. Wire outputs and recording were disabled for this grid test. Evidence: `runs/grid-v0.1.1/`.

Ordinary startup was independently checked without an explicit config: the first enumerated Mac camera opened, and three unavailable secondary camera IDs fell back to the sample. All four native sessions advanced without errors and retained their VR1–VR4 configuration paths. The test used copies with wire ports disabled; original rig files were unchanged. Evidence: `runs/default-sources-v0.1.1/`.

A locally assembled Mac ZIP was extracted under `/tmp` and its ad hoc signature verified with `codesign --verify --deep --strict`. From working directory `/tmp`, its CLI decoded all 300 bundled sample frames, and its native desktop found the configuration under its own `Contents/Resources`, processed the full sample and opened the separate configuration inspector. The inspector was visually inspected. The local iCloud workspace attached Finder metadata to the generated staging bundle, so signing was completed on a copy under `/tmp` after removing only that copy's Finder metadata. No quarantine/security attributes were removed. The GitHub packager operates on ordinary runner checkouts. Evidence: `runs/packaged-v0.1.1/`.

Ordinary launch of that extracted bundle also opened camera 0 and delivered 195 frames during the eight-second startup check, with no session error; this was not a ball-accuracy or sustained-rate test. The actual uploaded Apple Silicon ZIP was independently downloaded, extracted and signature-verified. Its CLI decoded all 300 bundled frames, and its native GUI processed all 300 from `/tmp`, using its own bundled configuration and reproducing position `[3.6200534616363367,-2.62729631571117]`. Evidence: `runs/official-release-v0.1.1/`.

### Native release builds and Windows runtime audit

[Release workflow 40139627271](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40139627271) passed on GitHub-hosted Linux x64, Windows x64, Mac Apple Silicon and Mac Intel: formatting, strict Clippy, workspace tests, release build, known-motion CLI demo, license collection, packaging and upload. All four archives are attached to [v0.1.1](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.1.1). These are actual native test/demo executions, not cross-target checks. They do not establish Windows/Linux GUI visual or camera-hardware behavior.

An import-table audit found `VCRUNTIME140.dll` in both original Windows executables, despite the runner having that runtime installed. The Windows archive was therefore rebuilt from the **unchanged v0.1.1 source tag**, using `RUSTFLAGS=-C target-feature=+crt-static`; [workflow 40140958508](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40140958508) repeated formatting, strict Clippy, full tests, release build and demo before replacing only that archive. LLVM's PE import inspection of the downloaded replacement confirms that neither executable imports VCRUNTIME/MSVCP/MSVCR/CONCRT redistributable DLLs. Native system DLLs remain expected. Replacement ZIP SHA256: `dbb16f0771047d777b37b34f13a9038b6902bca189747d64b0e6d2e3bde7bdea`. Evidence: `runs/windows-portability-v0.1.1/`. Main now defaults Windows x64 source builds to static CRT, and the release workflow includes an import guard for future archives. No immutable release tag was moved.
