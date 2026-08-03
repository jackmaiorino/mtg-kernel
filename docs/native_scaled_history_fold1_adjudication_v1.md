# Native scaled history fold-1 adjudication v1

## Question

Was fold 1's `1.28746032714844e-5` object-permutation delta a real
order-dependence defect or float32 reduction-order noise?

The original scaled screen remains formally failed. This diagnostic does not
rewrite that result. It determines whether the strong held-out policy and value
signals justify fitting a live candidate.

## Fixed reproduction

- Input: exact complete-history cache SHA-256
  `721aeeb8389464676edf1190b4e90d74ced286104cc0fb30deb46d36ffbc8090`.
- Fold: `pair_index mod 4 == 1` held out, with 1,536 fit pairs and 512
  held-out pairs.
- Training: unchanged width 48, last 16 complete public decisions, 5 epochs,
  batch 64, AdamW `3e-4`, weight decay `1e-4`, seed `20260803`, and 6 CPU
  threads.
- Full held-out policy and value metrics remain exhaustive.
- The trained state is saved atomically before evaluation.
- Invariant diagnostics use a deterministic 4,096-example sample. Digest
  ablations use 4,096 examples per lane.

The cache audit found 4,096 exact episodes, 512 pairs per fold, no pair overlap
between folds, complete physical-decision chronology, and only post-action
public card semantics. The current physical decision and all future decisions
are excluded from history.

## Advance conditions

Advance to one saved full-corpus fit only if all hold:

1. The five training-history rows and full held-out policy/value fields are
   exactly equal to the original fold-1 result.
2. Float64 object permutation maximum output delta is at most `1e-10`.
3. Float32 object permutations cause zero policy argmax changes on the sample.
4. Reference removal affects at least 20 percent of eligible sampled examples.
5. The pre-evaluation model-state file exists and its SHA-256 is recorded.

This diagnostic provides no live strength, promotion, or professional-level
claim.
