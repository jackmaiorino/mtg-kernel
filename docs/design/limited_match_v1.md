# Limited match driver and fair search (v1)

Issue #110 milestones 6 (fair Limited search) and 7 (DraftZero integration). Source:
`mtg-kernel/src/limited_match_v1.rs`, `mtg-kernel/src/bin/kernel_limited_match_v1.rs`,
`python/tools/limited_match_v1.py`.

## Games

Every game runs on the Prepared V57 session (`FastActorSessionV1::reset_prepared_v57`), the
newest general Limited session on main. It accepts any mainboard of 40 or more Full cards, with
Foundations combat, London mulligans and engine priority windows. Coverage therefore follows the
card registry: a deck plays once every card in it is Full.

## Policies

| Spec | What it does |
| --- | --- |
| `random` | Uniform over the legal actions |
| `pimc:sims=N:depth=D:closedlist` | Fair root-level determinized search (below) |
| `pimc:...:truelist` | The same search over the opponent's real unseen cards, shuffled. Leaks the decklist; for comparison only |
| `pimc:...:peek` | The same search with no redetermination. Sees every hidden card; for comparison only |

Each simulation picks a root action by UCB1 (exploration 1.4 on values in [-1, 1]), draws one
world, applies the action, plays uniform random decisions for at most `D` steps and scores the
result: +1/-1/0 for a natural result, otherwise half the existing kernel-native evaluator's score
scaled to [-0.5, 0.5]. The most visited action is played. All seeds derive from the game seed, so
games replay exactly at any worker count.

## Fair worlds (milestone 6)

A `closedlist` world is built from public facts only:

1. The actor's own library order is reshuffled (it knows its decklist, not the order). Cards the
   actor's own pending selection shows it stay in place.
2. The opponent's unknown hand and library slots are filled from the belief corpus: one deck of
   the right size that contains every opponent card the actor can see (battlefield, graveyard,
   exile, stack, revealed cards), chosen uniformly. With no consistent deck, a deck sharing the
   most seen cards supplies the rest. The true hidden cards are never read.
3. The existing hidden-source provenance check rejects worlds where a frozen source points at an
   unknown slot. Every unknown slot counts as changed, so a rejection depends only on public
   structure, never on which hidden card sat in a slot. A rejected simulation is skipped, not
   retried.

The belief corpus defaults to the pool's `train` split, as with gorge's closed-list worlds. The
corpus may then hold the decks being played (the summary's `belief_overlap` counts them by name);
the property above still holds, but such a belief is weaker evidence of fair play than a disjoint
one. Outside the train split the driver refuses any overlap, including `-belief-split all`.

Face-down cards outside hand and library count toward the opponent's deck size but are not seen
cards. Search simulations whose root step or rollout the engine refuses or halts score 0 and are
counted in each row's `rollout_errors` (with `first_rollout_error`); the fixture tests require
zero.

Acceptance tests (`limited_match_v1::tests`):

- `closedlist_worlds_and_decisions_ignore_hidden_opponent_composition`: replacing the opponent's
  unknown cards while public evidence is unchanged leaves every sampled world (whole state) and the
  search's action and statistics identical. `truelist` does see the change, so the test is not blind.
- `belief_conditions_on_observed_opponent_cards`: a corpus deck lacking a seen card is never
  drawn while a consistent deck exists, and a consistent draw reproduces that deck's whole
  multiset across seen and unknown cards.
- `hidden_source_rejection_does_not_depend_on_hidden_identity`: with a frozen trigger source in a
  hidden library slot, every seed is rejected in both world modes, while the older census clone
  admits or rejects depending on the hidden redraw.

Known approximation: seen cards are those the actor can see now. A card seen earlier and then
shuffled back into a hidden zone is not remembered, so the belief uses less than the full public
history, never more.

## DraftZero driver (milestone 7)

`kernel_limited_match_v1` mirrors `dzgorge`:

```text
kernel_limited_match_v1 version
kernel_limited_match_v1 coverage -decks DIR -pool FILE [-out supported.txt]
kernel_limited_match_v1 play -decks DIR -pool FILE -split S -seed N -pairs K -a SPEC -b SPEC -out games.jsonl
    [-first-pair P] [-workers W] [-single] [-max-turns 100] [-max-decisions 20000]
    [-belief-decks DIR] [-belief-pool FILE] [-belief-split train]
```

Inputs are DraftZero's deck directory (`<deck>.json`, `{name, cards: [{name, count}]}`) and
`pool.tsv` (header `deck split ...`). Pair `p` deals two distinct playable decks of the split
from `(seed, p)`, a shared environment seed and a starting seat. Both legs use that deal, with
policy A in seat 0 on leg 0 and the policies swapped on leg 1. Each row of `games.jsonl` starts
with `{"game":` and carries `pair, leg, seed, decks, side, pols, winner, result, turns, ms`, plus
`starting, decisions, rejected_worlds` and `error` on an engine error. `result` names the winning
policy (`A`, `B`), or `draw`, `stall` (turn or decision cap) or `error`. The summary
(`games.jsonl.summary.json`) has the counts, `wall_s` and `games_per_hour`. `coverage` prints
gorge's two line formats, which `draftzero.engine.Gorge.coverage()` parses.

`python/tools/limited_match_v1.py` holds `MtgKernel`, shaped like `draftzero.engine.Gorge`
(`version()`, `coverage()`, `play_argv()`, `play()` returning a `PlayResult`). Its `smoke`
command runs coverage, a random match and a fair-search match on the two original fixture decks
and replays each with another worker count; CI runs it in the Limited V5 group.

Its `draftzero` command plays DraftZero's 80 sample decks (`data/limited/draftzero_sample_v1`):
coverage per session, then random rules-only games on both splits, one process per game. A game
that passes `--game-timeout` (default 60 s) is killed and reported as a `hang`, with its deal read
back by replaying the pair with a zero decision cap. Halted and hung games count against every
non-Full card in their decks; `full_only_baseline` counts the halts in games with only Full cards,
which no admission can cause. Setting `MTG_LIMITED_MATCH_TRACE` makes every step print its chosen
action to stderr first, so the last line of a hung game names the step that never returned.

## Throughput (first measurement)

CI smoke on a hosted 4-vCPU Ubuntu runner, two workers, the two fixture decks, release build
(CI run 40288862019):

| Match | Games | Decisions | Wall s | Decisions/s | Games/hour |
| --- | ---: | ---: | ---: | ---: | ---: |
| random vs random | 4 | 3,682 | 32.9 | 112 | 438 |
| `pimc:sims=4:depth=8:closedlist` vs random | 2 | 1,675 | 100.0 | 16.7 | 72 |

The cost is the Prepared V57 session, not the policies. Before every step it re-runs
`prepared_observation_v57::validate_state` and rebuilds the whole action menu to compare it
with the bound one. The existing V5 process check shows about 19 ms of apply time per step even
for 40 Forest against 40 Island. Both checks are read-only, so a trusted internal step path for
self-play and search rollouts could skip them without changing play. That is filed as separate
engineering because it reshapes the shared step path.

One natural fair-search game (seed 11, pair 0, leg 0, turn 16) halts with the fail-closed
terminal `fail_closed:ravenous_v51_required:Phoenix public roster incomplete`, and about 0.1% of
that match's search simulations end the same way. Games replay exactly, so it reproduces. A
refused step's error row names the action being applied; a halted game's row carries the
engine's reason with its turn and decision count.

Not covered yet: gorge's training records (`-record-features`, `dzgorge pack`) and its position
encoding, which DraftZero's networks consume, have no mtg-kernel counterpart. Networks trained on
gorge do not transfer; DraftZero can use this driver for evaluation, deck-builder scoring and
search baselines.
