# Current Net8 population response cycle v1

Status: frozen before development outcomes; Fable countersigned with launch
amendments incorporated; implementation preflight pending.

## Question

Can one terminal-only response cycle, initialized from the formally positive
eight-update GAE lineage, improve against both that current policy and the
original Pool3 distribution without seat regression?

This is the distinct population-pressure branch required after the fixed
sixteen-update extension stopped. It is not a new return estimator, a longer
run against the old Pool3, or an accumulation-chain reclassification.

## Fixed response cycle

- Initial learner: exact native state SHA-256
  `ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952`,
  loaded directly from the retained GAE8 payload with file SHA-256
  `a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98`.
  The cycle does not spend compute retraining this initializer.
- Historical anchor: update-512 native state SHA-256
  `00333d987584d5cf7f9a37f1ba2b558cfd22a60388f2487c1bf1623fcc6686a0`.
- Retained promoted Pool3 primary: generation 384 state SHA-256
  `a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99`.
- Response population: the existing deterministic 40/20/20/20 schedule,
  remapped explicitly to current GAE8, historical update-512, retained Pool3
  primary, and the unchanged uniform floor. The report must retain the mapping
  and realized member for every episode. No production Pool3 constructor or
  identity is changed.
- Reward: zero at every nonterminal step and natural terminal win, draw, or
  loss only. The complete-history critic only propagates that terminal result.
- Update rule: the existing history-value GAE rule with `gamma=1`,
  `lambda=0.95`, value coefficient `0.5`, learning rate `0.001`, zero entropy
  bonus, and unchanged Adam state.
- Training: eight updates of 64 episodes, base seed `980001`, episodes
  `33280..33791`, starting at Adam step 520. No hyperparameter or checkpoint
  selection occurs within the cycle.

The response population contains no independently constructed exploiter:
GAE8, the update-512 anchor, and the retained Pool3 primary share the learner
class and training lineage. A pass therefore supports only a
population-pressure improvement claim. Exploiter robustness remains untested
and is deferred to a later cycle.

Before development, run one update at each 1/2/4-worker topology on base seed
`980000` using 32/32/16 sessions per worker. Select only the fastest topology
whose repeated result is bit-identical apart from timing and whose numerical
envelope matches. Record achieved games per second, utilization, and projected
wall time. These topology roots are excluded from training and evaluation.

## Fresh development evaluation

All policies are fixed during evaluation and post-rollout updates are
discarded. Candidate, GAE8, and the historical anchor use common episode
receipts within each panel.

1. Original Pool3: base seed `980001`, episodes `65536..66559`, 512
   seat-swapped clusters and 1,024 episodes per arm.
2. Pure current-policy opponent: base seed `980001`, episodes `66560..67583`,
   512 seat-swapped clusters and 1,024 episodes per arm. Every opponent policy
   slot contains an independently constructed inference handle for exact GAE8.

Every arm retains episode index, learner seat, pair environment seed, deck
hashes, and realized opponent identity. Pairing fails if any receipt differs.
The two panel schedules, training roots, topology roots, candidate-01 panels,
and sixteen-update panel are mutually disjoint by full base-seed and episode
identity.

## Development gate

Advance to a separately frozen candidate-02 V3 design only if all conditions
hold:

1. The initial state identity is exact; every update is finite; sampled policy
   entropy in natural-log nats is at least `0.10`; gradient L2 is at most
   `5.0`; and full parameter-space L2 movement from GAE8 is at most `0.75`.
2. On original Pool3, response versus GAE8 terminal-order better minus worse
   outcomes is at least `8/1024`.
3. On original Pool3, response versus the historical anchor has net at least
   `16/1024` and is nonnegative separately at P0 and P1.
4. Against the pure GAE8 opponent, response versus GAE8 control has net at
   least `8/1024` and is nonnegative separately at P0 and P1.

Terminal order is win greater than draw greater than loss. Win-only summaries
are diagnostics. The margins are intentionally noisy development selectors.
Using candidate-01's roughly 7.3 percent terminal-order discordance as a scale,
each 1,024-episode comparison has about 75 discordant outcomes and null net SD
about `sqrt(75) = 8.7`. A `+8` margin is therefore about 0.9 null SD with an
approximate one-sided null exceedance probability of 18 percent; `+16` is about
1.8 null SD with probability about 3 percent. The shared original-Pool arms
make an exact joint false-advance probability inappropriate, but it cannot
exceed the rarest marginal's roughly 3 percent and would be around 0.1 percent
if the three margins were independent. The both-seat nonnegativity checks add
substantial false-stop pressure: at an exact seatwise null, two independent
seat nets are both nonnegative only about one quarter of the time for one
comparison, before allowing for discrete ties and conditioning on the aggregate
margin. Thus neither a development pass nor a one-seat failure is formal
strength evidence.

These fixed margins are development selection heuristics, not confidence or
promotion claims. Failure closes this exact population mapping, eight-update
response horizon, and seed domain without threshold changes or panel reuse. A
pass authorizes a fresh V3 candidate sheet, assigned-alpha sizing, per-sheet
countersign, implementation-readiness review, and a new 1/2/4-worker formal
throughput replay. It does not promote a policy or support a pro-level claim.
