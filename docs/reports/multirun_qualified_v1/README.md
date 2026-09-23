# Multirun qualification receipts (2026-09-23)

Non-evidence engineering receipts for `python/tools/multirun_launcher_v1.py`.
Nothing here is a scientific result; the runs exist to qualify the launcher.
Bulk Stores stay under `D:/multirun-qualified-v1-work/q1` (not committed).

## Inputs

- Executable: `mtg_kernel` release lib-test binary with
  `experimental-burn-net8-packed-cuda-v1`, SHA-256
  `ffbc5f976dc668e8d82daf92eaf9bf52f2078964d03dfe7e473fade3532f6235`,
  built from the Rust sources of commit `d7967147`, copied to a
  content-addressed path before use. The only later Rust change on the branch
  is a type alias in the ticket module's unit tests (a clippy fix); a rebuild
  at the PR head therefore has another hash and requalifies before use, as
  the guard requires.
- Launcher: SHA-256 `c58b9453...b0ed` (commit `826c5c02`), stamped in both the
  receipt and the manifest. Runtime data tree: `bf51eeb0...246b`.
- Workload (`workload.json`, identity `0daaf3d2...6635`): the
  `multirun_pilot_v1` science loop, 128-update records, 2 arms (`base`,
  `envrand` with environment randomization v2) x 4 seeds, topology 2x32,
  broker target 16.
- Inventory at qualification: Jack's PC eligible (24 logical CPUs, RTX 4070
  SUPER + RTX 3050, no competing trainers); HaleysPC unreachable (SSH
  timeout); RunPod account reachable but excluded (Linux pods cannot
  reproduce Windows goldens).

## Qualification (`compute-choice.json`, adaptive sweep, 12-update prefix)

| Allocation | Episodes/s | Projected, 8 runs | Mean CPU | Byte-identical |
| --- | --- | --- | --- | --- |
| 1@gpu0 (serial) | 7.00 | 107 min | 22% | golden + serial repeat |
| 1@gpu0 + 1@gpu1 | 12.56 | 57 min | 26% | 8/8 |
| 2@gpu0 + 1@gpu1 | 16.31 | 47 min | 26% | 8/8 |
| 2@gpu0 + 2@gpu1 (selected) | 18.57 | 39 min | 37% | 8/8 |

The sweep stopped because no further process fits in GPU memory (measured
footprint 2,950 MiB per process on GPU 0, 2,611 MiB on GPU 1, 512 MiB safety
margin), not because throughput saturated. `superseded/` holds the two earlier
sweeps: the first applied GPU 0's footprint to GPU 1, the second omitted the
2+2 shape. Both are kept as the record of why the sweep became adaptive.

## Launch (`experiment-manifest.json`, `runs/`)

All 8 runs complete at generation 128 on 2@gpu0 + 2@gpu1: wall 1,466 s
(24.4 min; the projection of 38.8 min was conservative because early updates
are the slowest), 65,536 episodes, 44.7 episodes/s. Every run's first 12
generations are byte-identical to its serial golden. The monitor flagged six
idle-capacity windows (runs waiting, CPU 30-41% busy): the allocation width,
set by GPU memory, limits throughput.

## Full-length verification (`verification/verification.json`)

`base-s0` and `envrand-s0` rerun alone on GPU 0 for all 128 updates: all 233
Store outputs of each are byte-identical to the concurrent launch's. Serial
wall 525 s and 536 s, so the 8 runs serially take about 70.7 min against 24.4
min concurrently (2.9x). One arm of 4 runs fits in one wave (about 12.5 min,
1.4x one serial run).

## Other receipts

- `raw-harness-refusal.log`: a raw 128-update harness run without a ticket is
  refused in 1 s, before any Store exists.
- `superseded/determinism-probe-*.json`: the first probe (earlier build
  `d4360426...`, seed 424242, 8 updates): serial repeat, GPU 1 and two
  concurrent GPU 0 copies all byte-identical on 23 outputs.
- `monitor/`: 5-second CPU and GPU samples for every leg and the launch.
- `throughput.png`: the qualification table as a chart.
