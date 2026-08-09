# Bounded-staleness async production integration: real-trainer A/B dry run

Status: **dry run only, not a qualification campaign.** Per the production-
integration task's own scope ("a short matched sync-vs-async run... this is
the qualification artifact's dry run, not the qualification itself"), this
document records the raw output of the two small real-trainer A/B
comparisons already checked into the branch as ignored tests
(`bounded_staleness_async_production_v1::tests::
real_trainer_sync_and_async_arms_produce_identical_learning_trajectories`
and `...::real_trainer_matched_ab_reports_speedup_under_a_realistic_consumer_step`),
run in the foreground and captured verbatim below. It does not adopt the
async path for any campaign use; that decision, and any adversarial review,
comes later per the task.

Branch: `fable/async-production-integration-v1`. Both runs use the real
trainer (`NativeTrainerStateV2::run_even_batch_update_v2`: real "Burn"
mirror-deck rollout, real scoring, real gradient step, real Adam update),
eight updates, base seed 71,501, `max_staleness_updates = 2`. See
`bounded_staleness_async_production_v1.rs`'s module doc and
`REAL_TRAINER_HARNESS_LIMITATIONS_NOTE_V1` for the precise, honest boundary
of what this integration does and does not yet prove (it qualifies the
scheduling mechanism and its staleness ledger against the real trainer, not
the full rollout/gradient-decomposed throughput unlock).

## Run 1: default dry-run config (20ms synthetic consumer step)

`RealTrainerHarnessConfigV1::small_dry_run_v1()`. The 20ms synthetic
consumer step is negligible next to the ~5s real per-update compute at this
scale, so no overlap is expected or observed -- this run is the
learning-trajectory-equivalence proof, not a throughput demonstration.

```
$ cargo test --lib real_trainer_sync_and_async_arms_produce_identical_learning_trajectories -- --ignored --nocapture

running 1 test
test bounded_staleness_async_production_v1::tests::real_trainer_sync_and_async_arms_produce_identical_learning_trajectories has been running for over 60 seconds
DRY_RUN_REPORT sync.wall_time=43.6108743s
DRY_RUN_REPORT bounded_staleness_async.wall_time=42.9001227s
DRY_RUN_REPORT speedup_ratio=1.0165675889780148
DRY_RUN_REPORT sync.updates_per_second=0.18344048653938588
DRY_RUN_REPORT async.updates_per_second=0.1864796531222975
DRY_RUN_REPORT sync.loss_bits_by_update=[1064195456, 1063414533, 1059728334, 1053311763, 1040615182, 1058209395, 1055321600, 1067561080]
DRY_RUN_REPORT async.loss_bits_by_update=[1064195456, 1063414533, 1059728334, 1053311763, 1040615182, 1058209395, 1055321600, 1067561080]
DRY_RUN_REPORT sync.mean_learner_return_by_update=[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]
DRY_RUN_REPORT async.mean_learner_return_by_update=[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]
DRY_RUN_REPORT async.max_observed_staleness=Some(2)
DRY_RUN_REPORT learning_trajectories_match=true
test bounded_staleness_async_production_v1::tests::real_trainer_sync_and_async_arms_produce_identical_learning_trajectories ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1172 filtered out; finished in 86.68s
```

(loss_bits_by_update / mean_learner_return_by_update are `f32::to_bits` /
plain-f64 dumps of the real per-update loss and mean learner terminal
return; the earlier engineering pass's first run of this same test, before
this report existed, recorded sync=39.56s async=40.21s ratio=0.984 -- the
two runs' ratios (1.02 here, 0.98 there) bracket 1.0 as expected: at this
step size there is nothing to overlap, so the ratio is pure machine noise,
not a directional effect.)

## Run 2: realistic consumer step (3s synthetic consumer step)

`RealTrainerHarnessConfigV1::small_dry_run_realistic_consumer_step_v1()`.
The 3s step stands in for nontrivial consumer-side work a production loop
would actually do between updates (Store commit, checkpoint I/O,
evaluation), large enough relative to per-update compute for the scheduler's
overlap to show up in wall time.

```
$ cargo test --lib real_trainer_matched_ab_reports_speedup_under_a_realistic_consumer_step -- --ignored --nocapture

running 1 test
test bounded_staleness_async_production_v1::tests::real_trainer_matched_ab_reports_speedup_under_a_realistic_consumer_step has been running for over 60 seconds
REALISTIC_DRY_RUN_REPORT sync.wall_time=65.5616576s
REALISTIC_DRY_RUN_REPORT bounded_staleness_async.wall_time=44.9136585s
REALISTIC_DRY_RUN_REPORT speedup_ratio=1.4597265016832242
REALISTIC_DRY_RUN_REPORT learning_trajectories_match=true
test bounded_staleness_async_production_v1::tests::real_trainer_matched_ab_reports_speedup_under_a_realistic_consumer_step ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1172 filtered out; finished in 110.64s
```

(The earlier engineering pass's first run of this same test recorded
sync=62.95s async=42.52s ratio=1.48 -- consistent with this rerun's 1.46
within normal machine-load variance.)

## Reading the numbers

- `learning_trajectories_match=true` in both runs: `loss_bits_by_update` and
  `mean_learner_return_by_update` are exactly equal, index for index,
  between the sync and async arms. This is a structural property of this
  integration's design (see the module doc), not a statistical result: there
  is exactly one producer thread, it never forks and never replays from a
  stale snapshot, so both arms compute the identical deterministic update
  sequence; only wall-clock pipelining differs.
- `speedup_ratio` differs sharply between the two runs (Run 1: 1.02,
  bracketed by a repeat run's 0.98, i.e. noise around 1.0; Run 2: 1.46,
  consistent with a repeat run's 1.48) purely as a function of how large the
  synthetic consumer step is relative to real per-update compute cost, not
  any change to the mechanism. Run 2 demonstrates the mechanism delivers a
  real, non-trivial wall-clock benefit once the overlapped work is
  non-trivial; Run 1 demonstrates the mechanism costs nothing extra (no
  measurable regression, ratio indistinguishable from 1.0 across two
  independent runs) when there is nothing to overlap.
- `async.max_observed_staleness=Some(2)` in the async arm confirms the K=2
  bound was genuinely exercised (the producer got measurably ahead of the
  consumer's acknowledgement at least once), not vacuously satisfied at
  K=0. As documented in the module, in this single-producer,
  never-forked design this value reflects consumer-acknowledgement lag, not
  actual weight-lineage staleness (that distinction only becomes meaningful
  once rollout is decomposed from the gradient step, which remains future
  work).

## Reproducing

```
cargo test --lib bounded_staleness_async_production_v1:: -- --ignored --nocapture
```
