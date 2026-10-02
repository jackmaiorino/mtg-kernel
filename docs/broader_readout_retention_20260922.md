# Broader readout retention: exact preservation infeasible

Completed2026-09-22. Artifact-only g115 geometry, no model/displacement exported or applied, no optimizer, games, GPU or paid execution. Corrected calculation took6.877s on one BelowNormal CPU thread. Source checkpoint unchanged.

Selection was fixed before results: up to32 evenly spaced nonforced learner first substeps per episode from all10 episodes in the first pre-update control-variance r1/a batch. Retained261 roots spanning six deck labels (the published Gates label, Affinity, Burn, Rally, Elves, Terror), without outcome/action-based selection. These are broader archival coverage, not a representative sample of future play or independent training replications. All263 root comparisons, including the two provisional burn targets, pass native logit/value tolerance; maximum logit discrepancy1.10e-5.

There are3,040 unordered within-menu action pairs. The retention matrix has64 columns and rank64; largest/smallest singular values283.406/2.454, so full rank is not merely a tiny rounding singularity. Exact preservation therefore forces zero readout displacement. All four requested1-logit designation patterns are infeasible under exact preservation, including incumbent argmax because its second-root margin is only.317.

| Designation | Solver-reported minimum worst absolute pairwise gap shift | L2 displacement / incumbent L2 for returned solution |
| --- | ---: | ---: |
| Face on both roots | 5.3252 | 8.8793 |
| Incumbent argmax | .3666 | .6840 |
| Random seed0 | 4.5478 | 7.9513 |
| Random seed1 | 4.1375 | 7.3833 |

All four minimax solves returned optimal status and passed recomputed primal constraints within1e-6 (target violation at most5.69e-14). The L2 norms are not minimized. This is numerical LP evidence, not a formal optimality certificate. No updated float32 policy or game behavior was tested. A large worst-case gap change does not by itself prove an action flip, regression, or a win-rate effect.

This contradicts a zero-change readout repair on the expanded retained inputs, not every possible readout intervention. It does not authorize relaxing retention, unfreezing the trunk, changing rewards, or adopting provisional witness labels. The two witness games already ended in wins, so useful whole-match benefit remains unestimated. Independent review and full joint power remain outstanding; no repair selected, M1 unmet.

## Preserved implementation correction

Attempt001 used884 differences relative to action zero for the minimax objective, not all unordered pairs as declared. Its exact-preservation result remains equivalent, but its reported4.466 face minimax is a different metric and is not the declared all-pairs answer. Preserve `broader-readout-retention-001.json`, its source snapshot and `broader-readout-retention-001-scope.txt`. Attempt002 corrected only pair construction: same roots, designations, margins, solver settings and checkpoint. This was a computation-scope correction, not a rerun of a strength gate or tolerance tuning.

Source `python/tools/broader_readout_retention_v1.py`. Final evidence `E:/mtg-meta-recovery-20260921/broader-readout-retention-002.json` contains selection, member/input hashes, singular values, parity and every solver outcome. Protocol `collab/lit/20260922-broader-readout-retention.md`.
