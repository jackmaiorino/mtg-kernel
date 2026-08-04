# Native terminal credit-assignment screen v1

## Question

Does GAE propagate the same natural terminal result into a cleaner and more
repeatable policy-head direction than the current Monte Carlo advantage?

This is a rapid frozen-trajectory mechanism screen. Win, draw, or loss at the
natural terminal is the only reward. Every nonterminal reward is exactly zero.
The result is diagnostic only and cannot promote a policy.

## Fixed screen

- Corpus: confirmed disjoint Pool3 cache SHA-256
  `44eae5bee2b5556faa6293c80a88cb8f67f90d46066ffb5115ced2daac579800`,
  1,024 seat-swapped pairs and 2,048 natural games.
- Policy representation: frozen policy-only structured successor state SHA-256
  `ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0`.
  Gradients are measured in its 48-weight final policy-head basis.
- Value estimator: confirmed bounded width-48 state SHA-256
  `cae8e19ef825325508de351b883b2df3863dc66f0288be06ad2ccf868e3d7d7c`.
- Monte Carlo: `A_t = R_terminal - V(s_t)`.
- GAE: `gamma = 1`, `lambda = 0.95`, zero nonterminal rewards, terminal value
  zero, and the same frozen `V` used by Monte Carlo.
- Weighting: equal episode mass, equal physical-decision mass within episode,
  and seat-wise advantage standardization.
- Independent halves: even versus odd pair index. Pair vectors combine both
  seat-swapped games before gradient signal-to-noise is measured.
- A material transition diagnostic means absolute one-step TD residual at least
  `0.25`. This is a learned outcome-probability transition, not a causal card
  value or an extra reward.

A 64-pair run is the bounded throughput preflight. It may estimate wall time but
cannot change the scientific settings.

## Gate

Advance to one separately frozen matched head-only objective experiment only if:

1. the width-48 external value confirmation remains valid, all predictions are
   finite and bounded, `lambda = 1` reproduces Monte Carlo within `1e-9`, and
   policy-logit reproduction remains within the established `1e-5` numerical
   transport envelope;
2. GAE raw advantage variance is lower than Monte Carlo variance;
3. GAE overall even/odd gradient cosine is nonnegative and at least `0.05`
   greater than Monte Carlo;
4. GAE overall gradient signal-to-noise is at least `1.10` times Monte Carlo;
5. neither seat's GAE signal-to-noise is below `0.90` times its Monte Carlo
   value; and
6. GAE's material-transition versus other-decision signal contrast is at least
   as large as Monte Carlo's.

A failure closes this exact `lambda=0.95` frozen-value estimator without a
policy fit. A pass authorizes only a matched Monte Carlo-versus-GAE head-only
fit on a separate corpus, followed by a fresh integer win-count gate if the
offline objective gate also passes. It does not establish playing strength or
professional-level play.
