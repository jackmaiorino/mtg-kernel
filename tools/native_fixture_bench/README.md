# Native fixture benchmark (#595)

An opt-in local benchmark of `native_expanded_training_run_v1` on a small
deterministic workload. It follows the common benchmark contract in #594 and
needs no private checkpoint, plan or fleet access. Standard-library Python 3.11+
on Linux or macOS (the driver uses `os.wait4`).

The workload is the source-generated net-8 fixture that the run's own tests use:

- synthetic, valid FlatV4 weights;
- the checked-in Affinity and Terror decks;
- ordinary self-play against the current learner (the C recipe);
- an explicit seed schedule: episode `g` of iteration `i` has seed `seed + i * games + g` and starting player `g % 2`.

It has two cases:

- `fresh` runs `--updates` iterations from the fresh source (Adam step 0).
- `warm` resumes a retained warmed checkpoint. The generator runs the first `--warmup-updates` iterations of the same schedule in process. The warm case then runs the next `--updates` iterations from that state.

## 1. Build the two arms

Build the baseline (for example `origin/main`) and the candidate in separate worktrees. Use the same toolchain, profile and flags for both. Name both binaries in the results.

```bash
cargo build --release --locked -j 1 -p mtg-kernel --bin native_expanded_training_run_v1
```

The checked-in release profile (thin LTO, `codegen-units = 1`) needs well over 15 GiB of RAM to build the crate's lib tests. On a smaller machine you can apply the same override to both arms and record it with `--build-profile`. Timings from such a build are not checked-in-profile results.

```bash
CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_INCREMENTAL=0
```

## 2. Generate the fixture

The generator is an ignored lib test of the candidate source, `native_expanded_training_run_v1::fixture_bench_v1::write_native_fixture_v1`.

```bash
python3 tools/native_fixture_bench/bench.py generate --root /abs/fixture \
  --seed 2026091601 --updates 2 --games 2 --warmup-updates 2
# or, with an already built lib test executable:
#   ... generate --root /abs/fixture --test-binary target/release/deps/mtg_kernel-<hash>
```

`--root` must be absent or empty. It receives:

- `source/` and `descriptor.json`;
- `cases/NAME/config.json`;
- `warmup/run/` and `warm/checkpoint.json`;
- `fixture.json`, the schema `mtg-kernel-native-fixture/v1`. It holds the options, the generator's build identity and every case's config pin;
- `generate/`, the command and logs. Generation time includes the warm-up and is not a sample.

Every case writes its run to the fixed `ROOT/work/NAME/run`.

## 3. Run the comparison

```bash
python3 tools/native_fixture_bench/bench.py run --fixture /abs/fixture --out /abs/results \
  --baseline main=/abs/main/native_expanded_training_run_v1 \
  --candidate branch=/abs/branch/native_expanded_training_run_v1 \
  --blocks 2 --warmup-runs 1 --cold-resume \
  --build-profile "release, checked-in profile" --label "machine, conditions"
```

For each case the driver runs, in order:

1. untimed warm-up runs, A then B;
2. `--blocks` A-B-B-A blocks;
3. with `--cold-resume`, a check for each arm. It stops after one iteration with `--max-new-iterations 1`, then finishes in a new process.

Every sample is a new process. The page cache is not dropped. `--plan-only` prints the order. After each sample, the driver moves the case's fixed run directory to `runs/CASE/NNN-KIND-POSITION-ARM/run`.

For every sample, the driver records:

- wall time, user and system CPU and peak RSS, from `wait4`;
- the run's own per-iteration `scheduler_timing` lines;
- the receipts' phase timings: collection, input read, replay and preparation, learner update and checkpoint I/O;
- durable completed iterations, the state at each boundary and storage bytes;
- the cache diagnostics file, from `MTG_KERNEL_CACHE_DIAGNOSTICS_V1`.

The driver sets `MTG_KERNEL_CACHE_DIAGNOSTICS_V1` for every sample. A build without that variable ignores it. `--env KEY=VALUE` and `--arm-env ARM:KEY=VALUE` pass extra settings to the samples.

Outputs in `--out` (absent or empty):

- `bench.json` (`mtg-kernel-native-fixture-bench/v1`) is the manifest. It records:
  - fixture and config pins;
  - both binaries' paths and SHA-256;
  - the host, the build profile note, the policy and the plan;
  - the SHA-256 of the outputs.
- `samples.jsonl` holds one raw record per sample, failed ones included. The sample directories keep the stdout, stderr and diagnostics files.
- `comparison.json` holds each case's comparison. Its metrics give the per-arm median, range and spread, the candidate/baseline ratio, the per-block ratios and the verdict. It also includes the parity results, the counters and the cold-resume results.

### Parity

Each case's first completed sample is the reference. Every later sample's run tree is compared with it file by file. Identical raw files are expected, because every sample writes to the same fixed path. A difference is accepted only in these cases:

- **timing**: a numeric field whose key ends in `_seconds`, `_nanos` or `_ns`, such as `update.json`'s `checkpoint_io_seconds`;
- **provenance**: `git_commit`, `tracked_tree_sha256` or `implementation_sha256`, and only between samples of different builds;
- **pin**: the `sha256` of a run file that itself differs only in accepted fields.

Any other difference is a mismatch and is listed, as is a missing or extra file. A mismatch also occurs when the per-iteration output identity or the checkpoint bytes differ. On a mismatch the driver exits 1. The checkpoints hold the full parameters and both Adam moments, and the trajectories hold the ordered decisions.

The cold-resume check covers a fresh-process continuation after the completion marker. It does not cover an interruption before the marker. That is the failpoint tests' job and the handoff regression's in #595 milestone 3.

### Verdict rule

For each metric, the verdict is one of these:

- **inconclusive**: there are fewer than two complete A-B-B-A blocks, or the candidate/baseline median ratio is within the larger arm's relative spread (max minus min, over the median), or the blocks disagree in direction;
- **improvement** or **regression**: otherwise.

Raw samples are always kept. Do not report the fastest sample.

To see the noise floor of a machine, run an A/A comparison: pass the same binary as both arms under two names. One such probe here, with one block and a build running alongside, showed a 12% "effect" between identical binaries. That is why one block is never conclusive.

`--keep-runs reference` deletes each compared run tree except the reference, to bound disk use.

## Tests

```bash
python3 -m unittest discover -v -s tools/native_fixture_bench/tests -p 'test_*.py'
```

The tests use a fake run binary and need no Rust build. The generator's own Rust tests are in `native_expanded_training_run_v1::fixture_bench_v1`.
