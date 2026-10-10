# FDN library search, kicker and flashback v1

Second milestone-5 batch for issue 110 in the equipment, kicker, flashback
and library-search family. It adds four FDN reference names and the Rat
token Revenge of the Rats creates. Printed behavior was read from each
card's XMage file (`magefree/mage` master).

| Card | Cost | Type | Behavior |
| --- | --- | --- | --- |
| Campus Guide | {2} | Artifact Creature, Golem 2/1 | Enters: may search for a basic land, reveal it, then shuffle and put it on top |
| Burnished Hart | {3} | Artifact Creature, Elk 2/2 | {3}, sacrifice: search for up to two basic lands, put them onto the battlefield tapped, shuffle |
| Grow from the Ashes | {2}{G} | Sorcery | Kicker {2}; search for a basic land (two if kicked), put it onto the battlefield, shuffle |
| Revenge of the Rats | {2}{B}{B} | Sorcery | Create a tapped 1/1 black Rat for each creature card in your graveyard; flashback {2}{B}{B} |
| Rat Token | none | Token Creature, Rat 1/1 | Black; created only by Revenge of the Rats |

Engine changes append new variants and leave existing serialization alone:

- `EffectOp::SearchLibraryCardsToDestination` searches for up to
  `max_targets` cards matching a `LibraryCardFilter` and sends them to a
  `LibrarySearchDestinationV1`: the battlefield (tapped or untapped), or the
  top of the library after the shuffle. It uses the existing resumable
  library-search path: a `SelectTargets` choice with min 0 (the "may", or
  "up to"), a matching `EffectFrame`, and the same shuffle preflight,
  observation redaction and flat-action search-state projection as the
  single-card searches. The top-of-library destination accepts one card and
  reveals its library position to both players, since the card was revealed.
- Grow from the Ashes is `EffectCond::WasKicked` choosing between a two-card
  and a one-card search, using the existing kicker cast stage.
- `EffectOp::CreateTokensDynamic` creates a `DynamicValueDef` count of a
  token, optionally tapped. Revenge of the Rats counts
  `ControllerGraveyardCardsWithType(Creature)` at resolution, so the
  flashback cast also counts creature cards added since the first cast. The
  flashback uses the existing graveyard cast route and exile replacement.
- Burnished Hart's ability is a new `SearchLibraryUpToToBattlefield`
  activated-ability recipe over the same op.

`Elk` appends to `Subtype` and, under the Limited feature only, to
`Subtype::CREATURE_TYPES`; existing stable ids are unchanged.

The synthetic `FDN_reference_library_search.dck` (36 basics plus one copy of
each card) gives every new definition deck coverage. It is a correctness
fixture, not a Limited deck recommendation.

`mtg-kernel/tests/fdn_library_search_v1.rs` checks generated
characteristics, kicker and flashback costs; Campus Guide's basic-only
candidates, the found card on top after the shuffle, and declining; Burnished
Hart's mana and sacrifice cost, two tapped basics, and finding fewer; Grow
from the Ashes unkicked, declined-kicker and kicked counts with untapped
entry; and Revenge of the Rats' tapped Rats counting only the caster's
creature cards, nothing with none, and the flashback cast's exile.

Leyline Axe and Electroduplicate, the remaining family cards, need larger
engine work (opening-hand placement and token copies) and are left for a
later batch.
