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
catalog identity covers them. Pauper and FDN canon never include the Standard
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
| Extraction Specialist | Lifelink; ETB return a creature card with MV 2 or less from your graveyard; it can't attack or block while you control the Specialist | `TargetSpec` 42 + attack/block restriction record |
| Hullbreaker Horror | Flash; can't be countered; you cast a spell: return target spell you don't control or target nonland permanent to hand, or neither | `CastSpell` + placement-time modes, `TargetSpec` 43 |
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
- New target specs `CreatureCardInOwnGraveyardManaValueAtMost` (stable id 42)
  and `SpellYouDontControl` (43).
- Paying life is now a distinct life-loss proposal, so life-loss modifiers leave
  payments alone.
- Normal casts (including kicker and delve) and the Adventure, Omen and Bestow
  forms pass their cost through one static adjustment. Increases apply first,
  and a reduction only reduces generic mana.

## Deviations from the printed cards

- Quirion Beastcaller distributes at resolution among the creatures its
  controller controls then, one counter at a time, without targeting.
- Ward granted by Coppercoat Vanguard is not shown in observation features.
  Stack validation accepts a ward {1} trigger from any Human creature
  definition, because the grant may have ended by the time the trigger is
  checked.
- Thalia and Haughty Djinn do not adjust flashback, escape, madness or other
  alternative costs. None of those appear in the Standard pool's mono decks.
- Extraction Specialist's restriction is keyed to the exact Specialist
  incarnation and its controller at resolution. If that player loses and then
  regains control of the same Specialist, the restriction applies again.
- Recruitment Officer puts the rest on the bottom in the order they were looked
  at, not a random order. The kernel only advances randomness through library
  shuffles, and the order of the bottom cards matters only in games that draw
  through the whole library.
- Target spec stable ids 42 and 43 may collide with ids appended by parallel
  Standard or FDN threads. The second thread to merge renumbers its ids.

## Tests

`mtg-kernel/tests/standard_family_g_v1.rs` (Standard feature) checks every
definition's characteristics and trigger count, and covers each card's behavior
with 49 tests, including the edge cases above (stale sources, declined choices,
empty candidate sets, counters and LKI, uncounterable spells, priority-window
casting with flash).
