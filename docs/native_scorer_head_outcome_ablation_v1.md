# Native scorer-head outcome ablation v1

## Question

Did the previous full-network outcome update lose a useful policy-head change by simultaneously moving the encoders and value path on a small terminal-outcome corpus?

## Hypothesis source

The iteration-2 full-network policy-scale-2 child was correctly rejected. Across bases `1050001` and `1060001`, it produced paired `G/L/T = 3/1/124` against the retained parent, a weak two-game net with one-sided exact sign-test value `0.3125`.

That result is not validation for this ablation. It only motivates isolating the existing nonlinear scorer from the roughly 1.22 million other model parameters that moved in the full child.

## Fixed candidate

- Parent: retained manifest `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb` at Adam step 1.
- Corpus: exact base-`1040001` 32-pair outcome JSONL SHA-256 `b75677397c8461a702bdb5d0f7dfc47fe651e2cd1d4f048cc218001055a828cd`.
- Objective: the existing standardized, frozen-source-value, equal-episode-mass objective.
- Update: one full-corpus Adam step with learning rate `0.001`, value coefficient `0.05`, and policy scale `2.0`.
- Active tensors: `scorer.0.weight`, `scorer.0.bias`, and `scorer.2.weight`, totaling 8,320 scalar parameters.
- Frozen tensors: every encoder, embedding, and value tensor plus the gauge-fixed `scorer.2.bias`.
- Frozen optimizer state: first and second moments for every frozen tensor remain bit-identical to the parent. The global Adam step advances from 1 to 2.
- Implementation identity: perform the ordinary simultaneous one-step update, then atomically restore every frozen parameter and frozen moment from the pre-update snapshot.

For a single simultaneous update, the active scorer parameters and moments must be bit-identical to the existing full child manifest `34cd78edf4c10f3398cc8ed798b08ae8e1d3caecacbaa1520a697b15c620f2ad`. Every frozen parameter and moment must be bit-identical to the retained parent. Failure of either invariant rejects the package before live games.

This is an ablation of the model's existing nonlinear scorer. It does not add or tune a bilinear residual, projection, rank, or runtime logit overlay.

## Fresh qualification rule

Run the scorer-head candidate and retained control sequentially on the same 16 seat-swapped pairs at never-used base seed `1150001`, episodes `0..31`.

The candidate passes the first gate only if all conditions hold:

- Paired gains satisfy `G >= L + 2`.
- Net `G - L` is at least `-1` separately for candidate P0 and candidate P1.
- Both blocks complete all 32 games with identical episode seats and environment seeds.
- Candidate priority projections are zero and there is no scorer fallback, alignment, protocol, or identity failure.

Here, a gain is an episode the scorer-head candidate wins and the retained control loses. A loss is the reverse, and a tie has the same model win/loss result in both runs.

Failure retires the scorer-head ablation without changing its data, scope, learning rate, policy scale, or gate. Base seed `1150001` must not be used to tune another version.

## Confirmation rule

Only a first-gate pass authorizes 32 additional fresh seat-swapped pairs at base seed `1160001`. No parameter changes occur between blocks.

The pooled 48-pair result qualifies for broader work only if:

- Pooled paired net is at least `+4`.
- The one-sided exact sign-test value over discordant outcomes is at most `0.10`.
- Pooled paired net is at least `-2` separately for candidate P0 and candidate P1.
- Both confirmation blocks satisfy the same zero-projection and fail-closed transport conditions.

A pooled pass is evidence to collect a larger fresh training corpus for a proper scorer-head update. It is not a pro-level claim or automatic promotion.

## Result

Implemented and rejected at the first fresh qualification gate.

The formal scorer-head package is
`/mnt/d/mtg-kernel-xmage-cp7-outcome-base1040001-std-epbal-lr1e-3-vc0p05-ps2-scorer-head-v1` with these identities:

- Manifest: `8438f7eeb9466d12fb0a5681989886571f40870f0d71c85a0e2289afffaa9b54`.
- Payload: `88a9c91650b20ccbb5a2820fcb5c7ec315db4a1658aefa56ffd840092aa98579`.
- Native train state: `b251538890b7136b7324511967fa1f9fd4f0442723cf7c59b15e8e1f5aec0d20`.
- Model parameters: `3984203b86237034f054be7d5fef6630d2eace405f456ba0e125c38a43690f6f`.
- Adam step: `2`.

The external three-way audit passed. All three active parameter tensors and their
first and second moments bit-match the full policy-scale-2 child. All frozen
parameters and moments bit-match the retained parent, including inherited nonzero
moments. The gauge-fixed `scorer.2.bias` remained frozen.

On all 2,541 physical decision groups in the exact training corpus, candidate versus
parent policy movement was:

- Mean total variation: `0.0008456223682571215`.
- P90 total variation: `0.001779408768610935`.
- Mean KL divergence: `0.00002742591287472332`.
- Maximum absolute joint log ratio: `0.14769850630178105`.
- Clipped groups: `0`.

The Windows scorer used for live qualification had SHA-256
`6325132093a2e90f5a209dd344ebcf146fe2c4a5633493772d6f197b5e8f036d`.

At fresh base seed `1150001`, both candidate and retained control completed all 32
games at `15-17` against CP7. The paired result was `G/L/T = 0/0/32`, with seat
nets `0` for candidate P0 and `0` for candidate P1. Episode number, candidate seat,
and environment seed matched for all 32 rows. Both blocks had zero priority
projections and no fallback, alignment, protocol, or identity failure. Episodes 21
and 30 had different trajectories, but neither changed the terminal result.

The qualification requirement was `G >= L + 2`; the observed result was `0 >= 2`,
which is false. The scorer-head ablation is retired. Base seed `1160001` was not run,
and no scorer-head learning-rate, policy-scale, or scope sweep is authorized from
this result.

Raw live logs:

- Candidate: `/mnt/d/mtg-kernel-scorer-head-cp7-base1150001-candidate.log`, SHA-256
  `5c19939f97a2ea40e6ddbbbf3de9367a24c9000be6fbee6a55819985bce20e0c`.
- Control: `/mnt/d/mtg-kernel-scorer-head-cp7-base1150001-control.log`, SHA-256
  `8d2fde43b014c822f97357334f3a6ea50ce0020bb89472ae72bbd63396ef9370`.
