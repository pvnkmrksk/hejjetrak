"""Plot same-video replays with every missing-motion interval visible.

Development tool; requires matplotlib/numpy, never application dependencies.
Original FicTrac is a comparison baseline, not physical ground truth. Example:

  python3 scripts/plot_raw_tracking.py --candidate main=runs/main \
      --candidate candidate=runs/candidate --reference-dat runs/og.dat \
      --output runs/comparison.png
"""

import argparse
import math
from pathlib import Path

from analyze_raw_tracking import dat_rows, quality_rows, ranges


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", action="append", required=True, metavar="LABEL=DIRECTORY")
    parser.add_argument("--reference-dat", type=Path, required=True)
    parser.add_argument("--frame-offset", type=int, default=0)
    parser.add_argument("--start-frame", type=int)
    parser.add_argument("--end-frame", type=int, help="exclusive")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("--output must name a new file")
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        import numpy as np
    except ImportError as error:
        parser.error(f"plotting requires development packages matplotlib and numpy: {error}")
    candidates = []
    for value in args.candidate:
        if "=" not in value:
            parser.error("--candidate requires LABEL=DIRECTORY")
        label, directory = value.split("=", 1)
        if not label or not directory:
            parser.error("--candidate requires a nonempty label and directory")
        rows = dat_rows(Path(directory) / "all-frames.dat")
        quality = quality_rows(Path(directory) / "quality.csv", rows)
        candidates.append((label, rows, quality))
    reference = dat_rows(args.reference_dat)
    clock = candidates[0][1]
    frames = [i for i in clock if (args.start_frame is None or i >= args.start_frame)
              and (args.end_frame is None or i < args.end_frame)]
    if len(frames) < 2:
        parser.error("the selected interval must contain at least two video frames")
    if any(b != a + 1 or clock[b][21] <= clock[a][21] for a, b in zip(frames, frames[1:])):
        parser.error("the first candidate must supply contiguous IDs and increasing video timestamps")
    for label, rows, _ in candidates:
        if any(i not in rows or abs(rows[i][21] - clock[i][21]) > 1e-6 for i in frames):
            parser.error(f"{label}: candidates must use the same frame IDs and video timestamps")
    times = np.array([clock[i][21] / 1000 for i in frames])
    dt = {i: (clock[i][21] - clock[i - 1][21]) / 1000
          for i in frames if i - 1 in clock and clock[i][21] > clock[i - 1][21]}
    if not any(i in dt and i + args.frame_offset in reference
               and i - 1 + args.frame_offset in reference for i in frames):
        parser.error("the selected interval and frame offset have no consecutive reference pairs")
    fig, axes = plt.subplots(5, 1, figsize=(13, 10), sharex=True,
                             gridspec_kw={"height_ratios": [1, 1, 1, 1.2, .8]}, constrained_layout=True)
    colors = ["#dc7b20", "#0072b2", "#009e73", "#cc79a7"]
    reference_values = []
    reference_speeds = []
    for i in frames:
        ref = reference.get(i + args.frame_offset)
        available = ref is not None and i - 1 + args.frame_offset in reference and i in dt
        reference_values.append(ref[1:4] if available else [np.nan] * 3)
        reference_speeds.append(math.sqrt(sum(v * v for v in ref[1:4])) / dt[i] if available else np.nan)
    reference_values = np.array(reference_values)
    for axis in range(3):
        axes[axis].plot(times, reference_values[:, axis], color="#292929", lw=.9, alpha=.8,
                        label="original FicTrac baseline" if axis == 0 else None)
    axes[3].plot(times, reference_speeds, color="#292929", lw=.9, alpha=.8)
    for index, (label, rows, quality) in enumerate(candidates):
        color = colors[index % len(colors)]
        measured = {i for i in frames if i in dt and quality[i]["status"] == "tracked"
                    and i - 1 in quality and quality[i - 1]["accepted"]}
        values = np.array([rows[i][1:4] if i in measured else [np.nan] * 3 for i in frames])
        speeds = [math.sqrt(sum(v * v for v in rows[i][1:4])) / dt[i] if i in measured else np.nan for i in frames]
        for axis in range(3):
            axes[axis].plot(times, values[:, axis], color=color, lw=.85, alpha=.85,
                            label=label if axis == 0 else None)
        axes[3].plot(times, speeds, color=color, lw=.85, alpha=.85)
        missing = [i for i in frames if i not in measured]
        intervals = []
        for begin, end in ranges(missing):
            start_time = clock[begin][21] / 1000
            stop_time = (clock[end][21] / 1000 if end in clock
                         else clock[end - 1][21] / 1000 + dt.get(end - 1, .01))
            intervals.append((start_time, stop_time - start_time))
            for axis in axes[:4]:
                axis.axvspan(start_time, stop_time, color=color, alpha=.04, lw=0)
        axes[4].broken_barh([(times[0], times[-1] - times[0])], (index + .1, .65),
                            facecolors="#e9ecef")
        axes[4].broken_barh(intervals, (index + .1, .65), facecolors=color, alpha=.8)
    axes[0].legend(loc="upper left", ncol=len(candidates) + 1, fontsize=9)
    for axis, name in zip(axes[:3], ["ROI x", "ROI y", "ROI z"]):
        axis.set_ylabel(f"{name}\nrad/frame")
    axes[3].set_ylabel("Angular speed\nrad/s")
    axes[4].set_yticks([i + .4 for i in range(len(candidates))], [label for label, _, _ in candidates])
    axes[4].set_ylim(0, len(candidates))
    axes[4].set_ylabel("Missing\nmeasurement")
    axes[4].set_xlabel("Video presentation time (s); both speed traces use this shared clock")
    for axis in axes:
        axis.grid(alpha=.2)
        axis.set_xlim(times[0], times[-1])
    fig.suptitle("Same-video motion comparison — original FicTrac is a baseline, no ground truth\n"
                 "Colored shading/bars mark unmeasured intervals; missing motion is never plotted as measured zero.",
                 fontsize=12)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.output, dpi=160)
    plt.close(fig)
    print(args.output)


if __name__ == "__main__":
    main()
