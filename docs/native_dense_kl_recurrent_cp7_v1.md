# Native dense-KL recurrent CP7 screen v1

## Question

Can full CP7 imitation retain the dense recurrent learner's useful fit while a
properly scaled parent-policy KL keeps movement inside the deployment envelope?
The prior disagreement-only objective preserved movement but lost the agreement
examples needed to learn CP7's policy representation.

## Fixed screen

- Reuse the exact candidate-state corpus, width-128 recurrent residual, zero
  policy-head initialization, equal label-bearing-episode weights, and whole-pair
  train residues 1 and 2 from the prior screens.
- Evaluate only residue 3 for model and coefficient selection. The revealed
  residue-0 split remains unused.
- Optimize CP7 selected-index cross entropy on every labeled row plus
  `beta * KL(parent || candidate)` on every row.
- Screen beta values `3`, `10`, `30`, `100`, and `300` for eight epochs each.
  Use AdamW `3e-4`, weight decay `1e-4`, gradient cap `5`, batch 256, seed
  `20260810`, and exclusive GPU 1.
- Restore the hard `0.49` legal-action log-probability budget. The KL term, not a
  smaller uniform cap, must make movement selective. This keeps enough local
  range to flip the low-margin mistakes identified by the oracle audit.

## Gate

The residue-3 gate is unchanged: at least 5 percent CP7 NLL improvement, at least
3 percentage points top-1 improvement, nonnegative NLL improvement at both seats,
mean TV at most `0.03`, p90 TV at most `0.10`, and maximum legal-action log-
probability change at most `0.50`. Among passing checkpoints, select the lowest
mean-TV result.

A pass authorizes one fresh disjoint candidate-state CP7 label panel and fixed
out-of-sample label gate. It is not playing-strength evidence. A later natural
terminal win/loss gate remains mandatory for any strength or promotion claim.

## Result

The five-arm screen completed in 102.71 seconds and rejected the coarse grid
without touching residue 0. Beta `3`, epoch 6, passed the fit requirements with
6.48 percent NLL improvement and 4.54 percentage points top-1 improvement, but
failed movement at mean TV `0.050430` and p90 TV `0.159696`. Beta `10`, epoch 8,
was movement-safe at mean TV `0.017960` and p90 TV `0.051124`, but narrowly
missed fit at 4.53 percent NLL improvement and 2.47 percentage points top-1.
Betas `30`, `100`, and `300` moved still less and fit worse.

The coarse grid report SHA-256 is
`7abf009588b68968553ccfa3bf1ec141fea3befb31c222bfd2f92c2cbc940c96`.
This closes the exact coarse grid, but the monotone tradeoff brackets both gates.
A rapid interpolation at betas `5`, `6`, `7`, and `8` on the same permitted
selection split is justified before any fresh data collection.
