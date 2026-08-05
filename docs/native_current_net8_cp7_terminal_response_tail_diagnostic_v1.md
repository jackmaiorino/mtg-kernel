# Current Net8 CP7 terminal response tail diagnostic v1

Status: fixed development diagnostic. No fresh gameplay is authorized.

Replay the exact v2 policy-only arm from retained GAE8 on the exact revealed
corpus. Record, in addition to the existing movement report:

- p99 and maximum action TV over all 4,769 decision rows;
- p99 absolute joint log ratio over all physical groups;
- physical-group counts above absolute joint log ratios `1.0`, `1.5`, and
  `2.0`;
- the pair, episode, seat, decision kind, substep count, signed joint log
  ratio, and likelihood ratio of the maximum-absolute-log-ratio group.

The recipe remains value coefficient `0.0`, learning rate `0.001`, fixed
step-520 `pi_old`, PPO clip `0.10`, four full-batch updates, unchanged Adam,
and terminal win, draw, or loss only. The replay must reproduce the v2 final
payload SHA-256
`c1f269fff21c296bee9e53bd3ac8000ecd738957dc3c2f3a144999b2d304c89e`.

This diagnostic selects no model, publishes no package, changes no threshold,
and touches none of seeds `1830001`, `1840001`, or `1850001`. Its only purpose
is to decide whether the current cap is reacting to an isolated low-mass tail
or a broad unsafe distribution. GPU 1 remains reserved; projected wall time is
31 seconds plus the optimized rebuild.
