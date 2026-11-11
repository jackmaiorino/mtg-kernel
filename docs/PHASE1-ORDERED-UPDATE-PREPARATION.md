# Ordered update preparation

This opt-in execution path prepares one already collected, fixed V3 batch using independent physical-decision replay jobs. It does not change batch size, the terminal-return objective, behavior sampling, forward arithmetic, backward reduction order or Adam. Engineering parity and throughput measurements remain separate from playing-strength evidence.

## Interface and ordering

`ExpandedTrainingCommandV1::UpdatePrepared` serializes as `mode: "update_prepared"`. Its fields are the existing Update fields (`source`, `trajectories`, `learning_rate`, `value_coefficient`, `update_backend`, `output_directory`) plus `preparation_workers`. The CPU backend remains omitted when defaulted. `Update` retains its previous serialized shape and serial replay path. The runner uses `UpdatePrepared` only when explicitly configured with more than one preparation worker; omitted/default-one runner configuration preserves the old wire shape.

`validate_preparation_workers_v1(usize) -> Result<(), String>` accepts 1 through 32. One worker in the explicit command is useful as an engineering comparison for the new planner and opponent cache; it is distinct from the legacy `Update` reference.

The caller first reads pinned trajectories and validates the collecting state, source import, uniqueness, terminal outcomes and any registry-transfer schedule/scalars. Preparation again validates each trajectory in input order, including its original seat-specific sampling sequence. It then creates one job for each complete physical decision, retaining every substep in the group. The scoring API takes immutable policies and does not advance RNG. A group's result never depends on which worker ran it or on what that worker ran before: the only thing a worker keeps between groups is the exact row memo described below. Workers decode and replay only actor-visible tensors; opponent rows are checked as well as learner rows. Learner groups are returned in the original trajectory, physical-decision and substep order. Only after all workers join does the existing sequential training call perform one update.

Each distinct complete opponent source is loaded once per batch, in first-occurrence order. A source equal to the current learner source reuses a private fork of the already loaded learner. Actual full behavior identities, including state hashes and Adam steps, are checked against recorded seat identities. The model reference remains immutable throughout replay. No frozen opponent enters backward propagation and no next-batch work is collected under stale weights.

Ordinary replay errors remain attached to their original group ordinal. Monotonic dispatch stops new work beyond the earliest known failing ordinal while all lower in-flight jobs finish. Every worker is joined before returning, and the earliest group error is reported independently of completion order. Worker-start failures return after joining the workers that did start. Panic containment covers ordinary unwinding inside jobs, not process aborts or out-of-memory termination. Structural validation and safety-bound failures are preflight errors.

## Learner rows through the exact row memo

Collection scores net-8 decisions through an exact per-row memo of the five row layers (`ForwardRowCacheV1`): a row whose input bits were stored before is copied instead of computed, and the output is the same bit for bit. Consecutive decisions of a game repeat most of their rows, so that forward does a fraction of the reference forward's work.

By default the prepared replay uses the same memo, one per worker, for the learner's rows only, and only when the update runs a CPU learner step (`ReplayRowCacheV1::LearnerRows`). Every CPU learner step derives each learner row's logits and value again with its own tape forward and refuses any bit that differs from the stored row (`validate_forward_output_bits_v1`), so a learner row is still checked by a forward other than the one that produced it. The other seat's rows have no second check and keep the reference forward, unless the setting below asks otherwise. So do the CUDA backend (its learner compares within a tolerance), native S (it has no row memo), the serial `Update` path and every audit. `MTG_KERNEL_ROW_CACHE=0` turns the memo off everywhere.

Nothing else changes. Every replayed row is still compared with the stored rollout bits, with the same errors in the same order, and the groups, the telemetry and the update that follows are identical. A worker's memo holds at most 2,048 rows per row layer (about 9 MiB for net-8), is private to its thread and is dropped when preparation returns.

### The other seat's rows, by request

`MTG_KERNEL_ROW_CACHE_REPLAY_OTHER_SEAT=1` sends the other seat's rows through the memo as well (`ReplayRowCacheV1::AllRows`), again only when the update runs a CPU learner step. It is off by default, only the exact value `1` turns it on, and it is read once per process.

What it gives up is the independent forward for those rows. Each is still compared with the stored rollout bits, so a stored tensor or output that changed after collection is still refused, and so is an opponent whose parameters are not the recorded ones. But the forward that checks the row is then the forward that produced it, and no learner step derives it again: an error that the row memo made in collection and makes again in replay would pass. The other seat's rows never enter the learner step, so such an error could not reach the update's arithmetic. What it could hide is that the recorded opponent did not compute the decision the trajectory stores. `MTG_KERNEL_ROW_CACHE_CHECK=1` still compares every cached forward with an uncached one, in collection and in replay alike.

A worker then keeps a second memo, for the other seat's rows, so up to about 18 MiB per worker for net-8. The groups and the update that follows are the same bits with the setting on or off.

## Resource bounds and telemetry

| Bound | Scope |
|---|---|
| 32 workers | Each has an explicit 8 MiB stack reservation |
| 32 distinct opponents | One retained immutable inference policy per source, independent of worker count |
| 65,536 jobs | Complete physical decisions, never separately scheduled substeps |
| 256 MiB decoded tensor payload | Sum of actual numeric vector payload across all rows, checked before opponent loading or dispatch |
| 1,024 trajectories and 512 MiB input | Existing update ingestion bounds remain in force |

The decoded-payload bound is not a total RSS guarantee. Parsed input, vector headers, model storage, temporary forward buffers and the later learner's tapes are additional. The execution supervisor must still enforce the machine's memory reserve and measure peak RSS before scaling. More workers are not intrinsically more useful.

Only the explicit preparation path adds `update_preparation` telemetry to the update result. It separates validation/planning, opponent loading, behavior binding and parallel replay, and records job counts, retained sources, actual source load calls, current-model reuse, decoded payload, stack reservation and each worker's busy wall time. Busy wall time includes scheduling and any blocking; it is not CPU time or a utilization numerator. It must not be divided by affinity or advertised vCPUs to claim measured entitlement or useful CPU occupancy. Existing update-level stage timings remain available.

## Verification and limits

The `phase1_preparation` source tests cover:

- Exact ordered tensors and every parameter, first moment, second moment, Adam step, scorer anchor and full state hash through three successive real native updates at 1, 4 and 10 preparation workers. The tensors come from real actor-visible fixture decisions; the terminal/grouping setup is synthetic and is not a completed-game strength result.
- Repeated frozen-source loading once, current-model reuse without disk loading, and frozen-model immutability.
- Stale learner state, mismatched opponent state, corrupt opponent outputs and failed source loading before learning.
- Earliest-error ordering, simultaneous workers, full joining, panic containment and rejection before dispatch/loading at configured bounds. The payload-bound test deliberately allocates just over 256 MiB once.
- Legacy command serialization and compile-time immutable-sharing requirements.
- The row memo: the exact-state and rejection cases above run without it, with it for the learner's rows and with it for every row; a worker's state is made on its own thread and reaches only its own jobs; the memoized replay forward gives the reference replay's bits and errors for every stored row, serves no row of another model, and starts over when a policy's parameters are replaced; no row is memoized without a CPU learner step, and the other seat's rows only when the setting's value is exactly `1`.

The lead owns compilation and actual test execution. The initial suite compiled and five preparation tests passed. The exact-state test failed before replay or learning with `ParameterInvariant(card_embedding.weight)`: its opponent fixture changed element 8, inside the reserved 16-element zero embedding row. The fixture now uses `CARD_EMBEDDING_DIM_V1` to change the first real row and asserts that installed opponent identity changes. Production code was unchanged by this correction. Preserve the original failed receipt. The corrected suite passed all six tests in 50.01 test seconds, including all state bits through three updates at each of 1/4/10 workers. All nine runner tests passed, including preparation-mode binding during recovery. These results establish invariance on the declared unit fixture; actual complete-work qualification remains pending.

The runner defaults to the legacy Update command when preparation_workers is omitted or one, and dispatches UpdatePrepared only for an explicit value above one. Resume identity binds that configuration. Recovered updates must carry the matching preparation schema, requested count and started count equal to min(requested workers, physical groups); serial runs reject prepared receipts. Invocation progress output measures collection and update execution and their artifact validations separately, outside the immutable iteration receipts. These are wall times, excluding initial setup and validation of an already completed prefix.

Before accepting this path, complete the focused tests and existing expanded-training/registry-transfer regressions, then compare a fixed fresh multi-deck workload with legacy serial preparation versus explicit 1/4/10 workers. Compare every resulting full checkpoint state, trajectory semantics and recovery state. Use the same pinned Linux executable/runtime for local and cloud timing, and report total wrapper time, stage time, completed work, child CPU, peak memory and any unmeasured interval. Preserve the earlier Windows and Linux differences as their original evidence; this change does not establish cross-OS bit identity.

No acceleration claim follows from source changes, busy worker counts or synthetic test completion. The measured replay stage was only part of the end-to-end workload, so this change alone may not meet the 1.5x qualification threshold. Backward/Adam, checkpoint publication and remaining serial stages still contribute to elapsed time.

Fable's fresh focused review, session `de567b72-c390-4ad1-966a-566d67f2faf1`, failed at the weekly usage limit before any source read or substantive response. It provides no endorsement. Authorized bounded preparation continues with this independent-review limitation recorded; actual parity, recovery and throughput qualification remain outstanding acceptance evidence.
