# Native recurrent CP7 deployment calibration v1

## Purpose

Verify the mechanics of a fixed `0.97` post-projection deployment scale after the
full-refit model missed the P1 mean-TV envelope by `0.000457`. This reuses the
revealed second panel and therefore is calibration diagnostics only, not a new
held-out result.

The model, width, beta `6`, eight-epoch fit, and hard `0.49` projection remain
unchanged. For each decision, compute the hard-projected candidate logits first,
then deploy `parent + 0.97 * (candidate - parent)`. No further coefficient or
scale search is allowed.

The diagnostic should retain at least 5 percent overall CP7 NLL improvement and
3 percentage points top-1 improvement while bringing all movement metrics inside
the previous envelope. A mechanical pass authorizes transport work and one fresh
natural terminal win/loss A/B. It cannot support another CP7-label generalization
claim.

## Result

Pending.
