# W5 scratch reaper completion cursor

mtg-kernel#601 (T7) milestone 2. The reaper thread in the in-box driver now ticks a completion cursor
(`chains.Reaper`) instead of re-walking the segment. Each tick probes for the next iteration's `complete.json` and
reaps only the iterations that new completions released. `persist` keeps the full scan (`chains.reap`). Measured
with `tools/w5/tests/scratch_bench.py`, which ticks each tree's own reaper, against main.

In a 256-update segment (C's default), the reaper's file-system calls fall from 2.82 million to 22,443, and an
idle tick makes one `stat` call however long the segment has run. Its ticks took 8.66 s per segment on main and
0.108 s with the cursor, every sample faster (A-B-B-A). At the driver's 0.5 s cadence, the reaper's share of one
core falls from 3.2% to under 0.01% at 256 updates, and from 11.9% to under 0.01% at 1,024. Both trees
delete the same files at every tick, and the persisted logical member bytes match between them.
The comparison decompresses gzip members; it does not establish identical compressed file bytes.

**Decision.** The cursor removes the reaper's growth with segment length, which milestone 1 set as its goal. A
durable updates/hour claim needs T6's guarded comparison on a node (handoff below). At 256 updates per segment the
saving is about 3% of one core in the driver, so a box with spare cores may show no difference. With the reaper's
growth gone, persistence is the larger cost per segment: at 4,096 updates `persist` took 23 s here (40,968
fsyncs), against 1.5 s for all of the cursor's ticks. Milestone 3 still waits for the node's fsync latency
(milestone 1's handoff).

## Reaper

Small profile, as in milestone 1: two measured ticks per update (one that reaps the previous iteration, one idle).
Main is `0b8c0fa`, the cursor `82429c9`. Calls are exact and were equal in both count runs of each tree; times are
the mean (min to max) of 4 samples, 2 at 1,024 and 1 at 4,096.

| Segment length | Tree | Calls per segment | Calls in last idle tick | All ticks, per segment | Mean idle tick | Share of one core |
|---:|---|---:|---:|---:|---:|---:|
| 64 | main | 176,912 | 2,713 | 0.532 s (0.518-0.547) | 4.0 ms | 0.80% |
| 64 | cursor | 5,547 | 1 | 0.026 s (0.024-0.028) | 21 µs | <0.01% |
| 256 | main | 2,821,328 | 10,969 | 8.66 s (8.62-8.72) | 16.7 ms | 3.2% |
| 256 | cursor | 22,443 | 1 | 0.108 s (0.101-0.117) | 23 µs | <0.01% |
| 1,024 | main | not counted | | 138.1 s (137.5-138.8) | 67.3 ms | 11.9% |
| 1,024 | cursor | not counted | | 0.401 s (0.395-0.406) | 21 µs | <0.01% |
| 4,096 | cursor | 360,363 | 1 | 1.52 s | 19 µs | <0.01% |

- An idle tick on main makes 6 + 43h calls with h iterations already reaped (fitted b = 43.00, as in milestone 1).
  With the cursor it makes one `stat` of the next iteration's `complete.json` (fitted a = 1, b = 0 at every length
  counted).
- A tick after a completion also reaps the iteration it released, once: in this profile 8 directory scans, 29
  entries, 22 `stat`, 17 `lstat` and 11 unlinks. So the cursor's work per segment grows linearly with its length,
  and none of it goes to iterations already reaped (0 calls, against 99.2% of main's at 256).
- Both trees deleted the same files: 693 at 64 and 2,805 at 256. The bench checks each tick's deletions against the
  bulk the fixture wrote for the iteration it released, and stops otherwise.
- The core share is the mean idle tick over (0.5 s + mean idle tick), as in milestone 1. This VM ran main's reaper
  about 25% slower than on Nov 8 (4.29 s of idle ticks at 256 here, 3.44 s then, same code), so compare trees
  within one run only.

## Correctness

`tools/w5/tests/test_w5_scratch.py` adds 7 tests (6 portable, 1 Linux-only):

- **Same deletions as the full scan.** Twelve seeded schedules of starts, completions (several between ticks at
  times) and ticks. One tree is reaped by `chains.reap` on every tick and its twin by one `Reaper`; the counts and
  the remaining files are equal after every tick, and a final full scan finds nothing left.
- **Retained files.** The newest completed iteration (the next update's source) and the in-flight one keep their
  trajectories, checkpoint and S container. Reaped iterations keep `complete.json`, `update.json` and
  `collection.json`.
- **Restart.** A new process ticks a new `Reaper` on a segment an earlier process left part-reaped, with a
  trajectory left in an iteration it had reaped. It deletes exactly what a full scan of a copy deletes.
- **I/O failure.** An injected `PermissionError` on one unlink reaches the caller, and the cursor stays before that
  iteration; the next tick deletes the rest of it.
- **A gap in the prefix** (which the trainer refuses) stops the cursor: it reaps less than the full scan, never
  more.
- **`run_reaped`** ticks one `Reaper` beside a stand-in trainer and returns the files deleted.
- **Constant ticks** (Linux, through the bench's counters): no tick touches a reaped iteration, every idle tick
  makes the same calls, and so does every productive tick after the first.

Each of four deliberate faults in `Reaper` fails at least one test: reaping the newest iteration, counting an
iteration as reaped before its deletion returns, probing one completion per tick, and rescanning from iteration 0
on every tick.

What does not change: `persist` (its own pass is still the full scan), `scratch_settle`, the commit and
housekeeping lines, and recovery. Each `run_reaped` (a new segment, a driver start, a resumed trainer) starts a new
cursor at iteration 0, so the first tick rescans the whole completed prefix. A reaper error still ends the reaper
thread for the rest of the segment, as on main, and `persist`'s full pass raises it again.

## Paired timings (main = A, cursor = B)

Sums of all ticks per sample, in round order (A-B-B-A for each length):

| | A (ms) | B (ms) | B/A of means | Verdict |
|---|---|---|---:|---|
| Reaper ticks, 64 | 517.6, 543.3, 520.8, 546.6 | 24.4, 25.6, 26.5, 27.8 | 0.0490 | cursor faster |
| `persist`, 64 | 650.3, 639.8, 605.7, 631.0 | 595.6, 470.3, 699.8, 591.1 | 0.9327 | inconclusive |
| Reaper ticks, 256 | 8,678.1, 8,617.6, 8,720.7, 8,622.4 | 116.7, 106.2, 110.3, 100.6 | 0.0125 | cursor faster |
| `persist`, 256 | 2,180.0, 1,487.1, 2,238.2, 2,115.9 | 2,291.9, 2,457.2, 1,969.4, 1,751.6 | 1.0559 | inconclusive |
| Reaper ticks, 1,024 | 137,472.8, 138,779.6 | 406.4, 395.1 | 0.0029 | cursor faster |
| `persist`, 1,024 | 5,330.6, 6,000.0 | 6,025.4, 6,428.2 | 1.0991 | inconclusive |

`persist` is the same code in both trees, so its rows are an A/A spread. Its persisted copies have one logical
digest per segment length across all samples of both trees.

## Maintainer handoff (pending)

A durable updates/hour claim uses T6's code-only comparison (#600, #610). On one node, with Stage 0 inputs whose C
arm has a `scratch_root`, record C's box throughput (`chains.py throughput --arm C --updates 512`: two 256-update
segments, the fewest it accepts) with the W5 tools of main `0b8c0fa` and of this branch, in two A-B-B-A series
(main, cursor, cursor, main, twice). Then compare the records:

```bash
python3 tools/w5/chains.py throughput-compare --baseline main-1.json main-2.json main-3.json main-4.json \
  --candidate cursor-1.json cursor-2.json cursor-3.json cursor-4.json --out t7-cursor-compare.json
```

The comparison checks that only the code differs. It calls a difference only when the two sides' ranges separate
by more than the larger range, and two series give a separation chance of 2/70. Expect an inconclusive report
unless the boxes' cores are saturated.

## Method and pins

- Bench and B: `82429c9230008f07bf52df4017f3a093d47eaf13` (the branch commit with the cursor; clean tree). A: main
  `0b8c0fa814d376efab56a227e62e8bedc00603ce`. Each receipt pins its trees' `chains.py` and `retention.py` hashes.
- Host: Intel Xeon @ 2.10 GHz, 4 vCPU KVM guest, 15.7 GiB, Linux 6.18.44, Python 3.13.18. Scratch was tmpfs
  (`/dev/shm`), the work directory ext4 on a virtio disk (`/dev/vda`). No Rust build is involved. Seed 601.
- Each sample is a fresh worker process that builds its fixture untimed, then runs the ticks, `persist` and
  `compress_config`. Samples ran serially in balanced order; every sample is kept. Count runs use exact hooks and
  timing runs light timers only. Caches stay warm, as in milestone 1.
- Receipts (verbatim): `w5_scratch_cursor_20261109/main_cursor.json` (64 and 256, both trees, with counts),
  `scale_1024.json` (both trees, timings only) and `cursor_4096.json` (the cursor alone, with counts).
  `w5_scratch_cursor_20261109.json` is the manifest.

## Not covered

- The trainer writing beside the reaper: ticks are measured back to back in one process, so the core share comes
  from the 0.5 s cadence, not from observation.
- A real bind mount and node storage; times are this VM's.
- Durable updates/hour (the handoff above).
- The cgroup acceptance check (`chains_acceptance.py`): this host has cgroup v1. CI runs it.

## Reproduce

From the repository root, with a worktree of main as `MAIN` (any directory works as `--work-dir`):

```bash
python3 tools/w5/tests/scratch_bench.py --out main_cursor.json --sizes 64,256 --rounds 4 \
  --src MAIN/tools/w5 --candidate-src tools/w5 --scratch-dir /dev/shm --work-dir DIR --max-wall-seconds 2400
python3 tools/w5/tests/scratch_bench.py --out scale_1024.json --sizes 1024 --rounds 2 --no-counts \
  --src MAIN/tools/w5 --candidate-src tools/w5 --scratch-dir /dev/shm --work-dir DIR --max-wall-seconds 2400
python3 tools/w5/tests/scratch_bench.py --out cursor_4096.json --sizes 4096 --rounds 1 \
  --src tools/w5 --scratch-dir /dev/shm --work-dir DIR --max-wall-seconds 2400
```

The three runs took 125 s, 315 s and 158 s here.
