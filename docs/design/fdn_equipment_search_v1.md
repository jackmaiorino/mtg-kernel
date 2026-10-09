# FDN equipment, kicker and library search v1

Milestone-5 batch for issue 110 covering the equipment, kicker and
library-search mechanic family. It adds eight FDN reference names. Each one's
printed behavior was read from its XMage card file (`jackmaiorino/mage`
master, path in the registry entry).

| Card | Cost | Type | Behavior |
| --- | --- | --- | --- |
| Burst Lightning | {R} | Instant | Kicker {4}; 2 damage to any target, 4 if kicked |
| Evolving Wilds | none | Land | {T}, sacrifice: search for a basic land, put it onto the battlefield tapped, shuffle |
| Solemn Simulacrum | {4} | Artifact Creature, Golem 2/2 | Enters: may search for a basic land, put it onto the battlefield tapped, shuffle; dies: may draw a card |
| Swiftfoot Boots | {2} | Artifact, Equipment | Equipped creature has hexproof and haste; equip {1} |
| Grim Tutor | {1}{B}{B} | Sorcery | Search for any card, put it into hand, shuffle; lose 3 life |
| Quick-Draw Katana | {2} | Artifact, Equipment | During your turn, equipped creature gets +2/+0 and has first strike; equip {2} |
| Adventuring Gear | {1} | Artifact, Equipment | Landfall: equipped creature gets +2/+2 until end of turn; equip {1} |
| Goldvein Pick | {2} | Artifact, Equipment | Equipped creature gets +1/+1; whenever it deals combat damage to a player, create a Treasure token; equip {1} |

Engine changes are small and reuse existing primitives:

- Burst Lightning uses the existing kicker cast stage and `EffectCond::WasKicked`,
  in the same shape as Galvanic Blast's metalcraft conditional.
- Evolving Wilds and Solemn Simulacrum use `SearchLibraryToBattlefieldTapped`
  with the `BasicLand` filter. Optional selection is the "may". A land with
  no mana ability is now admitted by codegen when it has an executable
  activated ability.
- Solemn's death trigger is a two-option `Choice` (decline or draw), like
  Ninja of the Deep Hours.
- `LibraryCardFilter::AnyCard` appends for Grim Tutor. Its search is the only
  one whose found card is not publicly revealed, matching the printed text
  (XMage `SearchLibraryPutInHandEffect(..., false, ...)`).
- `EquipmentDef::pt_controller_turn_only` appends for Quick-Draw Katana.
  Equipment profiles written before the field existed omit it from their
  `equipment_for` text, so their card-database canon is unchanged and the
  default Pauper identity stays at v34. The public continuous-effect
  observation reports the turn-dependent deltas.
- `EffectOp::BoostAttachedCreatureUntilEndOfTurn` appends for Adventuring
  Gear. Its `ControlledLandEnters` trigger reads the host at resolution, like
  XMage's `BoostEquippedEffect`: the Gear's current attachment while it is
  the same battlefield incarnation, otherwise its last-known attachment. An
  unattached Gear, or a host that has left, gets nothing.
- `TriggerCondition::EquippedCreatureDealsCombatDamageToPlayer` appends for
  Goldvein Pick. It matches a combat-damage marker whose source incarnation
  is the Pick's exact current host; the trigger is the Pick's own (controlled
  by the Pick's controller) and creates the existing Treasure token.

`Golem` appends to `Subtype` and, under the Limited feature only, to
`Subtype::CREATURE_TYPES`; existing stable ids are unchanged.

The synthetic `FDN_reference_equipment_search.dck` (32 basics plus one copy
of each card) gives every new definition deck coverage. It is a correctness
fixture, not a Limited deck recommendation.

`mtg-kernel/tests/fdn_equipment_search_v1.rs` checks generated
characteristics, costs and programs; Burst Lightning unkicked, kicked and
declined-kicker damage and instant timing; Evolving Wilds' basic-only
candidates, tapped entry and fail-to-find; Solemn's entry search and
optional death draw (and no draw on a non-graveyard exit); Boots' cast and
equip costs, sorcery timing, haste on a summoning-sick creature, hexproof
against an opposing spell and grant movement on re-equip and removal; Grim
Tutor's unrestricted, unrevealed search and life loss; Katana's
turn-dependent bonus; Gear's stacking landfall boost, its indifference to
opposing lands, an unattached Gear and a host lost before resolution; and the
Pick's static bonus and Treasure on unblocked combat damage.
