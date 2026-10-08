# Periodic checkpoint publication: exact replay verified

Saving every 16 updates plus the final update preserves the tested learned states and optimizer continuation. Both semantic learners passed byte-identical comparisons against the already completed dense run, including a periodic save at update48, an off-cadence final save at49, and resume from48 with four workers. This removes the requirement to publish every update in the budget-diagnostic path. It does not qualify a new substantial run or establish playing strength.

Implementation commits: `4bc8336e`, `694e1514`. Binary SHA256: `6d30da756f8acb6d4ed061ea87278f062ca2f65d2a2d079cbd0c8f3aaef624fe`. Evidence: `E:/mtg-meta-recovery-20260921/checkpoint-cadence-engineering-001/completion.json` and `manifest.json`; native receipts and checkpoints remain at `D:/mtg-checkpoint-cadence-engineering-001`. Runner: `E:/mtg-meta-recovery-20260921/check-checkpoint-cadence.py`, pinned before execution. Build002 completed in146.624 seconds with four BelowNormal Cargo jobs; build001 failed on a variable-name collision and remains preserved.

## Behavior and verification

The existing dense default remains interval1. Budget mode also accepts16; other intervals and sparse use in older campaign modes are rejected. Every update still computes and hashes the full state, retains the complete Adam state, and records its learning receipt. Only durable checkpoint publication is less frequent. The existing atomic publisher is unchanged. The throughput receipt binds checkpoint cadence as well as executable, inputs, source, design and placement. A prior dense qualification cannot authorize a sparse substantial run.

For each of beta0 and beta0.5, the bounded check ran ten updates across separate replay branches, not a new training trajectory: dense32-to34; sparse32-to34 with one and four workers; resume33-to34; sparse47-to49; and resume48-to49 with four workers. Final checkpoint bytes match the original dense checkpoints, including all parameters, optimizer moments, age and checkpoint metadata. Dense versus sparse update receipts and training scores match; resumed49 training scores match continuous49. Periodic48 and final49 both match their independent saved references. No validation positions were read.

Four negative checks passed: sparse128 without qualification, a qualification for the wrong cadence, unsupported interval8, and publication replay starting at46. The dedicated publication replay command requires a pinned final reference, fixed endpoint49 and predecessor47 or48; it cannot advance arbitrary successive batches. Full-reference equality is required before final publication.

## Measured costs

Seconds below cover the same two updates33/34. Sparse publishes once, dense twice. These are bounded sequential engineering samples, not randomized performance estimates.

| Arm and setting | Forward preparation | Update and checks | Snapshot/publication | Total update stages |
| --- | ---: | ---: | ---: | ---: |
| beta0, dense, one worker | 0.008 | 0.061 | 0.337 | 0.405 |
| beta0, sparse, one worker | 0.008 | 0.064 | 0.185 | 0.257 |
| beta0, sparse, four workers | 0.008 | 0.052 | 0.186 | 0.246 |
| beta0.5, dense, one worker | 0.186 | 0.791 | 0.351 | 1.327 |
| beta0.5, sparse, one worker | 0.186 | 0.792 | 0.189 | 1.167 |
| beta0.5, sparse, four workers | 0.188 | 0.563 | 0.186 | 0.937 |

Snapshot/publication includes state snapshot, policy replacement, hashing and optional durable publication. The update/check stage includes more than backward computation. These boundaries do not support a pure serialization or pure backward speedup claim.

A32-to128 continuation would publish six checkpoints per arm (48,64,80,96,112,128), instead of96: 12 rather than192 across two arms, a93.75% reduction in checkpoint count. Copying those12 already completed checkpoints fromD SSD toE HDD, including source and destination hash verification, took0.386 seconds with one copy worker and0.113 with four, for249,227,627 bytes. Sources were just verified before timing and cache effects are substantial. This is not a forecast of durable full-run recovery, not evidence of sustained HDD bandwidth, and cannot be compared directly with the previous113.098-second recovery as an achieved speedup. No completed evidence was deleted.

Both PCs had no active native research worker after the check; seven idle human sessions were preserved. No paid allocation or new substantial training occurred. The old launcher still assumes dense checkpoint counts and forecasting. A successor must use a new launcher that verifies the declared sparse checkpoint set and qualifies the actual workload, storage/recovery and eligible placements before dispatch. No global launcher migration is claimed.

## Scientific next action and review gap

The fixed128 semantic controls both fit32/32, resolving the earlier control-underfitting obstacle. The next scientific question is whether correct-label teaching with retention still passes an equal128-budget comparison on a distinct evaluation panel. The failed32-update comparison stays closed; neither incorrect-label diagnostic is a playing candidate. Do not rerun the completed semantic controls or select intermediate checkpoints. A successor needs explicit lineage reuse, fresh evaluation, final-only gates and later current-engine whole-match evidence.

Independent Fable review remains missing: the recorded September19 consultation failedHTTP429 with zero source reads, reset September22 at07:00EDT. It was not retried or counted as endorsement. This reversible engineering work proceeds under the maintainer's assigned execution authority; scientific design and generalization remain independently unreviewed. CP7 outcomes are excluded.
