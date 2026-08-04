# Native dense-KL recurrent CP7 second fresh gate v1

## Question

Does the fixed full-refit beta-6 recurrent residual generalize to a second fully
disjoint candidate-state panel?

## Fixed evaluation

- Candidate model-file SHA-256:
  `6c33f6d449b76e24c00bc7d46052b04488ddb9ec574009831d2fa90ea01bd55d`.
  Model-state SHA-256:
  `d736296425de2c438bb9be02ab6c89e51da4c17c1408de6ff3309029b2d06dca`.
- Fit recipe: all 18,002 prior labels, beta `6`, eight epochs, width 128, and hard
  legal-action log-probability budget `0.49`.
- Fresh panel: base seed `1,400,001`, pair indices 320 through 383, 64 matched
  seat-swapped pairs, 128 completed games, and 3,679 usable labels.
- Collection report SHA-256:
  `d53e1afcc4a772d5d7628a94f4a58e3b2adbbdb676d63fb5c478c4649842956c`.
  Complete-history cache SHA-256:
  `e542413e4269daa2176143acebe82a71e0d9f46cc3ebbb0bfd2face8b1390c99`.
- No fitting, calibration, checkpoint choice, coefficient choice, or threshold
  change is allowed on this panel.

## Gate

Pass only with at least 5 percent relative CP7 NLL improvement overall, at least
3 percentage points top-1 improvement overall, nonnegative NLL improvement at
both seats, mean TV at most `0.03` and p90 TV at most `0.10` overall and at both
seats, and maximum legal-action log-probability change at most `0.50`.

A pass authorizes native transport and then a fresh natural terminal win/loss
strength gate. It is not itself playing-strength or promotion evidence.

## Result

The fixed evaluation completed in 6.71 seconds and rejected on one seat movement
threshold. NLL improved 5.42 percent overall, 5.26 percent at P0, and 5.56 percent
at P1. Top-1 improved 3.44 percentage points overall. Overall mean TV was
`0.029444`, p90 TV was `0.080725`, and maximum legal-action log-probability change
was `0.490000`.

P0 mean TV passed at `0.028416`. P1 mean TV was `0.030457`, above the `0.030000`
limit by `0.000457`. The result is therefore a formal reject. Report SHA-256 is
`0e89666ef3eb2a1acac30a2a66832793ea57f4a95cd66e8338ace61a0e1a39f5`.

The repeated out-of-sample fit gains show that the representation generalizes;
the remaining issue is movement calibration. For rapid engineering progression,
the next deployment recipe applies a fixed `0.97` interpolation from parent to
the already hard-projected candidate. This panel may verify calibration mechanics
but cannot provide another held-out label claim. Only a new natural terminal
win/loss gate can qualify that deployment recipe.
