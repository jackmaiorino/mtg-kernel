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

The fixed diagnostic completed in 6.70 seconds and mechanically passed every
prior label and movement threshold. Overall NLL improved 5.26 percent and top-1
improved 3.35 percentage points. P0 and P1 NLL improved 5.11 and 5.40 percent.
Overall mean TV was `0.028535`, P1 mean TV was `0.029516`, p90 TV was `0.078231`,
and maximum legal-action log-probability change was `0.478114`.

Report SHA-256 is
`f3fc251dfcda2e742b02bca5d92e4eb38c2e5afe3f203a00b9a2bebfa7fe3b82`.
Because the panel was revealed before choosing `0.97`, these values are
calibration mechanics, not held-out evidence. The calibrated recipe now advances
to transport and a fresh natural terminal win/loss A/B.
