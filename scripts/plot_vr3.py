"""Development-only trajectory/quality figures for the private VR3 benchmark.

Requires matplotlib in the analysis environment; never imported by the Rust app.
Pass the full all-frame candidate and benchmark report so rejected intervals and
long-sequence drift remain visible. No trajectory is interpolated or refitted.
"""
import argparse
import csv
import json
import math
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

from benchmark_vr3 import FAST, HEALTHY, STEM, dat_rows


def series(rows, column):
    return [row[column] for row in rows]


def figure_paths(reference, candidate, mapping, report, output, comparisons):
    begin, end = HEALTHY
    selected = list(range(begin, end))
    times = [i / 100 for i in selected]
    refs = [reference[mapping[i]] for i in selected]
    blue, orange = "#176b9c", "#c75d15"
    traces = comparisons + [("Rust current", candidate, orange)]
    fig, axes = plt.subplots(2, 3, figsize=(17, 10), layout="constrained")
    ax = axes[0, 0]
    ax.plot(series(refs, 14), series(refs, 15), color=blue, lw=1.2, label="OG live data (frame-mapped)")
    for label, rows, color in traces:
        selected_rows = [rows[i] for i in selected]
        ax.plot(series(selected_rows, 14), series(selected_rows, 15), color=color, lw=1, alpha=.9, label=label)
    ax.set(title="Whole clear pre-hand trajectory: raw coordinates", xlabel="Path X (sphere rad)", ylabel="Path Y (sphere rad)")
    ax.set_aspect("equal", adjustable="datalim")
    ax.legend(fontsize=8)

    first, last = FAST
    ref0 = reference[mapping[first]]
    rx, ry = [], []
    for i in range(first, last):
        r = reference[mapping[i]]
        rx.append(r[14] - ref0[14]); ry.append(r[15] - ref0[15])
    ax = axes[0, 1]
    ax.plot(rx, ry, color=blue, lw=1.5, label="OG live data")
    for label, rows, color in traces:
        origin = rows[first]
        angle = ref0[16] - origin[16]
        cosine, sine = math.cos(angle), math.sin(angle)
        xs, ys = [], []
        for i in range(first, last):
            x, y = rows[i][14] - origin[14], rows[i][15] - origin[15]
            xs.append(cosine * x - sine * y); ys.append(sine * x + cosine * y)
        ax.plot(xs, ys, color=color, lw=1.2, label=label)
    ax.set(title="Fast section, video frames 1690–2099", xlabel="Relative path X (sphere rad)", ylabel="Relative path Y (sphere rad)")
    ax.set_aspect("equal", adjustable="datalim")
    ax.legend(fontsize=8)

    ax = axes[0, 2]
    fast = list(range(1670, 2140))
    ax.plot([i / 100 for i in fast], [reference[mapping[i]][1] for i in fast], color=blue, lw=1.2, label="OG live data")
    for label, rows, color in traces:
        ax.plot([i / 100 for i in fast], [rows[i][1] for i in fast], color=color, lw=.9, label=label)
    ax.set(title="Acceleration and rapid rotation: every image", xlabel="Recorded-video time (s)", ylabel="Camera ΔX (rad/image)")
    ax.legend(fontsize=8)

    ax = axes[1, 0]
    rolling = report["rolling_healthy_100_image_rotation"]["windows"]
    x = [window["video_frame_interval"][1] / 100 for window in rolling]
    for index, color, name in [(0, "#176b9c", "X"), (1, "#b4730c", "Y"), (2, "#41905b", "Z")]:
        ax.plot(x, [1000 * window["rotation_component_rmse_rad_all_decisions"][index] for window in rolling], color=color, lw=1, label=name)
    ax.axhline(20, color="#9f2430", ls="--", lw=1, label="20 mrad/component gate")
    ax.set(title="Every rolling 100-image interval; all decisions", xlabel="Window end, recorded-video time (s)", ylabel="Increment component RMSE (mrad)")
    ax.legend(fontsize=8)

    ax = axes[1, 1]
    for label, rows, color in traces:
        path_error = [math.hypot(rows[i][14] - reference[mapping[i]][14], rows[i][15] - reference[mapping[i]][15]) for i in selected]
        ax.plot(times, path_error, color=color, lw=1.2, label=label)
    ax.legend(fontsize=8)
    ax.set(title="Global trajectory difference is retained", xlabel="Recorded-video time (s)", ylabel="Path-point error (sphere rad)")

    ax = axes[1, 2]
    for label, rows, color in traces:
        heading = [math.atan2(math.sin(rows[i][16] - reference[mapping[i]][16]), math.cos(rows[i][16] - reference[mapping[i]][16])) for i in selected]
        ax.plot(times, heading, color=color, lw=1.1, label=label)
    ax.legend(fontsize=8)
    ax.set(title="Global wrapped heading difference", xlabel="Recorded-video time (s)", ylabel="Heading error (rad)")
    for ax in axes.flat:
        ax.grid(alpha=.2)
    for ax in axes[1, :]:
        ax.axvspan(FAST[0] / 100, FAST[1] / 100, color="#c75d15", alpha=.08)
    fig.suptitle("VR3: motion estimates and trajectory through the fast section", fontsize=17)
    fig.supxlabel("Fast-path panel aligns initial position and heading only; every later error remains. OG live .dat and compressed replay differ. No physical ground truth or long-path parity is claimed.", fontsize=10)
    fig.savefig(output / "trajectory.png", dpi=160)
    fig.savefig(output / "trajectory.pdf")
    plt.close(fig)


def figure_quality(candidate, quality, report, output):
    times = [i / 100 for i in range(len(candidate))]
    fig, axes = plt.subplots(2, 1, figsize=(15, 6), sharex=True, layout="constrained")
    errors = [float(row["fit_error"]) for row in quality]
    plotted = [min(65025, value) for value in errors]
    axes[0].plot(times, plotted, color="#176b9c", lw=.9)
    axes[0].set(ylabel="Binary-map MSE", ylim=(0, 66000), title="All images: fit score, missing measurements and recovery")
    codes = {"initializing": 0, "tracked": 1, "rejected": 2, "recovering": 3, "relocalized": 4}
    axes[1].scatter(times, [codes[row["status"]] for row in quality], s=4, color="#176b9c")
    axes[1].set(yticks=list(codes.values()), yticklabels=list(codes.keys()), xlabel="Recorded-video time (s)")
    for ax in axes:
        ax.axvspan(46.9, 48.2, color="#c75d15", alpha=.14, label="Hand / reappearance guard")
        ax.axvline(48.2, color="#41905b", ls="--", lw=1)
        ax.grid(alpha=.2)
    axes[0].legend(fontsize=9)
    fig.supxlabel("Unscoreable fit sentinel values above 65025 are clipped only in this figure; exact scores and every decision remain in quality.csv and the JSON report.", fontsize=10)
    fig.savefig(output / "quality.png", dpi=160)
    fig.savefig(output / "quality.pdf")
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--candidate-dir", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--og-replay", type=Path, help="OG .dat from replaying this same AVI, with frame IDs 0..5156")
    parser.add_argument("--compare", action="append", default=[], metavar="LABEL=DIRECTORY", help="Additional Rust candidate directories, shown without hiding their drift")
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    reference = {int(row[0]): row for row in dat_rows(args.reference_dir / f"fictrac-{STEM}.dat")}
    mapping = [int(value) for value in (args.reference_dir / f"fictrac-vidLogFrames-{STEM}.txt").read_text().splitlines()]
    candidate = dat_rows(args.candidate_dir / "all-frames.dat")
    with (args.candidate_dir / "quality.csv").open(newline="") as stream:
        quality = list(csv.DictReader(stream))
    if len(candidate) != 5157 or len(quality) != len(candidate) or len(mapping) != len(candidate):
        raise ValueError("expected the full 5,157-image candidate and quality evidence")
    comparisons = []
    if args.og_replay:
        rows = dat_rows(args.og_replay)
        if len(rows) != 5157 or any(row[0] != i for i, row in enumerate(rows)):
            raise ValueError("OG replay must contain exact recorded-video frame IDs 0..5156")
        comparisons.append(("OG same AVI", rows, "#808080"))
    colors = ["#41905b", "#9366a4", "#aa9944"]
    for number, entry in enumerate(args.compare):
        label, directory = entry.split("=", 1)
        rows = dat_rows(Path(directory) / "all-frames.dat")
        if len(rows) != 5157 or any(row[0] != i for i, row in enumerate(rows)):
            raise ValueError("Comparison must contain exact recorded-video frame IDs 0..5156")
        comparisons.append((label, rows, colors[number % len(colors)]))
    args.output_dir.mkdir(parents=True, exist_ok=False)
    figure_paths(reference, candidate, mapping, report, args.output_dir, comparisons)
    figure_quality(candidate, quality, report, args.output_dir)
    print(args.output_dir.resolve())


if __name__ == "__main__":
    main()
