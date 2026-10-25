# HejjéTrac

**Native tracking for spherical treadmills.**

**HejjéTrac** (`hejjetrac`) brings FicTrac spherical-motion tracking to a native Rust application. It measures the rotation of a patterned ball and reconstructs a fictive animal path. A compact native app, command-line program and local agent interfaces share one Rust engine. Created by [Pavan Kumar Kaushik](https://github.com/pvnkmrksk).

[Download](../?download=1) · [Website](../) · [GitHub](https://github.com/pvnkmrksk/hejjetrak) · [First session](quick-start.md)

![HejjéTrac processing the supplied FicTrac sample](images/tracking-demo.gif)

## Get going

1. [Download and open](install.md) the application for your computer. Startup tries a camera, then the bundled sample if no camera can deliver frames.
2. [Choose your source](quick-start.md), resolution and frame rate.
3. [Calibrate](calibration.md) the ball outline, exclusions and lab axes. Save one config per physical rig.
4. [Record](recording.md) data and video, or configure live outputs for your experiment.

The app and CLI use native OS capture and Rust video codecs. Prebuilt packages include the sample, examples and notices; no compiler, Rust installation or separate media runtime is needed.

## FicTrac reference

The original documentation remains part of the manual: [setup and algorithm](upstream-setup.md), [coordinate frames](upstream-coordinates.md), [the complete parameter table](upstream-parameters.md) and [all 25 output fields](upstream-data.md). These chapters retain the source text and attribution. [Current configuration](configuration.md) and [output semantics](output.md) explain the Rust application’s additions and differences.

## Commands at a glance

```text
hejjetrac [--config FILE] [--windows N] [--grid COLSxROWS]
hejjetrac-cli track --config FILE --input VIDEO --output NEW_DIRECTORY
hejjetrac-cli agent
hejjetrac-cli mcp
```

This manual follows current source; release-specific notes identify differences from published packages. The reference chapters explain [coordinates and output](output.md), [compatibility](parity.md), [validation](validation.md) and [performance](tracking-performance.md). [Citation and license](citing.md) credits FicTrac and the other project references.
