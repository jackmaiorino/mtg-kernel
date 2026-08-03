# Native history-value search live v1

## Question

Can the complete-history model's improved terminal-value head produce a small
live Rally gain when used as a conservative one-step information-set selector,
despite the fixed direct-policy candidate failing its live gate?

## Fixed selector

- Package: exact complete-history candidate with composite model SHA-256
  `70b9e196ac6f7e7c391c2537d7173d6f9a87a7bdd6728e56d695cd83346a3463`.
- Fallback: the package's ordinary seeded policy sample.
- Eligible root: candidate-controlled surface decision, substep 0 of exactly 1,
  physical-decision ordinal at least 20, and 2 through 8 legal actions.
- Samples: exactly 4 deterministic acting-player information-set
  redeterminizations per root. Each sampled hidden state is shared across all
  legal actions.
- Evaluation: apply each legal action once. A natural terminal uses its exact
  win, draw, or loss reward. A nonterminal successor uses the history-aware
  value from the successor actor's perspective, sign-flipped when that actor is
  the opponent.
- Selection: override the fallback only when the largest mean successor value
  exceeds the fallback's mean by at least `0.25`. Exact ties preserve fallback.
- Reward: terminal win, draw, or loss only. No heuristic reward is introduced.

Trajectory exports remain disabled because their contract binds selection to
direct checkpoint-policy sampling.

## Preflight

The first one-pair implementation smoke used the real hidden state and is
invalid as strength evidence. It was replaced before any fresh measurement.

The corrected nonfresh base-`950001` smoke and exact repeat must match in every
normalized selector diagnostic and trajectory field. The selector must make
at least one override, every eligible root must record four distinct sampled
hidden-state hashes, and both games must complete without fallback, projection,
identity, protocol, or alignment failure.

## Fresh live gate

Run the search candidate and retained parent sequentially against XMage CP7 on
the same 8 fresh seat-swapped pairs at base seed `1200001`, episodes `0..15`.
Arm order is search candidate then retained parent.

A gain is a game the search candidate wins and the retained parent loses. A
loss is the reverse. The screen passes only if:

1. `G >= L + 2` over all 16 matched games.
2. Search-minus-parent wins are at least `-1` separately at P0 and P1.
3. Search makes at least one override in each candidate seat.
4. Every eligible search root records exactly four distinct shared
   information-set samples.
5. Both arms complete all legs with matched seats and environment seeds and
   zero scorer, search, projection, identity, protocol, or alignment failures.

A pass authorizes 16 additional fresh seat-swapped pairs at base seed
`1210001` with the unchanged code and package. A fail retires the fixed search
rule without threshold or root-selector tuning on the fresh report.

## Non-claims

This screen does not establish strength outside Rally mirrors, validate a
Bayesian belief model, test deeper tree search, authorize promotion, or
establish professional-level play. The information-set sampler is the existing
deterministic modulo-Fisher-Yates assignment sampler and is not a learned
posterior.
