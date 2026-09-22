# Root-allocation comparison result

2026-09-22. Source7c734f12. B/v4-core-controls-00432/32pass; B/v4-allocation-build-001 and B/v4-allocation-replay-001 complete, analysis allocation-summary.json. B=E:/mtg-meta-recovery-20260921. Fixed two consumed roots, not a population or training gate. Literature/design frozen before launch. No promotion, M1 unmet.

| Root/arm | Simulations | Transitions | Selected/maxmean | Visits to certified0 | Natural wins under0 | Mean0 |
|---|---:|---:|---|---:|---:|---:|
| cell25 current |64|359|2/1|1|0|7894|
| cell25 root-round-robin |64|152|0/0|16|3|8621|
| cell45 current |128|439|2/2|1|0|-4231|
| cell45 root-round-robin |128|338|3/3|16|0|-3761|

All arms complete, internal repeats with fresh tensor witnesses exact, original/policy unchanged. Full off/on collection identical and on/repeat fresh-process bytes identical. Every archived current64cell25 outcome field reproduced exactly. Round-robin16visits per root action, traces conserve transitions. Same model/sampler/key/value/interiorselection/depth8/seed20260922; actual completed work differs. No zero-prior floor, interior arm or sweep.

Predictions: cell25naturalwins/action0selection supported; exact pass-line completions at simulation ordinals9,37,41. Cell45root0terminal unlikely prediction supported; no certified line completion. Current128cell45 retainsroot0onevisit as predicted. The conditional prediction about selection after a discovered cell45win was not tested because none was reached. Round-robin selects uncertified3, not original2; no claim that this improves that state.

Cell45 trace localizes the observed blockage. Root0first-level node5 selects certifiedcast0 once during unvisited expansion at ordinal28, creating its target node. After covering all6interioractions, every subsequent choice at this node is action1. Cast0score stays-3318; chosen scores at ordinals68,76,84,92,100,108,116,124 are -941,-1179,-1992,-2341,-2496,-2450,-2418,-2404. Deficits2377,2139,1326,977,822,868,900,914. Thus the target node receives no further traversal and its zero-prior certified target is not tried. This is measured interior selection under low prior and negative leaf value, not evidence of dilution after discovering a win or an at-any-budget impossibility theorem.

Engineering conclusion: root allocation repairs one certified action selection but does not provide a candidate that resolves both fixtures. No whole-match effect or non-regression evidence, both original games were wins already. Reject promotion of this comparison as M1. Preserve the negative cell45 result.

Next design question for independent review: a generic bounded immediate-terminal check at newly evaluated live leaves, inspired by MCTS-Solver's mate-in-one leaf check, versus an interior exploration change. Such a check would use V4-consume clones and charge every probe transition, never turn sampled-world terminal wins into information-set proofs or modify training rewards. No implementation or launch yet. Keep both currentcomparisonarms immutable. The exact numerical and budget/backup contract requires review.
