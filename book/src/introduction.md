# hejjetrak(1)

**Native spherical-motion tracking, with FicTrac-compatible configuration and measurements.**

hejjetrak measures the rotation of a patterned ball and integrates it into a fictive animal path. A compact native window, command-line program and local agent interfaces share one Rust numerical engine.

[Download the app](../#download) · [First session](quick-start.md) · [Calibration](calibration.md) · [Measurement reference](output.md)

![Native hejjetrak preview of the supplied sample](images/preview.png)

## Synopsis

```text
hejjetrak [--config FILE] [--windows N] [--grid COLSxROWS]
hejjetrak-cli track --config FILE --input VIDEO --output NEW_DIRECTORY
hejjetrak-cli agent
hejjetrak-cli mcp
```

Prebuilt archives contain the desktop application, command-line tool, sample, example configurations and license notices. No Rust installation is needed to run them. The application uses native OS capture and Rust video codecs; it does not launch FFmpeg or require OpenCV, a Python environment or a web browser to track.

## Read the evidence with the feature list

The supported recorded-video workflow preserves legacy configuration text, coordinate conventions and 25-column data. Its optimizer differs from upstream NLopt. Reference comparisons use declared rotation, path and heading tolerances, rather than requiring byte-identical trajectories.

Measured timing is scoped to its fixture, configuration and machine. A half-millisecond tracking call on one sample is not evidence that sixteen physical 240 Hz cameras can be acquired and recorded on arbitrary hardware. Capture bandwidth, image conversion, display, encoding and outputs have separate costs. See [compatibility](parity.md), [validation](validation.md) and [tracking timings](tracking-performance.md).

## How this manual is organized

Start with installation and a sample session. Calibrate the source before interpreting motion scientifically, then configure recording or live output. The reference chapters document exact files, commands, measurements, timing boundaries and remaining limitations. Technical evidence is drawn directly from the repository documentation so the website and source reports stay synchronized.

The chapter-based manual and separate landing page follow the organization of [Rekkefly](https://github.com/pvnkmrksk/rekkefly); no Rekkefly artwork, measurements or implementation code is reused. FicTrac behavior and the explicitly labeled upstream chapters are attributed in [Citation and license](citing.md).
