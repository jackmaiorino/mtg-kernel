# FDN simple triggers and graveyard spells v1

Fifth milestone-5 batch for issue 110, from the milestone owner's share of
the tier-1 inventory (keyword and simple trigger cards). Four cards reuse an
engine primitive that an existing Pauper or FDN card already exercises. The
one new piece is a generated activated-ability recipe,
`AbilityEffectRecipe::Surveil`, which emits the existing `EffectOp::Surveil`.

| Card | Behavior | Engine recipe |
| --- | --- | --- |
| Zombify ({3}{B} sorcery) | Return target creature card from your graveyard to the battlefield | Dread Return's `ReturnOwnGraveyardCreatureToBattlefield`, without its flashback |
| Rune-Scarred Demon ({5}{B}{B}, 6/6 Demon) | Flying; when it enters, search your library for a card, put it into your hand, then shuffle | `Etb` + Grim Tutor's unrevealed `SearchLibraryToHand(AnyCard)` |
| Tatyova, Benthic Druid ({3}{G}{U}, legendary 3/3 Merfolk Druid) | Whenever a land you control enters, gain 1 life and draw a card | `ControlledLandEnters`, as Spitfire Lagac, with a gain-then-draw sequence |
| Thrill of Possibility ({1}{R} instant) | As an additional cost, discard a card; draw two cards | Grab the Prize's `DiscardCards(1)` additional cost + `DrawCards(2)` |
| Rune-Sealed Wall ({2}{U}, 0/6 artifact Wall) | Defender; {T}: Surveil 1 | `Tap` cost + the new `Surveil(1)` ability recipe |

Characteristics come from the XMage card files named in each registry entry
(`jackmaiorino/mage` master). Lightshell Duo still waits on multi-card
surveil: the current `EffectOp::Surveil` re-reads the library top after each
kept card, which is correct for surveil 1 only. Vanguard Seraph needs a
first-life-gain-each-turn trigger, Shivan Dragon an activated self-pump and
Exsanguinate an X spell value; none of these exists yet.

The definitions append five ids, 322-326, after the v59 batch. Demon appends
to `Subtype` and joins `CREATURE_TYPES` only under the Limited feature. The
feature catalog moves to `kernel_carddb/v60` (`63e4c9bf208f49e0`) with a new
`FdnSimpleTriggers` store profile; the previous `FdnLibrarySearch` profile
stays readable and is refused at publish and resume. Default builds keep
Pauper v34. The synthetic `FDN_reference_simple_triggers.dck` provides deck
coverage.

`mtg-kernel/tests/fdn_simple_triggers_v1.rs` checks ids and
characteristics, deck resolution, Zombify's own-graveyard creature target
(and that it is uncastable without one), the Demon's unrevealed tutor,
Tatyova's trigger on its controller's land drop but not an opponent's,
Thrill's discard cost (and that it is uncastable with an empty hand), and
the Wall's tap-to-surveil and defender.
