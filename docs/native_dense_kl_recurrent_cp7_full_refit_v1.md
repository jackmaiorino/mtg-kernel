# Native dense-KL recurrent CP7 full refit v1

## Purpose

Refit the already-selected beta-6 recurrent residual on every available labeled
candidate state before a second disjoint generalization gate. The first fresh
panel is now training data and will never be evaluated again.

## Fixed fit

- Original panel: pair indices 0 through 255, 14,525 labels.
- First fresh panel: pair indices 256 through 319, 3,477 labels.
- Combined fit: 320 whole pairs, 640 natural games, 18,002 labels, with equal
  total mass for each of the 633 label-bearing episodes.
- Architecture: width-128 recurrent structured residual with zero-initialized
  policy head and frozen ignored value head.
- Objective: all-row CP7 selected-index cross entropy plus
  `6 * KL(parent || candidate)` under the hard `0.49` legal-action log-probability
  budget.
- Optimizer: AdamW `3e-4`, weight decay `1e-4`, gradient cap `5`, batch 256,
  eight epochs, seed `20260810`, exclusive GPU 1.
- No checkpoint, coefficient, threshold, or architecture selection occurs.

The resulting model is eligible only for one second fresh CP7 label gate on pair
indices 320 through 383. Passing that gate would justify native transport work,
not a strength claim. Natural terminal win/loss remains the only promotion test.

## Result

The fixed refit completed in 61.82 seconds. Model-state SHA-256 is
`d736296425de2c438bb9be02ab6c89e51da4c17c1408de6ff3309029b2d06dca`;
model-file SHA-256 is
`6c33f6d449b76e24c00bc7d46052b04488ddb9ec574009831d2fa90ea01bd55d`.
On its training corpus, NLL improved 5.26 percent and top-1 improved 3.96
percentage points, with mean TV `0.028163` and p90 TV `0.081173`. These are fit
diagnostics only. Fit report SHA-256 is
`7c333e8bec2d332eb5dfba764f29df39d801211e74c0052bb2fd8555c68455f4`.

The second disjoint panel completed 128 natural games at pair indices 320 through
383. All 3,679 labels were usable; one game had no candidate priority label; and
parent-teacher disagreement was 47.76 percent. Collection report SHA-256 is
`d53e1afcc4a772d5d7628a94f4a58e3b2adbbdb676d63fb5c478c4649842956c`.
Its exact complete-history cache SHA-256 is
`e542413e4269daa2176143acebe82a71e0d9f46cc3ebbb0bfd2face8b1390c99`.
