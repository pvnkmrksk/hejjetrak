# Overnight native validation

After the completed run, the user requested removal of old raw videos and
compact retention. See [storage retention](storage-retention.md) for the verified
lossless archives, four retained final clips and restoration commands. Historical
counts below remain the actual audit-time results.

`scripts/overnight_soak.py` is a development supervisor for the real native
application. It is not an application decoder or recording implementation.
Capture, estimation, numerical logging and video encoding remain in Rust.
FFprobe/FFmpeg are independent development verifiers used only after a take
has closed. The supervisor does not reset or restart a failed tracker to hide
its history.

The intended campaign is four calibrated 320×240 cameras at nominal 100 FPS,
continuous tracking until 09:00 Europe/Berlin on 2026-10-27, with private native
FT publishers on 21971–21974. Original configs and inputs are read only. Config
copies disable ordinary UDP/serial/automatic recording and select the private
test publishers, leaving geometry and numerical settings unchanged.

## Continuous tracking and storage

One native process keeps all four cameras open. A new `record_start` replaces
the previous Recorder without resetting the engine or source. The asynchronous
native protocol exposes `recording_finalizations` and writes a take-specific
`completion.json` after numeric flush/fsync and encoder drain. The supervisor
waits for that actual completion marker before reading or pruning a closed take;
a new recording directory alone is not completion evidence. Missing, pending or
failed markers are explicit audit failures. Each closed take must continue from
the preceding take's last numeric frame ID, with a strictly increasing source
clock. The default source recording
uses 60-second, full-frame MJPEG takes. The first complete source clip in each
local wall-clock hour is kept, along with the final source clip when it is
smaller than 1 GiB and more than 2 GiB remains free. Other source clips are removed
only after full independent decode, frame-count/identity checks, SHA256 recording
and full-frame first/last PNG extraction.

An hourly 3-second uncompressed raw+debug probe additionally exercises both
video writers. Its successfully verified movie files are pruned after the same
evidence is saved. All numeric, quality and effective configuration files stay
on disk for every take. No cropped ball movie is substituted for the source
image. Partial or unverified videos are retained explicitly and never count as
completed recordings.

Finishing/opening a take can still delay acquisition. Numeric frame continuity
alone does not prove sensor completeness: the audit reports actual source-clock
intervals both within takes and across each rotation, warning on any interval
above 50 ms. The live-camera preflight must measure those
intervals before the long run is accepted. The recorded-video functional smoke
cannot establish camera acquisition behavior.

The supervisor projects remaining numerical storage from actual file growth,
with a conservative 1 KiB per processed-frame floor and 15% margin. It protects
2 GiB of additional space. Low projected capacity or verification backlog makes
the next segment explicitly data-only; it never silently discards numeric rows.
Below 1 GiB actual free space, it closes the run gracefully and records the reason.
The Rust live-video workers independently retain their 512 MiB reserve. These are
best-effort safeguards with concurrent writers/quotas, not an infinite recording
capacity claim.

## Start and confirm

Use a fresh directory. Run with host display/camera/process access; a restricted
PID namespace cannot truthfully confirm the background host process. The native
application is launched before the supervisor lowers its own scheduling priority,
so its priority is not lowered with the independent analysis work.

```sh
python3 scripts/overnight_soak.py start \
  --binary dist/hejjetrac-linux-x86_64/hejjetrac \
  --configs VR1.txt VR2.txt VR3.txt VR4.txt \
  --output runs/overnight-20261027 \
  --until 2026-10-27T09:00:00+01:00

python3 scripts/overnight_soak.py status \
  --output runs/overnight-20261027 --summary
```

`start` only acknowledges the detached supervisor. Poll until
`healthy_running=true` and recording directories/frames advance before claiming
that camera recording started. The default enables one healthy and one
undrained subscriber per rig, plus incomplete greetings, for sustained transport
load. Exact accepted-row bytes are checked independently for each closed take.
Subscriber packet retention is bounded at 100,000 packets per rig.

The process-scoped `systemd-inhibit` sleep lock is acquired and read back before
capture; it is released when the run closes. A failed/unconfirmed inhibitor is
an explicit warning. Global power settings are not changed. The computer,
display session and Codex host must remain available for external hourly checks.
For this GNOME desktop, the detached main campaign additionally wraps the
foreground `run` command with `gnome-session-inhibit --inhibit idle:suspend`.
That process-scoped wrapper keeps the desktop available without changing global
idle settings. It must wrap `run`, because `start` exits after launching its
child. The wrapper PID and the supervisor's `runner.pid` are distinct; health
checks use the supervisor and application identities from `status.json`.

## Hourly inspection

```sh
python3 scripts/overnight_soak.py status \
  --output runs/overnight-20261027 --summary --write-check
```

This reads actual PID start ticks and heartbeat age, not just a cached healthy
label. `status.json` is atomic, `telemetry.jsonl` retains compact ten-second
samples, and `hourly-checks/` retains explicit monitor observations. Reports
contain native-process CPU percent of one core, RSS, frame rates, engine/worker
cost, media queue state, storage projection and independent verification results.
The supervisor/verifier workload is part of this soak; it is not the isolated
one-versus-four-camera CPU benchmark.

Inspect `snapshots/<hour>/` for current approximate visual diagnostics and
`verifications/` for complete quality distributions and relative-motion velocity
statistics. Only `tracked` rows with real positive delta-time contribute to
velocity measurements. Rejections and absolute relocalizations remain missing
motion. Low acceptance, long unaccepted intervals, excessive angular velocity,
incomplete media, wire mismatches, staleness or unexpected source/config changes
are reported. These are operational review signals, not physical ground truth.

To stop only this test gracefully:

```sh
python3 scripts/overnight_soak.py stop --output runs/overnight-20261027
```

Poll for a terminal report. The supervisor closes its own native sessions,
finalizes active recordings, completes bounded independent media checks and
releases its sleep inhibitor. It does not terminate unrelated applications.

## Current execution evidence

The development functional smoke under
`runs/overnight-fixture-preflight-20261027/` processed the actual private VR3
video through the earlier tested portable binary. Its six recording takes retain
1,420 consecutive numeric/quality rows (source IDs 21–1440), exact 10 ms rotation
boundaries and exact accepted FT bytes. Raw+debug counts and pruning evidence
pass; the sleep inhibitor was confirmed and released. A second detached smoke
verifies actual PID/start-time monitoring, graceful stop and safe final-clip
retention in `runs/overnight-detached-preflight-20261027/`.

These are recorded-video supervisor checks. They do not claim the final live
overnight campaign has already run. The main run is created only by its actual
launch, and its reports remain the authority for elapsed duration and health.

The initial four-camera live preflight in
`runs/overnight-live-preflight-20261027/` retained 34,235 consecutive processed
rows and exact accepted wire bytes, but **failed acquisition continuity**.
Synchronous recorder finalization produced source-clock gaps up to 2.368 seconds;
queued V4L frames hid those gaps until several rows after the file boundary.
That failed evidence remains unchanged. The final overnight launch requires a
repeat with asynchronous finalization and acceptable active capture continuity;
the earlier file-only functional success does not override this failure.

The asynchronous repeat in
`runs/overnight-live-async-preflight-20261027/` retained 35,392 consecutive rows,
100% accepted quality decisions and exact accepted wire bytes. All 24 completion
markers were complete, the final audit backlog was zero and the native process
exited normally. Across the whole capture, including recording boundaries, the
largest source-clock interval was 39.983 ms, with no interval above 50 ms.
This removes the earlier multi-second finalization stall in this preflight.
Recording completeness still failed: one source-MJPEG movie and two debug movies
stopped with explicit partial prefixes under an eight-frame encoder queue. The
partial files remain available. A larger bounded queue requires another live
preflight before the main campaign can be accepted.

That repeat passed in
`runs/overnight-live-buffered-preflight-20261027/` with the 32-frame queue and
unchanged 32 MiB per-stream byte budget. It retained 35,374 consecutive numeric
and quality rows at 100.830–100.898 rows per source-clock second per rig. Every
requested movie completed: 20 source-MJPEG movies, four raw movies and four
debug movies, each checked by an independent complete decode and frame count.
All 24 finalization markers completed, all accepted wire bytes matched and the
native process exited with status zero. The largest whole-capture source-clock
interval was 31.994 ms; no interval exceeded 50 ms. These are measured operational
preflight gates, not a sensor-level zero-loss or live physical-ground-truth claim.

## Completed main campaign

The actual main run subsequently finished at **09:00:16 Europe/Berlin** after
**6 h 11 min 48.471 s**, with native exit **0** and stop reason `deadline`.
All 365 groups and 1,460 numeric completion markers were audited. The run retains
**8,998,214** aligned processed numeric/quality rows, continuous processed IDs,
exact accepted FT bytes, and all effective configuration files. All owned
processes exited and the temporary systemd/GNOME inhibitors were released.

The outcome is **completed with warnings**: all 1,424 requested source MJPEG
movies complete, but four debug movies are retained partials. Data-only groups
254 and 311 preserve 52,514 rows while omitting approximately 65 seconds of
source video each. Normal recording resumes and verification catches up afterward;
all four rigs have verified retained clips for hours 02–08 and the final take.
Those fallbacks must not be reported as continuous all-night video.

The heaviest short debug probe briefly reduces capture to 85.6–93.3 rows/s;
normal whole-run source-clock rates remain 100.910–100.915. Eleven capture-clock
gaps exceed 50 ms, maximum 80 ms. Later approximately one-second control
acknowledgments stretch requested three-second probes to about eight seconds,
exceeding the short-probe storage estimate and triggering the two capacity
fallbacks. Control-independent duration and conservative volume projection
remain follow-up work. No tracker reset or hidden data reconstruction is used.

The final verification backlog is zero, final sampled free disk is 4.20 GiB and
all partial media remain. The completed hourly monitor was removed; the separate
October 29 performance follow-up stays active.
[Exact terminal analysis](benchmarks/linux-overnight-20261027.json) and
`runs/overnight-20261027/final-completion-audit.json` retain the complete scope.
