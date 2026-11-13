# S learner backward work (T4 milestones 2 to 4, #598)

On the saved S fixture (`s8`, W5's S with 8 episodes; the warmed batch has
578 learner rows), the complete two-worker update took **3.198 s** against
**3.703 s** on main (0.864x), and its learner **1.656 s** against **2.152 s**
(0.770x). With one worker the learner took **3.153 s** against **3.735 s**
(0.844x). Every trial matched the batch's serial reference exactly: the state
hash, the published container with both Adam moments, the checkpoint record
and the continuation read. Each change helps on its own. At two workers the
compact final block alone gives a 0.869x learner and the pipeline alone
0.888x.

Merging changes code that S updates run, with every bit unchanged: every
S backward gets the compact final block, and the parallel learner, which
runs only when a run sets more than one preparation worker, gets the
pipeline. Nothing changes which path or worker count a run uses. The times
here are container evidence, not a production speedup. Qualifying the
parallel learner on a native node and next-run adoption stay with #593, and
whole-node acceptance with T6 (#600).

| Fixture, two workers, 8 trials per arm | main s | candidate s | ratio | every trial faster |
|---|---:|---:|---:|---|
| Complete update | 3.703 | 3.198 | 0.864 | yes |
| Learner | 2.152 | 1.656 | 0.770 | yes |
| Behavior replay | 1.296 | 1.280 | 0.988 | no (unchanged code) |

- **Removed work:** the compact final block skips 283.1 million of the
  backward's 1,289.6 million GEMM multiply-adds per `gae-bench` update
  (22.0%), exactly the census's final-block object query rows, in all 44
  substeps. In the one-worker profile the final block's backward went from
  1.01 times the mean of the other two blocks' to 0.36 times.
- **Retained K/V work:** every key and value row stays live. The final block
  still runs 67.6 million QKV multiply-adds: the key and value thirds for
  every token (62.4 million) and the query third for the live rows (5.1
  million). QKV is now 36% to 41% of that block's backward, about as much as
  its FFN.
- **Pipeline:** helpers start once per update instead of 22 times, and the
  workers' idle time inside the parallel sections fell from 23.6 to 5.7 ms
  of worker time. In the profile build its replay took half again as long
  beside the other worker's recording, so there the two-worker gradient did
  not improve; the default builds carry the timing claims.
- **Remaining bottleneck:** the forward (36% of the serial gradient) and the
  first two blocks' backward (48% to 49%), over half of it their FFNs, at the
  existing kernels. At two workers, recording (the forward, and the backward
  without weight gradients) is 70% of the workers' busy time.
- **Memory:** term storage is reserved once per update and bounded at 64 MiB
  (or one slot's bound); the `gae-bench` update reserves 5.34 MiB. Peak RSS
  on the fixture at two workers rose from 305.2 to 318.8 MiB (+4.5%), or
  +2.9% with the allocator's purge delay set to zero. On `gae-bench` it fell
  from 102.5 to 85.4 MiB.

The synthetic `gae-bench` learner agrees in direction: at two workers 0.923x
for M3, 0.934x for M2 alone and 0.817x for both, which was faster in all 12
rounds. On this VM single repetitions vary by 38% to 97% within an arm, the
A-A control included, so every synthetic verdict is inconclusive under
`run.py`'s rule.

## Changes

- **Compact final-block backward (milestone 3, `a6cbb2e7`).** The final
  block's forward computes only its live query rows, the state token and the
  action tokens, because the heads read nothing else. Its backward still
  visited every token. When every Q, K and V value of the block is finite,
  the backward now visits only the live query rows. The FFN, LN2, projection
  and the query side of attention use compact row buffers. Every key and
  value row stays live, and the combined QKV weight is split into its query
  rows (live rows only) and its key and value rows (every token). Each
  skipped term is +0 times a finite value and every accumulator starts at
  +0 and only adds, so no bit changes. A non-finite Q, K or V keeps the full
  backward. A test shows the check is needed: the NaN that an unused object
  query carries into `dK = dS^T Q` would otherwise be dropped.
- **Pipelined parallel learner (milestone 2, `236fdc42`).** The parallel
  gradient spawned scoped helpers twice for every block of `2 x workers`
  substeps, once to record and once to replay. Between the two, the caller
  applied the block's additions and split the gradient into shards. It now
  starts its helpers once per update and runs phases. With room for two sets
  of one slot per worker, phase `k` records block `k` while block `k - 1`
  replays from the other set; otherwise one set alternates record and
  replay phases. A weight shard replays block `k - 1` in phase `k`, before
  block `k` in phase `k + 1`, and one task per block applies the non-weight
  additions substep by substep, so every parameter receives its terms in the
  serial order. The gradient's weight shards and non-weight ranges are
  computed once per update instead of once per block.
- **Bounded term memory.** Every slot reserves an analytic bound of the
  update's largest substep once (`TermBoundV1`), so recording never
  reallocates. All slots together reserve at most 64 MiB, or one slot's bound
  when one substep alone exceeds it (`TermPlanV1`). A decision whose tensor
  shapes differ reserves nothing, since the forward refuses it before it
  records.
- **Errors and panics.** Helpers report refusals in worker order and the
  first failing substep wins in substep order. A worker that panics ends the
  update with its panic after every helper has joined. The gradient and Adam
  state are untouched on every failure, because `train_step_gae_v1` applies
  Adam only after a whole gradient.
- **Profile.** `s-learner-profile-v1` reports phases, barrier waits, each
  worker's record, replay and additions time, and the final block's removed
  multiply-adds. `run.py summarize` prints them.

## Correctness

- All 32 `s_transformer_v1::` tests pass, including the issue's five exact
  tests and six new ones:
  - `live_final_block_matches_the_full_backward_bit_for_bit`: through both
    gradient sinks, ragged decisions from one object or one action up to 60
    objects and 16 actions, repeated cards, subnormal and signed-zero inputs
    and adjoints, a one-block model and warm reused scratch.
  - `non_finite_final_block_values_keep_the_full_backward`: an infinite or
    NaN query, key or value keeps every row, and the gradient keeps every
    bit of the full backward's.
  - `term_bound_covers_every_backward_record`: the analytic bound covers what
    a backward records, including shapes with few objects and many actions.
  - `pipelined_gae_matches_serial_bits_for_every_term_plan`: ragged groups of
    one to four substeps that cross block boundaries, shared cards and
    signed-zero advantages, for two sets, one set of several slots and one
    slot, at 2, 3 and 5 workers. Every loss and gradient bit matches serial,
    and the slots never hold more than the plan reserves.
  - `pipelined_gae_reports_the_first_failure_in_substep_order`: policy-row
    refusals, forward refusals (an index outside its table, or shapes that
    differ) and invalid groups, early, late and where a block ends while the
    previous replays.
  - `pipelined_gae_reraises_a_worker_panic_after_joining_its_helpers`: a
    helper or the caller panicking in any phase, then a clean update.
- `parallel_gae_preserves_full_adam_checkpoint` and
  `parallel_gae_reports_first_batch_error_without_updating_adam` still hold
  the full Adam checkpoint and the unchanged state after an error. The 3
  `s_update_fixture_v1::tests::` and 7 `expanded_deck_training_v1::s_learner_v1::`
  tests pass (1 existing ignored), and so does `tests/s_learner_profile_v1.rs`
  with the profile feature.
- `gae-bench seed=7 groups=32` ends at main's full state
  `7d577ed1a63e151d978bb090d3f118081c265b56b158d2095aa318db6b38a2ed` (from
  `d5e8442f…`) at workers 1, 2, 3 and 5, in every attempt of every build.
  `parity-dump seed=7 batches=1 groups=4` gives main's `files_sha256` (7
  files) for every candidate.
- Every fixture trial below is verified against the batch's reference
  update: after state `22730187b54bf2e9…`, container `6398ede7f65774c5…`,
  Adam step 2.
- `cargo clippy --release --locked --workspace --all-targets -- -D warnings`
  is clean, and so are CI's saved-input feature set (lib, its binaries, the
  profile test and the codec example) and that feature set's lib tests.
  `cargo fmt --all -- --check` and `git diff --check` pass.

## Complete update

`s_update_fixture_v1 bench fixture=s8 batch=warmed path=<path> workers=<n>
reps=1`, one process per trial, CPUs 2-3, four A-B-B-A rounds (8 trials per
arm). Times are means; "separated" means every trial of one arm beat every
trial of the other.

| Comparison | Path, workers | Update s, A / B | Ratio | Learner s, A / B | Ratio | Learner separated | Peak RSS MiB, A / B |
|---|---|---|---:|---|---:|---|---|
| main / M3 | serial, 1 | 6.671 / 6.074 | 0.911 | 3.735 / 3.153 | 0.844 | yes | 294.4 / 295.1 |
| main / M2+M3 | serial, 1 | 6.379 / 6.049 | 0.948 | 3.586 / 3.150 | 0.878 | no | 295.0 / 297.3 |
| main / M3 | prepared, 2 | 3.699 / 3.426 | 0.926 | 2.141 / 1.860 | 0.869 | yes | 305.5 / 312.6 |
| main / M2 alone | prepared, 2 | 3.731 / 3.457 | 0.926 | 2.144 / 1.905 | 0.888 | yes | 307.3 / 311.5 |
| main / M2+M3 | prepared, 2 | 3.703 / 3.198 | 0.864 | 2.152 / 1.656 | 0.770 | yes | 305.2 / 318.8 |
| M3 / M2+M3 | prepared, 2 | 3.452 / 3.133 | 0.908 | 1.875 / 1.600 | 0.853 | yes | 311.7 / 315.7 |

One worker never runs the pipeline, so the second row repeats the first.
Behavior replay is unchanged code; its ratios range from 0.972 to 1.040.
Under `run.py`'s rule (a difference larger than the larger arm's
`(max - min) / mean`) the two-worker learner gains are all outside the
spread, and so is the combined complete update (0.864 against 0.076). The
one-worker rows are not: one fast M3 trial widens the spread to 0.252,
although every M3 trial beat every main trial.

## Synthetic learner A-B-B-A

`run.py run` on `gae-bench seed=7 groups=32 reps=3` (44 substeps), CPUs 2-3,
12 A-B-B-A rounds per comparison and worker count, run after the fixture
trials with nothing else running. Times are the warm repetitions' means in
seconds per update, the ratio is B over A, and "spread" is the larger arm's
`(max - min) / mean` over its warm repetitions.

| A / B | Workers | A s | B s | Ratio | Median ratio | Rounds B faster | Round ratios | Spread | Peak RSS MiB, A / B |
|---|---:|---:|---:|---:|---:|---:|---|---:|---|
| main / main (A-A) | 1 | 0.2491 | 0.2367 | 0.950 | 0.980 | 8/12 | 0.771-1.257 | 0.971 | 57.3 / 57.3 |
| main / main (A-A) | 2 | 0.1408 | 0.1378 | 0.979 | 0.977 | 8/12 | 0.821-1.102 | 0.379 | 99.2 / 100.4 |
| main / M3 | 1 | 0.2453 | 0.2086 | 0.850 | 0.886 | 10/12 | 0.634-1.118 | 0.852 | 57.4 / 57.4 |
| main / M3 | 2 | 0.1368 | 0.1263 | 0.923 | 0.926 | 11/12 | 0.800-1.068 | 0.568 | 101.1 / 104.0 |
| main / M2 alone | 1 | 0.2346 | 0.2334 | 0.995 | 1.001 | 4/12 | 0.719-1.157 | 0.959 | 57.4 / 57.6 |
| main / M2 alone | 2 | 0.1369 | 0.1279 | 0.934 | 0.962 | 9/12 | 0.852-1.025 | 0.404 | 100.8 / 93.4 |
| main / M2+M3 | 1 | 0.2351 | 0.2122 | 0.903 | 0.892 | 9/12 | 0.720-1.119 | 0.864 | 57.4 / 57.2 |
| main / M2+M3 | 2 | 0.1354 | 0.1106 | 0.817 | 0.819 | 12/12 | 0.728-0.904 | 0.571 | 102.5 / 85.4 |
| M3 / M2+M3 | 1 | 0.2035 | 0.1973 | 0.970 | 0.984 | 7/12 | 0.789-1.179 | 0.697 | 57.5 / 57.2 |
| M3 / M2+M3 | 2 | 0.1262 | 0.1118 | 0.886 | 0.905 | 11/12 | 0.776-1.003 | 0.410 | 103.0 / 84.9 |

One worker never runs the pipeline, so "main / M2 alone" at one worker is a
second A-A control. An earlier pass of 6 rounds per comparison gave the same
direction at two workers (0.903 for M3, 0.908 for M2 alone and 0.840 for
both, against an A-A of 0.987) and is in the manifest.

## Attribution

Profile builds (`s-learner-profile-v1`) of main, M3 and M2+M3 on
`gae-bench seed=7 groups=32` (44 substeps), each the mean of 8 warm
repetitions, from the 12-round pass. Spans and counters cost time, and on
this VM a build's absolute times moved by up to a fifth between runs (main's
first two blocks' backward took 97.8 ms in the earlier pass and 77.7 ms in
this one). The tables therefore compare shares within each run, and the
default builds above carry the timing claims. Counts, bytes and
multiply-adds are exact and the same in both passes.

**One worker (compact final block).**

| Share within the one-worker profile | main | M3 | M2+M3 |
|---|---:|---:|---:|
| Final block's backward, against the mean of the other two blocks' | 1.01 | 0.36 | 0.33 |
| QKV, of the final block's backward | 15% | 41% | 36% |
| Forward, of the gradient | 32% | 36% | 36% |
| First two blocks' backward, of the gradient | 41% | 48% | 49% |
| Their FFNs, of the gradient | 22% | 26% | 26% |

- **Removed work.** The census counts 1,289.6 million backward GEMM
  multiply-adds per update on main and 1,006.5 million with M3. The 283.1
  million removed are exactly the final block's object query rows, in all
  44 substeps: 208.9 million in the FFN, 26.1 million in the projection,
  26.1 million in the query third of QKV and 21.9 million in attention. In
  the earlier pass the final block's backward fell from 51.2 to 17.3 ms,
  while the first two blocks' stayed at 97.8 and 97.6 ms.
- **Retained K/V work.** The final block still runs 67.6 million QKV
  multiply-adds: the key and value thirds for every token (62.4 million) and
  the query third for the live rows (5.1 million). Attention adds 4.2
  million for the live queries against every key. QKV is now 36% to 41% of
  that block's backward, about as much as its FFN.
- **Remaining bottleneck.** With M3 the forward is 36% of the serial
  gradient and the first two blocks' backward 48% to 49%, of which their
  FFNs are 26%. Both are plain GEMM work at the existing kernels.

**Two workers (pipeline).** Main and M3 record and replay in separate
drains; M2+M3 runs phases.

| ms per update, both workers | main | M3 | M2+M3 |
|---|---:|---:|---:|
| Gradient (wall) | 135.3 | 116.4 | 120.0 |
| Helper spawns | 22 | 22 | 1 |
| Record work | 181.4 | 159.0 | 162.3 |
| Replay work | 55.7 | 45.8 | 69.0 |
| Weight-shard visits | 275 | 275 | 550 |
| Idle inside drains or phases | 28.0 | 23.6 | 5.7 |

- **Pipeline.** Helpers start once per update, and the workers' idle time
  inside the parallel sections fell from 23.6 to 5.7 ms. Recording no longer
  allocates: the Linear recording spans took 2.1 ms with no allocation,
  against 3.9 ms and 58 allocations (9.7 MiB) in M3. All slots together
  retain 5.34 MiB of term storage.
- **Replay under overlap.** The pipelined replay took 69.0 ms of worker time
  for the same 477.6 million weight multiply-adds that M3's replay drains did
  in 45.8 ms. A block is two substeps instead of four, so the replay visits
  each weight shard twice as often, and each visit runs beside the other
  worker's recording; #621's Windows receipt also found work slower with two
  workers active. So in the profile build the two-worker gradient does not
  improve (120.0 against 116.4 ms, and 122.9 against 123.9 ms in the earlier
  pass), while the default builds, timed without spans, show the pipeline's
  gain. How much of it is the visits and how much the overlap was not
  isolated.
- **Remaining bottleneck.** Recording (the forward, and the backward without
  weight gradients) is 70% of the workers' busy time at two workers.

## T3 composition

T3 (#622) adds an explicit one-worker update that reuses each
authenticated replay tape for that row's backward (`path=serial
verified_replay_tape_bytes=N`). That backward is `backward_into_v1`, so the
compact final block applies to it unchanged. The pipeline does not, because
T3's command refuses more than one worker. This change keeps the interfaces
T3 uses: the validated forward, the `backward_into_v1` signature, the direct
gradient sink, the backward scratch type, the tape layout and its sizing.

These trials use a local merge of #622's head with this change, on the same
fixture and protocol, with T3's own serial update as the reference. Every
trial verified exactly. Every verified-tape trial reused all 578 learner
tapes with no fallback, with a peak retained tape of 2,363,192 bytes.
"Replay + learner" is the time T3 compares, since its verified update runs
the learner's backwards inside the replay.

| A / B | Workers | Update s, A / B | Ratio | Every trial faster | Replay + learner s, A / B | Ratio |
|---|---:|---|---:|---|---|---:|
| T3 serial / T3 verified | 1 | 6.568 / 5.515 | 0.840 | yes | 6.243 / 5.202 | 0.833 |
| T3 serial / T3+T4 verified | 1 | 6.610 / 5.048 | 0.764 | yes | 6.281 / 4.721 | 0.752 |
| T3 verified / T3+T4 verified | 1 | 5.605 / 4.983 | 0.889 | no | 5.240 / 4.662 | 0.890 |
| T3 serial / T3+T4 serial | 1 | 6.630 / 6.207 | 0.936 | no | 6.312 / 5.885 | 0.932 |
| T3 prepared / T3+T4 prepared | 2 | 3.900 / 3.260 | 0.836 | yes | 3.643 / 2.987 | 0.820 |

The two compose. T3's verified update alone is 0.840x of T3's serial update
on this fixture, and with this change 0.764x, near the product of 0.840 and
the compact final block's 0.889 on the verified path (0.747). That 0.889 is
not separated: one of its four rounds gave 0.994. On T3's tree the
two-worker prepared update is 0.836x with this change, and its learner
0.751x. The verified update uses one worker and the prepared update two, so
which one a run uses stays with #593 and T6 (#600).

## Borrowed activations

The issue asked to compare copied term operands with activations borrowed
for a bounded block. It was not built, because the copies are too cheap for
borrowing to pay. In the two-worker profile all 1,141 Linear recording
spans took 2.1 ms of worker time, with no allocation, for 26.6 MiB of
Linear inputs and output gradients. Only the inputs (12.0 MiB, 45%) are
activations a tape already holds. The output gradients live in backward
scratch that the next row overwrites, and LayerNorm gains and biases are
computed values. Borrowing would therefore save at most about 1 ms of worker
time per update, under 0.5% of the workers' busy time. It would also keep a
tape for every slot until its block replays, two more tapes at two workers
(the fixture's largest learner tape is 2.3 MB), and change the tape
interface that T3 (#597) reuses.

## Memory

- **Term storage.** `TermPlanV1` reserves every slot's bound once per
  update, and never more than 64 MiB in all unless one substep's bound alone
  is larger. The `gae-bench` update reserves 5.34 MiB across four slots,
  against 5.61 MiB that main's drains retained and 5.92 MiB with M3 alone.
  Tests check that the slots never exceed the plan for every plan shape.
- **Fixture peak RSS** (prepared, two workers, `VmHWM` over the update):

| Comparison | Default allocator, MiB | Ratio | Purge delay 0, MiB | Ratio |
|---|---|---:|---|---:|
| main / M3 | 305.5 / 312.6 | 1.023 | 264.5 / 269.1 | 1.017 |
| main / M2+M3 | 305.2 / 318.8 | 1.045 | 263.9 / 271.6 | 1.029 |
| M3 / M2+M3 | 311.7 / 315.7 | 1.013 | 268.6 / 271.4 | 1.010 |

  The purge-delay runs set `MIMALLOC_PURGE_DELAY=0` (2 rounds each; a build
  ran during the first two, so only their RSS is used). With it, M2+M3's
  rise over main is 7.7 MiB instead of 13.6 MiB; the difference is freed
  memory the allocator had kept. One worker shows +0.2% to +0.8%.
- **`gae-bench` peak RSS** at two workers fell from 102.5 to 85.4 MiB, and
  the profile's peak heap is unchanged (30.7 MiB on main, 30.8 MiB here).

## Identities

Main is `0d9f23cf4c9094319cb89da87f7e42e8ddde7f9f`. The compact final block
(M3) is `a6cbb2e760aa289b64c0c4e528bf6d0c88ff894d`, and both changes
(M2+M3) are `236fdc428fb7e9003160eba19b6bc7e8ea2db59b`. "M2 alone" is
`236fdc42` with one local line that sends `FinalRowsV1::Auto` to the full
final block. T3 is #622's head `5d77d5af8836e297c027cd35d7aa39b127867c14`,
and T3+T4 is a local merge of `5d77d5af` and `236fdc42`
(`7cf76b66b6047afba547166bbb8c44ae6039ad4f`), made for this measurement and
not pushed. Every binary's SHA-256 is in the manifest. The builds used Rust
and Cargo 1.94.1 (LLVM 21.1.8), x86_64-unknown-linux-gnu, GNU ld 2.42,
`--release --locked`, `CARGO_PROFILE_RELEASE_LTO=false` and
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, and mimalloc, the binaries'
allocator. The host has 4 vCPUs (Intel Xeon @ 2.10GHz) and 15.7 GiB. The
fixture is `generate shape=s episodes=8 collection_workers=4` with identity
`d56d9fb0545658a1…`, the same `s8` as #609 and #622. Main has since gained
#630 and #631, which change update preparation and SHA-512 but not the
learner: the learner times carry over, but the complete-update ratios are
against `0d9f23c`.

Raw samples, digests and the aggregate profiles are in the
[manifest](s_learner_backward_t4_cloud_20261111.json). Each `synthetic`
entry is a `run.py` manifest: saved as its own file, `python
scripts/experiments/s_learner_attribution_v1/run.py summarize <file>` prints
its tables.

## Remaining

- **Adoption.** Runs keep their paths: the runner uses the parallel
  learner only when a run sets more than one preparation worker. Qualifying
  it on a free native node and next-run adoption are #593, and whole-node
  updates per hour are T6 (#600).
- **Checked-in release profile.** These receipts use CI's profile. A receipt
  with the checked-in profile (thin LTO, one codegen unit) needs a larger
  host, as #621's did.
- **Remaining bottleneck.** The forward and the first two blocks' FFN
  backward are plain GEMM work at the existing kernels. Specialized GEMM or
  tail tuning (the issue's optional step) is the next lever if wanted.
- **Replay under overlap.** In the profile build the pipelined replay took
  about half again the worker time of M3's replay drains for the same
  multiply-adds, with twice the shard visits. Blocks of `2 x workers`
  substeps in each of two sets would restore main's visit count for twice
  the term storage, still far inside 64 MiB. This was not tried.
- **T3.** When #622 lands, its verified serial update runs this backward
  unchanged; the composition above is from a local merge.
