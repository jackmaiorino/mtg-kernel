# Native structured policy-block response oracle v1

## Question

Can direct terminal-outcome optimization improve the qualified structured
policy by redistributing the known terminal-PPO displacement across semantic
network blocks?

Natural terminal win, draw, or loss is the only optimizer fitness and strength
measure. The frozen PPO displacement was also trained only from terminal
outcomes and a frozen learned-value estimator. No action label, heuristic
reward, or engine evaluation enters this oracle.

## Fixed subspace and optimizer

- Base: exact qualified policy-only structured successor.
- Direction source: the rejected five-epoch terminal-PPO state, used only as a
  displacement basis from the qualified initializer.
- Blocks: state, history, objects, relations, action, references, query,
  combine input, combine output, and policy head. The value head is exact and
  excluded.
- Coefficients: ten nonnegative block scales, each at most `0.125`, with total
  L1 budget at most `0.625`. Uniform `0.0625` exactly reproduces the previously
  qualified 1/16 trust projection.
- Opponent: exact Pool3 `40/20/20/20` mixture.
- Optimizer: deterministic projected antithetic CEM, population 20, five
  rank-weighted elites, six generations, seed `20260808`.
- Anchors: exact zero and uniform 1/16 are evaluated in every generation.
- Development: 512 natural games per candidate with common random numbers;
  generation seeds start at `1800001` and advance by `10000`.
- Fitness: twice total terminal reward plus the worse candidate-seat terminal
  reward sum.
- Topology: all 20 candidates run concurrently after a bounded four-candidate
  repeatability, activation, and throughput screen.

Zero, uniform 1/16, and the six generation means receive two untouched
512-game Pool3 selector panels at seeds `1860001` and `1870001`. Selection
maximizes worse-panel fitness, then summed fitness, then lower coefficient L2,
then earlier policy index. Zero remains eligible.

## Publication and strength

The selected policy must pass overall and both-seat mean TV maximum `0.030`,
p90 TV maximum `0.100`, and joint log-ratio maximum `0.50` on the fixed
on-policy cache. Native logit error must be at most `3e-5`, and retained parent
value must be bit-exact.

If publishable, compare it with the qualified initializer on 1,024 fresh
seat-swapped Pool3 pairs at base seed `1880001`. Pass only if matched gains are
at least losses plus 20, candidate wins are at least initializer wins plus 20,
and each seat delta is at least `-4`.

A pass authorizes adding the response to the empirical payoff matrix. A zero
selection, movement rejection, or fresh-gate failure retires this exact
blockwise basis. No result is a professional-level claim.

## Result

The bounded preflight passed in `98.25` seconds. The exact zero replay was
bit-identical, and both uniform 1/16 and concentrated block probes changed
normalized trajectories.

All six search generations completed. On their 512-game development panels,
the best searched policy exceeded zero by `0`, `3`, `1`, `1`, `1`, and `2`
wins. The untouched selector chose generation mean 2, index 3. It improved
from `334` to `336` wins on seed `1860001` and tied at `322` wins on seed
`1870001`; worst-panel fitness tied at `320`, while summed fitness improved
from `696` to `706`.

The selected policy passed movement and native transport. Overall mean TV was
`0.00201308`, p90 TV was `0.00600575`, maximum joint log-ratio was
`0.273447`, native maximum logit error was `0.000002861`, and retained parent
value was bit-exact.

The fresh base-`1880001` strength gate failed. Candidate and initializer each
won exactly `1300` of 2048 games. Matched `G/L/T` was `6/6/2036`, and both
seat deltas were `0`. This retires the exact ten-block displacement basis
without post-hoc tuning. Strength report SHA-256 is
`36bc453d7263ba86032d157132b759d40991a233c097ad4bff7812f6ec1fa2c4`.
