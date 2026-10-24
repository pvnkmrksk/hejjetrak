First public checkpoint of hejjetrak, a native Rust rebuild of FicTrac.

- Shared numerical engine, native GUI, CLI, JSONL agent and MCP interfaces.
- Pure Rust video decoding/encoding; no OpenCV, FFmpeg executable, webview or Python runtime.
- Lossless legacy configuration editing, calibration, diagnostics, raw/debug recording and UDP/serial output.
- Original 300-frame sample regression and known-motion synthetic checks; native Mac camera capture/record/reopen verified.

This is a POC, not a claim of complete scientific or platform parity. Local sample throughput has not matched the supplied C++ reference. Native Windows/Linux camera hardware is unverified. See docs/parity.md and docs/validation.md for exact limits.

Platform archives are built by GitHub Actions and attached as each build completes. macOS apps are ad hoc signed, not notarized; Windows binaries are unsigned. This initial checkpoint retains the original dense debug UI; the compact preview redesign follows it.

FicTrac attribution and CC BY-NC-SA 3.0 terms are preserved. The repository is a separate rebuild linked to pvnkmrksk/fictrac; GitHub returned the existing fork when asked to fork the owner's fork again.
