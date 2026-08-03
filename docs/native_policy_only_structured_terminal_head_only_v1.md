# Native policy-only structured terminal head-only v1

## Question

Can terminal-only PPO produce a safe fresh Pool3 gain when optimization is
restricted to the structured model's 48 final policy weights, eliminating the
representation drift that rejected the full-network update?

Natural terminal win, draw, or loss remains the only reward, learning signal,
and strength measure.

## Fixed fit

- Initializer: exact qualified policy-only structured successor.
- Corpus: the existing 2,048-pair, 4,096-game on-policy cache at base seed
  `1660001`. Reuse is development training, not fresh strength evidence.
- Frozen tensors: every state, object, relation, action, reference, complete-
  history, combination, policy-bias, and value tensor.
- Trainable tensor: `policy_head.weight`, exactly 48 parameters.
- Objective: unchanged physical-decision joint-ratio terminal PPO, clip `0.10`,
  equal episode mass, terminal return minus frozen parent value standardized by
  candidate seat.
- Fit: five epochs, batch size 64 physical decisions, AdamW learning rate
  `3e-4`, zero weight decay, gradient cap 5, seed `20260806`.
- Throughput: precompute the frozen 48-dimensional policy input for every row,
  then optimize only the final dot product.

Publish only if every frozen tensor remains bit-exact, the original overall
and both-seat mean TV maximum `0.030`, p90 TV maximum `0.100`, and joint log-
ratio maximum `0.50` all pass. Native logit error must be at most `3e-5`, and
the retained parent value must remain bit-exact.

## Fresh strength gate

If publication passes, compare the head-only candidate with the qualified
initializer on 1,024 fresh seat-swapped Pool3 pairs at base seed `1680001`.
Pass only if matched gains are at least losses plus 20, candidate wins are at
least initializer wins plus 20, and each candidate-seat win delta is at least
`-4`.

A pass establishes a Rally-only Pool3 gain. A failure retires structured
terminal PPO in this data regime and moves to direct terminal policy-space
response-oracle optimization. Neither result is professional-level evidence.
