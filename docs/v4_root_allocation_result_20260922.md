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

Cell45 round-robin trace localizes the observed blockage. Root0first-level node5 selects certifiedcast0 once during unvisited expansion at ordinal28, creating its target node. After covering all6interioractions, every subsequent choice at this node is action1. Cast0score stays-3318; chosen scores at ordinals68,76,84,92,100,108,116,124 are -941,-1179,-1992,-2341,-2496,-2450,-2418,-2404. Deficits2377,2139,1326,977,822,868,900,914. Thus the target node receives no further traversal and its zero-prior certified target is not tried. The measured discriminator is prior-weighted bonus asymmetry: cast0 has the better mean at seven of those eight decisions. This is not evidence of dilution after discovering a win or an at-any-budget impossibility theorem. Under current allocation root0 has only its coverage visit and never creates this subtree.

Engineering conclusion: root allocation selects the certified action on one fixed fixture but does not provide a candidate that resolves both fixtures. Cell25 mean margin is216.5quantized points (3464/16), consistent with the three discovered wins; no replication-based robustness claim. No whole-match effect or non-regression evidence, both original games were wins already. Reject promotion of this comparison as M1. Preserve the negative cell45 result.

Independent review completed: B/v4-leaf-terminal-design-review-001, session6e3074d7-74ea-42de-8a2f-66749770f60a, exit0/is_errorfalse, actual source/result reads. Defer integrated terminal probing; accept read-only decomposition first, then bounded standalone probe semantics and an interior bonus diagnostic. Review disposition and remaining qualifications are in v4_leaf_terminal_design_20260922.md. No new implementation or launch. Existing arms remain immutable.

Read-only decomposition B/v4-allocation-decomposition-001.json reconstructs node5 edge sums and integer bonuses from every saved trace, asserts all recorded scores, and matches final tree totals. No simulator rerun.

| Ordinal | Action1 mean | Action1 bonus | Action1 score | Cast0 mean/score |
|---|---:|---:|---:|---:|
|68|-3768|2827|-941|-3318|
|76|-3177|1998|-1179|-3318|
|84|-3622|1630|-1992|-3318|
|92|-3754|1413|-2341|-3318|
|100|-3759|1263|-2496|-3318|
|108|-3602|1152|-2450|-3318|
|116|-3484|1066|-2418|-3318|
|124|-3402|998|-2404|-3318|

Natural wins by ascending root action: cell25current [0,0,0,0], cell25RR [3,0,0,0], cell45current [0,0,78,1,0,0,0,0], cell45RR [0,0,1,1,1,1,0,0]. All natural losses/draws zero. These sampled-search counts are not match win rates. Node5 final action1sum=-31038 over9visits; immediately before ordinal124 it was-27218 over8visits. Cast0sum=-3318 over1visit.
