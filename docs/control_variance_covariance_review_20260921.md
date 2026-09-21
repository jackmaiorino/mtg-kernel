# Paired evaluation covariance must enter the variance decomposition

Source-only review of the running control-variance evaluation, before reading any of its outcomes. No foreign source, frozen input, analysis gate or live process was changed. The analysis file is pinned by the actual plan at SHA-256 `89c7aa09cebe9260af23b68feaec32b291cf89487beba41a12c138db96774b5b`.

**Finding:** `E:/mtg-kernel-control-variance-kimi/python/tools/control_variance_analysis_v1.py:80-93` subtracts the mean of the ten endpoints' marginal bootstrap variances from the observed between-endpoint variance. That is the evaluation contribution only when the average off-diagonal covariance is zero. These endpoints deliberately share the environment seeds and the bootstrap draw stream. Positive covariance makes the subtraction too large; negative covariance can reverse the error. Its size in the real panel is still unknown.

For n fixed endpoint expectations, let S be the evaluation-error covariance matrix and P = I - 11'/n. The expected evaluation contribution to the sample variance across endpoints is

`trace(P S)/(n-1) = [trace(S) - sum(S)/n]/(n-1)`.

Equivalently, sum the bootstrap variances of all distinct endpoint differences and divide by `n*(n-1)`. This equals the mean marginal variance minus the average off-diagonal covariance. This derivation follows by expanding the centered sum of squares, with zero-mean evaluation errors; it does not require independent errors. The general relevance of covariance to common-random-number comparisons is established by [Glasserman and Yao, Management Science 1992, p. 884](https://business.columbia.edu/sites/default/files-efs/pubfiles/4261/glasserman_yao_guidelines.pdf). Shared seeds do not guarantee a particular covariance sign, so measure it rather than assuming it.

An exact binary-outcome counterexample uses the actual pinned estimator and its production array shape, 64 matchups x 8 seeds x 2 seats x 11 endpoints x 2 metrics. In 63 strata every endpoint has the same alternating seed outcomes. In the final stratum, endpoints 0 through 4 always win and the others always lose. Thus the endpoint contrasts are constant under every bootstrap resample, while the endpoint marginal means fluctuate together.

| Quantity | Binary counterexample |
| --- | ---: |
| Known endpoint SD | 0.823510 percentage points |
| Observed endpoint SD | 0.823510 percentage points |
| Marginal evaluation-noise SD | 2.245340 percentage points |
| Existing reported training SD | 0 |
| Evaluation variance in endpoint contrasts | 0 |
| Covariance-aware endpoint SD | 0.823510 percentage points |

Receipt: `E:/mtg-meta-recovery-20260921/control-variance-covariance-audit-002.json`, 200 paired bootstrap draws, seed 123. All synthetic outcomes are binary. The earlier fractional check in `control-variance-covariance-audit-001.json` agrees. These are counterexamples to the estimator's general validity, not estimates from the live campaign or proof of its actual error magnitude.

Consequences: the `training_sd_pp`, excess-variance result, training-component power calculation, and line 250's claim that training seeds barely matter cannot be used without a covariance-aware sensitivity calculation. Keep the measured endpoint rates and their observed spread. Do not cancel, restart or retrain the running experiment. Preserve its predeclared output and identify any supplemental calculation explicitly. The chi-square interval note at lines 240-242 assumes independent evaluation noise; its usual iid-normal coverage is not established for an arbitrary paired covariance matrix. Also, the bootstrap implementation resamples seed pairs within each fixed matchup, not the 64 matchup strata themselves. Its intervals are conditional on this fixed matchup mixture; the label alone must not imply generalization to a different meta.

Before outcomes are inspected, the proposed supplemental calculation is fixed: after all 11,264 matches complete and pass the original verifier, reuse exactly the original 10,000 paired resamples and seed. Report the marginal noise estimate alongside `sum_{i<j} Var_boot(rate_i-rate_j)/(10*9)`, their difference, and the resulting nonnegative excess variance. Do not change winners, seeds, groups, endpoint inclusion or gates. Do not claim that subtracting estimated noise alone supplies a confidence interval for latent training variance. No model selection or power commitment follows from this audit.

Independent Fable cross-examination remains unavailable under the recorded zero-read quota failure until September 22 07:00 EDT. This is Codex's source review plus a reproducible counterexample, not Fable endorsement. The existing recorder repair and its pending native tests remain the main implementation lane.

Completed-panel supplement: after the original analysis and all 11,264 verified matches existed, `review-control-variance-covariance.py` reused the unchanged verifier and frozen 10,000 resamples/seed. Observed endpoint SD is 0.762650 pp. Marginal noise SD is 0.707147 pp; the noise contribution to between-endpoint spread is 0.534389 pp after accounting for covariance. The nonnegative excess-variance SD changes from 0.285620 to 0.544118 pp. This confirms a material decomposition error in this panel; neither estimate is a confidence interval for latent training variability. No sample-size, promotion or model-selection decision is made. Original analysis SHA `fcffbb7c714ac10f67bf080e446d3524f4e1b84052da3b6d38cb89c78c793922` is unchanged. Supplemental receipt: `E:/mtg-meta-recovery-20260921/control-variance-covariance-supplement-001.json`.
