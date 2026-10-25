# Configuration reference

The native editor and legacy CLI use FicTrac-style plain text. A field is written `key : value`; comments, unknown keys, whitespace and line endings survive unrelated edits. Raw text is canonical and never executable. The GUI, field commands and graphical edits share validation and undo/redo.

```text
src_fn        : 0
vfov          : 50
fisheye       : n
q_factor      : 6
sock_host     : 127.0.0.1
sock_port     : 1111
zmq_host      : 127.0.0.1
zmq_port      : 9871
```

This fragment is **not a complete calibrated configuration**. Use a saved rig file or calibrate the actual source. A video path can replace the integer `src_fn`. Relative source resolution follows the frontend/config path rules; an explicit `--input` is unambiguous for offline CLI use.

## Source and projection

| Field | Meaning |
|---|---|
| `src_fn` | Camera identifier or video path |
| `thr_rgb_tfrm` | Optional red/green/blue source channel for threshold tracking; otherwise integer RGB-to-gray conversion |
| `src_fps` | Positive explicit source-rate override; unset/nonpositive keeps native source timing |
| `rust_camera_width`, `rust_camera_height` | Exact advertised camera format request |
| `vfov` | Vertical field of view, degrees |
| `fisheye` | Equidistant fisheye projection (`y`) or pinhole (`n`) |
| `roi_circ` | Source-pixel rim clicks |
| `roi_c`, `roi_r` | Saved camera-ray sphere centre and angular half-radius; take precedence over old rim clicks |
| `roi_ignr` | Source-pixel exclusion polygons |
| `c2a_cnrs_xy`, `c2a_cnrs_yz`, `c2a_cnrs_xz` | Source-pixel lab-square corners |
| `c2a_src` | Selected square plane |
| `c2a_r`, `c2a_t` | Saved camera-to-animal pose; saved rotation takes precedence over old corner clicks |

Pixel geometry is not silently normalized on import. The explicit `resolution_reference`/`resolution_apply` operations save normalized extension metadata and regenerate legacy pixel marks only for matching aspect ratio, field of view and projection. Angular fit values retain their meaning. Source resize/crop changes still need review; see [calibration](calibration.md).

## Tracking and quality

| Field | Meaning |
|---|---|
| `q_factor` | Resolution of the angular tracking ROI; more pixels usually means more work |
| `thr_ratio` | Adaptive foreground/background threshold balance |
| `thr_win_pc` | Adaptive threshold neighborhood fraction of ROI width |
| `opt_max_evals` | Bounded optimizer evaluation budget, including complete final checks |
| `opt_bound` | Map optimizer per-axis half-window around its prediction, radians; this is not the pairwise rotation-vector norm cap |
| `opt_tol` | Local fit termination scale |
| `opt_max_err` | Maximum accepted fit error when enabled; map-mode scale differs from pairwise mode |
| `opt_global_evals` | Explicit budget for the legacy global covering/multistart path |
| `opt_do_global` | Enable the explicit legacy global atlas search path |
| `rust_resilient` | `y` by default: preserve learned atlas/pose/path on unreliable frames and use bounded confirmed reacquisition; `n` restores legacy permissive/reset policy |
| `max_bad_frames` | Legacy consecutive-failure reset threshold; default disabled behavior is retained |
| `sphere_map_fn` | Imported equal-area confidence map |
| `rust_engine` | `map` default for legacy text, or explicit `pairwise` alternative |

With `rust_resilient : y`, unreliable observations are rejected even when `opt_max_err` is unset/negative, and automatic `max_bad_frames` clearing is suppressed so a temporary obstruction cannot erase the learned atlas. Reacquisition uses a bounded accepted-view bank, is throttled and capped to `min(opt_global_evals, 768)` objective evaluations per call, and requires confirmed observations (normally two images; a consistent new-speed epoch uses the four-image path). `recovering` remains unaccepted while seeking or confirming a match. See [occlusion recovery](recovery.md). A recovered absolute pose produces `relocalized` with zero relative motion across the missing interval. The policy is an intentional behavior extension; use `rust_resilient : n` for the older permissive/reset behavior. v0.1.3 development also checks robust recent accepted-motion rates and delays atlas teaching after recovery; a low image score alone does not imply acceptance. No new config fields are required, and the legacy opt-out stays `rust_resilient : n`. It cannot recover hidden motion or guarantee recovery from every scene.

The accelerated local search proposes candidates on a deterministic site subset and verifies the selected candidate/predictor against all valid sites. The reported complete score controls quality acceptance. Optimizer differences mean trajectories need not be byte-identical to upstream. Do not transfer an error threshold between engines without understanding their [error scales](output.md).

## Recording and display

| Field | Meaning |
|---|---|
| `save_raw`, `save_debug` | Enable source/diagnostic video for configured recording |
| `vid_codec` | `h264`, `h264_mp4`, `xvid`, `mpg4`, `mjpg` or `raw` |
| `output_fn` | Legacy output prefix used by conventional CLI compatibility |
| `do_display` | Legacy display preference; native interactive view controls are also available |
| `rust_instance_name` | Human-readable VR/session label |

## Transport

| Field | Meaning |
|---|---|
| `sock_host`, `sock_port` | UDP destination; nonpositive port disables |
| `com_port`, `com_baud` | Serial device and baud rate |
| `zmq_host`, `zmq_port` | Localhost PUB bind; nonpositive port disables |
| `zmq_queue_capacity` | Bounded publish queue, 1–4096 rows; default 256 |

Ports and external handles reconfigure transactionally. Unchanged healthy publishers remain open. A failed new output attempts rollback without discarding numerical source state. See [measurement/transport reference](output.md) for exact messages and loss behavior.

## Editing and persistence

```sh
./hejjetrak-cli config --config VR1.txt --set thr_ratio=1.25 --output revised.txt
```

With no output path, `config` prints the edited document. Graphical calibration updates coupled values atomically. Numerical edits start a new tracking epoch on the current source image; unchanged input keeps its decoder position and clock continuity. A failed edit leaves state unchanged. Explicit Save writes the chosen file; external conflicts remain visible.

The [attributed upstream parameter table](upstream-parameters.md) is preserved as historical reference. Its defaults and implementation notes describe upstream, so use this chapter and the discovered current schema for hejjetrak-specific behavior. Strict JSON is supported by the CLI for modern configurations; the desktop canonical editor uses legacy text.
