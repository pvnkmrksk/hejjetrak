# Performance reassessment — 2026-10-25

This is the earlier decode/pipeline assessment. For numerical per-frame latency
and the current local fitting algorithm, see [tracking-stage performance](tracking-performance.md).

Rust alone does not make a serial, copying pipeline fast. Release profiling
identified architectural and codec costs. The changes preserve mathematical
settings, coordinate conventions and quality gates:

- CLI, agent and desktop overlap decoding and fitting through one bounded
  slot. Frames remain ordered, with no offline dropping. Shutdown disconnects
  the queue before joining its producer. Decode failures never become EOF.
- Validated frames share immutable pixels. Clock edits no longer copy or
  revalidate full images. Byte inputs normalize once with checked bounds.
- Map fitting prepares its ROI once and reuses it for atlas/diagnostics.
  Unused pairwise pyramids and previous full images are eliminated. Atlas
  fitting walks dense valid sites with one rotation matrix per candidate;
  global-search lattice, mask order and optimizer budgets remain unchanged.
- The pinned MIT Rust decoder has lazy disabled diagnostics, cached trace
  flags, byte-fragment bit reads and tail-first RBSP stop-bit detection.
  There is no approximate decoding, removed deblocking, C/C++ backend or
  subprocess. See [patch provenance](../vendor/oxideav-h264/HEJJETRAK_PATCHES.md).

## Same-machine comparison

Apple M2 Pro, macOS 27.0.1 arm64, Rust 1.99.1, release thin LTO,
codegen-units=1, no target-cpu override. Input: `examples/sample.mp4`,
384×288, 300 frames at 30 fps, SHA256
`afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad`.
Both Rust binaries used unmodified `examples/benchmark/config.txt`, complete
quality/all-frame output, no recording or wire sinks. Outputs were on the
temporary filesystem. Cases ran serially in rotating order: four runs each,
first discarded, median of the remaining three.

| Measurement | Released v0.1.1 | Redesigned Rust |
|---|---:|---:|
| Reported end-to-end throughput | 208.55 fps | 397.47 fps |
| Reported wall time, 300 frames | 1.4385 s | 0.7548 s |
| Tracker processing throughput | 547.71 fps | 634.12 fps |
| Outer process wall time | 1.4436 s | 0.7602 s |
| Rejected frames | 0 | 0 |

All 300 all-frame numerical rows were byte-identical in every Rust pair.
Throughput increased **1.91×** and reported wall time fell **47.5%** on this
fixture. Tracker throughput times `Tracker::process`, including numerical
diagnostics, excluding decode/conversion/file output. CLI wall starts before
source open and includes decoding, fitting, media finalization, map/data/quality
output and syncs; it stops before summary writing and filename promotion.
Process wall additionally includes startup and complete shutdown.

After the final source freeze, the checked-in comparison script repeated four
alternating pairs: **211.50 → 411.05 fps**, with tracker medians
**553.32 → 651.13 fps** and identical all-frame bytes. All reference gates
passed on every run. These repeats are retained separately in the comparison
record rather than replacing the three-way C++ comparison above.

Original C++ commit `10fc4472af1fc9062b9bf5a92738e835b98cc3ec` was built and
run on this Mac with the same input/config: **594.32 fps reported active
throughput**, all 300 rows, successful exit. Outer process median was 1.2033 s,
including original 250 ms supervisor polling and 250 ms exit sleep. That padded
time must not be used to claim Rust wins active processing. Upstream uses
`-Ofast -march=native`; Rust does not enable fast math. Installed Homebrew
OpenCV 5/NLopt/Boost served only this separate benchmark executable.
[Compatibility patch](benchmarks/cpp-compatibility.patch) updates obsolete
Boost names, disambiguates an OpenCV enum collision and adds the recorder's
required virtual destructor; it changes no tracking algorithm.

**Full C++ throughput parity is not established.** Rust end-to-end is about
67% of C++'s reported active rate, with different scopes. Sampling still
concentrates on decoder reconstruction, deblocking, motion compensation and
YUV/RGB conversion. Fitting is no longer this clip's pipeline bottleneck.
Further improvements require the same pixel oracles and native measurements.

This short, low-resolution fixture does not establish 16-camera capacity,
higher-resolution performance, other-platform speed, recording throughput or
latency percentiles. GUI video playback still follows source time; offline
CLI throughput measures available processing capacity.

## Reproduce

Retain a release baseline before rebuilding. Run regressions, build once, then
run performance cases without concurrent builds or tests:

```sh
cargo build --workspace --release --locked
python3 scripts/benchmark.py --baseline /path/to/retained-v0.1.1-cli \
  --candidate target/release/fictrac --output /tmp/hejjetrak-benchmark-new
cargo run -p fictrac --release --locked -- demo --frames 120 --output /tmp/hejjetrak-known-motion-new
cargo run -p fictrac-io --example profile_video --release --locked -- examples/sample.mp4 rgb 10
cargo test --manifest-path vendor/oxideav-h264/Cargo.toml --lib --locked
```

The original script alternated order, discarded warmup, checked every frame,
ran unchanged reference gates and rejected changed all-frame bytes at checkpoint
`0d00cdb`. The current script makes byte identity optional with
`--require-identical`; algorithm changes use the unchanged reference gates.
The profiler driver times
synchronous decode/conversion alone and black-boxes actual pixel buffers.
Sampling runs are diagnostic evidence, never unsampled timing substitutes.
Raw measurements: [comparison record](benchmarks/foundation-20261025.json).
