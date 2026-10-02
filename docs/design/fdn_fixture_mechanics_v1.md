# FDN fixture implementation batches

This inventories the **36 missing names** in the two pinned 40-card fixtures.
It is an implementation dependency list, not a support declaration. None of
these names is added to the registry by the priority-window PR.

Sources: the pinned `.dck` files under `data/limited/fdn_v1/`, the corresponding
XMage card implementations at `a5c90fe180021e70e2a644ade00eeab07f857a40`, and
[Wizards' FDN release notes](https://magic.wizards.com/en/news/feature/foundations-release-notes).
XMage paths below are relative to `Mage.Sets/src/mage/cards/`. Review the actual
implementation and current Oracle text again when registering each card.

UG and WG denote fixture membership. Forest, Island and Llanowar Elves already
resolve; Plains does not. Kiora, the Rising Tide is a creature, so planeswalker
support is a dependency of the wider pool rather than these two fixtures.

| Batch | Card | Deck | Required behavior | XMage source |
| --- | --- | --- | --- | --- |
| A | Plains | WG | Basic land; white mana; unlimited copies. | Basic-land implementation |
| A | Healer's Hawk | WG | Flying and lifelink, including combat damage. | `h/HealersHawk.java` |
| A | Fleeting Distraction | UG | Targeted temporary power reduction, then draw; illegal-target handling. | `f/FleetingDistraction.java` |
| A | Cathar Commando | WG | Flash; mana plus sacrifice activation cost; artifact/enchantment destruction. | `c/CatharCommando.java` |
| A | Spectral Sailor | UG | Flash, flying and a repeatable mana-cost draw ability. | `s/SpectralSailor.java` |
| A | Treetop Snarespinner | WG | Reach, deathtouch, targeted counters and sorcery-only activation. | `t/TreetopSnarespinner.java` |
| B | Blossoming Sands | WG | Enter tapped, ETB life gain, white/green mana choice. | `b/BlossomingSands.java` |
| B | Thornwood Falls | UG | Enter tapped, ETB life gain, blue/green mana choice. | `t/ThornwoodFalls.java` |
| B | Dazzling Angel | WG | Flying; another friendly creature entering causes life gain. | `d/DazzlingAngel.java` |
| B | Clinquant Skymage | UG | Flying; each draw causes a counter trigger. | `c/ClinquantSkymage.java` |
| B | Dwynen's Elite | UG | Intervening Elf condition at ETB and resolution; Elf Warrior token. | `d/DwynensElite.java` |
| B | Good-Fortune Unicorn | WG | Counter on the specific other creature that entered; incarnation-safe reference. | `g/GoodFortuneUnicorn.java` |
| B | Guarded Heir | WG | Lifelink; two Knight tokens on entry. | `g/GuardedHeir.java` |
| B | Youthful Valkyrie | WG | Flying; another friendly Angel entering causes a counter. | `y/YouthfulValkyrie.java` |
| C | Beast-Kin Ranger | UG | Trample; temporary power trigger for each other friendly creature entering. | `b/BeastKinRanger.java` |
| C | Bite Down | UG | Two targets with different controller constraints; creature power damage to a creature or planeswalker. | `b/BiteDown.java` |
| C | Felling Blow | UG, WG | Counter before measuring power; creature-sourced damage; partial target legality. | `f/FellingBlow.java` |
| C | Fleeting Flight | WG | Counter, temporary flying and combat-damage prevention on the same target. | `f/FleetingFlight.java` |
| C | Joust Through | WG | Attacking/blocking target restriction; damage then life gain. | `j/JoustThrough.java` |
| C | Overrun | WG | Temporary team power/toughness increase and trample. | `o/Overrun.java` |
| C | Dwynen, Gilt-Leaf Daen | UG | Reach; continuous Elf bonus excluding itself; attacking-Elf count life trigger. | `d/DwynenGiltLeafDaen.java` |
| D | Gnarlid Colony | WG | Kicker; enter with counters; continuously grant trample to friendly creatures with counters. | `g/GnarlidColony.java` |
| D | Mossborn Hydra | UG | Enter with a counter; landfall doubles its current counters; trample. | `m/MossbornHydra.java` |
| D | Exemplar of Light | WG | Flying; life-gain counter trigger; counter-event draw trigger limited once per turn. | `e/ExemplarOfLight.java` |
| D | Sun-Blessed Healer | WG | Kicker and lifelink; kicked ETB returns a targeted small nonland permanent. | `s/SunBlessedHealer.java` |
| E | Cackling Prowler | WG | Ward payment trigger; end-step intervening creature-died condition; counter. | `c/CacklingProwler.java` |
| E | Mischievous Mystic | UG | Flying; second-draw event per player per turn; Faerie token. | `m/MischievousMystic.java` |
| E | Strix Lookout | UG | Flying, vigilance; mana/tap activation; draw then chosen discard. | `s/StrixLookout.java` |
| E | Kiora, the Rising Tide | UG | ETB draw then discard; attacking threshold condition; optional legendary Octopus token. | `k/KioraTheRisingTide.java` |
| E | Sylvan Scavenging | WG | End-step modal trigger; targeted counter or conditional Raccoon token. | `s/SylvanScavenging.java` |
| F | Koma, World-Eater | UG | Uncounterable spell, ward payment, trample, combat-damage-to-player trigger and four Serpent tokens. | `k/KomaWorldEater.java` |
| F | Luminous Rebuke | WG | Cost discount conditioned on selected target's tapped state; creature destruction. | `l/LuminousRebuke.java` |
| G | Homunculus Horde | UG | Second-draw trigger; copy token preserving copiable characteristics and abilities. | `h/HomunculusHorde.java` |
| G | Celestial Armor | WG | Flash Equipment; targeted ETB attachment; temporary hexproof/indestructible; continuous bonus/flying; equip. | `c/CelestialArmor.java` |
| G | Uncharted Voyage | UG | Target owner's top/bottom choice; surveil choice; hidden-zone and library-order handling. | `u/UnchartedVoyage.java` |
| G | Witness Protection | UG | Aura; layered name, color, subtype, base stats and ability removal; interaction with other effects. | `w/WitnessProtection.java` |

Batch A has 6 names, B has 8, C has 7, D has 4, E has 5, F has 2 and G has 4.
Register tokens with their parent mechanic batch, rather than leaving their
creation to an unsupported or approximate definition.

Implement each batch through generic operations. Existing keyword flags,
`EffectOp` entries, draw counters, attachment state and trigger machinery are
starting points; they do not prove these new cards' entire behavior works.
Before marking a card fully supported, test every reachable branch, including
target loss, costs, repeat activations and relevant event ordering.

The next card slice is A. Its acceptance is six resolving names with targeted
rules tests, including flash in the new windows, lifelink during combat,
sacrifice paid even when the ability's target later becomes illegal, and the
sorcery timing restriction. It does not depend on copying, layered Aura
transformation or the unusual cards in G.

Batch C requires explicit current damage allocation and trample tests.
Wizards removed damage assignment order for FDN; arbitrary multi-blocker
allocation must be a real choice. Bite Down also needs planeswalker targeting
before its complete rules behavior can be declared supported. D and E build
on event-driven counters and predicates; F shares E's ward implementation.
G should land as separate mechanic slices, each with interaction tests and
pending-choice restoration checks.

After every batch, regenerate the fixture coverage inventory. The final
fixture gate remains two complete real-deck games plus targeted XMage
comparisons. A land-only smoke or successful card-name resolution cannot
satisfy that gate.
