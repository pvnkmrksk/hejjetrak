# hejjetrak

Native Rust spherical-motion tracking, rebuilt from [FicTrac](https://github.com/pvnkmrksk/fictrac). One numerical engine serves a compact desktop app, command line, JSONL agents and MCP stdio. Video decoding and recording are Rust; capture uses native OS APIs. No OpenCV, FFmpeg executable, webview, Python runtime or administrator access is needed to run it.

![Compact native ball preview](docs/images/preview.png)

Download [platform archives](https://github.com/pvnkmrksk/hejjetrak/releases): macOS Apple Silicon/Intel, Windows x64 and Linux x64. Extract the archive and launch **hejjetrak**; the Mac archive contains **hejjetrak.app**. The command-line executable is **hejjetrak-cli**. Cargo package/binary names remain `fictrac` and `fictrac-desktop` for existing scripts. macOS archives are ad hoc signed, not notarized; Windows executables are unsigned.

**Scientific boundary:** this is a rebuild with measured compatibility, not bit-identical or unrestricted parity. Rust BOBYQA/global search differs from upstream NLopt. The supplied 300-frame video is checked against the supplied original `.dat`, and synthetic known-motion sequences check rotation accuracy. Native Mac camera acquisition/recording is tested; tracking a physical ball on the live camera and Windows/Linux camera hardware remain unverified. See [parity](docs/parity.md), [validation](docs/validation.md) and [output semantics](docs/output.md).

## Desktop

The normal view is a uniformly scaled crop around the ball, measured contact trajectory and a small path inset. Images keep their original pixel aspect ratio in preview and diagnostics. The toolbar keeps **reset** and **record** available even in a 240×180 preview. Hover controls and the status strip for explanations.

- **Source** already lists cameras; click one to connect. Refresh rescans devices. Open or drop a video to play it at source speed. Ordinary startup tries the first enumerated camera; an opening/no-frame failure falls back to the bundled sample. An explicitly selected configuration is honored. Automatic fallback uses a temporary sample calibration and retains the instance's output ports; it never rewrites the rig file.
- **Reset** clears the pose/map/path while retaining source time and integrated displacement. Rejected intervals remain explicitly unmeasured. **Record** starts/stops immediately; its chevron selects raw video, processed diagnostics, both or data only. Every take also retains accepted `.dat`, all-frame numeric data and quality CSV. Configured `save_raw`/`save_debug` still start recording automatically. Default recordings go under your Movies/Videos `hejjetrak` folder; choose another folder in the inspector.
- **Settings** opens a separate native inspector, leaving the preview unobstructed. Groups cover source/camera, graphical calibration, geometry, tracking/quality, outputs and exact config text. The document preserves unknown fields, comments, spacing and line endings. Config fields, graphical edits and text share undo/redo. Save is explicit; external conflicts are preserved.
- **Calibration** pauses the source. Ball rim and exclusion tools add points only to the selected geometry; right-click deletes the nearest point, and Move points drags saved geometry. Fit the rim with three or more points; finish an exclusion with Enter. Lab square uses TL → TR → BR → BL in XY/YZ/XZ. Its corners stay visible; replacing a corner preserves its position in the legacy order. **F** performs a reflected-seed alternate pose refit; a distinct valid branch is not guaranteed and failure leaves the file intact. The shared forward-vector command remains available to existing agents.
- **Diagnostics** reveals the six traditional FicTrac panels. Calibration outlines/axes and the full image are separate view toggles, hidden by default. The status strip shows delivered FPS and processing milliseconds; its tooltip distinguishes fit time, the full worker pipeline, source budget, capture skips and output ports. Lost tracking is shown explicitly. Pauses are excluded from the running-drop count.
- Camera settings expose advertised native resolutions/rates with readback and supported exposure/gain controls. Unsupported controls explain why they are unavailable. The tested built-in Mac camera does not expose writable exposure controls through its public AVFoundation interface. [Camera details](docs/camera-controls.md).

`VR1.txt`–`VR4.txt` retain the original BR calibrations and camera IDs. They enable localhost UDP **1111–1114** and ZeroMQ PUB **9871–9874** simultaneously. New windows follow the next existing VR/BR config; after those, copies increment numeric camera ID, enabled ports and instance name. Video paths remain video paths. These rig calibrations must match your actual camera and resolution before their motion is interpreted scientifically.

To build locally, install [Rust](https://rustup.rs/) 1.95 or later, then:

```sh
cargo run --release --locked
cargo run --release --locked -- --config examples/upstream-config.txt
cargo run --release --locked -- --windows 16 --grid 4x4
cargo run --release --locked -- --config VR1.txt --window-size 480x360 --window-position=0,24
```

`--window-size` and `--window-position` use logical pixels. Without a size override, `--grid` derives cell sizes from the monitor; new windows otherwise place themselves beside existing ones. Sixteen native windows are supported; sustained sixteen-camera throughput depends on source, recording mode and hardware. Windows source builds require the usual Visual Studio linker, and Linux source builds need desktop/camera development headers. Windows x64 builds select Rust's static CRT linkage to avoid a separately installed Visual C++ redistributable. Prebuilt executables need no Rust installation.

See [the UI design and command mapping](docs/ui-design.md) and [agent control](docs/agent-control.md). The first public POC is v0.1.0; v0.1.1 contains the compact interface, camera controls, ZMQ and ZIP timestamp repair. Earlier v0.2/v0.3 validation names were internal prototype labels.

## CLI and the upstream benchmark

The [performance reassessment](docs/performance.md) measures the redesigned
pipeline against released v0.1.1 and a same-machine C++ reference. The sample
improves from 209 to 397 fps with identical Rust output; full C++ throughput
parity remains open.

```sh
cargo build --workspace --release --locked
cargo run -p fictrac --release --locked -- track --config examples/benchmark/config.txt --input examples/sample.mp4 --output runs/benchmark
cargo run -p fictrac --release --locked -- compare --reference examples/benchmark/upstream.dat --candidate runs/benchmark/tracking.dat --report runs/benchmark/comparison.json
```

`compare` requires matching frame counts and aligned video timestamps, then checks rotation-component, path and wrapped-heading errors. Default limits are 0.020 rad/component, 0.075 sphere rad path RMSE and 0.025 rad heading RMSE. Failure returns a nonzero exit status. The reference is another implementation's output, not measured physical ground truth. The benchmark files are preserved as supplied under `examples/benchmark/`.

Run the same system through an AI agent or other machine client:

```sh
./target/release/fictrac schema
./target/release/fictrac agent < examples/agents/sample-session.jsonl
./target/release/fictrac mcp
./target/release/fictrac-desktop --control-stdio
```

The JSON Schema describes every action. Commands cover config/history, all graphical calibration equivalents, sources, stepping/running/reset, multiple sessions, templates, snapshots, recording, export and shutdown. MCP uses stdin/stdout and needs no model key or network server. The native GUI bridge marks queued work explicitly; poll status for completion. [Protocol and examples](docs/agent-control.md).

Other CLI operations:

```sh
cargo run -p fictrac --release --locked -- demo --output runs/demo --frames 120
cargo run -p fictrac --release --locked -- inspect examples/sample.mp4 --decode
cargo run -p fictrac --release --locked -- track --config examples/sample.json --output runs/sample
```

`demo` generates known-motion video and checks its recovered rotations. `inspect --preview first.pgm` saves decoded luma. `calibrate --help` describes headless rim fitting. CLI output directories must be new; completed runs contain `tracking.dat`, `quality.csv`, `summary.json` and `path.svg`. See [the output contract](docs/output.md) before interpreting error scores, timestamps or rejected frames. Legacy `.txt` defaults to map tracking; the alternative photometric estimator is selected with `rust_engine : pairwise`. JSON is a strict CLI format; the desktop edits legacy FicTrac text.

## Structure and checks

```text
fictrac-core       geometry, threshold/map and pairwise estimators, SO(3), path, debug views
fictrac-io         lossless legacy configs, JSON, Rust codecs, .dat, comparison, SVG
fictrac-camera     native OS capture, bounded latest-frame handoff
fictrac-control    shared deterministic commands, JSONL, MCP and recording orchestration
fictrac            headless CLI and known-motion demo
fictrac-desktop    native egui/wgpu windows, graphical calibration, workers, recording
```

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The engine has no UI, filesystem, network or codec dependencies. [Architecture](docs/architecture.md) explains the boundaries, informed by PhotoCraft's engine-first approach. GitHub Actions has executed native Windows, Linux and both Mac architecture builds, tests and the synthetic CLI demo. Camera hardware and GUI visual checks remain scoped to the tested Mac. [Validation](docs/validation.md) distinguishes local results, cross-compilation and unverified hardware.

## Attribution

FicTrac was developed by R. J. D. Moore and collaborators. Research users should cite [Moore et al. (2014)](https://doi.org/10.1016/j.jneumeth.2014.01.010). This rebuild preserves upstream's **CC BY-NC-SA 3.0** terms. See [ATTRIBUTION.md](ATTRIBUTION.md), [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md).
