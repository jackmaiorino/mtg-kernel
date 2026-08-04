# Native composed credit factorial development v1

## Question

Does the qualified complete-public-history value critic with terminal GAE improve
learning relative to the current frozen-parent-value Monte Carlo objective, and
does that answer depend on current Net8 versus structured width-48 policy
initialization?

This is a rapid development rung, not a formal strength gate. Natural terminal
win, draw, or loss is the only reward and promotion measure. The history critic
may only propagate that terminal result.

## Two rows

The comparison is the 2x2 `{current Net8, structured width-48}` initialization by
`{frozen-parent value plus Monte Carlo, complete-history critic plus GAE}`.

The current-Net8 row runs first because its native complete-history seam and
4-worker by 16-session topology are already qualified. Each arm starts from the
same update-512 checkpoint and uses the same eight consecutive 64-game root
batches, Pool3 opponent mixture, environment-randomization-v2 contract,
optimizer, learning rate, value coefficient, and zero entropy bonus. The only
arm difference is the policy credit estimator. On-policy outcomes, per-seat
outcomes, entropy, value error, gradient norm, parameter movement, and state
digests are recorded per update.

The structured row may run only after identifying a training path that changes
the credit estimator without silently changing trainable parameters, optimizer,
data panel, weighting, or trust geometry between its two arms. A single bundled
structured-plus-GAE run is not an attributable substitute.

## Interpretation

All eight-update results are development evidence. On-policy training outcomes
are diagnostic because the two arms induce different later trajectories. A
finite, stable mechanism trajectory with no collapse is required before any
fresh matched Pool3 evaluation. The current-Net8 GAE cell advances from that
screen only if, on 1,024 fresh common Pool3 episodes, it has a strictly positive
paired win net against the MC sibling and no fewer total wins than the shared
parent. This development gate authorizes only a separately sized formal
strength design. Non-finite values, root/schedule drift, critic identity
mismatch, malformed complete histories, or reward-contract drift stop the
affected arm.
