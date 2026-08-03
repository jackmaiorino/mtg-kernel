# Native on-policy structured population v1

## Question

Can the complete-public-history structured residual learn a seat-stable policy
direction from its own terminal outcomes when the opponent is the explicit
Pool3 population rather than fixed XMage CP7?

This is a rapid mechanism screen. Natural terminal win, draw, or loss is the
only reward and the only success measure. Offline metrics are not strength
evidence.

## Fixed screen

- Candidate behavior policy: exact retained outcome parent manifest SHA-256
  `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`.
- Opponent: exact Pool3 contract SHA-256
  `6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71`,
  with frozen primary, predecessor-A, predecessor-B, and uniform weights
  `40/20/20/20`.
- Corpus: base seed `1501001`, pair indices `0..511`, 512 seat-swapped pairs,
  1,024 natural games, and both actors' complete selected-action streams.
- Split: four whole-pair folds by `pair_index mod 4`.
- Representation: width 48, structured state, objects, relations, actions,
  references, and the last 16 completed public physical decisions.
- Objective: physical-decision joint-ratio PPO from candidate actions only,
  with terminal return minus frozen parent value as the advantage. There are
  no heuristic or intermediate rewards.
- Fit: five epochs, batch size 64, clip 0.10, AdamW learning rate `3e-4`,
  weight decay `1e-4`, gradient cap 5, seed `20260802`.

## Mechanism gate

Advance only if aggregate and both-seat held-out terminal surrogates are
positive, at least three folds are positive, held-out mean total variation is
between 0.01 and 0.05, p90 total variation is at most 0.15, maximum absolute
physical-decision joint log ratio is at most 0.75, object permutation delta is
at most `1e-5`, and at least 20 percent of sampled reference-bearing decisions
respond to reference removal.

A pass authorizes one full-corpus fit and a fresh matched 64-pair native gate
against the same Pool3 population. A fail retires this exact one-batch package
without a live strength test. It does not reject on-policy terminal learning,
iterative policy improvement, other population curricula, or other model
architectures.

## Throughput preflight

Two pairs repeated from base seed `1500001` produced byte-identical teacher and
outcome streams. The teacher SHA-256 was
`fc2ab7ba32e217cdff0368c2143978f2b96eb150f4190ddeeb62a51cb4926131` and
the outcome SHA-256 was
`16919bda4083a9005fa7bb310886bde866edccb9afd2a44b701cf80cb8194b3b`.
The complete-history join covered all 449 policy steps and 382 physical
decisions. The repeated run spent 62.77 seconds loading and validating Pool3,
then completed the four games in 0.63 seconds. One persistent CPU process is
therefore the practical topology knee. GPU 1 remains unused because all
inference and training in this screen are CPU implementations.

## Result

The corpus completed in 171.39 seconds. It contained all 512 pairs, 1,024
natural terminals, 83,949 policy steps, and 70,748 complete physical
decisions. The candidate behavior policy won 631 games and lost 393 against
the frozen Pool3 mixture. Cache SHA-256 was
`82ba196f7ad719c0a51fb3235f2bf7039625a575cc08aad19f55ab84e28f29a9`.

The four folds ran concurrently in 11.5 minutes. Overall held-out terminal
surrogate was `+0.0000252972`, and three of four folds were positive. Candidate
P1 was positive at `+0.000128709`, but P0 was negative at `-0.0000781148`.
Held-out mean total variation was `0.00842156`, below the required `0.01`;
p90 total variation was `0.0322417`, and maximum absolute physical-decision
joint log ratio was `0.423733`. Representation diagnostics passed.

This exact one-batch package is rejected because the both-seat surrogate and
minimum-movement gates failed. No live strength gate is authorized. This does
not reject seat-conditioned models, iterative policy improvement, other
population curricula, or other model architectures. Aggregate SHA-256 was
`4914aa8fd724e257b30425361122e4d59a6da57310c14a24fce7d99412700644`.
