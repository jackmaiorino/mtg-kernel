# Ephemeral trainability probe: inconclusive for plasticity loss

Diagnostic completed, not a strength gate. No lineage checkpoint or playing candidate changed. All12 disposable copies and every step retained; no tuned rerun. Literature/design: collab/lit/20260922-ephemeral-trainability.md.

g110/g115, all-policy versus final-readout-only, three matched target-pattern seeds,20 SGD steps at.001. Eight natural inputs, first6 fit/last2 same-episode held-out, centered per-root Gaussian residual targets RMS.1 around each model's baseline. This equalizes residual size, not target functions or update size across parameterizations. Source checkpoint hashes rechecked unchanged. Runtime7.53s, one BelowNormal CPU thread, no GPU/games/saved model/optimizer. g115 baseline native-logit parity passed.

| Checkpoint / scope | Fit loss reduction, seeds0/1/2 | Final held-out MSE, seeds0/1/2 |
| --- | --- | --- |
| g110 all policy | -2675.4%, -4564.6%, -1410.9% | 13.763,13.990,12.100 |
| g115 all policy | -2603.1%, -2225.4%, -3843.0% | 30.960,31.440,38.774 |
| g110 final readout | 1.591%,1.916%,1.243% | .010189,.010000,.010023 |
| g115 final readout | 1.945%,2.230%,1.228% | .009975,.009982,.010016 |

Initial fit and held-out MSE are approximately.01 by construction. Negative reductions mean worse fit, not improvement. Full-policy updates were unstable for this chosen parameterization/step, while readout-only copies changed slightly. This does not demonstrate locked trunk, late-stage plasticity loss, superiority of frozen-trunk repair, or useful held-out transfer. Same scalar learning rate does not match functional update size across scopes. No independent run-to-run inference: the three seeds are arbitrary target patterns, not training replicates for a strength estimand.

Disposition: diagnostic is inconclusive for the intended plasticity comparison because optimizer sensitivity confounds full-policy fit. Preserve results, do not tune this same probe until it passes. Any follow-up needs a distinct stated question, such as local Jacobian/curvature or matched functional-step analysis, before interpreting optimizer-dependent fit. No reset, reward change or repair lever selected. No certified natural tactical label yet; M1 remains unmet.

Fresh Fable consultation session e5eea935-53ee-4d21-a584-01eee14aebb1 failed HTTP429 weekly limit, zero Read paths and zero input/output tokens, reset07:00 America/New_York. Evidence E:/mtg-meta-recovery-20260921/ephemeral-trainability-review-001. No findings or endorsement. Proceeded only with the monitor-required bounded disposable diagnostic under existing research authority, with no dependent intervention decision. Interpretation is provisional; no repeat review before availability changes.

Evidence E:/mtg-meta-recovery-20260921/ephemeral-trainability-001.json. Source python/tools/ephemeral_trainability_v1.py. The active model lineage remains g115 unchanged. This is not a fleet throughput qualification or calibrated strength result.
