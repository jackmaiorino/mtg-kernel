# Native scaled complete-history screen v1

## Question

Does substantially more complete-pair data turn the width-48 structured
public-history model into a seat-stable policy or value improvement?

The fixed 32-pair complete-history screen improved policy imitation but
regressed value MSE by 8.50 percent. The later 256-pair stateless value screen
also regressed by 4.33 percent. This screen changes data scale while preserving
the model family and actor-visible information boundary.

## Fixed screen

- Data: the passing 2,048-pair dual-export corpus at base seed `1400001`.
- Split: four held-out folds by `pair_index mod 4`, exactly 512 pairs per fold.
- Representation: width-48 structured state, object, relation, action, and
  reference encoder, plus a GRU over the last 16 completed public physical
  decisions by either player. Current and future decisions are excluded.
- Heads: zero-initialized residual policy and value heads over the unchanged
  parent outputs. Shared features train jointly with value coefficient `1.0`.
- Fit: 5 epochs, batch size 64, AdamW learning rate `3e-4`, weight decay
  `1e-4`, gradient norm cap 5, and seed `20260802`.
- Execution: four folds concurrently, six CPU threads per fold. Five epochs on
  64 times the prior history corpus expose the model to far more distinct
  trajectories and optimizer steps than the earlier 20-epoch screen without
  spending eight hours on repeated passes over the same examples.

## Gates

Common representation gates require object-permutation delta at most `1e-5`
and reference-removal effects above `1e-4` on at least 20 percent of eligible
held-out decisions.

The policy lane advances only if policy NLL improves by at least 5 percent,
neither acting seat regresses, top-1 is no worse than 0.5 percentage points
below the parent, and at least three of four folds improve policy NLL.

The value lane advances only if episode-balanced value MSE improves by at
least 5 percent, neither candidate seat regresses by more than 2 percent, and
at least three of four folds improve value MSE.

Passing one lane does not require the other lane to pass. A passing lane may
justify one full-data fit and a fresh short live mechanism gate. It does not
promote a model or establish professional-level play. If neither lane passes,
close this exact scaled structured-history model rather than tuning on the
revealed folds.
