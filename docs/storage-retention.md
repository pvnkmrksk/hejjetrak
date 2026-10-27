# Local validation storage after cleanup — 2026-10-27

At the user's request, obsolete generated test movies, the previous package
backup and regenerable release dependency caches were removed after validation.
The original inputs under `/home/pkaushi/src/fictrac/VR_array/`, all `examples/`
fixtures, source/Git history, current portable bundle and current tar/deb
packages remain. The running portable app was not stopped or changed.

The cleanup freed **12.92 GiB**, increasing available space from **4.32 GiB**
to **17.24 GiB** at measurement time. It removed 206 obsolete generated movies
and keeps the four final short full-source camera clips from overnight segment
364, approximately 91.6 MiB total. Reports, configuration text, figures and
existing first/last PNG excerpts remain accessible.

Full numerical evidence was preserved losslessly rather than discarded:

| Archive under `runs/.archives/20261027-cleanup/` | Original contents | Compressed size |
|---|---|---:|
| `numerical-evidence.tar.zst` | All 5,397 generated DAT/CSV files, including every numeric and quality row | 2.94 GiB |
| `failed-video-evidence.tar.zst` | All 41 generated partial movies, including the four overnight diagnostic failures | 101.51 MiB |
| `large-logs.tar.zst` | 24 large logs/JSONL transcripts | 15.27 MiB |

Every archived payload was read back through full decompression and checked
against its original byte count and SHA-256 **before** its expanded file was
removed. Each archive contains a catalog with original relative paths, sizes
and hashes; an identical catalog is also beside the archive. The detailed
`cleanup-manifest.json` records removed movie hashes and retained clip paths.
[The committed cleanup summary](benchmarks/storage-cleanup-20261027.json) records
archive hashes and the before/after storage measurements.

Historical validation reports describe files present at their audit time.
They were not rewritten to suggest a different test result. After this later
cleanup, older numeric/log paths and partial-video paths must be restored
from these archives before scripts can read them. The four short representative
source clips remain in their original segment-364 paths; the other 28 formerly
retained overnight source clips were intentionally pruned.

## Restore evidence

From the repository root, restore into a separate directory to avoid accidental
overwrites. Archive member paths start with `runs/`:

```sh
mkdir -p /tmp/hejjetrac-restored
tar --zstd -xf runs/.archives/20261027-cleanup/numerical-evidence.tar.zst \
  -C /tmp/hejjetrac-restored
```

To restore just one file, supply its exact catalog path:

```sh
tar --zstd -xf runs/.archives/20261027-cleanup/numerical-evidence.tar.zst \
  -C /tmp/hejjetrac-restored \
  runs/final-tracking-queue8/vr3/all-frames.dat
```

Use the corresponding failure-media or log archive the same way. Restored files
can be checked against their catalog SHA-256 values. A full numerical restore
requires approximately 7.61 GiB; selective restoration avoids expanding
unneeded cases. The large local archives remain outside Git, like the original
validation runs. Preserve them separately when backing up the numerical evidence.
