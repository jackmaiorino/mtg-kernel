# Phase 1 parallel episode collection

`ExpandedTrainingCommandV1` now accepts an explicit `collect_parallel` mode with
the same `source`, `episodes` and `output_directory` as `collect`, plus a required
integer `workers` in 2..=1024. It starts at most one worker per scheduled episode.
The original `collect` mode and its serialized shape remain unchanged.

The native run scheduler selects this command only when its optional
`collection_workers` setting is greater than one. An omitted value retains the
serial path. Worker selection is execution configuration, not a changed batch,
model, seed schedule or numerical update backend.

## Semantics

- Validate one immutable learner checkpoint, including optimizer state, then
  fork private model/embedding copies and fresh inference buffers for workers.
- Each worker owns its learner policy and one-entry opponent cache. Physical-seat
  RNG streams reset from the scheduled episode seed before every game.
- Dynamically assign independent episodes through an atomic index. There is no
  shared inference lock and no update or next-iteration collection in the pool.
- Publish each completed trajectory under its original schedule index. Join pins
  in that same order, so the existing single learner update retains its original
  reduction order. Workers publish rather than retain whole completed trajectories
  while waiting for slower games.
- Join every worker before return. Errors or panics cancel new dispatch; active
  games finish and join. No `collection.json` is published for a failed batch, and
  completed episode artifacts are preserved for inspection. The existing scheduler
  restarts the incomplete phase from the same learner in a new attempt directory.
- A completed parallel collection has the existing collection schema and adds
  worker counts, initialization/elapsed timing and per-worker busy time. Those
  timings and destination paths are not byte-parity targets. Episode JSON bytes
  and the resulting optimizer/model state are the parity targets.

The initial implementation clones the validated learner once per worker. Fixed
opponents are loaded lazily into each private one-entry cache. A current-self
opponent forks the already validated learner instead of parsing the same checkpoint
again. Worker stacks reserve 8 MiB. External supervision remains responsible for
aggregate memory, storage, CPU quota, resource reserves and useful-throughput policy.

## Qualification

Root owns Cargo and native execution. No build, tests or game execution has been
performed by the implementation agent.

Focused tests use the `phase1_` filter and cover overlapping jobs, ordered join,
private policy/RNG state, cancelled failures, panic cleanup, invalid worker counts
and unchanged legacy command serialization. The opt-in
`phase1_parallel_real_episodes_match_serial` test plays four actual Burn mirrors
at one, two and four workers, including common-model and explicit-opponent modes.

Before campaign use, root must additionally compare the same prepared mixed-deck
batch and full update at serial and measured worker levels. Compare every episode
SHA, final parameters, optimizer moments, Adam step and state SHA; measure end-to-end
collection/update/checkpoint time and peak memory. Re-run an interrupted batch and
verify it resumes only complete collection/update phases. This change alone proves
neither speedup nor playing strength.
