# Native structured policy-space response oracle v1

## Question

Can direct terminal-outcome optimization find a robust response in the learned
structured policy space after advantage-based terminal PPO produced only tiny
fresh effects?

Natural terminal win, draw, or loss is the only optimizer fitness and strength
measure. No action label, value target, intermediate reward, or heuristic is
used.

## Fixed policy space and optimizer

- Base: exact qualified policy-only structured successor.
- Adapter: an additive delta on the 48 learned `policy_head.weight` channels.
  Every other policy and value tensor is frozen. Each delta is bounded to
  `[-0.05, 0.05]`.
- Opponent: exact Pool3 `40/20/20/20` mixture.
- Optimizer: deterministic antithetic CEM, population 20, five rank-weighted
  elites, six generations, seed `20260807`, initial sigma `0.01`, sigma bounds
  `[0.003, 0.02]`.
- Development: 128 natural games per candidate with common random numbers;
  generation seeds start at `1710001` and advance by `10000`.
- Fitness: twice total terminal reward plus the worse candidate-seat terminal
  reward sum.
- Topology: all 20 generation candidates run concurrently after a bounded
  four-candidate activation and throughput screen.

The zero adapter and six generation means receive two untouched 256-game
Pool3 selector panels at seeds `1770001` and `1780001`. Selection maximizes
the worse-panel fitness, then summed fitness, then lower L2, then earlier
policy index. The zero adapter remains eligible.

## Publication and strength

The selected policy is publishable only if the original overall and both-seat
mean TV maximum `0.030`, p90 TV maximum `0.100`, and joint log-ratio maximum
`0.50` pass on the fixed on-policy cache. Native logit error must be at most
`3e-5`, and the retained parent value must be bit-exact.

If publishable, compare it with the qualified initializer on 1,024 fresh
seat-swapped Pool3 pairs at base seed `1790001`. Pass only if matched gains are
at least losses plus 20, candidate wins are at least initializer wins plus 20,
and each seat delta is at least `-4`.

A pass establishes a Rally-only response gain and authorizes adding the policy
to an empirical payoff matrix. A failure retires this exact 48-channel direct
response oracle. Neither result is professional-level evidence.
