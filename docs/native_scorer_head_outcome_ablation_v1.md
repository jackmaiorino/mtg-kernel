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

Pending implementation, exact splice verification, and the fresh qualification block.
