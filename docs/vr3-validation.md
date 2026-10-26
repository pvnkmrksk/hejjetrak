# Private VR3 occlusion benchmark

The selected complete-ROI/later-confidence policy tracks every clear pre-hand
image and every image in the fast section. Its pre-hand increment RMSE against
the supplied live data is `[0.006677, 0.006547, 0.009413]` radians, and every
rolling 100-image interval passes the unchanged 0.020-radian component limit.
It also recovers after the hand. Accumulated trajectory remains approximate:
path RMSE is 2.485641 sphere radians and heading RMSE is 0.061451 radians.
The detailed evidence below retains upstream live and same-video replay
comparisons, all quality decisions, and the remaining trajectory differences.

The supplied `20261026_212053` VR3 recording contains 5,157 decoded 320×240
H.264 images at 100 fps (51.57 seconds). The source `.dat` contains 5,163
capture rows. The original `vidLogFrames` file maps recorded video indices to
capture IDs: there are missing capture images before video frames 1 and 183.
Direct row-index comparison is therefore invalid. The benchmark script compares
single-frame rotations only when successive recorded images have consecutive
source IDs; it never interpolates the missing images.

Visual inspection selected these conservative, half-open video-frame intervals
before assessing candidate tracking decisions:

| Interval | Video frames | Purpose |
|---|---:|---|
| Clear pre-hand | `[2,4680)` | Compare increments against upstream |
| Acceleration / fast rotation | `[1690,2100)` | Require every image to remain tracked; inspect continuous trajectory |
| Hand/reappearance guard | `[4690,4820)` | Inspect rejection and recovery decisions |
| Clear post-hand | `[4820,5157)` | Verify resumed motion and fit quality |
| Settled post-hand | `[4920,5157)` | Compare against an independent fresh atlas |

The ball is clearly visible again by approximately frame 4810. The clear-post
interval starts later to avoid ambiguous boundary images. These intervals are
fixture annotations, not an automatic occlusion detector.

Upstream was configured with `opt_max_err=-1`, `max_bad_frames=-1`, and global
search disabled. Its median binary-map fit error rises from 3,502 before the
hand to 24,936 after the hand, with median reported rotation norm 3.178 radians
per image after the ball becomes clear again. Its post-hand output is therefore
not a valid accuracy oracle. The complete original data and logs stay intact.

`scripts/benchmark_vr3.py` reports all-frame decisions and enforces the existing
0.020-radian per-component increment RMSE gate over the clear pre-hand interval.
Rejected estimates are included as emitted; a tracker cannot improve its score
by omitting difficult frames. It additionally requires 95% clear-pre-hand
acceptance, 90% clear-post-hand acceptance, a confirmed relocalization, nonzero
post-hand motion, zero relative rotation on every rejected/relocalized row, and
post-hand p95 fit error no more than twice the accepted pre-hand p95. Path and
heading drift are reported separately; this 51-second fixture does not establish
long-sequence path parity or physical ground truth.

The full pre-hand average is supplemented by every overlapping 100-image window
whose source IDs are consecutive. Each window retains unaccepted decisions and
must satisfy the same 0.020-radian component limit. The fixed fast section must
also satisfy that limit and retain every image as `tracked`. Its continuous path
and relative heading are reported after aligning only the initial position and
heading; no later rotation, scale fitting or gap interpolation is applied.
The aggregate `passes` field covers the declared increment/recovery gates.
It does not turn the separately reported long-path drift into a parity claim.

Tracked-only increment RMSE is also reported as a diagnostic, alongside the
number of retained frames. Fixed non-overlapping 100-image windows require all
100 images to be tracked and source IDs to be consecutive. Their path metrics
remove starting position offset; an additional variant also rotates by the
starting heading discrepancy. Relative-heading error removes the starting
heading offset. These local diagnostics distinguish continuing estimation error
from accumulated path/heading loss across an unmeasured interval; they never
replace the all-frame gates or suppress the reported global drift.

An independent control resets the tracker immediately before video frame 4820
and tracks the remaining 337 images with a fresh atlas. It uses the same Rust
decoder and numerical configuration, without transcoding the video. Comparing
the settled suffix against this control tests recovered increments independently
of the failed upstream output. It retains the same 0.020-radian component gate.

## Reproduce

Build the release CLI, then run this development analysis command with the
private VR3 directory. All output directories/files must be new:

```sh
cargo build --release --locked -p fictrac
python3 scripts/benchmark_vr3.py \
  --reference-dir /path/to/VR_array/VR3 \
  --candidate-dir runs/vr3/run \
  --binary target/release/fictrac \
  --suffix-control-dir runs/vr3/control \
  --make-suffix-control \
  --output runs/vr3/comparison.json \
  --require-pass
```

The script writes a config copy with raw/debug media recording and wire outputs
disabled. It does not change numerical tracking settings or original camera
files. It retains source hashes, all-frame data, quality decisions, summary,
control requests/responses, and accuracy/acceptance gates. The control performs a
second decoding/tracking pass; its time is excluded from the candidate CLI's own
summary. Python and any independent FFmpeg inspection are development analysis
tools, never runtime application decoders.

## Initial branch result

Before the recovery fix, the branch accepted 92.82% of the clear-pre-hand
interval and 73.00% of the clear-post-hand interval. Increment RMSE was
`[0.038731, 0.014507, 0.015300]` radians against upstream. A healthy fast-motion
block near frames 1699–2036 was lost; outside that block component RMSE remained
below 0.013 radians. The tracker eventually relocalized at frame 4911, 910 ms
after the conservative clear-post interval begins. Its settled recovered
increments agreed with the fresh-atlas control within
`[0.011394, 0.010158, 0.015085]` radians. This established eventual recovery, but
failed the declared healthy accuracy and acceptance gates. Timing from that run
overlapped a desktop build and is not a serial performance measurement.

The fast-motion dropout was traced to atlas coverage during post-recovery
learning probation. Before acceleration, 65.75% of the equal-area map was
observed. An independent geometry probe reconstructed the same 2,489 valid ROI
sites and aligned upstream poses to the candidate's atlas pose at frame 1698.
Expected-view overlap with that frozen map falls from 50.9% at frame 1704 to
33.6% at 1705 and 19.2% at 1706; the objective requires more than 25%. Thus the
correct rapidly rotating view becomes unscoreable while learning remains
deferred. The atlas exports at frames 1699, 1706 and 1726 were identical. The
probe uses upstream poses as an approximate motion reference, not physical
ground truth. Increasing local optimization from 50 to 200 evaluations did not
resolve the failure.

## Spatial-coverage recovery result

The accepted-acceleration policy avoids declaring a gap during a consistent
visible speed ramp, so the atlas can learn the newly visible surface. Recovery
also retains spatially distinct older views: when the 256-pose bank is full,
it replaces a redundant neighbor only when the incoming view improves its
spacing. Recency alone no longer evicts a unique older viewpoint. This retains
the bounded bank, complete-score verification and missing-motion semantics.

An intermediate release run with these changes passed every declared VR3 gate.
All 4,678 clear-pre-hand images were tracked. Increment component RMSE was
`[0.008396, 0.008295, 0.011782]` radians against the mapped upstream reference.
The tracker relocalized at video frame 4820, approximately 90–100 ms after the
ball becomes clearly visible, and accepted all 337 clear-post-hand
images (one relocalized, 336 tracked). Their p95 fit error was 4,933, within the
healthy range. Settled recovered increments agreed with the independent
fresh-atlas control within `[0.011735, 0.010440, 0.015466]` radians per component.
Every rejected/relocalized row retained zero relative rotation.

The pre-hand global path RMSE remained **1.564794 sphere radians**, with wrapped
heading RMSE **0.080082 radians** and endpoint error **2.084281 sphere radians**.
The reference travels 273.950286 sphere radians and ends 12.652455 sphere radians
from its initial position. Path RMSE is 0.5712% of traveled length, or 12.3675%
of the endpoint displacement. These ratios provide scale context; this result
passes the declared increment/recovery gates and does not establish long-path
or heading parity. Final mature-atlas learning validation is recorded separately
after the final implementation is built and rerun.

## Same-recording upstream replay and trajectory audit

The existing local FicTrac 2.1.1 binary was also run against the exact supplied
AVI, with a config copy and new output directory. Numerical settings, geometry
and exclusions remained identical; only source, display, media recording and
output destinations changed. It processed all 5,157 images. This is an external
development oracle, not an application dependency. Its binary hash, config and
console/data logs are retained in the local evidence directory.

That replay is distinct from the supplied `.dat`, which was produced live before
the video encoder and includes six capture images absent from the recording.
Even upstream replay differs from upstream live data: its clear-pre-hand
rotation RMSE is `[0.006468, 0.006371, 0.009224]` radians, while raw path RMSE is
5.310276 sphere radians, heading RMSE 0.310140 radians and endpoint difference
6.298376 sphere radians. Recorded-video input, missing images and numerical
optimization therefore materially affect accumulated trajectory. This comparison
does not make those errors acceptable by definition or establish physical truth.

An early mature-atlas experiment, starting confidence damping after 64 distinct
teaching images per cell, retained every pre-hand and fast image and passed every
rolling increment gate. Its clear-pre-hand rotation RMSE was
`[0.008813, 0.008913, 0.012498]` radians. However, its trajectory relative to the
supplied live data worsened to 4.921565 sphere radians path RMSE, 0.164528 radians
heading RMSE and 3.436603 sphere radians endpoint difference. The prior
spatial-coverage result remains the more accurate live-reference trajectory.
This regression is tracked explicitly while the later-maturity policy is
validated; passing increment gates alone is insufficient evidence for a
trajectory improvement.

The same-video oracle gives additional context, rather than replacing that
regression: across the fast section, the early mature candidate's path RMSE
against upstream replay is 0.826082 sphere radians after initial alignment,
versus 3.961681 for the prior spatial-coverage candidate. Its relative-heading
RMSE is 0.051538 versus 0.157526 radians. The full pre-hand same-video path errors
remain 5.307077 and 3.814166 sphere radians respectively. All variants and the
upstream live/replay difference remain visible in the trajectory figures.

`scripts/plot_vr3.py` creates standalone PNG and PDF figures for the full path,
fast-section path, every fast-image increment, rolling component errors, global
path/heading differences and all-frame quality decisions. Optional `--og-replay`
and repeated `--compare LABEL=DIRECTORY` add comparison traces. Initial
alignment is explicitly labeled; trajectories are never rescaled or refitted.
The plotting dependency is needed only in a development analysis environment.

## Complete-ROI fitting with later confidence inertia

The selected numerical policy uses the complete ROI for resilient local fitting,
retains the accepted acceleration and spatial recovery changes, and delays mature
cell damping until after 1,024 distinct accepted teaching images of that cell.
New surface and young confidence retain the original learning behavior. This
choice removes candidate-generation subsampling from the scientific fit and
matches the requested later accumulation rather than suppressing early evidence.
Its configured optimizer budget remains 50 and its error limits are unchanged.

The complete 5,157-image replay, including a newly generated independent suffix
control, passes every declared increment/recovery gate. It tracks all 4,678
clear-pre-hand images, every one of the 410 acceleration/fast-section images,
and all 337 clear-post-hand images. Relocalization occurs at frame 4820; missing
intervals and that relocalization retain zero relative motion.

| Comparison | Component increment RMSE, rad |
|---|---|
| Clear pre-hand against supplied live data | `[0.006677, 0.006547, 0.009413]` |
| Fast section against supplied live data | `[0.009214, 0.008631, 0.011502]` |
| Worst of 4,479 overlapping 100-image windows against live data | `[0.010889, 0.010467, 0.014790]` |
| Clear pre-hand against upstream replay of the same AVI | `[0.006252, 0.006072, 0.008987]` |
| Worst of 4,579 overlapping windows against same-AVI upstream replay | `[0.011094, 0.008929, 0.012329]` |
| Recovered settled suffix against fresh-atlas control | `[0.008255, 0.006261, 0.010896]` |
| Unchanged increment limit | `0.020` on each component |

One hundred live-reference windows intersect the unavailable source-image delta
at video frame 183 and are explicitly excluded. No unaccepted candidate window
is hidden. The same-AVI upstream replay requires no frame-map exclusions.

Accumulated trajectory remains approximate. Against the supplied live data,
clear-pre-hand path RMSE is **2.485641 sphere radians**, heading RMSE is
**0.061451 radians**, and endpoint difference is **2.699863 sphere radians**.
The path RMSE is 0.9073% of traveled length and 19.6455% of endpoint displacement;
both raw errors and scale ratios are retained. Against upstream replay of the
same AVI, those metrics are 5.390370 sphere radians, 0.309163 radians and
8.838010 sphere radians respectively. These are reported limitations, not hidden
by the passing increment gates. The complete-ROI policy improves increment
agreement against both references and heading agreement against the live data;
the spatial-only candidate still has the smaller live-reference path RMSE.

The final trajectory figures retain the upstream live path, upstream same-AVI
path, spatial-only candidate, early-confidence experiment and selected policy.
The fast section is visibly measured throughout rather than frozen during its
high speed. Full path and heading differences remain visible beside the motion
curves. This constitutes fixture-specific tracking/recovery evidence and does
not establish unrestricted trajectory parity or physical ground truth.

The initial numerical run overlapped correctness testing, so that run's wall
time is not used as a performance claim. The final serial release measurement
below followed accuracy and native multi-camera stability checks.

## Final serial release validation

The final Linux release passed the complete 252-test workspace gate, formatting
and strict all-target Clippy before serial performance measurement. The machine
is Ubuntu 22.04 x86-64, Intel i7-6700K (four physical/eight logical cores), Rust
1.99.1 / LLVM 23.1.1, release thin LTO with one codegen unit. The measured CLI
SHA256 is `b7b30a1dd29d4601e5fea3d3f403b16760c35ed51904819a7c43c471d29ad31f`.
The [final machine-readable report](benchmarks/linux-tracking-final-queue8-20261027.json)
retains toolchain, input/config hashes, unchanged oracle limits, external process
wall/CPU and internal timing boundaries. No project build or native workload
overlapped these serial cases; filesystem caches were not flushed.

| Fixture | Images | External command wall, s | Internal tracking pipeline FPS | Tracker-only FPS |
|---|---:|---:|---:|---:|
| Known-motion demo | 120 | 0.6901 | 235.04 | 255.70 |
| Public sample | 300 | 1.8236 | 164.69 | 363.92 |
| Full VR3, one candidate pass | 5,157 | 22.5655 | 228.55 | 397.99 |

Demo external wall includes synthetic video generation. Its rotation RMSE is
0.00030405 radians against the unchanged 0.003-radian limit, with no rejection.
The public sample also has no rejected images and passes its unchanged limits:
path RMSE 0.04480675 sphere radians, heading RMSE 0.01318481 radians and component
increment RMSE `[0.00876080, 0.00829214, 0.01421073]` radians.

Final VR3 all-frame numeric data, quality decisions and accepted-only data are
byte-identical to the selected complete-ROI/later-confidence run above. The
freshly generated suffix control and every live/same-AVI increment/recovery
gate pass. Thus the reported trajectory figures and limitations also describe
the final numerical engine exactly.

Internal pipeline time starts at `track` entry and includes source/config/tracker
setup, first-frame decoding, the tracking loop, media/map/path finalization and
numeric fsync. It excludes process startup and final summary serialization/file
promotion/stdout. Tracker-only time measures `tracker.process`. Their difference
also includes setup, conversion, thresholding and file/output work and is not a decoder
measurement. Separate serial `inspect --decode` processes decode the public
sample in 1.2270 s and VR3 in 20.6621 s, including process startup/metadata output and
luma-frame allocation; they do not reproduce the complete RGB/threshold tracking
pipeline. The VR3 analysis wrapper takes 45.9708 s because it includes a second
full decoding/tracking pass for the independent suffix control.

The historical same-AVI upstream process takes 19.8739 s externally, compared
with 22.5655 s for the final Rust candidate, approximately 13.5% longer. Upstream's
internal log reports 264.48 FPS, whereas its live camera log reports 100.93 FPS.
These different boundaries are retained; no general speed superiority or
comparable upstream CPU figure is inferred from them.

## Target-recording decode and blur check

The actual Rust tracking-source RGB images were exported through the headless
snapshot command at video frames 0, 1695, 1700, 1705, 1750, 1800, 2000 and 4810.
An independent FFmpeg 4.4.2 decode exported those exact image indices without
resizing, sharpening or transcoding. All eight comparisons pass a predeclared
rounding gate of RGB RMSE at most 1.5 intensity levels on the 0–255 scale and
maximum absolute channel difference at most 3. Measured RMSE is 1.319–1.399 and
maximum difference is 2–3 levels; RGB hashes are retained, not claimed identical.

The ratio of Rust to FFmpeg mean neighboring-pixel gradient is
0.99895–1.00125. On these selected stationary, accelerating, fast and reappearing
images, neither source pixels nor edge contrast show evidence of material extra
blur introduced by the Rust decoder. This is a selected-image target-recording
check, not a new full-video byte-identity oracle or a physical exposure-time
measurement. The existing complete bundled-video decoder oracle remains a
separate required regression.

`scripts/decoder_oracle_vr3.py` reproduces the check and retains the config copy,
typed requests/responses, original RGB exports, independent RGB exports, hashes
and per-image metrics. FFmpeg remains a development analysis tool and is never
an application runtime decoder.
