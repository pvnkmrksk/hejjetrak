# Documentation artwork

`preview.png`, `calibration.png`, `tracking-demo.mp4` and `tracking-demo.gif`
show the actual native HejjéTrac app processing `examples/sample.mp4` during
the 2026-10-25 polish pass. The sample belongs to the attributed FicTrac
materials; see [ATTRIBUTION.md](../../ATTRIBUTION.md). No live camera images
or simulated UI are included.

The demo is a 6.3-second excerpt of source-paced playback. Capture timestamps
retain its wall-clock timing. The OS title bar was cropped and the remaining
window scaled to 960×720 for MP4 and 640×480 for GIF. The MP4 is silent H.264 at
15 frames/s; the GIF uses 8 frames/s and a reduced palette. These display files
are illustrations, not numerical or decoder-oracle inputs. The website uses
`autoplay loop muted playsinline` to play the silent video automatically when
the browser permits, with native playback controls and a still poster as a
fallback. The GIF loops in GitHub Markdown where the viewer permits animation.
FFmpeg was used only as an offline documentation encoder; the application
continues to use Rust codecs.

The current application icon is editable vector artwork in
`assets/hejjetrak.svg`. `vfov-measurement.svg` is an original project diagram
for a rectilinear lens. Both follow the project license.

Unchanged upstream coordinate-frame and lab-axis images have their own
[provenance, license and checksums](upstream/README.md). Older screenshots in
this directory belong to the historical validation records that reference them.
