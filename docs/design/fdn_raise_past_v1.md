# FDN Raise the Past preparation

Issue110's next retained source family uses the existing cast/payment and
replacement-aware simultaneous zone-change machinery. Raise the Past is a
{2}{W}{W} sorcery returning all creature cards with mana value at most two
from its controller's current graveyard. It has no targets or optional choices.
Printing FDN22, Scryfall ID6c6be129-56da-4fe7-a6bd-6a1d402c09e1 and Oracle
IDa69a24d0-ca58-4a44-8af1-a3bd1608d2f9 were fetched from the
[Scryfall API](https://api.scryfall.com/cards/fdn/22). The clean pinned
[XMage implementation](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/r/RaiseThePast.java)
uses the same creature-card/mana-value filter.

The appended effect scans the controller's owner-keyed graveyard at resolution,
excludes definition/runtime tokens and virtual spell copies, reads current
card characteristics and printed mana values,
then submits every qualifying move as one shared replacement/commit batch.
Existing zone entry resets control, counters, tapping and incarnation, and
existing enter/dies/replacement rules remain authoritative. All returned
creatures are present before ordinary ETB collection. No private continuation
or serialized state field is added. Search has no embedded object reference;
the rules-vector record describes the graveyard read and mass return, with
explicit opacity for its mana-value bound and card/token distinction.

Two primitive cases cover both seats, exclusions, owner-keyed selection,
incarnation/reset semantics, empty eligible results, snapshot replay and
simultaneous Faerie Miscreant ETB collection. Three unregistered card cases
cover printed cost/program, failed payment rollback, resolution-time additions,
finalized-stack replay and pending-order replay with both Faerie draws.
Tentative ID355 follows the retained future IDs; no registry, fixture, catalog
identity, profile or accepted coverage changes. Native checks and card
qualification remain pending; source preparation is not gameplay acceptance.

Read-only review found the initial filter could return a virtual spell copy
before its next SBA. The filter now requires real cards, matching the existing
graveyard-card contract, and also rejects runtime token copies. The primitive
exclusion case retains both transient objects in the graveyard during execution.
Lunar Insight's inherited absent Mulldrifter operand is separately repaired to
supported Tolarian Terror with unchanged distinct-count assertions.
