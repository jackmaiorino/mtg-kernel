# MageZero Standard family G v1: trigger and static creatures

Family G of the MageZero Standard inventory
(`docs/reports/standard_magezero_inventory_v1.md`) covers creatures with
triggered and static abilities. This batch builds the 21 cards from that family
that the five mono-color decks need. Kellan, Planar Trailblazer stays with the
FDN threads and is copied in once FDN merges it. The non-mono family G cards are
left for a later batch.

Every definition appends to `data/standard/magezero_v1/cards_v1.json`. The
behavior tables in `build.rs` (keywords, `trigger_recipe_for`,
`standard_static_recipe_for`, activated recipes) name each card, so the Standard
catalog identity covers them. The batch moves the Standard catalog to
`kernel_carddb_standard/v3` (`0xc6139dbdd2f0db62`); Assassin and Mercenary join
`Subtype::OUTLAW_TYPES` for Shoot the Sheriff. Pauper and FDN canon never include the Standard
static field. New engine state and rules are cfg-gated to
`standard-magezero-fixtures` wherever a default build would otherwise change. The
trigger tables live in `mtg-kernel/src/trigger/standard_family_g_v1.rs` and the
statics in `mtg-kernel/src/standard_statics_v1.rs`.

| Card | Behavior | Engine recipe |
| --- | --- | --- |
| Novice Inspector | ETB investigate | `Etb` + Clue token |
| Sentinel of the Nameless City | Vigilance; ETB or attack: create a Map | `Etb`/attack + Map token |
| Cenote Scout | ETB explore | `ExploreTarget` on the source |
| Gatekeeper of Malakir | Kicker; kicked ETB: target player sacrifices a creature | kicker intervening-if + `SacrificeCreature` |
| Deep-Cavern Bat | Flying, lifelink; ETB look at an opponent's hand, may exile a nonland card until the Bat leaves | Mesmeric Fiend's linked exile, made optional |
| Razorkin Needlehead | First strike on your turn; opponent draws: 1 damage to them | conditional keyword + `OpponentDraws` |
| Ascendant Packleader | Enters with a counter if you control MV 4+; cast MV 4+: counter | entry-counter static + `CastSpellManaValueAtLeast(4)` |
| Sharp-Eyed Rookie | Vigilance; bigger creature enters: counter and investigate | outgrowing-entrant trigger, rechecked at resolution |
| Evolving Adaptive | Enters with oil; +1/+1 per oil; another bigger creature enters: oil | oil counters + self counter boost |
| Quirion Beastcaller | Cast creature: counter; dies: distribute its counters | counter last-known information + per-counter choices |
| Unstoppable Slasher | Deathtouch; combat damage to a player: they lose half their life, rounded up; dies with no counters: return tapped with two stun counters | `LoseHalfLifeRoundedUp`, `DiesWithoutCounters` |
| Coppercoat Vanguard | Other Humans you control get +1/+0 and ward {1} | lord table + granted ward trigger |
| Adeline, Resplendent Cathar | Vigilance; power = creatures you control; you attack: tapped attacking Human token | characteristic-defining power + `ControllerAttacks` |
| Bloodletter of Aclazotz | Flying; opponents lose twice as much life on your turn | life-loss modification at commit |
| Thalia, Guardian of Thraben | First strike; noncreature spells cost {1} more | static spell-cost adjustment |
| Haughty Djinn | Flying; power = instants and sorceries in your graveyard; yours cost {1} less | characteristic-defining power + cost adjustment |
| Hired Claw | Attack with Lizards: 1 damage to target opponent; {1}{R}: counter, only if an opponent lost life this turn, once per turn | `ControllerAttacksWithSubtype(Lizard)` + per-turn life-loss record |
| Warden of the Inner Sky | Tap three untapped artifacts or creatures: counter and scry 1 (sorcery speed); flying and vigilance with three or more counters | `TapControlled` cost + conditional keywords |
| Extraction Specialist | Lifelink; ETB return a creature card with MV 2 or less from your graveyard; it can't attack or block while you control the Specialist | `TargetSpec` 50 + attack/block restriction record |
| Hullbreaker Horror | Flash; can't be countered; you cast a spell: return target spell you don't control or target nonland permanent to hand, or neither | `CastSpell` + placement-time modes, `TargetSpec` 51 |
| Recruitment Officer | {3}{W}: look at the top four, may take a creature card with MV 3 or less, rest on the bottom | private zero-or-one choice + typed partition frame |

Human Token (1/1 white Human) is appended for Adeline.

## New engine surface

- `GameState` gains three optional fields, each serialized and hashed only when
  `Some`: `counter_lki_v1` (counters a departing Quirion or Slasher had),
  `life_loss_turn_v1` (which players lost life this turn) and
  `attack_block_restrictions_v1` (Extraction Specialist). Only Standard builds
  fill them.
- New trigger conditions: `DiesWithoutCounters`, `ControllerAttacks`,
  `ControllerAttacksWithSubtype`, `CastSpell`, plus the opponent-draw, creature
  cast, mana-value cast and outgrowing-entrant conditions. Declaring attackers
  emits a `ControllerAttacked` event for each permanent with a "whenever you
  attack" trigger.
- New effect ops: `LoseHalfLifeRoundedUp`,
  `ReturnSourceFromGraveyardTappedWithStunCounters`,
  `CreateTokenTappedAndAttacking`, `AddPlusOneCounterToAbilitySource`,
  `ReturnTargetCreatureCardRestrictedWhileSourceControlled` and
  `LookTopMayTakeCreatureManaValueAtMostToHandBottomRest`, with the new
  `LookTopTakeCreatureManaValueAtMostToHand` selection purpose. That purpose is
  private to its chooser in the RL projection and is a `CardSelection`.
- New cost component `TapControlled { count, filter }`, which reuses the
  sacrifice-cost staging one pick at a time.
- New target specs `CreatureCardInOwnGraveyardManaValueAtMost` (stable id 50)
  and `SpellYouDontControl` (51).
- Life payments, including Phyrexian mana payments, use the life-loss commit
  path in Standard. Bloodletter modifies the life lost, while affordability
  uses the printed payment amount (CR 119.4 and 118.11). Other catalogs retain
  their existing Phyrexian payment event history.
- Normal, Adventure, Omen and Bestow casts have a static cost adjustment.
  Complete total-cost ordering and alternative-cost coverage remain unfinished;
  Thalia and Haughty Djinn are therefore Partial.

- The rules-vector extractor maps the new effect ops in a new slice,
  `rules_vector_v1/meaning/effect_g.rs`, and the new trigger conditions, cost
  component and target specs in the existing tables.

## Remaining limitations and admission

This batch adds 14 Full and seven Partial deck-card definitions, plus Human
Token. The tracked catalog has 34 Full nonbasic cards and eight Partial cards
(including the earlier Memory Deluge). Both Rust and Python full-deck admission
refuse every Partial card. Existing behavior tests exercise development support;
they do not certify these incomplete definitions as Full.

- Quirion Beastcaller is Partial. Its dies trigger uses untargeted per-counter
  choices at resolution. Printed targets and allocation must be announced when
  the trigger enters the stack, with target legality enforced at resolution.
- Extraction Specialist is Partial. Its restriction currently resumes if its
  controller loses and regains the same Specialist. The printed duration ends
  permanently when that player first stops controlling it (CR 611.2b).
- Sharp-Eyed Rookie and Evolving Adaptive are Partial. Resolution currently
  requires the entrant still to be on the battlefield; a departed entrant needs
  last-known power and toughness for the intervening-if check.
- Thalia and Haughty Djinn are Partial. Alternative, flashback, escape, madness
  and plotted costs lack the adjustment. Intrinsic reductions are applied before
  the tax, and kicker is added after the reduction. The shared Pauper prefix
  makes these omissions reachable even without changing the Standard decks.
- Recruitment Officer and Memory Deluge are Partial because bottomed cards keep
  looked-at order instead of the printed random order. Recruitment Officer's
  printed power/toughness is 2/1.
- Ward granted by Coppercoat Vanguard is not shown in observation features.
  Stack validation accepts a ward {1} trigger from any Human creature
  definition, because the grant may have ended by the time the trigger is
  checked.
- Target spec stable ids 50 and 51 follow FDN (42 to 46) and Standard family C
  (47 to 49).

Rules sources: [Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt),
[Dominaria United release notes](https://magic.wizards.com/en/news/feature/dominaria-united-release-notes-2022-08-26).

## Tests

`mtg-kernel/tests/standard_family_g_v1.rs` (Standard feature) checks every
definition's characteristics and trigger count. Behavior tests and life-payment
regressions cover stale sources, declined choices, empty candidate sets, counters
and LKI, uncounterable spells, priority-window casting with flash, and printed
affordability when Bloodletter modifies a Phyrexian payment. CI runs this suite
and the catalog suite, including the fixed identity and Partial admission checks.
