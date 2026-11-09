# T1 native fixture smoke (local, noisy)

Main `e8ad219` and branch `16208d6` (#607) ran the two-update net-8 fixture
in 3 A-B-B-A blocks per case on a 4-vCPU cloud guest. Every sample's run
tree matched its case's reference sample up to accounted timing fields, pins
and build provenance. That held for the fresh case, the warm case and a cold
resume of each. No timing verdict is conclusive. An A/A run of main against
itself showed a 15–29% run-to-run spread in wall time on this machine. This is a
smoke test of the driver and the counters, not a speed claim, and it used the
lower-memory build profile.

The [receipt](t1_native_fixture_smoke_20261109.json) was written by
`tools/native_fixture_bench/bench.py receipt`. It holds every sample, the
verdict rows, the pins and the SHA-256 of the raw `bench.json`,
`samples.jsonl` and `comparison.json`. The raw outputs are kept outside git.

## Setup

- **Host**: Claude Code cloud container, Intel Xeon @ 2.30 GHz, 4 vCPU KVM
  guest on a shared host, 15.7 GiB RAM, Linux 6.18, Rust 1.94.1 (LLVM 21.1.8).
- **Build, identical for both arms**: `CARGO_PROFILE_RELEASE_LTO=false
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_INCREMENTAL=0 cargo build
  --release --locked -j 4`. The checked-in profile's lib test build is
  OOM-killed at 15 GiB.
- **Arms**:
  - main `e8ad219`, sha256 `01ce8387…`, 80.8 MB;
  - branch `16208d6`, sha256 `091e6eff…`, 73.8 MB.

  The binaries differ in size only in `.rodata`, 26.4 vs 19.8 MiB, where main
  holds more duplicate copies of the same embedded source text. `.text` is
  33.1 MiB in both.
- **Fixture**: sha256 `28c367e6…`, generated at `16208d6` from a clean tree.
  - Seed 2026091601. Each case runs 2 updates of 2 games, Affinity vs Terror.
  - The learner is a synthetic net-8 FlatV4 source. There is 1 collection
    worker and 1 preparation worker.
  - The fresh case starts at Adam step 0. The warm case resumes the
    checkpoint left by 2 in-process warm-up updates (sha256 `7e01399e…`).
- **Order**: per case, one untimed warm-up per arm, then 3 A-B-B-A blocks, then
  a cold resume per arm. Each sample is a new process, and the page cache is
  not dropped.

## Correctness

All 32 A/B samples and all 28 A/A samples completed. Each run tree has 21
files.

- Between samples of the same build, 12 files are byte-identical. The other 9
  differ only in 14 timing fields and 8 pins of those files.
- Across builds, `run.json` also differs in its 3 provenance fields.

The checkpoint bytes and each iteration's output identity matched at both
boundaries in every sample. That includes the cold resumes, which stop after
one iteration and finish in a new process.

## Timing

Each figure is the median of 6 timed samples per arm. It is the whole
2-update run: process wall time, or a phase summed over both iterations.

| Case | Metric | main | branch | Ratio | Spread | Block ratios |
|---|---|---:|---:|---:|---:|---|
| fresh | wall, s | 2.596 | 2.549 | 0.982 | 0.197 | 0.912, 1.004, 1.050 |
| fresh | peak RSS, MiB | 465.4 | 465.9 | 1.001 | 0.068 | 1.019, 0.997, 1.014 |
| fresh | collection, s | 1.205 | 1.148 | 0.953 | 0.234 | 0.912, 0.966, 1.021 |
| fresh | update, s | 1.029 | 1.062 | 1.033 | 0.205 | 0.906, 1.042, 1.098 |
| warm | wall, s | 2.609 | 2.580 | 0.989 | 0.195 | 1.084, 0.880, 0.956 |
| warm | peak RSS, MiB | 518.2 | 517.7 | 0.999 | 0.016 | 0.991, 0.999, 0.999 |
| warm | collection, s | 1.107 | 1.110 | 1.003 | 0.284 | 1.136, 0.832, 0.928 |
| warm | update, s | 1.125 | 1.096 | 0.974 | 0.143 | 1.034, 0.921, 0.973 |

Every verdict in both runs is inconclusive. In the A/A run, main against
itself, median wall time was:

- fresh: 2.362 vs 2.327 s, ratio 0.985, spread 0.285;
- warm: 2.764 vs 2.631 s, ratio 0.952, spread 0.154.

## Loaded-source counters

These are the branch's counts per 2-update run. They were identical in every
sample and in both cases.

| Role | Lookups | Hits | Misses, kept | State copied | Policy copied | Scoped time, fresh |
|---|---:|---:|---:|---:|---:|---:|
| run initial | 1 | 0 | 1 | 14.8 MiB | 4.0 MiB | 77 ms |
| collection learner | 2 | 2 | 0 | 29.6 MiB | 8.0 MiB | 20 ms |
| opponent | 4 | 4 | 0 | 59.1 MiB | 16.0 MiB | 112 ms |
| update learner | 2 | 2 | 0 | 29.6 MiB | 8.0 MiB | 19 ms |
| update readback | 2 | 0 | 2 | 29.6 MiB | 8.0 MiB | 151 ms |
| update validation | 2 | 2 | 0 | 29.6 MiB | 8.0 MiB | 64 ms |
| **total** | 13 | 10 | 3 | 192.1 MiB | 52.0 MiB | 447 ms |

The fixed-opponent and unattributed roles were 0.

A state copy is 15,497,944 bytes: the parameters, the transposed weights and
both Adam moments. A policy copy is 4,194,368 bytes, its embedding table.

Each update makes 5 cache hits, and each one copies the full state. Only the
update learner trains. The collection learner, the two opponents and update
validation need just a policy or an identity.

A role's scoped time includes the file reread and hash check as well as the
copy, and this run does not separate them. The scoped total was 0.45 s of the
2.55 s fresh wall time (warm: 445 ms).

## Still needed

The checked-in profile (thin LTO, `codegen-units = 1`) needs a machine with
at least 32 GB of RAM. That comparison is planned on the requester's machine,
covering both cases, an A/A run and the cold resume.
