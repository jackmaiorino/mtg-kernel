# Both semantic controls fit at the fixed128 endpoint

The training-only budget diagnostic completed. **Both arms fit32/32 semantic labels after128 cumulative updates**, versus16/32 after32. Every family/seat cell is8/8. Continuing the same optimizer and data was sufficient for this label task; the earlier32-update budget was inadequate. This does not imply general architectural sufficiency, useful playing behavior from the deliberately incorrect labels, or human strength.

| Semantic control | Own targets after32 | Own targets after128 | Final raw-logit CE |
|---|---:|---:|---:|
| Beta0 | 16/32 | 32/32 | 0.019500 |
| Beta0.5 | 16/32 | 32/32 | 0.031574 |

Each endpoint correctly chooses self on the16 face-family control cases and opponent on the16 creature-family control cases. For beta0, mean target probabilities are0.978245 and0.983141 respectively; for beta0.5 they are0.957969 and0.980052. These probabilities use the production Hamilton sampler. The loss uses raw logits. These quantities are distinct, especially for the original highly separated g115 logits.

The frozen plan is `docs/semantic_budget_diagnostic_plan_20260921.md`. The run added96 updates per arm from the two existing32-update states, preserving original g115 lineage, all optimizer moments, learning rate0.0001, corrected semantic labels,32 fixed teacher inputs,256/303 retention groups/rows, each arm's existing beta, and deterministic full batches. Both end at Adam age32,528. The128 endpoint was fixed before either continuation ran. No intermediate checkpoint was evaluated to choose an endpoint. The consumed48-position and100-game validation panels and CP7 outcomes were excluded.

Selected **pre-update** training losses show gradual learning, not an immediate correction:

| Before cumulative update | Beta0 CE | Beta0.5 CE |
|---|---:|---:|
| 33 | 0.638734 | 0.639715 |
| 64 | 0.493194 | 0.513278 |
| 96 | 0.205603 | 0.260499 |
| 128 | 0.020361 | 0.033087 |

The first table scores after update128. The beta0.5 arm's pre-update parent KL falls from0.536310 at33 to0.121638 at128. That is a training-objective value on development retention inputs, not a fresh retention validation result.

## Verification and lineage

Source **6174d2a7** adds `SemanticBudgetDiagnostic` as a separate explicit continuation mode. Output schema `terminal-semantic-budget-diagnostic/v1`, loss identity `terminal-semantic-ce-fixed128-budget/v1`, and throughput schema `semantic-budget-diagnostic-compute/v1` distinguish it from the frozen prior experiments. Each checkpoint records the original predecessor file pin. Original checkpoints retain their actual schemas and design pins and were not rewritten. The previous inference reader will not accept these diagnostic checkpoints as eligible endpoints.

| Arm | Verified32-update predecessor state | Final128 state |
|---|---|---|
| Beta0 | `74491c251397e9463b67a97522524fdc416040080615db1293eadf1d9a2a2404` | `6642a1b70af0eb25aac0599abd3f8ac60a9ee5f4b1e073366bcee9e2b8394580` |
| Beta0.5 | `7d28ebc2ac82b82de08075523b036f1f316a2f8dc4a610c73d31c248612e9129` | `6dace40002179b6b160e6b550912abb2880ff92ffc664d6e1989014f20f9416f` |

Engineering reproduced all64 previously verified predecessor training logits/value bits exactly before updates. For each arm, one-worker continuous and four-worker resumed update34 checkpoints and final training scores matched exactly. The native mode rejected an unqualified128-update launch, a wrong-arm predecessor, an altered optimizer state, and use through the old diagnostic mode. Across all nine measured placements, both update33/34 checkpoint files were byte-identical. Final recovery verified all192 new checkpoint files with zero mismatches and both complete endpoint receipts. The independent analyzer recomputes argmax and target CE from saved logits and was hashed before launch.

These are meaningful state, replay and numerical checks. They do not test playing strength. Fable's independent review remains unavailable under the recorded zero-source-readHTTP429 through September22 07:00EDT. It was not repeatedly retried or treated as endorsement. That review gap remains explicit under Jack's bounded research/execution assignment.

## Actual compute and a publication bottleneck

The supported launcher `E:/mtg-meta-recovery-20260921/semantic-budget-diagnostic.py` validates completed, fresh, compatible qualification before substantial dispatch. The native mode also binds qualification to each predecessor hash, binary, source, inputs, arm, machine and worker count. Engineering is bounded to cumulative33/34. These are supported-path guards, not OS enforcement.

| Measured placement | Forecast total |
|---|---:|
| Jack1 worker, arms serial | 109.12s |
| Jack4 workers, arms serial | 100.41s |
| Jack1 worker, arms concurrent | 90.65s |
| **Jack4 workers, arms concurrent** | **78.88s** |
| HaleysPC1 worker, arms serial | 314.22s |
| HaleysPC4 workers, arms serial | 298.80s |
| HaleysPC1 worker, arms concurrent | 293.35s |
| HaleysPC4 workers, arms concurrent | 278.88s |
| Both PCs4 workers, one arm per PC | 220.20s |

The selected allocation was fastest among these measured candidates. The two-host case assigned zero-beta to Jack and retained to HaleysPC; its reversed orientation was not benchmarked, so no global-optimal-placement claim is made. Hardware, storage, memory and GPUs were inventoried. RunPod returnedHTTP403 at20:34:43UTC, with no allocation or spending. The retained continuation has no qualified CUDA path; no GPU speedup claim follows.

**Actual stage times:** staging2.762s, execution52.946s, recovery113.098s, total168.806s, plus separate ownership checks. The read-only controller/telemetry wrapper lasted184.074s. Native learners finished in19.991s and52.684s respectively. The two-update forecast substantially underestimated recovery, so it is not reliable for another run with the same volume. The build cost153.154s using4BelowNormalCargo jobs and E target/temp storage. Native execution/checkpoints used the D SSD; recovered evidence used E.

Why the underestimate matters: each full JSON checkpoint is approximately20.8MB. Publishing every update produced192 snapshots, approximately4GB, then copied and rehashed them on the E HDD. The live controller had already copied all192 files while still performing recovery verification. It remained alive and making I/O progress; nothing was restarted. This is excessive publication volume for a short diagnostic, rather than evidence that more model training was needed or that high CPU occupancy would solve the recovery bottleneck.

Identity-verified one-second process samples measured about0.75 average CPU cores for the zero-beta learner and1.10 for the retained learner over their observed windows, with peak RSS about139MB/161MB. Both were configured with4 backward workers. Their measured write deltas were1.91GB and1.95GB; read deltas were7.57GB and7.81GB. The readings exclude launch/exit gaps and do not distinguish forward, backward, serialization and durability checks. Thus worker count must not be reported as full-core utilization. No priority, affinity or process settings were changed by the monitor. The retained learner was terminal before60s; later low utilization corresponded to recovery with no learner work remaining.

**Required before another substantive run:** use a declared lower checkpoint frequency for short continuations, preserving full Adam snapshots at periodic recovery points and the final endpoint, atomic publication and all previously frozen outputs. Qualify the new path against the existing dense writer on bounded resumed/continuous training, requiring identical final state and scores plus same-mode replay. Re-measure end-to-end recovery at the intended cadence. Do not delete the completed snapshots or change the writer during a live frozen run. Retain the actual113s recovery cost in accounting.

## Research disposition

This diagnostic resolves the narrow adequacy question: the semantic labels are learnable by both arms with the preserved model/optimizer at this fixed longer budget. The original32-update comparison remains no-advance; the diagnostic does not repair that gate or turn its existing validation into a successful128-update experiment.

After fixing publication overhead, the next research step is a distinct comparison with equal training budgets for correct-label and control arms and fresh evaluation inputs. Declare it before running or scoring; retain terminal rewards and matched controls, and keep the consumed panels retired. A successful new feasibility result would still require separately designed current-engine whole-match and human evidence. No diagnostic model was promoted or installed into the human preview.

Evidence: `E:/mtg-meta-recovery-20260921/semantic-budget-diagnostic-001/{plan,analysis-before-launch,completion,analysis,telemetry}.json` and `semantic-budget-compute-001/{choice,engineering/completion}.json`. Binary SHA `666a6c4953996f9da7b0b6b0520a849c0def297dd798dea1a7dba1d40848e68d`. Both PCs had no active research-native jobs after completion, the seven idle human sessions were preserved, and the heartbeat remains paused. The full human/league objective remains unmet.
