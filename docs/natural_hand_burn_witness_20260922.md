# Provisional natural hand-combination witness

2026-09-22, source `75a547671de73f14af4a68b78c6b98bab6de8c02`. Fixed target selected before branch outcomes: cell45-r0-s0-g115, native game 2, decision 113, actor p1. This is bounded engineering verification, not a strength gate. No model update or independent training runs.

## Result

Exact collection off/on equality, byte-identical on/on audit, and archived-prefix equality all passed. One root, eight targets, 1,037 engine transitions, 131,577 root JSON bytes. Face Bolt has bound [1,1] after 132 transitions; seven other targets, including the chosen Burning-Tree Emissary, remain [-1,1], with 143 transitions for self-target and 127 for each creature target. They are unresolved, not proven losses.

The recorded interval arithmetic was independently recomputed by `python/tools/check_public_burn_bounds_v1.py`. Its extracted winning subtree contains two own-choice nodes and one natural terminal win, with zero unknown leaves. The restricted continuation permits only hand Galvanic Blast as an additional damage spell. The root has five opposing life, Bolt pending, Galvanic in hand, and three untapped red sources. This establishes an available win under the diagnostic's supported public semantics, provisionally pending independent engine review and the new controls. It does not establish that hand use is necessary among all possible game actions.

g115 face probability is 3.818775346864454e-8. The sampled selected action is index 2, Burning-Tree Emissary, probability .24537448810605111. It was NOT the highest-probability action; distinguish the sampled action from the policy argmax. The duplicate cell45-r0-s1 root was not rerun or counted as independent evidence.

## Scope and evidence

Separate opt-in `public_hand_burn_tree.rs`; original Bolt/Island tree unchanged. Depth 32, nodes 8,192 per target, 4 MiB recorder cap unchanged. Opponent alternatives are retained and unsupported actions remain unknown. Same-turn/phase/library/draw-information checks remain. No policy scores or hidden identities select branches.

Evidence `E:/mtg-meta-recovery-20260921/hand-burn-tree-natural-001`, audit SHA256 `59967a206bd26ac78a79c78177a94454299fe7c4bbba9e0f00404f102923c5a4`, `completion.json` and `bound-analysis.json`. Build completed in 200.087 seconds under the owner-checked BelowNormal four-job launcher. CPU correctness replay only; no substantial evaluation, GPU training or paid compute.

At this report commit, the both-seat hand-removal and insufficient-payment controls are running under `check-hand-burn-tree-controls.py`, evidence `hand-burn-tree-controls-001`, live tool session 7232 / Cargo PID 101304. Do not claim they passed until their completion and log are inspected. Fable's natural-witness review remains unavailable after HTTP429; no label or training decision is adopted. g115 unchanged, M1 unmet, goal active.
