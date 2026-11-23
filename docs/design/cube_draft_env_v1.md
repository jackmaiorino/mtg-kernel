# Cube draft environment v1

`mtg-kernel/src/cube_draft_v1.rs` (feature `eternal-fixtures`). It runs a seeded eight-seat
booster draft of the MTGO Vintage Cube, builds a 40-card deck for each seat, and plays a
round-robin pod of games through the custom-deck Limited session (`LimitedJsonlServerV1`,
schema 5, public shroud).

## Pool

The draftable pool is `data/cube_v1/mtgo_vintage_cube_20261007.json` minus every name in
`data/cube_v1/mtgo_vintage_cube_20261007_gaps.json`, which leaves about 483 cards in
cube-list order. `CubePool::from_json_texts` takes both file texts. A name outside the gap
record that the registry does not know is an error.

## Seeds

Everything follows from one `u64` draft seed. Each part takes its own sub-seed, chained from
`SplitMix64` outputs by `derive_seed(parent, stream, index)`:

| Part | Sub-seed |
| --- | --- |
| Packs | `packs_seed(draft)` |
| Seat pickers | `picker_seed(draft, seat)` |
| Deck builders | `deck_seed(draft, seat)` (the stated rule draws no randomness) |
| Pod | `pod_seed(draft)` |
| Each game | `game_seed(pod, game)`, then that game's seat-order coin, session `env_seed` and uniform action stream |

No result depends on hash-map order, time or thread scheduling.

## Draft

- **Packs:** 360 distinct pool cards are drawn without replacement by a partial
  Fisher-Yates shuffle. The packs are 3 rounds × 8 seats × 15 cards.
- **Picks:** all eight seats pick at the same time at each of the 15 pick steps. Then
  every pack passes: round 1 to the left (seat s to s+1 mod 8), round 2 to the right,
  round 3 to the left.
- **What a picker sees:** `DraftPicker::pick` gets a `PickView` with its seat, the round,
  the pick number, the pack in front of it and its own picks so far.
- **Errors:** an index outside the pack makes the draft return
  `CubeDraftError::PickOutOfRange`.
- **Record:** `DraftRecord` (serde JSON) holds the packs as opened, every pick with the pack
  it was taken from, and the final pools.

## Deck rule (`StatedRuleDeckBuilder`)

1. **Pair:** the two colours with the most coloured cards in the pool. A card counts once
   for each colour it has. Ties go in W, U, B, R, G order.
2. **Spells:** nonland cards inside the pair (colourless included), in pick order, up to 23.
   If that gives fewer than 22, off-colour nonland cards are added in pick order until
   there are 22.
3. **Nonbasic lands:** up to 3 from the pool, in pick order. Each must make a pair colour
   or no coloured mana at all.
4. **Basics:** basics of the pair fill the deck to 40. They are split by the pair's coloured
   symbols in the chosen spells' costs, using largest remainder with ties to the first
   colour.

`Deck::validate` checks that the deck has exactly 40 cards, that every card is from the
pool or a basic land, and that no non-basic card appears more than once.
`Deck::to_session_json` gives the reset's `{"cards": [{"name", "count"}]}` shape.

## Pod

`play_pod_round_robin(decks, seed)` plays all 28 pairings once:

- The seat order comes from a seeded coin.
- Both players choose uniformly at random among the legal actions.
- The driver caps a game at 8000 decisions; the session caps are 65,536 physical
  decisions and 131,072 policy steps.

Each `GameRecord` holds the seats, the decision count and the end. The end is one of:

- a natural end, with the winning seat;
- any other terminal reply (for example a session cap);
- a session error;
- the driver cap;
- an engine panic.

A game that ends any of these ways is recorded; none of them stops the pod.

## Fixture status

The only training signal will ever be game win or loss. No hand-written or heuristic player
may ever be an opponent or teacher in training. `UniformPicker`, `ColourCountPicker` and
`StatedRuleDeckBuilder` are test fixtures and reference baselines only. Engineering checks
here are not playing-strength evidence.

## Running

```
cargo test --release -p mtg-kernel --features eternal-fixtures --test eternal_integration cube_draft_v1
cargo run --release -p mtg-kernel --features eternal-fixtures --example cube_draft_timing_v1 -- --seed 20261122 --drafts 100 --pods 1
```

The test plays two games. The timing example runs on a single thread and prints one JSON
line with:

- drafts (draft plus deck build) per core-second;
- full pods per core-hour;
- game end counts by kind.
