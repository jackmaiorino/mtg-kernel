# Volrath's Shapeshifter Full and Alter Reality

Status: decided design, issue #460. The research lead ruled on its three contract points and
on the eight questions it raised (section 9). This document changes no source, test or data
file. It specifies the work that makes the last two Premodern cards Full, for an implementer
to build from.

Code references are to main at `fd9ff09a`. Line numbers drift; each reference also names
the function or constant. Rules are cited from the
[November 6, 2026 Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt),
as in `volrath_full_text_v1.md`.

## 1. Status and scope

Issue #460 stays open until Volrath's Shapeshifter (registered `partial`, catalog ID 1200,
`premodern-full-english-breakfast` mainboard, 3.0% of the snapshot field) and Alter Reality
(no registry row, one copy in the `premodern-replenish` sideboard, 6.0%) are Full. Both need
a public contract extension. The lead's rulings on the three contract points:

1. Donor scope: "Donor scope as proposed, with a typed refusal for any other donor at the
   moment it would become the top card. Refused beats wrong." The proposal was the Full
   English Breakfast (FEB) mirror donors, the five FEB sideboard creatures (Auramancer, Monk
   Realist, Uktabi Orangutan, Ghitu Slinger, Xantid Swarm) and the two Premodern control
   effects (Dominate, Legacy's Allure). Amended on this design's first question: "Mogg
   Fanatic and Ravenous Rats stay donors. The rule is 'every card with a donated program
   today, plus the listed additions; any other donor is refused with the typed error'.
   Nothing that plays correctly today is refused."
2. Public consumer: "Widen `premodern_ordered_public_v1::feb` to any two registered Premodern
   decks with sideboards and best of three; schema 5 untouched. No schema successor."
3. Alter Reality: "the per-object color-word record, the bounded audited table with refusal
   outside it, and one schema 5 observation field by the S1k precedent, on condition that
   every pinned golden of a game without the card is byte-identical."

Priority, as ruled: behind the chain steps and behind issue #657's pieces; this work takes no
hosted runner from the chain. Steps B2 and C2 take chain slots after S1o and C5 (section 7).

This document specifies what the rulings leave to design: the supported donor table, the
refusal's name, check sites and reachability argument (section 2); the registered owner's
identity, registration, routing and refusals, and the one schema 5 message that changes
(section 3); the color-word record, the complete audited table of the 305 Premodern cards and
the schema 5 field (section 4); the tests that gate Full (section 5); the census effect
(section 6); the pull request plan (section 7); and what remains after both cards are Full
(section 8). Section 9 records the lead's rulings on the eight questions this design raised
and where each is implemented. Appendix A classifies every creature card of the Premodern pool
as a donor. Section 10 records how the research note this work started
from was checked against main.

Out of scope: a schema successor or any change to schema 5 beyond Alter Reality's field, its
action meaning and the one refusal message of section 3.7; donors outside the supported set;
copies of the Shapeshifter (no card in the Premodern pool copies a permanent); a general
text-replacement layer for the whole Eternal and Standard catalog; flat policy encoders,
search, infoset export and the Spellbench bridge for these games (they keep refusing);
training.

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
  (Akroma: black and red; Karmic Guide: black), `eternal_premodern_p3c_v1/statics.rs::protection_mask`
  (Ihsan's Shade: white; Iridescent Angel: each color) and
  `eternal_premodern_p2c_v1/statics.rs::apply_entry_counters` (Triskelion three, Phantom
  Nishoba seven, Spike Feeder two) read `active_definition`. Meddling Mage's entry choice
  (`eternal_sideboard_s1b_v1`) does not read full text.

### 2.2 The supported donor set

A donor is a creature card that can be the top card of the graveyard of the player who
controls a face-up Shapeshifter. Amended ruling 1 has two sentences, and the set applies both:
"every card with a donated program today, plus the listed additions; any other donor is refused
with the typed error", and "Nothing that plays correctly today is refused". So the supported
set is what plays correctly today plus the listed additions, and only what would play wrongly
is refused.

What a Shapeshifter has today with any creature card on top: the card's name, card types,
subtypes, supertypes, color, mana value, base power and toughness and every keyword in its
`CardDef.keywords`, through `full_text_v1::active_definition` and the engine's characteristic
queries (`engine.rs`: `effective_name`, `object_has_type`, `unpainted_object_color_mask`,
`object_mana_value`, `effective_supertypes`, the base power and toughness queries,
`effective_keywords_unchecked`, `has_effective_keyword`, `effective_subtype_ids`), and it keeps
"{2}: Discard a card". Any other ability works only through a donated program (section 2.1).
Name-keyed statics outside those programs read the physical card
(`eternal_legacy_e9a_v1::active_named`), so they do not reach a Shapeshifter.

The donors fall into four classes, recognized by four named, audited constants in
`full_text_v1`, never by a guess from a definition: `DONATED_PROGRAM_DONORS_V1` (part A),
`LISTED_DONORS_V1` (part B), `GENERIC_TEXT_DONORS_V1` (part C) and `ENTRY_ONLY_DONORS_V1`
(part D). `donor_supported(definition)` is true for A, B, C and D; `entry_refused(definition)`
is true for D and for Meddling Mage. Appendix A lists the class of every one of the 101
creature cards in the Premodern pool; with registrations limited to that pool (section 3.3) no
other creature card can be a donor in a registered game.

Phantom Nishoba and Spike Feeder have only part of their text donated: they get their entry
counters from `apply_entry_counters`, but Nishoba's damage life gain and damage-prevention
replacement (keyed by `p2c::statics::is_nishoba`, which reads the physical name) and Spike
Feeder's two activated abilities (absent from `public_program`) are not donated. As donors today
they lose those abilities, so they do not play correctly and are refused. Exalted Angel is
refused for the same reason: its "Whenever this creature deals damage, you gain that much life"
is keyed by the physical name (`eternal_premodern_p1a_v1.rs`), so a Shapeshifter with its text
has flying but no life gain.

Cards with a donated program on main today (part A):

| Donor | What it donates | Program today, where it lives |
| --- | --- | --- |
| Akroma, Angel of Wrath | flying, first strike, vigilance, trample, haste; protection from black and from red; legendary | keywords through `active_definition`; protection in `eternal_premodern_p2c_v1/statics.rs::protection_mask` |
| Battlefield Scrounger | threshold activation, once each turn | `FullTextAbilityV1::public_program` `ScroungerThresholdPump` (`full_text_v1.rs`) |
| Birds of Paradise | flying; mana | `full_text_v1::mana_text` |
| Hermit Druid | activation | `public_program` `HermitDruidRevealThroughBasic` |
| Iridescent Angel | flying; protection from each color | `eternal_premodern_p3c_v1/statics.rs::protection_mask` (full-text branch) |
| Ihsan's Shade | protection from white | same function |
| Karmic Guide | flying; protection from black; echo; entry return | `DonatedTriggerKindV1` entry kinds and `KarmicGuideEcho` (`full_text_v1.rs`); protection in `p2c::statics::protection_mask` |
| Mogg Fanatic | sacrifice for damage | `public_program` `MoggFanaticSacrificeForDamage` |
| Palinchron | flying; entry untap; return-to-hand activation | entry kinds; `public_program` `PalinchronReturnHost` |
| Phyrexian Devourer | power-7 sacrifice; exile-top activation | `DevourerPower` (queued from `eternal_premodern_p2c_v1.rs`); `public_program` `DevourerExileTopForCounters` |
| Psychatog | two activations | `public_program` `PsychatogDiscardPump`, `PsychatogExilePump` |
| Ravenous Rats | entry: target opponent discards | `DonatedTriggerKindV1::RavenousRatsEntered` and its batch twin |
| Triskelion | enters with three counters; activation; artifact type | `p2c::statics::apply_entry_counters`; `public_program` `TriskelionRemoveCounterForDamage` |
| Volrath's Shapeshifter | its own text: retained discard only | `FullTextAbilityV1::RetainedDiscard`, slot 0 |
| Wall of Roots | defender; mana once each turn | `mana_text`, `FullTextRestrictedUseV1` |

The listed additions (part B):

| Donor | Why it is listed | What it donates | Work |
| --- | --- | --- | --- |
| Elvish Spirit Guide | FEB mirror donor | nothing on the battlefield (its ability works from hand; `FullTextAbilityV1::resolve` keeps battlefield abilities only) | none |
| Squee, Goblin Nabob | FEB mirror donor | nothing on the battlefield (graveyard ability); legendary | a legend-rule test (section 5.1) |
| Auramancer | FEB sideboard | entry: may return target enchantment card from its controller's graveyard | new entry kind over `eternal_premodern_p3h_v1::auramancer_enters` |
| Monk Realist | FEB sideboard | entry: destroy target enchantment | new entry kind over `p3h::monk_realist_enters` |
| Uktabi Orangutan | FEB sideboard | entry: destroy target artifact | new entry kind over `eternal_premodern_p3c_v1` `UKTABI_ORANGUTAN_TRIGGERS` |
| Ghitu Slinger | FEB sideboard | echo {2}{R}; entry: 2 damage to any target | new entry kind over `p3h::ghitu_slinger_enters`; a donated echo through `p2d::echo_cost` |
| Xantid Swarm | FEB sideboard | flying; attack trigger: defending player can't cast spells this turn | new attack kind over `eternal_premodern_p3b_v1` `XANTID_SWARM_TRIGGERS` |
| Meddling Mage | reached through Legacy's Allure (confirmed reading of "the two Premodern control effects") | its "as enters" name choice, which never happens for a Shapeshifter already on the battlefield | a test; refusal on entry (below) |

Cards that need no donated program, because their whole battlefield text is generic (part C).
The rule: every ability the card has on the battlefield is a keyword in its `CardDef.keywords`,
and every other ability functions only from the hand, from the graveyard, while the card is
face down, or as it is turned face up. A Shapeshifter with such a card on top has exactly the
card's battlefield text through the generic queries, and nothing is lost; the 2004-10-04
Volrath's Shapeshifter ruling also notes that the "enters" abilities of the card it copies do
not trigger when it changes forms.

| Donor | Battlefield-relevant Oracle text | Why nothing is lost |
| --- | --- | --- |
| Anger | Haste | haste is in `CardDef.keywords` (`build_eternal_e9a_v1.rs`); its other ability works only while the card is in the graveyard |
| Arrogant Wurm | Trample | trample is a keyword (`build_eternal_p2b_v1.rs`); madness works from the hand |
| Eternal Dragon | Flying | flying is a keyword (`build_eternal_p2a_v1.rs`); its return works from the graveyard and Plainscycling from the hand |
| Glory | Flying | flying is a keyword (`build_eternal_p2b_v1.rs`); its protection ability can be activated only while the card is in the graveyard |
| Krosan Tusker | none | cycling and its cycling trigger work from the hand |
| Gempalm Incinerator | none | cycling and its cycling trigger work from the hand |
| Genesis | none | its upkeep ability works only while the card is in the graveyard |
| Nether Spirit | none | its upkeep return works only while the card is in the graveyard |
| Thornscape Battlemage | two enters triggers, each "if it was kicked" | kicker is paid while casting the card itself; a Shapeshifter is never cast with this text (full text applies only on the battlefield), so it is never kicked and both intervening-if triggers do nothing, entering or not |
| Bane of the Living | none face up | morph and "When this creature is turned face up" need the permanent to be turned face up; `top_creature_text` reads only a face-up host, a face-up Shapeshifter cannot be turned face up, and no card in the Premodern pool turns a permanent face down |
| Nantuko Vigilante | none face up | the same as Bane of the Living |

Cards accepted as a top card except at entry (part D). The rule: every battlefield ability is a
keyword, except abilities that work only as the permanent enters (an enters trigger, an "as
enters" or "enters with" effect), and the card has no donated program for them and no
leaves-the-battlefield half. Such a card plays correctly when it becomes the top card while
the Shapeshifter is already on the battlefield (the entry has passed), and wrongly only when a
Shapeshifter enters with it on top (the enters ability would be lost). So it is accepted as a
top card for a Shapeshifter already on the battlefield, and the typed refusal fires at the
moment a Shapeshifter would enter with it on top (section 2.3, the entry check):

| Donor | Battlefield-relevant Oracle text | Lost only at entry |
| --- | --- | --- |
| Wall of Blossoms | Defender; when this creature enters, draw a card | the draw |
| Goblin Matron | When this creature enters, you may search for a Goblin card | the search |
| Goblin Ringleader | Haste; when this creature enters, reveal the top four cards | the reveal |
| Phyrexian Dreadnought | Trample; when this creature enters, sacrifice it unless you sacrifice creatures with total power 12 or greater | the sacrifice |
| Caller of the Claw | Flash; when this creature enters, create Bears | the tokens (flash works from the hand) |

Meddling Mage (part B) follows the same rule: accepted on a Shapeshifter already on the
battlefield, refused at entry. Cards whose enters ability has another half on the battlefield
are not in part D and are refused in both cases: Petradon (its leaves-the-battlefield return
and its pump), the echo creatures (Avalanche Riders, Deranged Hermit, Multani's Acolyte,
Yavimaya Granger: echo triggers at upkeep while the permanent is on the battlefield), Blastoderm
(fading removes counters at upkeep) and Siege-Gang Commander (its activated ability).

Dominate reaches Psychatog, already in part A. Every FEB mirror donor is in A or B: the 13
creature cards of the FEB mainboard are Akroma, Battlefield Scrounger, Birds of Paradise,
Elvish Spirit Guide, Hermit Druid, Karmic Guide, Palinchron, Phyrexian Devourer, Psychatog,
Squee, Triskelion, Volrath's Shapeshifter and Wall of Roots, and each is complete today.
The existing fixtures of Mogg Fanatic and Ravenous Rats in `full_text_v1/registered.rs`
(`volrath_native_paid_fanatic_uses_original_damage_source_both_seats`,
`volrath_native_paid_rats_discards_for_the_opponent_unless_suppressed_both_seats`,
`volrath_native_rats_uses_final_batch_text_after_donor_departure_both_seats`) stay as they are.

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
- Donated echo: Karmic Guide keeps `KarmicGuideEcho { upkeep, control }`, its program
  meaning `FullTextProgramV1::KarmicGuideEcho` and `P2dOpV1::DonatedGuideEchoPayV1`, so the
  frozen mirror's states and views do not change. An appended `DonatedEcho { upkeep, control }`
  kind, with an appended pay op, is queued by `queue_upkeep_triggers` only for other donors
  with an echo cost (Ghitu Slinger). Its cost comes from the captured donor definition through
  `p2d::echo_cost`, so a departed Slinger donor still costs {2}{R} (113.7a, 702.30a).
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

`donor_supported` is true for part D and Meddling Mage, so `refused_donor` accepts them on a
Shapeshifter already on the battlefield. The entry case is separate, and the backstop below
cannot see it, because it reads states, not events; it relies on `record_entry_trigger`, which
`event.rs` reaches for every entry: at the moment a Shapeshifter enters,
the check refuses it when its text at entry (`entry_refused`) is a part D card or Meddling
Mage. The engine tells the two moments apart, because an entry is a distinct event
(`full_text_v1::entry_source`: a zone change to the battlefield from elsewhere, or a token
creation) while a change of top card, a control change or a phase-in is not an entry. The entry
check sits where the entry's text is fixed: in `event.rs::record_entry_trigger` after
`active_text` (which, inside a batch, already reads the batch's final top for an entering host
through `batch_text_top`), and, for an entry whose donor arrives later in the same ordered
batch (`quoting_entry_scene`), in `graveyard_order_v1` where the batch's final text is selected
and `record_batch_entry_trigger` runs. No part D card has a leaves-the-battlefield half, so the
split is sound for every card in the list.

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

Backstop: `refused_donor` runs first in `engine::advance_until_decision`, before its
`graveyard_order_v1::owns_operation` branch, so it covers decisions returned by
`graveyard_order_v1::advance` (a pending owner order) as well as the historical path. Placed
inside `advance_until_decision_historical` it would miss the pending-order decision, which
`advance` returns after `validate_pending` alone. At that boundary the check is harmless: the
visible state is the checkpoint taken before the batch commits (the module doc of
`graveyard_order_v1.rs`: only the checkpoint at the order boundary becomes visible), and
`batch_text_top` returns the scene's `before` top while a runner is live, a top that was
already audited. When a pending order is restored without a runner, `batch_text_top` returns
`None` and `graveyard_text_top` reads the physical graveyard list of that checkpoint, again
the pre-batch top. The final top is audited when the order completes. A test restores a
pending order with an unsupported donor arriving in the batch and checks that the order menu
is offered and the halt fires only after the order completes. The backstop guarantees that no
decision is ever offered while an unsupported donor is in place, whatever path changed the
top; the event sites above make the halt fire at the event that caused it. The scan costs one
battlefield pass and returns at once when no Shapeshifter is on the battlefield.

What a session reports when it fires: the frozen FEB owner returns
`FebCurrentV1::NativeHalt` and its JSONL route `{"state_type": "native_halt"}`
(`feb_jsonl.rs`), both unchanged. A registered owner returns an appended
`FebCurrentV1::RegisteredHaltV1 { mechanic: UnsupportedMechanic, source: Option<CardStableRefV1> }`,
filled from `state.engine.halted` (the source's public card reference when the source is a
public object, otherwise `None`); the frozen owner never returns it, and the private
`decision::BoundBoundaryV1::Halted` keeps its shape. A halted game is never a completed game:
no terminal is reported, the best-of-three driver records no game result, and every harness
that sees a halt fails the game. Schema 5 never meets this halt: its reset refuses a
Shapeshifter in either seat's mainboard or cards outside the game (section 3.7), so no schema 5
game holds one.

Routes by which a card that was never in a mainboard reaches play. Checked for the Shapeshifter
and for donors outside the set, in schema 5 and in the registered owner:

- Cards outside the game. The catalog's fetch effects (`eternal_outside_game_v1::OutsideGameOpV1`)
  are Burning Wish (sorcery cards), Karn, the Great Creator's -2 (artifact cards), learn (Lesson
  cards) and Extrapolate the Impossible (any two cards with different names; `eternal_e5a_v1`).
  Only Extrapolate can fetch a creature card, so it is the one route for a Shapeshifter or an
  unsupported donor: fetched to hand, a creature card can be cast or discarded into its owner's
  graveyard. Schema 5 receives cards outside the game at reset and checks them only with the
  row check (`limited_session_v1.rs`, near line 212); `RlEpisodeSessionV1`'s reset runs the
  preflight over the two mainboards only (`rl_session.rs`, near lines 6090 to 6093). The design
  therefore adds the same Shapeshifter check over the outside-the-game cards (section 3.7). None
  of the four fetch cards is in the Premodern pool, so no snapshot deck reaches this route.
- Sideboard games of best of three. Each later game is a configuration of the registered 75.
  `EternalRegisteredDeckV1::register` checks both registered zones, so a Shapeshifter in a
  sideboard needs the ordered profile (section 3.3), and a schema 5 game's mainboard
  configuration passes the reset preflight like any mainboard.
- Companions. A companion must be a supported companion card whose condition the mainboard meets
  (`eternal_lurrus_v1::companion_condition_met`, `CustomDeckV1::resolve_with_companion`); a
  Shapeshifter or a creature donor that is not a companion is refused.
- Tokens that copy a card. A token copy of a Shapeshifter needs a Shapeshifter on the
  battlefield first; tokens are never donors (`graveyard_text_top` skips them).
- No other effect in the catalog creates a card in a hand, library or graveyard from nothing.
- In the registered owner every registered card is in the 305-card Premodern pool (section 3.3),
  which holds none of the four fetch cards, so no card reaches play there from outside the game.

So a Shapeshifter reaches no schema 5 game. Unsupported donors can still reach a
graveyard through Extrapolate, but only matter under a Shapeshifter, which is then absent from
schema 5, and in the registered owner the donor refusal halts at that moment.

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
a game between snapshot decks. Because registrations are limited to the Premodern pool, the
refusable donors are the 62 refused creature cards of appendix A (for example Masticore,
Phantom Nishoba, Spike Feeder or Exalted Angel in a custom deck that also plays Dominate or
Legacy's Allure), and the part D cards only when a Shapeshifter enters with one on top.

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
  Mage names (`meddling_mage_entry::requires_successor`, which scans the event history, so any
  state after a Mage has entered is refused), so Enchantress (Argothian Enchantress, Sterling
  Grove), White Weenie (True Believer) and the Mage decks would end as `UnsupportedDecision` in
  a widened game without it. The gates that today test `text_mode ==
  ObservationTextModeV2::PublicShroudV5Artifact` each take `PublicShroudV5Artifact |
  PremodernOrderedRegisteredV1Artifact`: the `public_shroud` field and the
  `meddling_mage_names` field of the projection, the `refuse_unrepresented_shroud_in_mode`
  calls, and the Mage refusal choice (`requires_copy_successor` instead of
  `requires_successor`); after section 4, the `text_color_changes` field too. The
  `full_text_v1::refuse_legacy` skip, which today tests `!= PremodernOrderedV1Artifact`, skips
  for both ordered modes. Schema 5 output is unchanged because its own mode takes the same
  branch as before at every gate.
- An observe entry for the registered profile, `rl::observe_premodern_ordered_registered_v1`,
  beside `rl::observe_premodern_ordered_v1` (which stays fixed to `PremodernOrderedV1Artifact`
  for the frozen owner). The owner records its profile, and `decision::bind_next` and
  `decision::view` call the observe entry of that profile. The legal actions need no work: the
  owner's menu comes from `rl::legal_action_candidates_v1`, which already maps
  `Decision::ChooseMeddlingMageNameV1` to `ActionSemanticV1::ChooseMeddlingMageNameV1`
  answered by `ChooseEffectOption` (`rl.rs`, the `ChooseMeddlingMageNameV1` arm near line 7394).

### 3.3 Registration and validation

`eternal_bo3_v1` gains an ordered-graveyard registration profile beside the native opening
one, and the S1i pattern carries over:

- `EternalRegisteredDeckV1::new_with_ordered_graveyard_v1(format, deck_id, mainboard,
  sideboard, companion)`. It requires `EternalFormatV1::Premodern`, every card Full and not a
  token, the usual bounds and copy limit, no companion (the Premodern snapshot has none) and
  no native-opening card, and every card in the 305-card Premodern snapshot pool (confirmed,
  section 9, question 4), with an appended `EternalBo3ErrorV1::OutsidePremodernPoolV1 { name }`.
  This makes copy effects, outside-the-game fetchers and cross-format donors impossible in a
  registered game by construction; the runtime donor refusal stays.
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
submission (confirmed, section 9, question 7).

### 3.6 Routing and refusals

Routing rule: a catalog game is played by the registered owner exactly when one of its decks
holds a card for which `full_text_v1::owns_definition` holds, that is a Volrath's
Shapeshifter. The ruling's "ordered-graveyard reader" (`engine::graveyard_order_readers_present`)
also covers Delve, but no card in the Premodern pool has Delve (Gurmag Angler, Murktide
Regent, Treasure Cruise, Dig Through Time and Hogaak are the registry's Delve cards), so Delve
games stay on schema 5 as today (confirmed, section 9, question 6).

- `PLAYABLE` (`tests/eternal/eternal_catalog_v1.rs`, 137 entries) gains
  `premodern-full-english-breakfast`. Its position in that list does not matter:
  `playable_snapshot_decks_finish_and_replay_through_public_sessions` filters
  `snapshot_decks()` by the list, pairs each deck with the next in snapshot order and seeds
  each pair with `0x4554_4552_0000 + index`. The new pairs are Replenish against FEB and FEB
  against Stasis; Replenish against Stasis goes away; Stasis against Reanimator and the
  wrap-around pair Reanimator against `pauper-mono-red-madness` move to new seeds (the
  Premodern snapshot is the last of `SNAPSHOTS`). A pair with a Shapeshifter registers both
  snapshot 75s with the ordered profile and plays game one only, with ranked and seeded random
  policies, through a new in-process driver
  (`tests/eternal/eternal_bo3_v1/eternal_bo3_ordered_v1.rs`), and must reach a natural terminal
  and replay to the same transcript and state hash. In B2 Replenish's 75 cannot register yet
  if C2 has not landed (Alter Reality is missing); the pair registers Replenish's mainboard
  with an empty sideboard for game one, which is all `PLAYABLE` plays.
- `PLAYABLE_75` (`tests/eternal/eternal_bo3_v1.rs`, 124 entries) gains FEB (and Replenish,
  section 4). Matches pair in snapshot order within the format, so once both cards are Full
  the new pairs are Replenish against FEB and FEB against Stasis; if the Shapeshifter is Full
  first, they are Enchantress against FEB and FEB against Stasis. The match seed counter of
  `playable_75_decks_play_best_of_three_with_random_sideboarding` runs across formats in
  `BTreeMap` order (legacy, pauper, premodern, vintage), so every later Premodern match and
  every Vintage match moves to a new seed. A match where either deck holds a Shapeshifter
  registers both decks with the ordered profile and plays every game through the same driver.
- Schema 5 refuses a Shapeshifter deck at reset (section 3.7).

Refusals of the registered owner: a registration outside section 3.3 (typed
`EternalBo3ErrorV1`); a mismatched profile; a state its text mode cannot represent
(`FebCurrentV1::UnsupportedDecision`, as today); a native halt, including the donor refusal
(`FebCurrentV1::RegisteredHaltV1` with mechanic and source, section 2.3); a foreign or stale
decision token or snapshot (as today).

### 3.7 Schema 5 is untouched, with one refusal message

How schema 5 meets a Shapeshifter deck. A schema 5 reset (`LimitedJsonlServerV1`, the
`reset` arm in `limited_session_v1.rs`) checks each row itself (`card.is_token ||
!card.has_full_support()`, near line 148, and near line 212 for cards outside the game), then
builds the session with
`RlEpisodeSessionV1::reset_with_custom_decks_companions_london_and_shroud_v1`
(`rl_session.rs` near line 6076), which calls `card_def::preflight_fully_supported_deck` for
both mainboards (near line 6091) and maps any error to `RlSessionErrorCode::UnsupportedDeck`.
That preflight is what refuses a Leyline deck at reset today (`RequiresNativeOpening`). Today
FEB is refused by the row check, because the Shapeshifter is Partial. Once it is Full (B2), both
checks would pass, so the design adds one refusal (section 9, question 3, ruled: "A deck that
can never complete a game is refused before the game, on condition that no input accepted
today changes"):

- Append `DeckPreflightError::RequiresOrderedGraveyardOwnerV1` to
  `card_def::preflight_fully_supported_deck`, beside `RequiresNativeOpening`, returned for a
  card for which `full_text_v1::owns_definition` holds.
- Add `card_def::preflight_outside_the_game`, which returns the same variant for a Shapeshifter
  and checks nothing else. In
  `RlEpisodeSessionV1::reset_with_custom_decks_companions_london_and_shroud_v1`, beside the
  existing mainboard loop (near lines 6090 to 6093), it runs over each seat's outside-the-game
  cards when present, mapped to the same `UnsupportedDeck` code. It applies only the new check,
  not the whole preflight, so a list that passes today's row check (no tokens, every card Full)
  meets nothing else new. Without it, Extrapolate the Impossible could bring a Shapeshifter in
  from outside the game (section 2.3).
- The changes are in `card_def.rs` and `rl_session.rs`; `limited_session_v1.rs` is not edited.

This is the one schema 5 message that changes. The reset reply for a deck with a Shapeshifter in
its mainboard or outside the game stays an error with code `unsupported_deck`, as today; only
its message changes, from the row check's "is a token or lacks full engine support" to the new
text naming the ordered owner. No input accepted today changes: the only inputs that carry
cards outside the game are best-of-three games (each game's sideboard, from
`session_outside_the_game`) and custom resets that pass a list, and no accepted input can hold a
Shapeshifter today, because the row check refuses a Partial card in either list; every accepted
mainboard likewise holds none. Every other reply is unchanged. The preflight is shared with
other session resets (`rl_session.rs`, `rl.rs`, `runtime_decks.rs`), which then refuse a
Shapeshifter mainboard the same way; none of them takes cards outside the game. So no schema 5
game ever holds a Shapeshifter (section 2.3 checks every other route), and neither the
observation refusal of `full_text_v1::refuse_legacy` nor the donor halt occurs in schema 5. The
test is in section 5.1.

Not chosen: leaving schema 5 without the refusal. A Shapeshifter deck would then be admitted
and end `fail_closed` at its first observation of a face-up Shapeshifter (or, for a custom deck
whose Shapeshifter enters on an unsupported donor, with the terminal reason
`engine_halted:FullTextDonorOutsideSetV1:source:N`): a game that can never complete would be
started instead of refused.

Evidence for "schema 5 untouched", which can hold:

- The schema 5 contract: no field, action or reply is added by A, B1 or B2. Their diffs touch
  `limited_session_v1.rs` not at all, and `rl.rs` only in the mode gates of section 3.2, which
  give `PublicShroudV5Artifact` the same branch as before. `card_def.rs` and `rl_session.rs`
  change only by the refusal above (in B1), whose one changed message becomes visible in B2,
  when the row check stops refusing the Shapeshifter.
- The schema 5 tests pass unchanged: `tests/eternal/eternal_public_shroud_v1.rs`,
  `tests/limited_session_v1.rs`, `tests/london_mulligans_v1.rs`,
  `src/limited_prepared_session_v5/tests.rs`.
- Byte identity of schema 5 games: A and B1 change no registry row, so `KERNEL_CARDDB_HASH`
  (carried in every schema 5 reply as `card_db_hash` and folded into each deck identity), the
  catalog lists, pairs and seeds are all unchanged by them. Every transcript SHA-256 printed by
  `playable_snapshot_decks_finish_and_replay_through_public_sessions` and by the best-of-three
  test must therefore be identical before and after A and before and after B1; the pull
  request quotes both CI logs. These transcripts are not pinned goldens; the comparison is
  evidence that the code changes leave schema 5 games alone.
- B2 changes every transcript, through the catalog hash in every reply and through the new
  pairs and seeds above. That is data, not schema 5 behavior, and touches no pinned golden
  (section 4.6 names them). No comparison across B2 is offered.

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
  "green", then "green" to "red", turns both original black and green words red).
- 613.8, a change on a Shapeshifter (ruled, section 9, question 8): under 613.8a both effects
  are in layer 3, neither comes from a characteristic-defining ability, and applying the
  full-text effect changes the words the color change acts on, so the color change depends on
  it and applies after it (613.8b). The Shapeshifter's text change applies first, then Alter
  Reality's change acts on the gained text. Alter Reality's 2004-10-04 ruling, "It will not
  change any effects that are on the permanent.", does not decide this: it speaks of effects on
  the permanent, not of text the permanent has. The current Comprehensive Rules govern.
  Section 4.7 works through the consequences.
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
TextColorCaptureV1 { item: StackItemId, read: Vec<(ManaColor, ManaColor)> }
TokenColorV1 { token: ObjectLinkV4, color_mask: u8 }
AnimationColorV1 { land: ObjectLinkV4, animation_timestamp: u64, color_mask: u8 }
TextColorChangesV1 {
  changes: Vec<TextColorChangeV1>,          // live changes on objects
  captures: Vec<TextColorCaptureV1>,        // printed to read word, per stack item
  token_colors: Vec<TokenColorV1>,          // created tokens whose printed color changed
  animation_colors: Vec<AnimationColorV1>,  // land animations whose color changed
}                                           // at least one list nonempty while present
text_color(state, object, printed: ManaColor) -> ManaColor
text_color_mask(state, object, printed_mask: u8) -> u8
captured_color(state, item: StackItemId, printed: ManaColor) -> ManaColor
```

`text_color` folds the object's live changes in timestamp order. `text_color_mask` maps each
color bit through `text_color`. `captured_color` returns the word an ability read when it was
put on the stack, or `printed` when the item has no capture. Timestamps come from
`engine::next_timestamp`, called only when Alter Reality resolves.

No existing struct, op variant or enum changes shape, and enums gain only appended variants.
`StackItem` and `trigger::PendingTrigger` derive `Hash` and are hashed with the state, so a
captured color is never stored in them, nor as a new parameter of an existing op (Crimson
Acolyte's grant op, the token ops, `circle_choose`). Everything new lives in the one optional
`GameState` field below.

- Storage: its own field on `GameState`, `text_color_changes_v1: Option<TextColorChangesV1>`,
  serialized with `skip_serializing_if = "Option::is_none"` and hashed in
  `GameState`'s manual `Hash` only when present, under the domain `text-color-changes/v1`, as
  `eternal_opening_v1` and `leyline_opening_v1` are. It must not be a field of
  `eternal_cards_v1::EternalStateV1`: that struct derives `Hash`, so even an empty new field
  would change the state hash of every Eternal game that holds that state.
- Save and restore: part of the state snapshot, restored exactly; an empty record is `None`.
- End: when the object changes zones the link goes stale and the entry is removed at that
  zone change, except that a permanent spell's entries move to the permanent's new
  incarnation as it resolves onto the battlefield (400.7a). When the last entry of all four
  lists goes, the field returns to `None`. Phasing (zone `Command` with the same incarnation,
  `eternal_e5a_v1::phasing`), control changes and face-down to face-up keep it.
- Copies: never copied (707.2). A token or permanent copy is a new object with no entry.
- A Shapeshifter whose text is defined by another card: the entry is keyed by the
  Shapeshifter's own link, like any other object's, and holds only the two words and the
  timestamp, never the donor or its text. `text_color` and `text_color_mask` apply it to
  whatever text the Shapeshifter has at the moment of the query, after the full text (613.8).
  The full-text branches of the protection sites (`p2c::statics::protection_mask` for Akroma and
  Karmic Guide, `p3c::statics::protection_mask` for Ihsan's Shade) therefore pass the donor's
  printed mask through `text_color_mask` with the host. Only masks that come from color words
  are mapped (Akroma, Karmic Guide, Ihsan's Shade). Iridescent Angel's "protection from each
  color" has no color word, and `p3c::statics::printed_mask` returns all five colors for it;
  that mask is never mapped, in the physical or the full-text branch, so a change on an
  Iridescent Angel or on a Shapeshifter with its text leaves protection from every color. When the top card changes, the entry
  stays and acts on the new donor's text; with a noncreature top, or a donor without the word,
  it has nothing to act on and stays. The donor leaving the graveyard does not end it; only the
  Shapeshifter changing zones does (or, for a Shapeshifter spell, it moves to the permanent,
  400.7a). An ability the Shapeshifter puts on the stack reads the word through its capture as
  any other source's does. The audit of section 4.4 reads the Shapeshifter's current text at
  Alter Reality's resolution; every supported donor is in the audited pool.
- Face-down targets: the change is recorded publicly and applies when the permanent is turned
  face up. The decision whether a target is in the audited table (section 4.4) never reads a
  face-down identity: a face-down permanent is accepted, and the audit runs when it is turned
  face up. The pool's morph creatures (Exalted Angel, Bane of the Living, Birchlore Rangers,
  Nantuko Vigilante) have no color word.
- Abilities: a site that reads a word for an ability (Plaguebearer's target, Crimson Acolyte's
  grant, Circle of Protection: Red's choice, Dystopia's and Verdant Force's upkeep triggers,
  the token makers' triggers and activations, the two land animations) reads it through
  `captured_color`, so a later change to the source does not reach it (113.7a). The capture is
  written as the item is put on the stack, keyed by its `StackItemId` (`item.v4.stack_item_id`,
  allocated once per stack item and never reused), and only when the source has a live change
  at that moment; an ability of an unchanged source has no capture and reads its printed word,
  so games without the card write nothing. A triggered ability is captured when it is put on
  the stack, not when it triggers: no player receives priority in between, and the only source
  of a change, Alter Reality's own resolution, commits no event that could trigger. The key
  survives snapshot and restore with the field. The capture is removed when its item leaves
  the stack (resolved, countered, or removed), after the resolution has read it. A copy of a
  stack item gets a new `StackItemId` and no capture; no card in the Premodern pool copies an
  ability, and a copy of an item that has a capture halts with
  `TextColorChangeOutsideAuditV1` rather than guess.
- Token colors: a token created by a site whose word changed gets a `token_colors` entry
  holding its printed color mask, read at the printed-color fallback of
  `engine::unpainted_object_color_mask`; the entry ends when the token leaves the battlefield.
- Animation colors: Treetop Village's and Ghitu Encampment's animations last until end of
  turn, and `continuous_characteristics_v1::animation` reads the colors from the definition's
  `animation` again on every query, keyed by `object.v4.animation_timestamp`. A capture removed
  with the stack item would lose the changed color, and reading the land's live text would let
  a later change recolor an effect that already resolved (611.2c fixes it at resolution). So
  when the ability resolves with a captured color different from the printed one, it writes an
  `animation_colors` entry keyed by the land's link and its `animation_timestamp`;
  `continuous_characteristics_v1::animation` uses that entry's color mask while the timestamp
  matches, and the entry is removed at cleanup with the animation, or when the land changes
  zones. An unchanged land writes nothing.
- The other effects in the audited table whose color matters after their stack item is gone
  need no further record. Tokens keep their color in `token_colors`. Crimson Acolyte's grant
  writes its color into wave E9a's protection record at resolution
  (`eternal_premodern_p3a_v1.rs`, the Acolyte arm of `execute`). Circle of Protection: Red's
  shield stores its `CircleColorV1` in `CopShieldV1::color` at resolution
  (`eternal_sideboard_s1d_v1::create_shield`), and the prevention check reads that field. Every
  other row is a static read live from its permanent's text (protection masks, Absolute Law,
  Light of Day, Sphere of Law, Chill, Gloom), a trigger condition read live when the event
  happens (Warmth, Quirion Dryad), or a spell or ability whose word is used only during its own
  resolution (the Blasts, Snuff Out, Anarchy, Plaguebearer, Dystopia, Verdant Force's and the
  other token makers' creation). No row grants a landwalk or creates a delayed trigger.
- `CircleColorV1` (`eternal_sideboard_s1d_v1.rs`) gains appended variants for white, black and
  green only, so its existing values serialize and hash as before; a non-red shield made from
  a changed Circle carries the new value in the existing `CopShieldV1::color` field.

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
| 9 | Crimson Acolyte | red (its {W} grant) | `eternal_premodern_p3a_v1.rs` grant op, `build_eternal_p3a_v1.rs` | capture; at resolution the color goes into the E9a protection record, which keeps it for the turn |
| 10 | Ihsan's Shade | white | `eternal_premodern_p3c_v1/statics.rs::protection_mask` (physical and full-text branches) | protection follows; also on a Shapeshifter with Ihsan's Shade's text |
| 11 | Absolute Law | red | `eternal_premodern_p3e_v1.rs::protection_mask` | each Law grants its own color; evaluated per Law, not "any Law" |
| 12 | Light of Day | black | `eternal_sideboard_s1c_v1.rs::statics::banned_by_light_of_day` | each Light of Day bans its own color; `any_active` becomes per instance |
| 13 | Sphere of Law | red | `eternal_premodern_p3a_v1/statics.rs::sphere_of_law_prevents` | 2 prevented per Sphere whose color matches the source; the count becomes per instance |
| 14 | Chill | red | `p3a/statics.rs` Chill tax, called from `spell_costs_v1.rs` | {2} per Chill whose color the spell has |
| 15 | Gloom | white (spells) | `eternal_premodern_p3b_v1/statics.rs` spell tax, called from `spell_costs_v1.rs` | {3} per Gloom whose color the spell has |
| 16 | Gloom | white (enchantment activations) | `p3b/statics.rs` activation tax, called from `engine.rs` | {3} per Gloom whose color the enchantment has |
| 17 | Warmth | red | `eternal_premodern_p3a_v1.rs` Warmth trigger (`active_permanents(state, WARMTH, ...)`) | each Warmth triggers on its own color |
| 18 | Dystopia | green, white | `eternal_premodern_p3b_v1.rs` upkeep trigger and the color mask in `effect.rs` (the "any of these colors" filter) | capture at trigger; either or both words |
| 19 | Circle of Protection: Red | red | `eternal_premodern_p3g_v1.rs::circle_of_protection_red`, `eternal_sideboard_s1d_v1::circle_choose(CircleColorV1::Red)` | capture; `CircleColorV1` has Blue and Red only and gains appended White, Black and Green variants; the shield keeps the color in `CopShieldV1::color` |
| 20 | Quirion Dryad | white, blue, black, red | `eternal_premodern_p1b_v1.rs` Dryad trigger filter | the trigger's color set follows the words |
| 21 | Blue Elemental Blast | red (both modes) | `TargetSpec::RedSpellOnStack`, `TargetSpec::RedPermanent` (`card_def.rs`), the resolution legality recheck in `engine.rs` | a change on the stack can make the chosen target illegal; targets are enumerated before any change can exist, so only the recheck reads it |
| 22 | Red Elemental Blast | blue (both modes) | `TargetSpec::BlueSpellOnStack`, `TargetSpec::BluePermanent`, same recheck | same; the main defensive use |
| 23 | Pyroblast | blue (both modes) | `EffectCond::TargetIsColor(0, U)`, written in the recipe in `card_def.rs` and evaluated in `effect.rs` (the `TargetIsColor` arm of the condition evaluator) | resolves and does nothing to an object without the new color |
| 24 | Hydroblast | red (both modes) | `EffectCond::TargetIsColor(0, R)`, same two places | same |
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
| 35 | Zombie Infestation | black | the generic token creation in `effect.rs` for the recipe `CreateToken("Zombie Token")` of `build_eternal_p2a_v1.rs` | capture at activation |
| 36 | Phyrexian Processor | black | `eternal_premodern_p3f_v1.rs` Minion token | capture at activation |
| 37 | Treetop Village | green | the animation colors of `build_support/standard_lands_v1.rs`, mapped at run time in `continuous_characteristics_v1::animation` | capture at activation; an `animation_colors` entry keeps the color until cleanup |
| 38 | Ghitu Encampment | red | same | same |

That is 36 cards and 38 sites (Crimson Acolyte and Gloom have two each). The code that
changes lives in 16 modules: 13 card modules (`eternal_premodern_p1a_v1`, `_p1b_v1`, `_p2b_v1`,
`_p2c_v1`, `_p2d_v1`, `_p3a_v1`, `_p3b_v1`, `_p3c_v1`, `_p3e_v1`, `_p3f_v1`, `_p3g_v1`,
`eternal_sideboard_s1c_v1`, `eternal_sideboard_s1d_v1`, each with its `statics` or `targets`
submodule where the site is there), and `engine.rs` (the Blast and Snuff Out rechecks, Gloom's
activation tax caller), `effect.rs` (Dystopia's color filter, `TargetIsColor`, the generic
token creation that Zombie Infestation's recipe compiles to, the token base color) and
`continuous_characteristics_v1.rs` (the land animation colors). The build tables
(`build_eternal_p2a_v1.rs`, `build_eternal_p3a_v1.rs`, `build_support/standard_lands_v1.rs`)
and the recipes in `card_def.rs` are read, never changed (section 7, C1). `spell_costs_v1.rs`
calls the Chill and Gloom taxes unchanged. A token's color is set as it is created through a
`token_colors` entry read at the printed-color fallback of `engine::unpainted_object_color_mask`,
so later layer-5 setters (Wild Mongrel, Painter's Servant, Mycosynth Lattice) still override it.

Supported as a recorded change with no effect (the word is in a cost paid from hand or
graveyard before Alter Reality can target the spell): Unmask, Contagion, Pyrokinesis
(`CostComponent::ExileCardFromHandWithColorV1`), Misdirection, Flash of Insight's flashback
cost.

Supported as a recorded change with no effect: the 264 cards with no color word, and the pool's
tokens. Examples worth stating because they look otherwise: Black Vise, Circle of Protection:
Red's name, Red and Blue Elemental Blast's names (proper names); Iridescent Angel and Mother of
Runes ("protection from each color", "the color of your choice"); Avatar of Woe (fear);
Funeral Charm, Snuff Out's alternative cost, Choke, Daze, Gush, Thwart, Foil, Mogg Salvage,
Fireblast, Lava Dart, Quirion Ranger, Anger, Tainted Field and Tainted Wood, Eternal Dragon and
the five fetch lands (Flooded Strand, Polluted Delta, Bloodstained Mire, Windswept Heath,
Wooded Foothills) (land types); Lurking Skirge (its current text has no color word).

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
  (613.7). Gated like `meddling_mage_names`: built only when `text_mode` is
  `PublicShroudV5Artifact` (schema 5) or `PremodernOrderedRegisteredV1Artifact` (the registered
  owner, section 3.2), and present only when the list is nonempty; absent otherwise.
- Action meaning: appended `ActionSemanticV1::ChooseTextColorWordV1 { actor, source, target,
  stage, from, color, option_index, option_count }`, serialized `choose_text_color_word_v1`.
  `source` is the resolving Alter Reality, `target` its `TargetRefV1`, `stage` is `from` or
  `to`, `from` is the chosen old word in the `to` stage, `color` the word this option offers.
  It answers with `ChooseEffectOption(option_index)`. Stage `from` offers the five colors in
  WUBRG order; stage `to` the four others in the same order.
- Every other consumer refuses a state with a record or a pending text-word choice, as for
  shroud and Meddling Mage: schema 4 and earlier observation versions (including the frozen
  ordered mode `PremodernOrderedV1Artifact`), flat encoders and action slices, infoset export,
  search, the Spellbench bridge. The new semantic is handled wherever S1k's
  `ChooseMeddlingMageNameV1` is: the Human labels (`human_bo3_v1/labels.rs`, which refuse it as
  they refused the Mage semantic), `agent_bridge_v1.rs` and the two `rl_session.rs` matches
  over action semantics. The registered ordered owner carries the field through its V5
  projection, so Replenish against FEB works.

### 4.6 The condition: games without the card stay byte-identical

The general argument: a game without Alter Reality never writes `text_color_changes_v1` (no
change, no capture, no token or animation color), never draws `next_timestamp` for it, and never offers its
action; the field is `None`, skipped in serde and not hashed, and no existing struct, op
variant or enum value changes shape (section 4.2). So its state bytes, state hash,
observations and legal actions are those of main, in every build. The main families of
pinned goldens, and how each is held, are below. The general argument covers every other
golden in the tree as well: the further golden tests under `mtg-kernel/src` (among them
`trace.rs`, `surface.rs`, `private_physical_trajectory_v1.rs`,
`native_full_episode_trajectory_v2.rs`, `async_flat_scored_rollout_v1.rs`, `flat_policy_v2.rs`,
`native_flat_tensorizer_v2/action_reference_tests.rs` and `streaming_tests.rs`,
`native_ladder_opponent_v1.rs`, `native_opponent_sampler_v1.rs`) and under
`mtg-kernel/tests/pauper` (`brainstorm.rs`, `fast_sampler.rs`, `flat_policy_v1.rs`) run in builds
where nothing new is ever written.

| Pinned golden | Build that runs it | Held by |
| --- | --- | --- |
| `card_def.rs` `card_db_hash_v34_is_frozen`, `card_db_hash_full_pool_provisional_fdn_is_frozen` | default, FDN | no row or recipe in those catalogs changes; pass unchanged |
| `card_def.rs` `card_db_hash_eternal_v1_is_frozen` | Eternal | a catalog pin, not a game golden: re-pinned once by C2's row, unchanged by C1 (section 7) |
| `state.rs` `legacy_randomization_bytes_are_sealed` | default | the general argument; passes unchanged |
| `environment_randomization_v2/reset_physical_trajectory_goldens_v1.rs`, `native_full_episode_trajectory_v1_goldens.rs`, `native_full_episode_trajectory_v2_goldens.rs` | default | same |
| `rl_session.rs` `environment_hashes_are_diagnostic_dispatched_with_exact_goldens`, `flat_action_core_matches_hand_authored_all_kind_golden` | default | same |
| `tests/pauper/async_rollout_v2_legacy_golden.rs` | Pauper integration | same |
| the flat tensorizer goldens (`native_flat_tensorizer_v2.rs`, `native_flat_tensorizer_v2_cost_tests_v1.rs`) and the `policy_surface_v5.rs` goldens | default | same |
| schema 5 behavior: `tests/eternal/eternal_public_shroud_v1.rs`, `tests/limited_session_v1.rs`, `tests/london_mulligans_v1.rs`, `src/limited_prepared_session_v5/tests.rs` | default and Eternal | the field is absent while empty; the mode gates give schema 5 its old branch; pass unchanged |

No stored transcript or state hash of an Eternal catalog game exists (`tests/eternal`,
`src/premodern_ordered_public_v1` and `src/eternal_bo3_v1.rs` hold none); those games check
self-replay only. Three tests prove the argument directly in C1: a schema 5 observation of a
game without the card has no `text_color_changes` key; a state with an item on the stack, a
pending trigger and no record hashes and serializes exactly as on main (the state built by the
same seeded steps on the base commit, its hash recorded in the test); and a state whose only
change was removed hashes like one that never had one. And because C1 changes no registry row,
recipe or catalog hash (section 7), every catalog and best-of-three transcript SHA-256 printed
in CI must be identical before and after C1, with the same pairs and seeds; the C1 pull request
quotes both logs. C2 then changes every transcript through the catalog hash carried in each
schema 5 reply and folded into each deck identity, and through `PLAYABLE_75`'s new pairs and
shifted seeds (section 3.6). Those transcripts are unpinned, so this does not touch the
ruling's condition, and no comparison across C2 is offered.

### 4.7 Volrath and Alter Reality together

Replenish (Alter Reality in the sideboard, Legacy's Allure in the mainboard) meets FEB in best
of three, so a change can land on a Shapeshifter in a snapshot game. By 613.8 (section 4.1)
the change acts on the Shapeshifter's current text, after its full text. The supported donors
with a color word are Akroma, Angel of Wrath (protection from black and from red), Karmic Guide
(protection from black) and Ihsan's Shade (protection from white); the Shapeshifter's own text
and every other supported donor have none. Worked through:

- "Black" to "green" on a Shapeshifter with Akroma's text: protection from green and from red.
- The top then becomes Karmic Guide: protection from green. It becomes Ihsan's Shade:
  protection from white, untouched. It becomes a noncreature card or a donor without a color
  word: no protection, and the change stays recorded for the next donor.
- A second change, "green" to "blue", made later: protection from blue for Karmic Guide's
  text, in timestamp order (613.7).
- A change made to a Shapeshifter spell carries over to the permanent (400.7a) and acts on its
  text from then on.
- The Shapeshifter leaving the battlefield ends the change.

There is no typed halt for this case. The test lands with whichever of B2 and C2 merges second
(section 5.2).

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
      text at entry; Phantom Nishoba and Spike Feeder as donors (partial programs). The
      Masticore branch of
      `volrath_native_registration_retains_paid_discard_with_no_or_unsupported_text_both_seats`
      becomes a refusal case.
- [ ] Part A donors without a dedicated donor fixture today: Ihsan's Shade and Iridescent Angel
      as donors give their protection on a Shapeshifter and are not refused, both seats.
- [ ] Part C, both seats, one fixture each: with Anger, Arrogant Wurm, Eternal Dragon or Glory
      on top the Shapeshifter has exactly that card's keywords and no other; with Krosan
      Tusker, Gempalm Incinerator, Genesis, Nether Spirit, Thornscape Battlemage, Bane of the
      Living or Nantuko Vigilante on top it has no keyword and offers no action beyond the
      retained discard; a Shapeshifter entering with Thornscape Battlemage on top triggers
      nothing and is not refused; no part C card is refused, entering or not.
- [ ] Part D, both seats, for each of the five cards: it becomes the top card under a
      Shapeshifter already on the battlefield (a death, a discard) and play continues with its
      keywords and no refusal; a Shapeshifter entering with it on top (cast, Animate Dead, and
      inside an ordered batch whose final top is the card) halts with
      `FullTextDonorOutsideSetV1` at that entry. Meddling Mage the same way.
- [ ] The audited lists: every creature card of the Premodern pool is in exactly one of the
      four classes or the refused class of appendix A.
- [ ] Pending order and the backstop: a pending owner order, also after a restore without a
      runner, with an unsupported donor among the arrivals offers the order menu and halts
      only once the order completes.
- [ ] Mogg Fanatic and Ravenous Rats stay donors: their three existing fixtures pass
      unchanged after the refusal lands.
- [ ] The frozen tests of section 3.1 pass unchanged, plus a new pin of the mirror's view
      sequence: the SHA-256 of the frozen JSONL wire states (numeric and Human views, prompt
      by prompt) of the seed `20261108` fixture in both starting seats, recorded on main before
      PR A and asserted by every later step. It is what catches a change to Karmic Guide's echo
      or any other mirror view.
- [ ] Registered owner: a twin of the hidden-world privacy test; step, snapshot, restore and
      stale tokens; shroud in its scene; a registered game in which a Meddling Mage is cast,
      its name chosen through the owner's menu, and the name shown in its scene;
      `RegisteredHaltV1` reporting.
- [ ] Registration: the ordered profile's admissions and refusals and the profile mismatch.
      The registered profile refuses a card outside the Premodern pool.
- [ ] Schema 5 refusal (section 3.7): a schema 5 FEB reset is refused with `unsupported_deck`
      and the new message, and so is a reset whose mainboard holds Extrapolate the Impossible
      and whose cards outside the game hold a Shapeshifter, in either seat; a reset with cards
      outside the game and no Shapeshifter (a `PLAYABLE_75` game's sideboard) is admitted with
      the same reply as on main.
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
- [ ] Storage: a state with an item on the stack, a pending trigger and no record hashes and
      serializes as on main; a capture is written only for a changed source, read at
      resolution, removed when the item leaves the stack, and kept across snapshot and restore;
      a copy of a captured item halts.
- [ ] Animation colors: a Treetop Village animated after a change to "blue" stays a blue Ape
      when the land is changed again the same turn, loses the entry at cleanup, and an
      unchanged land writes no entry; the same for Ghitu Encampment.
- [ ] Iridescent Angel: a change "black" to "green" on an Iridescent Angel, and on a
      Shapeshifter with its text, leaves protection from all five colors.
- [ ] Volrath interaction, a test of exactly the ruled case (section 4.7), in both seats: a
      Shapeshifter that has gained Akroma's text is the target of Alter Reality changing
      "black" to "green"; the changed text governs (protection from green and from red: a green
      source's damage is prevented and a black one's is not, a green spell cannot target it);
      then the top card becomes Karmic Guide (protection from green), then Ihsan's Shade
      (protection from white), then a noncreature card (no protection), and the change is still
      recorded; a snapshot restore in the middle keeps it; the Shapeshifter leaving the
      battlefield ends it.
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

The donor classes of section 2.2 change no census figure: the census counts registry rows,
and every donor class concerns only what a Shapeshifter may read in a game.

By deck: `premodern-full-english-breakfast` (3.0%) gains both its mainboard and its full 75
with the Shapeshifter (every other card of its 75 is already Full);
`premodern-replenish` (6.0%) gains its full 75 with Alter Reality (its mainboard is already
playable). Every other Premodern deck is already playable with a full 75. The session lists
follow: `PLAYABLE` goes from 23 to 24 Premodern decks, `PLAYABLE_75` from 22 to 24.

## 7. Pull request plan

Each step is reviewable alone. Steps that change a registry row take one slot on the
catalog-hash chain, because they re-pin `card_db_hash_eternal_v1_is_frozen`; the others take
none. All of it waits behind the chain steps and issue #657's pieces. The lead's order: "Steps
B2 and C2 take chain slots after S1o and C5, and behind issue 657's pieces."

1. **A: Volrath donor scope.** Files: `full_text_v1.rs` (the four donor class constants
   of section 2.2, `donor_supported`, `entry_refused`, `refused_donor`, the new kinds and programs), `premodern_ordered_public_v1.rs` (program
   meanings), `eternal_premodern_p2d_v1.rs` (donated echo cost), `engine.rs` (the appended
   mechanic and the backstop), `event.rs`, `graveyard_order_v1.rs`, `eternal_e5a_v1/phasing.rs`
   (check sites), `full_text_v1/registered.rs` and `tests.rs`, `volrath_full_text_v1.md`.
   Before any of it, a first commit adds the frozen mirror's view-sequence pin (section 5.1)
   recorded on main. No registry row; no chain slot. Acceptance: section 5.1's donor and
   refusal fixtures and the frozen tests, the view pin among them, pass on CI and the hosted
   Windows Eternal job; the catalog and best-of-three transcript SHA-256s in CI are identical
   to main's (section 3.7).
2. **B1: registered ordered owner.** Files: `premodern_ordered_public_v1/feb.rs`
   (`reset_registered_v1`, `REGISTERED_PROFILE_V1`, `FebCurrentV1::RegisteredHaltV1`),
   `feb_jsonl.rs` (the frozen transport's match on `FebCurrentV1` in `Projection::read` ends
   with a wildcard that maps anything else to `unsupported_numeric_projection`; B1 adds an
   explicit `RegisteredHaltV1` arm mapping it to the existing `native_halt` state, so a
   registered halt can never be reported as unsupported; the frozen owner never returns the
   variant, so the frozen wire is unchanged) and
   `decision.rs` (`bind_next` and `view` call the observe entry of the owner's profile),
   `premodern_ordered_public_v1.rs` (registered scene), `rl.rs` (the text mode
   `PremodernOrderedRegisteredV1Artifact`, its gates of section 3.2, and the observe entry
   `observe_premodern_ordered_registered_v1`), `eternal_bo3_v1.rs` (profile, errors,
   `start_game_with_ordered_graveyard_v1`, the Premodern pool check), `card_def.rs` and
   `rl_session.rs` (the preflight variant and the outside-the-game check of section 3.7; the
   row check still refuses the Partial card first, so no reply changes until B2),
   `tests/eternal/eternal_bo3_v1/eternal_bo3_ordered_v1.rs` (driver),
   owner tests. The Shapeshifter stays Partial, so the owner's own tests register it through a
   test-only admission of the frozen FEB 75 (as the frozen owner admits a Partial card today).
   No row; no chain slot. Acceptance: the registered-owner, registration and frozen tests of
   section 5.1; schema 5 evidence of section 3.7 (transcripts identical to main's); registered
   FEB matches against Psychatog
   (Dominate) and against a test deck with Legacy's Allure and Meddling Mage play to terminal
   and replay. Replenish's own 75 cannot register before C2, since Alter Reality is missing.
3. **B2: Shapeshifter Full.** Files: `data/eternal_v1/cards_v1.json` (row), `feb.rs` (the
   capability line), `card_def.rs` (re-pin), `tests/eternal/eternal_catalog_v1.rs` (`PLAYABLE`,
   `expected_capability`, and routing in both loops of
   `playable_snapshot_decks_finish_and_replay_through_public_sessions`: the schema 5 preflight
   loop over every `PLAYABLE` deck (around lines 1168 to 1176) skips Shapeshifter decks, which
   the preflight now refuses, and the pair loop sends their pairs to the ordered driver), `tests/eternal/eternal_bo3_v1.rs` (`PLAYABLE_75`, routing),
   `eternal_formats_support_v1.md` (a pointer to this document and the census), this document's
   status. One chain slot. Acceptance: section 5.1 complete; census as in section 6.
4. **C1: color-word record and sites.** Files: `text_color_change_v1.rs`, `state.rs` (field,
   hash, serde), the 16 modules of section 4.4, `rl.rs` (field, semantic, gates, refusals),
   `effect.rs` (choice purpose), the consumers of section 4.5. No card and no row; tests write
   records directly. C1 changes no build-time recipe or canonical field: no
   `build_eternal_*.rs` table, no `build_support/standard_lands_v1.rs` field (Treetop
   Village's and Ghitu Encampment's `colors` stay as they are) and no recipe in `card_def.rs`;
   every changed color is read at run time through `text_color` and `captured_color`. That
   keeps `KERNEL_CARDDB_HASH`, the FNV hash of one canonical line per card built from those
   fields, unchanged, so C1 takes no chain slot, and `card_db_hash_eternal_v1_is_frozen`
   passing unchanged is the check. If C1 had to change any of those fields it would re-pin
   the hash and become a chain step. Acceptance: section 5.2's site, storage, refusal and
   condition tests; transcripts identical to main's (section 4.6).
5. **C2: Alter Reality Full.** Files: `build_eternal_p3i_v1.rs` and
   `eternal_premodern_p3i_v1.rs` (wave P3i), the row, `card_def.rs` (re-pin),
   `tests/eternal/eternal_premodern_p3i_v1.rs`, `tests/eternal/eternal_bo3_v1.rs`
   (`PLAYABLE_75`), `eternal_formats_support_v1.md`. One chain slot. Acceptance: section 5.2
   complete, census as in section 6.

C1 and C2 are independent of A, B1 and B2 except for the interaction test of section 4.7,
which lands with whichever of B2 and C2 merges second. Issue #460 closes when B2 and C2 are
both merged and the census on main shows 305 of 305 Full.

## 8. Risks and limits after both cards are Full

- The refusals are real: a custom registration with a control effect and a creature outside
  the donor set, a Shapeshifter entering with Meddling Mage's text, Alter Reality on a card
  outside the 305, or a copy of a stack item that holds a captured color halts the game.
  Snapshot games never reach them. The last is a refusal beyond the audited table: no card in
  the Premodern pool copies an ability, but a custom schema 5 deck that pairs Alter Reality with
  an ability copier from another format can reach it.
- The registered owner's Human view labels only the meanings its renderer knows; the other
  rows stay `unsupported`, and qualification uses numeric ordinals.
- Donors that would play wrongly are refused (section 2.2, appendix A): 62 pool creatures in
  any case, the five part D cards when a Shapeshifter enters with one on top. Only a custom
  registration with Dominate or Legacy's Allure, or a custom FEB 75, reaches them. Giving them
  programs is a later wave.
- The registered owner is in process only; a JSONL route for registered games is a later
  issue (section 9, question 5).
- Flat encoders, search, infoset export and earlier observation versions still refuse these
  games, so they are not training-ready inputs.
- The observation shows Alter Reality's change once it is recorded, not during the two-stage
  choice beyond the actions themselves, as S1k shows Mage names only once entered.
- The 38 sites are hand-routed; a later card with a color word needs its own site and audit
  row. The audit test catches a new Premodern snapshot card but not a new site in an existing
  card's code.
- The donor check runs at every commit tail; its cost is one battlefield scan, measured in
  the A pull request.

## 9. Rulings on the questions this design raised

The lead ruled on all eight questions in the shared record entry of 2026-11-17 19:52 ET. Each
ruling, quoted or closely paraphrased, and where the design implements it:

1. Mogg Fanatic and Ravenous Rats: "Ruling 1 amended: Mogg Fanatic and Ravenous Rats stay
   donors. The rule is 'every card with a donated program today, plus the listed additions; any
   other donor is refused with the typed error'. Nothing that plays correctly today is refused."
   Implemented in section 1 (the amended ruling), section 2.2 (the donor classes), section 5.1
   (their fixtures stay) and appendix A.

   How the amended ruling is applied. This design reads the ruling's two sentences together:
   the supported set is what plays correctly today plus the listed additions, and only what
   would play wrongly is refused. So besides the cards with a donated program (part A) and the
   listed additions (part B), it supports the eleven cards whose whole battlefield text is
   generic (part C: Anger, Arrogant Wurm, Eternal Dragon, Glory, Krosan Tusker, Gempalm
   Incinerator, Genesis, Nether Spirit, Thornscape Battlemage, Bane of the Living, Nantuko
   Vigilante), which play correctly today with no program. And it accepts the five cards whose
   only other text works as the permanent enters (part D: Wall of Blossoms, Goblin Matron,
   Goblin Ringleader, Phyrexian Dreadnought, Caller of the Claw) as a top card for a
   Shapeshifter already on the battlefield, refusing them only at the moment a Shapeshifter
   would enter with one on top, the one case where they play wrongly. Phantom Nishoba, Spike
   Feeder, Exalted Angel and the other refused cards each lose a battlefield ability today.
2. Meddling Mage through Legacy's Allure, as a donor reached through "the two Premodern
   control effects": confirmed. Section 2.2, part B, with the refusal of a Shapeshifter entering
   with Mage text.
3. Schema 5 and a Shapeshifter deck: "refuse at reset with `unsupported_deck` and the new
   message, by the appended preflight variant. A deck that can never complete a game is
   refused before the game, on condition that no input accepted today changes. Name it in the
   design as the one schema 5 message that changes." Section 3.7 (the refusal, including the
   same check over the cards outside the game, and the proof that no input accepted today
   changes) and section 5.1 (its test). It touches no field, action or error code; it changes
   one message, through `card_def.rs` and `rl_session.rs`.
4. The registered profile limited to the 305-card Premodern pool: confirmed. Section 3.3.
5. No JSONL route for registered games yet: confirmed. Sections 3 and 8; the in-process owner
   and driver are the qualifying consumer.
6. The Shapeshifter as the only ordered-graveyard reader for routing: confirmed. Section 3.6.
7. Foundations combat, London mulligans and the public shroud flag for the registered profile,
   with numeric submissions: confirmed. Sections 3.4 and 3.5.
8. Alter Reality on a Shapeshifter: "Rule 613.8 (the dependency reading): the Shapeshifter's
   text change applies first, then Alter Reality's change acts on the gained text. The current
   Comprehensive Rules govern; quote the 2004 ruling in the design with the reason it does not
   decide this (it speaks of effects on the permanent, not of text the permanent has), and add
   a test of exactly this case. No typed halt." Sections 4.1 (the rule and the 2004 ruling),
   4.2 (the record for a Shapeshifter whose text is defined by another card), 4.7 (the worked
   consequences) and 5.2 (the test).

The lead also set the order of the chain steps: "Steps B2 and C2 take chain slots after S1o
and C5, and behind issue 657's pieces" (section 7).

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
- The site count is 38, in 16 modules, not about 30 in 10.
- Not in the note: `feb.rs::mainboard` requires the Shapeshifter to be Partial, and
  `expected_capability` pins it Partial; both change with the flip. The frozen scene's text
  mode refuses shroud and Meddling Mage names, and the frozen owner plays without Foundations
  combat. Once the card is Full, a schema 5 FEB reset would pass both the session's row check
  and the preflight, which is why section 3.7 adds the refusal. `EternalStateV1`, `StackItem` and `PendingTrigger` derive `Hash`, so neither
  the color record nor its captures can live in them without moving state hashes.

## Appendix A. The 101 creature cards of the Premodern pool, by donor class

Source: the 305 distinct cards of the Premodern snapshot, of which 101 are creature cards, each
read against its current Oracle text and the code paths of section 2.2.

| Class | Count | Cards |
| --- | ---: | --- |
| A: donated program today | 15 | Akroma, Angel of Wrath; Battlefield Scrounger; Birds of Paradise; Hermit Druid; Ihsan's Shade; Iridescent Angel; Karmic Guide; Mogg Fanatic; Palinchron; Phyrexian Devourer; Psychatog; Ravenous Rats; Triskelion; Volrath's Shapeshifter; Wall of Roots |
| B: listed additions | 8 | Elvish Spirit Guide; Squee, Goblin Nabob; Auramancer; Monk Realist; Uktabi Orangutan; Ghitu Slinger; Xantid Swarm; Meddling Mage (refused at entry) |
| C: generic battlefield text, no program needed | 11 | Anger; Arrogant Wurm; Bane of the Living; Eternal Dragon; Gempalm Incinerator; Genesis; Glory; Krosan Tusker; Nantuko Vigilante; Nether Spirit; Thornscape Battlemage |
| D: accepted except at entry | 5 | Caller of the Claw; Goblin Matron; Goblin Ringleader; Phyrexian Dreadnought; Wall of Blossoms |
| Refused: loses a battlefield ability today | 62 | Academy Rector; Anurid Brushhopper; Argothian Enchantress; Avalanche Riders; Avatar of Woe; Ball Lightning; Basking Rootwalla; Beloved Chaplain; Birchlore Rangers; Blastoderm; Crimson Acolyte; Deranged Hermit; Devoted Caretaker; Druid Lyrist; Exalted Angel; Fyndhorn Elves; Glowrider; Goblin Goon; Goblin Lackey; Goblin Piledriver; Goblin Sharpshooter; Goblin Tinkerer; Goblin Vandal; Goblin Warchief; Graveborn Muse; Grim Lavamancer; Hypnotic Specter; Jackal Pup; Knight of Stromgald; Llanowar Elves; Masticore; Mother of Runes; Multani's Acolyte; Multani, Maro-Sorcerer; Mystic Enforcer; Nantuko Shade; Nova Cleric; Petradon; Phantom Nishoba; Plaguebearer; Priest of Titania; Putrid Imp; Quirion Dryad; Quirion Ranger; Ravenous Baloth; Siege-Gang Commander; Skirk Prospector; Soltari Monk; Soltari Priest; Spike Feeder; Terravore; Tireless Tribe; True Believer; Verdant Force; Veteran Brawlers; Veteran Explorer; Vintara Snapper; Weathered Wayfarer; Wild Mongrel; Wirewood Symbiote; Withered Wretch; Yavimaya Granger |

Each refused card has at least one battlefield ability that is neither a keyword nor donated:
an activated or mana ability outside `public_program` and `mana_text`, a triggered ability
outside `DonatedTriggerKindV1` (including dies, upkeep, echo and fading triggers), a static
ability or characteristic-defining ability keyed by the physical name, or protection or shadow
read through `active_named`. Of the snapshot decks only FEB, Psychatog and Replenish can
control a Shapeshifter, and their creature cards are all in A or B (section 2.3).
