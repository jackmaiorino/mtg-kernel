# FDN keyword-only creatures v1

First milestone-5 batch for issue 110. It adds the seven remaining FDN
reference creatures whose complete printed behavior is a mana cost, creature
types, power/toughness and static keywords the engine already enforces:

| Card | Cost | Types | P/T | Keywords |
| --- | --- | --- | --- | --- |
| Aegis Turtle | {U} | Turtle | 0/5 | none |
| Brazen Scourge | {1}{R}{R} | Gremlin | 3/3 | haste |
| Quakestrider Ceratops | {3}{G}{G}{G} | Dinosaur | 12/8 | none |
| Savannah Lions | {W} | Cat | 2/1 | none |
| Serra Angel | {3}{W}{W} | Angel | 4/4 | flying, vigilance |
| Swiftblade Vindicator | {R}{W} | Human Soldier | 1/1 | double strike, vigilance, trample |
| Vampire Nighthawk | {1}{B}{B} | Vampire Shaman | 2/3 | flying, deathtouch, lifelink |

Characteristics were read from the XMage card files named in each registry
entry (`jackmaiorino/mage` master). Elementalist Adept is deliberately
excluded: prowess is not implemented.

The definitions append as ids 236 through 242 after the unchanged Pauper
prefix and earlier FDN ids. Turtle, Gremlin and Dinosaur append to `Subtype`
and, under the Limited feature only, to `Subtype::CREATURE_TYPES`; existing
stable ids are unchanged. The feature catalog moves to `kernel_carddb/v52`
(`77a197d40460bf5c`) with a new `FdnKeywordCreatures` training-store profile.
The rebased Witness Protection profile stays readable and is refused for
mutation like every earlier FDN profile. Default builds keep the Pauper v34
identity.

Every non-token registry definition needs deck coverage, so the batch adds the
synthetic `FDN_reference_keyword_creatures.dck` (33 basics plus one copy of
each card). It is a correctness fixture, not a Limited deck recommendation.

`mtg-kernel/tests/fdn_keyword_creatures_v1.rs` checks the generated
characteristics and ids, exact and insufficient casting costs and sorcery
timing, haste versus summoning sickness, flying evasion and vigilance,
deathtouch with lifelink against the 12/8 Ceratops, the Turtle's toughness,
and Vindicator's double strike in both legacy and Foundations combat,
including a first-strike kill followed by trample damage to the player.
