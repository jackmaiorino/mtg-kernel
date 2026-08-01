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
