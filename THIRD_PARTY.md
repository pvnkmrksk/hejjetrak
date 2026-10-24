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
| image 0.25 | Rust image buffers/PNG | MIT OR Apache-2.0 |
| serial2 0.2 | Native serial configuration and bounded writes | BSD-2-Clause OR Apache-2.0 |
| rustix (test dependency) | Native pseudo-terminal serial test harness | MIT OR Apache-2.0 |
| nokhwa 0.10 | Windows/Linux native capture | Apache-2.0 |
| objc2 and framework crates, block2, dispatch2 | Rust bindings to macOS native APIs | MIT |

OxideAV supplies Rust codec/container implementations. No FFmpeg/OpenH264/libav/OpenCV/NLopt C/C++ implementation is linked or invoked. Nokhwa's decoding/MJPEG features are disabled. OS libraries and graphics drivers are native system dependencies, accessed through Rust bindings; this is not a claim that the operating system itself is Rust. The CLI links libSystem on macOS; the GUI additionally links native Apple frameworks.

Dependency sources, authorship and full license texts remain in Cargo registry distributions. CI vendors dependency sources alongside artifacts. The local development app includes 707 available license/notice files for 461 cached locked registry packages under `Contents/Resources/DependencyLicenses/`, with a package/license/source manifest. Future binary distributions must carry appropriate notices. The historical v0.1 zip in this workspace predates the GUI and must not be confused with the current app.

Upstream FicTrac's CC BY-NC-SA terms remain in LICENSE and ATTRIBUTION.md. Permissive dependency licenses do not remove those terms.
