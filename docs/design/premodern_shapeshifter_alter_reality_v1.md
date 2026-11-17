# Volrath's Shapeshifter Full and Alter Reality

Status: design for review, issue #460. This document changes no source, test or data
file. It specifies the work that makes the last two Premodern cards Full, so that the
research lead can rule on it as a whole and an implementer can build from it.

Code references are to main at `fd9ff09a`. Line numbers drift; each reference also names
the function or constant. Rules are cited from the
[November 6, 2026 Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt),
as in `volrath_full_text_v1.md`.

## 1. Status and scope

Issue #460 stays open until Volrath's Shapeshifter (registered `partial`, catalog ID 1200,
`premodern-full-english-breakfast` mainboard, 3.0% of the snapshot field) and Alter Reality
(no registry row, one copy in the `premodern-replenish` sideboard, 6.0%) are Full. Both need
a public contract extension. The lead ruled on the three open points:

1. Donor scope: "Donor scope as proposed, with a typed refusal for any other donor at the
   moment it would become the top card. Refused beats wrong." The proposal was the Full
   English Breakfast (FEB) mirror donors, the five FEB sideboard creatures (Auramancer, Monk
   Realist, Uktabi Orangutan, Ghitu Slinger, Xantid Swarm) and the two Premodern control
   effects (Dominate, Legacy's Allure).
2. Public consumer: "Widen `premodern_ordered_public_v1::feb` to any two registered Premodern
   decks with sideboards and best of three; schema 5 untouched. No schema successor."
3. Alter Reality: "the per-object color-word record, the bounded audited table with refusal
   outside it, and one schema 5 observation field by the S1k precedent, on condition that
   every pinned golden of a game without the card is byte-identical."

Priority, as ruled: behind the chain steps and behind issue #657's pieces; this work takes no
hosted runner from the chain.

This document adds what the rulings leave to design: the exact supported donor table, the
refusal's name, check sites and reachability argument (section 2); the registered owner's
identity, registration, routing and refusals (section 3); the color-word record, the complete
audited table of the 305 Premodern cards and the schema 5 field (section 4); the tests that
gate Full (section 5); the census effect (section 6); the pull request plan (section 7); and
what remains after both cards are Full (section 8). Section 9 lists the points where reading
the code left a question, each with a proposed answer. Section 10 records how the research
note this work started from was checked against main.

Out of scope: a schema successor or any change to schema 5 beyond Alter Reality's field and
its action meaning; donors outside the ruled set; copies of the Shapeshifter (no card in the
Premodern pool copies a permanent); a general text-replacement layer for the whole Eternal
and Standard catalog; flat policy encoders, search, infoset export and the Spellbench bridge
for these games (they keep refusing); training.

## 2. Volrath's Shapeshifter: the Full contract

### 2.1 What exists

The layer-3 primitive is `mtg-kernel/src/full_text_v1.rs` (unchanged since the research note).
`top_creature_text` reads a face-up battlefield host and its current controller's graveyard
top, through `graveyard_order_v1::batch_text_top` while an owner-ordered batch is pending and
`graveyard_text_top` otherwise. A noncreature top disables the effect without searching
backward; tokens and spell copies are skipped. The engine's characteristic queries read
`active_definition` (color, mana value, supertypes, names, base power and toughness,
keywords, subtypes in `engine.rs`), so a donor's characteristics and keywords are generic.

Abilities are not generic. Today a donor outside the tables silently donates characteristics
and keywords and drops everything else, which is why the card is Partial:

- Activated: `FullTextAbilityV1::public_program` is a closed table (Palinchron, Phyrexian
  Devourer, Triskelion, Battlefield Scrounger, Hermit Druid, Psychatog's two abilities, Mogg
  Fanatic). `engine.rs::available_activatable_abilities` offers a donated slot only when
  `full_text_activation_selector` finds it in that table.
- Mana: `full_text_v1::mana_text` (Birds of Paradise, Wall of Roots with its once-per-turn
  record `FullTextRestrictedUseV1`).
- Triggered: `DonatedTriggerKindV1` (Palinchron and Karmic Guide entry, Ravenous Rats entry,
  Phyrexian Devourer's power check, Karmic Guide's echo). `DonatedTriggerKindV1::entry`
  returns `None` for any other donor, and `record_entry_trigger` then queues nothing.
- Static and replacement: name keyed. `eternal_premodern_p2c_v1/statics.rs::protection_mask`
  (Akroma: black and red; Karmic Guide: black) and `apply_entry_counters` (Triskelion) read
  `active_definition`. Meddling Mage's entry choice (`eternal_sideboard_s1b_v1`) does not read
  full text.

### 2.2 The supported donor set

A donor is a creature card that can be the top card of the graveyard of the player who
controls a face-up Shapeshifter. The set below is closed. `full_text_v1` gains a constant
`SUPPORTED_DONORS_V1` with these names and a predicate `donor_supported(definition)`; every
other creature card is refused (section 2.3).

| Donor | Where it can come from | What it donates | Program today | Work |
| --- | --- | --- | --- | --- |
| Akroma, Angel of Wrath | FEB main | flying, first strike, vigilance, trample, haste; protection from black and from red; legendary | keywords generic; protection in `p2c::statics::protection_mask` | a legend-rule test (section 5.1) |
| Battlefield Scrounger | FEB main | threshold activation, once each turn | `ScroungerThresholdPump` | none |
| Birds of Paradise | FEB main | flying; mana | `mana_text` | none |
| Elvish Spirit Guide | FEB main | none on the battlefield (its ability works from hand) | `FullTextAbilityV1::resolve` keeps battlefield abilities only | none |
| Hermit Druid | FEB main | activation | `HermitDruidRevealThroughBasic` | none |
| Karmic Guide | FEB main | flying; protection from black; echo; entry return | entry kinds, `KarmicGuideEcho`, protection | none |
| Palinchron | FEB main | flying; entry untap; return-to-hand activation | entry kinds, `PalinchronReturnHost` | none |
| Phyrexian Devourer | FEB main | power-7 sacrifice; exile-top activation | `DevourerPower`, `DevourerExileTopForCounters` | none |
| Psychatog | FEB main; Psychatog 75 (through Dominate) | two activations | `PsychatogDiscardPump`, `PsychatogExilePump` | none |
| Squee, Goblin Nabob | FEB main | none on the battlefield (graveyard ability); legendary | generic | a legend-rule test |
| Triskelion | FEB main | enters with three counters; activation; artifact type | `apply_entry_counters`, `TriskelionRemoveCounterForDamage` | none |
| Volrath's Shapeshifter | FEB main | its own text: retained discard only | slot 0 | none |
| Wall of Roots | FEB main | defender; mana once each turn | `mana_text`, `FullTextRestrictedUseV1` | none |
| Auramancer | FEB sideboard | entry: may return target enchantment card from its controller's graveyard | `eternal_premodern_p3h_v1::auramancer_enters` (physical only) | new entry kind |
| Monk Realist | FEB sideboard | entry: destroy target enchantment | `p3h::monk_realist_enters` | new entry kind |
| Uktabi Orangutan | FEB sideboard | entry: destroy target artifact | `eternal_premodern_p3c_v1` `UKTABI_ORANGUTAN_TRIGGERS` | new entry kind |
| Ghitu Slinger | FEB sideboard | echo {2}{R}; entry: 2 damage to any target | `p3h::ghitu_slinger_enters`, `p2d::echo_cost` | new entry kind; generalize the donated echo |
| Xantid Swarm | FEB sideboard | flying; attack trigger: defending player can't cast spells this turn | `eternal_premodern_p3b_v1` `XANTID_SWARM_TRIGGERS` | new attack kind |
| Meddling Mage | Replenish sideboard (through Legacy's Allure) | its "as enters" name choice, which never happens for a Shapeshifter already on the battlefield | none needed | a test; refusal on entry (below) |
| Mogg Fanatic | not reachable from the snapshot lists | sacrifice for damage | `MoggFanaticSacrificeForDamage` | none (section 9, question 1) |
| Ravenous Rats | not reachable from the snapshot lists | entry discard | `RavenousRatsEntered` | none (section 9, question 1) |

New programs, each following the existing pattern exactly:

- Entry kinds `AuramancerEntered`, `MonkRealistEntered`, `UktabiOrangutanEntered`,
  `GhituSlingerEntered`, each with its `...AfterBatch { entry, marker }` twin, appended to
  `DonatedTriggerKindV1`. `DonatedTriggerKindV1::entry`, `target_spec`,
  `DonatedTriggerCaptureV1::program`, `trigger_record`, `trigger_producer` and the
  `FullTextProgramV1` public meanings gain one row each. The programs call the existing
  physical recipes (`auramancer_enters`, `monk_realist_enters`, the Orangutan trigger,
  `ghitu_slinger_enters`) with their existing target specs (`p3h::AURAMANCER_TARGET`,
  `TargetSpec::EnchantmentPermanent`, `TargetSpec::ArtifactPermanent`, `TargetSpec::AnyTarget`).
  Auramancer's return and Monk Realist's destruction can change the top card of the
  Shapeshifter controller's graveyard; the trigger is already frozen (113.7a), and the new top
  is audited like any other change (section 2.3).
- Donated echo: `KarmicGuideEcho { upkeep, control }` becomes the model for a general
  `DonatedEcho { upkeep, control }` whose cost comes from the captured donor definition
  through `p2d::echo_cost`. Today `P2dOpV1::DonatedGuideEchoPayV1` hard-codes Karmic Guide's
  cost (`eternal_premodern_p2d_v1.rs`, the `DonatedGuideEchoPayV1` arm of `execute`); it
  takes the captured donor's cost instead, so a departed Slinger donor still costs {2}{R}
  (113.7a, 702.30a). `queue_upkeep_triggers` queues it for any donor with an echo cost. The
  existing Guide variant stays readable for restored states.
- Attack kind `XantidSwarmAttacked { attack }`: captured when the host is declared as an
  attacker while its text is Xantid Swarm's, frozen like the entry kinds, resolving through
  the physical program of `XANTID_SWARM_TRIGGERS`.
- Meddling Mage: its only ability is a replacement as it enters (614.12 reads the text the
  permanent would have on the battlefield). A Shapeshifter that gains Mage text later names
  nothing and prohibits nothing; that is correct and needs only a test. A Shapeshifter that
  would enter while its controller's top card is Meddling Mage would need the name choice,
  which `meddling_mage_entry` does not offer to a full-text host. That entry is refused with
  the same typed refusal. It is unreachable from the snapshot lists: no deck with Meddling
  Mage can cast or put onto the battlefield a Shapeshifter it controls.

### 2.3 The typed refusal

Name and shape: `UnsupportedMechanic::FullTextDonorOutsideSetV1`, appended to
`engine::UnsupportedMechanic` (whose doc already reserves it for runtime mechanics the
kernel cannot simulate; appending keeps every earlier discriminant and serialization). The
halt is `state.engine.halted = Some((FullTextDonorOutsideSetV1, host))`, where `host` is the
Shapeshifter, and `advance_until_decision` then returns `Decision::Halted` for good. One pure
function decides it:

```text
full_text_v1::refused_donor(state) -> Option<ObjectId>
  for each face-up battlefield object whose physical definition owns_definition:
    if top_creature_text(state, host) = Some(text) and !donor_supported(text.definition()):
      return Some(host)
  None
```

plus the entry case: `record_entry_trigger` refuses a host whose text at entry is Meddling
Mage's.

Every event that can make a card the top of a graveyard a Shapeshifter is looking at, or make
a Shapeshifter look at a top card:

| Event | Example in the Premodern pool | Check site |
| --- | --- | --- |
| A card arrives in the controller's graveyard (death, discard, mill, destruction, sacrifice, a resolved or countered spell, cycling) | Hermit Druid's reveal, Cabal Therapy, Wrath of God | the commit tails in `event.rs` (`propose_and_commit`, and `propose_and_commit_batch` after the batch's final text is selected, beside its `record_batch_entry_trigger` loop) |
| The top card leaves the graveyard, exposing an older card | Karmic Guide, Animate Dead, Exhume, Psychatog's exile cost, Auramancer, Krosan Reclamation | the same commit tails |
| An owner orders a simultaneous batch | any two-card arrival under `order_graveyard_batches_v1` | `graveyard_order_v1`, where the order completes and `record_batch_entry_trigger` runs for the final text |
| A Shapeshifter enters the battlefield | cast, Shallow Grave, Animate Dead, Parallax Wave's return | `event.rs::record_entry_trigger` (already reached for every entry) |
| A face-down Shapeshifter is turned face up | none in the Premodern pool | the backstop below |
| A Shapeshifter changes controller | Dominate, Legacy's Allure | `full_text_v1::record_control_gained`, which both control paths already call (`standard_cards_v1::gain_control`, `effect/temporary_control_v1.rs::transfer`) |
| A Shapeshifter phases in | Vision Charm phases out a Shapeshifter with Triskelion's text (an artifact) | `eternal_e5a_v1::phasing`, where a permanent phases in |
| A Shapeshifter becomes a copy target or a copy becomes a Shapeshifter | none in the Premodern pool | covered by the backstop below |

Backstop: `refused_donor` is also added to the validation chain at the top of
`engine::advance_until_decision_historical` (beside `eternal_opening_v1::validate` and
`leyline_axe_v1::validate`). That guarantees no decision is ever offered while an unsupported
donor is in place, whatever path changed the top. The event sites above make the halt fire at
the event that caused it, so a refused donor never acts in a partial resolution that is later
observed. The scan costs one battlefield pass per check and returns at once when no
Shapeshifter is on the battlefield.

What a session reports when it fires: the frozen FEB owner returns
`FebCurrentV1::NativeHalt` and its JSONL route `{"state_type": "native_halt"}`, unchanged. The
registered owner (section 3) reports the halt with its mechanic and the host's public card
reference. A halted game is never a completed game: no terminal is reported, the
best-of-three driver records no game result, and every harness that sees a halt fails the
game. Schema 5 never sees it, because the schema 5 session refuses a Shapeshifter deck at
reset (section 3.6).

Why no supported game reaches it. A card goes only to its owner's graveyard (400.3), and the
Shapeshifter reads only its controller's graveyard, so every donor is a creature card from the
controlling player's own 75. Tokens and spell copies are skipped by `graveyard_text_top`. No
card in the 305-card Premodern pool copies a permanent, so only a physical Shapeshifter is a
host. A Shapeshifter changes controller only through Dominate or Legacy's Allure, the only
control effects in the pool. From the deck lists in
`data/eternal_v1/meta_snapshot_mtgtop8_premodern_20261101.json`:

| Registered deck | Can control a Shapeshifter by | Creature cards in its 75 (possible donors) |
| --- | --- | --- |
| `premodern-full-english-breakfast` | casting or returning its own | main: Akroma, Angel of Wrath; Battlefield Scrounger; Birds of Paradise; Elvish Spirit Guide; Hermit Druid; Karmic Guide; Palinchron; Phyrexian Devourer; Psychatog; Squee, Goblin Nabob; Triskelion; Volrath's Shapeshifter; Wall of Roots. Sideboard: Auramancer, Ghitu Slinger, Monk Realist, Uktabi Orangutan, Xantid Swarm |
| `premodern-psychatog` | Dominate (1, sideboard only: games two and three) | Psychatog |
| `premodern-replenish` | Legacy's Allure (1, mainboard) | Meddling Mage (4, sideboard) |
| the other 21 decks | nothing | not donors |

Every name in the right column is in the supported set, and the Mage case is the "already on
the battlefield" one. So the refusal is reachable only from a custom registration, never from
a game between snapshot decks.

## 3. The public consumer: the registered ordered owner

### 3.1 What is frozen and stays byte-identical

The frozen owner is `premodern_ordered_public_v1::feb::FebSessionV1::reset(seed,
starting_player)` and its JSONL route `kernel_limited_env --premodern-feb-v1`
(`docs/design/premodern_feb_jsonl_v1.md`). The widening adds a second constructor and leaves
every frozen path in place. Pins and where each is checked:

| Pin | Value | Defined | Checked |
| --- | --- | --- | --- |
| Owner profile | `mtg-kernel-premodern-feb-ordered/v1` | `feb.rs` `PROFILE_V1` | `tests/eternal/premodern_feb_jsonl_v1.rs` (`assert_eq!(reply["profile"], PROFILE_V1)`), `python/tools/premodern_feb_session_v1.py` `PROFILE`, `python/tests/test_premodern_feb_session_v1.py` |
| JSONL protocol and schema | `kernel_premodern_feb_jsonl`, `1` | `feb_jsonl.rs` `PROTOCOL`, `SCHEMA_VERSION` | the same JSONL test and Python client (`PROTOCOL`) |
| Ordered schemas | `mtg-kernel-premodern-ordered-public/v1`, `mtg-kernel-numeric-premodern-ordered/v1`, `mtg-kernel-human-premodern-ordered-public/v1`, `mtg-kernel-human-premodern-graveyard-order/v1` | `premodern_ordered_public_v1.rs` `SCHEMA_V1` to `HUMAN_ORDER_SCHEMA_V1` | the owner's view tests in `feb_tests.rs` |
| Snapshot bytes | SHA-256 `9e9db030...` | `feb.rs` `SNAPSHOT_SHA256` | `feb.rs::mainboard` at first use |
| Frozen mainboard | 27 rows, no companion, 60 cards, snapshot row order | `feb.rs::mainboard` | `resolve_frozen_mainboard_v1` callers |
| Reset request | no deck, state or rules fields | `feb_jsonl.rs` `Request` (`deny_unknown_fields`) | `feb_jsonl_tests.rs::feb_jsonl_rejects_foreign_profiles_decks_and_excess_snapshots` |
| Terminal and replay | seed `20261108`, both starting seats | `feb_tests.rs::frozen_feb_mirrors_reach_native_terminal_and_replay_both_starting_seats`; `tests/eternal/premodern_feb_jsonl_v1.rs::feb_jsonl_subprocess_reaches_terminal_and_replays_both_starting_seats` | self-replay (actor, menu, ordinal sequence, natural result, state hash, event history) |
| Native client qualification | `test_native_opening_reset_step_snapshot_restore_both_seats` | `python/tests/test_premodern_feb_session_v1.py` | `MTG_KERNEL_FEB_ENV_BIN` and `MTG_KERNEL_FEB_CARD_DB_HASH` |

There is no stored byte golden for the mirror; its byte identity is the frozen code path left
unchanged plus the self-replay tests above. One frozen line must change with the flip to Full:
`feb.rs::mainboard` requires Volrath's Shapeshifter to be `CardCapability::Partial` with two
copies. It becomes "two copies and Full" in the pull request that flips the row (section 7,
PR B2). The catalog hash the owner reports (`KERNEL_CARDDB_HASH`) changes with that row, as it
does with every registry wave; clients already pass the current hash.

### 3.2 Identity of the widened owner

- Profile `mtg-kernel-premodern-ordered-registered/v1` (`feb::REGISTERED_PROFILE_V1`), reported
  in every numeric and Human view of a registered game in place of `PROFILE_V1`.
- Numeric stable scene schema `mtg-kernel-numeric-premodern-ordered-registered/v1`. The
  ordered part (`ObservationV1`: graveyards bottom to top, `unplaced`, `order_choice`,
  `full_text`, `producers`) keeps `SCHEMA_V1` and its shape. The stable part is a new struct
  whose `projection` is `rl::PublicObservationProjectionV5` (the schema 5 projection with its
  optional `foundations_combat`, `london_mulligans`, `public_shroud`, `meddling_mage_names`
  and, after section 4, the text-change field) instead of the V2 projection the frozen scene
  uses. The frozen `NumericStableSceneV1` is unchanged.
- A new observation text mode in `rl.rs`, `PremodernOrderedRegisteredV1Artifact`, which
  accepts exactly the union of what `PublicShroudV5Artifact` accepts (shroud, Meddling Mage
  names) and what `PremodernOrderedV1Artifact` accepts (ordered graveyards and full text,
  `fixed_source_damage_v3::validate_state`, `crystal_prevention_v1` with full text). The
  implementer lists every `text_mode` branch in `build_observation_common_v2_fishing_v39_conqueror_v49_fake_v50`
  and `build_observation_v2` in the pull request and states which side each takes. This is
  needed: the frozen scene refuses shroud (`refuse_unrepresented_shroud_in_mode`) and Meddling
  Mage names (`meddling_mage_entry::requires_successor`), so Enchantress (Argothian
  Enchantress, Sterling Grove), White Weenie (True Believer) and the Mage decks would halt as
  unsupported in a widened game without it.

### 3.3 Registration and validation

`eternal_bo3_v1` gains an ordered-graveyard registration profile beside the native opening
one, and the S1i pattern carries over:

- `EternalRegisteredDeckV1::new_with_ordered_graveyard_v1(format, deck_id, mainboard,
  sideboard, companion)`. It requires `EternalFormatV1::Premodern`, every card Full and not a
  token, the usual bounds and copy limit, no companion (the Premodern snapshot has none) and
  no native-opening card. Section 9, question 2 proposes also requiring every card to be in
  the 305-card Premodern snapshot pool.
- `register` refuses a card for which `full_text_v1::owns_definition` holds unless the
  profile is on, with an appended `EternalBo3ErrorV1::OrderedGraveyardSetup { reason }`.
- `EternalBestOfThreeMatchV1::new_v1` requires both registrations to share the profile
  (appended `EternalBo3ErrorV1::GraveyardProfileMismatch`), as it does for the opening
  profile.

### 3.4 Sideboarding and the three games

The match state machine is unchanged (`BestOfThreeMatchStateV1`, `admit_configuration_v1`,
`RandomSideboardPolicyV1`). A new
`EternalBestOfThreeMatchV1::start_game_with_ordered_graveyard_v1(chooser, choice,
configurations, seed)` mirrors `start_game_with_native_opening_v1`: it admits both
configurations, maps native seat P0 to the starting player, and returns the prepared game and
an owner built by `FebSessionV1::reset_registered_v1`. That constructor:

1. deals each seat's configured mainboard with the frozen reset's procedure (one `SplitMix64`
   from the game seed, seat 0's library then seat 1's, seven alternating draws);
2. creates the state with `GameState::new_from_libraries_with_graveyard_order_v1`;
3. turns on Foundations combat (`combat_damage_v1::enable_foundations_combat_v1`), London
   mulligans (`london_mulligan_v1::enable_london_mulligans_v1`) and the public shroud flag
   (`rl::enable_public_shroud_v1`), the rules every catalog game of the other 23 decks plays
   under in schema 5; the frozen mirror keeps its own profile, which has no Foundations combat;
4. puts each seat's sideboard outside the game (`eternal_outside_game_v1::set_outside_the_game`);
5. binds the first decision with the frozen owner's `decision::bind_next` and
   `HarnessSurfaceV2` in `EngineWindowsV1` priority mode.

Step, snapshot, restore, stale-token and same-owner rules are the frozen owner's. A natural
`GameOver` is recorded with `record_game_result_v1`; nothing else is.

### 3.5 What each reader sees

Each seat sees, for itself as viewer: both graveyards bottom to top; during an owner order,
the established order and the sorted `unplaced` arrivals, with the order menu only for the
chooser; every face-up Shapeshifter's layer-3 characteristics with its public donor reference
(`FullTextFormV1`); every donated producer bound to `(host, donor, selector, slot)`
(`FullTextProducerV1`); and the schema 5 projection facts of section 3.2. A face-down
Shapeshifter shows no text and no donor. Hidden hands, libraries and face-down identities keep
the V2 privacy rules (`premodern_ordered_public_v1.rs` test
`paired_hidden_worlds_preserve_public_order_and_both_native_presentations` is the model for a
registered-profile twin). The Human view keeps the frozen renderer with its explicit
`unsupported` row for meanings it cannot label; numeric ordinals are the qualifying
submission (section 9, question 6).

### 3.6 Routing and refusals

Routing rule: a catalog game is played by the registered owner exactly when one of its decks
holds a card for which `full_text_v1::owns_definition` holds, that is a Volrath's
Shapeshifter. The ruling's "ordered-graveyard reader" (`engine::graveyard_order_readers_present`)
also covers Delve, but no card in the Premodern pool has Delve (Gurmag Angler, Murktide
Regent, Treasure Cruise, Dig Through Time and Hogaak are the registry's Delve cards), so Delve
games stay on schema 5 as today (section 9, question 5).

- `PLAYABLE` (`tests/eternal/eternal_catalog_v1.rs`, 137 entries) gains
  `premodern-full-english-breakfast`, inserted immediately before `premodern-reanimator` so
  that both of its neighbours are Premodern: the pairs become Madness Threshold against FEB and
  FEB against Reanimator. Such a pair registers both snapshot 75s with the ordered profile and
  plays game one only, with ranked and seeded random policies, through a new in-process driver
  (`tests/eternal/eternal_bo3_v1/eternal_bo3_ordered_v1.rs`), and must reach a natural terminal
  and replay to the same transcript and state hash. Other pairs are unchanged.
- `PLAYABLE_75` (`tests/eternal/eternal_bo3_v1.rs`, 124 entries) gains FEB (and Replenish,
  section 4). Matches pair in snapshot order within the format, so once both cards are Full
  the new pairs are Replenish against FEB and FEB against Stasis; if the Shapeshifter is Full
  first, they are Enchantress against FEB and FEB against Stasis. A match where either deck
  holds a Shapeshifter registers both decks with the ordered profile and plays every game
  through the same driver.
- The schema 5 session refuses a Shapeshifter deck at reset:
  `card_def::preflight_fully_supported_deck` gains an appended
  `DeckPreflightError::RequiresOrderedGraveyardOwnerV1`, as Leylines have
  `RequiresNativeOpening`. Today the same deck is refused there as `NotFullySupported`; the
  reply's error code is unchanged and only its message names the owner (section 9,
  question 3).

Refusals of the registered owner: a registration outside section 3.3 (typed
`EternalBo3ErrorV1`); a mismatched profile; a state its text mode cannot represent
(`FebCurrentV1::UnsupportedDecision`, as today); a native halt, including the donor refusal
(`NativeHalt` with mechanic and host); a foreign or stale decision token or snapshot (as
today).

### 3.7 Schema 5 is untouched

The widening adds no field, action or reply to schema 5. Evidence the pull requests must
show: their diffs touch `limited_session_v1.rs`, `PublicObservationProjectionV5` and the
schema 5 action builders not at all (B1, B2), apart from the preflight variant above;
`tests/eternal/eternal_public_shroud_v1.rs`, `tests/limited_session_v1.rs`,
`tests/london_mulligans_v1.rs` and `src/limited_prepared_session_v5/tests.rs` pass unchanged;
and the 137 catalog games of `playable_snapshot_decks_finish_and_replay_through_public_sessions`
that do not involve FEB print the same transcript SHA-256s before and after (from the CI
logs), since their pairs and seeds are unchanged.

## 4. Alter Reality

### 4.1 Card and rules

Alter Reality, {1}{U}, Instant: "Change the text of target spell or permanent by replacing all
instances of one color word with another. (This effect lasts indefinitely.) Flashback {1}{U}".
The registry has no rules text; the implementer cross-checks XMage's
`Mage.Sets/src/mage/cards/a/AlterReality.java` and records its commit and SHA-256 as
`volrath_full_text_v1.md` does.

- 612.1 and 612.2: a text-changing effect changes only words used as color words, never a
  proper name ("Circle of Protection: Red", "Black Vise", "Red Elemental Blast") and never a
  mana symbol. "Nonblack" uses "black" as a color word and changes. Basic land type words
  (swampwalk, Plainscycling, "Island") are not color words.
- 613.1c: layer 3. 613.7: two changes on one object apply in timestamp order ("black" to
  "green", then "green" to "red", turns both original black and green words red). 613.8: a
  change depends on Volrath's full text, which replaces the words it acts on, so the full text
  applies first whatever the timestamps; a change on a Shapeshifter acts on its current
  donated text.
- 608.2: both words are chosen as the spell resolves; target legality is checked then
  (608.2b).
- 400.7a: a change to a permanent spell continues on the permanent it becomes. Otherwise a
  change ends when the object changes zones (400.7).
- 113.7a: an ability already on the stack is independent of its source, and Alter Reality
  cannot target it. An activated or triggered ability reads its source's changed words when
  it is put on the stack.
- 707.2: copiable values exclude text-changing effects, so a copy never inherits a change.
- 708.8: effects applied to a face-down permanent keep applying once it is turned face up.
  Phasing (702.26) is not a zone change.
- 702.36: fear is a keyword; the black in its reminder text is not rules text and does not
  change (Avatar of Woe keeps its check in `p2c::statics::blocker_allowed`).
- 702.34: flashback is generic in the engine (Call of the Herd's `flashback` builder).

### 4.2 The per-object color-word record

Shape, in a new core module `mtg-kernel/src/text_color_change_v1.rs` (the counterpart of
`full_text_v1.rs`):

```text
TextColorChangeV1 { object: ObjectLinkV4, from: ManaColor, to: ManaColor, timestamp: u64 }
TextColorChangesV1(Vec<TextColorChangeV1>)   // nonempty while present
text_color(state, object, printed: ManaColor) -> ManaColor
text_color_mask(state, object, printed_mask: u8) -> u8
```

`text_color` folds the object's live changes in timestamp order. `text_color_mask` maps each
color bit through `text_color`. Timestamps come from `engine::next_timestamp`, called only when
Alter Reality resolves.

- Storage: its own field on `GameState`, `text_color_changes_v1: Option<TextColorChangesV1>`,
  serialized with `skip_serializing_if = "Option::is_none"` and hashed in
  `GameState`'s manual `Hash` only when present, under the domain `text-color-changes/v1`, as
  `eternal_opening_v1` and `leyline_opening_v1` are. It must not be a field of
  `eternal_cards_v1::EternalStateV1`: that struct derives `Hash`, so even an empty new field
  would change the state hash of every Eternal game that holds that state.
- Save and restore: part of the state snapshot, restored exactly; an empty record is `None`.
- End: when the object changes zones the link goes stale and the entry is removed at that
  zone change, except that a permanent spell's entries move to the permanent's new
  incarnation as it resolves onto the battlefield (400.7a). When the last entry goes, the
  field returns to `None`. Phasing (zone `Command` with the same incarnation,
  `eternal_e5a_v1::phasing`), control changes and face-down to face-up keep it.
- Copies: never copied (707.2). A token or permanent copy is a new object with no entry.
- Face-down targets: the change is recorded publicly and applies when the permanent is turned
  face up. The decision whether a target is in the audited table (section 4.4) never reads a
  face-down identity: a face-down permanent is accepted, and the audit runs when it is turned
  face up. The pool's morph creatures (Exalted Angel, Bane of the Living) have no color word.
- Abilities: a site that reads a word for an ability (Plaguebearer's target, Crimson Acolyte's
  grant, Circle of Protection: Red's choice, the token makers' triggers and activations, the
  two land animations) captures `text_color` of its source when the ability is put on the
  stack and stores the captured color with the stack item or its pending trigger, so a later
  change to the source does not reach it (113.7a).

### 4.3 The card

- Registry row appended to `data/eternal_v1/cards_v1.json`: `Alter Reality`, `full`,
  `{1}{U}`, mana value 2, colors `["U"]`, types `["Instant"]`, mechanics `["text_change",
  "flashback"]`, decks `["premodern-replenish"]`, `sideboard_only: true`, the XMage path.
  Appending keeps every existing ID (`eternal_registry_follows_the_pauper_and_standard_ids`).
- Target: "spell or permanent". No `TargetSpec` covers both; the card uses an appended
  `StandardTargetV1` variant with its own fingerprint code, so existing codes are unchanged.
- Resolution: if the target is illegal, nothing happens (608.2b). Otherwise two option choices
  by the controller: the old word (five options) and then the new word (the four other
  colors; a word replaced by itself changes nothing). They use `EffectOp::Choice` with an
  appended `EffectOptionChoicePurpose::ChooseTextColorWordV1 { stage, from }`, answered by
  `Action::ChooseEffectOption`. Then the record is appended, after the audit of section 4.4.
- Flashback {1}{U} with the existing builder.

### 4.4 The audited table

Source: the 305 distinct cards of the Premodern snapshot. The registry carries no rules text,
so every card was read against its current Oracle text, and where a site's doc comment
quotes the card, the quote was compared too. Of the 305: 36 have color words whose change alters
play, 5 have color words only in a cost already paid by the time Alter Reality can target them,
and 264 (Alter Reality itself among them) have none. Tokens the pool creates (Elephant,
Squirrel, Bear, Goblin, Angel, Soldier, Saproling, Zombie, Phyrexian Minion) have no rules
text and are in the no-word group.

Supported: every row below. "Capture" means the word is read when the ability goes on the
stack (section 4.2).

| # | Card | Words | Site that reads it (module) | A change there |
| ---: | --- | --- | --- | --- |
| 1 | Goblin Piledriver | blue | `eternal_premodern_p1a_v1/statics.rs::protection_mask` | protection follows the new word |
| 2 | Knight of Stromgald | white | same function | same |
| 3 | Mystic Enforcer | black | same function | same |
| 4 | Soltari Monk | black | `eternal_premodern_p2d_v1/statics.rs::protection_mask` | same |
| 5 | Soltari Priest | red | same function | same |
| 6 | Akroma, Angel of Wrath | black, red | `eternal_premodern_p2c_v1/statics.rs::protection_mask` (physical and full-text branches) | either or both words; also on a Shapeshifter with Akroma's text |
| 7 | Karmic Guide | black | same function | same; also on a Shapeshifter |
| 8 | Crimson Acolyte | red (printed protection) | `eternal_premodern_p3a_v1/statics.rs` (Crimson Acolyte mask) | same |
| 9 | Crimson Acolyte | red (its {W} grant) | `eternal_premodern_p3a_v1.rs` grant op, `build_eternal_p3a_v1.rs` | capture; the granted protection's color |
| 10 | Ihsan's Shade | white | `eternal_premodern_p3c_v1/statics.rs::protection_mask` | protection follows |
| 11 | Absolute Law | red | `eternal_premodern_p3e_v1.rs::protection_mask` | each Law grants its own color; evaluated per Law, not "any Law" |
| 12 | Light of Day | black | `eternal_sideboard_s1c_v1.rs::statics::banned_by_light_of_day` | each Light of Day bans its own color; `any_active` becomes per instance |
| 13 | Sphere of Law | red | `eternal_premodern_p3a_v1/statics.rs::sphere_of_law_prevents` | 2 prevented per Sphere whose color matches the source; the count becomes per instance |
| 14 | Chill | red | `p3a/statics.rs` Chill tax, called from `spell_costs_v1.rs` | {2} per Chill whose color the spell has |
| 15 | Gloom | white (spells) | `eternal_premodern_p3b_v1/statics.rs` spell tax, called from `spell_costs_v1.rs` | {3} per Gloom whose color the spell has |
| 16 | Gloom | white (enchantment activations) | `p3b/statics.rs` activation tax, called from `engine.rs` | {3} per Gloom whose color the enchantment has |
| 17 | Warmth | red | `eternal_premodern_p3a_v1.rs` Warmth trigger (`active_permanents(state, WARMTH, ...)`) | each Warmth triggers on its own color |
| 18 | Dystopia | green, white | `eternal_premodern_p3b_v1.rs` upkeep trigger and the color mask in `effect.rs` (the "any of these colors" filter) | capture at trigger; either or both words |
| 19 | Circle of Protection: Red | red | `eternal_premodern_p3g_v1.rs::circle_of_protection_red`, `eternal_sideboard_s1d_v1::circle_choose(CircleColorV1::Red)` | capture; `CircleColorV1` has Blue and Red only and gains a general color |
| 20 | Quirion Dryad | white, blue, black, red | `eternal_premodern_p1b_v1.rs` Dryad trigger filter | the trigger's color set follows the words |
| 21 | Blue Elemental Blast | red (both modes) | `TargetSpec::RedSpellOnStack`, `TargetSpec::RedPermanent` (`card_def.rs`), the resolution legality recheck in `engine.rs` | a change on the stack can make the chosen target illegal; targets are enumerated before any change can exist, so only the recheck reads it |
| 22 | Red Elemental Blast | blue (both modes) | `TargetSpec::BlueSpellOnStack`, `TargetSpec::BluePermanent`, same recheck | same; the main defensive use |
| 23 | Pyroblast | blue (both modes) | `EffectCond::TargetIsColor(0, U)` in `effect.rs` | resolves and does nothing to an object without the new color |
| 24 | Hydroblast | red (both modes) | `EffectCond::TargetIsColor(0, R)` | same |
| 25 | Snuff Out | black ("nonblack") | `TargetSpec::NonblackCreature` recheck in `engine.rs` | the target may become illegal on the stack |
| 26 | Plaguebearer | black ("nonblack") | `eternal_premodern_p3b_v1/targets.rs` (`NonblackCreatureManaValueV1`) | capture at activation; enumeration and recheck use it |
| 27 | Anarchy | white | `eternal_premodern_p3a_v1.rs` `P3aOpV1::AnarchyV1` | on the stack: destroys permanents of the new color |
| 28 | Call of the Herd | green | `eternal_premodern_p1a_v1.rs` Elephant token (spell and flashback) | the token's color is the new word |
| 29 | Siege-Gang Commander | red | `p1a` entry trigger, Goblin token | capture at trigger (a change made on the spell carries over, 400.7a) |
| 30 | Decree of Justice | white | `eternal_premodern_p1b_v1.rs` Angel token (hard cast) | on the stack only; the cycling trigger is an ability from hand and cannot be targeted |
| 31 | Chatter of the Squirrel | green | `eternal_premodern_p2b_v1.rs` Squirrel token | token color |
| 32 | Deranged Hermit | green | `p2b` entry trigger, Squirrel tokens | capture at trigger |
| 33 | Caller of the Claw | green | `p2b` entry trigger, Bear tokens | capture at trigger |
| 34 | Verdant Force | green | `eternal_premodern_p2c_v1.rs` upkeep trigger, Saproling token | capture at each trigger |
| 35 | Zombie Infestation | black | `build_eternal_p2a_v1.rs` recipe `CreateToken("Zombie Token")` | capture at activation |
| 36 | Phyrexian Processor | black | `eternal_premodern_p3f_v1.rs` Minion token | capture at activation |
| 37 | Treetop Village | green | `build_support/standard_lands_v1.rs` animation colors, read through `continuous_characteristics_v1::animation` | capture at activation |
| 38 | Ghitu Encampment | red | same | same |

That is 36 cards and 38 sites (Crimson Acolyte and Gloom have two each), in 16 module
families: 11 Premodern wave modules (P1a, P1b, P2a's build table, P2b, P2c, P2d, P3a, P3b,
P3c, P3e, P3f), two sideboard wave modules (S1c, S1d with P3g), the core engine, effect and
card definition files (including `spell_costs_v1.rs`), and the land build table. A token's
color is set as it is created through a base-color entry read at the printed-color fallback
of `engine::unpainted_object_color_mask`, so later layer-5 setters (Wild Mongrel, Painter's
Servant, Mycosynth Lattice) still override it.

Supported as a recorded change with no effect (the word is in a cost paid from hand or
graveyard before Alter Reality can target the spell): Unmask, Contagion, Pyrokinesis
(`CostComponent::ExileCardFromHandWithColorV1`), Misdirection, Flash of Insight's flashback
cost.

Supported as a recorded change with no effect: the 264 cards with no color word, and the pool's
tokens. Examples worth stating because they look otherwise: Black Vise, Circle of Protection:
Red's name, Red and Blue Elemental Blast's names (proper names); Iridescent Angel and Mother of
Runes ("all colors", "the color of your choice"); Avatar of Woe (fear); Funeral Charm, Snuff
Out's alternative cost, Choke, Daze, Gush, Thwart, Foil, Mogg Salvage, Fireblast, Lava Dart,
Quirion Ranger, Anger, Tainted Field and Tainted Wood, Eternal Dragon and Tranquil Thicket
(land types); Lurking Skirge (its current text has no color word).

Refusal: any target whose face-up definition is outside the 305 cards and their tokens halts
the game at resolution with an appended `UnsupportedMechanic::TextColorChangeOutsideAuditV1`
(source: the Alter Reality spell). The audited list is a constant in
`text_color_change_v1.rs` with the 41 color-word names and a test that every Premodern
snapshot card is either in it or audited as having none. A Shapeshifter is audited by its
current text (its own text has no color word; its supported donors are in the pool). The
audit makes no guess about a card outside the list, even when the change would be a no-op.

Reconciliation with the research note's "36 cards, about 30 sites, 10 modules": the 36 cards
are confirmed. The site count is 38 because the note did not count Gloom's two taxes, Crimson
Acolyte's grant or the shared Blast checks separately, and the module count is 16 rather than
10. One row of the note was wrong: Crimson Acolyte's {W} ability grants protection from red,
not from green (the engine's quote in `eternal_premodern_p3a_v1.rs` agrees).

### 4.5 The schema 5 observation field and action meaning

Modelled on S1k (`docs/design/limited_public_shroud_v1.md`, the S1k section of
`eternal_formats_support_v1.md`, and `rl.rs` `meddling_mage_names`):

- Field: `projection.text_color_changes: Option<Vec<PublicTextColorChangeV1>>` on
  `rl::PublicObservationProjectionV5`, with `#[serde(default, skip_serializing_if =
  "Option::is_none")]`. Each entry is `{ object: CardStableRefV1, from: ManaColor, to:
  ManaColor }`, the object being the public reference the projection already uses for that
  battlefield permanent or stack spell (a face-down permanent's public reference, never its
  identity). Order: seat 0's battlefield then seat 1's in public battlefield order, then the
  stack bottom to top; one object's entries in timestamp order, which is their meaning
  (613.7). Present only under the public shroud contract (as `meddling_mage_names` is, through
  `ObservationTextModeV2::PublicShroudV5Artifact`) and only when the list is nonempty; absent
  otherwise.
- Action meaning: appended `ActionSemanticV1::ChooseTextColorWordV1 { actor, source, target,
  stage, from, color, option_index, option_count }`, serialized `choose_text_color_word_v1`.
  `source` is the resolving Alter Reality, `target` its `TargetRefV1`, `stage` is `from` or
  `to`, `from` is the chosen old word in the `to` stage, `color` the word this option offers.
  It answers with `ChooseEffectOption(option_index)`. Stage `from` offers the five colors in
  WUBRG order; stage `to` the four others in the same order.
- Every other consumer refuses a state with a record or a pending text-word choice, as for
  shroud and Meddling Mage: schema 4 and earlier observation versions, flat encoders and
  action slices, infoset export, search, the Spellbench bridge. The registered ordered owner
  (section 3) carries the field through its V5 projection, so Replenish against FEB works.

### 4.6 The condition: games without the card stay byte-identical

| Pin family | Why it cannot move | Test that proves it |
| --- | --- | --- |
| Schema 5 observation goldens and hashes | the field is absent unless a change applies, and `visible_projection_hash` covers serialized bytes | a new test asserts that a schema 5 observation of a game without the card has no `text_color_changes` key; `tests/eternal/eternal_public_shroud_v1.rs`, `tests/limited_session_v1.rs`, `tests/london_mulligans_v1.rs`, `src/limited_prepared_session_v5/tests.rs` pass unchanged |
| Session transcripts | the catalog's single games play mainboards (Alter Reality is sideboard only) and keep their pairs and seeds; best-of-three matches without Replenish never see it | the C2 pull request quotes the transcript SHA-256 lines of the catalog and best-of-three runs before and after: identical for every pair and match not involving Replenish |
| State hashes and snapshot bytes | `text_color_changes_v1` is `None` and skipped, hashed only when present; no existing struct gains a field; `next_timestamp` is drawn only by the card | the pinned state goldens pass unchanged: `environment_randomization_v2/reset_physical_trajectory_goldens_v1.rs`, `native_full_episode_trajectory_v1_goldens.rs`, `rl_session.rs` `environment_hashes_are_diagnostic_dispatched_with_exact_goldens` and `flat_action_core_matches_hand_authored_all_kind_golden`; a unit test checks a state whose only change was removed hashes like one that never had it |
| Catalog hash rules | the default and FDN catalogs gain nothing; appended enum variants keep existing fingerprint codes | `card_def.rs` `card_db_hash_v34_is_frozen` and `card_db_hash_full_pool_provisional_fdn_is_frozen` unchanged; `card_db_hash_eternal_v1_is_frozen` is re-pinned once, by the row (PR C2), with the value the build prints |

### 4.7 Volrath and Alter Reality together

Replenish (Alter Reality in the sideboard, Legacy's Allure in the mainboard) meets FEB in
best of three. A change on a Shapeshifter acts on its current donated text (613.8), so
"black" to "green" on a Shapeshifter with Akroma's text gives protection from green and red;
when the top becomes Karmic Guide it has protection from green; with a noncreature top it has
no color word. A change made to a Shapeshifter spell carries over as it enters (400.7a). The
test for this lands with whichever of B2 and C2 merges second.

## 5. Tests that gate Full

### 5.1 Volrath's Shapeshifter

- [ ] One `full_text_v1/registered.rs` style fixture per new donor, both seats, real entry or
      attack, paid costs, snapshot restore and donor departure: Auramancer (return changes the
      top), Monk Realist (the destroyed enchantment can become the top), Uktabi Orangutan,
      Ghitu Slinger entry, Ghitu Slinger echo paid, declined and unpayable with the donor
      departed, Xantid Swarm's attack trigger. Model:
      `volrath_native_paid_rats_discards_for_the_opponent_unless_suppressed_both_seats`.
- [ ] Meddling Mage donor through a native Legacy's Allure transfer: names nothing, prohibits
      nothing, both seats.
- [ ] Legend rule: a Shapeshifter with Akroma's (or Squee's) text and the physical card under
      one controller.
- [ ] Refusal fixtures, each asserting `Decision::Halted { mechanic:
      FullTextDonorOutsideSetV1, source: host }` and no later decision, both seats: entry with
      an unsupported top; a single arrival; an ordered batch; a departure exposing an older
      card; a control change onto a graveyard with an unsupported top; phase-in; Meddling Mage
      text at entry. The Masticore branch of
      `volrath_native_registration_retains_paid_discard_with_no_or_unsupported_text_both_seats`
      becomes a refusal case.
- [ ] The frozen tests of section 3.1 pass unchanged.
- [ ] Registered owner: a twin of the hidden-world privacy test; step, snapshot, restore and
      stale tokens; shroud and Meddling Mage names in its scene; halt reporting.
- [ ] Registration: the ordered profile's admissions and refusals, the profile mismatch, and
      the schema 5 preflight refusal of a Shapeshifter deck.
- [ ] Games: FEB in `PLAYABLE` (both pairs, ranked and random, terminal and replay) and in
      `PLAYABLE_75` (both matches, random sideboarding, every game terminal and replayed).
- [ ] CI: the Eternal library filter (`.github/workflows/ci.yml`, the
      `run_rust_lib_filters.py` line with `full_text_v1::tests:: graveyard_order_v1::tests::
      premodern_ordered_public_v1::`) and the `eternal_integration` suite; hosted Windows
      Eternal job at the pull request head.
- [ ] Registry row `partial` to `full`; `expected_capability` in
      `tests/eternal/eternal_catalog_v1.rs` drops the Shapeshifter; `feb.rs::mainboard`
      accepts Full; catalog hash re-pinned.

### 5.2 Alter Reality

- [ ] Rules: a spell target and a permanent target; the full 20-pair menu; a target without
      the word (recorded, no effect); flashback; 400.7a carry-over; two changes in timestamp
      order; zone change ends it; phasing, control change and face-up keep it; an ability
      already on the stack keeps its captured word; illegal target at resolution.
- [ ] One test per site row of section 4.4 (rows sharing a function may share a test, each row
      asserted): at least Light of Day with two instances of different colors, Sphere of Law,
      both Elemental Blasts fizzling, Pyroblast doing nothing, the protection masks, Absolute
      Law per instance, Anarchy on the stack, Gloom's two taxes, Chill, Warmth, Dystopia,
      Circle of Protection: Red with a non-red choice, Quirion Dryad, Plaguebearer, Snuff Out,
      a token-making spell, a token-making trigger, a token-making activation, Treetop
      Village and Ghitu Encampment.
- [ ] The five cost-only cards and a sample of no-word cards record a change with no effect.
- [ ] Refusal: a target outside the audit (Circle of Protection: Blue) halts with
      `TextColorChangeOutsideAuditV1`; a face-down target is accepted.
- [ ] The audit table test: every snapshot card is listed or audited as having none.
- [ ] Schema 5: the field's presence, order and absence; the `choose_text_color_word_v1`
      actions; schema 4 and the flat encoders refuse; section 4.6's tests.
- [ ] Volrath interaction (section 4.7).
- [ ] Games: a one-copy sideboard card is cast too rarely, so, as S1k did, a blue-white deck
      with four Alter Realities in its mainboard plays a deck of color-word cards through
      schema 5 sessions in both seats with ranked and seeded random actions, terminal and
      replay, requiring changes chosen from the action list; then `premodern-replenish` joins
      `PLAYABLE_75`.

## 6. Census

Tool and command (input tooling only, no build):

```text
python python/tools/eternal_meta_census_v1.py data/eternal_v1/meta_snapshot_mtgtop8_premodern_20261101.json --markdown
```

On main at `fd9ff09a` it reports 303 of 305 distinct cards Full, a playable share of 87.0% of
the snapshot's 90.0%, and full 75s for 81.0%. Running the tool's `census` function with the
two cards counted as Full gives:

| State | Full cards | Playable share | Full 75 share |
| --- | ---: | ---: | ---: |
| Main today | 303 | 87.0% | 81.0% |
| Shapeshifter Full | 304 | 90.0% | 84.0% |
| Alter Reality Full | 304 | 87.0% | 87.0% |
| Both | 305 | 90.0% | 90.0% |

By deck: `premodern-full-english-breakfast` (3.0%) gains both its mainboard and its full 75
with the Shapeshifter (its four other sideboard cards are Full since P3h);
`premodern-replenish` (6.0%) gains its full 75 with Alter Reality (its mainboard is already
playable). Every other Premodern deck is already playable with a full 75. The session lists
follow: `PLAYABLE` goes from 23 to 24 Premodern decks, `PLAYABLE_75` from 22 to 24.

## 7. Pull request plan

Each step is reviewable alone. Steps that change a registry row take one slot on the
catalog-hash chain, because they re-pin `card_db_hash_eternal_v1_is_frozen`; the others take
none. All of it waits behind the chain steps and issue #657's pieces.

1. **A: Volrath donor scope.** Files: `full_text_v1.rs` (`SUPPORTED_DONORS_V1`,
   `refused_donor`, the new kinds and programs), `premodern_ordered_public_v1.rs` (program
   meanings), `eternal_premodern_p2d_v1.rs` (donated echo cost), `engine.rs` (the appended
   mechanic and the backstop), `event.rs`, `graveyard_order_v1.rs`, `eternal_e5a_v1/phasing.rs`
   (check sites), `full_text_v1/registered.rs` and `tests.rs`, `volrath_full_text_v1.md`.
   No registry row; no chain slot. Acceptance: section 5.1's donor and refusal fixtures and
   the frozen tests pass on CI and the hosted Windows Eternal job.
2. **B1: registered ordered owner.** Files: `premodern_ordered_public_v1/feb.rs` and
   `decision.rs`, `premodern_ordered_public_v1.rs` (registered scene), `rl.rs` (text mode),
   `eternal_bo3_v1.rs` (profile, errors, `start_game_with_ordered_graveyard_v1`), `card_def.rs`
   (preflight variant), `tests/eternal/eternal_bo3_v1/eternal_bo3_ordered_v1.rs` (driver),
   owner tests. The Shapeshifter stays Partial, so the owner's own tests register it through a
   test-only admission of the frozen FEB 75 (as the frozen owner admits a Partial card today).
   No row; no chain slot. Acceptance: the registered-owner, registration and frozen tests of
   section 5.1; schema 5 evidence of section 3.7; registered FEB matches against Psychatog
   (Dominate) and against a test deck with Legacy's Allure and Meddling Mage play to terminal
   and replay. Replenish's own 75 cannot register before C2, since Alter Reality is missing.
3. **B2: Shapeshifter Full.** Files: `data/eternal_v1/cards_v1.json` (row), `feb.rs` (the
   capability line), `card_def.rs` (re-pin), `tests/eternal/eternal_catalog_v1.rs` (`PLAYABLE`,
   `expected_capability`, routing), `tests/eternal/eternal_bo3_v1.rs` (`PLAYABLE_75`, routing),
   `eternal_formats_support_v1.md` (a pointer to this document and the census), this document's
   status. One chain slot. Acceptance: section 5.1 complete; census as in section 6.
4. **C1: color-word record and sites.** Files: `text_color_change_v1.rs`, `state.rs` (field,
   hash, serde), the 16 module families of section 4.4, `rl.rs` (field, semantic, refusals),
   `effect.rs` (choice purpose). No card, no row, no chain slot; tests write records directly.
   Acceptance: section 5.2's site, refusal and condition tests.
5. **C2: Alter Reality Full.** Files: `build_eternal_p3i_v1.rs` and
   `eternal_premodern_p3i_v1.rs` (wave P3i), the row, `card_def.rs` (re-pin),
   `tests/eternal/eternal_premodern_p3i_v1.rs`, `tests/eternal/eternal_bo3_v1.rs`
   (`PLAYABLE_75`), `eternal_formats_support_v1.md`. One chain slot. Acceptance: section 5.2
   complete, the transcript evidence of section 4.6, census as in section 6.

C1 and C2 are independent of A, B1 and B2 except for the interaction test of section 4.7,
which lands with whichever of B2 and C2 merges second. Issue #460 closes when B2 and C2 are
both merged and the census on main shows 305 of 305 Full.

## 8. Risks and limits after both cards are Full

- The refusals are real: a custom registration with a control effect and a creature outside
  the donor set, a Shapeshifter entering with Meddling Mage's text, or Alter Reality on a card
  outside the 305 halts the game. Snapshot games never reach them.
- The registered owner's Human view labels only the meanings its renderer knows; the other
  rows stay `unsupported`, and qualification uses numeric ordinals.
- The registered owner is in process only unless section 9, question 4 is answered otherwise.
- Flat encoders, search, infoset export and earlier observation versions still refuse these
  games, so they are not training-ready inputs.
- The observation shows Alter Reality's change once it is recorded, not during the two-stage
  choice beyond the actions themselves, as S1k shows Mage names only once entered.
- The 38 sites are hand-routed; a later card with a color word needs its own site and audit
  row. The audit test catches a new Premodern snapshot card but not a new site in an existing
  card's code.
- The donor check runs at every commit tail; its cost is one battlefield scan, measured in
  the A pull request.

## 9. Open questions for the lead

1. Mogg Fanatic and Ravenous Rats have qualified donated programs but are not in the ruled
   set, and no snapshot deck can put them under a Shapeshifter. Proposed: keep them supported
   (refusing them removes tested paths for no gain).
2. "Registered Premodern deck" admits any Full card today (`EternalFormatV1::copy_limit`'s
   doc: format legality is not checked). Proposed: the ordered profile also requires every
   card to be in the 305-card Premodern pool, with a typed registration error. That makes copy
   effects and cross-format donors impossible by construction; the runtime refusal stays.
3. The schema 5 preflight gains `RequiresOrderedGraveyardOwnerV1` (the S1i Leyline
   precedent); it only renames the refusal a Shapeshifter deck already gets. Proposed: accept
   it as consistent with "schema 5 untouched".
4. A JSONL route for registered games. Proposed: not part of Full; the in-process owner and
   driver are the qualifying consumer, as S1i's native best-of-three games are; a follow-up
   issue adds a `reset_registered_v1` request beside the frozen reset.
5. The routing rule reads "ordered-graveyard reader" as the full-text reader only. Proposed:
   yes, since no Premodern card has Delve and Delve games keep schema 5.
6. The registered profile turns on Foundations combat, London mulligans and the public shroud
   flag to match the other decks' catalog games, and qualifies on numeric submissions.
   Proposed: yes to both.

## 10. How the research note was checked

The note was written at `111ed7ba`. Between that commit and `fd9ff09a`, `full_text_v1.rs`,
its tests, `graveyard_order_v1.rs` and the FEB owner files did not change;
`premodern_ordered_public_v1.rs` changed only to read `CardType::ALL`; `card_def.rs` and
`engine.rs` moved by the Kindred and Powerstone work. Every file, function and count repeated
here was re-read on `fd9ff09a`. Corrections:

- Crimson Acolyte's grant is protection from red, not green.
- `full_text_v1/tests.rs` holds 29 tests, not 22 (`registered.rs` holds 11).
- CI runs the Eternal library filter at `.github/workflows/ci.yml` line 564 and the four
  native producers at lines 517 to 531 (the note had 510 and 468 to 499).
- The catalog harness is `tests/eternal/eternal_catalog_v1.rs` (`play_observed` at line 854,
  `PLAYABLE` with 137 entries at line 22), not a source file.
- `card_def.rs` lines moved: the Blast target specs are at 1714 to 1733, `NonblackCreature`
  at 1827, the Blast recipes at 4841 to 4883; `engine::object_color_mask` is at 23801.
- A Phyrexian Metamorph copy sets the object's `card_def` to the copied definition
  (`eternal_vintage_v1::enter_as_copy`), so a copy of a Shapeshifter would be a full-text
  host; the gap is an untested path, not a missing ability, and no Premodern card copies a
  permanent.
- Dominate is in Psychatog's sideboard only; Legacy's Allure is in Replenish's mainboard.
- The site count is 38 in 16 module families, not about 30 in 10.
- Not in the note: `feb.rs::mainboard` requires the Shapeshifter to be Partial, and
  `expected_capability` pins it Partial; both change with the flip. The frozen scene's text
  mode refuses shroud and Meddling Mage names, and the frozen owner plays without Foundations
  combat. Once the card is Full, the schema 5 preflight would admit FEB and the session would
  refuse it mid-game without the new preflight variant. `EternalStateV1` derives `Hash`, so
  the color record cannot live there without moving state hashes.
