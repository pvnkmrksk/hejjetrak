# HejjéTrac v0.3.3

Fast ball tracking is more robust during acceleration, motion blur and irregular
camera intervals. The shared Rust engine adds bounded temporal refinement,
guarded partial-atlas learning and capture-interval-aware prediction. Rejected
and occluded frames remain visible in all-frame quality evidence; recovery does
not invent motion hidden during a gap.

The complete supplied 47° take tracks all 3413 clear fast-motion frames. The
follow-up live check measures 3053/3053 fast frames with no rejection, and a
controlled real-video omission test measures 4407/4407 increments within the
existing FicTrac comparison gates. These are fixture-specific results without
physical ground truth. Accurate source-specific rim/FOV calibration still
matters, and incomplete-atlas ambiguity remains a limitation.

ZeroMQ is enabled by default on `0.0.0.0:9871`, serving localhost and IPv4 LAN
clients. Existing explicit settings and per-rig ports remain authoritative.
Native FT remains the default format; legacy MATREX VR JSON modes and ZMTP 2
compatibility are retained. All 1800 tested localhost/LAN-address sample
payloads match byte for byte.

Linux adds bounded graphics startup retries, private persistent diagnostic logs
and `--safe-mode` for Mesa software rendering. The Debian app-menu entry also
offers **Software graphics mode**. For a window that opens but shows wallpaper
or corrupted contents, launch `./hejjetrac --safe-mode` from the portable folder.
This renders correctly on the tested Ubuntu machine; the reported friend-machine
issue has not been reproduced there. Run `./hejjetrac --diagnostics` for log
locations. Software rendering can reduce performance.

The restored threshold controls display effective shared defaults without
rewriting omitted configuration fields. The separately reported **Save and
close** confirmation issue remains outstanding.

Local validation: formatting, strict locked all-target Clippy and 314 workspace
tests pass. Native Ubuntu 22.04 windows and extracted Debian software rendering
are visually checked. Published platform build/runtime results are reported
separately by GitHub Actions; local execution does not establish every platform
or GPU. See [validation](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/validation.md)
for exact inputs, tolerances and remaining limitations.

Created by [Pavan Kumar Kaushik](https://github.com/pvnkmrksk).
Based on [FicTrac](https://github.com/rjdmoore/fictrac).
