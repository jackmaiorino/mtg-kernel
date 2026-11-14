# T1 native fixture benchmark: Milestones 2 and 3 (checked-in profile, Windows PC)

Main `7e51023b3` and branch `af3e6d200` (#595 Milestones 2 and 3: policy/identity-only
cache serving, streaming check-only pin verification, and carried net-8 learner state
across committed iteration boundaries) ran the two-update, two-game `fresh`, `warm`,
and `pool2` native fixtures on a 128 GiB Windows 11 desktop (`DESKTOP-DJ1C40R`),
built identically with the checked-in release profile (`lto = "thin"`,
`codegen-units = 1`). Each case ran 3 A-B-B-A blocks on dedicated P-cores 0-7 after
one untimed warm-up per arm, followed (in the A/B run) by a cold resume per arm
(`--max-new-iterations 1` followed by a fresh continuation process). Every sample's
run tree matched its case's reference sample up to accounted timing fields, pins, and
build provenance across all 42 A/A samples, all 48 A/B samples (including all 6
cold-resume samples), and all 12 counter-collection samples.

In the A/A noise check (`main` against itself), every metric verdict across all three
cases was inconclusive (`wall_seconds` median ratios `1.004`, `0.996`, `1.002`). In
the A/B comparison (`main` vs `branch`), `branch` achieved conclusive improvements
across every case on end-to-end wall time, throughput (`updates_per_hour`), peak
working set (`max_rss_mib`), collection execution, update execution, update
validation, and update input reads:

- `fresh` (current-policy C control, Adam step 0): median wall `1.608` -> `1.481` s
  (`-7.9%`, ratio `0.921`), `updates_per_hour` `+8.6%` (`1.086`), peak working set
  `511.1` -> `490.3` MiB (`-20.8` MiB), `update_validation` `34.7` -> `9.5` ms
  (`-72.6%`);
- `warm` (current-policy C control, resumed from warmed checkpoint): median wall
  `1.680` -> `1.572` s (`-6.4%`, ratio `0.936`), `updates_per_hour` `+6.8%` (`1.068`),
  peak working set `534.1` -> `525.2` MiB (`-8.8` MiB), `update_validation`
  `35.8` -> `9.8` ms (`-72.6%`);
- `pool2` (2-member fixed opponent pool): median wall `2.036` -> `1.740` s (`-14.5%`,
  ratio `0.855`), `updates_per_hour` `+17.0%` (`1.170`), peak working set
  `573.7` -> `543.3` MiB (`-30.4` MiB), `update_validation` `35.3` -> `10.1` ms
  (`-71.4%`).

Loaded-source train-state copies (`kept_state_bytes + served_state_bytes`) dropped
from `192.14` MiB (`201,473,272` B) to `0` B on `fresh` and `warm`, and from
`251.26` MiB (`263,465,048` B) to `0` B on `pool2` (where `checkpoint.json` decodes
also dropped from `10` to `6`, `-40.0%`, because carrying the learner state outside
the 4-slot `loaded_source_cache_v1` stopped evicting cached pool members).

The [receipt](t1_native_fixture_desktop_dj1c40r_20261114.json) was written by
`tools/native_fixture_bench/bench.py receipt`. It records every sample row, verdict
row, boundary pin, counter snapshot, and the SHA-256 of each run's raw `bench.json`,
`samples.jsonl`, and `comparison.json`.

## Setup

- **Host**: `DESKTOP-DJ1C40R`, 13th Gen Intel Core i7-13700K (8 P-cores on logical
  CPUs 0-15 + 8 E-cores on logical CPUs 16-23, 24 logical CPUs total), 128 GiB RAM
  (`130,884.2` MiB), Windows 11 10.0.26300, Python 3.14.3.
- **Guard and concurrent claims disclosure**: all builds, test runs, fixture
  generation, and timed benchmark runs went through the `#594` host guard serialized
  by `cargo_slot.py`. Each benchmark run (`aa`, `ab`, and `counters`) acquired
  `host_reservation_v1.py --cores 0-7` and executed inside
  `host_slots_v1.py timed --cores 0-7` on dedicated P-cores 0-7 (`affinity_cpus = 8`).
  Before and after each run, no other timed reservation was held. Memory headroom and
  concurrent non-overlapping claims sampled inside each reservation window:
  - `aa`: available physical RAM `75.1` -> `74.0` GiB, available commit `31.2` ->
    `31.1` GiB; concurrent claims `pr689-verify` (lane `agy-0f78afa6`, claim ID
    `a414475adf0545aead355241b502ad1c`, running on E-cores 16-19) and `ci-win-5`
    (lane `ci-runners`, claim ID `f32f784af25043df83e38354a9a0aa10`, suspended on
    cores 14-19);
  - `ab`: available physical RAM `73.9` -> `75.1` GiB, available commit `31.0` ->
    `31.0` GiB; same concurrent claims `pr689-verify` (cores 16-19) and `ci-win-5`
    (suspended on cores 14-19);
  - `counters`: available physical RAM `46.8` -> `46.1` GiB, available commit `22.1`
    -> `21.0` GiB; concurrent claims `ci-win-5` (lane `ci-runners`, claim ID
    `04b832fbc2c94674a5b4fbd8a466c50c`, running on cores 10-15) and `pr689-final`
    (lane `agy-0f78afa6`, claim ID `acc45d535bb140c48bb00748b880567d`, running on
    E-cores 16-19);
  - untimed 4-core claims on E-cores 20-23 (`wt460-suite8`, `wt460-suite9`,
    `actor454-dev-eternal`) ran before or after the timed reservations.
  None overlapped P-cores 0-7. Builds, Rust test filters, and fixture generation ran
  as untimed 4-core below-normal claims on E-cores 20-23.
- **Build, identical for both arms**: checked-in release profile with no overrides
  (`lto = "thin"`, `codegen-units = 1`),
  `cargo build --release --locked -j 4 -p mtg-kernel --bin native_expanded_training_run_v1`,
  `rustc 1.94.1 (e408947bf 2026-03-25)` on `x86_64-pc-windows-msvc` (LLVM 21.1.8,
  MSVC linker):

  | Step | Cargo build time | Artifact size | SHA-256 prefix |
  |---|---:|---:|---|
  | `main` binary (`7e51023b3`, clean target dir) | 54.3 min | 64.20 MB | `a10ac747...` |
  | `branch` binary (`af3e6d200`, clean target dir) | 48.8 min | 64.28 MB | `820912aa...` |

- **Arms**:
  - `main` `7e51023b3d25e96ade4f993add02d8f688aad653`, `64,195,584` bytes,
    sha256 `a10ac747b715709555319eb50d281294514fbef7359864fb139a912e722b72bf`;
  - `branch` `af3e6d2004f3f7593a67419bdcbd7f384d52e485`, `64,281,088` bytes,
    sha256 `820912aa2ae1ec70b36b324112ea9112b96d01531c318a7aadbd0fc22fba92d1`.
- **Fixture**: sha256 `6137e0ba923b313cea9ff876752158dad4b9ee5eb445e7b254c4f97aac63e973`,
  generated at `af3e6d200` from a clean tree (`git_clean = "true"`) by the lib test
  executable (`45d048f9...`) with
  `--seed 2026091601 --updates 2 --games 2 --warmup-updates 2 --pool-members 2`:
  - Affinity vs Terror, synthetic net-8 FlatV4 learner source over shared import
    descriptor `83f35d5f...`, 1 collection worker, 1 preparation worker, default
    strict rereads (`VerifiedReadsV1::Strict`).
  - `fresh` (config `90c9a1f9...`): 2 updates from Adam step 0 against current policy
    (`ExpandedOpponentAssignmentV1::Current`), matching #607's `fresh` workload.
  - `warm` (config `1aaa8ff4...`): next 2 updates resumed from the warmed checkpoint
    `e66eb4b0...` (Adam step 2), matching #607's `warm` workload.
  - `pool2` (config `23842ffe...`): 2 updates from Adam step 0 playing fixed pool
    members `member-00` (`7f11e61c...`) and `member-01` (`ae405bd8...`) drawn by
    `mtg-kernel-pool-fixture-draw/v1` (`[member-00, member-01]` in both iterations).
- **Order**: for `aa` and `ab`, per case (`fresh`, `warm`, `pool2`), 1 untimed
  warm-up per arm, then 3 A-B-B-A blocks (6 timed samples per arm), then (`ab` only)
  1 cold resume per arm (`--max-new-iterations 1` followed by a new continuation
  process), with `--no-cache-diagnostics`. For `counters`, 1 A-B-B-A block (`main` vs
  `branch`, 2 samples per arm per case) with `MTG_KERNEL_CACHE_DIAGNOSTICS_V1`
  enabled.

## Correctness

`cargo fmt --all -- --check`, `git diff --check`,
`cargo clippy --locked -p mtg-kernel --lib --tests --bins -- -D warnings`, and
`tools/native_fixture_bench/tests` (16 passed, 0 skipped on Windows) all passed. All
17 Rust unit test filters passed (132 passed, 0 failed):

| Filter | Passed | Failed | Ignored |
|---|---:|---:|---:|
| `expanded_deck_training_v1::loaded_source_profile_v1::` | 1 | 0 | 0 |
| `expanded_deck_training_v1::verified_bytes_cache_v1::` | 13 | 0 | 0 |
| `expanded_deck_training_v1::loaded_source_cache_v1::` | 11 | 0 | 0 |
| `expanded_deck_training_v1::opponent_cache_diagnostics_v1::` | 5 | 0 | 0 |
| `expanded_deck_training_v1::pool_fixture_v1::` | 4 | 0 | 0 |
| `expanded_deck_training_v1::tests::opponent_cache` | 1 | 0 | 0 |
| `state_digest_memo_v1::tests` | 4 | 0 | 0 |
| `phase1_preparation_real_updates_preserve_all_state_bits_and_group_order` | 1 | 0 | 0 |
| `expanded_deck_training_v1::no_state_digest_v1::` | 15 | 0 | 0 |
| `expanded_deck_training_v1::fresh_initialization_source::` | 5 | 0 | 0 |
| `expanded_deck_training_v1::cache_diagnostics_v1::` | 1 | 0 | 0 |
| `expanded_deck_training_v1::phase1_parallel_collection::` | 10 | 0 | 1 |
| `expanded_deck_training_v1::s_learner_v1::` | 8 | 0 | 1 |
| `expanded_deck_training_v1::w_learner_v1::` | 8 | 0 | 0 |
| `native_expanded_training_run_v1::fixture_bench_v1::` | 3 | 0 | 1 |
| `native_expanded_training_run_v1::verified_pin_tests::` | 2 | 0 | 0 |
| `native_expanded_training_run_v1::tests::` | 40 | 0 | 2 |

All 42 A/A samples, all 48 A/B samples (including all 6 cold-resume samples), and all
12 `counters` samples completed both iterations durably with `parity_ok = true`. Each
completed run tree has 21 files:

- Between samples of the same build, 12 files are byte-identical; the other 9 differ
  only in 14 timing fields and 8 pins of those files.
- Across builds (`main` vs `branch`), 11 files are byte-identical; `run.json` also
  differs in its 3 build provenance fields (`git_commit`, `implementation_sha256`,
  and `tracked_tree_sha256`).

Checkpoint SHA-256, output-identity SHA-256, and state SHA-256 matched at every
iteration boundary across `aa`, `ab` (warm continuation and cold two-process resume),
and `counters`:

- `fresh`: iter 0 (`adam_step = 1`) checkpoint `744ffcef...`, output identity
  `a47b17e1...`, state `d41a3954...`; iter 1 (`adam_step = 2`) checkpoint
  `c91b8578...`, output identity `bb107355...`, state `22667393...`.
- `warm`: iter 0 (`adam_step = 3`) checkpoint `4d30ab51...`, output identity
  `940e98a9...`, state `05b67c1f...`; iter 1 (`adam_step = 4`) checkpoint
  `148e2623...`, output identity `f8e8c9d0...`, state `32b069a8...`.
- `pool2`: iter 0 (`adam_step = 1`) checkpoint `4a4e10d2...`, output identity
  `81e51db3...`, state `d41a3954...`; iter 1 (`adam_step = 2`) checkpoint
  `1b8e6cc5...`, output identity `32b2a933...`, state `e93cbaef...`.

## Timing

Each figure below is the median of 6 timed samples per arm (3 A-B-B-A blocks with
`--no-cache-diagnostics`) over the 2-update run:

| Case | Metric | main | branch | Ratio | Spread | Block ratios | Verdict |
|---|---|---:|---:|---:|---:|---|---|
| `fresh` | wall, s | 1.608 | 1.481 | 0.921 | 0.031 | 0.919, 0.915, 0.931 | improvement |
| `fresh` | updates/hour | 4477.0 | 4861.6 | 1.086 | 0.031 | 1.088, 1.093, 1.074 | improvement |
| `fresh` | user CPU, s | 1.172 | 1.031 | 0.880 | 0.182 | 0.986, 0.829, 0.852 | inconclusive |
| `fresh` | peak working set, MiB | 511.1 | 490.3 | 0.959 | 0.019 | 0.957, 0.961, 0.953 | improvement |
| `fresh` | collection exec, s | 0.788 | 0.732 | 0.929 | 0.028 | 0.934, 0.924, 0.941 | improvement |
| `fresh` | update exec, s | 0.514 | 0.470 | 0.913 | 0.034 | 0.915, 0.908, 0.920 | improvement |
| `fresh` | update validation, s | 0.0347 | 0.0095 | 0.274 | 0.114 | 0.268, 0.278, 0.279 | improvement |
| `fresh` | update input read, s | 0.102 | 0.077 | 0.754 | 0.047 | 0.742, 0.756, 0.742 | improvement |
| `fresh` | checkpoint I/O, s | 0.229 | 0.237 | 1.036 | 0.038 | 1.034, 1.024, 1.046 | inconclusive |
| `warm` | wall, s | 1.680 | 1.572 | 0.936 | 0.019 | 0.936, 0.929, 0.938 | improvement |
| `warm` | updates/hour | 4286.3 | 4579.0 | 1.068 | 0.020 | 1.069, 1.076, 1.066 | improvement |
| `warm` | user CPU, s | 1.188 | 1.086 | 0.914 | 0.171 | 0.986, 0.882, 0.940 | inconclusive |
| `warm` | peak working set, MiB | 534.1 | 525.2 | 0.983 | 0.006 | 0.984, 0.985, 0.978 | improvement |
| `warm` | collection exec, s | 0.803 | 0.762 | 0.949 | 0.034 | 0.944, 0.941, 0.958 | improvement |
| `warm` | update exec, s | 0.546 | 0.508 | 0.930 | 0.033 | 0.941, 0.920, 0.931 | improvement |
| `warm` | update validation, s | 0.0358 | 0.0098 | 0.274 | 0.073 | 0.280, 0.279, 0.270 | improvement |
| `warm` | update input read, s | 0.112 | 0.084 | 0.755 | 0.098 | 0.729, 0.760, 0.739 | improvement |
| `warm` | checkpoint I/O, s | 0.224 | 0.239 | 1.067 | 0.054 | 1.095, 1.047, 1.071 | regression |
| `pool2` | wall, s | 2.036 | 1.740 | 0.855 | 0.040 | 0.848, 0.848, 0.856 | improvement |
| `pool2` | updates/hour | 3537.0 | 4139.0 | 1.170 | 0.039 | 1.179, 1.178, 1.169 | improvement |
| `pool2` | user CPU, s | 1.508 | 1.242 | 0.824 | 0.176 | 0.788, 0.770, 0.849 | improvement |
| `pool2` | peak working set, MiB | 573.7 | 543.3 | 0.947 | 0.009 | 0.948, 0.949, 0.946 | improvement |
| `pool2` | collection exec, s | 0.910 | 0.766 | 0.842 | 0.067 | 0.844, 0.824, 0.843 | improvement |
| `pool2` | update exec, s | 0.695 | 0.573 | 0.824 | 0.035 | 0.816, 0.836, 0.826 | improvement |
| `pool2` | update validation, s | 0.0353 | 0.0101 | 0.286 | 0.131 | 0.291, 0.276, 0.288 | improvement |
| `pool2` | update input read, s | 0.127 | 0.080 | 0.626 | 0.058 | 0.639, 0.621, 0.626 | improvement |
| `pool2` | checkpoint I/O, s | 0.226 | 0.238 | 1.056 | 0.062 | 1.040, 1.079, 1.070 | inconclusive |

Notes on timing verdicts:

- **A/A noise check (`main` vs `main`, 3 A-B-B-A blocks)**: every verdict across all
  three cases was `inconclusive`:
  - `fresh`: median wall `1.650` vs `1.657` s (ratio `1.004`, spread `0.035`), peak
    working set `511.1` vs `511.8` MiB (ratio `1.001`, spread `0.028`);
  - `warm`: median wall `1.704` vs `1.698` s (ratio `0.996`, spread `0.088`), peak
    working set `536.1` vs `535.2` MiB (ratio `0.998`, spread `0.008`);
  - `pool2`: median wall `2.028` vs `2.031` s (ratio `1.002`, spread `0.016`), peak
    working set `573.8` vs `574.8` MiB (ratio `1.002`, spread `0.012`).
- **`checkpoint_io_seconds` sub-phase**: `branch` still writes and reads back
  `checkpoint.json` on every update to verify persisted durability before advancing
  `CarriedLearnerV1`. On `warm`, `receipts.checkpoint_io_seconds` shifted by `+15.0`
  ms (`0.2245` -> `0.2395` s, ratio `1.067`, exceeding the `5.4%` spread and marked
  `regression`), while `fresh` (`+8.2` ms, `1.036`) and `pool2` (`+12.7` ms, `1.056`)
  were `inconclusive` (and `fresh` in `counters` was `0.949`). Total
  `scheduler.update_execution_seconds` (which includes `checkpoint_io_seconds`)
  improved conclusively across all three cases (`-44.7` ms on `fresh`, `-38.2` ms on
  `warm`, `-122.2` ms on `pool2`). Borrowed serialization and checkpoint codec
  reduction belong to T2 (#596).
- **First-execution warm-up**: as in #607 and #608 on Windows, the first execution of
  each newly linked binary (`main-a` warm-up 0 in `aa` at `5.16` s, `branch` warm-up 1
  in `ab` at `4.93` s) paid Windows first-launch scanning overhead and was excluded
  from timed medians by the warm-up policy.

## Loaded-source and I/O counters

The `counters` run executed one A-B-B-A block (`main` vs `branch`, 2 samples per arm
per case) with `MTG_KERNEL_CACHE_DIAGNOSTICS_V1` enabled. Every `loaded_source` count
and byte total was identical across samples for each arm (`verified_bytes` differed
by at most 1-2 bytes across samples from variable-length timing floats in run JSON
files).

### `fresh` and `warm` loaded-source counters (13 scoped loads per 2-update run)

`fresh` and `warm` record identical `loaded_source` counts and byte totals:

| Role | Scoped loads | `main` lookups / hits / kept | `main` state copied (`kept + served`) | `main` policy copied | `branch` lookups / hits / kept | `branch` carried (`policy / state`) | `branch` state copied (`kept + served`) | `branch` policy copied (`kept + served + carried`) | Scoped median (`fresh`: `main` -> `branch`) | Scoped median (`warm`: `main` -> `branch`) |
|---|---:|---|---:|---:|---|---|---:|---:|---|
| `run_initial` | 1 | 1 / 0 / 1 | 14.78 MiB | 4.00 MiB | 1 / 0 / 1 | 0 / 0 | **0 B** | 4.00 MiB | 57.5 -> 50.2 ms | 73.9 -> 68.5 ms |
| `collect_learner` | 2 | 2 / 2 / 0 | 29.56 MiB | 8.00 MiB | 0 / 0 / 0 | 2 / 0 | **0 B** | 8.00 MiB | 14.6 -> 8.8 ms | 20.0 -> 11.5 ms |
| `opponent` | 4 | 4 / 4 / 0 | 59.12 MiB | 16.00 MiB | 4 / 4 / 0 | 0 / 0 | **0 B** | 16.00 MiB | 72.4 -> 15.9 ms | 78.4 -> 23.0 ms |
| `update_learner` | 2 | 2 / 2 / 0 | 29.56 MiB | 8.00 MiB | 0 / 0 / 0 | 0 / 2 | **0 B** | **0 B** | 14.0 -> 7.1 ms | 17.9 -> 11.1 ms |
| `update_readback` | 2 | 2 / 0 / 2 | 29.56 MiB | 8.00 MiB | 2 / 0 / 2 | 0 / 0 | **0 B** | 8.00 MiB | 87.8 -> 93.1 ms | 74.5 -> 94.7 ms |
| `update_validation` | 2 | 2 / 2 / 0 | 29.56 MiB | 8.00 MiB | 2 / 2 / 0 (`identity_served = 2`) | 0 / 0 | **0 B** | **0 B** | 43.0 -> 11.2 ms | 38.6 -> 10.7 ms |
| **total** | **13** | **13 / 10 / 3** | **192.14 MiB** | **52.00 MiB** | **9 / 6 / 3** | **2 / 2** | **0 B** | **36.00 MiB** | **289.4 -> 186.2 ms** | **303.2 -> 219.5 ms** |

(`14.78 MiB` = `15,497,944` bytes per `NativePolicyValueTrainStateV1` copy;
`4.00 MiB` = `4,194,368` bytes per `LoadedSourcePolicyV1` copy.)

### `pool2` loaded-source and decode counters (17 scoped loads per 2-update run)

On `pool2`, `main` suffered 4-slot FIFO thrashing in `loaded_source_cache_v1` because
learner checkpoints (`run_initial`, `update_readback`) competed with the 2 fixed-pool
opponents, causing 13 cache misses and 10 full `checkpoint.json` decodes
(`147.88 MB`). On `branch`, `CarriedLearnerV1` kept `collect_learner` and
`update_learner` out of the cache and stored policy-only entries (`remember_policy_v1`),
turning all 4 replay opponent loads after iteration 0 into policy-only cache hits and
cutting `checkpoint.json` decodes from 10 to 6 (`88.73 MB`, `-40.0%`):

| Role | Scoped loads | `main` lookups / hits / kept | `main` state copied (`kept + served`) | `branch` lookups / hits / kept | `branch` carried (`policy / state`) | `branch` state copied (`kept + served`) | Scoped median (`main` -> `branch`) |
|---|---:|---|---:|---|---|---:|---|
| `run_initial` | 1 | 1 / 0 / 1 | 14.78 MiB | 1 / 0 / 1 | 0 / 0 | **0 B** | 48.1 -> 45.2 ms |
| `run_fixed_opponent` | 2 | 2 / 0 / 2 | 29.56 MiB | 2 / 0 / 2 | 0 / 0 | **0 B** | 127.0 -> 126.3 ms |
| `collect_learner` | 2 | 2 / 1 / 1 | 29.56 MiB | 0 / 0 / 0 | 2 / 0 | **0 B** | 32.5 -> 7.8 ms |
| `opponent` (serial replay) | 6 | 6 / 0 / 6 | 88.68 MiB | 6 / 4 / 2 (`policy_served = 4`) | 0 / 0 | **0 B** | 319.5 -> 127.9 ms |
| `update_learner` | 2 | 2 / 1 / 1 | 29.56 MiB | 0 / 0 / 0 | 0 / 2 | **0 B** | 31.5 -> 6.9 ms |
| `update_readback` | 2 | 2 / 0 / 2 | 29.56 MiB | 2 / 0 / 2 | 0 / 0 | **0 B** | 73.3 -> 90.8 ms |
| `update_validation` | 2 | 2 / 2 / 0 | 29.56 MiB | 2 / 2 / 0 (`identity_served = 2`) | 0 / 0 | **0 B** | 37.7 -> 10.7 ms |
| **total** | **17** | **17 / 4 / 13** | **251.26 MiB** | **13 / 6 / 7** | **2 / 2** | **0 B** | **669.7 -> 415.6 ms** |

## Scope boundary and remaining follow-ups

- **Milestone 4 (S-learner / W-learner carried state)**: `s_learner_v1` and
  `w_learner_v1` use separate checkpoint containers and loaders outside
  `NativePolicyValueTrainStateV1`. In `native_expanded_training_run_v1.rs`, non-net-8
  learners and non-verifiable sources explicitly fall back to `carried = None` with
  zero behavioral change (verified by `s_learner_v1::` and `w_learner_v1::` test
  suites); extending carried-state handoff to S remains an explicit Milestone 4
  follow-up if needed.
- **Checkpoint codec / I/O (`T2 / #596`) and serial-replay opponent sharing
  (`T5 / #599`)**: `update_readback` still decodes persisted `checkpoint.json` once
  per update (`2` decodes on `fresh`, `3` on `warm`), and serial replay on `pool2`
  still decodes each distinct pool member once before `loaded_source_cache_v1`
  captures its policy (`6` total decodes across `run_fixed_opponent`, `OPPONENT_CACHE`,
  and `update_readback`). Those paths belong to #596 and #599 respectively.
