# Native bounded-logit terminal residual v1

## Question

Can the broad terminal-only PPO update produce a fresh Pool3 gain when its
rare-action tail is bounded at the policy output instead of shrinking every
parameter by the same factor?

Natural terminal win, draw, or loss remains the only reward, learning signal,
and strength measure.

## Fixed mechanism screen

- Initializer: the exact qualified policy-only structured successor.
- Direction: the exact rejected full-network terminal PPO state from base seed
  `1660001`. No refit or outcome-dependent parameter tuning is performed.
- For each policy substep, evaluate both states. Subtract the initializer-
  probability-weighted mean from the trained-minus-initializer logit residual,
  clamp each centered residual to `[-c, +c]`, and add it to the initializer
  logits. The retained parent value remains bit-exact.
- Screen `c` in the fixed grid `0.03, 0.04, 0.05, 0.06, 0.08, 0.10, 0.12,
  0.16, 0.20, 0.24, 0.28, 0.32, 0.40` on the
  existing training cache. Select the largest value passing the original
  overall and both-seat mean-TV maximum `0.030`, p90-TV maximum `0.100`, and
  physical-decision joint-log-ratio maximum `0.50`.
- Require at least four times the qualified 1/16 projection's movement:
  selected mean TV at least `0.0071903344` and weighted top-action change rate
  at least `0.0023608848`. These are mechanism checks, not strength evidence.

The 128-pair throughput preflight completed in 135.93 seconds. Its largest
initial grid point, `c=0.10`, was still far inside the joint-ratio envelope at
`0.190773`. Before the full formal screen and without reading any fresh game
outcome, the grid was extended toward the frozen `0.50` boundary and the
movement floor was tied to the prior qualified projection instead of an
unsupported absolute one-percent top-action threshold.

The screen uses no fresh game outcomes. If no grid point passes, retire this
mechanism without native implementation or strength games.

## Transport and fresh strength gate

On a mechanism pass, publish a strict package containing both exact structured
states and the selected clamp. Python and Rust logits must agree within
`3e-5`; `c=0` must reproduce the initializer; and the parent value must remain
bit-exact. One repeated native seed must be bit-identical.

Then compare the bounded candidate with the initializer on 1,024 fresh
seat-swapped Pool3 pairs at base seed `1900001`. Pass only if matched gains are
at least losses plus 20, candidate total wins are at least initializer wins
plus 20, and candidate-minus-initializer wins are at least `-4` separately at
P0 and P1. Every terminal must be natural and all identity, transport, and
pairing checks must pass.

A pass establishes a Rally-only native Pool3 improvement. A failure retires
this exact output-bounded direction without tuning on the fresh panel. Neither
outcome establishes CP7, human, cross-deck, or professional-level strength.
