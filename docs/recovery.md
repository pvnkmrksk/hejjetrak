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
on subsequent images. Ordinary recovery needs two matching images; a genuine
rate change uses the stronger four-image confirmation described below. The
`relocalized` row has zero relative rotation and preserves measured displacement. Later
`tracked` rows measure new relative movement. No trajectory is invented across
the hidden interval. See [the output contract](output.md).

## Acceleration with a partly learned atlas

A stationary startup learns only the visible side of the ball. Freezing every
atlas cell for the eight-row acceleration probation could then prevent a
correctly recovered turn from learning its newly visible side. The fit lost
known surface before probation expired and could remain stuck despite clear
images. Increasing the optimization budget did not fix that failure.

After the existing four-image acceleration confirmation, the relocalized image
and the next eight accepted tracked images may now initialize genuinely unseen
atlas cells. Each cell receives its ordinary first-hit confidence once. All
already observed cells, including cells added earlier in probation, remain
unchanged until full learning resumes. A neutral value of 128 counts as unseen
only when it also has no teaching history; imported nonneutral cells remain
protected even though template imports reset ages. Imported 128 retains the
template format's existing unseen meaning.

Error, edge and known-view histories remain frozen throughout probation.
Rejected images cannot initialize or change cells, and rejection clears this
special teaching permission. Ordinary occlusion recovery retains its complete
three-row teaching deferral. Image-error limits, motion gates, four-image
confirmation, optimizer budgets and zero-delta recovery semantics are unchanged.
The optimizer and displayed/exported atlas still use the same committed map.
This does not resolve the ambiguity of a consistently plausible false view.

Two cold-start known-motion regressions use 40 stationary images followed by
90 images at 0.20 rad/frame. Both recover on image four, measure all 86 later
increments and grow their learned surface without rewriting known cells during
probation. They retain the existing 0.008 rad/component RMS gate. One preserves the
historical 50 evaluations and quality six; the other uses 150 evaluations and
quality twelve to distinguish the teaching failure from solver precision.

## Guarded initialization and appearance adaptation

Default quality remains six; the shared local evaluation budget is now 100.
Actual-default sharp/exposure-blurred fast-motion regressions retain the same
0.008 rad/component gate as the explicit historical configurations.

Before a clean atlas can initialize, at least half the active ROI sites must
have local mixed evidence: a cross neighborhood of radius `max(ROI width / 8, 1)`
needs at least two black and two white samples. Uniform unsupported regions are
not stamped into new atlas cells. During ordinary resilient learning, an image
with that support across at least half the active ROI can teach marking interiors at the ordinary confidence rate.
A lower-support accepted view requires local mixed support for each new cell;
it cannot stamp its entire uniform panel into the atlas. Existing cells retain
their usual confidence update and mature-cell inertia; no cells are cleared.
Acceleration probation's new-surface-only updates always require local support.
Neutral atlas value 128 can therefore mean unsupported as well as unseen.
This guard does not distinguish an unrelated textured obstruction from a ball
at first observation.

A quiet startup can also make accepted error history too strict for subsequent
appearance changes. When the persistent-map full MSE exceeds its adaptive limit,
a separate previous-accepted-image map can verify the same proposed pose. It
requires at least 32 known sites, at least half of all possible sites known,
binary MSE at most 10000, and at least 85% of known samples in spatial tiles
with at least eight samples and MSE at most 16000 (a 6 by 6 tile grid). Acceptance
also requires three consecutive accepted motion observations or the equivalent
three visible increments already established by strong four-image acceleration
confirmation. Its temporal score must be less than 75% of the persistent score,
and the explicit `opt_max_err` cap always applies. Persistent MSE at most 18000
retains the earlier route. Above 18000, at least 32 persistent samples and at
least half of the possible sites must be known, and persistent MSE must be at
most 85% of the independent image/map-marginal score:
`E[I²] - 2 E[I] E[M] + E[M²]`. For binary ROI observations `E[I²] = 255 E[I]`.
This measures whether the atlas still contains information about the image;
it is not a new reported error scale. Empty, nonfinite or zero null scores
cannot authorize the higher-error route. Texture and motion guards still apply.
A supported accepted frame learns normally under guarded seeding and updates
accepted quality history. Recovery probation continues to limit learning as above.

This single-image temporal reference is invalidated by rejection, reset and
map import. It cannot bridge an unmeasured interval or bypass recovery
confirmation. An uncertain primary fit (score failure or motion-history outlier)
can first be refined against that last accepted binary observation. The better temporal
seed of the original prediction and primary candidate starts a second bounded
BOBYQA fit with a finer initial step; the full configured prediction window
is retained. It is selected only with spatial verification and at least 10%
better temporal MSE, then scored against the complete persistent atlas. The
same final motion, appearance and explicit-cap gates apply. Healthy local
fitting and legacy opt-out retain their previous numerical objective.
Each optimizer is capped by `opt_max_evals`; the fallback adds at most five
complete comparison scores and two complete evidence passes. This is bounded
extra computation, not an unchanged total per-frame evaluation count.

The reported full persistent-map MSE is never replaced by its smaller temporal
score. Strong-confirmation probation retains existing-cell protection and
new-surface-only learning; it no longer demands three redundant tracked rows
before validating the already confirmed rate's blurred appearance. Rejection
revokes that permission immediately. [Current take validation](take-validation.md)
records the fast-motion result and repeated masking controls; the
[earlier masking assessment](masking-validation.md) retains its original scope.

## Continuous acceleration and long-lived reference views

Resilient healthy fitting now uses the complete ROI throughout the original
`opt_max_evals` budget. Final-score verification alone could not remove bias
from a sampled optimizer objective on blurred motion. The legacy opt-out keeps
sampled candidate generation, and no image-error unit or numerical regression
tolerance changes.

The current branch additionally distinguishes an observed acceleration ramp
from an isolated motion outlier. Three consecutive accepted velocity vectors
must increase in magnitude with consistent acceleration and direction (adjacent
direction cosines at least 0.75). A fourth complete-score-verified image may
establish a new motion-rate epoch within a bounded acceleration prediction.
Rejected images clear this evidence. The image-error and texture ceilings do
not change. This permits continuous measurement and atlas teaching during the
supplied VR3 fast spin, instead of declaring a false gap and withholding newly
visible surface from the atlas. It is a temporal consistency policy, not a
physical acceleration limit or permission to integrate an occluded interval.

Recovery retains the measured increment between its final visible confirmation
images as the next optimizer seed. The relocalized output still has zero relative
motion. The 256-view bank preserves spatially distinct older poses: a new pose
can replace a member of the closest existing pair when it improves their
spacing. A long run through one cluster can no longer cyclically evict all
earlier views. Nearest-neighbor caches keep ordinary insertion linear in the
256-view bound; an eviction can require a bounded quadratic cache repair.
Recovery ranks probes using up to 512 ROI sites and still verifies candidates on
the complete ROI within the original 768-objective-evaluation cap.

## Mature atlas inertia

The original confidence update already freezes saturated black/white cells
(`0`/`255`) permanently. In resilient mode, the current branch also protects
strong, partially consolidated cells against contradictory observations. Ages
count distinct accepted teaching frames per cell, including colliding ROI
samples only once. The first 1,024 teaching frames retain the original learning
rate. Thereafter contradictory votes against values at least 160 or at most 96
are applied every sixteen teaching frames.
Agreement still consolidates at the original rate; near-neutral cells adapt at
the original rate, and genuinely new cells retain their first-hit bonus.
A mature observed cell crossing neutral `128` does not receive another first-hit
bonus; younger cells retain the original update behavior.

This adds inertia when a few unreliable observations pass the quality checks;
it does not establish that an arbitrary accepted occluder is safe. A regression
explicitly supplies 300 accepted contradictory images to mature, unsaturated
cells. Rejected/recovering frames remain unable to teach anything. Per-cell
ages and frame stamps are bounded and their wrap behavior is tested. A valid
reset/template import resets age history; PNG confidence maps do not serialize
ages, so imported partial cells rebuild maturity over 1,024 teaching frames.
`rust_resilient : n` retains the original confidence-update policy.

The private [VR3 validation](vr3-validation.md) reports the complete recording,
source-frame mapping, fast section, trajectory drift and recovery separately.

## Earlier protection policy

The following describes v0.1.3. Local formatting, strict Clippy, all 173 workspace
tests and the release build pass. All four v0.1.3 native release jobs and the
downloaded Mac archive checks pass too. The v0.1.2 protection and long-occlusion
fixtures remain the historical baseline. See [validation](validation.md).

The normal local fit keeps its complete final ROI score. Resilient mode also
checks for an almost uniform/textureless ROI and for a large drop in edge density
relative to accepted images. Its complete-score limit adapts from accepted-fit
history using the existing exponentially smoothed mean ceiling. Once eight
teaching samples exist, it also applies
`clamp(1.8 × median(last 32 teaching errors) + 2000, 10000, 18000)`.
The smallest applicable limit wins, including nonnegative `opt_max_err`; the
robust median never loosens the previous ceiling. These are binary map MSE
units, not pixel distances or physical accuracy. Ordinary clean tracking still
uses `opt_max_evals` and complete final scoring.

The per-video `rust_resilient_error_floor` setting can raise the lower bound of
both adaptive ceilings from its unchanged default 10000, up to 18000. The upper
bound remains 18000, and a nonnegative `opt_max_err` remains a hard cap even
below the chosen floor. This is an explicit scene-specific allowance for image
appearance changes, not a new error unit or an accuracy guarantee. It leaves
motion, texture, confirmation and atlas protections active. The default and an
explicit value of 10000 retain that ceiling formula exactly. The independent
current-source temporal verification path described above can support an
otherwise out-of-ceiling local fit; the floor setting alone does not do so. Invalid/nonfinite
values fail configuration validation before tracking state changes.

A separate accepted-motion guard addresses low-error textured impostors. After
eight accepted tracked motions, it estimates rotation rate from the last 32
accepted rotation magnitudes divided by their processed-frame intervals.
A candidate must satisfy
`angle <= max(0.04 rad, (4 × median(rate) + 8 × MAD(rate)) × current delta_ms)`.
MAD is the median absolute deviation of accepted rates. The 0.04 rad floor
is a **per-frame allowance**, not an FPS-independent physical velocity. The rate
term accounts for each processed-frame period. During bootstrap, before eight
motion samples exist, image/score gates still apply but this history-based motion
restriction does not. This is a consistency heuristic, not physical ground truth
or a universal speed limit.

Ordinary recovery confirmation checks motion between consecutive proposed visible
poses against the same guard. A good image-score motion outlier remains only an
unaccepted candidate; it never teaches the atlas or appears as measured motion.
Its fitted velocity is retained only as a proposal for the next recovery search.
The proposal is compared with the held pose on the complete ROI before choosing
the solver's starting pose. If that fit fails the unchanged image-score limit,
the solver can fall back to the held pose, within the same total recovery budget.
Without this proposal, recovery repeatedly assumed zero velocity even after a
good fast-motion fit and could remain stuck on a clearly patterned rotating ball.
An extrapolation beyond the existing visible-increment bound is discarded.

To avoid permanent lock after genuine acceleration, an alternate confirmation
requires **three mutually consistent fitted angular velocities over four good
images**. Each increment must remain within `sqrt(3) × opt_bound`; rate consistency
uses `max(0.006 rad / current interval, 35% of current rate)` in rad/ms. This
stronger confirmation starts a fresh accepted-motion-rate epoch and reports
zero-delta relocalization. It constrains visible confirmation increments, not
absolute displacement across the hidden interval, and invents no gap trajectory. A rejected
motion retains its actual complete-ROI MSE, even if low; a small image error alone
therefore does not imply an accepted measurement.

After an ordinary confirmed `relocalized` row, that row and the next three accepted
`tracked` rows do not update the atlas, error/edge learning or known-view bank.
Their accepted relative motion still updates path/output normally; accepted
tracked motion history continues to update. This short atlas-learning probation
keeps a newly recovered pose from immediately rewriting its own reference.
Ordinary teaching resumes afterward. Acceleration-epoch recovery instead protects
existing atlas cells and freezes error/edge/known-view teaching for **eight**
following accepted tracked rows, matching motion-history bootstrap. Current
source permits the first-hit new-surface teaching described above. Rejected rows change none of those
learned histories, pose, path or integrated displacement.

Accepted orientations form a bounded bank of up to 256 known views, separated by
at least approximately 0.08 radians. This costs about 8 KiB of quaternion storage
per instance. Reacquisition first rescans known views rather than asking a very
coarse whole-sphere search to rediscover an earlier viewpoint. A rotating,
deterministic SO(3) cover remains a fallback for unfamiliar poses. Every proposed
pose receives complete ROI verification before it can be confirmed. In v0.1.3,
rare recovery solver refinements also optimize the **complete ROI objective**,
rather than merely checking a sampled-objective result afterward. The healthy
local optimizer stays unchanged and the same recovery evaluation cap applies.

Recovery runs local checks selectively and a broader known-view/fallback search
at most once per 30 unresolved textured frames. A single recovery call spends at
most `min(opt_global_evals, 768)` actual objective evaluations, including coarse
probes, local solver calls and complete final scores. Budget exhaustion cannot
publish an unverified candidate. It does not start additional tracker threads,
run a GPU search or consume the full default 100000-evaluation global allowance
on every occluded image. Uniform images fail before search. The pose bank is
updated only by accepted teaching frames outside recovery probation and is cleared by an explicit tracker reset or
new template/configuration.

In resilient mode, `max_bad_frames` cannot automatically discard the learned
atlas. Bounded reacquisition is available even if the legacy `opt_do_global` is
`n`. With `rust_resilient : n`, `opt_do_global` and `max_bad_frames` retain their
previous global-search/reset meanings. Explicit Reset remains available when the
calibration, ball appearance or initial reference is wrong; initialize while the
ball is visible.

## What is tested

A default-budget regression first learns 330 sharp images at 0.02 rad/frame,
then changes to 0.20 rad/frame for 90 images at 30 frames/s. It covers both sharp
images and symmetric nine-sample exposures spanning half the frame interval.
The identical regression fails against the unmodified v0.2.1 engine: no fast
frame is recovered. With the held-velocity search proposal, both cases recover
on the fourth visible image and measure all 86 subsequent relative deltas.
Component RMS is `[0.003609, 0.002340, 0.005844]` rad for sharp images and
`[0.005320, 0.002951, 0.005703]` rad for the blurred case, under the unchanged
0.008 rad/component synthetic gate. The tests also check exact atlas/pose/path
preservation before confirmation, zero-delta relocalization and the eight-row
atlas-learning probation. The fit budget stays at the actual default 50
evaluations; no image-error, edge-density or motion acceptance threshold changes.
These are controlled rendering tests, not a measured physical-camera speed or
exposure limit. [Source hashes, exact results and unchanged gates](benchmarks/fast-motion-recovery-20261026.json)
retain the regression evidence.

The existing actual-video full/partial-obstruction regression also passes with
the proposal change. Its 178 measured suffix rows in each case retain the exact
path, heading and rotation errors recorded in the v0.1.3 table below; the original
0.075 sphere-radian path, 0.025 radian heading and 0.020 radian/component gates
are unchanged.

A v0.1.3 known-motion acceleration fixture changes roughly 0.0054 to 0.065
rad/frame. It requires rejecting the first jump, recovering on the fourth good
image, then measuring 86 subsequent deltas with component RMS
`[0.002160993677, 0.001679486020, 0.003679960497]` rad, within the unchanged
0.008 rad/component RMS gate. A separate stationary-to-moving fixture also
retains its existing 0.008 rad sequence gate. These fixtures pass in the complete
173-test local workspace suite; they are not physical-camera acceleration bounds.

The v0.1.2 baseline fixtures below have completed evidence. For v0.1.3, a new
deterministic textured-outlier fixture reproduces the previous false acceptance
of approximately 0.063 rad at complete MSE 3749.68. Its regression checks exact
atlas preservation and an ensuing 180-frame clear suffix against unchanged
0.075 sphere rad path, 0.025 rad heading and 0.012 rad/component limits. The local
workspace suite, all four native release jobs and downloaded Mac archive
checks pass.

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

### Stronger actual-video recovery checks in v0.1.3

The strengthened actual-sample fixture inserts 240 full obstructions: 80 black,
80 horizontal-gradient and 80 unrelated-pattern frames. Recovery occurs on the
**second clear image**; its remaining 178 measured suffix rows are compared with
the unobstructed control after aligning only the unknown initial
translation/heading offsets. The partial-obstruction case covers approximately
70% of visible ball pixels with 60 black and 60 vertical-gradient frames, then
likewise checks 178 measured suffix rows.

| Actual-video fixture | Path RMS, sphere rad | Heading RMS, rad | Rotation component RMS, rad |
|---|---:|---:|---|
| Full obstruction, 80/80/80 sequence | 0.015623 | 0.010662 | [0.007349, 0.007956, 0.012259] |
| Approximately 70% partial obstruction | 0.015419 | 0.010577 | [0.008069, 0.008420, 0.014038] |
| Unchanged gates | 0.075 | 0.025 | 0.020 each |

Exact metrics, input, build profile and unchanged gates are retained in the
[recovery evidence](benchmarks/recovery-20261025.json). These correctness tests
are not timing measurements.

These focused fixture results use complete-ROI recovery refinement. The previous
sampled recovery refinement missed the existing heading gate (0.025728 versus
0.025); the solver objective was corrected rather than loosening that tolerance.
The complete local suite, native lifecycle checks, four-platform release CI
and downloaded Mac archive checks pass. Native hover/loupe pointer interaction
remains incomplete; [validation](validation.md) records each boundary separately.

These are deterministic fixtures, not proof that every real obstruction can be
recognized. A patterned occluder resembling the ball, very little learned map,
blur, changed lighting, physical ball replacement or an unseen pose can remain
ambiguous. The bounded search can take several attempts or fail to reacquire;
uncertain intervals remain rejected. Real live-ball occlusion on camera and
Windows/Linux camera hardware have not been validated by these tests. Source
acquisition and recording can dominate resource usage independently of the
tracking-stage latency.
