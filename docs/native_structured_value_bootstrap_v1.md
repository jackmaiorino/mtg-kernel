# Native structured value bootstrap v1

## Question

Does the 256-pair outcome corpus support a seat-stable structured value model
that is accurate enough to justify learned-value bootstrap search?

The prior 32-pair structured screen improved held-out value MSE by 10.50
percent overall but regressed candidate P1 by 13.34 percent. The completed
terminal PPO screen supplied eight times as many complete pairs and did not
evaluate this value-only objective.

## Fixed development screen

- Data: exact outcome corpus SHA-256
  `317148bc19c6b33214181ed807d672b1a6f135cb6cbee1b5f9139667382fa9b0`,
  pairs `1..256`, 512 natural terminals.
- Split: four folds by whole pair, with `pair_index mod 4` held out.
- Example: one value target per physical decision, using the first substep's
  observation and frozen parent value.
- Target: terminal win, draw, or loss only. Each episode has equal total mass.
- Representation: the same 48-wide structured object, relation, action,
  reference, and attention path. The parent policy is unchanged.
- Initialization: zero value residual, exactly preserving the parent value.
- Fit: 20 epochs, batch size 32 physical decisions, AdamW learning rate
  `3e-4`, weight decay `1e-4`, gradient norm cap 5, and seed `20260802`.
- Execution: deterministic PyTorch CPU. Four folds may run concurrently.

## Gates

Advance only if all conditions hold:

1. Aggregate episode-balanced value MSE improves by at least 5 percent.
2. Neither candidate seat regresses by more than 2 percent.
3. At least three of four folds have positive overall MSE improvement.
4. Mean absolute value residual is at most `0.25`, weighted p90 absolute
   residual is at most `0.50`, and maximum absolute prediction is at most
   `1.50`.
5. Object permutation changes value by at most `1e-5`.
6. Removing valid action references changes value by more than `1e-4` for at
   least 20 percent of 1,024 sampled eligible held-out decisions.

## Disposition

- Pass: fit once on all 256 pairs and test it only as a frozen bootstrap in a
  fresh short-horizon selective-search mechanism screen.
- Fail: close this exact structured value bootstrap. Do not tune width,
  epochs, or seat mixing on the revealed folds.

This is reused development data. It produces no live policy, strength result,
promotion evidence, or pro-level claim.

## Completed result

All four folds completed concurrently in 521 through 528 seconds while total
CPU utilization remained approximately 100 percent.

| Fold | Overall MSE change | Candidate P0 | Candidate P1 |
| ---: | ---: | ---: | ---: |
| 0 | 11.97% worse | 10.75% worse | 13.13% worse |
| 1 | 6.39% worse | 4.36% worse | 7.93% worse |
| 2 | 5.24% better | 12.18% better | 3.58% worse |
| 3 | 3.61% worse | 3.40% worse | 3.88% worse |

Aggregate parent MSE was `0.647926`; candidate MSE was `0.675949`, a
`4.33%` regression. P0 regressed `1.19%` and P1 regressed `7.52%`. Only one
of four folds improved. Mean absolute value residual was `0.345178`, weighted
p90 was `0.712791`, and maximum absolute prediction was `2.25466`, so all
three movement gates failed as well.

The representation checks passed. Maximum object-permutation value delta was
`2.38e-7`, and 1,021 of 1,024 sampled reference-bearing held-out decisions
changed by more than `1e-4` when references were removed.

The aggregate-MSE, seat, fold-count, and all residual-size gates failed. The
fixed structured value bootstrap is rejected. Do not fit a full-data value
package or implement its short-horizon search screen.

This closes only the fixed 48-wide value residual trained on 256 pairs. The
consistent fit activity plus held-out regression is evidence of overfitting,
not a lack of structured feature use. The next useful test should increase
data scale substantially rather than tune this model against the revealed
folds.

Aggregate report:
`D:\mtg-kernel-structured-value-bootstrap-v1\development-aggregate.json`,
SHA-256
`bf6d6415259df8f5d16fabc0a48442928ab5149ff2fd30cc93df6bd2ef283de1`.
