# Same-decision policy drift and direct prevention-input check

All four fixed endpoints were compared on 240 schedule-selected archived games and 13,083 identical learner decisions. Their behavioral differences persist when the prevention inputs are zero. A separate fixed-checkpoint masking check shows that the learned prevention projection is active numerically, but changes none of the 95 exposed decisions' top-ranked actions in this sample. This is not evidence that the omitted information is unimportant to MTG, or that the models play well.

## Shared-input comparison

| Checkpoint pair | Top-action disagreement | Mean total variation | Mean forward KL |
| --- | ---: | ---: | ---: |
| Training 1 control / treatment | 3.41% | 0.0364 | 0.0450 |
| Training 2 control / treatment | 6.22% | 0.0597 | 0.1630 |
| Control 1 / control 2 | 6.18% | 0.0642 | 0.1239 |
| Treatment 1 / treatment 2 | 5.60% | 0.0520 | 0.0997 |

All six pairwise comparisons, source-schedule/source-arm/deck/pre-postboard summaries and trajectory-weighted alternatives are in `analysis.json`. These are descriptive comparisons on shared inputs, not independent-game confidence intervals or a measure of decision quality. The conditional evaluation crossover already established endpoint-dependent strength differences; these metrics do not explain which changed actions caused them.

The panel takes the last scheduled game in each own-deck / pre-postboard / seat / opponent-model stratum, both source arms and both training schedules, without selecting outcomes. There are 60 strata per schedule, at updates 170 through 199. All eight own and opponent deck labels occur, but only 38 ordered matchups; Gates has 16 archives versus 32 for each other own deck. Of 20,229 eligible genuine-choice rows, the deterministic at-most-64-per-trajectory sample contains 13,083. Autoregressive substeps are separate rows. Rare states and long trajectories are not uniformly represented.

Only 95 sampled rows in 27 archives have any active prevention feature. Counts by bit are W=0, U=1, B=44, R=47, G=24, cannot-prevent=0; bits can co-occur. The other 12,988 rows still show 3.41% disagreement for training pair 1, 6.21% for pair 2 and 6.15% between controls. In the verified state-only model, object projections are zero and the six zero state inputs add exactly zero, so these differences cannot be attributed to an immediate nonzero prevention projection on those rows.

The mean top probability is 92.20%, 91.47%, 91.22% and 92.13% for C1/T1/C2/T2. Their fractions of genuine choices with top probability at least 99% are 70.08%, 64.01%, 64.39% and 68.78%. The better focal endpoint within each evaluated pair is less concentrated on this shared panel. That association motivates investigating exploration/optimization, but does not establish harmful overconfidence: many correct MTG choices should be nearly deterministic. There is no untouched-parent comparison in this four-model audit.

## Fixed-checkpoint masking check

On exactly the 95 already sampled exposed rows, preserve each tensor, legal menu and checkpoint, then replace only the six auxiliary prevention bits with zero. These are synthetic diagnostic fixtures, not new natural games. Unmasked scores reproduce the original full-audit scores exactly; both disabled-input controls remain bit-identical under masking. Serial and four-worker checks produce identical files.

| Fixed model | Rows with changed logits / value | Changed top actions | Largest absolute top-probability change |
| --- | ---: | ---: | ---: |
| Control 1 | 0 / 0 | 0/95 | 0 |
| Treatment 1 | 95 / 95 | 0/95 | 0.0952 percentage points |
| Control 2 | 0 / 0 | 0/95 | 0 |
| Treatment 2 | 95 / 95 | 0/95 | 0.0211 percentage points |

The largest absolute logit changes are 0.00650 and 0.01915 for the two treatments. This supports a small direct effect on this sampled set, while ruling out a completely disconnected inference projection. It does not establish that full action distributions are identical, that the same stochastic samples would always occur, that prevention caused no training-path changes, or that unseen prevention situations are covered.

## Verification and cost

The fresh replay binary, SHA `b73c8ddd045f86f286fb490e16c48d9a20bafe05603d67794d73950bac45fc32`, was built from `ab34ab1c` in 113.0 seconds using four BelowNormal jobs and owned E-drive target/temp. The older replay executable cannot read state-only configs and was not used. Frozen trainers, evaluators, checkpoints and prior results were not changed.

Before comparison, all four before-final policies exactly reproduced 64 archived final-update logit/value rows each. Including duplicate models, that is 512 matching behavior scores. Four corrupt-value fixtures were rejected; four unused terminal/selected-deck metadata changes preserved rows. All final loaded model config/checkpoint pins, projection mode and Adam ages (legacy 32600/public 200) were checked. Full analysis verified every archive hash, expected selected-row index, menu length and finite diagnostic.

Jack had Kimi's active native test owners at qualification, so the existing whole-host guard excluded Jack. Haley was eligible; saved RunPod inventory remains HTTP403 with no new paid scope. On the same 24 archives / 1,241 choice rows, Haley's end-to-end serial/four/eight-worker times were 10.98/7.89/7.95 seconds, with byte-identical outputs. The supported full launcher verified pinned qualification and selected four workers. The very small four/eight difference is not a robust optimum claim, and whole-host exclusion is not a globally optimal shared-resource policy.

Full dispatch took 23.28 seconds: 2.49 staging, 17.77 execution and 3.02 recovery. The separate initial asset stage took 20.32 seconds. All 252 exported full-run files were reverified with zero mismatches. Correctness, timing and full dispatch together took 133.40 seconds, excluding initial stage/build/analysis and engineering time. The 12 telemetry samples missed the short native interval, so no measured full-run CPU occupancy claim is made. No two-minute idle-capacity interval occurred.

The supplemental mask check took 6.15 seconds of staging plus 8.44/8.03 seconds for serial/four-worker checks, with exact outputs and verified recovery. All our processes are terminal and reaped. Final inventories show no competing native owners on either PC; Jack is eligible for requalification of the next workload. This does not claim Kimi's test suite passed, only that those processes exited.

## Next research action

Do not extend the failed prevention treatment or promote an endpoint from these diagnostics. Prepare one opt-in exploration/optimization intervention in the current grouped terminal-GAE trainer, separately from stack/cost/observation changes. First establish loss normalization and analytic gradients on ragged menus and physical groups, zero-coefficient exact parity, actual optimizer continuation, and useful throughput. Freeze a bounded matched comparison before new outcome measurement. Entropy is a candidate, not an established cure; an anchor can preserve errors, and entropy can randomize already-correct play. The human-interface/adapter and stack-attribute lanes remain necessary and independent.

The current primary literature check uses [Rudolph et al., v4, May 2026](https://arxiv.org/html/2502.08938v4), whose entropy analysis supports testing regularization in imperfect-information games. Its games, exploitability computation and coefficient scaling are not ours, so no coefficient or MTG capability is imported. [Sokota et al.](https://arxiv.org/abs/2206.05825) provide a distinct regularized-game-learning approach; adding an entropy term here would not implement their magnetic mirror descent algorithm.

Fable's September 19 review failed HTTP429 with zero source reads until September 22 07:00 EDT. No repeated retry or endorsement is claimed. The independent-review gap remains explicit under Jack's execution authority. Original replication NO-ADVANCE remains. No CP7 selection, new training, paid compute, model promotion, human-strength claim or actual Jack game occurred in this diagnostic turn. Goal active; heartbeat paused.

Evidence roots: `E:/mtg-meta-recovery-20260921/prevention-policy-drift-001` and `E:/mtg-meta-recovery-20260921/prevention-feature-mask-001`. Full results are in each `analysis.json`; the first includes `policy-drift-summary.png/.svg` and pinned figure artifacts. Source runners: `ae3d54fd` and `63596ec9`.
