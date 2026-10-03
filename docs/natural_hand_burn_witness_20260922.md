# Provisional natural hand-combination witness

2026-09-22, source `75a547671de73f14af4a68b78c6b98bab6de8c02`. Fixed target selected before branch outcomes: cell45-r0-s0-g115, native game 2, decision 113, actor p1. This is bounded engineering verification, not a strength gate. No model update or independent training runs.

## Result

Exact collection off/on equality, byte-identical on/on audit, and archived-prefix equality all passed. One root, eight targets, 1,037 engine transitions, 131,577 root JSON bytes. Face Bolt has bound [1,1] after 132 transitions; seven other targets, including the chosen Burning-Tree Emissary, remain [-1,1], with 143 transitions for self-target and 127 for each creature target. They are unresolved, not proven losses.

The recorded interval arithmetic was independently recomputed by `python/tools/check_public_burn_bounds_v1.py`. Its extracted winning subtree contains two own-choice nodes and one natural terminal win, with zero unknown leaves. The restricted continuation permits only hand Galvanic Blast as an additional damage spell. The root has five opposing life, Bolt pending, Galvanic in hand, and three untapped red sources. This establishes an available win under the diagnostic's supported public semantics, provisionally pending independent engine review and the new controls. It does not establish that hand use is necessary among all possible game actions.

g115 face probability is 3.818775346864454e-8. The sampled selected action is index 2, Burning-Tree Emissary, probability .24537448810605111. It was NOT the highest-probability action; distinguish the sampled action from the policy argmax. The duplicate cell45-r0-s1 root was not rerun or counted as independent evidence.

The unchanged archived continuation ends in a natural p1 win (141 gameplay decisions). At decisions 114-116 the policy casts Voldaren Epicure, then Galvanic Blast, targeting the other Burning-Tree Emissary. Thus this is a missed immediate win, not an observed lost game. Evidence: `observed-continuation.json` alongside the replay.

## Scope and evidence

Separate opt-in `public_hand_burn_tree.rs`; original Bolt/Island tree unchanged. Depth 32, nodes 8,192 per target, 4 MiB recorder cap unchanged. Opponent alternatives are retained and unsupported actions remain unknown. Same-turn/phase/library/draw-information checks remain. No policy scores or hidden identities select branches.

Evidence `E:/mtg-meta-recovery-20260921/hand-burn-tree-natural-001`, audit SHA256 `59967a206bd26ac78a79c78177a94454299fe7c4bbba9e0f00404f102923c5a4`, `completion.json` and `bound-analysis.json`. Build completed in 200.087 seconds under the owner-checked BelowNormal four-job launcher. CPU correctness replay only; no substantial evaluation, GPU training or paid compute.

The both-seat hand-removal and insufficient-payment controls completed: 1 focused test passed, 0 failed, 2,343 filtered; six engine cases (two supported wins, four unresolved controls). Build plus test took486.616 seconds, test body0.00 seconds. Evidence `hand-burn-tree-controls-001/completion.json` and `test.log`; session7232 exited0. This verifies these specific controls, not the full engine or complete oracle soundness. Fable's natural-witness review remains unavailable after HTTP429; no label or training decision is adopted. g115 unchanged, M1 unmet, goal active.

## Observed outcome coverage

Read-only linkage of all eight screened candidate records found seven acting-player wins and one loss. There are seven distinct visible positions; the cell45 duplicate is retained in the eight-record count. The lone loss is cell09-r1-s0 / decision317, whose nominal two-Fireblast continuation fails the immediate resource screen. Both provisionally verified natural witnesses occurred in observed wins. Source archives were hash-checked; evidence `E:/mtg-meta-recovery-20260921/natural-burn-observed-outcomes-001.json` retains every record and terminal outcome.

This weakens any inference from missed immediate wins to a useful whole-match gain. Terminal win/loss feedback need not distinguish an immediate win from a different line that eventually wins. That is an objective-level alternative explanation, not proof of how historical g115 credit shaped these preferences. No failed candidate was dropped, no new root selected using these outcomes, and no gate or reward changed. The proposed 2pp benefit remains a required useful effect with no supporting estimate from this candidate set.
