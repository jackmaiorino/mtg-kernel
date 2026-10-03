# Frozen readout geometry: feasible on ten inputs, not a repair

Completed 2026-09-22 in1.200 seconds, one BelowNormal CPU thread. No optimizer, model update, exported displacement, games or paid compute. g115 checkpoint SHA remains `88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1`.

The final scorer has64 coefficients. Eight fixed natural roots from one archived episode supply25 within-menu retention constraints, numerical rank25. Two additional natural burn roots supply the target constraints. All10 logits/value comparisons pass the established native tolerance2e-5 absolute/relative. Natural witness labels remain provisional and are used only as hypothetical designations in this diagnostic.

The linear program minimizes L1 displacement, requiring each designated action to exceed every alternative by1 logit while preserving all retention logit differences. Bias cancels. Every predeclared designation was retained:

| Designation on the two burn roots | Minimum L1 reported by solver | L2 of returned solution / incumbent L2 | Maximum retention residual |
| --- | ---: | ---: | ---: |
| Face / face | 61.1262 | 5.7192 | 4.01e-12 |
| Incumbent argmax / argmax | 8.0416 | .8255 | 1.16e-14 |
| Random seed0 | 52.0677 | 5.0052 | 2.17e-13 |
| Random seed1 | 80.2123 | 7.2694 | 4.62e-13 |

All four solver statuses are optimal; independently recomputed primal residuals pass1e-6. Maximum target violation is1.57e-13. Incumbent readout L2 is2.79181. Face baseline margins against the strongest alternatives are-5.44754 and-19.37348. The incumbent designation still needs movement because its second-root margin is only.31718, below the arbitrary1-logit convention.

This rules out exact finite-set linear infeasibility for these constraints. It does not select the readout repair: random designations fit too, retention has only25 constraints in64 dimensions, and all retention roots come from one episode. The reported L2 values belong to L1-minimizing solutions, not minimum-L2 solutions. Exact double-precision constraints on captured float features may be sensitive to conditioning; no float32 updated policy was constructed or verified. No whole-match retention, generalization, trainability, or strength effect follows.

Source `python/tools/fixed_readout_feasibility_v1.py`; complete evidence `E:/mtg-meta-recovery-20260921/fixed-readout-feasibility-001.json`, including input hashes, native parity, all statuses and residuals. Protocol `collab/lit/20260922-fixed-readout-feasibility.md`. No constructed tactical inputs were used. The fixed-readout hypothesis remains provisional; representative coverage, independent review, full joint gate power and compute qualification are still required. M1 unmet.
