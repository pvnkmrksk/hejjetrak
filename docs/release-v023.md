# HejjéTrac v0.2.3

Desktop recordings now use the camera or video's reported frame rate when
`src_fps` is automatic. Previously, one slightly longer capture interval could
set the entire recording's playback speed: a 9 ms interval from a 120 FPS camera
produced a 111.111 FPS file. Explicit `src_fps` overrides retain priority, and
timestamp inference remains available when the source reports no rate.

The correction applies to raw and diagnostic video, including a take containing
only one frame. Numeric data and quality timestamps are unchanged. Recorded
video uses a constant frame rate; the numeric timestamps remain the reference
for capture jitter, skipped frames and experiment timing.

Opening the app normally creates a writable default configuration in your user
config folder. Save calibration there and the next launch reuses that file and
its selected source. Bundled and existing configs are preserved. If the folder
is unavailable, the app keeps a temporary preview and offers Save As. Explicit
`--config` files keep their usual save behavior.

This patch includes v0.2.2's Ubuntu 22.04 support, Linux camera-startup fixes,
fast-motion recovery correction and direct legacy MATREX VR output. The Mac
capture adapter, numerical thresholds, coordinate frames and native FT default
remain unchanged.

Real-frame recording tests cover jittered timestamps, nominal rates, explicit
overrides, unknown-rate fallbacks and a one-frame take. Both videos fully decode,
and their numeric and quality evidence stays byte-for-byte unchanged.

For a new camera, redraw the ball outline and check vertical field of view.
The supplied rig configs contain example calibration; an outline that includes
the stationary holder can prevent reliable tracking even when the preview is
clear.

[Downloads](https://pvnkmrksk.github.io/hejjetrak/?download=1) ·
[Calibration](https://pvnkmrksk.github.io/hejjetrak/manual/calibration.html) ·
[Recording and timing](https://pvnkmrksk.github.io/hejjetrak/manual/output.html)
