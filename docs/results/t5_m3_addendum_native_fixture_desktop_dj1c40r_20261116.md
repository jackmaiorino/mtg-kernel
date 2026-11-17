# T5 Milestone 3 addendum: model bytes charged in collector and replay caches (checked-in profile, Windows PC)

The #599 cache follow-up at `95c33b0dd` against the Milestone 3 build at `ed3f5e29e` (`m3`),
on the fixtures, host and build settings of the
[Milestone 3 receipt](t5_m3_native_fixture_desktop_dj1c40r_20261116.md): the public pool
fixture on `DESKTOP-DJ1C40R` (i7-13700K, 128 GiB, Windows 11), checked-in release profile
(`rustc 1.94.1`, MSVC). The raw outputs name the follow-up build `m4`. That is an arm label,
not the issue's milestone 4. All runs are from 2026-11-16, 11:39 to 16:53 UTC. This receipt
was written on 2026-11-17 from copies of the retained outputs. Nothing was rerun for it.

The follow-up commit:

- charges, in every collector and replay cache, each distinct model its entries hold, once
  per cache, whether or not the process cache also holds that model;
- serves a load of the source in the transient slot from that slot;
- runs the native driver on a thread with an explicit 16 MiB stack;
- removes the `evicted_file_bytes` diagnostics counter, which nothing incremented, and adds
  `transient_hits`, `live_charged_bytes` and `peak_live_charged_bytes` to the collection
  diagnostics.

Receipts:

- [`t5_m3_addendum_native_fixture_desktop_dj1c40r_20261116.json`](t5_m3_addendum_native_fixture_desktop_dj1c40r_20261116.json),
  written by `bench.py receipt`: every sample of the timed `pool18` run (`ab4`), with
  verdicts, boundaries and raw-output SHA-256.
- [`t5_m3_addendum_parity_desktop_dj1c40r_20261116.json`](t5_m3_addendum_parity_desktop_dj1c40r_20261116.json):
  the binaries, the clippy and unit test results, the stack probe, the untimed parity runs
  with their cache counters, and the SHA-256 of every output and of every file this receipt
  was written from.

## What this evidence covers

- Every native run is of the binary built from `95c33b0dd` exactly. The later commits on the
  branch add a unit test with its test-only helper, one comment and documents; the code the
  run binary executes is unchanged. Unit tests on a Mac and CI cover those commits (see
  Checks), not these runs.
- Every opponent of the fixture is a source the process cache can verify and fork. In every
  arm and case of the three counter runs, `not_kept.unverifiable`, `not_kept.unforkable` and
  the collection caches' `misses.unverified` are 0. The native runs therefore exercise the
  default budgets, process-cache eviction, refusal by both budgets and the transient slot. A
  source the process cache cannot re-verify, cannot fork or never sees is covered by unit
  tests only.
- In no native run does the model charge decide what a collector or replay cache keeps. At
  the default budget the peak charge (78,760,072 bytes) is far below it, with one entry the
  entry cap decides, and a 1 MiB budget is already below an entry's own bytes (4,194,368). A
  budget between an entry's own bytes and its bytes with its model is covered by unit tests
  only.

## Result

Outputs are unchanged and the timed comparison is inconclusive. No throughput or memory
change is claimed for the follow-up.

| Case | updates/hour m3 -> m4 | Ratio | Spread | Block ratios | Peak working set, MiB | Verdict |
|---|---:|---:|---:|---|---:|---|
| `pool18` | 1740.7 -> 1734.6 | 0.9965 | 0.0207 | 0.9972, 1.0003 | 1064.0 -> 1065.4 (1.0014, inconclusive) | inconclusive |

Medians of 4 timed samples per arm (2 A-B-B-A blocks after one untimed warm-up per arm,
`--no-cache-diagnostics`). Raw wall seconds per sample, m3 | m4:

- `pool18`: 6.210 6.163 6.199 6.293 | 6.211 6.197 6.245 6.242

Of the 18 compared metrics, 15 are inconclusive, among them wall time (1.0035), user CPU
(1.0048), system CPU (1.1608, spread 0.2410), time outside iterations (1.0028) and the
update's elapsed time (1.0060, spread 0.0189). Three phase timings pass the rule: behavior
replay 0.9841 (improvement, spread 0.0090), input read 1.0177 (regression, spread 0.0168)
and learner update 1.0096 (regression, spread 0.0090). Two blocks is the rule's minimum, so
these three are weak evidence either way.

## Charges and counters (untimed runs, 1 block, diagnostics on)

Counters are those of each arm's first timed sample; the outputs hold no other sample's.

Default budgets (`counters4`). In all four cases, every counter of the `opponent_cache`, `io`
and `loaded_source` sections that both builds report is equal between `m3` and `m4`: lookups,
hits, misses, full loads, kept and refused entries, evictions, charged bytes, checkpoint
decodes and read bytes. Only the verified-bytes memo, which the follow-up does not touch,
and the retained-file totals that count it differ: its copied, recorded and compared byte
totals by at most 31 bytes, and the bytes it holds, has evicted and peaked at by up to
3,921,659 (`fresh`). The `m3` arms of the three runs differ from each other in the same way,
by up to 5,687,105 held bytes on `fresh`. `m4` adds the charge of the
collector and replay caches:

| Case | Peak live entries | Their own policy bytes | Peak live charge, bytes | Process cache charge, bytes |
|---|---:|---:|---:|---:|
| `fresh` | 1 | 4,194,368 | 9,845,049 | 29,537,565 |
| `pool2` | 2 | 8,388,736 | 19,690,018 | 19,691,854 |
| `pool18` | 8 | 33,554,944 | 78,760,072 | 177,226,686 |
| `pool64` | 8 | 33,554,944 | 78,760,072 | 315,069,664 |

The budgets are 268,435,456 bytes for each collector or replay cache and 805,306,368 for the
process cache. With 8 entries live, the charge is 9,845,009 bytes an entry: 4,194,368 of its
own policy bytes, 5,649,992 of its model (the process cache reports the same bytes per
model) and 649 of descriptor and overhead. The live charge is 0 at the end of every run, and
no load is served from a transient slot.

Forced eviction (`evict4`: process budget 12 MiB, 1 entry per collector or replay cache). The
process cache holds 1 entry (9,845,927 bytes charged; 9,846,007 on `fresh`) and evicts 2, 33,
64 and 111 times (`fresh`, `pool2`, `pool18`, `pool64`). The collector and replay caches
evict 0, 28, 42 and 42 entries and never charge more than one entry with its model
(9,845,009 bytes; 9,845,049 on `fresh`). Full loads, evictions, decodes and read bytes equal
those of `m3` under the same settings in the Milestone 3 parity receipt.

Both budgets at 1 MiB (`over4`). The process cache refuses every full load for its budget
(6, 36, 66 and 112) and keeps nothing. The collector and replay caches refuse every entry too
(6, 34, 48 and 48) and charge 0. The transient slot serves 42 loads on `fresh` and 14 on
`pool2`, so full loads fall from 48 to 6 and from 50 to 36 against `m3` under the same
settings (Milestone 3 parity receipt), and read bytes from 1,870,903,178 to 628,184,386 and
from 2,484,644,216 to 1,932,418,192. `pool18` and `pool64` repeat no source back to back
(0 transient hits) and load as `m3` did.

## Parity

Every sample completed all 3 iterations in every run whose samples this receipt holds or
whose comparison it holds (`ab4`, `counters4`, `evict4`, `over4`).

- `counters4`, `evict4`, `over4`: 24 samples each (4 cases, each with 4 timed samples and 2
  cold resumes), driver exit 0. Every sample equals its case's reference, the `m3` arm's first
  sample, up to accounted fields: timing fields, build provenance and the pins of files that
  differ only in those.
- `ab4`: 10 samples, equal up to accounted fields. Every sample's three `pool18` boundaries
  (checkpoint, state and output identity) equal the Milestone 3 receipt's; the final
  checkpoint is `c85e8ad6...`.
- Parallel fixture (4 collection and 4 preparation workers), `m3` against `m4` under forced
  eviction (`par-evict4`) and against `m4` with 0 entries per collector cache (`par-zero4`):
  24 samples each. The driver exits 1 and marks all 20 compared samples of each run
  `mismatch`, as it does on this fixture for `base` against `base`: it has no rule for the
  per-worker scheduling fields (Milestone 3 receipt). The pipeline checked each run's
  `samples.jsonl` and found every sample complete, with boundaries equal to its reference's
  and no extra file. The copies this receipt was written from hold these two runs' manifests
  and log lines but not their `samples.jsonl` or `comparison.json`, so that check is quoted
  from the pipeline log, not recomputed, and their counters are not reported here.

## Stack depth on Windows

Probe copies of `m4` with the main-thread stack reserve set by `editbin /STACK`, `pool18` and
`pool64` under the 12 MiB process budget:

| Reserve, KiB | 1024 | 896 | 512 | 256 |
|---|---|---|---|---|
| `m4`, `pool18` | pass | pass | pass | pass |
| `m4`, `pool64` | pass | pass | pass | pass |

`m3` overflowed at 960 KiB and below on this path and `base` at 896 KiB and below (Milestone
3 receipt). The driver no longer runs on the main thread, so the platform's main-thread
default no longer bounds it. Not probed: a reserve below 256 KiB, and how much of the driver
thread's 16 MiB the deepest path uses.

## Accounting after the follow-up

A collector or replay cache charges, for each entry, its policy's own copies (the embedding
table and any no-state-digest check model), its source descriptor and a fixed overhead, and,
once per cache, each distinct net-8 model its entries hold. An entry whose own bytes plus its
model exceed the budget, or any entry when the entry cap is 0, is not kept. Every policy that
enters such a cache goes through `OpponentCacheV1::insert`, whose charge does not consult the
process cache, so a model the process cache does not hold is charged too:

| The process cache does not hold the model because | Unit test |
|---|---|
| it evicted the entry while a collector or replay entry lives on | none of that exact sequence; `process_and_collection_opponent_caches_enforce_byte_budgets_lru_and_oversized_passthrough` shows every kept entry charging its model |
| it refused the entry for its budget and served it once | the same test |
| it cannot re-verify the source and keeps nothing | `archive_distillation_bases_always_load_in_full` (replay cache) |
| the collector loads the source in full without it (unverified) | `archive_distillation_bases_always_load_in_full` (collector cache) |
| it cannot fork the loaded policy and serves that policy itself (`not_kept_unforkable`) | `a_policy_that_cannot_be_forked_is_charged_for_its_model`, added after `95c33b0dd` |

The last row was open in the Milestone 3 review. The code of `95c33b0dd` already charges it:
the policy served is inserted like any other, and its model is counted by the same function.
No loader of the training module returns a policy that cannot be forked today, so the test
makes one.

What stays outside every charge:

- The transient slot: at most one refused entry per collector or replay cache.
- The native S model or wide net of an S or wide source. The process cache never keeps such
  a source. A collector or replay cache charges its policy's own copies and net-8 model, but
  not the S model or wide net installed on that policy. Charging it is a follow-up; no run or
  test here checks the charge of an S or wide opponent.
- The working buffers a policy grows while it plays (encoder, tensors, forward scratch, row
  cache). The charge counts weights and descriptors.
- A policy between its load and its insertion: a miss loads before the bounds evict.
- Not a cache: an update with preparation workers holds one policy per distinct opponent of
  the update, at most 32, for the length of the preparation, under no byte budget.

## Follow-ups of the Milestone 3 receipt

- Windows main-thread stack margin: closed by the driver thread (above).
- Current-self fork reloaded under 0 entries or a budget below one fork: closed. The
  transient slot serves it (`over4` above; unit test
  `parallel_current_self_collection_matches_under_zero_and_sub_fork_cache_limits`).
- `evicted_file_bytes`: removed. `m3` reports it as 0 in each of the 12 counter sets held
  here, and `m4` no longer has the key.
- Run-start roster warm-up cost: not addressed. Time outside iterations is unchanged
  against `m3` (1.0028, inconclusive).

## Retained artifacts

The outputs behind this receipt (`ab4`, `counters4`, `evict4`, `over4`, `par-evict4`,
`par-zero4`, the state files and the `m4` binary) and the pipeline log are not in the repo.
They are kept with the Milestone 3 outputs under `D:/antigravity-20261112/bench-599` and
`D:/antigravity-20261112/logs/jobs` on the maintainer desktop. That is a live-work SSD, not
the artifact drive: the tree still has to move there with a manifest, and both receipts then
need the new location. The SHA-256 in the two JSON receipts identify each file until then.

## Checks and setup

- At `95c33b0dd` on the desktop: `cargo clippy --release --locked --workspace --all-targets -- -D warnings`
  (exit 0); `cargo test --release --locked -p mtg-kernel --lib -- --test-threads 4` with filters
  `opponent_cache verified_bytes_cache_v1:: s_learner_v1:: w_learner_v1:: no_state_digest_v1::
  fixture_bench_v1:: loaded_source_cache_v1:: collection_cache_boundary_rejects_same_path_mutation_of_all_pins
  phase1_preparation_rejects_stale_and_corrupted_opponent_before_learning pool_fixture_v1
  cache_diagnostics_v1 loaded_source_profile_v1 phase1_parallel_collection::`: 87 passed, 0
  failed, 3 ignored.
- Binaries (`cargo build --release --locked -j 4 -p mtg-kernel --bin native_expanded_training_run_v1`):
  `m4` `95c33b0dda4d1a72c1ff9911154d1a27518bb1e8`, sha256
  `97e35121d7cdc0d09ca8e38863370e1a660f0c0b4274cce8e2c767e8de44c8e6`, 64,573,440 bytes;
  `m3` as in the Milestone 3 receipt (`43dc0774...`).
- Fixtures: the Milestone 3 ones, unchanged (`fixture.json` sha256 `f2c4d411...` serial,
  `47f461b9...` with 4 workers).
- Timed run: `cargo_slot.py timed --cores 0-7`, `bench.py run --cases pool18 --blocks 2
  --warmup-runs 1 --no-cache-diagnostics --keep-runs reference`. No snapshot of the host's
  other claims or free memory was recorded around it.
- Untimed runs and the stack probe: 4-core below-normal claims (cores 20-23), `--blocks 1
  --warmup-runs 0 --cold-resume`; clippy, the unit tests and the build: 4-core claims (cores
  12-15).
@@MAC@@
- Not measured here: full-node memory and durable throughput (T6, #600).
