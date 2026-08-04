# Action-conditioned counterfactual Q mechanism screen

Status: frozen before collection.

## Question

Can a small action-conditioned scorer pool shared-information-set, retained-policy terminal outcomes across native Rally roots well enough to choose better one-step deviations than the retained parent argmax?

This is a mechanism screen only. Terminal natural win/loss/draw reward is the sole target and success measure. A pass authorizes a fresh-seed confirmation corpus, not a candidate, promotion, or strength claim.

## Fixed collection

- Exact retained `706b` outcome checkpoint and Rally versus Rally native runtime.
- 256 roots, one per source episode, balanced to 128 per acting seat. Root eligibility is fixed before rollouts: surface, one substep, physical decision at least 10, and 2 through 8 legal actions.
- Pair the nth accepted P0 root with the nth accepted P1 root for split grouping only. These are balance groups, not matched environment trajectories.
- Four acting-player information-set redeterminizations per root, shared exactly across every legal action. Preserve the acting player's private information and resample only hidden opponent/library assignments consistent with represented knowledge.
- Retained-policy full-terminal continuation after each forced root action. Export all four actor-relative natural terminal rewards for every action.
- Require four distinct sampled hidden-state hashes per root, exact root public tensor/logit/action identity under every sample, and natural completion of every branch with no timeout, substitution, or branch failure.
- Export the root tensor, parent logits and argmax, public action history, action identities, and per-action terminal rewards. No privileged state, opponent hand, library order, or rollout-derived input feature is exported.

## Fixed fit and split

- Whole balance-group split: pair index modulo 4 in 1 or 2 is train, 3 is selection, and 0 is held out. This yields 128/64/64 roots and 64/32/32 balance groups.
- Load the qualified complete-public-history structured initializer and freeze it. Use its 48-wide action joint representation plus parent-logit delta.
- Fit a deterministic ridge linear head to root-centered targets `mean_reward(action) - mean_reward(parent_argmax)`, weighting roots equally.
- Selection may choose ridge lambda from `0.01, 0.1, 1, 10, 100` and deployment margin from `0, 0.125, 0.25, 0.5`. Selection maximizes paired empirical uplift, then changed-root count, then lower complexity. No choice changes after held-out evaluation.

## Fixed held-out gates

All must pass:

1. Corpus and split integrity pass exactly.
2. Label adequacy: at least 16 of 64 roots have observed action range at least 0.5, and at least 10 have a parent argmax that is not empirically best.
3. Actionability: scorer differs from parent argmax on at least 8 of 64 roots.
4. Primary paired uplift: mean terminal reward uplift is at least +0.125 per root, the deterministic 10,000-resample one-sided 95% paired-bootstrap lower bound is above zero, and mean uplift is nonnegative for each acting seat.
5. Decisive-root diagnostic: at least 20 roots have a unique empirical best action at least 0.5 above the runner-up, and scorer top-1 accuracy on them is at least 10 percentage points above parent accuracy.

Root-centered RMSE, pairwise ranking loss, win/loss/tie branch comparisons, and all seat slices are reported as diagnostics. Failure means only that this four-sample linear screen found no robust transferable action-value signal.

## After a pass

Repeat the complete root screen on fresh source, redeterminization, and continuation seeds. Only a second pass may authorize packaging for a fresh matched natural-terminal game gate. Retained-policy continuation estimates a one-step deviation value and does not establish safe repeated deployment.
