# Native structured adapter screen v1

## Question

Can a new action-conditioned object representation extract held-out policy and value signal that Net8 misses, while preserving Net8 exactly at zero residual?

This is a development screen. The corpora have already informed prior experiments, so no fold is a fresh strength test. A pass authorizes a fresh confirmation corpus and an integration review only. It does not authorize promotion or establish play strength.

## Fixed inputs

- Parent policy outputs: the exact old logits and values exported with each row.
- Policy corpus: the 32-pair CP7 teacher export at base seed `970001`.
- Value corpus: the 32-pair generation-384-versus-CP7 outcome export at base seed `1010001`.
- Split: four folds by whole pair, with `pair_index mod 4` selecting the held-out fold.
- Reward: terminal win, loss, or draw only. No shaped target is introduced.
- Execution: deterministic PyTorch CPU prototype. Four folds may run concurrently after one shared cache build.

The loader rechecks that every teacher physical decision has all declared autoregressive substeps and that every outcome episode ends naturally with a terminal return in `-1, 0, 1`. It consumes only the committed typed tensors. XMage diagnostic fields outside that contract are not inputs.

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

The report also breaks policy NLL down by decision kind and evaluates the trained residual after zeroing the 96-feature state digest tail and 96-feature action digest tail. These are diagnostics, not additional acceptance gates.

## Disposition

- Pass: collect a small fresh CP7 teacher and outcome confirmation block, rerun the frozen metrics, and review native integration of the exact architecture. Only a fresh confirmation pass may authorize a small matched live rung.
- Fail: do not tune thresholds or hidden width against these folds. Add explicit public action history to the observation contract and test whether history supplies held-out signal before revisiting model capacity.

No XMage games or GPU training are part of this screen.

## Result

The four deterministic folds completed concurrently in 466.2 seconds. The strict cache contained 4,952 policy rows and 2,629 value rows. Measurement used source commit `66b02207804a445cb707a60888d9b4ee89eab253` and script SHA-256 `d8cf3d4e2e62bd43981be05b8da694256f40d8df92686b40e85a53982f0a0b7f`.

| Held-out metric | Parent | Adapter | Change |
| --- | ---: | ---: | ---: |
| Policy NLL | 2.272464 | 0.523379 | 76.97% better |
| Policy top-1 | 73.95% | 80.17% | +6.22 pp |
| Value MSE | 0.712730 | 0.637915 | 10.50% better |
| P0 value MSE | 0.832062 | 0.603295 | 27.49% better |
| P1 value MSE | 0.593399 | 0.672535 | 13.34% worse |

Policy NLL improved for both acting seats and for surface, attacker-inclusion, and blocker-inclusion decisions. Object permutation changed an output by at most `3.8147e-6`, and removing references affected all 7,581 eligible held-out decisions. With both 96-feature digest tails zeroed only for adapter evaluation, policy NLL still improved by 31.04% and value MSE by 14.07%. The structured path therefore learned semantic signal rather than relying only on the digest.

Six of seven gates passed. The exact package failed because P1 value MSE regressed by 13.34%, beyond the 2% floor. P1 regressed in three of four folds, so this is not one anomalous partition.

A complete fold-0 rerun reproduced counts, training history, held-out metrics, diagnostics, digest ablation, and raw weights exactly. The canonical scientific-field SHA-256 was `032553bd88968d0ae34e656bffaaa14817983d930fac6afdd929c865769fb5cf` for both runs. Runtime was excluded from that comparison.

## Result disposition

Reject the exact combined policy-plus-value adapter. Do not integrate it or spend live XMage games on it. The strong policy-label result is representation evidence, not strength evidence; the earlier behavior-cloning campaign already showed that much better offline CP7 agreement can reduce live win rate.

The next bounded test is the declared public-action-history falsifier. Keep the parent and structured object/action path fixed, add only a compact actor-visible public action sequence, and ask whether history removes the repeated P1 value regression on held-out pairs. Do not sweep width, epochs, thresholds, or digest use against these revealed folds.

Primary result: `D:\mtg-kernel-structured-adapter-screen-v1\aggregate.json`.
