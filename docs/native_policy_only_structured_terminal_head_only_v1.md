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

## Result

The 48-weight fit passed every numerical and frozen-state gate. Overall mean
TV was `0.00100184`, p90 TV was `0.00287721`, and maximum absolute physical-
decision joint log ratio was `0.217552`. Every non-head tensor remained bit-
exact. Native maximum absolute logit error was `0.000001907`, and the retained
parent value was bit-exact.

The fresh base-`1680001` 1,024-pair gate completed in 336.39 seconds. The
candidate won 1,282 games and the initializer won 1,281. Matched `G/L/T` was
`2/1/2045`, with candidate-minus-initializer win deltas `0` at P0 and `+1` at
P1. All validity, transport, exact-pair, natural-terminal, and seat-floor gates
passed. Both required +20 strength gates failed.

This retires structured terminal PPO in the tested data regime. The full-
network trust projection and head-only update were both weakly positive but
changed too few trajectories to establish strength. Do not tune either update
on its revealed panel. Strength report SHA-256 is
`e138f56bd00047c4513274151ffca149791f89d42ed45337be97af2a1293c94d`;
evidence root is
`D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal`.
