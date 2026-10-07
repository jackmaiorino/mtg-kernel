# The reversal follows the learned endpoints

The complete retrospective crossover shows the discovery treatment beating its control on both evaluation panels, while the replication treatment loses to its control on both. Evaluation-panel variation does not explain away the failed replication. This identifies an endpoint-dependent result; it does not establish an optimizer bug, an average treatment effect over training runs, or a model ready for human play.

| Training pair | Discovery panel: control / treatment wins | Treatment minus control | Replication panel: control / treatment wins | Treatment minus control |
| --- | --- | --- | --- | --- |
| 1 | 366 / 384 | +3.75 pp | 361 / 396 | +7.29 pp |
| 2 | 394 / 375 | -3.96 pp | 402 / 377 | -5.21 pp |

Each cell has 480 focal BO3 matches per endpoint. Diagnostic paired 95% intervals are respectively [+1.46, +6.25], [+4.79, +9.79], [-6.46, -1.67] and [-7.71, -2.71] percentage points. The diagnostic uses its own predeclared bootstrap seeds; the original discovery/replication analyses and their frozen gates remain unchanged.

On the same discovery panel, control 2 beats control 1 by +5.83 pp [+3.33, +8.33]; on the replication panel the difference is +8.54 pp [+6.04, +11.25]. Treatment 2 minus treatment 1 is -1.88 pp [-4.38, +0.63] and -3.96 pp [-6.67, -1.25], respectively. Ordinary continuation improved more in the second training schedule, while the feature treatment did not follow it.

The predeclared symmetric decomposition attributes -10.10 pp [-12.71, -7.71] of the diagonal contrast change to the fixed-endpoint component and +1.15 pp [-1.25, +3.54] to the panel component. Their point estimates sum to the original -8.96 pp reversal. These are conditional contrasts among four actual checkpoints, not a population estimate of training-seed variability. Both seats and all four endpoints stay paired within each resampled matchup/environment seed. Panels are resampled independently. No endpoint was selected for adoption.

Canonical BO3 treatment-control differences across the four cells are -1.02, +0.77, +0.77 and 0.00 pp, with all intervals spanning zero. Those small averages do not erase the previously observed canonical game-one decline or weak individual matchups. The original replication remains NO-ADVANCE. There is still no human or league strength claim.

## Coverage, integrity and cost

All 872 new four-match jobs completed: 3,488 new BO3 matches and 7,609 natural games. The analysis separately tags 3,488 reused diagonal matches and 7,624 reused natural games, for 6,976 endpoint-panel observations total. It verifies complete coverage, unique keys, requests, model pins, raw hashes, seat/start/seed alignment and natural-game counts before computing contrasts. The remote full export contained 9,596 files, all reverified with zero mismatches. There were no failed or unresolved scientific jobs.

Twelve completed allocation measurements executed 2,304 engineering matches on 192 unique original cases, all byte-identical to their original outputs, including cases from the middle of larger jobs. One the compute host serial attempt hit its 300-second group cap after 47 jobs and 189 match files. The final job had only 3.734 seconds remaining. A fresh retry completed in 304.43 seconds. The interrupted outputs were excluded, preserved and independently archived with 528 verified files and zero mismatches. No partial timing or outcome prefix was treated as complete.

Kimi resumed local native work during the remote-only qualification phase. The launch guard rejected the combined-PC preflight before spawning. Nine completed local measurements remain preserved; the two planned combined allocations are unmeasured. Fresh availability checks excluded the actively owned workstation, and the unchanged guard selected the compute host's fastest eligible completed allocation: 16 workers on its C SSD. This is not a claim that compute host is faster than an idle desktop PC, or that the guard's whole-host exclusion is a globally optimal shared-resource policy.

Full dispatch took 467.60 seconds: 13.66 staging, 382.74 execution and 71.20 recovery, versus an 870.29-second projection. Twenty-four telemetry samples over roughly 144 seconds averaged 99.96% host CPU use, with 16 native workers active and completed jobs increasing from 185 to 502. All our native workers exited. Kimi's independent local tests were left untouched. No paid compute was used.

Qualification cost was substantial: 1,159.59 seconds across completed allocations, plus 300.02 native seconds in the interrupted attempt, before the 7.8-minute scientific run. Do not repeat this entire matrix for an unchanged workload without assessing reuse of compatible recent timing evidence. Four-match splitting is byte-qualified and maintained parallel work; no matched full-panel coarse-versus-split experiment was performed, so do not claim a measured overall speedup over the previous, different panel.

## Next action and limits

Use the existing `public_features::replay_audit` and `PublicInputPlayPolicyV1::replay` path to score all four endpoints on an identical, schedule-selected archive panel. Measure exact sampler-mass KL/total variation, top-action disagreement, entropy and confidence before selecting a retention, exploration or feature intervention. Include both source arms and schedules, seats, pre/postboard states and opponent identities; select no trajectory by terminal result. Require same-state behavior replay, duplicate-model zero contrast and corruption rejection. Check state-only configuration compatibility before reusing the older audit executable. A new training campaign is not selected by this diagnostic.

The public checkpoint human bridge remains separate, and Kimi's active human-interface lane must not be duplicated. Read-only inspection of `E:/mtg-kernel-human-interface-kimi` at 87a297d0 and `E:/mtg-human-g115-20260920/kimi-baseline-001` found label repairs and a completed offline census with three projected recordings and six failures. That is interface evidence, not nine-deck delivery or actual desktop games. Stack-attribute transport remains a verified, separately scoped candidate.

[Agarwal et al.](https://arxiv.org/abs/2108.13264) motivate accounting for uncertainty over training runs; [Henderson et al.](https://arxiv.org/abs/1709.06560) document RL result variability. Our inference is narrower: more evaluation games cannot substitute for independent training repetitions or identify a learning mechanism. Fable's independent review remains unavailable after the recorded zero-read HTTP429 until September 22 07:00 EDT. No repeated retry or endorsement is claimed. CP7 outcomes were not used. The research goal remains active and the old heartbeat remains paused.

Evidence root: `E:/mtg-meta-recovery-20260921/prevention-crossover-001`, including `analysis.json`, `record-provenance.json`, `summary-artifacts.json`, `crossover-summary.png`, `full-telemetry-001.json`, `availability-change.json` and the separate interrupted-attempt recovery. Frozen design: `docs/prevention_crossover_design_20260921.md`.
