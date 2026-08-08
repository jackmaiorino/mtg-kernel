# Native scaled self-play population refresh algorithm v1

This note fixes the mechanical order and arithmetic left implicit by the
authorized population program. It adds no new reward, measurement, or
promotion criterion.

At each completed 128-update boundary, first bind the next eight policy
identities from the program's outcome-free seed rotation. Carry an unchanged
exploiter or historical-fallback identity through boundaries where no response
rebuild is scheduled. The complete 28-matchup payoff matrix evaluates these
newly bound identities before the next training interval begins.

For policy `i`, let `u_i` be the sum of terminal ranks against the other seven
policies over exactly 7,168 games. A win has rank `+1`, a draw `0`, and a loss
`-1`. The normalized multiplicative-weights input is `p_i=u_i/7168`.
Nonterminal rewards and gameplay proxies are not inputs.

Let `w_i` be the prior weight of slot `i`. Deterministic slot rotation carries
the prior slot weight to its newly bound identity. Compute
`r_i=w_i*exp(0.10*p_i)` and normalize the eight `r_i` values to sum to one.
Project deterministically onto the declared constraints by repeatedly capping
policies above 25%, redistributing free mass proportionally, raising any
two-policy role below 20% while preserving its internal ratio, and rescaling
the remaining roles proportionally. Stop when all constraints hold.

Convert projected weights to one million integer units by largest remainder.
Remainder ties use ascending slot index. A one-unit repair may move mass from a
role above its floor to a deficient role, preserving the total, role floors,
and policy cap. The refresh binds the exact payoff-panel SHA-256 and the prior
refresh SHA-256. Any identity mismatch, incomplete panel, reused pair seed,
non-natural terminal, or invalid weight blocks the next interval.
