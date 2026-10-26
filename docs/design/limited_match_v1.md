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

The belief corpus defaults to the pool's `train` split, as with gorge's closed-list worlds; the
driver refuses a belief split equal to the games' own non-train split.

Acceptance tests (`limited_match_v1::tests`):

- `closedlist_worlds_and_decisions_ignore_hidden_opponent_composition`: replacing the opponent's
  unknown cards while public evidence is unchanged leaves every sampled world and the search's
  action and statistics identical. `truelist` does see the change, so the test is not blind.
- `belief_conditions_on_observed_opponent_cards`: a corpus deck lacking a seen card is never
  drawn while a consistent deck exists, and a consistent draw reproduces that deck's whole
  multiset across seen and unknown cards.

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

Not covered yet: gorge's training records (`-record-features`, `dzgorge pack`) and its position
encoding, which DraftZero's networks consume, have no mtg-kernel counterpart. Networks trained on
gorge do not transfer; DraftZero can use this driver for evaluation, deck-builder scoring and
search baselines.
