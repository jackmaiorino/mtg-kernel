# S learner attribution (T4 first milestone, #598)

At two workers the exact parallel S learner's gradient took **148.7 ms** against
**260.4 ms** serial (1.75x) on `gae-bench seed=7 groups=32` (44 substeps), on a
4-vCPU cloud VM pinned to two vCPUs. Of the 18.5 ms lost against an ideal half
of the serial gradient, **13.2 ms is record-phase imbalance** across the 11
four-substep blocks. Copying the terms and replaying them cost about what the
serial sink saves. Helper spawning cost 1.2 ms per update. Serial replay
additions cost 2.4 ms. The final block's object query rows are **22.0%** of the
backward's multiply-adds, which no output reads.

These are attribution measurements, not a speedup claim. Counts and bytes are
exact. Times come from a measurement-only build on a noisy VM, and both builds
use the CI receipt profile (LTO off, 16 codegen units). The checked-in release
profile timings still need a machine with at least 32 GB (see Pending).

| 2-worker gradient, mean of 8 repetitions | ms | share |
|---|---:|---:|
| Record drain (forward, recorded backward) | 115.8 | 77.9% |
| Replay drain (weight-gradient shards) | 30.1 | 20.2% |
| Replay additions (serial) | 2.4 | 1.6% |
| Shard build, score merge, rest | 0.5 | 0.3% |
| Gradient | 148.7 | |

- **Record against replay:** in the record drain the two workers were busy
  107.7 and 95.1 ms. The sum of each block's slower worker is 114.6 ms against a
  101.4 ms mean, and the caller waited 7.5 ms on joins. Record work totals
  202.7 ms, 1.6% more than the serial forward and backward without weight
  gradients (199.6 ms). Replay work totals 56.3 ms, below the serial sink's
  60.3 ms. Replay is balanced: 28.5 and 27.8 ms busy, 0.4 ms imbalance.
- **Copies:** each update records 41.58 MiB of term values. That is 14.98 MiB of
  Linear inputs, 18.26 MiB of Linear output gradients, 3.82 MiB of
  materialized LayerNorm gains, 3.82 MiB of LayerNorm biases and 0.69 MiB of
  embedding additions. They come from 1,097 Linear terms, 10,441 LayerNorm rows
  and 4,479 additions. Copying took 6.1 ms of worker time.
- **Peak term bytes:** one block holds at most 4.60 MiB live (5.61 MiB
  reserved). Peak process heap was 30.7 MiB at two workers and 29.1 MiB serial.
- **Helpers:** each update spawns and joins 22 scoped helpers (11 per drain
  kind) with no spawn failure. The caller spent 0.51 ms spawning record helpers
  and 0.70 ms spawning replay helpers (46 and 64 us each).
- **Shards:** replay has 25 shards, one per Linear weight. None is split,
  because the target of 57,544 parameters exceeds the largest weight (36,864).
  Parameter count does not track work. FFN weights are 49.7% of shard
  parameters, 61.6% of replay multiply-adds and 60.0% of replay time. The state,
  action, policy and value input weights are 13.1% of parameters, 1.6% of
  multiply-adds and 4.1% of time. The dynamic queue still keeps workers even.
- **Serial path:** forward is 29.2% of the gradient and backward 70.6%. Weight
  gradients in the sink are 23.2%. Per layer the backward spends about 30.5 ms
  in the FFN (including its weight gradients), 11.9 ms in attention, 9.3 ms in
  QKV and 3.2 ms in the projection. Adam took 4.9 ms and the state hash 5.4 ms.
- **Dead rows:** the census counts 1.290 billion backward multiply-adds per
  update. The final block's object query rows account for 283.1 million of them,
  70.6% of that block's backward. They sit in the FFN, the projection, the Q
  part of QKV and attention (dP, dQ and their zero terms in dV and dK). Their
  keys and values stay live.
- **Allocations:** the serial gradient allocates 1,068 times (6.9 MiB) per
  update and the two-worker one 812 times (11.4 MiB). The parallel path builds
  fresh helper workspaces and term records every update. `gae-bench` also
  starts every repetition from fresh workspaces. Unlike the production training
  binaries, `s_transformer_v1` does not call `retain_freed_memory_v1`.

## Correctness

The profile is measurement only (`s-learner-profile-v1`, off by default).
Default builds compile its spans and counters to nothing. No arithmetic,
reduction order, parameter or Adam state changes.

- All 21 `s_transformer_v1::` tests pass, including the five exact tests
  `parallel_gae_preserves_full_adam_checkpoint`,
  `parallel_gae_reports_first_batch_error_without_updating_adam`,
  `gemm_matches_the_plain_loop_bit_for_bit`,
  `gradient_matches_central_differences` and
  `forward_is_repeatable_and_independent_of_tape_history`. The 7
  `expanded_deck_training_v1::s_learner_v1::` tests pass (1 existing ignored).
- `tests/s_learner_profile_v1.rs` passes. Replay multiply-adds equal the
  analytic weight-gradient census, byte counters equal the recorded values,
  every span, drain, spawn and join is accounted for, and serial and parallel
  states match.
- `parity-dump seed=7 batches=1 groups=4` gives identical `files_sha256` (7
  files) under baseline and candidate. Every `gae-bench` attempt in every build
  and at both worker counts ends at the same full state,
  `7d577ed1a63e151d978bb090d3f118081c265b56b158d2095aa318db6b38a2ed`.
- `cargo clippy --release --locked -p mtg-kernel --all-targets -- -D warnings`
  is clean, and so is the same lint of the lib, binary and profile test with the
  feature. `cargo fmt --all -- --check` and `git diff --check` pass.

## Default-build A-B-B-A

Four A-B-B-A rounds per worker count, one process per attempt, 3 repetitions
each, CPUs 2-3. Warm-process repetitions exclude each process's first.

| Workers | Baseline mean s | Candidate mean s | Mean ratio | Median ratio | Paired rounds |
|---:|---:|---:|---:|---:|---|
| 1 | 0.2534 | 0.2481 | 0.979 | 0.991 | 0.948, 0.948, 0.963, 1.063 |
| 2 | 0.1502 | 0.1507 | 1.004 | 0.990 | 1.050, 1.005, 0.993, 0.965 |

Both differences are within the observed spread, so the default build's
instrumentation shows no measurable cost here. This is not a speedup.

## Identities

Baseline is `c6329d868ad32467b22a411018894a6eae65205c`. The candidate and the
profile build are `dc3a0081aefab2cb7ed30c25cd1fa50fc29be95b`. Later commits on
the branch touch documentation and the receipt script only. Binary SHA-256s are
`361fcaf5…e370` (baseline), `ad8df6c6…c504` (candidate) and `79f7d5b8…624a`
(profile); the full values are in the manifest. The builds used Rust and Cargo
1.94.1 (LLVM 21.1.8), x86_64-unknown-linux-gnu, GNU ld 2.42, `--release
--locked`, `CARGO_PROFILE_RELEASE_LTO=false` and
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`. The host has 4 vCPUs (Intel Xeon @
2.10GHz) and 15.7 GiB. Raw samples, digests and the aggregate profile are in
the [manifest](s_learner_attribution_cloud_20261108.json). `python
scripts/experiments/s_learner_attribution_v1/run.py summarize <manifest>`
regenerates every table from it.

## Pending

The checked-in release profile (thin LTO, one codegen unit) needs more memory
than this 15 GiB host. These steps still need a machine with at least 32 GB:

1. Build baseline `c6329d8` and candidate `dc3a008` with that profile, the
   candidate both with and without `--features s-learner-profile-v1`.
2. Run the five exact tests and `--test s_learner_profile_v1` under it.
3. Run `run.py run` with the three binaries, fixed `--cpus`, workers 1 and 2,
   four rounds and four profile runs. The result is a second manifest.

Synthetic `gae-bench` speed is not durable-update gain. Any later kernel change
needs its own real-input qualification.
