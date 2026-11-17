# T5 Milestone 3: byte-bounded opponent caches (checked-in profile, Windows PC)

Main `3b4875917` (`base`) and the #599 Milestone 3 branch at `ed3f5e29e` (`m3`) ran
the public pool fixture (`--seed 2026091601 --updates 3 --games 8 --warmup-updates 0
--pool-members 2,18,64`) on `DESKTOP-DJ1C40R` (i7-13700K, 128 GiB, Windows 11),
built identically with the checked-in release profile (`rustc 1.94.1`, MSVC).

Receipts:

- [`t5_m3_native_fixture_desktop_dj1c40r_20261116.json`](t5_m3_native_fixture_desktop_dj1c40r_20261116.json),
  written by `bench.py receipt`: every timed sample (`ab`) and the counter run
  (`counters`), with verdicts, boundaries and raw-output SHA-256.
- [`t5_m3_parity_desktop_dj1c40r_20261116.json`](t5_m3_parity_desktop_dj1c40r_20261116.json):
  forced-eviction, oversized-entry, parallel-worker and 64-entry runs, the stack probe and
  the unit test result.

## Result

`m3` is faster than `base` on every pool case, beyond the observed spread, and smaller on
`pool18` and `pool64` (the `pool2` peak working set is inconclusive), with bit-identical
outputs. Run startup is slower: `outside_iterations_seconds` regresses
on `pool18` (1.010 -> 1.125 s, ratio 1.114, spread 0.028, blocks 1.112, 1.119, 1.114) and
`pool64` (3.239 -> 3.653 s, ratio 1.128, spread 0.025, blocks 1.116, 1.139, 1.124), and is
inconclusive on `pool2` (1.043). The likely cause is that run start now loads every
fixed-roster member into the process cache (on `pool64`, 32 of 64 are then evicted).
Whole-run wall time still improves.

| Case | updates/hour base -> m3 | Ratio | Spread | Block ratios | Peak working set, MiB | Verdict |
|---|---:|---:|---:|---|---:|---|
| `pool2` | 2010 -> 2073 | 1.032 | 0.017 | 1.034, 1.030, 1.044 | 856 -> 846 (inconclusive) | improvement |
| `pool18` | 1400 -> 1693 | 1.209 | 0.023 | 1.206, 1.218, 1.207 | 1140 -> 1045 (0.917) | improvement |
| `pool64` | 999 -> 1124 | 1.125 | 0.038 | 1.130, 1.111, 1.147 | 1409 -> 1189 (0.844) | improvement |

Medians of 6 timed samples per arm (3 A-B-B-A blocks after one untimed warm-up per arm,
`--no-cache-diagnostics`). Raw wall seconds per sample, base | m3:

- `pool2`: 5.442 5.355 5.375 5.371 5.431 5.364 | 5.233 5.206 5.221 5.212 5.193 5.145
- `pool18`: 7.664 7.679 7.686 7.746 7.789 7.738 | 6.393 6.327 6.305 6.366 6.449 6.415
- `pool64`: 10.75 10.97 10.71 10.79 11.13 10.83 | 9.596 9.621 9.794 9.555 9.516 9.618

Update execution fell to 0.915 (`pool2`), 0.653 (`pool18`) and 0.612 (`pool64`) of base,
collection to 0.982 (inconclusive), 0.829 and 0.861. Job user CPU: 0.972 (inconclusive),
0.795 and 0.883.

## Misses, copies and memory (counter run, 1 block, diagnostics on)

| Counter (3-update run) | `pool2` base -> m3 | `pool18` base -> m3 | `pool64` base -> m3 |
|---|---:|---:|---:|
| Full opponent loads | 5 -> 2 | 52 -> 18 | 111 -> 75 |
| Checkpoint decodes | 8 -> 5 | 55 -> 21 | 114 -> 78 |
| Pinned file read bytes, MB | 749 -> 591 | 2090 -> 1518 | 3510 -> 3826 |
| File copies held by the opponent cache, MB | 39.4 -> 0 | 256.4 -> 0 | 453.6 -> 0 |
| Duplicate retained file bytes, MB | 39.4 -> 0 | 182.5 -> 0 | 231.8 -> 0 |
| All retained file bytes, MB | 198.8 -> 159.4 | 424.0 -> 159.4 | 621.2 -> 161.6 |
| Verified-bytes memo copies, MB | 187.0 -> 187.0 | 719.4 -> 423.6 | 1784 -> 1266 |
| Process cache charge / budget, MB | - / - -> 19.7 / 805.3 | - / - -> 177.2 / 805.3 | - / - -> 315.1 / 805.3 |

On `fresh` (current-policy control) the cache drops its 44.4 MB of file copies and keeps
3 decodes. On `pool64` the 32-entry cap binds (32 entries, 43 evictions, 315 MB charged),
and the hit revalidation streams each pinned file, so read bytes rise 9% while decodes,
copies and the memo fall. A 64-entry run on `pool64` (untimed, 1 block) cut full loads
75 -> 64 and read bytes 3826 -> 3392 MB with no evictions at a 630 MB charge, but peak
working set rose 1157 -> 1483 MiB; the default stays at 32 entries.

## Parity

Every sample completed all 3 iterations, and every boundary checkpoint, state and output
identity equals its case's reference:

- `ab`: 48 samples (pool2, pool18, pool64, cold resumes included), driver parity `ok`.
  Final checkpoints `32a8eab8...`, `c85e8ad6...`, `2600a979...`.
- `counters`: 16 samples, `fresh` included.
- Forced eviction (`MTG_KERNEL_OPPONENT_CACHE_MIB=12`, one process entry;
  `MTG_KERNEL_COLLECTION_OPPONENT_CACHE_ENTRIES=1`) against `m3` defaults: 24 samples with
  cold resumes, parity `ok`. `pool64` evicted 111 process and 42 collection entries.
- Oversized entries (`MTG_KERNEL_OPPONENT_CACHE_MIB=1`, `..._COLLECTION_..._MIB=1`): every
  load served once and not kept (112 on `pool64`), 24 samples, parity `ok`.
- Parallel (fixture with 4 collection and 4 preparation workers), `base` against the
  forced-eviction `m3`: 24 samples, every boundary equal. The driver marks all of them,
  `base` against `base` included, because per-worker scheduling fields and the pins of
  the files holding them vary with scheduling; it has no rule for those fields yet.
- Serial against parallel: every checkpoint's tensor payload (parameters and both Adam
  moments) is byte-identical across the two fixtures; only the header's fixture path
  differs.

## Stack depth on Windows

The first `m3` build overflowed the 1 MiB main-thread stack under a 12 MiB budget: serial
replay ran on the main thread and its process-cache miss loaded from inside the cache's
frames. `ed3f5e29e` splits the cache's load into never-inlined lookup, full-load and
admission steps that replay calls itself. Probe copies with the stack reserve set by
`editbin /STACK`, `pool18` and `pool64` under the 12 MiB budget:

| Reserve, KiB | 1024 | 1008-976 | 960-912 | 896 and below |
|---|---|---|---|---|
| `base` | pass | pass | pass | overflow |
| `m3` before the fix | overflow | - | - | overflow |
| `m3` | pass | pass | overflow | overflow |

`m3` still needs 48 to 80 KiB more main-thread stack than `base` on this path, leaving
48 to 64 KiB of the Windows default (`base` leaves 112 to 128 KiB). Linux main threads
usually get 8 MiB. Running the native driver on a thread with an explicit stack, as `--validate-config` already does,
would remove this margin question for both builds.

## Accounting limit

A collection or replay cache entry charges only its owned policy buffers
(`collection_entry_charge_v1`), never the shared model. The model is charged to the
process cache, so it is counted only while the process cache holds it. It is uncharged
anywhere when:

- the process cache evicts the model while a collection or replay entry still holds it;
- the process cache refuses the entry for its budget and serves it once (the fork then
  holds the only reference to the model);
- the source cannot be re-verified (S and W learner sources, archive-distillation
  parents, other schemas), so the process cache does not keep it;
- the collection cache loads in full without the process cache (an unverified source).

In those cases each collection or replay cache can keep up to its entry cap (8 by default)
of full models while charging only their owned buffers, and there is one such cache per
collection worker, so the per-collection byte budget does not bound them. This is not a
regression: `base` had no byte bound at all. Charging the model for entries whose model
the process cache does not hold is a follow-up. Defaults: process cache 32 entries and
768 MiB, collection cache 8 entries and 256 MiB.

## Follow-ups not fixed here

- Windows main-thread stack margin on the serial-replay miss path (above).
- With collection entries 0, or a collection budget below one fork, the current-self fork
  goes to the transient slot and the next load clears it, so the current source is loaded
  again for each episode. Outputs should not change, but no test or parity run covers it.
- The `evicted_file_bytes` diagnostics counter is still emitted but never incremented.
- Reduce the run-start roster warm-up cost noted under Result.

## Retained artifacts

The bench outputs behind both receipts (`bench.json`, `samples.jsonl`, `comparison.json`
and the reference run trees) and the `base` and `m3` binaries are not in the repo. They
are kept under `D:/antigravity-20261112/bench-599` on the maintainer desktop; the receipts
pin each by SHA-256.

## Checks and setup

- At `ed3f5e29e`: `cargo clippy --release --locked --workspace --all-targets -- -D warnings`
  (exit 0); `cargo test --release --locked -p mtg-kernel --lib` with filters
  `opponent_cache verified_bytes_cache_v1:: s_learner_v1:: w_learner_v1:: no_state_digest_v1::
  fixture_bench_v1:: loaded_source_cache_v1:: collection_cache_boundary_rejects_same_path_mutation_of_all_pins
  phase1_preparation_rejects_stale_and_corrupted_opponent_before_learning pool_fixture_v1
  cache_diagnostics_v1 loaded_source_profile_v1`: 76 passed, 0 failed, 2 ignored;
  `cargo fmt --all -- --check` and `git diff --check` clean.
- Binaries (`cargo build --release --locked -j 4 -p mtg-kernel --bin native_expanded_training_run_v1`):
  `base` `3b487591741b01ef1500324b618c9ffb6f2e2bc1`, sha256
  `2930839d58491c0736d521ac3b156f4e06dd4218c743cd467cfbd1cc5e608f91`;
  `m3` `ed3f5e29e8ae6f07e00f1700dbbd446a991baf10`, sha256
  `43dc077473d0b49ff848dadfec7a45995fa1ec6f15d89c64090489780a568473`.
  Commits after `ed3f5e29e` add receipts only.
- Fixtures were generated by the lib test executable of `9953e96ff` (the parent of
  `ed3f5e29e`, same generator source).
- Timed run: `cargo_slot.py timed --cores 0-7` (host reservation, P-cores 0-7), `bench.py run
  --cases pool2,pool18,pool64 --blocks 3 --warmup-runs 1 --cold-resume --no-cache-diagnostics
  --keep-runs reference`. Other lanes held cores 12-23 throughout (one claim on 4-7 was
  suspended by the hold); available RAM 82 GiB before and after.
- Untimed runs (counters, parity, probes): 4-core below-normal claims, `--blocks 1
  --warmup-runs 0`.
- Not measured here: full-node memory and durable throughput (T6, #600).
