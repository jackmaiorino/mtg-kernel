# Native seat-conditioned population diagnostic v1

## Question

Did the shared residual policy head cause the opposite P0 and P1 held-out
gradients in the rejected on-policy Pool3 screen?

## Fixed inputs and method

This diagnostic reuses the revealed 512-pair cache with SHA-256
`82ba196f7ad719c0a51fb3235f2bf7039625a575cc08aad19f55ab84e28f29a9`.
It keeps the width-48 encoder, last 16 complete public decisions, terminal-only
win/loss return, parent value baseline, pair-mod-4 folds, five epochs, batch
size 64, PPO clip 0.10, AdamW learning rate `3e-4`, weight decay `1e-4`, and
gradient cap 5 unchanged.

The only model change is replacing the single zero-initialized policy residual
head with two zero-initialized heads selected by candidate seat. The structured
encoder and value path remain shared.

Four folds run concurrently with six CPU threads each. The prior matched
throughput profile predicts about 12 minutes wall time. GPU 1 remains reserved
but unused because this path uses CPU PyTorch.

## Development criterion

The mechanism is supported only if aggregate and both-seat held-out terminal
surrogates are positive, at least three of four folds are positive, held-out
mean total variation is between 0.01 and 0.05, p90 total variation is at most
0.15, maximum absolute physical-decision joint log ratio is at most 0.75, and
the representation diagnostics pass.

A supported result authorizes confirmation on a fresh independent 512-pair
corpus with the same frozen analysis gates. It does not authorize a live
strength test, promotion, or a pro-level claim. A failed result retires this
seat-head variant and sends the project to a deeper iterative architecture.
