# Native dense-KL recurrent CP7 fresh gate v1

## Question

Does the beta-6 recurrent CP7 residual selected on pair residues 1 through 3
generalize to a fully disjoint candidate-state panel without tuning?

## Fixed evaluation

- Candidate: exact interpolation model file SHA-256
  `93732c91aee17782441ee7c8276ae4337a093ca643912e8c734df10de511265a`,
  model-state SHA-256
  `0c2f0b83235cde8af05ca98c8ed58c06157ce3de5ff9305145b70f54efedc903`,
  beta `6`, epoch 8, hard legal-action log-probability budget `0.49`.
- Fresh panel: base seed `1,400,001`, pair indices 256 through 319, 64 matched
  seat-swapped pairs, collected with eight workers from the unchanged parent.
- Collection result: 128 completed natural games, 3,477 usable labels, one game
  with no candidate priority label, and 47.34 percent parent-teacher disagreement.
- Collection report SHA-256:
  `38b0102fe285557be16107894e83e657f3edb34ae5d289cbad79a3d1e5f79303`.
  Exact complete-history cache SHA-256:
  `05b815ee237043865e23457ba69ec791a5c07aeac6d09778fed90074e8c16278`.
- No fitting, checkpoint choice, coefficient choice, calibration, or threshold
  change is allowed on the fresh panel.

## Gate

Pass only with at least 5 percent relative CP7 NLL improvement overall, at least
3 percentage points top-1 improvement overall, nonnegative NLL improvement at
both seats, mean TV at most `0.03` and p90 TV at most `0.10` overall and at both
seats, and maximum legal-action log-probability change at most `0.50`.

A pass is evidence that this representation and objective generalize to new
candidate-visited states. It is not playing-strength evidence. Native transport
and a fresh natural terminal win/loss gate remain mandatory before promotion.

## Result

Pending.
