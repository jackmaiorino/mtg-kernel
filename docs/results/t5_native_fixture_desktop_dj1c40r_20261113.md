# T5 native pool-fixture benchmark (checked-in profile, Windows PC)

Main `791701e2f` and branch `b5dc72c0a` (#608, `c2a259f3a` merged cleanly with
`791701e2f`) ran the three-update, eight-game `pool2` and `pool18` native
fixtures on a 128 GiB Windows 11 desktop (`DESKTOP-DJ1C40R`), built with the
checked-in release profile (thin LTO, `codegen-units = 1`). Each case ran 2
A-B-B-A blocks under a host reservation on P-cores 0-7 after one untimed
warm-up per arm, followed (in the A/B run) by a cold resume per arm. Every
sample's run tree matched its case's reference sample up to accounted timing
fields, pins, and build provenance, across all 20 A/A samples, all 24 A/B
samples (including cold resumes), and all 8 counter-collection samples. Every
timing verdict in both A/A and A/B is inconclusive: `branch` median wall time
is within +0.9% on `pool2` (spread 1.9%) and -1.9% on `pool18` (spread 7.0%),
with peak working set within +1.2% on `pool2` (`916.8` vs `905.9` MiB) and
-0.7% on `pool18` (`1139.2` vs `1147.7` MiB). Because #608 adds only the public
pool fixture and opt-in cache counters with no cache-limit or behavior changes,
null timing overhead and exact boundary parity are the expected qualification
result.

The [receipt](t5_native_fixture_desktop_dj1c40r_20261113.json) was written by
`tools/native_fixture_bench/bench.py receipt`. It records every sample row,
verdict row, boundary pin, counter snapshot, and the SHA-256 of each run's raw
`bench.json`, `samples.jsonl`, and `comparison.json`. The raw run directories
and built binaries remain outside git under `D:/antigravity-20261112/`.

## Setup

- **Host**: `DESKTOP-DJ1C40R`, 13th Gen Intel Core i7-13700K (8 P-cores on
  logical CPUs 0-15 + 8 E-cores on logical CPUs 16-23, 24 logical CPUs total),
  128 GiB RAM (`130,884.2` MiB), Windows 11 10.0.26300, Python 3.14.3.
- **Guard and concurrent claims disclosure**: all builds, test runs, fixture
  generation, and timed benchmark runs went through the `#594` host guard
  serialized by `cargo_slot.py`. Each benchmark run (`aa`, `ab`, and `counters`)
  acquired `host_reservation_v1.py --cores 0-7` and executed inside
  `host_slots_v1.py timed --cores 0-7` on dedicated P-cores 0-7. Before and
  after each run, no other timed reservation was held; available physical RAM
  was 63.4-78.7 GiB and available commit was 39.7-56.0 GiB. Concurrent untimed
  claims on non-overlapping cores are disclosed explicitly:
  - WSL2 claim `fe-c` (lane `fe-20261112`, claim ID
    `620c0a6986724aec908495a615c7eb18` earlier in the session and
    `dd43a204f4094e10801105c05af6aa21` during the benchmark runs) ran
    concurrently on isolated E-cores 20-23;
  - background CI claim `ci-win-5` (`bf1ed275d38d4784a554cc13273b6932`) ran on
    cores 10-15 during `aa` and `ab`;
  - untimed 4-core claims on E-cores 16-19 ran before/after timed reservations.
  None overlapped P-cores 0-7. Builds, Rust test filters, Python unit tests,
  and fixture generation ran as untimed 4-core below-normal claims on E-cores
  16-23.
- **Build, identical for both arms**: checked-in release profile with no
  overrides (`lto = "thin"`, `codegen-units = 1`),
  `cargo build --release --locked -j 4 -p mtg-kernel --bin native_expanded_training_run_v1`,
  `rustc 1.94.1 (e408947bf 2026-03-25)` on `x86_64-pc-windows-msvc` with Visual
  Studio 2022 (`17.13.2`).

  | Step | Cargo build time | Artifact size | SHA-256 prefix |
  |---|---:|---:|---|
  | `clippy --release --lib --tests --bins -- -D warnings` (`branch`) | 6.5 min | - | - |
  | `test --release --lib --no-run` (`branch`) | 81.1 min | 155.22 MB | `230f3c07...` |
  | `branch` binary (`b5dc72c0a`, reusing compiled deps) | 47.6 min | 63.67 MB | `f40805a7...` |
  | `main` binary (`791701e2f`, clean target dir) | 59.0 min | 63.61 MB | `5c50ca85...` |

- **Arms**:
  - `main` `791701e2fc11cad336815faac558b21e41387140`, `63,614,976` bytes,
    sha256 `5c50ca8591f96f8f842a318c38022dc84a8b7603d92a6aa844b5c40648996859`;
  - `branch` `b5dc72c0a9b88864d363087b68c4e5c154556958` (`c2a259f3a` + `main`
    `791701e2f`), `63,673,856` bytes,
    sha256 `f40805a700ce6cc27a4cb7fbae13e16c624453963d7b8cadc6e2ecc06dabf0ac`.
- **Fixture**: sha256 `bc7afaecf25faf98f6c21a2c1e477466da4f7759a7df9bd91d8b48836fa7c5b8`,
  generated at `b5dc72c0a` from a clean tree (`git_clean = "true"`) by the
  checked-in-profile lib test executable (`230f3c07...`) with
  `--seed 2026091601 --updates 3 --games 8 --warmup-updates 0 --pool-members 2,18`:
  - Affinity vs Terror, synthetic net-8 FlatV4 fresh learner source, 1
    collection worker, 1 preparation worker.
  - 18 distinct valid checkpoint containers (`pool/member-00..17`, ~14.8 MB
    each) over shared import descriptor `6516cf3e...`.
  - `pool2` (config sha256 `c726200e...`) draws uniformly from `member-00..01`;
    `pool18` (config sha256 `1e9eee94...`) draws from `member-00..17` (7
    distinct members in iteration 0, 6 in iteration 1, 8 in iteration 2, and 13
    distinct across the 3-update run).
- **Order**: for `aa` and `ab`, per case (`pool2`, then `pool18`), 1 untimed
  warm-up per arm, then 2 A-B-B-A blocks (4 timed samples per arm), then (`ab`
  only) 1 cold resume per arm (`--max-new-iterations 1` followed by a fresh
  continuation process), with `--no-cache-diagnostics`. For `counters`, 1
  A-B-B-A block (`branch-a` vs `branch-b`, 4 samples per case) with cache
  diagnostics enabled (`MTG_KERNEL_CACHE_DIAGNOSTICS_V1`) to collect per-update
  counters and verify cross-sample counter repeatability.

## Correctness

The checked-in-profile lib test executable passed all 8 required filters from
#608:

| Filter | Passed | Failed | Ignored |
|---|---:|---:|---:|
| `opponent_cache` | 7 | 0 | 0 |
| `collection_cache_boundary_rejects_same_path_mutation_of_all_pins` | 1 | 0 | 0 |
| `verified_bytes_cache_v1::tests` | 13 | 0 | 0 |
| `phase1_preparation_rejects_stale_and_corrupted_opponent_before_learning` | 1 | 0 | 0 |
| `pool_fixture_v1` | 4 | 0 | 0 |
| `cache_diagnostics_v1` | 6 | 0 | 0 |
| `loaded_source_profile_v1` | 1 | 0 | 0 |
| `fixture_bench_v1` | 3 | 0 | 1 |

`cargo fmt --all -- --check`, `git diff --check`, `tools/native_fixture_bench/tests`
(16 passed, 0 skipped on Windows), and `tools/w5/tests/test_w5_pgrow.py` (21
passed) also passed cleanly.

All 20 A/A samples, all 24 A/B samples (including cold resumes on `pool2` and
`pool18`), and all 8 `counters` samples completed all 3 iterations durably with
`parity_ok = true`. Each completed run tree has 48 files:

- Between samples of the same build, 35 files are byte-identical; the other 13
  differ only in 21 timing fields and 12 pins of those files.
- Across builds (`main` vs `branch`), 34 files are byte-identical; `run.json`
  additionally differs in 2 provenance fields (`git_commit` and
  `tracked_tree_sha256`, while `implementation_sha256` `adeba0f2...` is
  identical).

Checkpoint SHA-256, output-identity SHA-256, and state SHA-256 matched at all 3
iteration boundaries across every sample of `aa`, `ab`, and `counters` (cold
resumes included):

- `pool2` iteration 2 boundary: checkpoint `ab7c3b4d...`, output identity
  `f9073a63...`, state `14b134e1...`.
- `pool18` iteration 2 boundary: checkpoint `c26e3a5c...`, output identity
  `c3a76413...`, state `7d88bed6...`.

## Timing

Each figure below is the median of 4 timed samples per arm (2 A-B-B-A blocks
with `--no-cache-diagnostics`) over the full 3-update, 8-game run:

| Case | Metric | main | branch | Ratio | Spread | Block ratios | Verdict |
|---|---|---:|---:|---:|---:|---|---|
| `pool2` | wall, s | 6.518 | 6.577 | 1.009 | 0.019 | 1.016, 1.001 | inconclusive |
| `pool2` | user CPU, s | 5.258 | 5.234 | 0.996 | 0.072 | 1.006, 0.996 | inconclusive |
| `pool2` | peak working set, MiB | 905.9 | 916.8 | 1.012 | 0.021 | 1.020, 0.995 | inconclusive |
| `pool2` | collection, s | 4.024 | 4.077 | 1.013 | 0.058 | 1.012, 1.004 | inconclusive |
| `pool2` | update, s | 1.748 | 1.760 | 1.007 | 0.051 | 1.022, 1.002 | inconclusive |
| `pool18` | wall, s | 9.373 | 9.197 | 0.981 | 0.070 | 0.985, 0.977 | inconclusive |
| `pool18` | user CPU, s | 7.680 | 7.719 | 1.005 | 0.061 | 1.000, 0.991 | inconclusive |
| `pool18` | peak working set, MiB | 1147.7 | 1139.2 | 0.993 | 0.023 | 0.993, 0.996 | inconclusive |
| `pool18` | collection, s | 4.981 | 4.724 | 0.948 | 0.096 | 0.965, 0.942 | inconclusive |
| `pool18` | update, s | 2.663 | 2.712 | 1.018 | 0.079 | 1.015, 1.047 | inconclusive |

Every metric verdict in both `ab` and `aa` is inconclusive. In the `aa` noise
check (`main` against itself):

- `pool2`: median wall `6.767` vs `6.792` s (ratio `1.004`, spread `0.099`),
  peak working set `906.1` vs `910.4` MiB (ratio `1.005`, spread `0.035`).
- `pool18`: median wall `9.298` vs `9.248` s (ratio `0.995`, spread `0.032`),
  peak working set `1147.9` vs `1150.0` MiB (ratio `1.002`, spread `0.192`).

As in T1's Windows qualification (#607), the very first execution of each newly
linked executable (`main-a` warm-up 0 in `aa`, `branch` warm-up 1 in `ab`)
took ~10.25 s wall and ~29 s job user CPU on `pool2` instead of ~6.6 s wall and
~5.3 s user CPU on subsequent spawns; warm-ups are excluded from timed medians.

## Opponent-cache and verified-bytes counters

The `counters` run executed one A-B-B-A block (`branch-a` vs `branch-b`, 4
samples per case) with `MTG_KERNEL_CACHE_DIAGNOSTICS_V1` enabled. Across all 4
samples of `pool18`, every event count in `opponent_cache`, `verified_bytes`,
`retained_files`, `io`, and `loaded_source` repeated identically; non-timing
byte totals differed by at most 22 bytes (from variable-length timing floats in
run JSON files recorded by the verified-bytes memo).

### `pool18` per-iteration and cumulative counters

Iteration 0 is the cold start, and the roster's encountered working set grows
from 7 to 9 to 13 distinct members across the 3 updates:

| Counter | Iter 0 | Iter 1 | Iter 2 | 3-update total / final |
|---|---:|---:|---:|---:|
| Distinct members drawn | 7 | 6 | 8 | 13 distinct (21 draws) |
| `OPPONENT_CACHE` full loads / trusted hits | 7 / 0 | 2 / 4 | 4 / 4 | 13 loads / 8 hits |
| Collection caches: `misses.verified` / `misses.unverified` (serial replay) | 7 / 7 | 6 / 6 | 8 / 8 | 21 / 21 |
| `verified_bytes` evictions (evicted MB) | 17 (222.2) | 13 (101.1) | 27 (272.8) | 57 (596.1 MB) |
| `verified_bytes` hashed files (hashed MB) | 25 (315.5) | 3 (29.6) | 16 (221.8) | 44 (566.9 MB) |
| Fresh-source pinned reads (MB) / checkpoint decodes | 209 (1018.6) / 33 | 89 (464.1) / 10 | 121 (607.4) / 14 | 419 (2090.2 MB) / 57 |

State at the end of `pool18` iteration 2:

- `OPPONENT_CACHE` holds 13 entries (limit 32): `256.39 MB` of captured file
  copies (`197.18 MB` unique within `OPPONENT_CACHE`), `54.53 MB` of owned
  policy `f32` bytes, and `73.45 MB` across 13 shared models.
- `verified_bytes` holds 20 entries at `167.60 MB` charged against its
  `167.77 MB` (`160 MiB`) budget.
- `retained_files` across both caches totals `423.99 MB`, of which only
  `241.53 MB` is unique (`182.45 MB` duplicated).
- Peak working set (`max_rss_mib` on the Windows job object) is `1139-1148 MiB`
  (`~1.15 GB`) for 13 active members, leaving ample headroom on 32+ GiB hosts
  and matching the cloud dry-run's ~1.2 GB RSS envelope.

### `pool2` cumulative summary (3 updates)

On `pool2` (2 distinct members), `OPPONENT_CACHE` performs 2 full loads in
iteration 0 and serves 4 trusted hits across iterations 1-2 (`file_bytes =
39.44 MB`, `policy_bytes = 8.39 MB`, `model_bytes = 11.30 MB`), while serial
replay (`--preparation-workers 1`) still records `collection.misses.unverified =
6` (2 per update). `verified_bytes` reaches `159.41 MB` charged with 11
evictions (`71.95 MB`), and `io` totals 147 pinned reads (`749.08 MB`) and 12
checkpoint decodes (`177.46 MB`).

## Milestone 3 baseline takeaways

The checked-in-profile run confirms the three #599 optimization targets with
zero measurement regression:

1. **Serial replay bypasses `OPPONENT_CACHE`**: with `preparation_workers = 1`,
   update replay uses an unverified `OpponentCacheV1::default()` and reloads
   every distinct opponent per update (`collection.misses.unverified = 21` on
   `pool18`, `6` on `pool2`).
2. **Verified-bytes memo churns behind opponent-cache hits**: `verified_bytes`
   saturates its 160 MiB cap during iteration 0 of `pool18`, evicting 57
   entries (`596.1 MB`) and hashing `566.9 MB` even for members already held in
   `OPPONENT_CACHE`.
3. **Duplicate retained file copies**: `OPPONENT_CACHE` retains `19.72 MB` of
   raw checkpoint + descriptor bytes per cached opponent (`256.39 MB` across 13
   entries), duplicating `verified_bytes` and inflating per-process memory.
