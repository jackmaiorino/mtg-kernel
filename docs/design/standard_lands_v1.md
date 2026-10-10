# MageZero Standard lands v1

First lands batch for the `standard-magezero-fixtures` catalog (families A and B of
`docs/reports/standard_magezero_inventory_v1.md`). It covers the four mono-deck lands that
the batch owns (Mishra's Foundry, Eiganjo, Seat of the Empire, Mirrex, Rockface Village)
and all 37 dual lands. Soulstone Sanctuary stays with the FDN threads; the Restless lands,
the other channel lands and the remaining utility lands are the second lands batch, which
reuses the animation and channel primitives added here.

Card text comes from the XMage card files named in each registry entry
(`magefree/mage` master). Every definition appends to `data/standard/magezero_v1/cards_v1.json`.

## Cards that reuse existing rules

| Cards | Behavior | Engine recipe |
| --- | --- | --- |
| Painlands: Adarkar Wastes, Battlefield Forge, Brushland, Caves of Koilos, Karplusan Forest, Llanowar Wastes, Shivan Reef, Sulfurous Springs, Underground River, Yavimaya Coast | `{T}`: add `{C}`; `{T}`: add either color, and the land deals 1 damage to you | Primary colorless ability stays on the automatic payment path. Each colored ability is its own `AdditionalManaAbilityDef` with `controller_damage: 1`, activated explicitly (Elves of Deep Shadow's damage path) |
| Surveil lands: Elegant Parlor, Lush Portico, Underground Mortuary | Two basic land types, enters tapped, ETB surveil 1 | Printed subtypes, `enters_tapped`, Conduit Pylons' `etb:surveil:1` trigger |
| Triomes: Jetmir's Garden, Spara's Headquarters, Ziatora's Proving Ground | Three basic land types, enters tapped, Cycling `{3}` | Printed subtypes, `enters_tapped`, Twisted Landscape's hand-zone cycling recipe |

## New rules primitives

**Conditional entry** (`CardDef::enters_tapped_unless_controller`). Fastlands (Blackcleave
Cliffs, Blooming Marsh, Concealed Courtyard, Copperline Gorge, Darkslick Shores, Inspiring
Vantage, Razorverge Thicket, Seachrome Coast, Spirebluff Canal) enter tapped unless their
controller controls two or fewer other lands. Slowlands (Deserted Beach, Dreamroot Cascade,
Haunted Ridge, Overgrown Farmland, Rockfall Vale) enter tapped unless they control two or
more other lands. Starting Town enters tapped unless it is its controller's turn and that
player's first, second or third turn (XMage `StartingTownCondition`: active player and
`Player.getTurns() <= 3`; in this two-player kernel without extra turns that is
`active_player == controller && turn <= 3`). The new field sits beside Gingerbread Cabin's
existing `enters_battlefield_tapped_unless`, so that definition's identity is unchanged.

**Conditional mana abilities** (`CardDef::additional_mana_ability_conditions`, parallel to
`additional_mana_abilities`). Verges (Floodfarm, Gloomlake, Hushwood, Riverpyre, Thornspire,
Wastewood) keep their unconditional color on the automatic path; the second color is an
explicit ability legal only while the controller controls a land with either named basic
land type, matching XMage's `ActivateIfConditionManaAbility` over
`PermanentsOnTheBattlefieldCondition`. Mirrex's any-color ability is legal only if Mirrex
entered the battlefield this turn. The kernel's `entered_battlefield_turn` is a round
number shared by both players' turns, so Standard builds also mark
`ObjectStateV4::entered_battlefield_this_turn` on entry and clear it at every untap step.

**Pay-life mana cost** (`ManaAbilityCostDef::TapSelfPayLife(n)`). Starting Town's
`{T}, Pay 1 life: Add one mana of any color.` is a life payment, not damage: it needs
`life >= 1` (119.4) and is not damage for prevention or lifelink.

**Creature-only mana** (`CardDef::restricted_mana_abilities`). Rockface Village's
`{T}: Add {R}. Spend this mana only to cast a creature spell.` is never offered as an
explicit action, because the floating pool has no way to carry a spending restriction.
Instead the payment planner (`mana::can_pay_spell`) adds the restricted color to the
land's source choices only while it pays the total cost of a creature card cast normally,
with Kicker or Delve included. Bestow, Adventure and Omen forms and X costs do not use it
yet; none of those spells is in the Standard catalog. The player
loses the option of floating restricted mana ahead of time, which never makes an illegal
play legal. The lands in the second batch with "spend only on creature spells" or
"legendary spells" mana (Lupinflower Village, Mudflat Village, Plaza of Heroes) reuse it.

**Land animation** (`CardDef::animation`, `ObjectStateV4::animation`). Mishra's Foundry's
`{2}` ability resolves `EffectOp::AnimateSource`, which records the source incarnation's
animation timestamp. While it lasts the object is also an Artifact Creature with base
power and toughness 2/2, the Assembly-Worker subtype and its printed colors, "still a
land". All engine creature and artifact queries for battlefield objects go through the
existing effective-characteristics helpers (`object_has_type`, `effective_base_power`,
`effective_base_toughness`, `effective_subtype_ids`, `has_effective_subtype`,
`object_color_mask`, `has_effective_keyword`); the remaining printed-type reads for
battlefield objects in combat, state-based actions, targeting, mana sources and
activation costs move to those helpers. An Aura creature override (FDN's
`creature_override`) on an animated land replaces the animation until the Aura leaves; no
card in either pool can put both on one permanent yet. The animation ends at cleanup (514.2) or when the
object changes zones. Summoning sickness applies to the animated land's attacks and tap
abilities through the ordinary `summoning_sick` flag, which every permanent already
carries.

Mishra's Foundry's `{1}, {T}: Target attacking Assembly-Worker gets +2/+2 until end of
turn` uses the new `TargetSpec::AttackingCreatureWithSubtype(Subtype)`.

**Channel with legendary cost reduction.** Eiganjo's Channel is a hand-zone activated
ability with `{2}{W}` plus `DiscardSelf`, dealing 4 damage to target attacking or blocking
creature (`TargetSpec::AttackingOrBlockingCreature`). Its cost is reduced by `{1}` for each
legendary creature its controller controls (XMage `LegendaryCreatureCostAdjuster`, generic
mana only), recorded in `CardDef::activated_ability_generic_reductions` and applied in both
the activation affordability check and payment.

**Toxic and poison.** Mirrex's `{3}, {T}` ability creates a Phyrexian Mite token: a 1/1
colorless Phyrexian Mite artifact creature with toxic 1 and "This creature can't block".
Players gain `poison_counters`; combat damage a creature with toxic N deals to a player also
gives that player N poison counters (702.164c), and a player with ten or more poison
counters loses the game as a state-based action (704.5c). `Keywords::TOXIC_1` is a new
keyword bit. "Can't block" reuses family D's name-keyed rule
(`standard_keywords_v1::cant_block`, Forsaken Miner's).

Rockface Village's `{R}, {T}` sorcery-speed ability gives target Lizard, Mouse, Otter or
Raccoon you control +1/+0 and haste until end of turn
(`TargetSpec::ControlledPermanentWithAnySubtype`, stable id 56; Foundry's
`AttackingCreatureWithSubtype` is 55).

## Identity

New subtypes append to `Subtype` (Assembly-Worker, Mite, Otter, Sphere, Town; family D
already added Mouse); the creature types are added to `CREATURE_TYPES` only under the Standard feature. New `CardDef`
fields are appended with empty defaults and enter the catalog contract only when a card
sets them, so the Pauper (`kernel_carddb/v34`) and FDN Limited identities do not move. The
Standard catalog moves to `kernel_carddb_standard/v5`. New object and player state
(`animation_timestamp`, `entered_battlefield_this_turn`, `poison_counters`) is skipped on the wire
and in state hashes while empty, so existing states keep their bytes.

## Tests

`mtg-kernel/tests/standard_lands_v1.rs` checks every land's characteristics, each entry
condition on both sides of its threshold, painland damage per color, verge and Mirrex
conditions, Starting Town's life payment, Rockface Village's red mana paying a creature
spell and not a noncreature spell, Foundry animating, attacking, being pumped and reverting
at cleanup (including summoning sickness and lethal damage), Eiganjo's channel at full
cost and one cheaper with Adeline, Resplendent Cathar (an opponent's legend never counts),
Rockface Village pumping Manifold Mouse at sorcery speed, triome cycling, surveil-land
entry, and a Mite's toxic damage and blocking restriction through the poison loss.

## Rules vector

The rules-vector extractor (`rules_vector_v1`) reads every new field: the entry conditions
and Eiganjo's reduction as static abilities, the verge and Mirrex conditions on their mana
abilities, Rockface's creature-only red as a conditional mana ability, Starting Town's life
payment as a cost, and Mishra's Foundry's animation in the definition record with
`EffectOp::AnimateSource` in its own meaning slice (`meaning/effect_i.rs`).
