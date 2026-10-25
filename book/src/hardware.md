# Experimental setup

The following setup guidance is adapted from the upstream [FicTrac README](https://github.com/pvnkmrksk/fictrac#hardware-requirements) and [detailed requirements](https://github.com/pvnkmrksk/fictrac/blob/10fc4472af1fc9062b9bf5a92738e835b98cc3ec/doc/requirements.md), by R. J. D. Moore and collaborators, under CC BY-NC-SA 3.0. The application-specific limits are documented separately below.

## Ball and visible pattern

Use a lightweight sphere with a matte, high-contrast, non-repeating pattern. Mix small and larger features so the thresholded image remains distinctive across poses. Repeated stripes, glare and large untextured regions make rotations ambiguous. Keep a substantial portion of the ball visible; mask the tether, animal and supports rather than asking the optimizer to track them.

## Camera, optics and lighting

Choose the lens, working distance and frame rate together. Enough sharp pattern must remain visible during the fastest expected motion. Faster exposure reduces blur but needs more light. Diffuse lighting reduces specular highlights; check the pattern under the actual stimulus lighting, not only during setup. Correct vertical field of view and the lens model matter more than a very high pixel count.

A source delivering 100, 120 or 240 Hz has a frame interval of 10, 8.33 or 4.17 ms respectively. This is a total pipeline budget, not a promise based on a tracker-only timing. A high-resolution camera can overwhelm USB bandwidth or conversion even when the numerical ROI is small. Start with the lowest native resolution that preserves the required pattern detail.

## Computer and several cameras

Each instance needs capture, conversion, a sequential numerical state machine and optionally video encoding. A GPU renders native previews, but the tracking engine currently runs on CPU. More cores help independent instances; they do not remove camera bus, memory, recording or driver limits. Place cameras across suitable host controllers when bus bandwidth matters, and test the real camera formats together.

No universal minimum CPU specification or sixteen-camera capacity has been established. Repository benchmarks identify the tested Mac, fixture and boundaries. Validate your intended rig count, source rate, recording mode and dropped-frame indicators for an entire experiment-length run before using it for data collection.

## Platform coverage

Built-in Mac camera acquisition and controls have native evidence; its timing scene was a room, so it is not a live-ball accuracy validation. Windows/Linux native CI executes tests and synthetic tracking; that does not verify their GUI appearance or physical camera drivers. Industrial SDK-only cameras and vendor-specific controls remain outside current coverage. See [camera controls](camera-controls.md), [parity](parity.md) and [validation](validation.md).
