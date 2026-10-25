# hejjetrak

[![Download hejjetrak](docs/images/download.svg)](https://pvnkmrksk.github.io/hejjetrak/#download)

**[Quick start](https://pvnkmrksk.github.io/hejjetrak/manual/quick-start.html) · [Full manual](https://pvnkmrksk.github.io/hejjetrak/manual/) · [GitHub releases](https://github.com/pvnkmrksk/hejjetrak/releases/latest)**

Native Rust spherical-motion tracking, rebuilt from [FicTrac](https://github.com/pvnkmrksk/fictrac). One numerical engine serves a compact desktop app, command line, JSONL agents and MCP stdio. Video decoding and recording are Rust; capture uses native OS APIs. No OpenCV, FFmpeg executable, webview, Python runtime or administrator access is needed to run it.

![Compact native ball preview](docs/images/preview.png)

The **[Download page](https://pvnkmrksk.github.io/hejjetrak/#download)** selects a published archive for your OS and shows first-launch instructions. Direct [GitHub platform archives](https://github.com/pvnkmrksk/hejjetrak/releases/latest) are also available: macOS Apple Silicon/Intel, Windows x64 and Linux x64. Extract the archive and launch **hejjetrak**; the Mac archive contains **hejjetrak.app**. The command-line executable is **hejjetrak-cli**. Cargo package/binary names remain `fictrac` and `fictrac-desktop` for existing scripts. macOS archives are ad hoc signed, not notarized; Windows executables are unsigned. [Per-app opening instructions](https://pvnkmrksk.github.io/hejjetrak/manual/install.html) explain the usual OS prompts without disabling global security. **You do not need Rust, a compiler or a package manager to run the downloads.**

The online manual follows current **v0.1.2 source**. The download page resolves the latest **published** release dynamically; read that release's notes for its included features.

**Scientific boundary:** this is a rebuild with measured compatibility, not bit-identical or unrestricted parity. Rust BOBYQA/global search differs from upstream NLopt. The supplied 300-frame video is checked against the supplied original `.dat`, and synthetic known-motion sequences check rotation accuracy. Native Mac camera acquisition/recording is tested; tracking a physical ball on the live camera and Windows/Linux camera hardware remain unverified. See [parity](docs/parity.md), [validation](docs/validation.md) and [output semantics](docs/output.md).

## Desktop

The normal view is a uniformly scaled crop around the ball, measured contact trajectory and a small path inset. Images keep their original pixel aspect ratio in preview and diagnostics. The toolbar reduces to **reset** and **record** as the preview shrinks, then **reset** alone at a tiny 96×96 preview. Hover controls and the status strip for explanations.

- **Source** already lists cameras; click one to connect. Refresh rescans devices. Open or drop a video to play it at source speed. Ordinary startup tries the first enumerated camera; an opening/no-frame failure falls back to the bundled sample. An explicitly selected configuration is honored. Automatic fallback uses a temporary sample calibration and retains the instance's output ports; it never rewrites the rig file.
- **Occlusion recovery** protects the learned atlas, pose and path when a frame is unreliable. Default `rust_resilient : y` suppresses destructive automatic bad-frame resets and uses bounded/throttled, two-observation-confirmed reacquisition. Missing motion stays explicitly missing; recovery is not guaranteed for arbitrary scenes. `rust_resilient : n` restores legacy permissive/reset behavior.
- **Reset** clears the pose/map/path while retaining source time and integrated displacement. Rejected intervals remain explicitly unmeasured. **Record** starts/stops immediately; its chevron selects raw video, processed diagnostics, both or data only. Every take also retains accepted `.dat`, all-frame numeric data and quality CSV. Configured `save_raw`/`save_debug` still start recording automatically. Default recordings go under your Movies/Videos `hejjetrak` folder; choose another folder in the inspector.
- **Settings** opens a separate native inspector, leaving the preview unobstructed. Groups cover source/camera, graphical calibration, geometry, tracking/quality, outputs and exact config text. The document preserves unknown fields, comments, spacing and line endings. Config fields, graphical edits and text share undo/redo. Save is explicit; external conflicts are preserved.
- **Calibration** pauses the source and proceeds one step at a time: **Ball → Exclusions → Lab**. Place new points or edit saved marks; only the active step is editable. A cursor-following **4× view magnifier** helps place marks precisely. Ball rim and exclusion tools add points only to the selected geometry; right-click deletes the nearest point, and Edit saved marks drags saved geometry. Fit the rim with three or more points; finish an exclusion with Enter. Lab square uses TL → TR → BR → BL in XY/YZ/XZ. Its corners stay visible; replacing a corner preserves its position in the legacy order. **F** performs a reflected-seed alternate pose refit; a distinct valid branch is not guaranteed and failure leaves the file intact. The shared forward-vector command remains available to existing agents. An explicit resolution reference can retain normalized copies of marks and adapt legacy pixel fields to a proportional source resize with matching aspect ratio, field of view and projection; old files are never silently reinterpreted. [Calibration guide](https://pvnkmrksk.github.io/hejjetrak/manual/calibration.html).
- **Diagnostics** reveals the six traditional FicTrac panels. Calibration outlines/axes and the full image are separate view toggles, hidden by default. The status strip shows delivered FPS and mean tracking milliseconds; its tooltip separates engine time from the full worker pipeline, source budget, capture skips and output ports. Lost tracking is shown explicitly. Pauses are excluded from the running-drop count.
- Camera settings expose advertised native resolutions/rates with readback and supported exposure/gain controls. Unsupported controls explain why they are unavailable. The tested built-in Mac camera does not expose writable exposure controls through its public AVFoundation interface. [Camera details](docs/camera-controls.md).

`VR1.txt`–`VR4.txt` retain the original BR calibrations and camera IDs. They enable localhost UDP **1111–1114** and ZeroMQ PUB **9871–9874** simultaneously. New windows follow the next existing VR/BR config; after those, copies increment numeric camera ID, enabled ports and instance name. Video paths remain video paths. These rig calibrations must match your actual camera and resolution before their motion is interpreted scientifically.

For a tiled rig, use the downloaded application:

```sh
hejjetrak --windows 16 --grid 4x4
hejjetrak --config VR1.txt --window-size 480x360 --window-position=0,24
```

<details>
<summary>Building from source (developers)</summary>

Install [Rust](https://rustup.rs/) 1.95 or later, then:

```sh
cargo run --release --locked
cargo run --release --locked -- --config examples/upstream-config.txt
```

See the [build/contributor manual](https://pvnkmrksk.github.io/hejjetrak/manual/development.html). Ordinary users can use the downloads above.

</details>

`--window-size` and `--window-position` use logical pixels. Without a size override, `--grid` derives cell sizes from the monitor; new windows otherwise place themselves beside existing ones. Sixteen native windows are supported; sustained sixteen-camera throughput depends on source, recording mode and hardware. Windows source builds require the usual Visual Studio linker, and Linux source builds need desktop/camera development headers. Windows x64 builds select Rust's static CRT linkage to avoid a separately installed Visual C++ redistributable. Prebuilt executables need no Rust installation.

See the [step-by-step manual](https://pvnkmrksk.github.io/hejjetrak/manual/), [UI design and command mapping](docs/ui-design.md) and [agent control](docs/agent-control.md). The first public POC is v0.1.0; v0.1.1 introduced the compact interface, camera controls, ZMQ and ZIP timestamp repair. v0.1.2 source adds lighter scheduling/publication, guided calibration with a cursor magnifier, opt-in resolution-aware marks and protected atlas recovery. Earlier v0.2/v0.3 validation names were internal prototype labels.

A measured **16-session, 120 Hz uncompressed grayscale replay** used about **2.66 logical CPU cores and 350 MiB RSS** in aggregate on a 12-core M2 Pro; enabling UDP/ZMQ used about 2.92 cores. This is a native video workload, not sixteen physical cameras or an energy measurement. See [resource use and scheduling](docs/resources.md) for before/after cases and unresolved idle variability, and [occlusion recovery](docs/recovery.md) for atlas protection and missing-motion semantics.

## CLI and the upstream benchmark

[Tracking-stage latency](docs/tracking-performance.md) measures the numerical
engine separately: **1.639 → 0.503 ms/frame** on the predecoded sample, and
**3.045 → 1.398 ms** in sampled Mac camera timings. Sample trajectory RMS
against the supplied FicTrac output improves from 0.0448 to 0.0256 sphere radians;
all original gates remain unchanged. These are fixture- and machine-specific
measurements, with complete scoring retained for quality decisions.

The [performance reassessment](docs/performance.md) measures the redesigned
pipeline against released v0.1.1 and a same-machine C++ reference. The sample
improves from 209 to 397 fps with identical Rust output; full C++ throughput
parity remains open.

```sh
hejjetrak-cli track --config examples/benchmark/config.txt --input examples/sample.mp4 --output runs/benchmark
hejjetrak-cli compare --reference examples/benchmark/upstream.dat --candidate runs/benchmark/tracking.dat --report runs/benchmark/comparison.json
```

These commands use the packaged executables; invoke them from their extracted location or add that location to PATH. On Mac the CLI is inside `hejjetrak.app/Contents/MacOS` and examples are in `Contents/Resources`; see [platform installation](https://pvnkmrksk.github.io/hejjetrak/manual/install.html). Source builds use Cargo's `fictrac`/`fictrac-desktop` compatibility names.

`compare` requires matching frame counts and aligned video timestamps, then checks rotation-component, path and wrapped-heading errors. Default limits are 0.020 rad/component, 0.075 sphere rad path RMSE and 0.025 rad heading RMSE. Failure returns a nonzero exit status. The reference is another implementation's output, not measured physical ground truth. The benchmark files are preserved as supplied under `examples/benchmark/`.

Run the same system through an AI agent or other machine client:

```sh
hejjetrak-cli schema
hejjetrak-cli agent < examples/agents/sample-session.jsonl
hejjetrak-cli mcp
hejjetrak --control-stdio
```

The JSON Schema describes every action. Commands cover config/history, all graphical calibration equivalents, sources, stepping/running/reset, multiple sessions, templates, snapshots, recording, export and shutdown. MCP uses stdin/stdout and needs no model key or network server. The native GUI bridge marks queued work explicitly; poll status for completion. [Protocol and examples](docs/agent-control.md).

Other CLI operations:

```sh
hejjetrak-cli demo --output runs/demo --frames 120
hejjetrak-cli inspect examples/sample.mp4 --decode
hejjetrak-cli track --config examples/sample.json --output runs/sample
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
