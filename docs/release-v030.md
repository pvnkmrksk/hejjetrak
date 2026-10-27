# HejjéTrac v0.3.0

HejjéTrac v0.3.0 brings the four-camera Linux work into the main release, with
more resilient fast-motion tracking and a simpler daily workflow.

- Review saved ball, exclusion and laboratory calibration layers, keeping or
  redrawing each layer without restarting the entire process.
- Set preview FPS independently of tracking and choose configured, MJPEG or
  uncompressed recording. Separate bounded encoder workers keep camera tracking
  responsive; live Stop and take rotation finalize old recordings in the background.
- Inspect explicit completion markers, stream counts and partial-recording
  warnings. Numeric and quality evidence remains independent of optional video.
- Preserve Linux capture timestamps, identify cameras already used by another
  rig, and use the tested Linux display path for the affected mixed-GPU setup.
- Bind ZeroMQ explicitly to a LAN interface, or use the four-rig helper with an
  existing UDP republisher. Native FT remains the default; legacy MATREX JSON
  and framing options remain available.
- Retain v0.2.3's nominal recording FPS and writable per-user default configuration.

## Downloads and first launch

Choose the ZIP for your Mac or Windows PC. Ubuntu 22.04 or newer can use the
Debian installer or portable Linux archive. Mac builds are ad hoc signed; Windows
builds are unsigned. [Installation and permissions](https://pvnkmrksk.github.io/hejjetrak/manual/install.html).

Save existing calibration files outside the application folder before updating.
The bundled VR1–VR4 files contain the supplied rig's calibration, camera IDs
0/2/4/6, 320×240 at 100 FPS, UDP ports 1111–1114 and direct ZeroMQ ports
9871–9874. The bridged launch helper disables direct ZeroMQ in temporary
calibrated copies so existing republishers can own those ports. Review those settings for
your equipment; calibration is specific to its optics and geometry.

## Validation and limits

The merged Mac ARM candidate passes 270 workspace tests, 1,353 decoder tests,
the unchanged synthetic and complete-sample accuracy gates, and five native
control rounds with 20 independently verified recording takes. Main, diagnostics
and calibration views were visually checked.


The user's native Ubuntu four-camera campaign and completed overnight run are
recorded in the [validation evidence](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/validation.md).
That historical run retained 8,998,214 aligned numeric/quality rows with exact
accepted wire data. It completed with warnings: four debug movies stayed
partial, and two capacity-protection intervals kept data without video.

Recording throughput depends on format, storage and camera bandwidth. Check
completion markers and media status before treating a video as complete.
[Recording behavior](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/recording-performance.md)
and [tracking limits](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/parity.md)
retain the scope of each measurement.

Created by [Pavan Kumar Kaushik](https://github.com/pvnkmrksk).
Based on and inspired by [FicTrac](https://github.com/rjdmoore/fictrac).
