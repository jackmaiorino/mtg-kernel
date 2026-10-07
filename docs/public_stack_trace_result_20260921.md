# Stack trace diagnostic: sparse special-attribute exposure and small behavior effect

The completed NO-ADVANCE screen does not establish that stack information is useless. The saved-trace audit finds sparse exposure to the highlighted special attributes and a small effect of the entire new message branch on the sampled behavior. This is an explanation to investigate, not a causal explanation of the failed win-rate comparison or a justification to promote or train longer.

## Complete exposure census

All 6,000 audited training archives were read and hash-checked. They contain 558,604 learner policy decisions across 509,224 distinct per-episode physical decisions; sequential substeps are not independent samples. Counts below use the structured arm's 182,686 choices with more than one legal action, across 2,000 games. A multi-action menu does not necessarily represent a strategically difficult choice. Each stack item is counted once through its baseline row, avoiding duplicated target messages.

| Public stack condition | Choice observations | Percentage of learner choices |
| --- | ---: | ---: |
| Any stack | 48,911 | 26.773% |
| Any highlighted special attribute | 2,439 | 1.335% |
| Flashback | 1,785 | 0.977% |
| Kicked | 217 | 0.119% |
| Nonzero X | 115 | 0.063% |
| Non-primary mode | 337 | 0.184% |

Highlighted special attributes are the union of kicked, flashback, nonzero X and non-primary mode. Categories overlap. Primary mode zero also represents non-modal cards, so non-primary counts underestimate all modal-card exposure. These are repeated learner observations of stack state, not unique casts, difficult tactical decisions or independent learning examples. In the structured arm, kicked occurred in 99 games, nonzero X in 81, flashback in 562, and non-primary mode in 178.

| Arm | Learner choices | Stack-present choices | Special-attribute choices |
| --- | ---: | ---: | ---: |
| structured | 182,686 | 48,911 | 2,439 |
| disabled | 184,564 | 49,401 | 2,484 |
| permuted | 184,864 | 49,434 | 2,472 |

All per-deck and per-episode counts are retained in the exposure result. Different arms visit different states under their own policies; their exposure differences are not a paired causal estimate.

## Exactly replayed message sensitivity

The fixed sample comprises all ten structured episodes from updates 19,39,59,79,99,119,139,159,179,199. Each uses the preceding saved checkpoint, which is the actual behavior model for those episodes. All 8,564 archived learner logits and values reproduce bit-for-bit. These are checkpoints throughout training, not a measurement of the final update-200 endpoint. The diagnostic switches off the whole stack message while retaining the same trained base network and legacy tensors. It removes object-binding contributions as well as explicit attributes, so it does not isolate the four highlighted fields.

| Choice category | Choices | Highest-score action changes | Mean softmax total variation | Maximum total variation |
| --- | ---: | ---: | ---: | ---: |
| all | 8,466 | 1 | 0.000262 | 0.154407 |
| empty | 6,323 | 0 | 0.000000 | 0.000000 |
| default | 2,036 | 1 | 0.001015 | 0.154407 |
| special | 107 | 0 | 0.001386 | 0.032563 |

Total variation ranges from zero for identical distributions to one for disjoint distributions. It is computed from stable float64 softmax of native logits, not claimed as bit-identical native sampler probabilities. Empty-stack scores and values are exactly unchanged. The one highest-score action flip occurred in the update-59 default-stack category. Argmax changes are descriptive; the actual policy samples actions. Neither these differences nor the absence of flips establishes which action is correct.

## Follow-up confidence check

A separate follow-up plan was written after sensitivity completed and before computing confidence. It uses the same 100 archives without selection. Median highest-action softmax probability is 99.48% across all choices and 98.47% on the 107 special-attribute choices. A highest-action probability of at least 99% occurs on 4,829/8,466 choices and 45/107 special choices. Median normalized entropy is 0.0293 overall and 0.0549 on special choices. The special-state lower quartile of highest-action probability is 65.06%, so not every relevant decision is saturated. These data show high confidence in much of the sample; they do not prove confidence is correct or that saturation caused the weak message effect. Equivalent legal actions can also affect menu entropy. This is not grounds to repeat the already negative entropy intervention without a new causal argument.

## Verification and cost

Exposure parser tests passed target-row de-duplication, learner/opponent separation, physical/substep accounting, primary-mode handling and changed-file rejection. A 90-file fixed sample produced identical serial/eight-worker counts in 2.71/0.83 seconds. The full census took 35.51 seconds with eight workers reading existing local SSD files. No new simulator games, gradients or optimizer updates were executed.

The separate native diagnostic compiled successfully in 106.71 seconds using four BelowNormal Cargo jobs. Runtime verification covered exact archived behavior, unchanged legal score-vector widths, exact empty-stack invariance and identical signatures across serial, parallel and both-PC placements. The small numerical unit test in the new Rust module was added but not separately invoked; do not report it as a passed test. No prior measurement binary was rebuilt in its frozen evidence directory.

Four-job timing walls were the maintainer's one worker 4.60 seconds, the maintainer's four workers 1.91 seconds, the compute host one worker 9.22 seconds and the compute host four workers 4.88 seconds. The both-PC split was also checked. Including transport/setup, projected full times were 4.76 seconds for the maintainer's four workers, 10.52 seconds for both PCs and 12.42 seconds for the compute host four. The supported diagnostic chose desktop four workers. Its full 100-archive pass took 2.89 seconds; the entire controller with all checks took 49.49 seconds. Repeated timing passes are diagnostic executions, not additional independent archives or games. Confidence extraction took 0.85 seconds. Both PCs had no active native workers at the final snapshot. RunPod remains the dated authenticated403 record, with no paid allocation authorized or launched.

## Research disposition

Keep the frozen NO-ADVANCE result and the incumbent. Sparse exposure and weak message influence limit this short screen's ability to test attribute usefulness; they do not demonstrate that more training, a larger feature contract or an entropy change would improve terminal play. Before selecting another learning intervention, review the prior entropy/prevention failures and establish an independently checkable tactical error question on public-information states. A test of model confidence without action-correctness evidence is insufficient. This diagnostic does not supply independent competent-opponent, human-match or league evidence.

Fable consultation remains unavailable: review876765fa-0834-4f03-8db1-6b1f99300885 failed HTTP429 with zero source reads until September22 07:00EDT. No retry or endorsement. Work proceeds under the maintainer's explicit research authority with the review gap recorded. CP7 outcomes remain excluded. Kimi's separate Escape gate remains unadopted.

Authoritative artifacts:

- [Exposure census](E:/mtg-meta-recovery-20260921/public-stack-exposure-001/result.json)
- [Exact replay and sensitivity](E:/mtg-meta-recovery-20260921/public-stack-sensitivity-001/result.json)
- [Confidence follow-up](E:/mtg-meta-recovery-20260921/public-stack-sensitivity-001/confidence.json)
- [Frozen diagnostic scope](E:/mtg-kernel-public-stack-features-codex/docs/public_stack_trace_diagnostic_20260921.md)
