# Phase 1 execution instructions

Jack explicitly assigned implementation of Phase 1: Pauper Meta Competence with RunPod Training Bursts on September 13, 2026. Implement and test the infrastructure. The accepted plan also explicitly leaves the campaign paused: keep campaign training, formal measurements and paid allocation paused until Jack resolves that launch scope. Work in this assigned worktree; preserve other owners' checkouts and all frozen measurements.

## Mandatory useful-compute policy

Use all eligible capacity that increases completed useful work, and permit no unexplained idle paid compute. Never create artificial load or unplanned experiments to increase utilization readings.

- RunPod must support complete training streams: collection, update, checkpoint and continuation. Keep a learner and its collectors on the same host.
- Qualify a fixed local/cloud workload including setup, transfer, storage and recovery. Require at least 1.5x end-to-end speedup before a substantive acceleration burst.
- Target at least 90% of eligible CPU capacity while CPU-ready work exists. Measure actual CPU quota/allowed cores. Two consecutive 60-second windows below target require a recorded diagnosis and correction, resizing or release.
- Scale workers only when completed-work throughput improves and correctness/resource limits remain satisfied. GPU activity alone is not a qualification; verify actual forward/gradient/update/optimizer continuation and full-iteration speedup.
- Preserve synchronous batch semantics, fresh behavior state, deterministic episode order and one update after a complete batch. No speculative stale next-batch data.
- Use spare capacity only for already planned independent replicas or evaluations. Distinguish population throughput from speedup of one sequential learner.
- Five minutes without eligible work triggers recovery/export and Pod release. Sustained production performance below 1.2x the local reference after startup amortization stops new dispatch for diagnosis/resizing/release.
- Maintain telemetry for collection, update, checkpoint/I/O, CPU/GPU use, throughput, spend and unused-capacity reasons. Retain existing local memory/storage reserves and BelowNormal priority.
- Every rental needs a bounded lease guard independent of the workstation, armed before training, and verified release. Expose guard failure modes; never claim a platform guarantee that does not exist.

## Scope and budgets

The first qualification burst is capped at $10 including associated storage and recovery headroom, within the existing $200 authorized scope. Larger stages need a measured runtime/cost/funding notice before launch. Existing credit is not unlimited spending authority. Preserve old lease records; use new scoped state. Entry fees are separate.

Root owns Cargo builds and native execution. Agents may implement their assigned non-overlapping paths and run offline checks, but must not start Cargo, native engines, GPU work or cloud allocations without root assignment. Use E-drive target/temp/evidence storage and at most four BelowNormal build jobs. Formal runs preserve the existing GPU assignment and user reserves.

Preserve matched seeds, frozen gates, source/model identities, full optimizer state and crash-consistent publication. CP7 outcomes never select models or experiments. Engineering verification is not playing-strength evidence. Apply the standing Fable review requirement; a failed consultation is not endorsement or a new user-approval gate.

See docs/phase1_pauper_meta_competence.md for deliverables and accepted numerical defaults.
