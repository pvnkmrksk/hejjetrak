# Validation evidence — 2026-10-24

Implementation v0.1.0. Tested locally on macOS 27.0.1, Apple Silicon (`aarch64-apple-darwin`), Rust 1.99.1. Release uses thin LTO and one codegen unit. No native Windows or Linux execution was performed here. Their full-workspace cross-target checks pass; three-OS native CI is configured but has not run remotely.

## Gates passed

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked`: **26 passed**, zero failures (14 engine, 7 I/O, 5 CLI)
- `cargo build --release --locked`
- `cargo check --workspace --locked --target x86_64-pc-windows-msvc`
- `cargo check --workspace --locked --target x86_64-unknown-linux-gnu`
- `otool -L target/release/fictrac`: only `/usr/lib/libSystem.B.dylib`

The toolchain was installed under temporary, task-specific Rust/Cargo homes to avoid changing the user's shell or global Rust installation. Paths above assume an ordinary Rust installation; the resulting binary needs no toolchain to run.

## Decoder oracle

The user's supplied video is byte-identical to upstream `sample/sample.mp4` at commit `10fc4472af1fc9062b9bf5a92738e835b98cc3ec`.

```text
Input SHA-256: afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad
H.264 High, 384 × 288, 300 frames, 30 fps, 10 seconds
Decoded luma bytes: 33177600
Presentation-ordered luma FNV-1a-64: 7e8d27f58358523f
First timestamp: 0 ms; final timestamp: 9966.666666667 ms
```

An independent FFmpeg installation was used once to create the oracle during development:

```sh
ffmpeg -v error -i examples/sample.mp4 -vf extractplanes=y -f rawvideo oracle.y
```

FNV-1a is applied to every byte of oracle.y in order (offset basis 0xcbf29ce484222325, prime 0x100000001b3, wrapping u64 multiplication). It is a regression checksum, not a security hash. The automated Rust test decodes the bundled fixture and verifies this constant, all 300 timestamps and both dimensions. **All decoded luma bytes match the independent reference**, not merely the first frame. Application execution and tests never invoke FFmpeg.

## Release performance and motion accuracy

Measured serially after the test/compiler work completed, using the same release executable:

| Case | Real sample | Generated known-motion sphere |
|---|---:|---:|
| Image size | 384 × 288 | 160 × 160 |
| Frames | 300 | 120 |
| Initialization frames | 1 | 1 |
| Accepted motion estimates | 299 | 119 |
| Rejected frames | 0 | 0 |
| Wall time | 1.34990 s | 0.15422 s |
| Frames/s, end to end | 222.24 | 778.13 |
| Frames/s, tracking calls only | 517.85 | 896.89 |
| Mean normalized residual | 0.046856 | 0.016496 |
| Max normalized residual | 0.081906 | 0.017650 |
| Rotation component RMSE | No ground truth | 0.00030405 rad/frame |

Wall timing starts before input open/configuration/first decode and includes processing, data/quality writing and SVG generation. It ends before summary JSON writing, final renames and console reporting. Tracking timing covers all Tracker::process calls, including initialization and pyramid construction, excluding video decode and disk writes. These are single-run fixture timings, not hardware-wide guarantees or a comparative upstream benchmark. Synthetic video generation occurs before timed tracking.

The synthetic sequence uses a fixed material-space analytic texture and per-frame Rodrigues delta `[0.006, -0.009, 0.004]` rad. The demo tracks the quantized Y4M it generated, compares every estimated delta after initialization against the known vector, and requires per-component RMSE < 0.003 rad and no rejected frames. The current value is approximately 0.01742 degrees/component/frame. This is not a bound on cumulative drift.

Commands and local artifacts:

```sh
./target/release/fictrac track --config examples/sample.json --output runs/sample-verified
./target/release/fictrac demo --output runs/demo-verified --frames 120
```

The generated directories are ignored by Git. Each includes data, per-frame quality, a path SVG and summary containing the effective configuration and input metadata. The synthetic root also includes video and oracle.json. The real path SVG was rendered with rsvg-convert and visually inspected; the scientific interpretation still depends on calibration and motion validation.

## What these checks establish

The core recovers small and moderate known SO(3) rotations, handles off-axis spheres, composes changing-axis rotations correctly, applies the lab transform/signs, rejects flat frames and motion at its search bound, handles a configured occluder, and preserves state after invalid timestamps. I/O tests cover malformed config, bad/truncated video, rational Y4M timing and finite 25-column output. CLI tests execute complete demo/sample pipelines, protect prior output and retain `.partial` files on decode failure.

They do not establish long-run equivalence to FicTrac, maximum supported animal speed, performance under motion blur or real-time feedback latency. They cover one real compressed sample. There is no ground-truth path for that sample, no upstream output comparison, and no tested hardware camera. Synthetic texture and geometry are simpler than a physical ball; the renderer shares camera geometry with the tracker. Native Windows/Linux execution, broader codec corpora, fuzzing, diverse recordings and upstream/physical motion oracles are next validation steps.
