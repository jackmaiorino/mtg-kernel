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

## Result

The four folds completed concurrently in 1,251.9 seconds with zero stderr.

| Held-out metric | Parent | History adapter | Change |
| --- | ---: | ---: | ---: |
| Policy NLL | 2.272464 | 0.517349 | 77.23% better |
| Policy top-1 | 73.95% | 80.34% | +6.39 pp |
| Value MSE | 0.712730 | 0.789083 | 10.71% worse |
| P0 value MSE | 0.832062 | 0.789093 | 5.16% better |
| P1 value MSE | 0.593399 | 0.789073 | 32.98% worse |

The stateless structured adapter scored value MSE `0.637915` overall and `0.672535` on P1. Candidate-only action history therefore made both values worse, despite a small additional policy NLL improvement from `0.523379` to `0.517349`.

With the state and action digest tails zeroed for adapter evaluation, policy NLL still improved by 28.89%, but value MSE regressed by 5.57%. Permutation invariance and reference sensitivity passed.

A complete fold-0 repeat reproduced all scientific fields exactly. The canonical SHA-256 was `09f8c417376bf68640414729af4b4938189b9b621748c539de4e187652af1af4` for both runs; runtime was excluded.

## Result disposition

Reject candidate-only action history. It failed both the overall value improvement gate and the P1 value floor. Do not tune history length, GRU width, training duration, or thresholds on these folds.

This does not reject complete public history because the source corpora omit opponent actions. The next architecture step, if pursued, must add both players' actor-visible public actions to the native observation contract and confirm on fresh pairs. A cheaper alternative is to move directly to selective search, which does not require the value model to compress the entire trajectory into one hidden state.

Primary result: `D:\mtg-kernel-candidate-action-history-falsifier-v1\aggregate.json`.
