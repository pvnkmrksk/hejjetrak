# Complete VR3 motion structure and derivatives

The complete pre-occlusion body movement agrees closely with the supplied
FicTrac data, even though accumulated heading differences produce a visibly
different world path. Across all **4,678** clear pre-hand images, the combined
RMS difference in heading-independent integrated forward/side movement is
**0.029703 sphere radians**. The two component RMS differences are
**0.020669 / 0.021332**, and endpoint differences are
**+0.026678 / −0.017858 sphere radians**. These are raw cumulative outputs;
no later translation, rotation, scaling or re-anchoring is applied.

The corresponding raw world-path RMS difference is **2.485375 sphere radians**
over all 4,678 images. The existing strict benchmark reports **2.485641** when
excluding the one image-pair that spans absent source captures. Both figures
retain the accumulated discrepancy rather than fitting it away. This comparison
supports separating heading accumulation from body movement; it does not prove
that either implementation measures the physical trajectory without error.

The [machine-readable analysis](benchmarks/vr3-motion-structure-20261027.json)
records both the original live data and an original-FicTrac replay of the same
compressed AVI. Against that same-AVI replay, heading-independent body movement
RMS is **0.255918 sphere radians**, while world-path RMS is **5.390370**. The
original live/replayed outputs themselves differ, so neither is physical ground
truth. The existing [increment/recovery gates](vr3-validation.md) are unchanged.

## Derivatives over the whole clear interval

Comparisons use the predeclared clear interval `[2,4680)`, not just the fast
section. The source `.dat` has 5,163 capture rows while the AVI contains 5,157
images. The supplied frame map identifies images absent from the recording.
There are **4,677 comparable consecutive-capture pairs** in this interval;
the missing pair is reported, not interpolated. Unaccepted or relocalized zero
deltas are never counted as measured zero velocity.

Original live timestamps have approximately 8/12 ms spacing, with mean
**9.908572 ms**. Encoded AVI presentation timestamps are spaced at **10 ms**.
The report therefore provides two derivative comparisons:

- **Each output's own clock:** the actual reported velocities, including the
  difference between live capture/host timing and encoded video timing.
- **One shared reference image-pair clock:** both unchanged motion estimates
  divided by the same original recorded capture interval. This isolates motion
  disagreement from time-base jitter; it does not rewrite application output.

The original implementation can use a device timestamp or a host-clock fallback;
these recorded timestamps are not an independently verified shutter clock.

| Measurement, shared recorded capture clock | Pearson correlation | Residual RMS |
|---|---:|---:|
| Camera angular velocity X | 0.991111 | 0.718804 rad/s |
| Camera angular velocity Y | 0.987519 | 0.702252 rad/s |
| Camera angular velocity Z | 0.941326 | 1.013198 rad/s |
| Body forward velocity | 0.991064 | 0.718849 sphere rad/s |
| Body side velocity | 0.984907 | 0.886512 sphere rad/s |
| Uncorrected world north/east velocity | 0.986605 / 0.989263 | 0.952146 / 0.887224 sphere rad/s |
| Wrapped-heading derivative | 0.859694 | 0.856598 rad/s |
| Body speed | 0.990263 | 0.784339 sphere rad/s |

Body-speed RMS is **8.337777** for the original and **8.336828 sphere rad/s**
for Rust: ratio **0.999886**. The diagnostic zero-intercept scale is **0.995462**;
no scale correction is applied. On each output's own clock, body-speed
correlation is **0.946881** and residual RMS **1.842457 sphere rad/s**, showing
why the timing convention must be stated.

Agreement is not equally strong everywhere. Outside the fast section,
body-speed correlation remains **0.9883**; after it, **0.9751**. In the first
low-amplitude 16.9 seconds, however, framewise speed correlation is only
**0.1093** (reference/candidate RMS **0.7233 / 0.7560**, residual RMS **0.5367
sphere rad/s**). Heading-rate correlation in the fast section is **0.5190**.
The report retains these weaker results. High aggregate correlation does not
establish derivative identity on every image, nor physical accuracy at low
motion amplitudes. Every overlapping 100-image shared-clock window is included
in the summary; low-signal windows can have weak or negative correlations.

## Recovery is assessed separately

The clear suffix `[4820,5157)` contains one relocalization and **336 tracked
measurements**. On the application's encoded-video clock, recovered angular
speed has median **4.263**, p95 **7.143**, maximum **8.561 rad/s**; clear pre-hand
maximum is **30.307 rad/s**. Recovered binary-map MSE median/p95 are
**3,852 / 4,681**, compared with **3,750 / 5,108** before the hand. These are
plausible healthy-scale estimates rather than the original's post-hand runaway
rotations. The settled recovered increments also agree with an independently
initialized suffix atlas within **.008255 / .006261 / .010896 rad** component
RMS. This is same-decoder internal agreement, not independent physical truth.
No motion is reconstructed while the ball is obscured.

The standalone figures under `runs/vr3-motion-structure-deliverable/figures/`
show:

- `whole-trajectory.pdf/png`: all 5,163 original capture rows, including its
  visibly unreliable suffix, and all 5,157 Rust image decisions/recovered path;
- `prehand-derivatives.pdf/png`: every comparable clear pre-hand angular/body
  velocity and heading derivative, without smoothing or fitted correction;
- `full-recovery.pdf/png`: whole-recording fit quality and speed scales,
  showing the failed original suffix and Rust's resumed estimates separately.

Raw trajectories use east = column 16 and north = column 15. Rejected intervals
are visual gaps. Supported binary-map MSE is shown without altering values;
unscoreable sentinel values, if present, are marked separately and remain exact
in the numeric evidence. Figure hashes and input hashes are retained alongside
the figures.

## Existing one/four-camera compute measurements

These are the final queue-eight **short** native measurements on this i7-6700K:
2–4 measured seconds after one warmup second, not overnight results. Processing
stays near 100 FPS per camera. CPU 100% means one logical CPU; this machine has
eight logical CPUs. The total-capacity column divides by eight and does not
measure physical-core headroom, GPU load or power.

| Selection | One camera CPU, one-core basis | Four cameras CPU, one-core basis | One / four, eight-logical capacity |
|---|---:|---:|---:|
| Data only | 44.48–45.97% | 186.58–189.36% | 5.56–5.75% / 23.32–23.67% |
| Full-source MJPEG video | 69.20% | 315.84% | 8.65% / 39.48% |
| Uncompressed full source + diagnostic video | 88.85% | 491.16% | 11.11% / 61.40% |

The full-source MJPEG and uncompressed cases above complete their selected
videos. Low CPU after a compressed diagnostic stream stops is not evidence of
successful ongoing diagnostic encoding.

Differencing cumulative `Tracker::process` timing totals over each measurement
gives approximately **2.88 ms/frame** for one data-only camera and
**2.78–3.01 ms/frame** for four. With four uncompressed raw+debug streams,
numerical means are **3.28–3.64 ms/frame**. These are wall-clock engine timings,
including numerical diagnostic maps and scheduling effects, not CPU-cycle counts.
The post-acquisition worker EMA median is **2.76 ms** for one data-only camera,
**3.19 ms** for four, and **3.75 ms** for four raw+debug cameras. This is a
different statistic (polled EMA versus arithmetic tracker mean), so these
numbers cannot be subtracted as component costs. Camera acquisition/adapter
decode and pacing are excluded from that worker EMA; whole-process CPU includes
camera decoding, GUI, encoders and transports.

Each rig's numerical worker is chronological and can migrate between CPU cores.
The per-core relay pattern alone does not rank the design. The relevant checks
are actual processing rate, source-clock gaps, quality evidence, successful
recording and total CPU. Sustained overnight measurements must be reported
separately when they have actually run.

## Reproduce

```sh
python3 scripts/analyze_vr3_derivatives.py \
  --reference-dir /path/to/VR_array/VR3 \
  --candidate-dir runs/final-tracking-queue8/vr3 \
  --suffix-dir runs/final-tracking-queue8/vr3-control \
  --og-replay runs/vr3/og-replay-valid/og-replay-20261026_220515.dat \
  --output runs/vr3-motion-new --summary runs/vr3-motion-new-summary.json

PYTHONPATH=/tmp/hejjetrac-analysis MPLCONFIGDIR=/tmp/hejjetrac-matplotlib \
python3 scripts/plot_vr3_derivatives.py \
  --reference-dir /path/to/VR_array/VR3 \
  --candidate-dir runs/final-tracking-queue8/vr3 \
  --og-replay runs/vr3/og-replay-valid/og-replay-20261026_220515.dat \
  --metrics runs/vr3-motion-new/metrics.json \
  --output runs/vr3-motion-new/figures
```

The plotting dependencies are an optional development environment. Neither
analysis script is imported or launched by the application. The rolling prefix
statistics were checked against the independent direct-window computation;
maximum numerical difference was **9.92e-13**, with identical window counts.
