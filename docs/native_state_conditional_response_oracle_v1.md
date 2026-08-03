# Native state-conditional response oracle v1

## Question

Can a compact actor-relative response policy turn direct terminal-outcome
optimization into a fresh gain against both Pool3 and pure promoted(2), after
the global action-bias oracle changed trajectories but failed to generalize?

This remains a rapid native development screen. Natural terminal win, draw, or
loss is the only reward, optimizer signal, selector quantity, and promotion
measure. A pass is not evidence of pro-level play.

## Fixed policy and search

- Parent and candidate base: promoted(2), generation 384.
- Development opponent: the resolved Pool3 40/20/20/20 mixture.
- Policy class: 160 bounded additive logit parameters. Sixteen semantic action
  channels are crossed with eight public actor-relative features: intercept,
  life difference, own life, hand-size difference, battlefield creature-count
  difference, battlefield creature-power difference, graveyard-count
  difference, and active-player sign. Six binary action channels distinguish
  pay, cast, boolean value, retarget, attacker inclusion, and blocker inclusion.
  Sixteen stable card-token buckets add an intercept and life-difference term.
- Information boundary: scorer-visible typed state, action semantics, and card
  tokens only. No seed, hidden opponent card, raw arena identity, or future
  state is available.
- Limits: each parameter is in `[-0.5, 0.5]`; total added logit is clamped to
  `[-1.5, 1.5]`. Zero parameters reproduce the parent trajectory exactly.
- Optimizer: deterministic antithetic cross-entropy method, population 40,
  ten rank-weighted elites, eight generations, RNG seed `202608031`, initial
  sigma `0.12`, minimum `0.04`, maximum `0.20`.
- Development: 96 games per candidate using generation seeds
  `1301001 + generation * 10000`.

The eight generation means and the zero parent are selected only after search.
Each receives 256 Pool3 games at seed `1390001` and 256 at seed `1391001`.
Selection maximizes the worse panel's terminal fitness, then summed fitness,
then lower L2, then earlier policy index. This makes the zero parent eligible
and prevents a single favorable selector panel from advancing a policy.

## Fresh gate and disposition

The selected policy and parent receive 512 matched Pool3 games at seed
`1392001` and 512 matched pure promoted(2) games at seed `1393001`. For each
panel, `G` counts episodes where candidate terminal reward exceeds parent and
`L` counts the reverse.

Advance only if both panels satisfy `G >= L + 12`, and
candidate-minus-parent win count is at least `-4` separately at P0 and P1 in
both panels. A pass authorizes XMage scorer integration and one small fresh live
gate. A failure retires this exact compact state-conditional oracle and moves
the project to selective search, without tuning these revealed seeds.

A bounded release throughput screen must first prove bit-identical repeated
zero panels, real trajectory activation by a fixed semantic probe, and a wall
time consistent with completing the formal run in well under one hour.

The release preflight passed. Two zero-residual 16-game panels were
bit-identical, while the fixed semantic probe changed the trajectory-set digest
on the same seeds. Simulation throughput was 31.44 games per second and total
preflight wall time, including Store loading, was 94.45 seconds. Throughput
report SHA-256 is
`ebc29c646e01b7b3e5e72bd7ec2148853edf60753adfc1c237b877c9f3dfb240`.
