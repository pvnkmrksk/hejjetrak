# Sustained resource use

The resource target is a source-paced camera session that leaves CPU time for
other work. A shorter numerical frame time does not justify busy waiting,
unbounded queues or extra worker pools. Each native session has one numerical
worker; sessions can run on different CPU cores. Capture and bounded decode/output
adapters have their own blocking responsibilities. No GPU tracking kernel or
nested numerical thread pool has been introduced: their energy benefit has not
been demonstrated for this workload. The GUI uses the native graphics backend.

## Work avoided

- Paused and ended workers park until notified, with a one-second safety timeout;
  commands explicitly wake them. Source pacing waits for the next deadline and
  retains an already acquired frame when a command interrupts that wait.
- Every numerical frame, quality decision and recording frame is retained.
  Preview images and cloned debug panels are published at most roughly 33 times
  per second, and immediately for stepping, pausing and end of input.
- The GUI uploads a texture when its source image changes. Parsed calibration
  geometry is cached by exact config text and source dimensions. Hidden guides
  are not constructed. Inactive viewports request fewer repaints.
- Bounded contact/pose history uses a deque rather than shifting its entire
  contents on each frame. Network transports remain bounded.

These changes reduce housekeeping without reducing the requested tracking rate.
A camera still acquires into its bounded latest-frame slot while tracking is
paused; this does not claim that the camera driver or pixel conversion is free.

## Measurement boundaries

`scripts/resource_probe.py` drives the real native sessions through JSONL. It
excludes startup and warmup, then samples the launched application's cumulative
CPU time, resident memory and OS thread count. CPU **100% means one logical core**;
several active threads can exceed 100%. Application CPU includes capture/decode,
tracking, rendering, file/wire output and its internal workers. The external
Python/OS sampler, WindowServer and driver/GPU power are outside that total.
RSS includes shared/mapped pages and graphics allocations as reported by the OS;
it is not a private-memory or energy measurement.

Cases must run serially without overlapping builds, tests or other benchmark
processes. A short video ending during a case fails the probe. A result also
records actual frame progression, requested rate, quality status, binary hash
and platform so idle/late playback cannot masquerade as a low-cost camera.

For repeatable scheduling comparisons, set `rust_resilient=n` in both builds;
otherwise the new default occlusion policy can change work on loop-boundary
pose jumps. Shipping-default recovery is checked separately. The H.264 comparison uses a developer-only stream-copy repetition of the
300-frame sample, including compressed decode work and artificial loop
boundaries. The separate uncompressed grayscale fixture follows adjacent source
frames forward and backward, avoiding a last-to-first pose jump, and tests the
shipping-default resilient policy. Neither fixture demonstrates physical camera
acquisition at 120 or 240 FPS. The built-in Mac camera is 30 FPS.

## Measured active workloads

Measured on **2026-10-25, Apple M2 Pro, 12 logical CPU cores, 32 GiB RAM,
macOS 27.0.1 arm64, Rust 1.99.1**. Release uses thin LTO and one codegen unit,
without a target-CPU native override. The before build is commit `51dca654`;
the measured after build is `cd249662`. The final v0.1.2 spot check followed
its tiny-window CLI-bound/documentation fixes, which did not alter that engine/
scheduler. These are historical v0.1.2 measurements. Separate final v0.1.3
measurements appear below; see [validation](validation.md) for completed local,
native-platform CI and downloaded-release checks.

Each table entry is a measured case after three seconds of warmup and at least
20 seconds of sampling, not a cross-machine or experiment-length guarantee.
**CPU 100% is one logical core**, so sixteen-session CPU values are the total
for the process, not the load of each camera. RSS is the median in MiB; it
includes shared/mapped/graphics pages and can increase even when CPU decreases.
All cases show native previews. Recording is disabled; wire outputs are disabled
except the explicitly labeled row.

| Workload | CPU before → after | RSS before → after, MiB | After delivered FPS per session |
|---|---:|---:|---:|
| 1 uncompressed grayscale session, 120 Hz | 45.7% → **37.1%** | 164.4 → 164.7 | 119.96 |
| 16 uncompressed grayscale sessions, 120 Hz | 276.2% → **266.4%** | 327.6 → 350.4 | 120.05 mean |
| 16 uncompressed sessions + UDP/ZMQ, 120 Hz | — → **291.6%** | — → 313.7 | 120.04 mean |
| 1 uncompressed grayscale session, 240 Hz | — → **52.9%** | — → 170.5 | 239.85 |
| Actual built-in camera, room scene, 30 Hz | 42.3% → **32.6%** | 253.9 → 182.5 | 29.94 |
| 1 H.264 session, 120 Hz | 53.9% → **52.8%** | 176.2 → 122.3 | 120.06 |
| 16 H.264 sessions, requested 120 Hz | 916.0% → **872.6%** | 456.0 → 422.4 | 120.16 mean |

The uncompressed sixteen-session result uses about **2.66 logical cores** in
aggregate; with live outputs it uses about **2.92**. These are process CPU-time
measurements on this Mac. The mono fixture excludes RGB capture/conversion and
USB/driver costs. It is a scheduling/tracking workload, not sixteen physical
cameras. H.264 input is much more CPU-intensive here; its before run delivered
117.93 FPS per session on average versus the after run's 120.16, so it did not
complete exactly the same amount of paced work. Wire measurements include
formatting/queue/send costs with **no receiver/subscriber** and establish no
independent packet-delivery count. The actual camera viewed a room, not a
calibrated sphere; its resource result is not live-ball accuracy evidence.

Quality in the resource sampler is **last-row state only**, not a full acceptance
log. A separate complete run processed all **7,176** uncompressed fixture frames:
1 initialization, 7,175 tracked, no relocalized/rejected frames. The clean
300-frame source still has byte-identical all-frame output to the before build
and passes the original reference gates (path RMS 0.02559384 sphere radians).
A separate five-repeat, predecoded tracking-only profile with resilient mode on
measures **0.495 ms** median run-mean, excluding initialization, I/O, capture,
pacing, display and recording. It is not an end-to-end resource percentage.

A final-source repeat (`f92efbb`, binary SHA-256 `8cdc3c9bd0e80003dc728cd33a15b366ae52123af764b2cd3336845a78c9f2a0`) sustains 120.05 FPS per session with both outputs, using **2.55 cores and 290.6 MiB median RSS**. This repeats the same workload; it is not an additional algorithmic improvement. The two output-enabled samples span 2.55–2.92 cores. No application publisher discards/failures were observed; receiver delivery remains unmeasured.

## Reading CPU percentages on a multi-core computer

The table above uses the common process convention: **100% equals one logical
core**. Dividing by this Mac's twelve logical cores gives the fraction of its
total nominal CPU capacity. This changes the percentage's denominator, not the
amount of work measured. It is CPU-time normalization; differing core speeds and
power states mean it is not an energy or equal-performance-core measurement.

| Measured workload on the 12-core M2 Pro | Core equivalents | Total CPU capacity |
|---|---:|---:|
| Actual room camera, 30 FPS | 0.326 | **2.72%** |
| One grayscale replay, 120 Hz | 0.371 | **3.09%** |
| One grayscale replay, 240 Hz | 0.529 | **4.41%** |
| Sixteen 120 Hz replays with UDP/ZMQ, recorded cases | 2.55–2.92 | **21–24%** |

This normalization table describes the v0.1.2-era cases. The separate v0.1.3
repeat below measures the updated acceptance and UI behavior directly.

For a **purely arithmetic illustration**, the same 0.371-core workload would be
9.28% of a four-core CPU, 4.64% of eight cores, 3.09% of twelve, or 1.16% of
thirty-two, **if per-core performance and every surrounding cost were identical**.
Those are estimates, not measurements on Intel, AMD or another Mac. Fewer cores
do not inherently increase this workload's per-frame compute; different single-
core speed, capture conversion, drivers and recording can change the actual cost.
A GPU model or total core count alone does not predict acquisition capacity.
GPU tracking and wattage/battery use remain unmeasured.

## v0.1.3 final repeat

Local formatting, strict Clippy, all 173 workspace tests and the release build
pass. Native lifecycle checks, all four release CI jobs and downloaded Mac
archive checks also pass; see [release validation](validation.md). The
measurements below use the final local binary, not a cross-platform performance
claim.

The same serial fixture on the same Mac uses 3.19% of total 12-core CPU capacity
for one 120 Hz replay (38.3% of one core), with 160.0 MiB median RSS. Sixteen visible
replays with UDP/ZMQ use 22.65% total CPU (2.72 cores), 291.3 MiB combined RSS,
and deliver 119.97–120.12 FPS each. Outputs use isolated loopback ports 21111–21126
and 19871–19886 to avoid disturbing existing sessions; default rig ports are
unchanged. No subscribers/receivers participate. All publishers remain alive,
with no application discards/failures. A paused case measures 0.26% total CPU,
but this single result does not resolve earlier idle variability.

The final healthy-sample tracking profile is 0.537 ms median run-mean; all numeric
bytes match v0.1.2. Recovery guards add work for accepted history, and rare
reacquisition now uses complete-ROI refinement. These resource samples do not
establish an energy saving or guarantee physical camera bandwidth. See the
[final binary hash, cases, profile and native lifetime evidence](benchmarks/resources-v013-20261025.json).

## Idle variability remains unresolved

The final paused one-session run measured **20.9% → 18.8%** of one core;
sixteen paused sessions measured **89.4% → 53.9%**. An earlier candidate pair
measured 1.95% and 33.3%, respectively. Later single-session diagnostic runs
measured about 1.75–1.83%, and sampling showed the numerical worker parked and
the main thread mostly waiting, with remaining sampled work including Metal
drawable acquisition. The high paused cost did not reproduce in those diagnostic
runs, but profiling adds overhead and is not a controlled replacement benchmark.
The cause of the variability is **unresolved**. These observations do not justify
a stable “2% idle” claim or a general “90% CPU reduction” claim.

## Reproduce the uncompressed workload

The development-only Rust fixture builder decodes the source once through
`next_tracking_frame(None)`, caches its exact 8-bit legacy grayscale packets and
writes a 120 Hz Y4M sequence `0..N-1,N-2..1`, repeated twelve times. The supplied
300-frame sample therefore yields 7,176 frames, approximately 794 MB on disk.
Cache plus packet metadata is bounded to 512 MiB and output to 2 GiB. It invokes
no subprocess decoder; this fixture is not RGB capture or new physical ground
truth.

```sh
cargo run -p fictrac-io --example resource_fixture --release --locked -- examples/sample.mp4 12 /tmp/hejjetrak-resource-pingpong.y4m
python3 scripts/resource_probe.py --binary target/release/fictrac-desktop --config examples/benchmark/config.txt --input /tmp/hejjetrak-resource-pingpong.y4m --mode video --sessions 16 --fps 120 --seconds 20 --warmup 3 --output /tmp/hejjetrak-resource-16
```

Use fresh output paths, finish all compilation first, then run measurement cases
serially. Add `--outputs` for the wire-cost case. A paired scheduling comparison
must use identical settings in both builds; the older H.264 comparison used
`--field rust_resilient=n`, whereas the raw ping-pong cases test the default
resilient policy. A 240 Hz override consumes this finite input twice as quickly,
so its measured duration must still finish before EOF. The sampler records binary
and input hashes, frame progression, requested/delivered rates, source and scope.

[Full 19-case record, source/fixture hashes, per-session frame counts and timing
distributions](benchmarks/resources-20261025.json) retains the raw summary evidence.

## Capacity and energy limits

Measured numerical latency alone cannot establish sixteen-camera capacity.
Resolution, pixel format, USB bandwidth, camera delivery, recording codec, output
backpressure and display size matter. Dropped capture frames and worker headroom
are exposed separately from tracking milliseconds. No hardware wattmeter,
battery-drain test, NVIDIA compute test, or modern Intel laptop measurement has
been performed here. Energy and sixteen high-rate camera claims require those
measurements on the intended rig.
