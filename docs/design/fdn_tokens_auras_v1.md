# FDN token makers and static creature Auras v1

Milestone-5 batch for issue 110 covering the token-making and Aura mechanic
family. Characteristics come from the XMage card files named in each registry
entry. In this two-player kernel, "each opponent" is the single opponent.

| Card | Behavior | Engine recipe |
| --- | --- | --- |
| Dragon Trainer ({3}{R}{R}, 1/1 Human) | When it enters, create a 4/4 red flying Dragon | `Etb` + `CreateToken(Dragon Token)` |
| Resolute Reinforcements ({1}{W}, 1/1 Human Soldier, flash) | When it enters, create a 1/1 white Soldier | `Etb` + `CreateToken(Soldier Token)` |
| Elfsworn Giant ({3}{G}{G}, 5/3 Giant, reach) | Landfall: create a 1/1 green Elf Warrior | `ControlledLandEnters` (Mossborn Hydra) + existing Elf Warrior token |
| Eager Trufflesnout ({2}{G}, 4/2 Boar, trample) | Combat damage to a player: create a Food | `DealsCombatDamageToPlayer` (Koma) + Pauper Food token |
| Rite of the Dragoncaller ({4}{R}{R} enchantment) | Whenever you cast an instant or sorcery, create a 5/5 red flying Dragon | `CastInstantOrSorcery` (Murmuring Mystic) |
| Heroic Reinforcements ({2}{R}{W} sorcery) | Create two 1/1 Soldiers, then creatures you control get +1/+1 and haste until end of turn | New `CreateTokensThenBoostControlled` special: tokens, then the Overrun snapshot boost |
| Goblin Surprise ({2}{R} instant) | Choose one: creatures you control get +2/+0 until end of turn; or create two 1/1 red Goblins | New `BoostControlledOrCreateTokens` special using the existing second-mode slot |
| Twinblade Blessing ({1}{W}{W} Aura, flash) | Enchanted creature has double strike | New `AttachmentDef::AuraCreatureStatic` |
| Blanchwood Armor ({2}{G} Aura) | Enchanted creature gets +1/+1 for each Forest you control | `AuraCreatureStatic` with a per-controlled-subtype multiplier |

New token definitions: Dragon Token (4/4 red flying Dragon), Dragon 5/5 Token
(5/5 red flying Dragon), Soldier Token (1/1 white Soldier) and Goblin Token
(1/1 red Goblin). Boar appends to `Subtype` and joins `CREATURE_TYPES` only
under the Limited feature.

Both Auras are cast through the Bind the Monster / Witness Protection
"put onto the battlefield attached to target creature" program. The new
static attachment grants layer-6 keywords and layer-7c power/toughness while
the Aura is validly attached to the host's current incarnation and its own
abilities are active. Granted keywords follow the existing timestamp rule
against ability removal, so Witness Protection removes double strike from an
earlier Twinblade Blessing but not from a later one. Blanchwood Armor counts
Forests the Aura's controller controls (not the host's controller), so on an
opponent's creature it still uses its caster's Forests. Its bonus applies on
top of Witness Protection's layer-7b base.

The definitions append as nine cards followed by the four tokens, with a new
`FdnTokensAuras` store profile and catalog identity bump; prior batch profiles
stay readable and are refused for mutation. Default builds keep Pauper v34.
The synthetic `FDN_reference_tokens_auras.dck` provides deck coverage.

`mtg-kernel/tests/fdn_tokens_auras_v1.rs` checks ids and characteristics;
exact costs; Dragon Trainer's token; Resolute Reinforcements casting at
instant speed while a sorcery-speed creature is not castable; Elfsworn Giant
triggering only on its controller's land entries; Trufflesnout's Food on
unblocked and trample damage but not when fully blocked; Rite of the
Dragoncaller triggering on its controller's instants and sorceries only;
Heroic Reinforcements boosting and hasting the new Soldiers and existing
creatures, letting summoning-sick creatures attack, and expiring at cleanup;
both Goblin Surprise modes and instant timing; Twinblade's flash, double strike
on either player's creature, two combat damage steps (two Food from
Trufflesnout) and ability-removal ordering; Blanchwood's live Forest count; and
an enchanted host dying.
