# Several rigs

Each native window owns an independent source, calibration, tracker, output state and recording. The bundled `VR1.txt`–`VR4.txt` derive from the original BR rig configurations.

| Instance | Saved camera ID | UDP destination port | Local ZeroMQ PUB bind |
|---|---:|---:|---:|
| VR1 | 0 | 1111 | 9871 |
| VR2 | 2 | 1112 | 9872 |
| VR3 | 4 | 1113 | 9873 |
| VR4 | 6 | 1114 | 9874 |

These are examples for their original physical setup, not calibration for arbitrary cameras. Check every device and source geometry before using its data.

## Creating windows

A new instance selects the next existing numbered VR/BR config. After those are exhausted, a copy increments its numeric camera ID, enabled output ports and instance name. A video path remains a video path. Disabled outputs remain disabled. Unsaved copies have no inherited save path, preventing accidental overwrite of the parent config.

## A compact tiled grid

```sh
./hejjetrak --windows 16 --grid 4x4
./hejjetrak --config VR1.txt --window-size 360x260 --window-position=0,24
```

Size and position use logical pixels. Without a size override, grid placement derives cells from the monitor. New windows otherwise place themselves beside existing ones. The supported native session count is bounded at sixteen.

## Budget the actual workload

A sixteen-window sample test verifies independent native windows and numerical output. It is not a sustained sixteen-camera acquisition/recording guarantee. Source USB links, capture drivers, conversion, display, encoders and disk throughput all add work. Recording and diagnostic views can cost more than numerical tracking. Use correct source pacing, inspect skips, and test the entire rig count under the intended experiment load.

Use a distinct UDP destination and ZeroMQ bind per instance. PUB binds are localhost-only in the current implementation. Queue/drop counters describe application-level behavior; they cannot certify subscriber delivery. The exact wire contract and its loss semantics are in [outputs](output.md).
