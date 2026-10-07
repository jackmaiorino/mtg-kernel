# Terminal teacher continuation: complete, feasibility gate failed

The frozen comparison completed 32 full-batch updates for each of the correct-label and rotated-label arms, followed by one evaluation of unchanged g115 and both final endpoints on all 24 reserved positions. The correctly taught model selected a certified winning action in all 16 winning positions, but its mean probability on the creature-winning action was 0.7510, below the predeclared 0.8 requirement. The engineering feasibility result is FAIL. No checkpoint is promoted or substituted into the human preview.

Subsequent training-only audit found that the raw-index rotated control assigns inconsistent actor-relative labels to all16 physical-seat pairs. This weakens teacher-versus-control interpretation; it does not change the frozen failed result. See `docs/public_terminal_teacher_training_fit_20260921.md` for the exact semantic mappings, training fit and proposed consistent control. Original measurements and labels remain untouched.

## Reserved evaluation

| Endpoint | Face winning argmax | Face mean winning probability | Creature winning argmax | Creature mean winning probability |
| --- | ---: | ---: | ---: | ---: |
| Original g115 | 0/8 | 1.2641e-9 | 8/8 | 0.760739 |
| Correct terminal labels, update 32 | 8/8 | 0.942121 | 8/8 | 0.751011 |
| Rotated labels, update 32 | 0/8 | 0.179770 | 0/8 | 0.038459 |

The integer requirements passed: at least 7/8 in each family and at least two more winning argmax choices than the rotated control across the 16 winning positions. The creature probability requirement failed. Compared with g115, the correct learner fixes the face argmax error but reduces creature winning probability by about 0.97 percentage points. Beating a damaged rotated-label control alone would not establish improvement over g115.

The other eight reserved positions have no winning action. They remain descriptive controls without invented optimal labels. All three endpoints were evaluated on all 24 positions; no intermediate checkpoint was selected. Probabilities are descriptive float64 softmax values, not exact production Hamilton sampler masses. These are constructed positions with shared templates and physical-seat copies, not 16 independent matches. No whole games were played in this comparison, and there is no broad meta or human-strength claim.

The original gates and fixed 32 updates remain in `docs/public_terminal_teacher_engineering_result_20260921.md`. The reserved set is now consumed: do not relax its gate, choose an intermediate checkpoint, or train against its revealed failures and call the same set held out.

## Implementation and verification

Source 649b7ea9 adds the supported `public_terminal_teacher_v1` continuation/evaluation path and a distinct CUDA terminal-imitation entry point. It preserves g115 Adam moments and advances its age from 32400 to 32432 independently for each arm. The objective is certified-terminal-winner cross entropy, learning rate 1e-4, policy coefficient 1 and value-loss coefficient 0. Existing on-policy GAE checks still reject zero value coefficient. Existing momentum means zero value loss does not guarantee frozen value parameters.

Checkpoints publish after every update with source, dataset, arm, backend/device, optimizer age, parameter/moment state and state hashes. A one-update checkpoint resumed to update two is byte-identical to uninterrupted two-update execution. Correct-label CPU checkpoints match across one/four backward workers and both PCs. An unqualified 32-update request was rejected before output creation or training.

The supported Python launcher validates its pinned plan, helper dependencies, replay and throughput reports, source inputs and ownership. The native launcher requires compatible, fresh qualification for the substantial 32-update request. This is enforcement in these launch paths, not an operating-system restriction or a claim that every legacy launcher is guarded.

The maintainer GPU 1 passed the native numerical checks, CPU comparison envelope and fresh GPU checkpoint replay. The compute host's GPU remained ineligible: an initial missing-NVRTC problem was repaired using two verified DLLs in a task-local directory and process-local PATH, after which the native device-failure check still failed. Both attempts are preserved. No global CUDA installation was changed. The residual device failure is unresolved and is not numerical qualification.

## Allocation and costs

The fastest qualified allocation was two concurrent independent arms on the maintainer's SSD, one backward worker per arm. Increasing backward workers did not improve completed workload time. The qualification included the desktop and the compute host CPU serial/parallel placements, splitting arms across both PCs, and the eligible GPUs. The existing authenticated RunPod availability check returned 403; no paid allocation was made.

| Qualified placement | Forecast for both 32-update arms, including staging/recovery |
| --- | ---: |
| The maintainer CPU, one arm at a time, one backward worker | 23.84 s |
| The maintainer CPU, one arm at a time, four backward workers | 23.43 s |
| The maintainer CPU, two concurrent arms, one backward worker each | 18.31 s |
| The compute host CPU, two concurrent arms | 78.69 s |
| One CPU arm on each PC | 69.68 s |
| The maintainer GPU 1 | 38.63 s |

Forecasts use a short two-update qualification and are uncertain. `allocation-check.json` amortizes measured GPU cold start once per job; the original more conservative forecast is preserved and the selected allocation is unchanged. The compute host GPU is excluded after its failed repair qualification.

Actual formal stages were 2.884 s staging, 7.385 s execution and 4.231 s recovery, totaling 14.499 s. This excludes owner inventory and other controller overhead. Native training loops were 6.746 s and 6.696 s concurrently. The new binary build took 142.34 s; qualification and troubleshooting cost substantially more than the tiny formal training run. Reuse compatible qualification rather than repeating the grid without a material change. There were 64 updates and 2,048 training-position presentations in total.

## Evidence and disposition

Evidence root: `E:/mtg-meta-recovery-20260921/public-terminal-teacher-campaign-001`. `completion.json` covers both training arms; `analysis.json` records complete=true, engineering_feasibility_pass=false and no_promotion=true. Evaluation SHA256: `1b01fbdc9b326989d6492e5663ad96c6c03c5eb57ce0bb6fece5d882e7fa2e1e`.

Correct final state SHA256: `d2f87a3d7c3e9294789a8690385234bfe10468019a45243fdba2c5e3e7f5229a`. Rotated final state SHA256: `e7a9ccff6ef2788d3c089504e8709b27a9eed365daa7c8822a9b7e08a7de0822`. Both endpoints are `checkpoint-032.json` in their respective recovered output directories. Binary SHA256: `026477c89145dae678ddfb0fe9ee89cb0c81218239c9314b63c944bda2d1d993`.

Compute evidence: `public-terminal-teacher-compute-001/choice.json`, `allocation-check.json`, and `engineering/completion.json`. The compute host repair evidence: `public-terminal-teacher-cuda-recovery-001/completion.json`. Final owner inventory at 18:06 UTC shows no active owned native jobs on either PC, with seven existing desktop human sessions preserved.

The known independent Fable consultation failed HTTP429 with zero source reads and reset September 22 at 07:00 EDT. It was not retried and provides no endorsement. Work proceeded within the maintainer's existing local authority; objective design, narrow generalization and the implementation retain that review gap. CP7 outcomes were excluded.

Next work should investigate training-side retention and objective behavior without reusing this reserved set for selection. Any successor requires a separately frozen question and fresh evaluation data; this failed gate does not authorize automatic whole-match advancement. The original g115 human preview remains unchanged. Human/league competence is still unproven, and the heartbeat remains paused.
