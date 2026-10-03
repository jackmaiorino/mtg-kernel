# Terminal teacher fits training data; raw-index control is confounded

The read-only final-endpoint diagnostic completed without training or reserved-data reads. Correct labels produce 32/32 winning argmax choices on the training positions. The failed reserved confidence gate is therefore a transfer limitation despite strong training fit, not evidence that the learner failed to fit these examples. It does not identify whether spell identity, changed creatures, numeric features or their interaction caused the transfer gap.

| Training family | g115 winning argmax | Teacher winning argmax | g115 mean winning probability | Teacher mean winning probability |
| --- | ---: | ---: | ---: | ---: |
| Face required | 0/16 | 16/16 | 1.2051e-8 | 0.997545 |
| Creature required | 16/16 | 16/16 | 0.998950 | 0.988609 |

The teacher's minimum winning probability is 0.995660 on face positions and 0.976552 on creature positions. Its final mean cross entropy is 0.00697268 across both families. This is measured after update32, unlike the campaign's final logged loss, which was measured before that update. Training creature confidence drops about1.03 percentage points despite retaining every winning argmax. This is narrow retention evidence, not demonstrated catastrophic forgetting across games.

The actual production Hamilton sampler gives mean winning probabilities 0.997547 and 0.988621, respectively. Each exact integer mass vector sums to2^64. Softmax and sampler agree closely here, although at the original parent's tiny face probabilities their relative difference is large (1.2051e-8 softmax versus6.1691e-8 sampler). No draws or game outcomes are inferred from these probabilities.

## Correction to the negative-control interpretation

The control rotates the raw legal-action index using `(winner + 1) % width`. That operation does not commute with a physical-seat swap. All16 paired training situations receive inconsistent actor-relative control labels. For example, face-required actor0 is taught to target the opposing Hunter, while equivalent actor1 is taught to target itself. In the creature-required family, actor0 is taught to target itself while actor1 is taught to target its own Hunter.

For all16 pairs, each of g115, the teacher and the rotated learner has exactly equal logit bits after mapping the legal actions by actor-relative meaning. The control matches its own argmax target in only16/32 training positions:8/8 face actor0 and8/8 creature actor1, zero in the opposite cells. Its own final mean target cross entropy is0.803565. This is not an evenly comparable control for learnable semantic supervision. There are no byte-identical full-tensor duplicates; the fault is exposed by semantic pairing, not a raw tensor hash.

Observed endpoint symmetry is not a proof that every parameter setting must be seat-invariant. Nevertheless, the label inconsistency is a direct property of the control construction and weakens teacher-versus-control conclusions. The teacher's improvement over original g115 on face choices remains observed; the failed reserved probability gate remains FAIL. Do not reinterpret this audit as retroactively passing the experiment.

A proposed replacement cycles through the legal actions ordered by their actor-relative semantic records. It is a derangement, agrees under all16 seat mappings and preserves its target under all24 legal-menu permutations for each of32 positions (768 checks). The old raw-index rule changes its target in512 of those768 checks. Proposed targets are exported only as preparation. No existing labels, checkpoints, outcome records or frozen gates were changed, and no corrected-control training was launched. These checks establish seat/menu-order consistency for this dataset, not universal arena-ID or hidden-state invariance.

## Verification, evidence and next research question

Implementation b8df6bbd adds `training_diagnostic` to `public_terminal_teacher_v1` while leaving frozen training and reserved evaluation functions unchanged. It accepts both final32 endpoints and their original32 training rows, verifies natural winning targets, scores the parent and both endpoints, and checks optimizer hashes before/after. Reserved rows, duplicate endpoints and invalid winner indices are rejected before output publication. A fresh process reproduces result bytes exactly; originalg115 logits and values match all32 earlier one-update input scores exactly. Dataset and checkpoint file hashes remain unchanged.

Build145.77seconds, four BelowNormal jobs, E-drive target/temp. First diagnostic1.20seconds, replay0.72seconds; three rejection checks total0.88seconds. These are bounded read-only correctness checks, not a substantial training run or a new hardware qualification. No paid compute. Final inventory shows no active owned jobs on either PC and preserves seven idle human sessions.

Evidence roots under `E:/mtg-meta-recovery-20260921/`:

- `public-terminal-teacher-training-fit-001/completion.json`, `analysis.json` and `first/result.json`. Result SHA256 `d51796cd090e90e5b023bb06f1cfdcc046019cfcbf76793caf8602f8767d4c61`.
- `public-terminal-teacher-control-audit-001/result.json`, SHA256 `9a5db81af03b993fea57af4473cf2f8981442e2997eaf374d4a6cf5ed9c25036`, plus `menu-permutation-check.json`.
- `public-terminal-teacher-diagnostic-tools-001/completion.json`, binary SHA256 `f554f6cce064eaaa51f2991d0f99c7b35bcd65a40ffd283f92f6bc5491396561`.
- Helpers `check-terminal-teacher-training-fit.py` and `audit-terminal-teacher-control.py`.

Next separate two questions: whether certified tactical supervision transfers across genuinely different public situations, and whether it preserves the incumbent policy outside that tiny domain. Do not merely increase updates to pass the consumed reserved set. Use the corrected semantic control in any separately designed successor, declare fresh validation before scoring, and retain originalg115 as the reference. Before more training, build read-only measurement of policy changes on existing natural development trajectories, stratified by deck, seat and decision type, without treating policy agreement as optimal play or using terminal outcomes for selection. This is a diagnostic prerequisite, not promotion or a whole-match strength test of the failed candidate.

The research literature supports separating imitation fit from return optimization: [Policy Distillation](https://arxiv.org/pdf/1511.06295) studies transferring policies by supervised learning, while [Kickstarting Deep Reinforcement Learning](https://arxiv.org/pdf/1803.03835), section2, combines teacher supervision with a return objective and reduces teacher influence over time. These results motivate examining retention and broader trajectories; they do not validate our synthetic labels or establish MTG strength. No learning objective is changed by this diagnostic.

Fable's previously recorded HTTP429 consultation had zero source reads and resets September22 at07:00EDT. No retry or endorsement is claimed. This reversible read-only work proceeds under Jack's authority; the control correction and successor design still need independent scrutiny. CP7 excluded, human preview unchanged, heartbeat paused, human/league objective unmet.
