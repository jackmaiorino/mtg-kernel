# S learner attribution (T4 first milestone, #598)

At two workers the exact parallel S learner's gradient took **148.7 ms** against
**260.4 ms** serial (1.75x) on `gae-bench seed=7 groups=32` (44 substeps), on a
4-vCPU cloud VM pinned to two vCPUs. Of the 18.5 ms lost against an ideal half
of the serial gradient, **13.2 ms is record-phase imbalance** across the 11
four-substep blocks. Copying the terms and replaying them cost about what the
serial sink saves. Helper spawning cost 1.2 ms per update. Serial replay
additions cost 2.4 ms. The final block's object query rows are **22.0%** of the
backward GEMM multiply-adds. The object keys and values remain live.

These are attribution measurements, not a speedup claim. Counts and bytes are
exact. Times come from a measurement-only build on a noisy VM, and both builds
use the CI receipt profile (LTO off, 16 codegen units). A second receipt, with
the checked-in release profile on a 32 GB Windows host, is under
[Checked-in release profile](#checked-in-release-profile).

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
  and 4,479 additions. Linear recording spans took 6.1 ms of worker time,
  including operand copies, record storage and instrumentation.
- **Peak term bytes:** the active block holds at most 4.60 MiB live (5.61
  MiB reserved). Peak process heap was 30.7 MiB at two workers and 29.1 MiB
  serial, including profile bookkeeping.
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
- **Dead rows:** the census counts 1.290 billion backward GEMM multiply-adds
  per update. The final block's object query rows account for 283.1 million of them,
  70.6% of that block's backward. They sit in the FFN, the projection, the Q
  part of QKV and attention (dP, dQ and their zero terms in dV and dK). Their
  keys and values stay live.
- **Allocations:** the caller thread records 1,068 allocations (6.9 MiB)
  during the serial gradient and a mean 812 allocations (11.4 MiB) during the
  two-worker gradient. The record helper reports another 656 allocation
  events on average during its work. The parallel path builds fresh helper
  workspaces and term records every update. `gae-bench` also
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
profile build are `dc3a0081aefab2cb7ed30c25cd1fa50fc29be95b`. The tests and
measurements above refer to those original builds. The follow-up profile
also reports capacity retained across all slots. That field has no value
in the archived receipt. Binary SHA-256s are
`361fcaf5…e370` (baseline), `ad8df6c6…c504` (candidate) and `79f7d5b8…624a`
(profile); the full values are in the manifest. The builds used Rust and Cargo
1.94.1 (LLVM 21.1.8), x86_64-unknown-linux-gnu, GNU ld 2.42, `--release
--locked`, `CARGO_PROFILE_RELEASE_LTO=false` and
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`. The host has 4 vCPUs (Intel Xeon @
2.10GHz) and 15.7 GiB. Raw samples, digests and the aggregate profile are in
the [manifest](s_learner_attribution_cloud_20261108.json). `python
scripts/experiments/s_learner_attribution_v1/run.py summarize <manifest>`
regenerates every table from it.

## Checked-in release profile

The second receipt builds baseline `c6329d8` and the reviewed successor
`aa85af1` with the checked-in release profile (thin LTO, one codegen unit) on
a 32 GB Windows host. There the serial gradient took **130.8 ms** and the
two-worker gradient **80.8 ms** (1.62x). Host, operating system and profile all
differ from the cloud receipt, so its times are not comparable with those
above. Shared workload-derived term and MAC counts and common term-storage
byte totals match the cloud receipt exactly. Dynamic allocation counts and
bytes differ; the retained-capacity counter below is new and has no value in
the cloud receipt.

| 2-worker gradient, mean of 8 repetitions | ms | share |
|---|---:|---:|
| Record drain (forward, recorded backward) | 61.4 | 76.0% |
| Replay drain (weight-gradient shards) | 18.2 | 22.6% |
| Replay additions (serial) | 0.8 | 1.0% |
| Shard build, score merge, rest | 0.3 | 0.4% |
| Gradient | 80.8 | |

- **Lost time:** of the 15.4 ms lost against an ideal half of the serial
  gradient, **7.0 ms is record-phase imbalance**. The sum of each block's
  slower worker is 60.7 ms against a 53.8 ms mean, and the caller waited
  6.2 ms on joins. About 5.5 ms more comes from slower work with two workers
  active. Record work totals 107.5 ms, 11.5% more than the serial forward and
  backward without weight gradients (96.4 ms). On the cloud VM that excess
  was 1.6%. The forward records nothing yet took 12.7% longer, so recording
  is not the main cause.
- **Replay:** 34.6 ms of work against the serial sink's 34.0 ms, balanced at
  17.6 and 17.0 ms busy (0.3 ms imbalance).
- **Unchanged counts:** each update records 41.58 MiB of term values from 1,097
  Linear terms, 10,441 LayerNorm rows and 4,479 additions. The peak active
  block holds 4.60 MiB live and 5.61 MiB reserved. The final block's object
  query rows are 22.0% of the 1.290 billion backward GEMM multiply-adds.
  Capacity retained across all slots peaks at 5.61 MiB, the same as the active
  block, because every block is full.
- **Serial path:** forward is 30.3% of the gradient, backward 69.5% and weight
  gradients in the sink 26.0%. Per layer the backward spends about 14.9 ms in
  the FFN, 5.9 ms in attention, 4.7 ms in QKV and 1.6 ms in the projection.
  Adam took 2.6 ms and the state hash 4.4 ms.
- **Helpers and allocations:** each update spawns and joins 22 helpers with no
  failure, spending 0.22 ms spawning record helpers and 0.25 ms spawning replay
  helpers. The caller thread records 1,068 allocations (6.9 MiB) serially and
  a mean 822 (11.4 MiB) at two workers, and the record helper reports 645 more.
  Peak process heap was 30.7 MiB at two workers and 29.1 MiB serially.

### Correctness

The test results here and the build and host conditions under Identities come
from the run's own logs. Those logs stay on the host and are not committed.
The manifest holds the timings, digests and profile.

- With this profile all 21 `s_transformer_v1::` tests pass, including the five
  exact tests listed above (`test result: ok. 21 passed; 0 failed; 0 ignored;
  0 measured; 3865 filtered out`). `tests/s_learner_profile_v1.rs` passes
  (`test result: ok. 1 passed; 0 failed`).
- `parity-dump` gives identical `files_sha256` under baseline and candidate,
  and the same 7 file hashes as the cloud receipt.
- Every `gae-bench` attempt, in all three builds and at both worker counts,
  goes from full state `d5e8442f…fc54` to `7d577ed1…a2ed`. Those are the
  cloud receipt's digests. The Linux CI-profile builds and these Windows
  checked-in-profile builds give bit-identical parameters and Adam state on
  this fixture.

### Default-build A-B-B-A

Four A-B-B-A rounds per worker count, one process per attempt, 3 repetitions
each. Warm-process repetitions exclude each process's first.

| Workers | Baseline mean s | Candidate mean s | Mean ratio | Median ratio | Paired rounds |
|---:|---:|---:|---:|---:|---|
| 1 | 0.1444 | 0.1396 | 0.966 | 1.000 | 1.028, 0.913, 1.059, 0.874 |
| 2 | 0.0861 | 0.0850 | 0.987 | 0.969 | 0.950, 1.032, 0.947, 1.024 |

Both differences are within the observed spread, which reaches 34% within an
arm at one worker. There, each process's repetitions agree closely, but
processes in both arms settle near 0.128 s or 0.146 s (once 0.172 s). The
default build's instrumentation again shows no measurable cost. This is not a
speedup.

### Identities

Baseline is `c6329d868ad32467b22a411018894a6eae65205c`. The candidate and the
profile build are `aa85af14a458d9b235592a69d8e69ca2da02f036`, which adds only
this milestone's changes to the baseline. Main has since merged other S
learner changes (#609, #617, #619), so these times describe `aa85af1`, not
current main.

- **Binaries:** SHA-256s are `a5dcb591…93d5` (baseline), `90819b35…be06`
  (candidate) and `a1172b54…d83e` (profile).
- **Toolchain:** Rust and Cargo 1.94.1 (LLVM 21.1.8),
  x86_64-pc-windows-msvc, MSVC link.exe 14.50.35725.0, `--release --locked`.
  The builds used the checked-in profile with no profile or RUSTFLAGS
  overrides.
- **Host:** Windows 11 (10.0.26200), AMD Ryzen 9 5900XT (16 cores, 32
  threads), 32 GiB.
- **Builds:** they ran under the host's build guard (4 cores, below-normal
  priority). With `-j 4` the profile build left as little as 2.6 GB
  available, and its page file grew by about 0.7 GB.
- **Timed step:** it ran at normal priority in a host-guard reservation of
  logical CPUs 4-7 (two physical cores), with WSL stopped and desktop load
  under 7% of total CPU.

The [manifest](s_learner_attribution_windows_20261110.json) holds the raw
samples, digests and aggregate profile. The run wrote it as 67,089 bytes with
SHA-256 `c02f8b37eb10ac144a6b9bf3c886d04eaef74a07cc9609de7345b539faef5411`. The
committed copy differs in only three ways: the run folder path is shown as
`<W>`, the host name is omitted, and line endings are LF. `run.py summarize`
regenerates the tables from it.

## Remaining

This milestone's checked-in release-profile steps are complete. Synthetic
`gae-bench` speed is not durable-update gain. Any later kernel change needs
its own measurements and real-input qualification.
