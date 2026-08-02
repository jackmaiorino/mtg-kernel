# Native structured adapter screen v1

## Question

Can a new action-conditioned object representation extract held-out policy and value signal that Net8 misses, while preserving Net8 exactly at zero residual?

This is a development screen. The corpora have already informed prior experiments, so no fold is a fresh strength test. A pass authorizes a fresh confirmation corpus and an integration review only. It does not authorize promotion or establish play strength.

## Fixed inputs

- Parent policy outputs: the exact old logits and values exported with each row.
- Policy corpus: the 32-pair CP7 teacher export at base seed `970001`.
- Value corpus: the 64-pair candidate-versus-CP7 outcome export at base seed `1070001`.
- Split: four folds by whole pair, with `pair_index mod 4` selecting the held-out fold.
- Reward: terminal win, loss, or draw only. No shaped target is introduced.
- Execution: deterministic PyTorch CPU prototype. Four folds may run concurrently after one shared cache build.

## Candidate representation

The parent logit and value are immutable inputs. The adapter emits residuals whose final layers are initialized to zero, so initialization is exactly the parent.

The adapter consumes the existing typed tensors directly:

- global state, including the existing digest channel
- object features, card identity, and semantic object group
- one relation-message pass over typed edges
- group-wise permutation-invariant object pooling
- typed action features
- action references joined to referenced object representations
- action-conditioned attention over all visible objects

The policy objective is CP7 selected-action cross-entropy with each physical decision receiving equal total mass across its autoregressive substeps. The value objective is terminal-result mean squared error with each episode receiving equal total mass across its decisions.

This differs from the rejected bilinear residual. That experiment could only recombine Net8's frozen 64-wide state and action latents. This adapter can learn new joins from raw typed objects, relations, actions, and references before scoring. It still remains small and parent-preserving.

## Frozen development gates

Aggregate the four held-out folds. Pass only if every condition holds:

1. Policy NLL improves by at least 5 percent relative to the parent.
2. Neither acting-player seat has negative aggregate policy NLL improvement.
3. Substep top-1 accuracy is no worse than 0.5 percentage points below the parent.
4. Episode-balanced value MSE improves by at least 5 percent relative to the parent.
5. Neither candidate-seat value MSE regresses by more than 2 percent.
6. Permuting object rows and consistently remapping edges and action references changes logits and value by at most `1e-5`.
7. Removing valid action references changes at least 20 percent of eligible held-out decisions by more than `1e-4`, demonstrating that the new structured path is used.

The policy and value gates are necessary but not sufficient. Prior behavior cloning improved offline CP7 agreement and reduced live strength, so an offline pass cannot produce a live candidate directly.

## Disposition

- Pass: collect a small fresh CP7 teacher and outcome confirmation block, rerun the frozen metrics, and review native integration of the exact architecture. Only a fresh confirmation pass may authorize a small matched live rung.
- Fail: do not tune thresholds or hidden width against these folds. Add explicit public action history to the observation contract and test whether history supplies held-out signal before revisiting model capacity.

No XMage games or GPU training are part of this screen.
