# Checkpoint codec benchmark v1 (mtg-kernel#596, milestone 1)

An opt-in benchmark of what an update spends publishing its checkpoint and
reading it back, stage by stage, for the ordinary (net-8) learner and the S
learner. It is measurement only: it changes no codec, wire format, default,
validation or cache. It is the baseline later T2 milestones (borrowed views,
streamed validation) are measured against. Milestone 2 (borrowed views)
changed what some ordinary stages do, not their names, so receipts of builds
before and after it compare row for row; see
[After milestone 2](#after-milestone-2-borrowed-views).

- Code: `mtg-kernel/src/expanded_deck_training_v1/checkpoint_codec_bench_v1.rs`
  and the binary `mtg-kernel/examples/checkpoint_codec_bench_v1.rs`, built only
  with the `checkpoint-codec-bench-v1` feature. Default builds carry neither.
- A-B-B-A runner: `tools/checkpoint_codec_bench/abba_v1.py` (standard library,
  Python 3.9 or later), tested by `tools/checkpoint_codec_bench/tests`.

Its numbers are stage costs on a generated fixture. They are not an
updates-per-hour, full-node or fleet claim: a faster codec stage does not by
itself establish a complete-update gain (mtg-kernel#594).

## Build

Timings use the checked-in release profile (thin LTO, one codegen unit):

```bash
cargo build --release --locked -p mtg-kernel --features checkpoint-codec-bench-v1 \
    --example checkpoint_codec_bench_v1
cp target/release/examples/checkpoint_codec_bench_v1 /abs/bins/bench-<sha>
```

Plan on a machine with about 32 GB of RAM for the checked-in profile: with it,
the library's test build was OOM-killed at 15 GiB, and this build compiles the
same library. On a smaller machine, correctness and smoke runs can use an override
such as `CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`
(about 15.5 GiB with swap, `-j 4`), applied identically to every build being
compared, and their timings must be labelled as coming from that profile.

## Generate a fixture once

```bash
checkpoint_codec_bench_v1 generate --fixture /abs/fixture [--out manifest.json]
```

`generate` writes, into a fresh absolute directory that must never move
afterwards (records pin absolute paths):

- the synthetic fresh FlatV4 source the update tests use (a real deterministic
  net-8 model with real hashes, `write_synthetic_fresh_source_with_parameters_v1`);
- a chain of `--warm-updates` (default 8) real `Collect` and `Update` commands,
  each collecting `--games-per-update` (default 2) self-play games of the
  checked-in Affinity and Terror lists with the update tests' settings (GAE
  gamma 1.0, lambda 0.9, learning rate 0.0003, value coefficient 0.5,
  sequential CPU backward, platform libm), published in the default format;
- the measured ordinary states: `update-1` (Adam step 1, nearly empty moments)
  and `warmed` (the chain's last state), each with the trajectories it collected,
  the inputs of a real next update;
- S train states (`s_transformer_v1`, W5's 460,346-parameter config):
  `s-initial` (the pilot initializer with a fresh optimizer) and `s-warmed`
  after `--s-warm-steps` (default 8) real GAE steps on synthetic batches of
  `--s-groups` (default 16) groups at the S4a mean shape;
- `fixture.json`, the manifest, with the generating build and machine.

The fixture is deterministic for a given seed and build. Generate it once and
measure every build against the same directory; a baseline and a candidate
measured on one fixture publish from identical inputs, so their record bytes
must be equal. The receipt reports each state's value counts and how many are
nonzero (moment density drives v1 JSON size and zstd ratio). The fixture's two
decks touch only part of the card embedding table, so a long run's saved state
can be denser; `measure --extra-state LABEL=SOURCE.json` measures any ordinary
source whose `checkpoint` pins a saved checkpoint (a maintainer's
representative state), alongside the fixture's.

## Measure

```bash
checkpoint_codec_bench_v1 measure --fixture /abs/fixture --work /abs/work-1 \
    --public-pair python/tools/tests/fixtures/checkpoint_container_v1 \
    --out receipt.json [--sections stages,updates,public-pair,s] [--reps 10] \
    [--warmup 2] [--update-reps 5] [--update-warmup 1] [--preparation-workers 2] \
    [--extra-state LABEL=SOURCE.json]... [--max-wall-seconds N] \
    [--verified-reads reread|resident] [--label TEXT]
```

The work directory must not exist; every repetition's publication is checked
and removed. `--max-wall-seconds` refuses to start a repetition after the cap.
`--verified-reads resident` turns on the process's resident verified reads
before anything is measured, as a run with `verified_reads: "resident"` does
(`native_expanded_training_run_v1`); the default, `reread`, is the production
default. The receipt records the setting, so the A-B-B-A comparison refuses
runs that differ in it.
The binary installs the production allocator (`FastAllocatorV1`, with
`retain_freed_memory_v1`, as `native_expanded_training_run_v1` does) wrapped in
relaxed atomic counters. The receipt reports requested allocation bytes and
counts, a sampled process-wide live-byte high-water mark above the starting
heap, and the live-byte delta. Concurrent allocator operations can overlap
the accounting. These are instrumented heap measurements, separate from
peak RSS. The same wrapper is used for both builds, and its work varies with
the number of allocations.

### `stages`: the ordinary publication and readback sequence

One repetition replays `execute_update_inner_v1` from `checkpoint_started`
through its v2 comparison, with the same calls in the same order, on the
fixture state. Cells: `v1`, `v1-zstd1`, `v2` and `v2-zstd1` (`auto` resolves
to `v2` for these checkpoints; the `updates` section measures it). Cells
rotate between repetitions.

| Stage | What runs |
|---|---|
| `snapshot` | `state.snapshot_v1()`; since milestone 2 `state.tensors_v1()`, the same validation and tensors borrowed from the live state instead of copied |
| `digest` | `snapshot.state_sha256_v1()`, a memo miss as after a real update; since milestone 2 the live state's `state.state_sha256_v1()`, the same digest of the same stream |
| `bits` | the checkpoint's three tensor sections and `record_format.resolve_v1`; since milestone 2 the sections borrow the state's tensors (`ParameterBitsRefV1`) instead of copying them into `ParameterBitsV1`s |
| `encode` | `serialize_json_v1` (v1) or `checkpoint_container_v1::encode_v1` (v2) |
| `seal` | `RecordEnvelopeV1::seal_owned_v1` (zstd1 compresses and reopens to verify) |
| `publish` | the output directory and `publish_json_shared_v1` (durable publication; the verified-bytes cache keeps the sealed allocation) |
| `readback_load` | `initialize_any_math_v1` on the new pin: the continuation loader |
| `readback_digest` | `restored.state_sha256_v1()` and the math check |
| `readback_v2_equality` | v2 only: `restored.snapshot_v1()` and the item-for-item comparison; since milestone 2 `restored.tensors_v1()` compared in place (`same_tensors_v1`), with no restored snapshot and no copy of each tensor's bits |
| `total` | the whole sequence |

The total's time and allocation counts include sample bookkeeping and work
between stages, such as constructing the readback source. Its heap peak
takes the highest measured stage peak. It can miss a peak that occurs
between stages.

Before each repetition the process digest memo (`state_digest_memo_v1`) is
emptied by hashing streams it does not hold, untimed, so `digest` misses as it
does after a real update. The readback then runs with whatever publication
left in the process caches, as in production: the verified-bytes cache holds
the published allocation and the digest memo the new stream; the loaded-source
cache misses because the pin is new.

After the sequence, the decode is broken down on the published file, each
call timed alone and outside `total`: `probe_read_pinned` (the read
`read_checkpoint_v1` makes, `read_pinned_shared_v1`), `probe_open`
(envelope), `probe_decode` (`decode_v1` or the JSON parse), and for v2
`probe_decode_structure` (`document_v1`, the structure check alone),
`probe_decode_reencode` (`encode_v1` of the decoded checkpoint) and
`probe_decode_compare` (the canonical byte comparison). `decode_v1` is the
structure check, the section copy, the re-encode and the comparison, so the
section copy is about `probe_decode` minus the other three. Since milestone 2,
`decode_v1` compares the re-encoding with the bytes as it is produced, without
a re-encoded copy; the two probes still time a copy and a comparison as
separate calls, so builds before and after report the same probes, and the
section copy is about `probe_decode` minus `probe_decode_structure` and one
streamed pass. Cell `any` holds
`probe_digest_miss` (a stream the memo does not hold) and `probe_digest_hit`
(the same stream again: a full comparison against the held copy).

Checks per repetition: the published file matches its pin and is
byte-identical across repetitions of the cell; the readback digest and math
match (and the v2 item equality); the record decodes to exactly the fixture
checkpoint's v1 JSON in the cell's format and envelope. Before measuring, each
state's reassembled checkpoint must equal the fixture's file byte for byte, so
the benchmark publishes what an update from that state publishes.

### `updates`: real updates per format

Real `UpdatePrepared` commands from each fixture state with its collected
trajectories, the same settings as the chain, `--preparation-workers` for every
cell (only `UpdatePrepared` takes an envelope). Cells: `v1`, `v2`, `auto`,
`v1-zstd1`, `v2-zstd1`, rotated. Before each update the source is loaded once,
untimed, so it runs from the loaded-source cache as a run's next update does.
Stages: `execute_update` (the whole command) and the receipt's own timers
(`receipt_update_elapsed`, `receipt_checkpoint_io`, `receipt_learner_update`,
`receipt_input_read`, `receipt_behavior_replay`; wall time only). Checks: every
cell reaches one after-state digest, every checkpoint opens (and exports) to
one v1 JSON document, and `auto` published the `v2` bytes.

### `public-pair`: the checked-in V1/V2 pair

`python/tools/tests/fixtures/checkpoint_container_v1` (a small codec example,
not an optimizer workload): `v1_parse`, `v1_serialize`, `v2_decode`,
`v2_encode`, `v2_export` and `v1_to_v2_convert`, each output checked against
the files byte for byte. Measured only when `--public-pair` is given.

### `s`: the S learner's codec

The S update's publication and `load_s_v1`'s readback, codec calls only (the
S checkpoint record JSON and the FlatV4 shell policy are not codec): `encode`
(`encode_checkpoint_v1`), `publish`, `readback_initial` (the loader decodes the
member's initial container on every load), `readback_read`,
`readback_decode`, `readback_digest` (digest, step and config checks),
`readback_model_clone` (the clone `install_s_model_v1` takes),
`readback_equality` (`restored.state == state`), `readback_digest_again` (the
update's second digest) and `total`; probes `probe_digest` and
`probe_decode`. S takes no envelope and is outside the v2 checkpoint format,
so neither is measured for it.

## Materialization inventory

What each stage copies at baseline `e8ad219656c72e505ba6b22af0944a46a60de908` (main with #592's shared
publication and resident reads),
for net-8 with N = 1,230,994 values per section (3N values, 14.8 MB as f32 or
u32). The receipt's `allocated_bytes`, `allocations` and `peak_heap_bytes` per
stage quantify each line.

Ordinary publication:
1. `snapshot`: an owned copy of parameters and both moments (3N f32).
2. `digest` (miss): SHA-256 over the stream, and the memo entry keeps its own
   copy of every value (3N f32) until two newer streams evict it.
3. `bits`: three `ParameterBitsV1` sections (3N u32) plus names and shapes.
4. `encode`: the v1 JSON text (receipt `v1_json_bytes`) or the container
   (about 3N × 4 bytes plus its document and table).
5. `seal` (zstd1 only): the compressed frame plus a full decompression to
   verify it (record-sized).
6. `publish`: the file write, fsync, SHA-256 of the bytes and link. The
   verified-bytes cache keeps the sealed allocation itself, not a copy (up to
   its 160 MiB budget). With resident reads, the publication also skips its
   two non-bracketing readbacks.

Ordinary readback (`readback_load`, a loaded-source cache miss):
7. The checkpoint read: by default (`reread`) the file is read (file-sized)
   and compared against the verified-bytes cache's held bytes instead of
   hashed; with resident reads, a metadata check and the held allocation
   itself, with no read and no copy. For zstd1, the decompressed record.
8. The parse: v1 JSON into 3N u32; v2 `decode_v1`: the structure check, a 3N u32
   copy out of the sections, then a full re-encode (container-sized) and a
   byte comparison.
9. The restore: 3N f32 (`restore_parameters`), the restored snapshot's digest
   (a memo hit: a full comparison against the held copy), the train state and
   the policy's parameter copies, and the loaded-source cache's clones of the
   policy and the state. The loader also reads and checks the
   fresh-initialization payload, the same for every format.
10. `readback_digest`: another memo hit (full comparison).
11. `readback_v2_equality` (v2 only): a second restored snapshot (3N f32) and a
    temporary u32 copy of each tensor for the comparison.

S (460,346 parameters, so 3 × 460,346 values, 5.5 MB per state), whose digest
is never memoized:
1. `encode`: the container, after hashing the state once for its document.
2. `publish`: as above.
3. `readback_initial` and `readback_decode`: each decode allocates the three
   sections, rebuilds the model's transposed weights, hashes the state, then
   re-encodes it (hashing again) and compares bytes: two hashes and a
   container-sized re-encode per decode, two decodes per load.
4. `readback_digest` and `readback_digest_again`: two more full hashes.
5. `readback_model_clone`: a full model clone for the shell policy.

A net-8 codec change does not by itself change any S stage.

### After milestone 2 (borrowed views)

Milestone 2 removes the codec's own copies from the ordinary sequence, and
the two snapshots taken only for it, without changing either format's bytes,
the readers or what they refuse:

- `snapshot` (item 1): the update reads the live state's tensors through
  `tensors_v1()` (T1's accepted view on #595), which validates the state as
  `snapshot_v1()` does and borrows the snapshot's tensors in the snapshot's
  order instead of copying them. The digest is the live state's own
  `state_sha256_v1()`, which hashes the same stream.
- `bits` (item 3): the published checkpoint is an
  `ExpandedCheckpointV1<ParameterBitsRefV1>`, whose tensors borrow the
  state's. The v1 JSON serializes through the same derived serialization
  (each value's `to_bits`, as `ParameterBitsV1` holds it) and the container
  encoder (`encode`, item 4) writes each value's little-endian bits straight
  from the state, so neither format copies the 3N values first.
- The parse (item 8): `decode_v1`'s second guard streams the re-encoding into
  a comparison with the bytes (`encodes_to_v1`), so it no longer holds a
  container-sized re-encoded copy. It compares the same bytes and refuses
  exactly the same files.
- `readback_v2_equality` (item 11): the restored state's tensors
  (`tensors_v1()`, no second snapshot) are compared with the published ones
  in place (`same_tensors_v1`), without a temporary `u32` copy of each
  tensor. The v1-to-v2 converter compares its two restored snapshots in
  place the same way.

What remains: the decode's `u32` section copy and the restore's `f32` copy
(items 8 and 9) need a train state built from borrowed tensors, which T1
accepted on #595 for milestone 3 together with its rework of `decode_v1`'s
validation. The digest memo's held copy (item 2) is milestone 4's, and the
restore's copies into the policy and the loaded-source cache (item 9) are
T1's. S is unchanged: its encoder already writes from the borrowed state and
its decoder reads straight into `f32`; its decode's re-encode (two hashes per
decode) is milestone 3's.

## A-B-B-A

Build the baseline and candidate binaries from their exact commits with one
profile (`git worktree add`, then the build above in each), copy them out of
`target/`, generate one fixture, then:

```bash
python tools/checkpoint_codec_bench/abba_v1.py run \
    --baseline-bin /abs/bins/bench-<baseline-sha> --candidate-bin /abs/bins/bench-<candidate-sha> \
    --baseline-build '{"source":"<baseline-full-git-sha>","toolchain":"rustc 1.94.1, pinned linker","profile":"release thin LTO, one codegen unit"}' \
    --candidate-build '{"source":"<candidate-full-git-sha>","toolchain":"rustc 1.94.1, pinned linker","profile":"release thin LTO, one codegen unit"}' \
    --fixture /abs/fixture --out-dir /abs/abba-1 --rounds 3 \
    -- --public-pair /abs/repo/python/tools/tests/fixtures/checkpoint_container_v1
```

Each round runs baseline, candidate, candidate, baseline, each a fresh
process, cold except for the OS file cache. Everything after `--` is passed to
every attempt. The run directory keeps `run.json` (binaries and fixture
pinned by SHA-256, `rustc -vV`, `cargo -V`, `cc`/`ld` versions, the machine,
and every attempt's command, exit status and wall time), each attempt's
stdout, stderr and receipt, and `comparison.json`. `compare --run-dir` reads a
finished run back (`--metric wall_ns|cpu_ns|allocated_bytes|...` for the
table). The build declarations are caller-supplied and recorded beside each
binary's hash. The declared profile and toolchain must match between arms,
and each available binary build source must match its declaration. Record the
exact build commands. The runtime tool versions record the measurement host
and do not attest which profile compiled the executables.

The runner compares only a complete A-B-B-A schedule of successful attempts.
Failed attempts keep their logs and any receipt they wrote. Every receipt
must bind the run's pinned fixture and use the same settings, checks, stage
rows, metrics, and published identities. Each real update must reach the same
after-state and checkpoint content across arms. Then each run contributes its
median per stage and metric, a role's value is the median of its runs' medians, and
the candidate is reported `candidate_lower` or `candidate_higher` only when no
candidate run overlaps any baseline run **and** a separation that complete has
chance probability at most `--alpha` (default 0.01) if the role made no
difference; otherwise `inconclusive`. With `n` baseline and `m` candidate runs
that probability is `2 n! m! / (n + m)!`: one round (2 + 2 runs) gives 1/3 and
can never decide, two rounds give 0.029, three rounds give 0.0022, so three
rounds are the fewest that can decide at the default. The table's first line
states the run counts and the probability, and its last line how many rows
were called against how many a null comparison would call by chance (rows ×
probability; there are about a hundred rows per metric). Running the same
binary as both roles (an A/A run, flagged `null_comparison`) measures the
run-to-run variation on the machine and how many rows it calls.

## Receipt (`mtg-kernel-checkpoint-codec-bench/v1`)

- `build` (git head and tree, clean flag, tracked-tree SHA-256, debug
  assertions, effective rustflags), `machine`, `config`, `fixture` (directory,
  manifest SHA-256, generation config and generator), `label`, `argv`.
- `states`: per measured state, its checkpoint pin, Adam step, state digest,
  value and nonzero counts per section, v1 JSON size, and the format `auto`
  resolves to.
- `series`: one entry per (section, state, cell, stage) with every timed
  repetition's raw sample (`wall_ns`, `cpu_ns`, `allocated_bytes`,
  `allocations`, `peak_heap_bytes`, `retained_heap_bytes`; a receipt's own
  timers in `updates` carry wall time only)
  and their median, quartiles, minimum and maximum. CPU time is the whole
  process's, so it includes other threads (Linux and macOS
  `CLOCK_PROCESS_CPUTIME_ID`, Windows `GetProcessTimes`).
- `published`: the SHA-256 and length every (state, cell) published.
- `checks`: the identity checks above, all `passed`.
- `section_seconds`, `peak_rss_bytes` at section boundaries (Linux `VmHWM`,
  Windows peak working set, macOS `ru_maxrss`) and `process_peak_rss_bytes`
  at the end.
