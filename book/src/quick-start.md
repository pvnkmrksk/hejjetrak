# Your first session

[Website](https://pvnkmrksk.github.io/hejjetrak/) · [GitHub](https://github.com/pvnkmrksk/hejjetrak) · [Full parameter table](upstream-parameters.md)

## 1. Open the application

Extract and launch the [platform download](install.md). Normal startup enumerates cameras and tries the first available camera. If opening fails or no frame arrives, it falls back to the bundled sample. An explicitly selected config keeps its requested source authoritative.

To inspect the example directly, open or drag a video into the source view, or launch with the supplied sample configuration. Video playback is paced at its source rate; offline CLI processing can run without that pacing.

## 2. Choose the source

The source selector already lists discovered cameras. Select a device to open it; the refresh control rescans after plugging in hardware. Device identifiers are OS-specific and should not be assumed stable across unplugging or rebooting. Select the advertised resolution and rate you actually intend to use, then check the native readback.

The bundled video is `examples/sample.mp4`, a 384×288, 300-frame, 30 fps upstream fixture. Its calibration belongs to that video, not to your camera or ball.

## 3. Calibrate your ball

Choose **Calibrate again…** from the preview context menu or Settings to start [graphical calibration](calibration.md) immediately. Set the correct vertical field of view and lens model, mark three or more points around the ball rim, mask the animal/supports and set the lab axes using a square. The fitted ball outline appears as soon as the third valid point is placed. Save the config explicitly. Check that the fitted outline and axes match the source.

A plausible-looking path is not a calibration test. Incorrect field of view or lab orientation can produce a smooth but scientifically incorrect trajectory.

### Check the coordinate frames

![FicTrac camera, animal and world coordinate frames](images/upstream/coordinate-frames.png)

*Original FicTrac illustration by R. J. D. Moore and contributors, CC BY-NC-SA 3.0. [Full coordinate guide](upstream-coordinates.md) · [Source and unchanged-image checksums](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/images/upstream/README.md).*

Use the diagram when aligning the lab square and checking forward, sideways and turning motion. The [original parameter table](upstream-parameters.md) explains every FicTrac field; [current configuration](configuration.md) covers HejjéTrac additions. The [25-column reference](upstream-data.md) and [output chapter](output.md) explain the recorded values.

## 4. Watch quality and timing

The preview shows the ball and measured path. The status reports delivered frame rate and tracking milliseconds. Use the tooltip/details to distinguish tracking, worker processing, source budget and capture skips. A 120 Hz source gives 8.33 ms between frames; numerical work must fit inside a pipeline that also has conversion, output and display costs.

Rejected and `recovering` frames are **missing motion**, not stationary measurements. Default resilient tracking preserves the learned atlas while the ball is hidden, then needs a confirmed absolute-pose match before resuming. Inspect diagnostic maps and quality data if the path freezes, jumps or needs repeated resets. Do not lower quality requirements merely to hide rejected frames.

## 5. Record or stream

Use Record for data, raw video, diagnostic video or both. Pick a new destination, stop the take normally and keep its config and quality files. UDP and localhost ZeroMQ can run simultaneously; each VR instance needs distinct ports. Read [recording](recording.md) and [output semantics](output.md) before using measurements in a closed loop.

## 6. Reopen your configuration

Use the same source geometry, saved config and physical camera setup next time. Check the source, framing and ball outline after a lens, mounting or format change. Old config text is preserved without automatic normalization. The [configuration chapter](configuration.md) explains which geometry is angular and which is pixel-based.
