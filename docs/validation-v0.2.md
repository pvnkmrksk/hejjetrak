# Validation evidence — v0.2, 2026-10-24

Local platform: macOS 27.0.1, Apple Silicon (`aarch64-apple-darwin`), Rust 1.99.1. Release uses thin LTO and one codegen unit. The task toolchain is under temporary Rust/Cargo homes; no global shell/toolchain changes or sudo were needed. Historical first-pass evidence is in [validation-v0.1.md](validation-v0.1.md); its estimator, performance and parity statements do not describe v0.2.

## Checks actually completed

- Formatting and strict workspace Clippy pass.
- `cargo test --workspace --locked --offline`: **44 passed**, zero failures: 16 core, 13 I/O, 8 desktop, 2 camera-buffer, 5 CLI. The final run was outside the execution sandbox because its network isolation prevents binding even a UDP loopback test socket.
- `cargo build --workspace --release --locked --offline` passes locally.
- Windows desktop cross-target check and strict Clippy pass. This is type checking, not linking or native execution.
- Linux desktop cross-target check cannot complete on this Mac: `v4l2-sys-mit` requires `linux/videodev2.h`, absent from the macOS SDK. Native three-OS CI is configured but has not run remotely. Earlier CLI-only cross-checks are historical evidence, not proof of the new desktop/camera backend.
- `otool -L` on the desktop reports only system Apple frameworks/libraries; no third-party C/C++ media runtime. A transitive `block 0.1.6` future-compatibility notice remains in Cargo's report; application code has no Clippy warnings.

## Supplied upstream benchmark

Preserved verbatim:

| Fixture | User file |
|---|---|
| `examples/benchmark/config.txt` | `config_headless.txt` |
| `examples/benchmark/upstream.dat` | `sample_headless-20261024_180017.dat` |
| `examples/benchmark/upstream.log` | `fictrac-20261024_180017.log` |

The 300-frame reference exposed a real first-pass error: legacy FicTrac exports rotations in its recentered ROI basis, despite labeling them camera rotations. Correcting that basis and retaining the stored lab transform greatly reduced integrated drift. The map backend then further improved path/heading agreement over the retained pairwise alternative. The original fixture was not retuned or rewritten to force agreement.

Current release result, `runs/benchmark-final-v02/`:

| Metric | Measured | Regression limit |
|---|---:|---:|
| Frames | 300 (1 initialization, 299 tracked, 0 rejected) | 300 aligned |
| Rotation component RMSE X | 0.008760805 rad | 0.020 rad |
| Rotation component RMSE Y | 0.008292136 rad | 0.020 rad |
| Rotation component RMSE Z | 0.014210732 rad | 0.020 rad |
| Euclidean path-point RMSE | 0.044816058 sphere rad | 0.075 sphere rad |
| Wrapped heading RMSE | 0.013184811 rad | 0.025 rad |
| Final endpoint discrepancy | 0.054551157 sphere rad | 0.075 sphere rad in regression test |
| Maximum later video timestamp discrepancy | 3.46e-11 ms | 1 ms |

Final reference position: `[3.674266189723, -2.6214661833162]`; Rust: `[3.620026383681, -2.627286165588]`. Final heading: reference `6.1845202293513`, Rust `6.177704995564` rad. Errors exclude the initialization row; heading uses angular wrapping. The anomalous upstream first clock value and wall-clock columns are intentionally not treated as motion disagreement. Rows must be contiguous and aligned; different counts or later timestamp misalignment fail the comparison.

The pairwise alternative retains its existing separate gates and achieves path RMSE about 0.15588 sphere rad, heading RMSE 0.06167 rad on this fixture. Its tests were not loosened to accommodate the new backend. Lower integrated error does not mean every map-mode per-frame component is closer than pairwise.

`fictrac compare` emits the complete report and returns an error if its declared gates fail. The automated map regression tracks the whole sample; the desktop/headless test independently verifies that the same default map frames produce identical deltas, path and error in both frontends. Matching the reference is a sanity check, not physical ground truth. Rust PRIMA BOBYQA differs from upstream NLopt; neither numerical identity nor broad scientific equivalence is established.

## Release performance

Measured serially after compiler/test/native-window work and development dependency installation completed:

| Case | Real sample, map backend | Synthetic known motion, pairwise backend |
|---|---:|---:|
| Resolution | 384 × 288 | 160 × 160 |
| Frames | 300 | 120 |
| Accepted motion estimates | 299 | 119 |
| Rejected | 0 | 0 |
| Wall time | 1.59143 s | 0.20661 s |
| End-to-end fps | 188.51 | 580.80 |
| Tracking-call fps | 466.94 | 650.03 |
| Known-motion component RMSE | No physical ground truth | 0.00030405 rad |

Wall timing begins before input/config/first decode and includes processing and data/quality/SVG writes; it ends before summary JSON/final renames/reporting. Tracking timing includes `Tracker::process`, including numerical debug-map updates in map mode, but excludes video decode/disk output. These are one-run fixture measurements, not upstream speed comparisons or general hardware guarantees. Synthetic generation is before timed tracking. The generated sequence's delta is `[0.006, -0.009, 0.004]` rad/frame; its oracle passes the existing 0.003 rad/component gate.

Commands:

```sh
./target/release/fictrac track --config examples/benchmark/config.txt --input examples/sample.mp4 --output runs/benchmark-final-v02
./target/release/fictrac compare --reference examples/benchmark/upstream.dat --candidate runs/benchmark-final-v02/tracking.dat --report runs/benchmark-final-v02/comparison.json
./target/release/fictrac demo --output runs/demo-final-v02 --frames 120
```

Artifacts are ignored by Git. `comparison.png`/`.pdf` overlay the reference and candidate path/heading and plot point error; these development plots use Matplotlib, which is not an application dependency.

## Native GUI and camera evidence

The actual Metal-rendered application was launched outside the macOS execution sandbox, rendered, and exited successfully. Its screenshot was inspected: sample color video, saved rim/masks, orientation arrows/contact trace, thresholded ROI, warped difference, instant map, path and accumulated atlas are present in the expected layout. These are computed views, not reference placeholders.

A seven-second `--windows 4` run produced four native viewports. Each reached 200 video frames, with no application error, independent session ID, unchanged video source and `native_window: true`. Snapshot evidence: `runs/gui-validation/four-windows.json`; rendered root window: `runs/gui-validation/debug-screen.png`. Worker tests cover independent pause/step/reconfiguration, malformed-calibration preview, four sources, identical CLI/desktop numerical output and real UDP loopback rows with the `FT, ` prefix.

Configuration tests cover byte-exact legacy/benchmark/BR1–4 round trips, comments/unknown keys/CRLF, undo/redo, next-rig selection, camera-only increment and saved ROI precedence. GUI-level tests exercise parameter/text synchronization and graphical rim/mask commits. The native app was visually verified, but actual pointer gestures, drag/drop and file-dialog interaction remain unverified: Computer Use reports the Mac is locked and cannot be automatically unlocked.

The native camera enumeration returns `0: FaceTime HD Camera`. Capture probing reports that Camera permission is unavailable. The user was asked to unlock the Mac and allow the expected camera prompt; no unlock response has arrived. No claim of live-camera reliability, measured camera latency or real four-camera acquisition is made. A local unsigned app is available at `target/local/FicTrac.app`; no release or installer was published.

## Decoder and remaining limits

Input SHA-256: `afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad`. The bundled sample is byte-identical to the user's video and upstream sample. All 33,177,600 decoded luma bytes match the independent FFmpeg development oracle, presentation-ordered FNV-1a-64 `7e8d27f58358523f`, 300 frames from 0 to 9966.666666667 ms. The test never invokes FFmpeg. RGB conversion is implemented in Rust; raw luma checksum behavior remains unchanged.

The known-motion renderer shares camera geometry with the tracker, supported by separate analytic rim and off-axis tests; it does not replace a physical calibration. Only one real compressed sample and one upstream run are available. Global relocalisation, imported maps, serial output, encoded raw/debug recording and automatic bad-frame recovery remain missing. See [parity.md](parity.md) for the explicit boundary.
