# V4 core consumed-root replay result

2026-09-22. Source a34f0e86, build B/v4-core-replay-build-001 complete137.881s, replay B/v4-core-natural-replay-001 complete. B=E:/mtg-meta-recovery-20260921. This is a fixed engineering compatibility check, not a strength gate. g115 unchanged, M1 unmet.

| Archived root | g115 action | Search action | Root visits | Transitions | Natural wins/losses | Raw value range |
|---|---:|---:|---|---:|---|---|
| cell25, game1 decision242, P1 |2|2|1,1,61,1|359|0/0|[-.968733,.964191]|
| cell45, game2 decision113, P1 |2|2|1,1,33,20,1,6,1,1|183|15/0|[-.707739,.076540]|

Both64simulations,depth8,transitioncap512,seed20260922. Both available. Each search repeated internally with fresh tensor witnesses; output exact. Full collection with diagnostics equals collection without; fresh-process on/repeat files byte-identical. Archived root tensor/logits/value and trajectory equal except two explicitly validated runtime package identity fields. Original session, policy scratch and policy RNG unchanged. Zero clips. Census includes unused root-prior forward.35/42nodes;65/50forwards. Firstroot26depthcutoffs,34expansions,4coverage;second41expansions,8coverage,15natural.

Both existing exact certificates identify action0 as winning. This bounded search selects2 instead, providing no tactical-fix evidence. Root0 received one simulation in each case and no continuation expansion after its coverage leaf. This is an observed allocation failure for these fixtures, not proof that larger-budget PUCT or all information-set search fails. The15naturalwins reached elsewhere do not certify a forced win or vindicate action2. No model promotion, no M1, no whole-match no-regression claim.

Whole collection process timings off/on/repeat: cell25 1.508/1.927/2.234s; cell45 .926/1.473/1.278s. On contains two core searches plus existing diagnostics. These timings are not isolated search latency, throughput qualification, or deployment guarantees.

Next decision proposed for independent review: a fixed equal-budget root-allocation diagnostic on these same consumed fixtures, keeping sampler/checkpoint/depth/value rule/interior policy/seed and transition cap fixed. Do not promote this core merely because compatibility passed. Investigate root underexploration before paying for whole-match evaluation or building a purported tactical candidate. Adapter error typing/eligibility and accurate evaluation behavior records also remain unresolved.
