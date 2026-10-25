# Recording and files

![Native recording state on the public sample](images/recording.png)

The active recording control changes to a red Stop icon and shows **REC** where
space permits. Healthy tracking uses green; unreliable tracking uses amber.
Labels, icon changes and tooltips retain meaning without relying on colour alone.
This is an actual v0.1.3 downloaded Mac build recording the bundled sample.

Use the Record control to start or stop immediately. Select data only, raw video, processed diagnostic video or both. The transient selection does not change the saved config or reset tracking. Canonical `save_raw` and `save_debug` still select media when a recording starts from config.

## Keep the whole take

Each take has a new directory and retains numerical data, quality decisions and effective configuration. A normal completed export includes:

| File | Meaning |
|---|---|
| `tracking.dat` | Legacy 25-column accepted rows; rejected rows are omitted in legacy mode |
| `all-frames.dat` | Full numeric record of every processed frame |
| `quality.csv` | Per-frame status, acceptance and fit error |
| Saved config | Source/calibration/tracking settings for that epoch |
| `path.svg` | Convenient path plot, in sphere radians |
| `summary.json` | Headless summary, settings and timing boundaries |
| Raw/debug media | Present when requested; epoch/take naming depends on frontend |

Native recording may use epoch-numbered filenames. A numerical config edit closes the current recording instead of silently combining incompatible calibrations. Start another take for the new epoch. Default native recording directories are in the user's Movies/Videos `hejjetrak` folder; the inspector permits another destination.

## Video codecs

| `vid_codec` | Container/codec |
|---|---|
| `h264` | H.264 in AVI, default |
| `h264_mp4` | H.264 in MP4 |
| `xvid` | MPEG-4 Visual in AVI |
| `mpg4` | MPEG-4 Visual in MP4 |
| `mjpg` | MJPEG in AVI |
| `raw` | Uncompressed AVI |

These are real Rust encoders, not renamed or silently substituted formats. H.264 uses independent High-profile frames (GOP1), prioritizing predictable encoding cost over maximum inter-frame compression. Even display dimensions retain their intended crop; odd H.264 4:2:0 dimensions are rejected. Large AVI recordings use segmented OpenDML indexes.

Recording adds conversion, encoding and disk costs beyond `ms track`. Raw and diagnostic recording together encode two streams. Validate your intended recording selection at the real source resolution and rate; a tracker-only timing is not an encoder throughput guarantee.

## Finalization and interruption

Files begin with `.partial` and are promoted only after normal finalization. Closing normally joins the worker and finalizes encoders. An interrupted process, power loss or failed disk write can leave partial artifacts; keep them for diagnosis rather than assuming a complete take. Outputs never overwrite an existing experiment directory. Completion is not an atomic directory transaction, so an I/O failure during final renaming can leave a mix of promoted and partial files.

Keep quality data alongside legacy `.dat`. A zero delta in rejected all-frame evidence denotes missing measurement; it does not establish that the animal or ball stopped. See the complete [measurement contract](output.md).
