# Native complete public-history falsifier v1 result

## Status

REJECT. A compact GRU over both players' last 16 completed public physical
decisions did not repair held-out value prediction. It made value prediction
worse than both the parent and the fresh stateless adapter.

## Data and execution

The teacher and outcome exports came from one generation-384-versus-CP7 bridge
trajectory at base seed `1150001`: 32 seat-swapped pairs, 64 natural games,
5,905 policy steps, and 4,871 physical decisions. The strict join covered every
step and physical decision, matched terminal hashes exactly, and accepted all
selected semantics as public after completion.

The four stateless folds, four complete-history folds, and one complete-history
fold-0 repeat ran concurrently. Peak CPU utilization was 99.5 to 100 percent.
The repeat matched every scientific JSON field exactly after excluding only
runtime.

## Held-out result

| Metric | Stateless | Complete history |
| --- | ---: | ---: |
| Policy NLL relative improvement | 71.366% | 73.181% |
| Policy top-1 delta | +6.893 pp | +7.056 pp |
| Value MSE relative improvement | -0.981% | -8.500% |
| P0 value MSE relative improvement | -7.006% | -13.216% |
| P1 value MSE relative improvement | +4.749% | -4.016% |
| Gates passed | 5/7 | 5/7 |

Complete-history value MSE was 7.446 percent higher than stateless value MSE.
All four history folds regressed against the parent: 11.105, 6.812, 0.280, and
15.408 percent respectively. The result is therefore not a single-fold or
single-seat anomaly.

Both arms passed the policy, top-1, permutation, and reference-sensitivity
gates. Both failed the overall value-improvement gate and the per-seat value
floor. Complete history increased policy fit while degrading value
generalization.

## Disposition

Close recurrent public-history compression at this width, history length, and
corpus scale. Do not tune this held-out screen. Move to selective belief-aware
search at high-impact decisions, first as a policy-preserving inference-time
comparison against generation 384.

This was an offline fixed-corpus falsifier. It produced no live candidate, no
promotion evidence, and no professional-level play claim.

Primary artifacts are under `D:\mtg-kernel-public-history-screen-v1`; the
complete manifest is `manifest.json` in that directory.
