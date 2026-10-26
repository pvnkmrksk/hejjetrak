# Third-party dependencies

Cargo.lock pins the complete dependency graph. These are Rust libraries, not external executables users must install.

| Direct dependency | Role | Declared license |
|---|---|---|
| glam | Geometry and quaternions | MIT OR Apache-2.0 |
| serde, serde_json, thiserror, clap | Data/errors/CLI | MIT OR Apache-2.0 |
| bobyqa 0.3.2 | Pure Rust PRIMA bounded optimization | MIT OR Apache-2.0 |
| oxideav-core 0.1.41, oxideav-h264 0.1.9, oxideav-mp4 0.0.10, oxideav-mpeg4video 0.1.8, oxideav-avi 0.0.10 | Pure Rust media decode/encode/containers | MIT |
| eframe/egui 0.33.3 | Native desktop GUI | MIT OR Apache-2.0 |
| wgpu 27 | Native GPU rendering | MIT OR Apache-2.0 |
| rfd 0.15 | Native file dialogs | MIT |
| image 0.25 | Rust image buffers/PNG and bounded camera JPEG decoding | MIT OR Apache-2.0 |
| Lucide icon artwork | Bundled professional toolbar icons | ISC; Feather-derived subset MIT |
| serial2 0.2 | Native serial configuration and bounded writes | BSD-2-Clause OR Apache-2.0 |
| zeromq 0.6, tokio 1 | Pure Rust ZMQ transport and bounded asynchronous publisher | MIT |
| rustix (test dependency) | Native pseudo-terminal serial test harness | MIT OR Apache-2.0 |
| nokhwa 0.10 | Windows/Linux native capture | Apache-2.0 |
| objc2 and framework crates, block2, dispatch2 | Rust bindings to macOS native APIs | MIT |

OxideAV supplies Rust codec/container implementations. No FFmpeg/OpenH264/libav/OpenCV/NLopt C/C++ implementation is linked or invoked. Nokhwa's optional native MJPEG decoder remains disabled. Camera MJPEG frames use the existing pure Rust `image`/`zune-jpeg` decoder with explicit dimension/allocation limits. OS libraries and graphics drivers are native system dependencies, accessed through Rust bindings; this is not a claim that the operating system itself is Rust. The CLI links libSystem on macOS; the GUI additionally links native Apple frameworks.

Dependency sources, authorship and full license texts remain in Cargo registry distributions. CI vendors the locked dependency graph, and the packager copies its available license/notice files into `DependencyLicenses/` with a manifest listing each package's included files. On macOS this directory is under `Contents/Resources/`. Windows x64 releases use [Rust's static CRT linkage](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes) to avoid a separately installed Visual C++ redistributable; native Windows system DLLs remain OS dependencies. Historical development archives in this workspace must not be confused with current GitHub releases.

Upstream FicTrac's CC BY-NC-SA terms remain in LICENSE and ATTRIBUTION.md. Permissive dependency licenses do not remove those terms.

Lucide artwork is pinned to commit `7faf87628a1ad6175975bc595344337ce14a7db4`.
Original SVGs, rendered PNGs and the full ISC/Feather MIT notices are retained
under `assets/lucide/`. Release packages include those notices. Icons are decoded
once and cached by the native GUI; no icon font, download or SVG renderer is
required at runtime. Source: https://github.com/lucide-icons/lucide.

`oxideav-h264` 0.1.9 is retained under `vendor/oxideav-h264` with its MIT
license and original tests. A documented local performance patch is selected
by Cargo's registry patch table; provenance and changed files are listed in
`vendor/oxideav-h264/HEJJETRAK_PATCHES.md`. Packaging collects local-path notices
alongside registry-vendored licenses. CI runs the decoder's library suite
separately because this third-party package is excluded from our workspace.

`nokhwa-bindings-linux` 0.1.4 is retained under
`vendor/nokhwa-bindings-linux` with the original copyright and Apache-2.0
license. Its small documented patch recognizes V4L2 `GREY` mono cameras and
bounds a stalled MMAP frame read. Provenance and the Linux regression command
are in `vendor/nokhwa-bindings-linux/HEJJETRAK_PATCHES.md`; package notices include
this local dependency as well. No native JPEG/media library is enabled.
