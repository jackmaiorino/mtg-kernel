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

## Result

The 128-pair profile completed in 47.86 seconds, including 24.90 seconds to
hash and load the cache and 21.16 seconds for 101 optimizer steps. The four
full folds then ran concurrently for 11.3 minutes at about 22 effective CPU
cores, used about 8.1 GiB of memory, and produced no stderr. GPU 1 remained
idle as declared.

The diagnostic failed. Overall held-out terminal surrogate was
`-0.0000791183`. P0 was `-0.000265506` and P1 was `+0.000107270`. Folds 0, 1,
and 2 were positive, while fold 3 was `-0.000752408`. Held-out mean total
variation was `0.00750823`, below the required `0.01`; p90 was `0.0267591`,
and maximum absolute physical-decision joint log ratio was `0.339450`.
Representation diagnostics passed.

This result rejects the seat-conditioned-head mechanism. It did not resolve
the shared-head screen's P0 weakness and made the aggregate surrogate
negative. No fresh-corpus confirmation or live strength gate is authorized.
Aggregate SHA-256 was
`6a4562a194ab17a72488c3a4d74412c5794fd8e060f0f103a967dc9bf6556960`.
