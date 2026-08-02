# Native candidate-action history falsifier v1

## Question

Does a compact chronological summary of the candidate's prior public actions remove the structured adapter's repeated P1 value regression?

This is a partial public-history falsifier. The existing JSONL corpora record candidate decisions but not every opponent action, so it cannot establish the value of complete two-player public history. A positive result would justify adding full public action history to the engine observation contract.

## Fixed design

- Reuse the exact structured-screen corpora, whole-pair four-fold split, parent outputs, seeds, optimizer, width, epochs, and acceptance gates.
- Keep the structured object, edge, action-reference, and action-conditioned attention path unchanged.
- Add one history encoder over at most the 16 most recent completed candidate physical decisions in the same episode.
- Represent each completed physical decision by the mean of its selected autoregressive substep action vectors.
- Consume only the first 99 explicit action features. Exclude the 96-feature action digest tail.
- Exclude the current physical decision and all future decisions.
- Use a 48-wide GRU final state as an additive input to the existing state context.
- Keep zero residual output initialization, so the initial policy and value remain exactly the parent.

The same seven gates apply. The decisive requirement is that pooled P1 value MSE no longer regresses by more than 2%, while overall held-out policy and value gates, both seat floors, permutation invariance, and reference sensitivity still pass.

## Disposition

- Pass: implement complete actor-visible two-player public history in the native observation contract and confirm on fresh pairs before any live candidate.
- Fail: candidate-only action history is insufficient. Do not tune sequence length, GRU width, epochs, or thresholds on these folds. Move to a complete public-history contract or the selective-search lane.

No XMage games or GPU training are part of this falsifier.
