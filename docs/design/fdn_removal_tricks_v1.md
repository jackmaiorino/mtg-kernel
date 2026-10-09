# FDN removal, damage and combat tricks v1

Milestone-5 batch for issue 110 covering the removal, damage and pump mechanic
family. Characteristics come from the XMage card files named in each registry
entry. In this two-player kernel, "each opponent" is the single opponent.

| Card | Behavior | Engine recipe |
| --- | --- | --- |
| Sure Strike ({1}{R} instant) | Target creature gets +3/+0 and first strike until end of turn | `Program`: pump + keyword grant |
| Snakeskin Veil ({G} instant) | Put a +1/+1 counter on target creature you control; it gains hexproof until end of turn | `Program`: counter + keyword grant |
| Seismic Rupture ({2}{R} sorcery) | 2 damage to each creature without flying | `DamageAllCreatures` with `WithoutKeyword(FLYING)` |
| Boltwave ({R} sorcery) | 3 damage to each opponent | `DealDamage` to the opponent |
| Day of Judgment ({2}{W}{W} sorcery) | Destroy all creatures | New `EffectOp::DestroyAllCreatures` |
| Incinerating Blast ({4}{R} sorcery) | 6 damage to target creature, then you may discard a card; if you do, draw a card | Damage, then `MayPayCostThen` (discard, draw) |
| Slagstorm ({1}{R}{R} sorcery) | Choose one: 3 damage to each creature; or 3 damage to each player | Two modes; new `CreatureFilter::All` and `TargetRef::Controller` |
| Abrade ({1}{R} instant) | Choose one: 3 damage to target creature; or destroy target artifact | Two modes with separate target specs |
| Preposterous Proportions ({4}{G}{G} sorcery) | Creatures you control get +10/+10 and vigilance until end of turn | Controlled-creature boost snapshot |
| Bake into a Pie ({2}{B}{B} instant) | Destroy target creature and create a Food | Conditional destroy, then the Food token |
| Hero's Downfall ({1}{B}{B} instant) | Destroy target creature or planeswalker | New `TargetSpec::CreatureOrPlaneswalker` (42) |
| Broken Wings ({2}{G} instant) | Destroy target artifact, enchantment or creature with flying | New `ArtifactEnchantmentOrFlyingCreature` (43) |
| Make Your Move ({2}{W} instant) | Destroy target artifact, enchantment or creature with power 4 or greater | New `ArtifactEnchantmentOrCreaturePowerAtLeastFour` (44) |
| Meteor Golem ({7}, 3/3 artifact Golem) | When it enters, destroy target nonland permanent an opponent controls | `Etb` trigger with new `OpponentNonlandPermanent` (45) |
| Reclamation Sage ({2}{G}, 2/1 Elf Shaman) | When it enters, you may destroy target artifact or enchantment | `Etb` trigger with an optional choice |
| Fanatical Firebrand ({R}, 1/1 Goblin Pirate, haste) | {T}, sacrifice it: 1 damage to any target | Activated ability with tap and sacrifice costs |
| Dauntless Veteran ({1}{W}{W}, 2/2 Human Soldier) | Whenever it attacks, creatures you control get +1/+1 until end of turn | `Attacks` trigger with a controlled boost |
| Crackling Cyclops ({2}{R}, 0/4 Cyclops Wizard) | Whenever you cast a noncreature spell, it gets +3/+0 until end of turn | `CastNoncreatureSpell` trigger bound to the source |

The thirteen spells use a new table-driven `Special::Program` in `build.rs`:
each entry names a target spec, a hashed recipe token and an `EffectOp`
program, with an optional second mode. Targeted destroy programs check that
the target is still in its zone, so a removed target makes the spell do
nothing. Cyclops appends to `Subtype` and joins `CREATURE_TYPES` only under the
Limited feature; Golem was already added by the equipment batch.

One shared engine fix: a creature's `{T}` activation cost now respects haste,
so Fanatical Firebrand can be activated the turn it enters. Mana abilities
keep their existing check.

The definitions append as ids 289 to 306 with a new `FdnRemovalTricks` store
profile and catalog identity bump to v57; prior batch profiles stay readable
and are refused for mutation. Default builds keep Pauper v34. The synthetic
`FDN_reference_removal_tricks.dck` provides deck coverage.

`mtg-kernel/tests/fdn_removal_tricks_v1.rs` checks ids, characteristics and
costs, then each card's behavior: the pump and keyword expiry, hexproof and the
counter, flying creatures surviving Seismic Rupture, Boltwave's damage, Day of
Judgment sparing indestructible creatures, Incinerating Blast's optional
discard gate (accepted and declined), both Slagstorm and Abrade modes and
Abrade's single viable mode selected silently, the +10/+10 boost, the Food from
Bake into a Pie, the legal target sets for the three new specs, Make Your Move
seeing pumped power and doing nothing once the target's power drops below 4,
targeted spells doing nothing when their target leaves, Meteor Golem offering only opponent
nonland permanents (not an artifact land), Reclamation Sage accept and decline,
Firebrand activating with haste, Veteran's attack boost and Cyclops triggering
on noncreature spells only.
