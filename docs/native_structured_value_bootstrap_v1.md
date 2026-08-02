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
