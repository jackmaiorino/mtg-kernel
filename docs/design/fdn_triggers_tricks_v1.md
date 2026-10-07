# FDN trigger creatures and tricks v1

Third milestone-5 batch for issue 110. Every card reuses an engine primitive
that an existing Pauper or FDN card already exercises; the only new generated
recipe is a plain targeted pump (`Special::PumpCreature`), which emits the same
`PumpTargetUntilEndOfTurnDynamic` op that Fleeting Distraction uses without its
draw.

| Card | Behavior | Engine recipe |
| --- | --- | --- |
| Helpful Hunter ({1}{W}, 1/1 Cat) | When it enters, draw a card | `Etb` + `DrawCards(Controller, 1)`, as Ichor Wellspring |
| Prideful Parent ({2}{W}, 2/2 Cat) | Vigilance; when it enters, create a 1/1 white Cat token | `Etb` + `CreateToken(Cat Token)` |
| Icewind Elemental ({4}{U}, 3/4 Elemental) | Flying; when it enters, draw a card, then discard a card | `Etb` + draw then `DiscardCards`, as Kiora |
| Burglar Rat ({1}{B}, 1/1 Rat) | When it enters, each opponent discards a card | `Etb` + `DiscardCards(Opponent, 1)` |
| Diregraf Ghoul ({B}, 2/2 Zombie) | Enters tapped | `enters_tapped` registry mechanic |
| Infestation Sage ({B}, 1/1 Elf Warlock) | When it dies, create a 1/1 black and green flying Insect token | `LeftBattlefieldToGraveyard` + `CreateToken(Insect Token)` |
| Wary Thespian ({1}{G}, 3/1 Cat Druid) | When it enters or dies, surveil 1 | `Etb` and `LeftBattlefieldToGraveyard` + `Surveil(1)`, as Conduit Pylons |
| Firebrand Archer ({1}{R}, 2/1 Human Archer) | Whenever you cast a noncreature spell, 1 damage to each opponent | Kessig Flamebreather's trigger table |
| Spitfire Lagac ({3}{R}, 3/4 Lizard) | Landfall: 1 damage to each opponent | `ControlledLandEnters` + Kessig's damage effect |
| Giant Growth ({G} instant) | Target creature gets +3/+3 until end of turn | `PumpCreature(3, 3)` |
| Stab ({B} instant) | Target creature gets -2/-2 until end of turn | `PumpCreature(-2, -2)` |
| Think Twice ({1}{U} instant) | Draw a card; flashback {2}{U} | `DrawCards(1)` + mana flashback |

Characteristics come from the XMage card files named in each registry entry
(`jackmaiorino/mage` master). In this two-player kernel, "each opponent" is the
single opponent. Lightshell Duo was considered and deferred because prowess is
not modeled yet.

The definitions append as ids 254 through 267, including Cat Token and Insect
Token. Archer and Lizard append to `Subtype` and join `CREATURE_TYPES` only
under the Limited feature; Insect already existed and gains its registry
spelling. `is_creature_type` now also covers every FDN-appended creature type (Angel, Noble, Unicorn, Homunculus and the later additions were missing),
so the changeling invariant holds under the feature. The feature catalog moves
to `kernel_carddb/v54` (`4076ff9c637705ce`) with a new `FdnTriggersTricks`
store profile; the gainland/life-gain profile stays readable and is refused at
publish and resume. Default builds keep Pauper v34. The synthetic
`FDN_reference_triggers_tricks.dck` provides deck coverage.

`mtg-kernel/tests/fdn_triggers_tricks_v1.rs` checks ids and characteristics,
exact creature costs, each trigger's resolution (draw, token, loot, opponent
discard including an empty hand, enters tapped, dies-only token, surveil on
enter and on death), that Firebrand Archer ignores creature spells and Spitfire
Lagac ignores the opponent's lands, Stab killing a 2/2 and shrinking a 3/4, and
Think Twice's flashback cost and exile.
