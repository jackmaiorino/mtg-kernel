# Saved S update fixture

T3 ([#597](https://github.com/jackmaiorino/mtg-kernel/issues/597)) needs a
real replay-plus-learner workload that baseline and candidate builds can run on
identical inputs. `s_learner_collects_updates_and_runs_against_net8_opponents`
plays real S and net-8 games and checks the ordinary S update, but it deletes
its output and is correctness coverage, not a harness.
`expanded_deck_training_v1::s_update_fixture_v1` builds the same sources and
keeps the inputs: a synthetic runner-fixed net-8 shell and opponent, and a
pilot-initialized S whose initial container carries one Adam step that the
fresh start drops. The test now uses these builders.

## Batches

`generate` writes one new directory:

| Path | Content |
|---|---|
| `fixture.json` | manifest: options, S config, sources, batches, identities, census |
| `sources/` | net-8 descriptor and parameters, S initial container `s0.bin` |
| `initial/collect/` | episodes collected at the fresh start: Adam step 0, zero moments |
| `initial/reference/` | the serial reference update of that batch (step 1) |
| `warmed/collect/` | episodes collected at the `initial` reference's state: step 1, nonzero moments |
| `warmed/reference/` | its serial reference update (step 2) |

Every episode is the source test's Affinity-against-Terror game with an
explicit opponent source, as the run driver resolves assignments. With the
default two episodes the batches repeat the source test's run-driver
iterations: self-play and net-8 at the fresh start, then self-play and the
initial S at the warmed state. A third episode adds net-8 to the warmed
batch. Episode `i` of batch `b` uses seed `seed + 1000 * b + i`.

`shape=test` (default) is the source test's one-layer, eight-wide S.
`shape=s` is W5's S (`SConfigV1::s_v1`: 96 wide, three layers, four heads),
the representative forward and backward cost. The size limits are explicit:
at most 64 episodes per batch; each batch's trajectory bytes at most
`max_batch_mebibytes` (default and maximum 512, the update's ingestion bound);
each batch's decoded tensor payload, counted as a prepared update counts it,
at most `max_prepared_tensor_mebibytes` (default 256), which prepared updates
of the fixture then use. Generation checks both after collection and before
the reference update.

The manifest's census gives each batch's row counts by seat and opponent,
learner groups by substep count, learner rows' token counts and forward-tape
bytes (`STapeV1::bytes_for_counts_v1`, tested against the buffers a forward
sizes), the sum of those tape bytes, trajectory bytes and decoded payload.

## Commands

```bash
cargo build --release --locked -p mtg-kernel --bin s_update_fixture_v1 --bin expanded_deck_training_v1
F=/path/to/new/fixture
target/release/s_update_fixture_v1 generate out=$F
target/release/s_update_fixture_v1 generate out=$F-s shape=s episodes=8 collection_workers=4
target/release/s_update_fixture_v1 bench fixture=$F batch=warmed path=prepared workers=2 out=$F-bench-1
```

Before timing, `bench` checks the fixture's generated source and checkpoint
chain and recomputes the selected batch's census and game identity from its
pinned trajectories. This preflight reads the input files. `bench` then runs
`reps` updates (default 1) in its own process, each published
under `out/rep-NNN`, and writes `out/bench.json`: the census, every sample's
elapsed and process CPU seconds, peak RSS (Linux `VmHWM`, reset before each
update), the update receipt's input, replay, learner, checkpoint and total
seconds, the preparation timings, the executable's SHA-256, and the
verification. `path=serial` is the `Update` command: serial replay and the
one-worker learner, the reference path. `path=prepared workers=N` is
`UpdatePrepared`: ordered preparation and the learner with `N` workers, the
run driver's path for more than one. Repeated reps share process caches, so
timing trials run one process each.

`command` prints the update command of a batch and path for any build's
`expanded_deck_training_v1` binary, including baselines that predate this
module: defaults stay off the wire, so an older strict reader accepts it.
`verify` checks the published `update.json` of such a run:

```bash
target/release/s_update_fixture_v1 command fixture=$F batch=warmed path=prepared workers=2 out=$T/A1 > $T/A1.json
$BASELINE/expanded_deck_training_v1 $T/A1.json > /dev/null
target/release/s_update_fixture_v1 verify fixture=$F batch=warmed result=$T/A1/update.json
```

All four subcommands refuse unknown, repeated and out-of-range arguments.
`s_transformer_v1 gae-bench` is unchanged: it times synthetic learner work and
ignores unknown keys, so it is not a fixture reader.

## Verified serial replay

Add `verified_replay_tape_bytes` to `command` or `bench` with
`path=serial workers=1`. The cap is required for reuse and accepts 1 through
67,108,864 bytes. For example, use the representative fixture above.

```bash
target/release/s_update_fixture_v1 bench fixture=$F-s batch=warmed path=serial workers=1 out=$T/A1
target/release/s_update_fixture_v1 bench fixture=$F-s batch=warmed path=serial workers=1 verified_replay_tape_bytes=67108864 out=$T/B1
target/release/s_update_fixture_v1 command fixture=$F-s batch=warmed path=serial workers=1 verified_replay_tape_bytes=67108864 out=$T/B2 > $T/B2.json
target/release/expanded_deck_training_v1 $T/B2.json
```

The opt-in emits `mode=update_s_verified_serial` with a required
`tape_cap_bytes`. It accepts ordinary S GAE with zero entropy, one worker
and the fixture's default training math. The strict command rejects
prepared, multiworker, EX1 and W5 options. Omitting the opt-in produces the
original serial or prepared command. The native run driver still selects
those reference paths.

The opt-in receipt's `verified_replay.used` identifies a completed reused
update. On fallback the receipt gives the reason and the reference update
runs from the original state. The `attempt` fields report the candidate's
learner and opponent replay forwards, completed backwards and peak retained
tape capacity before forward. The cap includes one tape slot, retained
layer headers and tape buffers. Inputs, gradients, backward scratch,
transient allocations and allocator overhead are outside that cap.

Compare `verified_replay.combined_replay_learner_seconds` with the serial
reference's `behavior_replay_seconds + learner_update_seconds`. On reuse,
the replay timer includes gradient computation. On fallback, the combined
time includes both the discarded attempt and the reference replay and
learner. Compare the complete-update elapsed time separately and verify the
published checkpoint and continuation through `bench` or `verify`.

## Verification

An update matches its batch's reference when every field of the batch
identity is equal: the before and after state hashes, the published S
container's SHA-256 (parameters, both Adam moments and step, bit for bit),
the parameter hash, step, decision and substep counts, loss, policy and value
sums and terms, and the advantage statistics. The container is also decoded:
its state hash must be the reference's, and if its bytes differ the report
names which of parameters, first moments, second moments and step differ.
The checkpoint record must equal the reference's outside the container's own
path, and the run driver's continuation read of the new checkpoint must bind
the update's inputs and reach the reference state.

Ordinary episode files embed absolute paths
([#475](https://github.com/jackmaiorino/mtg-kernel/issues/475); #590's
content-addressed export packages native run chains, not this layout), so
trajectory pins differ between roots. A batch's `games_sha256` hashes each
episode's id, seed, behavior state, deck configurations, decision rows and
terminal instead, and the manifest's `identity_sha256` covers every batch
identity. A fixture is used where it was generated; readers refuse a moved
one. To move it, generate it again with the recorded options: the same build
gives the same `identity_sha256`, which the tests check across two roots.

## Forward counts

A build with `--features s-update-profile-v1` adds the `forward_profile` of
`s_transformer_v1::update_profile_v1` to each sample. It counts four stages: replay of
learner-seat rows, replay of other rows, learner forwards and learner
backwards. For each stage it records finished calls, summed thread time and
an order-independent digest of the forward outputs. The counters are armed
per calling thread and handed to the preparation and learner worker pools,
so concurrent updates never mix counts. Default builds compile them to
nothing; take uninstrumented timings from a default build.

For the reference paths, `counts_match_census` checks that every row was
replayed once and every learner row forwarded and backpropagated once. When
`learner_forwards_repeat_replayed_rows` is true, the learner's forward calls
and output digest equal the learner-row replay's. The learner then
recomputed the outputs the replay had already authenticated, with one
`duplicate_forwards` entry per learner row in the count. A count or digest
mismatch leaves the duplicate metrics unavailable and fails the benchmark.
The thread-time shares exclude hashing the outputs. Count one removed forward once; a shift
between the preparation and learner timers is not a saving.

Opt-in benches report `counts_match_execution` and
`removed_learner_forwards`. Reuse must authenticate every replay row and
backpropagate every learner row, with zero learner forwards. A fallback
retains all discarded replay and backward calls in the stage totals and
reports zero removed learner forwards. Compare the replay output digests of
the reused update and the serial reference on the same batch.

## Timing procedure

Use one fixture for baseline and candidate, with identical shape, seed, batch,
path and worker counts. Compare one worker first, then 2 and 4. Run A-B-B-A,
one process per trial, and retain every receipt. When a baseline predates
this module, give both arms commands from `command` with fresh output
directories. Time both update binaries with the same platform tool and
verify both results. For comparisons using `bench ... reps=1`, use the
same preflight and timing boundary on both arms. Record both SHAs, the
Rust and linker versions, the release profile and the machine. A
cloud container or CI-profile timing is not a qualified speedup; final
acceptance goes through [T6 / #600](https://github.com/jackmaiorino/mtg-kernel/issues/600).
Compare verified reuse with the one-worker serial reference. Record
fallbacks separately and retain their complete timing and forward counts.

The fixture uses synthetic weights and makes no playing-strength claim.
