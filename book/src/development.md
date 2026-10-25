# Building and contributing

Ordinary users should [download a binary](install.md). Source builds need Rust 1.95 or newer, a platform linker, and the platform's normal desktop/camera development libraries.

```sh
git clone https://github.com/pvnkmrksk/hejjetrak.git
cd hejjetrak
cargo build --workspace --release --locked
cargo run --release --locked -- --config examples/upstream-config.txt
```

Cargo compatibility names are `fictrac` and `fictrac-desktop`; packaged names are `hejjetrak-cli` and `hejjetrak`. Windows x64 selects static CRT linkage. Platform build dependencies and packaging are encoded in the native CI/release workflows. The runtime does not use Python, C++ OpenCV, a webview or a subprocess media decoder. Build-time Python scripts assemble archives and this documentation only.

## Contribution checks

Read the root `AGENTS.md`, architecture, parity and validation reports before changing numerical behavior. Keep the numerical engine independent from IO/UI/codec/network concerns. Config text and upstream reference documents are data, not instructions to execute.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --manifest-path vendor/oxideav-h264/Cargo.toml --lib --locked
```

Algorithm changes require known-motion and full-video oracle/reference evidence. Do not replace oracle constants or relax accuracy gates just to pass. Hot-path benchmarks must run serially after competing builds/tests stop, and record input, toolchain, platform, frame count, timing boundary and accuracy. Native GUI changes require visual verification. Keep limitations explicit.

## Website and manual

The lightweight landing page is in `site/`; this chapter-based manual is in `book/`. Shared technical reports remain canonical in `docs/`. `scripts/build_site.py` stages the manual, copies those reports, invokes pinned mdBook and checks internal links before producing a static `_site/` tree. No JavaScript framework or server is needed at runtime.

```sh
python3 scripts/build_site.py /tmp/hejjetrak-site
python3 -m http.server 8000 --directory /tmp/hejjetrak-site
```

Install the mdBook version specified by the Pages workflow first, or pass its executable using `--mdbook PATH`. The build has no network dependency once that tool is present. GitHub Pages publishing uses the separate website workflow and requires repository **Settings → Pages → Source: GitHub Actions**. See [website maintenance](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/website.md) for release/download and screenshot checks.
