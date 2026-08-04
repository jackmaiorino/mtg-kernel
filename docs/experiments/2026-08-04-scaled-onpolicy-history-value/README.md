# Scaled on-policy complete-history value bootstrap

Status: frozen before training.

## Question

Can the qualified structured policy's complete-public-history representation learn a seat-stable terminal value bootstrap from the existing 2,048-pair on-policy Pool3 corpus?

This is the remaining distinct prerequisite for learned-value short-horizon search. Natural terminal win, draw, or loss is the only target. A pass authorizes a fresh search mechanism screen, not a live policy or strength claim.

## Fixed data and model

- Cache: `D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal\cache.pt`, SHA-256 `454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d`.
- Data: 2,048 seat-swapped pairs and 4,096 natural Rally games generated on-policy by the qualified structured successor against the exact Pool3 `40/20/20/20` mixture at base seed `1660001`.
- Split: four whole-pair folds by `pair_index mod 4`.
- Initialization: exact qualified policy-only structured state SHA-256 `ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0`.
- Inputs: width-48 structured state, objects, relations, legal actions, action references, digest channels, and the last 16 complete public physical decisions from both actors.
- Target: actor-relative natural terminal reward. The retained parent value is the fixed baseline and the model learns a value residual.
- Train all representation and value tensors. Keep the policy head frozen; this is a separate value model and is never deployed as a policy.
- Fit each fold for five epochs, batch size 32 physical decisions, AdamW learning rate `3e-4`, weight decay `1e-4`, gradient cap 5, seed `20260809`.

## Throughput and execution

Run one 128-pair, one-epoch profile before formal training. It may choose only execution topology. Formal training uses four concurrent folds with six CPU threads each unless the profile or memory check shows that fewer concurrent folds give better wall time. Scientific settings stay fixed.

## Held-out gates

All must pass:

1. Aggregate episode-balanced value MSE improves by at least 5 percent over the frozen retained value.
2. Neither candidate seat regresses by more than 2 percent.
3. At least three of four folds improve overall MSE.
4. Mean absolute value residual is at most 0.25, weighted p90 residual is at most 0.50, and maximum absolute prediction is at most 1.50.
5. Object permutation changes value by at most `1e-5`.
6. Removing valid action references changes value by more than `1e-4` for at least 20 percent of 1,024 sampled held-out decisions.

## Disposition

A pass authorizes a fresh, bounded short-horizon information-set search screen using the equal ensemble of the four cross-fit value models. Search must still win paired natural terminal continuations before any policy package or strength gate. A failure closes this width-48 complete-history value bootstrap and moves the project away from local search and residual optimization toward a larger recurrent end-to-end learner.
