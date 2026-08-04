# Native recurrent on-policy terminal correction v1

## Question

Can terminal-only on-policy learning improve the active CP7-initialized recurrent
policy when the earlier recurrent terminal screen trained offline on parent
trajectories and did not replicate?

This changes the data distribution and initialization, not the reward. Natural
terminal win, draw, or loss is the only reward. The current recurrent policy is
the behavior policy and supplies every training trajectory.

## Fixed corpus and model

- Collect 512 fresh seat-swapped Rally pairs against XMage CP7 at base seed
  `2030001`, using eight parallel persistent scorer processes and private copies
  of the pinned card database.
- Export the candidate outcome stream and opponent public-action stream, then
  require an exact complete-history join for all 1,024 natural games.
- Split whole pairs by residue modulo four: residues one and two fit, residue
  three selects the epoch, and residue zero is held out until selection ends.
- Initialize the exact width-128 recurrent model from the CP7 model state
  `d736296425de2c438bb9be02ab6c89e51da4c17c1408de6ff3309029b2d06dca`.
  Preserve its structured encoder, reset only the policy head to zero, freeze
  the unused value head, and treat the output as a correction to the behavior
  logits recorded on the on-policy corpus.
- Use terminal return minus the frozen retained-parent value, standardized by
  candidate seat on the fit split. No intermediate reward or heuristic target
  is allowed.
- Fit four physical-decision joint-ratio PPO epochs, clip `0.10`, AdamW
  learning rate `3e-4`, weight decay `1e-4`, batch 256, gradient cap 5, and
  behavior KL coefficient `0.01`.
- Hard-project every legal substep so every selected physical decision has
  absolute behavior-policy log ratio at most `0.20`.

Epoch zero and epochs one through four are ranked on the selection split by the
lower seat surrogate, then overall surrogate. An epoch that regresses either
seat loses to the zero-correction initializer.

## Throughput and gate

Before the formal fit, compare batch 128 and 256 on a fixed 64-pair subset for
eight measured steps after two warmup steps. Select the largest batch within
five GiB that reaches at least 95 percent of the best rate, and repeat it with
identical loss-trace and state hashes. The formal screen requires batch 256.

Advance to one full-corpus fit and fresh matched terminal gate only if the
selected epoch is nonzero, held-out surrogate is positive overall and
nonnegative at both seats, the pair-bootstrap 80 percent lower bound is above
zero, mean policy TV is in `[0.005, 0.03]`, p90 TV is at most `0.10`, and the
maximum selected physical-decision log ratio is at most `0.20`.

A pass is mechanism evidence only. The subsequent candidate must beat the
behavior policy by a frozen integer terminal-win gate. A failure closes this
exact CP7-encoder, one-batch, on-policy PPO correction without rejecting
terminal-only learning, recurrent policies, or population training.

## Collection and throughput record

The eight-way collection completed six 64-pair shards. Pair 58 and pair 143
each reproduced an outcome-blind mapper failure before terminal adjudication.
They were excluded and replaced with pairs 514 and 515, respectively, which
preserve the same modulo-four folds. The repaired panel therefore contains 512
pairs and 128 pairs per fold. Its exact-history join covers 1,024 games,
113,317 policy steps, and 92,832 physical decisions. The cache SHA-256 is
`b526c2534db44a4372f6231d1c7bc159d6f3f05560b9f943701832b083712039`.

The bounded GPU-1 profile selected batch 256. It measured 3,799.5 physical
decisions per second versus 3,053.8 at batch 128, used 344,152,576 peak bytes,
and repeated with identical loss-trace and model-state hashes. The profile
report SHA-256 is
`89223944b534c2e165d252b50f59532d8e3e92a01c0451052a76dd09ce0ec674`.

## Result

The formal screen completed in `77.37` seconds at commit `3b21fe6`. Selection
chose epoch four, but the held-out terminal surrogate was `-0.00001611`
overall. Candidate-seat values were `+0.00005152` at P0 and `-0.00008374` at
P1, and the pair-bootstrap 80 percent lower bound was `-0.00011383`.

The candidate remained inside the hard behavior envelope. Maximum absolute
physical-decision log ratio was `0.093745`, and p90 total variation was
`0.002655`. Mean total variation was only `0.000984`, below the frozen
`0.005` activity floor. The overall-sign, both-seat, bootstrap, and mean-TV
gates failed.

This rejects the exact CP7-initialized, one-batch on-policy terminal-correction
recipe. No full refit, deployment package, or fresh terminal gate is justified.
It does not reject online terminal learning or recurrent population training.
The formal report SHA-256 is
`8be2a78a7ab12321d3465d16a2eb2b161574de0f21536477da5bf7314edd1dbe`.
