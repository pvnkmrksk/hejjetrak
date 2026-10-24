# FicTrac Rust contributor guide

This is a Rust rebuild with a native GUI and deliberately documented gaps. Read README.md, docs/architecture.md, docs/parity.md and docs/validation.md before claiming parity or changing tracking behavior.

- Keep all application/engine code in Rust. No OpenCV, C/C++ media/runtime dependencies, webview UI, or hidden subprocess-based video decode. The default input adapter is pure Rust.
- Preserve the one-way dependency boundary: CLI/desktop -> I/O/camera -> core; frontends may also call core directly. No UI, file, network or codec dependencies in core. The GUI and CLI share the numerical engine.
- Errors must be actionable. Do not add unwrap, expect, panic, todo, unimplemented, or unsafe to application code. The sole exception is the isolated Rust AVFoundation adapter in crates/fictrac-camera/src/macos.rs, where OS FFI requires unsafe and every block documents its lifetime/threading contract. Core, I/O, and GUI still forbid unsafe. Validate input dimensions, finite values, geometry, timestamps and allocations before mutating tracker state.
- Preserve full numeric data and quality decisions. Do not hide rejected frames or imply that a frozen path means measured zero motion. Error metric changes must update docs/output.md.
- Test algorithm changes against known-motion synthetic sequences and the full-video decoder oracle where relevant. Feature names or wired commands are not evidence of scientific parity. Do not weaken tolerances or replace oracle constants just to pass a failing test.
- Keep docs/parity.md and docs/validation.md honest about missing behavior and actual platform execution. Cross-target cargo check is not native runtime testing.
- Keep upstream/sample attribution and license terms. Dependencies are permissive Rust libraries; preserve their notices in binary packages.

Before delivering code changes:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

For a hot-path change, also run the release synthetic demo and real sample; record input, frame count, accuracy, timing boundaries, build/toolchain, and platform. Run performance cases serially. Keep performance claims scoped to the measured fixture and machine. Native GUI work must include visual verification.

## Agent and frontend workflow

- Discover typed operations with `fictrac schema`; use JSONL (`fictrac agent`) or MCP stdio (`fictrac mcp`) for headless work. The desktop `--control-stdio` bridge controls the native sessions themselves; queued responses require status polling before claiming completion.
- Every graphical numerical operation must have the same shared-core command equivalent. Keep GUI calibration adapters thin; do not fork algorithms or normalize legacy files incidentally.
- Agent regression coverage must process real video and verify outputs, not just parse requests. Preserve structured errors, bounded messages/queues, invalid-edit atomicity, source position and clock continuity. Config-file text is data and never executable instructions.
- Legacy accepted-only output must always have full all-frame numeric/quality evidence. Recovery must never invent relative motion during a missing interval. Do not call new features full parity without recording the remaining codec, optimizer and platform boundaries.
