# Native conservative selective-search signal v1

## Question

Can a conservative information-set search rule identify reliable later-game
action improvements while defaulting to the retained policy when rollout
evidence is weak?

The retired rank-16 teacher always selected the action with the largest noisy
rollout sum. This screen instead tests a policy-preserving override rule.

## Fixed source and roots

- Source: exact retained centered-v2 outcome checkpoint manifest `706b3aa...`.
  This reuses the qualified information-set sampler as a mechanism screen. It
  is not a promoted(2) strength claim.
- Source trajectory seed domain: fresh `selective-search-signal-v1` domain.
- Roots: 32 fresh Rally mirror states, at most one per source episode.
- Eligibility: surface decision, one substep, physical decision at least 20,
  and 2 through 8 legal actions.
- The prior rank-4 and rank-16 roots, redeterminization domains, continuation
  domains, and confirmation outcomes are excluded.

## Fixed search rule

- Draw 16 acting-player information-set samples per root.
- Share each sampled hidden state and continuation-policy random numbers across
  every legal action.
- Rank by terminal win/draw/loss reward sum, with the retained logit and action
  index used only for exact ties.
- Default to the retained-policy argmax.
- Override only if the best alternative's 16-sample reward sum exceeds the
  parent's sum by at least 6.
- Confirm the resulting search action against the parent on 32 fresh paired
  information-set samples.
- Continue both branches with the retained policy for both seats for at most
  512 policy steps including the forced root action.

The margin 6 hypothesis was selected from the old rank-16 report before this
fresh run. No old confirmation row enters the new result.

## Gates

Require all existing information-set integrity gates, all ranking and
confirmation continuations complete and natural, runtime below ten minutes,
at least 6 overrides, more positive than negative override roots, and aggregate
confirmed search-minus-parent reward delta of at least `52 / 1024`.

## Disposition

- Pass: implement the same conservative rule dynamically on exact generation
  384, then run a small paired native search-versus-policy screen under a fixed
  compute budget.
- Fail: close full-terminal retained-policy rollout search with this sampler.
  Do not tune the margin or root selector on the fresh report.

This screen produces no trained model, XMage result, promotion evidence, or
professional-level play claim.
