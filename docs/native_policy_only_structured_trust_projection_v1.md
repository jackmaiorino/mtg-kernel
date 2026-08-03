# Native policy-only structured trust projection v1

## Question

Does the terminal-only PPO direction contain a fresh Pool3 win-rate gain after
its rare-action tail is constrained, even though the unprojected package failed
the frozen maximum joint-log-ratio gate?

Natural terminal win, draw, or loss remains the only reinforcement-learning
reward, training signal, and strength measure.

## Fixed projection

- Source: exact rejected fit report SHA-256
  `355c1b179ccd5de5d16f0aeb39dc101ae97a876208a2315358f98b06dcc30a81`
  and unpublished state SHA-256
  `4d1e9853d3472eb8817c10051c5ff779258bc1fc26130e956492ad598c877fe9`.
- Initializer: exact qualified policy-only structured successor state SHA-256
  `ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0`.
- Operation: for every floating model tensor, set
  `projected = initializer + (trained - initializer) / 16`. Non-floating
  tensors must be unchanged. The retained parent value must remain bit-exact.
- Data: reuse the formal training cache only to measure numerical movement and
  construct transport fixtures. Do not optimize, search a scale, or use the
  reserved strength outcomes.

Publish only if the original overall and both-seat mean TV maximum `0.030`,
p90 TV maximum `0.100`, and physical-decision joint log-ratio maximum `0.50`
all pass. Native maximum absolute logit error must be at most `3e-5`, and the
retained parent value must be bit-exact.

## Fresh strength gate

If publication passes, compare the projection with the qualified initializer
on 1,024 fresh seat-swapped Pool3 pairs at base seed `1670001`. Pass only if
matched gains are at least losses plus 20, projected-policy wins are at least
initializer wins plus 20, and each candidate-seat win delta is at least `-4`.

A pass establishes a Rally-only Pool3 gain and authorizes broader evaluation.
A failure retires this learned direction and moves to a different learning or
population mechanism. Neither result is evidence of professional-level play.

## Result

The fixed 1/16 projection passed every numerical and transport gate. Overall
mean TV was `0.00179758`, p90 TV was `0.00447179`, and maximum absolute
physical-decision joint log ratio was `0.422299`. Both candidate seats passed.
Native maximum absolute logit error was `0.000002861`, and the retained parent
value was bit-exact.

The fresh 1,024-pair Pool3 strength gate completed in 337.34 seconds. The
projection won 1,310 games and the initializer won 1,306. Matched `G/L/T` was
`5/1/2042`, with projection-minus-initializer win deltas `+3` at P0 and `+1`
at P1. Identity, transport, natural-terminal, exact-pair, and both seat-floor
gates passed. The required +20 paired net and +20 total-win gates failed.

The result is directionally favorable but too small to distinguish as a
strength improvement under the frozen gate. Do not promote it or tune another
scale on base seed `1670001`. This retires the learned direction and closes the
terminal-only structured PPO branch tested here. Strength report SHA-256 is
`ba4374f9a6505a6feeab28b166dc7097d1508658215332719923d65effa889b4`;
evidence root is
`D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal`.
