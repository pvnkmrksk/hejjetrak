# Supplied raw.avi: 47-degree replay

The [new supplied-take validation](take-validation.md) supersedes the numerical
engine results below. The user subsequently repurposed `examples/raw47/config.txt`
for the live VR1 camera; that configuration is preserved. The original video
calibration is frozen separately in
[examples/raw47/supplied-video.txt](../examples/raw47/supplied-video.txt).

**Historical initial tuning experiment.** The follow-up
[masking/default validation](masking-validation.md) supersedes the numerical
profile below. The ready-to-run config now contains calibration and frontend
settings only. The final defaults measure 3,552 increments, with ten rejected
images and two zero-delta recoveries; the zero-rejection prototype was not kept
because it regressed existing VR3 checks. Initial results and their bound JSON are retained as evidence of
the earlier experiment, not results for the current config.

This focused change starts from fetched main `6c0bea6` on
`codex/raw-video-robustness`. The supplied private video remains outside the
repository at `/media/pkaushi/Pavan/raw.avi`; it is not distributed here.
Its SHA-256 is
`217c3f1d27cd5d050566d30921e177c5fee921e3b2fa8bddcf9ec825b15f7cc3`.
It contains 3,565 H.264 AVI images, 640×480 at 100 FPS, lasting 35.65 seconds.

## Calibration and use

[The original-video config](../examples/raw47/supplied-video.txt) uses the supplied **47°
vertical FOV**, a pinhole model and eight visually selected rim marks fitted
with the shared calibrated-ray solver. The fitted unit centre is
`[0.03600279982549754, 0.07631136607934809, 0.9964338281149576]`, and angular
radius is `0.30091335364466654` radians. Stationary and rotating-frame overlays
follow the visible rim within approximately 1–3 pixels. Inspected images show
no fixed obstruction inside the ball requiring an exclusion.

No aligned calibration square, verified laboratory direction or physical ground
truth is available. `c2a_r` is explicitly identity for camera/ROI comparisons;
anatomical/world axes and physical movement accuracy remain unverified. This
config is for the supplied framing, not an automatic replacement for rig files.

```sh
target/release/fictrac-desktop --config examples/raw47/supplied-video.txt
target/release/fictrac track --config examples/raw47/supplied-video.txt --output runs/raw47-new
```

The output directory must be new. Existing VR1–VR4 configurations and installed
applications were not edited. The dedicated config disables wire outputs and
automatic media recording.

## Changes and controls

Two separate mechanisms caused the failure. First, acceleration probation
froze all atlas learning even when a cold startup had learned only one side of
the ball. Confirmed motion could exhaust known surface before teaching resumed.
After four-image acceleration confirmation, current source can initialize only
genuinely unseen cells during probation. Every already observed cell remains
protected; rejected images cannot teach. Ordinary occlusion recovery retains
its full teaching deferral. [The exact policy](recovery.md) documents age/import
limits and the remaining ambiguity of plausible false views.

Second, this video's image appearance changes during rotation. Original
FicTrac's complete binary-map error rose smoothly to approximately 15,000 while
the Rust quiet-start history had tightened its ceiling to 10,000. The optional
`rust_resilient_error_floor` now allows a scene-specific lower bound from 10,000
through 18,000. The default remains 10,000, the upper ceiling stays 18,000, and
explicit `opt_max_err` remains a hard cap. Motion, texture and confirmation
checks remain active. Reported error units and numeric/output semantics do not
change.

The initial tuned profile used quality 12, 100 local evaluations, a 0.15-radian per-axis
prediction window, and an explicit floor/cap of 18,000. Threshold ratio/window
remain 1.25/0.25. A smaller optimizer window improves local precision; it alone
does not fix the image-error ceiling or atlas deferral.

| Complete replay | Measured relative rows | Rejected/recovering rows | Longest rejected run |
|---|---:|---:|---:|
| Main, default tracking settings with fitted rim | 1,356 | 2,202 | 1,291 images |
| Main, window 0.08 / 100 evaluations only | 2,384 | 1,176 | 811 images |
| Atlas fix alone, default settings | 1,356 | 2,202 | 1,291 images |
| Tuned quality 6, floor 18,000 | 3,559 | 4 | 4 images |
| Chosen quality 12 config, floor 18,000 | **3,560** | **3** | **3 images** |

Every row remains in all-frame numeric and quality files. The final run also
has one initializing and one relocalized row. Images 2993–2995 are unaccepted;
image 2996 relocalizes with zero relative rotation, and measured motion resumes
at 2997. The resulting 40 ms missing-motion interval is not reconstructed.
Maximum final reported MSE is 16,394.48, below the configured 18,000 cap.

## Same-config original FicTrac comparison

The installed original C++ FicTrac processed the same 3,565 images with a copy
of that initial tuned numerical profile. Its executable SHA-256 is
`3e2b3fb4a4b54eca4b9c4e0498226566d726dc46d9d5c7a2280a6e573d8bd5ae`.
It writes defaults back into its config, so the baseline ran in its own output
directory. Original configuration files were not exposed to that rewrite.

Comparison joins exact source-image IDs 0–3564 with no fitted lag, axis transform
or scale. Original timestamp anomalies are retained and listed explicitly;
derivatives use the shared 10 ms video clock. Original `.dat` has no independent
quality flag, and neither implementation is ground truth.

The original and full native numerical replay for this historical profile used
display-disabled copies. The current ready-to-run config enables preview and
uses the newer shared defaults, so it is no longer numerically identical to
this historical profile.

| Metric, including every candidate decision | Difference from same-config FicTrac |
|---|---:|
| Rotation component RMS, radians | 0.002957 / 0.001963 / 0.003353 |
| Worst overlapping 100-image component RMS, radians | 0.012444 / 0.003578 / 0.006782 |
| Measured angular-speed correlation | 0.995707 |
| Complete raw path-point RMS, sphere radians | 0.167234 |
| Complete raw endpoint discrepancy, sphere radians | 0.340548 |
| Wrapped heading RMS, radians | 0.022875 |

Measured component correlations are 0.996689 / 0.987331 / 0.849680. The weaker
third component, accumulated trajectory difference and missing interval remain
visible; high acceptance alone is not evidence of physical accuracy. No new
scientific parity claim or relaxed reference-test tolerance follows from this
unlabelled clip.

## Reproduction and evidence

Full local data, original logs, controls, source/binary hashes, native pixels and
plots are retained under `runs/raw-video-20261027/`. Development helpers
`scripts/analyze_raw_tracking.py` and `scripts/plot_raw_tracking.py` preserve
missing intervals and compare complete trajectories. Python, Matplotlib and
the original C++ baseline are development tools, not application dependencies.

Native platform: Ubuntu 22.04.5, Linux x86-64, Intel i7-6700K, eight logical CPUs,
Rust 1.99.1 / LLVM 23.1.1. Release uses thin LTO, one codegen unit and no
target-CPU override. Formatting, strict locked workspace/all-target Clippy,
all **291 workspace tests** and the release workspace build pass. The unchanged
full-video luma/RGB decoder oracles and full/partial-obstruction regressions
pass. Two new cold-start known-motion cases confirm image four and measure all
86 later increments under the existing 0.008 rad/component gate. Configuration
tests cover finite bounds, older JSON defaults and explicit hard caps.

The actual native desktop processes all 3,565 images with numeric-only
recording; all 25 numeric columns are byte-identical to the CLI, and shared
quality fields match to CLI output precision. Queued advancement and recording
completion were polled, the completion marker is clean and exit is zero. A
separate 50-image check of the final display-enabled config captures and visually
verifies the actual X11 normal/diagnostic views. The 47° rim follows the source
silhouette, with retained pattern boundaries and explicit gray unknown cells.

After builds, tests and native sessions finished, serial release checks pass:

| Fixture | Images | Accuracy evidence | CLI internal / external wall seconds |
|---|---:|---|---:|
| Known-motion demo | 120 | Component RMS 0.00030405 rad, unchanged 0.003 gate | 0.51471 / 0.71631 |
| Original sample | 300 | Path RMS 0.04480675 sphere rad; unchanged path/heading/component gates | 1.79461 / 1.79669 |
| Existing VR3 recording | 5,157 | All 4,678 clear pre-hand and 410 fast images accepted; all rolling component gates pass | 22.10706 / 22.10905 |

VR3's healthy component RMS remains 0.006677/0.006547/0.009413 rad, with the
earlier complete path/endpoint discrepancies still 2.485641/2.699863 sphere
radians. Recovery remains at image 4820. All 337 post-hand images are accepted;
recovered increments pass the fresh-suffix control and no gap motion is invented.
These retain the existing gates, not new physical accuracy claims.

Internal wall begins before input/config/decoding and includes tracking and
ordinary numeric/quality/map/SVG output; it ends before summary/rename
completion. External wall includes the whole CLI process. Demo generation is
outside its internal tracking wall but inside its external time. The separate
VR3 harness, including suffix generation/replay and analysis, takes 44.99636 s;
it is not the VR3 CLI time above. These single fixture/machine measurements do
not establish a general speedup. The loaded raw-video/native runs likewise do
not establish real-time 100-FPS decoding.

[The aggregate evidence record](benchmarks/raw47-20261027.json) binds source,
input, configuration, executables, commands, logs, accuracy and timing scopes.

This is local Linux execution. It does not add Mac/Windows runtime, arbitrary
video, physical ground-truth or throughput-parity evidence. Exploratory raw
replay timings overlapped build/test activity and are not speed claims.
