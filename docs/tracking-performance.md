# Tracking-stage latency — 2026-10-25

Current robust source uses complete-ROI local fitting in resilient mode. The
subset-search timings below describe the earlier implementation. The [VR3
validation](vr3-validation.md) reports current full-video accuracy and trajectory
limits; current serial timing evidence is recorded separately in validation.md.
On the final 2026-10-26 Linux release build, the complete-ROI policy's supplied
300-frame sample has path RMS **0.044807 sphere radians**, heading RMS
**0.013185 rad**, and component RMS **.008761/.008292/.014211 rad**. All original
gates below pass. The earlier accelerated result of 0.025594 path RMS is retained
as historical evidence, not the selected policy's current result.

This assessment measures per-frame numerical processing. Capture, decoding,
playback rate, recording and process throughput are outside the timed engine.
It supersedes the earlier end-to-end comparison for questions about fitting
latency; [that comparison](performance.md) remains historical pipeline evidence.

## Results on the tested Mac

Apple M2 Pro, macOS 27.0.1 arm64, Rust 1.99.1, release thin LTO,
codegen-units=1. Both isolated benchmark drivers include line-table debug info.

| Tracking-only measurement | Before | Accelerated |
|---|---:|---:|
| Predecoded sample: mean frame time | 1.639 ms | **0.503 ms** |
| Predecoded sample: P95 frame time | 1.900 ms | **0.581 ms** |
| Mac camera: mean sampled frame time | 3.045 ms | **1.398 ms** |
| Mac camera: P95 sampled frame time | 4.969 ms | **2.183 ms** |

The controlled sample is **3.26× faster**. Every measurement includes ROI
preparation, fitting, integration, atlas updates and numerical diagnostic maps.
The 300-frame 384×288 sample is decoded and converted once before timing.
Three alternating before/after pairs each run one discarded warmup and five
complete repetitions; initialization is excluded. The table gives the median
of the run means and the median P95 across measured repetitions. These are
repeated observations of one fixture, not thousands of independent videos.
Source SHA256: `afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad`.

The live-camera runs use native 640×480/30 acquisition, the same calibration and
q_factor 6, no recording or network sinks, and 100 distinct published frame
timings after 20 warmup frames. All sampled frames were tracked, with zero
reported running capture drops. Polling can miss intermediate status updates;
these are sampled native timings, not a full capture latency distribution.
The room scene cannot be replayed identically, and does not establish physical
sphere accuracy. The optimized app's separate cumulative engine mean was
1.237 ms in that run. Live scenes and source-speed scheduling produce different
measurements from the controlled in-memory sequence.

The supplied FicTrac log ends with **1.8 ms optimization + 0.1 ms mapping**.
The earlier same-Mac C++ run reports **1.6 + 0.1 ms**. Those are rounded to
0.1 ms and exclude preprocessing performed by its grabber thread. Rust's
0.503 ms scope includes ROI thresholding and numerical diagnostic preparation,
so it is broader than that optimization/map subtotal. Neither comparison uses
decode throughput or the C++ supervisor/exit sleeps.

## What changed

Release sampling found approximately half of the original numerical CPU time
in repeated scalar `atan2` calls during atlas projection. Fixed sites are now
normalized once. A shared 1025-entry atan table interpolates longitude with
less than 7.8e-8 radians interpolation error; cell-boundary guards fall back to
the precise projection whenever the error could change the discrete map cell.
Tests exercise map boundaries and poles at several resolutions.

Threshold preparation uses a median-selection network and linear-time
prefix/suffix extrema instead of repeatedly scanning each window. Masked
pixels, saturated-white handling and clipped borders retain their semantics.
Exhaustive binary threshold cases verify the median network; independent
scalar windows verify the extrema pass.

Local BOBYQA searches a deterministic distributed subset of valid ROI sites,
about 1024 or fewer, then evaluates the candidate and predictor on **every
valid site** and selects the lower complete score. Two evaluations are reserved
inside the configured budget. Small ROIs or budgets below 16 retain complete
search. Reported error and rejection thresholds always use complete scoring;
an explicit blind-spot test prevents sampled error from hiding a bad fit.
Global recovery keeps its existing candidate lattice and complete final score.

There are no changes to camera/lab axes, integration, timestamps, map resolution,
rejected-frame evidence, calibration or decoder pixels. Optimizer trajectories
may differ; byte identity is no longer an acceptance requirement for this
algorithm change.

## Accuracy

All 300 sample frames remain present, with 299 tracked and zero rejected.
The original reference gates are unchanged:

| Metric against supplied FicTrac output | Before | Accelerated | Gate |
|---|---:|---:|---:|
| Path-point RMS, sphere radians | 0.044807 | **0.025594** | 0.075 |
| Wrapped heading RMS, radians | 0.013185 | 0.013651 | 0.025 |
| Rotation X/Y/Z component RMS, radians | .008761/.008292/.014211 | .009476/.010925/.017820 | 0.020 each |
| Endpoint discrepancy, sphere radians | 0.054525 | **0.041252** | 0.075 regression gate |

Path RMS improved on this clip; rotation-component RMS rose slightly while
remaining below the original gates. This is a measured tradeoff, not unrestricted
accuracy parity. A new independent 120-frame binary-texture known-motion test
uses delta `[0.004,-0.007,0.003]` rad/frame and a 50-evaluation budget. Its path
RMS is **0.009876 sphere radians**, heading RMS **0.004819 rad**, and component
RMS `[0.003219,0.003285,0.005803]` rad. Existing recovery, off-axis, occlusion,
source-clock and reference checks remain in the suite.

The engine has not reached 100 microseconds. It estimates three-axis rotation
through repeated atlas-fitting evaluations; a single-pass edge tracker has a
different workload. The remaining dominant cost is the candidate projection
and score loop. No throughput claim for sixteen physical cameras follows from
these latency measurements.

## Display and reproduction

The status strip now labels **ms track**: an arithmetic mean of
`Tracker::process`, including its numerical maps, reset at a new configuration
or tracking reset. Its tooltip separately shows the full worker cost and source
headroom. Engine construction, capture, RGB conversion, recording and preview
publication are excluded from the tracking mean. Native agent status exposes
`tracking_ms` and `tracking_samples`; `processing_ms` retains its full-worker
meaning for existing clients.

```sh
cargo build -p fictrac-io --example profile_tracking --release --locked
target/release/examples/profile_tracking examples/sample.mp4 \
  examples/benchmark/config.txt 5 /tmp/hejjetrak-tracking-new
```

This fixture driver bounds retained decoded pixels to 512 MiB, times the shared
engine only, writes all numeric rows, reports mean/median/P95/max, compares the
trajectory with the supplied reference and fails if its existing gates fail.
Performance cases must run serially with no concurrent builds or tests.
`scripts/benchmark.py` remains a separate throughput tool; its optional
`--require-identical` is for exact-work optimizations, not algorithm changes.
Raw results: [tracking benchmark record](benchmarks/tracking-20261025.json).
