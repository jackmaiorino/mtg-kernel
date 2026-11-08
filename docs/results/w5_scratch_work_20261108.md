# W5 scratch reaper and persistence work

mtg-kernel#601 (T7) milestone 1. Measurement only: `tools/w5/chains.py` runs unchanged.
`tools/w5/tests/scratch_bench.py` calls its `reap`, `persist`, `compress_config` and `scratch_settle` on
temporary-directory segments, with the bind mount simulated, and counts their file-system calls exactly before
timing them in separate runs.

The reaper's work per tick grows linearly with the history it has already pruned, so its work per segment grows
with the square of the segment length. In a 256-update segment (C's default), 99.2% of its 2.82 million
file-system calls go to iterations whose bulk it had already deleted, or to listing them. On this host a tick costs
about 0.11 ms per already-reaped iteration. At the driver's 0.5 s cadence the reaper takes about 2.6% of one core
over a 256-update segment and 10% over a 1,024-update one. Persisting a 256-update segment makes 2,568 fsyncs
(about 10 per update) and took 1.3 to 2.3 s here, 39 to 55% of it in fsync.

**Decision.** Milestone 2's completion cursor is justified by the counts. Milestone 3 (grouped durability) waits
until the node's fsync latency is measured; the handoff command is below.

## Reaper

Main `c6329d8`, small profile, two measured ticks per update (one that deletes the previous iteration's bulk, one
idle). Calls and the history share are exact and were equal in both count runs of both trees; times are the mean
(min to max) of 4 samples, 2 at 1,024.

| Segment length | Calls per segment | Share on reaped history | Calls in last idle tick | Idle ticks, total | Mean idle tick | Share of one core |
|---:|---:|---:|---:|---:|---:|---:|
| 64 | 176,912 | 96.8% | 2,713 | 0.224 s (0.214-0.240) | 3.5 ms | 0.7% |
| 256 | 2,821,328 | 99.2% | 10,969 | 3.44 s (3.27-3.60) | 13.4 ms | 2.6% |
| 1,024 | not counted | | | 57.1 s (56.2-57.9) | 55.7 ms | 10.0% |

- An idle tick makes 6 + 43h calls with h iterations already reaped (fitted b = 43.00, every tick within 2 calls).
  Each reaped iteration costs 8 directory scans, 18 entries, 10 `stat` and 6 `lstat`, plus its entry in the
  iterations listing. `reap()` itself walks every completed iteration except the newest on each tick; Python
  3.13's `rglob` scans each of the iteration's 4 directories twice, so other Python versions differ in the constant,
  not the growth.
- Time follows the calls: 105 to 112 µs per already-reaped iteration per tick (fitted slopes). The last idle tick
  took 6.5 ms at 64, 26 ms at 256 and 106 ms at 1,024. The reaper's time is CPU (3.40 of 3.44 s at 256).
- The core share is the mean tick over (0.5 s + mean tick), since `REAP_EVERY` waits 0.5 s between ticks. It does
  not depend on how long an update takes. The slope extrapolates to about 31% at the 4,096-update maximum.
- The reaper runs in the in-box driver, whose CPU is part of the chain's ledger, so this time is charged to the
  box. Whether it displaces training depends on whether the box's cores are saturated; T6's metric will show it.
- With full-size C bytes (profile `c-plain`: ten 3.2 MB trajectories and a 21 MB checkpoint per update, the
  plain-JSON sizes in `tools/ex1/stagef.py`) the idle walk takes about as long (3.59 s against 3.44 s at 256).
  Unlinking 53 MB per update from tmpfs adds 0.9 s to the productive ticks.

## Persistence

| Segment | Members | fsyncs | Physical / logical bytes | `persist` | fsync | Process CPU |
|---|---:|---:|---:|---:|---:|---:|
| 64, small | 387 | 648 (388 file, 260 dir) | 0.91 / 1.50 MB | 0.57 s (0.46-0.65) | 0.21 s | 0.40 s |
| 256, small | 1,539 | 2,568 (1,540 file, 1,028 dir) | 3.42 / 5.74 MB | 2.13 s (1.73-2.30) | 0.84 s | 1.45 s |
| 1,024, small | 6,147 | 10,248 | 13.4 / 22.7 MB | 4.84 s (4.68-5.00) | 2.81 s | 2.58 s |
| 256, c-plain | 1,539 | 2,568 | 24.4 / 26.7 MB | 1.44 s (1.38-1.55) | 0.80 s | 0.79 s |

- Every persisted copy's logical members (gzip members decompressed, the marker excluded) equal the fixture's,
  with one digest per segment across all samples and both trees.
- Each iteration costs 6 file and 4 directory fsyncs, at 0.27 to 0.33 ms each on this host. Copying is small:
  `copy2` took 0.30 s and gzip 9 ms at 256 (3.5 MB in, compression level 1). In the c-plain run `copy2` took
  0.11 s including the 21 MB checkpoint. `rmtree` of the scratch tree took 15 ms. The index lock was held 0.4 to
  1.2 ms (simulated unmount, rename, parent fsync) and waited on for about 5 µs.
- The rest of `persist` is per-member path work. At 256 it made 7,172 directory scans, 16,671 `stat`, 6,677 opens
  and 2,566 `mkdir` calls, and its process CPU was mostly kernel time. Python's traced peak during `persist` was
  1.3 MB at 64 and 3.5 MB at 256. `compress_config` took 9 ms at 256.
- Persist time ranged over 12 to 52% of its mean between samples of one run, and it drifts between runs on this
  shared VM: the c-plain run copied more bytes yet ran faster than the small run minutes earlier. Per update it
  cost 4.7 to 9.0 ms (run means).
- `persist` runs serially after the trainer exits, so the box waits for it. chains.py estimates about half a second
  per C update. At that rate, 2.1 s is about 1.7% of a 256-update segment. With f ms per fsync, persistence costs
  about 1 s plus 2,568 f per segment: about 6 s at 2 ms or 27 s at 10 ms.

## Recovery paths (`scratch_settle`)

| Case | main `c6329d8` | #602 `87ae0de` |
|---|---|---|
| Nothing to do | 5 calls, no fsync, 0.1 ms | the same |
| Finish an interrupted swap | 15 calls, no fsync, 0.7 ms | 28 calls, 2 fsyncs (parent directory, commit line), 1.5 ms |
| Persist an ended segment late | 2,568 fsyncs, 2.24 s | 2,570 fsyncs (commit and housekeeping lines), 2.37 s |
| Reset an unfinished segment | 7,717 calls, no fsync, 18 ms | the same |

Every case left the expected state: persisted and unmounted, or reset with its scratch tree removed. #602 adds one
to two fsyncs per boundary, which is negligible next to persistence.

## A/A spread (main against #602)

`reap`, `persist`, `compress_config` and their helpers are identical in the two trees (compared by syntax tree), so
the paired runs measure this host's spread. Every verdict is inconclusive, so there is no difference to report.

| Sum per sample | B/A of means | Mean difference | Larger arm's range |
|---|---:|---:|---:|
| Reaper ticks, 64 | 0.993 | -3 ms | 81 ms |
| Reaper ticks, 256 | 0.994 | -43 ms | 669 ms |
| `persist`, 64 | 0.960 | -23 ms | 223 ms |
| `persist`, 256 | 0.851 | -318 ms | 940 ms |

## Next milestones

- **Milestone 2 (completion cursor): build it.** A cursor can remove the 99% of reaper calls spent on reaped
  history, so per-tick work stops depending on segment length. It still needs the authoritative rescan at process
  start, and it must keep the newest and in-flight iterations. It would save about 2.6% of one core at 256 (about
  3.4 s of driver CPU per segment at one tick per update) and about 10% at 1,024. The reaper's own spread here is
  at most 18% of its total.
- **Milestone 3 (grouped durability): not yet.** About 10 fsyncs per update matter only if the node's fsync latency
  is high. Run the bench with `--work-dir` on the node's root volume and compare its time per fsync with this
  host's 0.3 ms.
- **Milestone 4 (background compression): no case.** Gzip takes 9 ms per segment.
- A durable updates/hour claim for any candidate uses T6's metric (#600, #602).

## Method and pins

- Bench `a820e75f10a54e84a955e23dedf66c17589e99bb` (`scratch_bench.py` sha256 `46478509...3985`), clean tree.
  A = main `c6329d868ad32467b22a411018894a6eae65205c`. B = #602 head
  `87ae0de6a1593f2a24d06300940d7e172c3d3623`. Each receipt pins its trees' `chains.py` and `retention.py` hashes.
  Main has since moved to `23a614c32fa77e57ecf43e4e33ab2960c39acb96` with no change under `tools/w5`.
- Host: Intel Xeon @ 2.10 GHz, 4 vCPU KVM guest, 15.7 GiB, Linux 6.18.44, glibc 2.39, Python 3.13.18. Scratch was
  tmpfs (`/dev/shm`). The work directory was ext4 on a virtio disk (`/dev/vda`, `relatime,discard`). No Rust build
  is involved. Seed 601.
- Each sample is a fresh worker process that builds its fixture untimed, then runs the ticks, `persist` and
  `compress_config`. Samples run serially in balanced order, A-B-B-A for each length. Every sample is kept. Count
  runs use exact hooks; timing runs use light timers only (fsync, lock, copy, gzip, rmtree, unlink). Caches stay
  warm: tmpfs has no cold-read state, the fixture was just written, and `persist`'s own fsyncs push its writes to
  the device. Nothing was dropped or discarded as warm-up.
- Receipts (verbatim): `w5_scratch_work_20261108/main_ab.json` (64 and 256, both trees, counts and settle),
  `scale_1024.json` and `cplain_256.json`. `w5_scratch_work_20261108.json` is the manifest.

## Maintainer handoff (pending)

Milestone 3's decision needs one bounded run on a node, on the same volume as its W5 root, by a maintainer and at a
time when no chain on that volume is persisting:

```bash
python3 tools/w5/tests/scratch_bench.py --out FRESH_RECEIPT.json --sizes 256 --rounds 2 --no-counts \
  --scratch-dir /dev/shm --work-dir TEMP_DIR_ON_THE_W5_ROOT_VOLUME --max-wall-seconds 1800
```

It touches no plan, chain or mount: two samples of about 10 s each here, at most 13 MB live in tmpfs and in the work
directory per sample, and about 2,600 fsyncs per sample, removed afterwards. Compare the receipt's per-fsync time
with this host's 0.3 ms. A durable updates/hour claim for milestone 2 or 3 needs T6's guarded multi-segment run.

## Not covered

- A real bind mount, `umount` (a subprocess inside the index lock on a node) and `os.path.ismount`. The trainer's
  concurrency with the reaper is not simulated either. Ticks are measured back to back, so the CPU share comes from
  the 0.5 s cadence, not from observation.
- Node storage. Times are this VM's, and fsync latency is the unknown that decides milestone 3.
- Fault injection. No behavior changed in this milestone; the interruption, restart and I/O-failure tests in #601
  apply to milestones 2 and 3.
- The cgroup acceptance check (`chains_acceptance.py`) was skipped: this host has cgroup v1 (hybrid).

## Reproduce

From the repository root, with worktrees of the two trees (any directory works as `--work-dir`; use one on the
volume under test):

```bash
python3 tools/w5/tests/scratch_bench.py --out main_ab.json --sizes 64,256 --rounds 4 --settle \
  --src MAIN/tools/w5 --candidate-src PR602/tools/w5 --scratch-dir /dev/shm --work-dir DIR --max-wall-seconds 2400
python3 tools/w5/tests/scratch_bench.py --out scale_1024.json --sizes 1024 --rounds 2 --no-counts \
  --src MAIN/tools/w5 --scratch-dir /dev/shm --work-dir DIR --max-wall-seconds 2400
python3 tools/w5/tests/scratch_bench.py --out cplain_256.json --profile c-plain --sizes 256 --rounds 4 --no-counts \
  --src MAIN/tools/w5 --scratch-dir /dev/shm --work-dir DIR --max-wall-seconds 2400
```

The three runs took 173 s, 245 s and 125 s here. The c-plain run writes 13.6 GB per sample to tmpfs, but the
reaper deletes it as it goes, so at most about 213 MB is live at once.
