# Native response oracle CEM v1

## Question

Can a deliberately small policy class find terminal-win improvements that the
failed Net8 gradient recipes missed when optimized directly against the
explicit Pool3 opponent mixture?

This is a rapid native development screen. Natural terminal win, draw, or loss
is the only reward and the only source of optimizer fitness. A pass is evidence
for advancing this response-oracle policy, not evidence of pro-level play.

## Fixed screen

- Parent and candidate base: promoted(2), generation 384.
- Development opponent: the resolved Pool3 mixture, with its existing fixed
  40/20/20/20 member selection.
- Policy class: 33 bounded additive logit terms over 27 typed action kinds and
  six explicit action flags. The frozen checkpoint supplies every other logit
  component. Each term is restricted to `[-1.5, 1.5]`.
- Optimizer: deterministic antithetic cross-entropy method with 20 candidates,
  five rank-weighted elites, five generations, initial sigma `0.35`, minimum
  sigma `0.08`, maximum sigma `0.50`, and RNG seed `20260803`.
- Development panels: 128 games per candidate. Generation seed bases are
  `1281001 + generation * 10000`.
- Selection anchor: 256 Pool3 games at seed `1289001` after each generation.
  The zero residual parent is eligible throughout and wins ties by lower L2.
- Fitness: twice total terminal reward plus the worse-seat terminal win/loss
  net. No shaped, intermediate, or clairvoyant reward is used.
- Fresh panels: 512 matched games against Pool3 at seed `1290001`, then 512
  matched games against pure promoted(2) at seed `1291001`.

The bounded release throughput screen repeated a 16-game panel bit-identically
and measured 29.31 games per second of simulation. Store validation dominates
startup. The formal run is expected to take about 12 minutes after compilation
and uses CPU execution only.

## Gate and disposition

For each fresh panel, `G` counts episodes where candidate terminal reward is
greater than parent reward and `L` counts the reverse. Advance only if both
Pool3 and pure promoted(2) satisfy `G >= L + 12`, and candidate-minus-parent win
count is at least `-4` separately at P0 and P1 in both panels.

A pass authorizes integration of the selected residual into the XMage scorer
and one small fresh live gate. A failure retires this exact 33-parameter CEM
oracle without tuning on revealed seeds. It does not reject population training,
terminal-outcome optimization, richer response policies, or lookahead.

## Completed result

The fixed release run completed cleanly in 616.64 seconds. Development-anchor
fitness started at 164. Generation 4 improved it to 176, and generation 5 tied
that fitness but lost the lower-L2 tie break. This development improvement did
not survive the untouched panels.

Against fresh Pool3 seeds, parent and candidate both scored 310 wins and 202
losses. Paired `G/L/T` was `6/6/500`, with candidate-minus-parent seat win
deltas of `-1/+1`. Against fresh pure promoted(2), parent scored 243 wins and
269 losses while candidate scored 246 wins and 266 losses. Paired `G/L/T` was
`7/4/501`, with seat deltas of `+1/+2`. Both seat floors passed, but neither
panel met the required paired-gain margin of 12.

Formal report SHA-256 is
`b52bd0e1558410e873daed822731292eb40e7b8dfc96f1b0c3f8893486c2eb0e`.
The exact 33-parameter action-kind and flag residual is rejected and must not
be tuned on these revealed seeds. The result shows that direct terminal CEM can
alter native trajectories and optimize its fixed development anchor, but this
policy class did not produce a robust fresh advantage. The next branch must add
state-conditional response capacity rather than repeat global action biases.
