# Tracking through an obstruction

`rust_resilient : y` is the default for spherical-map tracking. It protects the
learned atlas when a hand or another object hides the ball, then tries to resume
without clearing the past. The checkbox **Preserve atlas and recover after
occlusion** in Tracking & quality controls the same field. Set `rust_resilient : n`
to retain the original permissive quality/reset policy; configuration geometry,
coordinate frames and numeric columns do not change.

A rejected image does not update the atlas, absolute pose, path or integrated
movement. It remains present in all-frame numeric data and quality.csv. A frozen
path means **missing measurement**, not zero physical movement. When the ball is
visible again, the first plausible absolute-pose match is held for confirmation
on the following image. Only a second matching image produces `relocalized`;
that row has zero relative rotation and preserves measured displacement. Later
`tracked` rows measure new relative movement. No trajectory is invented across
the hidden interval. See [the output contract](output.md).

## How the protection works

The normal local fit keeps its complete final ROI score. Resilient mode also
checks for an almost uniform/textureless ROI and for a large drop in edge density
relative to accepted images. Its complete-score limit adapts from accepted-fit
history: `clamp(1.8 × mean_error + 2000, 10000, 18000)`. A nonnegative
`opt_max_err` remains an additional, potentially stricter ceiling. These are
binary map MSE units, not pixel distances or physical accuracy. The error/texture
history learns only from accepted images. Ordinary clean tracking still uses
`opt_max_evals` and the shared numerical engine.

Accepted orientations form a bounded bank of up to 256 known views, separated by
at least approximately 0.08 radians. This costs about 8 KiB of quaternion storage
per instance. Reacquisition first rescans known views rather than asking a very
coarse whole-sphere search to rediscover an earlier viewpoint. A rotating,
deterministic SO(3) cover remains a fallback for unfamiliar poses. Every proposed
pose receives complete ROI verification before it can be confirmed.

Recovery runs local checks selectively and a broader known-view/fallback search
at most once per 30 unresolved textured frames. A single recovery call spends at
most `min(opt_global_evals, 768)` actual objective evaluations, including coarse
probes, local solver calls and complete final scores. Budget exhaustion cannot
publish an unverified candidate. It does not start additional tracker threads,
run a GPU search or consume the full default 100000-evaluation global allowance
on every occluded image. Uniform images fail before search. The pose bank is
updated only by accepted frames and is cleared by an explicit tracker reset or
new template/configuration.

In resilient mode, `max_bad_frames` cannot automatically discard the learned
atlas. Bounded reacquisition is available even if the legacy `opt_do_global` is
`n`. With `rust_resilient : n`, `opt_do_global` and `max_bad_frames` retain their
previous global-search/reset meanings. Explicit Reset remains available when the
calibration, ball appearance or initial reference is wrong; initialize while the
ball is visible.

## What is tested

The known-motion core regression learns 400 clean textured-sphere images, then
inserts 240 images covering uniform, gradient and unrelated patterned occluders.
It checks exact atlas preservation, rejected evidence, no displacement during
the missing interval, confirmed return to an earlier known view within 120
visible frames, absolute-pose error below 0.08 radians and subsequent relative
rotation error below 0.012 radians. The existing numerical tolerances remain
unchanged.

The real-video regression decodes all 300 original sample images without
changing the decoder oracle, then repeatedly plays the sequence forwards and
backwards to avoid an artificial end-to-start jump. It learns 900 frames, inserts
240 full occluders and compares the measured 180-frame visible suffix with an
unobstructed control. Because hidden movement is unavailable, only the suffix's
unknown initial translation/heading offsets are aligned. Gates remain 0.020
radians per rotation component, 0.075 sphere radians path RMSE and 0.025 radians
heading RMSE. It also overlays a panel over approximately 70% of actual visible,
exclusion-masked ball pixels, tests rejection protection or accurate retained
motion, removes the panel and checks the subsequent measured rotations.

These are deterministic fixtures, not proof that every real obstruction can be
recognized. A patterned occluder resembling the ball, very little learned map,
blur, changed lighting, physical ball replacement or an unseen pose can remain
ambiguous. The bounded search can take several attempts or fail to reacquire;
uncertain intervals remain rejected. Real live-ball occlusion on camera and
Windows/Linux camera hardware have not been validated by these tests. Source
acquisition and recording can dominate resource usage independently of the
tracking-stage latency.
