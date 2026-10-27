"""Full VR3 trajectory and motion-derivative figures; development analysis only.

Every source image is represented. Missing/rejected motion appears as a gap,
never a measured zero. No path alignment, rotation or scale fit is applied.
"""
import argparse
import csv
import json
import math
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

from benchmark_vr3 import HAND, HEALTHY, POST, STEM, dat_rows, sha256

BLUE, ORANGE, GREEN, RED, GRAY = "#176b9c", "#c75d15", "#28754c", "#ad3345", "#808080"
NAN = float("nan")


def save(fig, output, name):
    fig.savefig(output / (name + ".png"), dpi=160)
    fig.savefig(output / (name + ".pdf"))
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--candidate-dir", type=Path, required=True)
    parser.add_argument("--og-replay", type=Path, required=True)
    parser.add_argument("--metrics", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    candidate = dat_rows(args.candidate_dir / "all-frames.dat")
    refs = {int(row[0]): row for row in dat_rows(args.reference_dir / f"fictrac-{STEM}.dat")}
    original_all = [refs[i] for i in sorted(refs)]
    mapping = [int(v) for v in (args.reference_dir / f"fictrac-vidLogFrames-{STEM}.txt").read_text().splitlines()]
    reference = [refs[source] for source in mapping]
    replay = dat_rows(args.og_replay)
    with (args.candidate_dir / "quality.csv").open(newline="") as stream:
        quality = list(csv.DictReader(stream))
    metrics = json.loads(args.metrics.read_text())
    if any(len(rows) != 5157 for rows in [candidate, reference, replay, quality]):
        raise ValueError("Full5157-frame evidence required")
    if metrics["files"]["candidate_all_frames_sha256"] != sha256(args.candidate_dir / "all-frames.dat"):
        raise ValueError("Metrics describe a different candidate output")
    times = [(r[21] - reference[0][21]) / 1000 for r in reference]
    original_times = [(r[21] - original_all[0][21]) / 1000 for r in original_all]
    valid = [i > 0 and mapping[i] == mapping[i - 1] + 1 and
             quality[i]["status"] == "tracked" and quality[i - 1]["accepted"] == "true"
             for i in range(len(candidate))]
    accepted = [q["accepted"] == "true" for q in quality]
    cut = HEALTHY[1]
    original_cut = mapping[cut - 1] + 1
    original_hand = mapping[HAND[0]]

    def xy(rows, start=0, end=None, mask=None):
        end = len(rows) if end is None else end
        return ([rows[i][15] if mask is None or mask[i] else NAN for i in range(start, end)],
                [rows[i][14] if mask is None or mask[i] else NAN for i in range(start, end)])

    def span(ax):
        ax.axvspan(times[HAND[0]], times[POST[0]], color=RED, alpha=.10, label="Hand/reappearance guard")
        ax.axvline(times[POST[0]], color=GREEN, lw=1, ls="--")

    # Both a full raw-coordinate plot and a usable reliable/reference detail are
    # included; the failed OG suffix remains visible at its actual extent.
    fig, axes = plt.subplots(2, 2, figsize=(15, 11), layout="constrained")
    ax = axes[0, 0]
    ax.plot(*xy(original_all, 0, original_hand), color=BLUE, lw=1.4, label="OG live before hand")
    ax.plot(*xy(original_all, original_hand), color=RED, lw=.8, alpha=.75, ls="--", label="OG live suffix: unreliable")
    ax.plot(*xy(candidate, 0, POST[0], accepted), color=ORANGE, lw=1.2, label="Rust pre-hand measured path")
    ax.plot(*xy(candidate, POST[0], mask=accepted), color=GREEN, lw=1.4, label="Rust recovered suffix")
    ax.set(title="Entire recording, raw coordinates: OG failure is retained", xlabel="East (sphere rad)", ylabel="North (sphere rad)")
    ax.set_aspect("equal", adjustable="datalim"); ax.legend(fontsize=8)
    ax = axes[0, 1]
    ax.plot(*xy(original_all, 0, original_cut), color=BLUE, lw=1.4, label="OG live clear pre-hand")
    ax.plot(*xy(replay, 0, cut), color=GRAY, lw=1, label="OG same-AVI clear pre-hand")
    ax.plot(*xy(candidate, 0, POST[0], accepted), color=ORANGE, lw=1.2, label="Rust pre-hand measured path")
    ax.plot(*xy(candidate, POST[0], mask=accepted), color=GREEN, lw=1.5, label="Rust recovered suffix")
    ax.set(title="Reliable OG portion and entire Rust continuation; no fitted alignment", xlabel="East (sphere rad)", ylabel="North (sphere rad)")
    ax.set_aspect("equal", adjustable="datalim"); ax.legend(fontsize=8)
    ax = axes[1, 0]
    for column, name, style in [(19, "forward", "-"), (20, "side", "--")]:
        ax.plot(original_times[:original_cut], [r[column] for r in original_all[:original_cut]], color=BLUE, lw=1.2, ls=style, label="OG " + name)
        ax.plot(times, [r[column] if ok else NAN for r, ok in zip(candidate, accepted)], color=ORANGE, lw=1, ls=style, label="Rust " + name)
    span(ax)
    ax.set(title="Heading-independent accumulated body movement", xlabel="Mapped source capture time (s)", ylabel="Integrated movement (sphere rad)")
    ax.legend(fontsize=8)
    ax = axes[1, 1]
    codes = {"initializing": 0, "tracked": 1, "rejected": 2, "recovering": 3, "relocalized": 4}
    ax.scatter(times, [codes[q["status"]] for q in quality], s=5, color=ORANGE)
    span(ax)
    ax.set(yticks=list(codes.values()), yticklabels=list(codes), xlabel="Mapped source capture time (s)", title="Every image decision, including the missing-motion interval")
    for ax in axes.flat:
        ax.grid(alpha=.2)
    fig.suptitle("Complete VR3 recording: trajectory, structure and post-occlusion continuation", fontsize=16)
    fig.supxlabel("All 5,163 OG capture rows and 5,157 decoded images retained. World axes: east = column 16, north = 15. Rejected motion is a gap, not measured zero. No rescaling, path rotation or hidden-interval reconstruction.", fontsize=9)
    save(fig, args.output, "whole-trajectory")

    def velocities(rows, columns, difference=False, heading=False):
        output = []
        for i in range(len(rows)):
            if not valid[i]:
                output.append([NAN] * len(columns)); continue
            dt = (reference[i][21] - reference[i - 1][21]) / 1000
            values = []
            for column in columns:
                value = rows[i][column] - rows[i - 1][column] if difference else rows[i][column]
                if heading:
                    value = math.atan2(math.sin(value), math.cos(value))
                values.append(value / dt)
            output.append(values)
        return output

    camera_r, camera_c = velocities(reference, [1, 2, 3]), velocities(candidate, [1, 2, 3])
    body_r, body_c = velocities(reference, [19, 20], True), velocities(candidate, [19, 20], True)
    heading_r, heading_c = velocities(reference, [16], True, True), velocities(candidate, [16], True, True)
    fig, axes = plt.subplots(3, 2, figsize=(16, 11), sharex=True, layout="constrained")
    for j, name in enumerate(["X", "Y", "Z"]):
        ax = axes[j, 0]
        ax.plot(times[:cut], [v[j] for v in camera_r[:cut]], color=BLUE, lw=.8, alpha=.8, label="OG live")
        ax.plot(times[:cut], [v[j] for v in camera_c[:cut]], color=ORANGE, lw=.7, alpha=.75, label="Rust")
        ax.set(title="Camera angular velocity " + name, ylabel="rad/s")
    for j, name in enumerate(["forward", "side"]):
        ax = axes[j, 1]
        ax.plot(times[:cut], [v[j] for v in body_r[:cut]], color=BLUE, lw=.8, alpha=.8, label="OG live")
        ax.plot(times[:cut], [v[j] for v in body_c[:cut]], color=ORANGE, lw=.7, alpha=.75, label="Rust")
        ax.set(title="Heading-independent body velocity: " + name, ylabel="sphere rad/s")
    ax = axes[2, 1]
    ax.plot(times[:cut], [v[0] for v in heading_r[:cut]], color=BLUE, lw=.8, alpha=.8, label="OG live")
    ax.plot(times[:cut], [v[0] for v in heading_c[:cut]], color=ORANGE, lw=.7, alpha=.75, label="Rust")
    ax.set(title="Wrapped-heading derivative", ylabel="rad/s")
    for ax in axes.flat:
        ax.grid(alpha=.2); ax.legend(fontsize=8)
        ax.axvspan(times[1690], times[2100], color=GRAY, alpha=.08)
    for ax in axes[-1, :]:
        ax.set_xlabel("Mapped source capture time (s)")
    fig.suptitle("Every comparable clear pre-hand image: motion derivatives, not only the fast section", fontsize=16)
    fig.supxlabel("Both unchanged motion estimates use the SAME original image-pair capture interval in this diagnostic. Native-clock results are reported separately. Missing source-image pairs are gaps; no smoothing or trajectory fit.", fontsize=9)
    save(fig, args.output, "prehand-derivatives")

    fig, axes = plt.subplots(3, 1, figsize=(15, 10), sharex=True, layout="constrained")
    ax = axes[0]
    ax.plot(original_times, [r[4] if r[4] <= 65025 else NAN for r in original_all], color=BLUE, lw=.9, label="OG live complete fit score")
    ax.plot(times, [r[4] if r[4] <= 65025 else NAN for r in candidate], color=ORANGE, lw=.9, label="Rust complete fit score")
    sentinels = [i for i, row in enumerate(candidate) if row[4] > 65025]
    if sentinels:
        ax.scatter([times[i] for i in sentinels], [65025] * len(sentinels), marker="x", s=9, color=RED, label="Unscoreable sentinel (exact CSV value >65025)")
    ax.set(ylabel="Binary-map MSE", title="Full recording fit scores; OG suffix becomes unreliable")
    def norm(values):
        return [math.sqrt(sum(x * x for x in row)) if all(math.isfinite(x) for x in row) else NAN for row in values]
    # OG rows remain numerical after hand, but are visibly unreliable. The
    # original capture clock remains usable even when its tracker is failing.
    og_camera = []
    og_speed = []
    for i, row in enumerate(original_all):
        if i == 0:
            og_camera.append(NAN); og_speed.append(NAN); continue
        dt = (row[21] - original_all[i - 1][21]) / 1000
        og_camera.append(math.sqrt(sum(row[j] ** 2 for j in [1, 2, 3])) / dt)
        og_speed.append(row[18] / dt)
    cand_camera = norm(camera_c)
    cand_speed = [candidate[i][18] / ((reference[i][21] - reference[i - 1][21]) / 1000) if valid[i] else NAN for i in range(len(candidate))]
    for ax, original, current, title, unit in [
        (axes[1], og_camera, cand_camera, "Camera angular speed: recovered estimates return to the healthy scale", "rad/s"),
        (axes[2], og_speed, cand_speed, "Body speed: every measured interval; missing estimates remain gaps", "sphere rad/s"),
    ]:
        ax.plot(original_times[:original_hand], original[:original_hand], color=BLUE, lw=.8, label="OG pre-hand")
        ax.plot(original_times[original_hand:], original[original_hand:], color=RED, ls="--", lw=.8, label="OG hand/suffix: unreliable")
        ax.plot(times, current, color=GREEN, lw=.9, label="Rust measured intervals")
        ax.set_yscale("symlog", linthresh=.1)
        ax.set(ylabel=unit, title=title)
    for ax in axes:
        span(ax); ax.grid(alpha=.2); ax.legend(fontsize=8, loc="upper left")
    axes[-1].set_xlabel("Mapped source capture time (s)")
    fig.suptitle("Full VR3 recovery: fit quality, velocity scale and explicit missing measurements", fontsize=16)
    fig.supxlabel("Velocity panels use mapped capture intervals for both outputs. Recovery does not reconstruct motion while obscured. Healthy ranges and fresh-atlas agreement support plausibility, not independent physical ground truth.", fontsize=9)
    save(fig, args.output, "full-recovery")
    evidence = {"metrics_sha256": sha256(args.metrics), "candidate_all_frames_sha256": sha256(args.candidate_dir / "all-frames.dat"),
                "files": [{"file": path.name, "sha256": sha256(path)} for path in sorted(args.output.iterdir())],
                "scope": "Whole raw-coordinate trajectories and all derivative/quality samples. Failed OG suffix visually marked unreliable, no numerical retuning or fitted path corrections; PDFs/PNGs are standalone analysis artifacts."}
    (args.output / "figure-evidence.json").write_text(json.dumps(evidence, indent=2))
    print(args.output.resolve())


if __name__ == "__main__":
    main()
