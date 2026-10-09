# FDN counterspells and simple threats v1

Fourth milestone-5 batch for issue 110, from the milestone owner's share of
the tier-1 inventory (keyword and simple trigger cards). Every card reuses an
engine primitive that an existing Pauper or FDN card already exercises. The
new pieces are one target filter (`TargetSpec::CreatureSpellOnStack`, stable
id 46) and two generated spell sequences built from existing ops.

| Card | Behavior | Engine recipe |
| --- | --- | --- |
| Elementalist Adept ({1}{U}, 2/1 Human Wizard) | Flash; prowess | `CastNoncreatureSpell` + `BindTemporaryBoostToTriggerSource(1, 1)` |
| Crypt Feaster ({3}{B}, 3/4 Zombie) | Menace; threshold attack trigger for +2/+0 | `AttacksWithControllerGraveyardCardCountAtLeast(7)` with a resolution recheck, as Kiora |
| Erudite Wizard ({2}{U}, 2/3 Human Wizard) | Second card drawn each turn puts a +1/+1 counter on it | `DrawNth(2)` + the Writhing Chrysalis counter marker |
| Phyrexian Arena ({1}{B}{B} enchantment) | At your upkeep, draw a card and lose 1 life | `BeginningOfUpkeep { controller_only }`, as Delver of Secrets |
| Gleaming Barrier ({2}, 0/4 artifact Wall) | Defender; when it dies, create a Treasure | `LeftBattlefieldToGraveyard` + the Pauper Treasure Token |
| Angel of Finality ({3}{W}, 3/4 Angel) | Flying; when it enters, exile target player's graveyard | Bojuka Bog's trigger table |
| Bigfin Bouncer ({3}{U}, 3/2 Shark Pirate) | When it enters, return target creature an opponent controls to its owner's hand | `Etb` + `MoveObject(Target, Hand)` with Humbling Elder's target filter |
| Essence Scatter ({1}{U} instant) | Counter target creature spell | `CounterTarget` with the new creature-spell filter |
| Refute ({1}{U}{U} instant) | Counter target spell, draw a card, then discard a card | `CounterTargetThenLoot`: the shared counter program, then draw 1 and discard 1 |
| Macabre Waltz ({1}{B} sorcery) | Return up to two creature cards from your graveyard to your hand, then discard a card | `ReturnCreatureCardsThenDiscard`: Blood Fountain's target filter, `MoveAllTargets(Hand)`, then discard 1 |

Characteristics come from the XMage card files named in each registry entry
(`jackmaiorino/mage` master). In this two-player kernel, "each opponent" is
the single opponent. Shivan Dragon and Rune-Sealed Wall were left out because
activated self-pumps and activated surveil are not generated recipes yet.
Lightshell Duo still waits on a rules check of multi-card surveil.

The definitions append ten ids after the previous batch. Shark appends to
`Subtype` and joins `CREATURE_TYPES` only under the Limited feature. Crackling
Cyclops and Dauntless Veteran were first built here and moved to the
removal/pumps batch, which implements them the same way and merges first.
The feature catalog moves to `kernel_carddb/v58` (`f248c83c818bd26a`) with a new
`FdnCountersThreats` store profile; the previous profile stays readable and is
refused at publish and resume. Default builds keep Pauper v34. The synthetic
`FDN_reference_counters_threats.dck` provides deck coverage.

`mtg-kernel/tests/fdn_counters_threats_v1.rs` checks ids and characteristics,
exact permanent costs, flash and prowess timing, Crypt Feaster at six and
seven graveyard cards, Erudite Wizard growing once per turn, the Barrier's
defender and Treasure, Phyrexian Arena's upkeep drain, the Angel's player target, the Bouncer's
opponent-only target and its no-target case, Essence Scatter refusing a
noncreature spell, Refute's loot and Macabre Waltz's two-target return.
