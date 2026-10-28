# Troubleshooting

## The app will not open

Use the platform **binary archive**, extract it fully and follow [installation](install.md). Mac release bundles are ad hoc signed and not notarized; Windows binaries are unsigned. Use only a per-app trust exception after verifying the source. Do not disable global OS protections. An actual malware/damaged-file warning needs investigation, not an automatic override.

## Linux: no suitable graphics adapter

[Upgrade to v0.3.2 or newer](releases.md): X11/XWayland startup checks the window
surface and automatically uses compatible Vulkan when OpenGL is unavailable.

In v0.3.1, X11/XWayland startup forces OpenGL to avoid a driver problem observed
on a particular hybrid-GPU system. Some systems cannot present OpenGL to the
window surface even though Vulkan works. For that release, try:

```sh
WGPU_BACKEND=vulkan hejjetrac
# Portable archive, from its extracted folder:
WGPU_BACKEND=vulkan ./hejjetrac
```

v0.3.2 prefers a surface-compatible OpenGL adapter and automatically
falls back to compatible Vulkan, including software adapters when installed.
Explicit `WGPU_BACKEND` choices are respected. The terminal reports the actual
selected GPU/backend; a failure lists available adapters and their window-surface
compatibility. This concerns window rendering and does not change calibration,
camera settings or numerical tracking.

## Linux: the window closes or startup hangs

Current source writes a private diagnostic log under
`$XDG_STATE_HOME/hejjetrac/logs`, or `~/.local/state/hejjetrac/logs` when unset.
Run `hejjetrac --diagnostics` (portable: `./hejjetrac --diagnostics`) to find
recent logs and the graphics environment. Logs include the selected graphics
driver and exit status, without dumping the config or camera images.

Normal Linux launches have at most three startup attempts: automatic graphics,
Vulkan, then Mesa software graphics. Each attempt has a 20-second pre-window
timeout. Explicit `WGPU_BACKEND`/`WGPU_ADAPTER_NAME` choices are honored rather
than automatically overridden. Once the window reports ready, a later crash
exits with its diagnostics and never automatically restarts acquisition.
The native `--control-stdio` bridge is not supervised or replayed; it supports
explicit `--safe-mode` while preserving its JSONL protocol.

A window that opens but shows wallpaper, blank or corrupted contents can also
need software graphics. Startup readiness cannot diagnose incorrect displayed
pixels, so this case needs the explicit software-mode launch.

Try `hejjetrac --safe-mode` to force software graphics; the Debian desktop entry
also offers **Software graphics mode**. Mesa software drivers must be installed
(`libegl-mesa0` and `libgl1-mesa-dri` on Ubuntu, included in the new Debian
package dependencies). Software graphics can reduce performance. This fallback
addresses graphics startup failures, and cannot guarantee recovery from every
camera, kernel or hardware failure. If it still closes, retain the newest log.

## The camera is missing or cannot open

Refresh enumeration after connecting the device, close other applications using it and verify OS camera permission. Select a supported native format/rate and inspect readback. A vendor SDK-only device is not automatically supported by generic native capture. Built-in Mac camera evidence does not establish every industrial camera or Linux/Windows driver. See [camera controls](camera-controls.md).

## The image is visible, but the path is wrong

Verify the source-specific field of view, lens model, rim, exclusions and lab-square order. Saved angular rim and lab rotation take precedence over historical clicked points. Resize/crop changes can invalidate pixel masks or the physical projection. Check [calibration](calibration.md) and use known physical motion to verify the signs.

## The trajectory freezes or tracking is rejected

A frozen path across rejected frames denotes missing motion. Inspect threshold contrast, glare, blur, exclusions, error score and quality records. A hand obscuring the ball cannot produce a measured trajectory for that interval. The default `rust_resilient : y` policy protects the learned atlas on unreliable frames, suppresses destructive automatic resets and uses bounded, confirmed reacquisition after the view returns. Recovery restores absolute pose, but cannot reconstruct movement hidden during the obstruction. The policy requires two matching observations before resuming relative tracking and does not guarantee recovery in every scene. More permissive thresholds can accept a wrong fit; do not use them merely to make a path continuous.

## It drops frames despite a small “ms track”

Tracking excludes acquisition, image conversion, recording and display publication. Check source/capture skips, output queues, image format, USB bandwidth and encoding. Test raw/debug recording separately; two recorded streams add work. Reduce unnecessary preview/diagnostic work and validate the actual rig count. A 300-frame file benchmark is not an experiment-length camera stress test.

## CPU usage is high

Offline unpaced video intentionally consumes available compute to finish sooner; that differs from a paced camera waiting for arrivals. Use source-rate GUI playback or agent `realtime` pacing for monitoring. Examine whether diagnostic rendering, recording or many viewports are active. The tracking engine is CPU-based; adding a discrete GPU does not automatically accelerate the sequential fitter. Measure resource use with the actual source and outputs before choosing a rig count.

## Config changes seem to do nothing

Use the graphical fit operation after editing rim/square clicks; saved `roi_c`/`roi_r` and `c2a_r` have precedence. Text fields must parse and validate before Apply. Failed edits leave the previous config intact. Disk Save is explicit. A dirty/external conflict is preserved for review rather than silently overwritten.

## UDP or ZeroMQ receives no rows

Check the selected instance, enabled ports and accepted quality status. Rejected legacy rows are omitted by design. UDP sends to a destination; ZeroMQ **binds PUB** on this computer and subscribers **connect**. The default is enabled on `0.0.0.0:9871`, serving localhost and IPv4 LAN interfaces. The receiver connects to the tracker IP, not `0.0.0.0`. Check that a Python republisher is not already using the same port. Subscribe to `FT, ` for native FT, or the empty prefix / `{` for single legacy JSON, and allow subscription setup before starting the experiment. Late subscribers miss previous rows. `published` means a send attempt completed, not subscriber receipt. Inspect busy-port and queue/failure notices in status.

## A recording ended with `.partial` files

Keep the data and error report. Normal stop/close finalizes the take; interruption or disk failure may leave incomplete containers or mixed final/partial files. Do not rename an unfinalized file and treat it as complete. Use a new output directory for another take.

## Reporting an issue

Open an [issue](https://github.com/pvnkmrksk/hejjetrak/issues) with the release/source revision, OS/CPU, source format/rate, config stripped of private paths, exact error and a minimal non-sensitive sample if possible. Include all-frame quality evidence when tracking fails. Do not publicly upload animal/lab footage, camera images or private paths unintentionally.
