# Native trust-projected recurrent learner v2

## Question

Can the recurrent structured direction that passed every v1 signal gate retain its
fresh held-out terminal signal when every physical decision is hard constrained to
the deployment log-ratio envelope?

Natural terminal win, draw, or loss remains the only reward and value target. This
is a new mechanism screen on fresh data, not a post-hoc rescore of v1.

## Fresh corpus and fixed model

- Collect 1,024 seat-swapped pairs from the exact qualified structured successor
  against Pool3 `40/20/20/20`, base seed `1910001`, pair indices `0..1023`.
- Freeze the resulting teacher and outcome hashes, then construct one validated
  complete-history cache. No model fit begins before those identities are pinned.
- Use the exact v1 width-128 recurrent structured model, history length 32,
  one behavior-distillation epoch, three terminal-outcome epochs, optimizer,
  losses, seeds, and four whole-pair folds.
- Replace only the unconstrained direct policy output. For each policy substep,
  interpolate from parent logits toward raw candidate logits using the largest
  bisection scale for which every action's absolute log-probability ratio is at
  most `0.49 / substep_count`. Sixteen fixed bisection steps are used.
- This guarantees every possible selected physical decision has absolute joint
  log ratio at most `0.49`, leaving numerical room under the `0.50` gate.

## Throughput and gate

Repeat the bounded GPU 1 batch screen on 128 fresh pairs and select topology by the
same 95-percent rate and 5 GiB memory rule. Run the selected arm twice and require
identical packed-input, loss-trace, and state hashes.

The cross-fit gate is unchanged from v1: positive overall and both-seat held-out
terminal surrogate, at least three positive folds, positive pair-bootstrap 90
percent lower bound, mean TV in `[0.01, 0.05]`, p90 TV at most `0.15`, maximum
physical-decision joint log ratio at most `0.50`, value MSE improvement at least
5 percent overall with no seat regression, and all permutation, reference, digest,
identity, finiteness, and deterministic-repeat checks.

A pass authorizes a native port and fresh matched strength gate. A failure closes
this projected recurrent direction. It does not authorize tuning on the revealed
fresh folds, promotion, or a professional-level claim.
