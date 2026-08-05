# Current Net8 GAE 16-update development v1

Status: frozen before development outcomes.

## Question

Does extending the exact terminal-only history-value GAE candidate from eight
to sixteen updates create enough additional fresh paired win-rate margin to
justify candidate slot 02 under V3?

Candidate 01 ended `INCONCLUSIVE-AT-MAX-N` with
`Delta_hat=0.010528564453125` and confidence sequence
`[0.00475336130478099, 0.018388660485134878]`. It remains closed and is not
the parent. This screen asks whether one fixed longer horizon strengthens that
mechanism. It does not retune GAE lambda, learning rate, optimizer, reward,
architecture, or opponent mixture.

## Fixed candidate and data

- Parent: exact update-512 native state SHA-256
  `00333d987584d5cf7f9a37f1ba2b558cfd22a60388f2487c1bf1623fcc6686a0`.
- Critic: exact qualified complete-history model parameter SHA-256
  `6329233bcc22f7941e8085ef0235107eb75293fe74c727434c0474da15354f22`.
- Reward: zero at every nonterminal step and natural terminal win, draw, or
  loss only.
- Credit estimator: `gamma=1`, `lambda=0.95`, with the critic only
  propagating the terminal result.
- Training: sixteen 64-episode updates at base seed `970001`, episodes
  `32768..33791`, Pool3, environment-randomization-v2, the existing optimizer
  and learning rate, value coefficient `0.5`, and zero entropy bonus.
- The first eight updates deliberately reproduce candidate 01. Their native
  state must equal
  `ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952`.
- Fresh development evaluation: parent, reproduced 8-update state, and fixed
  16-update state on common episodes `98304..99327`, base seed `970001`.
  These roots are disjoint from training, candidate-01 development, and both
  candidate-01 formal schedules.
- Every arm retains the episode index, physical seat, pair environment seed,
  deck hashes, and Pool3 component. These receipts, the exact 512/512 seat
  counts, and the canonical schedule SHA-256 must match before pairing.

The established 4-worker by 16-session topology is retained. It was the
winner of the exact 1/2/4-worker screen for this native rollout path. This
bounded screen is expected to finish in a few minutes. Any later candidate-02
formal run requires its own candidate-bound throughput replay before launch.
Before this development launch, one manifest binds the git commit, Rust/Cargo/
LLVM and linker versions, release executable, GPU identity, input hashes,
critic package, exact training and evaluation ranges, topology, and canonical
evaluation schedule hash. The harness requires the manifest SHA-256 and
retains that binding in the result.

## Development gate

Advance to a separately frozen candidate-02 V3 design only if all conditions
hold:

1. The reproduced 8-update state has the exact native-state identity above.
2. Every update is finite, sampled policy entropy is at least `0.10`, gradient
   L2 norm is at most `5.0`, and final 16-update movement from the parent is at
   most `1.25`.
3. On the 1,024 fresh common episodes, the 16-update state has V3-compatible
   paired terminal-order better outcomes minus parent-better outcomes of at
   least `16`, where win is better than draw and draw is better than loss.
4. The 16-update state has terminal-order better outcomes minus reproduced
   8-update-better outcomes of at least `8`.
5. The 16-update versus parent terminal-order net is nonnegative separately
   for candidate P0 and candidate P1.

The `16/1024` and `8/1024` thresholds are heuristic selection margins. The
first is above the later `1%` formal effect threshold; the second only demands
incremental evidence over the reproduced state. Neither is a confidence claim
or a promotion rule. Win-only summaries remain diagnostics, while every gate
condition uses the ordered natural terminal result.
Failure closes this fixed sixteen-update extension without changing any
threshold on the revealed panel. The next branch is a genuinely distinct
population or response-oracle mechanism. A pass only authorizes fresh,
disjoint V3 sizing and design review for candidate slot 02.
