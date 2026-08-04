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

Pending.
