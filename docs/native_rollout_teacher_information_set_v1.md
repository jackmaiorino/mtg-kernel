# Native information-set rollout teacher v1

## Question

Does the retained Rally policy have stable full-terminal action corrections after removing the perfect-information teacher's access to the actual opponent hand and future library order?

## Fixed design

- Source: retained outcome checkpoint manifest `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`, Adam step 1.
- Roots: the same 32 deterministic Rally mirror states used by the perfect-information ceiling, at most one per episode.
- Eligibility: surface decision, one substep, physical decision at least 10, and 2 through 8 legal actions.
- Ranking: four retained-policy continuations per legal action. Each rollout ordinal draws one acting-player information-set sample and every candidate action starts from that exact snapshot.
- Confirmation: 32 fresh samples. Within each ordinal, the teacher choice and retained-policy argmax start from the same snapshot.
- Hidden-card sampler: deterministic modulo-Fisher-Yates assignment over card placements consistent with the actor's represented knowledge. It has negligible modulo bias and is neither an exact-uniform sampler nor a Bayesian posterior over likely opponent holdings or future draws.
- Continuation randomness: common-random-number retained-policy sequences independent of the forced root action.
- Horizon: 512 policy steps including the forced root action.
- Continuation policy: the retained checkpoint for both seats, sampled with `f32-q8-expq63-hamilton-splitmix64-v1`.
- Runtime gate: below ten minutes.

The deterministic report records every redeterminization seed, sampled privileged-state hash, and checked branch-start hash. A valid report requires zero redeterminization failures, exact shared-snapshot use, more than one sampled hidden state at every root, zero branch failures or incomplete pairs, all ranking continuations natural, at least 99 percent natural completion overall, at least six changed roots, more positive than negative changed roots, and mean confirmed reward delta of at least `+0.05`.

Redeterminization, snapshot restoration, decision-observation preservation, and flat-binding errors are fatal. A failed sample produces no report rather than a partially valid report. The serialized aggregate therefore records required samples and successfully recorded samples; equality is the zero-failure gate.

## Interpretation boundary

This is an information-set-consistent diagnostic, not an XMage or CP7 strength evaluation. Its first successful report is not training authorization. Training admissibility remains `diagnostic-corpus-gate-only` until a second run produces byte-identical deterministic report bytes under the runtime gate.

The frozen sampler uses modulo-Fisher-Yates over hidden-card assignments allowed by represented knowledge. Its modulo bias is negligible but nonzero. It does not model a Bayesian posterior, opponent deck-selection uncertainty, or strategic card correlations.

## Formal v1 evidence

The implementation lineage is:

- `24d9caa` adds Rally information-set redeterminization.
- `52d09c9` adds the information-set rollout-teacher probe.
- `2456bc1` corrects handling of non-reentrant rollout-root decisions.

An initial attempt stopped fail-closed without producing a report because its post-redeterminization safety probe tried to re-surface an already-published, intentionally non-reentrant combat-priority decision. That attempt was diagnostic only. The formal result below starts after the correction in `2456bc1` and does not count the stopped attempt as evidence.

The two formal runs used executable SHA-256 `67562ad88fb2b903bf826f23a50aaa287f05469a581942ea9928d12782fa5f8c`. Each report was 591,742 bytes and had SHA-256 `e5fd54cbd9587cfef46b15bacc714690e108e47dea7aa16d7da948b6f9243460`, establishing byte-identical reproducibility. Their runtimes were 101,734 ms and 97,235 ms, both below the ten-minute gate.

All integrity, sampling, and natural-completion gates passed:

- All 32 required roots were collected.
- All 1,152 required redeterminization samples were recorded successfully.
- All 2,792 branch outcomes completed naturally, with no branch failure, horizon exhaustion, or incomplete confirmation pair.
- Every ranking sample was shared by all compared actions, every confirmation sample was shared by both branches, and there were zero shared-sample branch-start mismatches.
- Every root had multiple distinct information-set samples.

The teacher changed the retained action at 6 roots: 4 had positive confirmed delta, 1 had negative confirmed delta, and 1 had zero delta. The aggregate confirmed teacher-minus-parent reward delta was `12 / 1024 = +0.01171875`.

The only failed gate was the declared mean-delta requirement of at least `+0.05`. Therefore the information-set signal gate failed. The v1 disposition is **reject and do not train**: this result does not justify producing a candidate or spending a CP7 evaluation block.

## Next bounded test

Increase ranking evidence from 4 to 16 samples per legal action while keeping confirmation fixed at 32 fresh paired samples per root. Keep the information-set sampler, roots, horizon, integrity checks, and signal gate unchanged. This tests whether ranking noise hid a stable correction without weakening the acceptance criterion.

## Held-out rank-16 result

Commit `2a6fc45` implemented the final bounded rank-16 diagnostic under a separate schema. It kept the same 32 source roots and acceptance gates, but used four new seed domains held out from v1: ranking redeterminization, ranking continuation policy, confirmation redeterminization, and confirmation continuation policy. The frozen pass rule required all integrity gates, at least 6 changed roots, more positive than negative changed roots, and a confirmed reward delta of at least `52 / 1024`, the smallest attainable numerator meeting `+0.05`. Failure meant retirement without rank 32 or post-hoc root filtering.

The formal run used executable SHA-256 `a4bdab61c557d26a1347c1d9144a6772f484bc807e651ef3f98809da8b4994d0`. Its 770,015-byte report has SHA-256 `24a0506b93d9d16cf1670a2490b5cf45f70ae278649d365f353544b1a518e298`. Runtime was 174,034 ms, below the ten-minute cap.

All structural evidence passed:

- All 32 roots were collected.
- All 1,536 required redeterminization samples were recorded successfully.
- All 5,024 branch outcomes completed naturally, with no failure, horizon exhaustion, incomplete ranking action, or incomplete confirmation pair.
- All ranking samples were shared across legal actions, all confirmation samples were shared across teacher and parent, and there were zero branch-start mismatches.
- Every root had 48 distinct sampled hidden states across ranking and confirmation.

The rank-16 teacher changed 14 roots: 7 positive, 5 negative, and 2 neutral. Its held-out confirmed reward delta was `20 / 1024 = +0.01953125`. This remained well below the required `52 / 1024`, so the signal gate failed and the disposition was reject.

The perfect-information ceiling produced `+204 / 1024 = +0.19921875`, while information-set rank 4 produced `+12 / 1024 = +0.01171875` and held-out rank 16 produced `+20 / 1024 = +0.01953125`. The large perfect-information gain therefore did not survive removal of hidden-card clairvoyance. The residual visible-information signal is weak and does not justify teacher labels, a trained candidate, or a CP7 evaluation block.

## Final disposition

Retire this full-terminal rollout-teacher path. Do not run rank 32, filter roots after seeing confirmation, generate training labels from either information-set report, train a candidate, or spend CP7 games on this branch. The next work should target representation and learning architecture rather than additional rollout-budget search.
