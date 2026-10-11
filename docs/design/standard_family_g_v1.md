# MageZero Standard family G v1: trigger and static creatures

Family G of the MageZero Standard inventory covers creatures with triggered and
static abilities. The v7 completion candidate includes all 48 listed family G cards,
including shared FDN creatures and the two-color additions. The registry entries are
Full admission candidates; combined native/runtime verification remains pending in
`docs/reports/standard_completion_v1.md`.

Definitions keep their append order in `data/standard/magezero_v1/cards_v1.json`.
The behavior tables in `build.rs`, triggers in `src/trigger/standard_family_g_v1.rs`,
statics in `src/standard_statics_v1.rs`, and choices/upgrades in
`src/standard_creatures_v1.rs` and `src/standard_creature_choices_v1.rs` implement the
cards. Pauper and FDN catalogs retain their separate identities. Assassin and Mercenary
remain part of `Subtype::OUTLAW_TYPES` for Shoot the Sheriff.

The original mono-color batch uses these recipes:

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
- Static spell-cost adjustments apply to total costs, including alternative
  routes, additional costs and resolving free casts. Free casting waives only
  the base mana cost; Thalia's tax and applicable reductions remain relevant.

- The rules-vector extractor maps the new effect ops in a new slice,
  `rules_vector_v1/meaning/effect_g.rs`, and the new trigger conditions, cost
  component and target specs in the existing tables.

## Completion changes and admission

- Quirion Beastcaller binds targeted recipients and allocations when its death
  trigger is placed, then validates those targets during resolution.
- Extraction Specialist's restriction ends permanently when its source leaves
  or its controller first loses control of that source incarnation.
- Sharp-Eyed Rookie and Evolving Adaptive use frozen creature characteristics
  when the entrant has departed before their intervening-if recheck.
- Recruitment Officer and Memory Deluge randomly order the remainder with the
  deterministic game RNG after the controller chooses the cards to keep.
- Ward grants and the added persistent restrictions have public observation
  support. The new choices use the generic decision projection; frozen policy
  encoders refuse representations they cannot encode completely.
- The additional creatures and Adventures have focused behavior tests in the
  same family suite. Full flags permit those integration acceptance checks;
  they are not runtime verification receipts.

Rules sources: [Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt),
[Dominaria United release notes](https://magic.wizards.com/en/news/feature/dominaria-united-release-notes-2022-08-26).

## Tests

`mtg-kernel/tests/standard_family_g_v1.rs` (Standard feature) checks every
definition's characteristics and trigger count. Behavior tests and life-payment
regressions cover stale sources, declined choices, empty candidate sets, counters
and LKI, uncounterable spells, priority-window casting with flash, and printed
affordability when Bloodletter modifies a Phyrexian payment. CI runs this suite
and the catalog suite, including the observed v7 identity and all-deck admission checks.
Combined executable verification is pending.
