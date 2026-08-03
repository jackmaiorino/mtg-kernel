# Native policy-only structured terminal rung v1

## Question

Can one genuinely on-policy terminal-only PPO update turn the qualified
absolute structured policy into a fresh Pool3 win-rate gain without changing
its architecture, retained value model, opponent population, or reward?

Natural terminal win, draw, or loss is the only reward, training signal, and
promotion measure. Distillation is used only by the already qualified
initializer and is not used in this rung.

## Frozen training rung

- Initializer: exact structured candidate SHA-256
  `204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72`.
- Opponent: exact Pool3 `40/20/20/20` contract SHA-256
  `6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71`.
- Corpus: 2,048 new seat-swapped pairs and 4,096 natural games at base seed
  `1660001`, collected by four parallel persistent native scorers in four
  contiguous 512-pair shards.
- Representation: the unchanged width-48 absolute structured policy with the
  last 16 complete public physical decisions from both actors.
- Value: the exact retained parent value, frozen bit-for-bit.
- Advantage: terminal candidate reward in `{-1,0,1}` minus the frozen parent
  value at the physical decision, standardized separately by candidate seat
  using only the training corpus.
- Objective: physical-decision joint-ratio PPO, clip `0.10`, equal episode
  mass, equal physical-decision mass within each episode, and all
  autoregressive substeps joined into one ratio.
- Fit: five epochs, batch size 64 physical decisions, AdamW learning rate
  `3e-4`, zero weight decay, gradient norm cap 5, seed `20260805`. A bounded
  64-pair, one-epoch profile chooses 12 or 24 CPU threads before the formal
  fit; all other settings remain fixed.

There is no entropy bonus, heuristic reward, intermediate reward, teacher
target, hidden-card target, or learned-value update.

## Publication checks

The trained package is publishable only if all tensors are finite, the value
head remains bit-exact, overall and both-seat mean policy TV from the behavior
policy are at most `0.030`, overall and both-seat p90 TV are at most `0.100`,
and maximum absolute physical-decision joint log ratio is at most `0.50`.
These are numerical safety checks, not strength evidence. Native Rust parity
must have maximum absolute logit error at most `3e-5`, and retained-parent
value transport must be bit-exact.

## Fresh strength gate

Compare the trained package with the qualified initializer on the same 1,024
fresh seat-swapped Pool3 pairs at base seed `1670001`, using two parallel
persistent native scorers. Join all 2,048 games exactly by pair, episode,
environment seed, and candidate seat.

Advance only if trained-policy matched gains are at least matched losses plus
20, trained-policy total wins are at least initializer wins plus 20, and the
trained-minus-initializer win delta is at least `-4` separately at P0 and P1.
Every terminal must be natural and all identity and transport checks must pass.

A pass establishes a Rally-only native Pool3 improvement and authorizes a
separate external-opponent and broader-deck evaluation. A failure retires this
exact one-batch update without tuning on the revealed seed panel. Neither
outcome is a human or pro-level claim.
