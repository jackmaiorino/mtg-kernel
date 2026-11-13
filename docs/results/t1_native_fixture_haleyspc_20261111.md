# T1 native fixture benchmark (checked-in profile, Windows PC)

Main `40e278b` and branch `c369a77` (#607) ran the two-update net-8 fixture
natively on a 32 GB Windows PC, built with the checked-in release profile
(thin LTO, `codegen-units = 1`). Each case ran 3 A-B-B-A blocks under the host
reservation of logical CPUs 4-15, after one untimed warm-up per arm, and then a
cold resume per arm. Every sample's run tree matched its case's reference
sample up to accounted timing fields, pins and build provenance. That held for
the fresh case, the warm case and a cold resume of each. No timing verdict is
conclusive. The branch's median wall time is within 1% of main's, inside a
run-to-run spread of 8–10%, and an A/A run of main against itself showed a
spread of 9–11%. The branch adds measurement only, so this is the expected
result. It is not a speed claim.

The [receipt](t1_native_fixture_haleyspc_20261111.json) was written by
`tools/native_fixture_bench/bench.py receipt`. It holds every sample, the
verdict rows, the pins and the SHA-256 of each run's raw `bench.json`,
`samples.jsonl` and `comparison.json`. The raw outputs and the binaries are
kept on the PC, outside git.

## Setup

- **Host**: the requester's PC (HaleysPC), AMD Ryzen 9 5900XT (16 cores, 32
  logical CPUs), 32 GB RAM, Windows 11 Home 10.0.26200, Python 3.13.14.
- **Guard**: the host guard from #594. Each timed run held a reservation of
  logical CPUs 4-15 and ran under `host_slots_v1.py timed`. Before and after
  each timed run, no other timed reservation was held, and another lane's two
  claims were running on CPUs 16-31. Free commit was about 10 GiB at the start
  of each timed run. The builds, the filters, fixture generation and the
  counters run were untimed 4-core below-normal claims.
- **Build, identical for both arms**: the checked-in release profile with no
  overrides, `cargo +1.94.1 build --release --locked -j 4`, for
  `x86_64-pc-windows-msvc` with the VS Build Tools 2026 (18.3) linker.

  | Build | Wall | Peak commit |
  |---|---:|---:|
  | lib tests (`test --lib --no-run`) | 46.8 min | 17.1 GiB |
  | branch binary | 29.2 min | 14.3 GiB |
  | main binary | 29.5 min | 14.3 GiB |

  Peak commit is the largest commit charge of any one process in the build.
  The branch binary reused the lib test build's dependencies, so only
  `mtg-kernel` was compiled. The main binary was a full build.
- **Arms**:
  - main `40e278b`, sha256 `4de051a3…`, 57.53 MB;
  - branch `c369a77`, sha256 `c9b9e205…`, 57.55 MB.
- **Fixture**: sha256 `58577b40…`, generated at `c369a77` from a clean tree by
  the lib test executable (sha256 `570b27eb…`). It is the workload of the
  [Linux smoke](t1_native_fixture_smoke_20261109.md):
  - Seed 2026091601. Each case runs 2 updates of 2 games, Affinity vs Terror.
  - The learner is a synthetic net-8 FlatV4 source. There is 1 collection
    worker and 1 preparation worker.
  - The fresh case starts at Adam step 0. The warm case resumes the checkpoint
    left by 2 in-process warm-up updates (sha256 `be6ced8d…`).
- **Order**: per case, one untimed warm-up per arm, then 3 A-B-B-A blocks,
  then (A/B run only) a cold resume per arm. Each sample is a new process in
  its own job object, and the page cache is not dropped. Both timed runs used
  `--no-cache-diagnostics`.

## Correctness

The checked-in-profile lib test executable passed every filter:

| Filter | Passed | Failed | Ignored |
|---|---:|---:|---:|
| `loaded_source_cache_v1::tests` | 9 | 0 | 0 |
| `state_digest_memo_v1::tests` | 4 | 0 | 0 |
| `phase1_preparation_real_updates_preserve_all_state_bits_and_group_order` | 1 | 0 | 0 |
| `a_crash_after_the_checkpoint_resumes_in_a_new_attempt_with_the_same_states` | 1 | 0 | 0 |
| `loaded_source_profile_v1::tests` | 1 | 0 | 0 |
| `cache_diagnostics_v1::tests` | 1 | 0 | 0 |
| `fixture_bench_v1` | 2 | 0 | 1 |

The ignored test is the fixture generator, `write_native_fixture_v1`, which
runs only when named. The driver's 15 unit tests also passed on the PC, with
none skipped.

All 32 A/B samples and all 28 A/A samples completed both iterations durably.
Each run tree has 21 files.

- Between samples of the same build, 12 files are byte-identical. The other 9
  differ only in 14 timing fields and 8 pins of those files.
- Across builds, `run.json` also differs in its 3 provenance fields.

The checkpoint bytes and each iteration's output identity matched at both
boundaries in every sample. That includes the cold resumes, which stop after
one iteration and finish in a new process. The A/A, A/B and counters runs
recorded the same boundary pins.

## Timing

Each figure is the median of 6 timed samples per arm. It is the whole 2-update
run: process wall time, the job's user CPU time, the process's peak working
set, or a phase summed over both iterations.

| Case | Metric | main | branch | Ratio | Spread | Block ratios |
|---|---|---:|---:|---:|---:|---|
| fresh | wall, s | 1.999 | 1.982 | 0.992 | 0.101 | 0.998, 1.029, 0.973 |
| fresh | user CPU, s | 1.242 | 1.180 | 0.950 | 0.201 | 0.981, 1.000, 0.890 |
| fresh | peak working set, MiB | 475.2 | 479.0 | 1.008 | 0.024 | 1.010, 1.011, 0.993 |
| fresh | collection, s | 0.883 | 0.883 | 1.000 | 0.132 | 0.990, 1.046, 0.983 |
| fresh | update, s | 0.718 | 0.718 | 0.999 | 0.093 | 1.003, 1.020, 0.956 |
| warm | wall, s | 2.133 | 2.148 | 1.007 | 0.078 | 0.991, 1.030, 0.992 |
| warm | user CPU, s | 1.352 | 1.281 | 0.948 | 0.231 | 1.045, 0.966, 0.983 |
| warm | peak working set, MiB | 531.0 | 529.9 | 0.998 | 0.013 | 0.995, 0.997, 1.002 |
| warm | collection, s | 0.913 | 0.937 | 1.027 | 0.067 | 1.020, 1.014, 0.996 |
| warm | update, s | 0.782 | 0.777 | 0.994 | 0.156 | 0.974, 1.064, 0.980 |

Every verdict in both runs is inconclusive. Windows reports CPU time in whole
15.6 ms clock ticks, so every user and system time here is a multiple of
1/64 s. In the A/A run, main against itself, median wall time was:

- fresh: 1.991 vs 2.002 s, ratio 1.006, spread 0.094;
- warm: 2.202 vs 2.134 s, ratio 0.969, spread 0.106.

The first run of each binary was its warm-up: main's in the A/A run and the
branch's in the A/B run. Each took about 8.2 s instead of about 2 s. The extra
6 s was in the collection phase, and each used about 36 s of user CPU time.
Warm-ups are excluded from the comparison, and the cause was not
investigated.

## Loaded-source counters

A separate untimed run with diagnostics on collected these, with one A-B-B-A
block per case in a 4-core claim on logical CPUs 12-15. The branch's counts per
2-update run were identical in every sample and in both cases, and they equal
the Linux smoke's counts. Main writes no diagnostics.

| Role | Lookups | Hits | Misses, kept | State copied | Policy copied | Scoped time, fresh | Scoped time, warm |
|---|---:|---:|---:|---:|---:|---:|---:|
| run initial | 1 | 0 | 1 | 14.8 MiB | 4.0 MiB | 54 ms | 86 ms |
| collection learner | 2 | 2 | 0 | 29.6 MiB | 8.0 MiB | 12 ms | 18 ms |
| opponent | 4 | 4 | 0 | 59.1 MiB | 16.0 MiB | 74 ms | 84 ms |
| update learner | 2 | 2 | 0 | 29.6 MiB | 8.0 MiB | 13 ms | 17 ms |
| update readback | 2 | 0 | 2 | 29.6 MiB | 8.0 MiB | 98 ms | 101 ms |
| update validation | 2 | 2 | 0 | 29.6 MiB | 8.0 MiB | 42 ms | 40 ms |
| **total** | 13 | 10 | 3 | 192.1 MiB | 52.0 MiB | 293 ms | 345 ms |

The fixed-opponent and unattributed roles were 0. A role's scoped time
includes the file reread and hash check as well as the copy. The scoped total
was 0.29 s of the branch's 1.99 s fresh wall time in this run (warm: 0.34 of
2.30 s).

This run's label repeats the timed runs' host text, "timed on logical CPUs
4-15 under the host reservation". That text is wrong for this run, which was
the untimed claim described above, as the 4 affinity CPUs in its host record
show. Its timings come from a single block and give no verdict.

## Still open

This milestone counts the copies and does not remove them. Each update still
makes 5 cache hits that copy the full state, although only the update learner
trains. The policy/train-state split and check-only pin path (milestone 2) and
the committed-iteration handoff (milestone 3) remain open in #595.
