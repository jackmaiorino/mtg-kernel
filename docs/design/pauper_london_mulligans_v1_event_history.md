# Event-history enumeration for Pauper London mulligans v1

Design section 3, step 4 (`pauper_london_mulligans_v1.md`). Question: can any
reader of `engine.event_history` reachable in the Pauper build count a London
pregame event as a turn-1 event? Outcome: **no**. Nothing is bounded and no
`GameState` field is added.

## What a London opening adds

Tested by `pauper_london_pregame_history_holds_only_hand_to_library_moves_and_draws`
(`mtg-kernel/src/rl_session/pauper_london_v1/tests.rs`), both starting seats:

- only `Draw { object: Some(_) }` (redraws) and `ZoneChange { from: Hand,
  to: Library }` (returned hands and bottoms) are appended;
- all of them precede the first `UpkeepBegan`; the deal prefix equals the
  `keep_seven_v2` deal; shuffles append no event;
- `event_log` is cleared and `draws_this_turn` zeroed at completion, as the
  `keep_seven_v2` handoff does. The keep-seven deal already holds fourteen
  pregame `Draw` events, so `Draw` is not a new pregame kind.

## Readers

Found by scanning every `event_history` use in `mtg-kernel/src` for windows
that match `Draw` or `ZoneChange`, then reading each.

| Reader | How it reads | Pregame event counted as turn 1? |
|---|---|---|
| `eternal_cards_v1.rs` `permanent_left_battlefield_this_turn` | backward to `UpkeepBegan`, `ZoneChange` from Battlefield | no: stops at upkeep; pregame has no battlefield move |
| `eternal_cards_v1/e3.rs` `opponent_cast_blue_or_black_spell_this_turn` | backward to `UpkeepBegan`, `SpellCast` | no |
| `eternal_legacy_e9a_v1/targets.rs` `died_this_turn` | backward to `UpkeepBegan`, Battlefield to Graveyard | no |
| `engine.rs` about 3106, 16431, 18326, 19949, 20762, 23052, 26350, 28145, 28882, 29652, 33925 | slice from an index captured in game (`len()` at capture) | no |
| `engine.rs` 2795-2803 | moves of one token spell copy's id | no: tokens do not exist pregame |
| `controlled_creature_grants_v1.rs` 329, `event.rs` 140/176 | journal record plus its recorded departure index | no |
| `engine/mandatory_cast_cost_v1.rs` 226, `engine/tinybones_v72.rs` 326, `engine/muldrotha_v1.rs` 831, `librarian_public_v1.rs` 222, `graveyard_return_public_v61.rs` 965, `banishing_light_v1.rs`, `extra_turn_v1.rs`, `progenitus_v1.rs`, `effect/harvester_v1.rs`, `fake_your_own_death_v50/*` | one journal's object, kind or recorded index; Battlefield, Stack or Graveyard moves | no |
| `trigger.rs` 6562 | `event_log` | no: cleared at completion |
| trigger draw counts | `draws_this_turn` minus pending `Draw` events | no: zeroed at completion |
| `game_summary_v1.rs` fold | whole game, `times_drawn` per card | not a turn-1 reader (see below) |

`game_summary_v1` builds the between-games Bo3 sideboarding summary over the
whole game, so a London game's redraws would count as draws there. It is not
reachable from the training collector, and Bo3 London is a follow-up
(`Bo3OpeningProtocolV1::LondonV1`, design section 8) whose design gate should
decide whether pregame redraws belong in that summary.
