# FDN Rise of the Dark Realms preparation

Issue110's retained follow-on family is an untargeted {7}{B}{B} sorcery putting
all creature cards from all graveyards onto the battlefield under its
controller's control. Printing FDN183 has Scryfall
ID8645bf0c-631f-4003-bd24-3e069ae23513 and Oracle
IDe5223a09-f732-4747-8914-e6546ab0ef4c, verified with the
[Scryfall API](https://api.scryfall.com/cards/fdn/183) and the clean pinned
[XMage implementation](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/r/RiseOfTheDarkRealms.java).

The appended effect samples both owner-keyed graveyards at resolution. It
excludes definition/runtime tokens and virtual spell copies and submits every
creature card through one shared replacement/commit batch. An event constructor
sets the existing battlefield-controller field without changing the serialized
proposal shape. Ordinary entry resets incarnation/tapping/counters and assigns
the destination battlefield membership to the effect controller; ownership
remains unchanged for later departure. Both sides' creatures see the complete
entry group when ETB triggers are collected. No state/continuation field changes.
Search has no embedded object references. Rules facets record each-player
graveyard reads, mass movement and control acquisition, with opacity for the
card/token distinction and simultaneous entry.

One primitive case exercises both seats, small/large creatures, both graveyards,
card/token/copy exclusions, empty eligible results, exact entry incarnations,
simultaneous Faerie Miscreant triggers, restore and subsequent return to the
physical owner's hand. Two unregistered card cases cover cost/program/payment
rollback, finalized-stack and pending-order replay, both controlled Faerie draws
and later stolen-creature departure. Tentative ID356 follows the retained
Raise the Past preparation. No registry, fixture, profile, catalog identity or
accepted coverage changes. Formatting/diff checks and source review precede
native qualification; pending native/card checks are not passing results.

Source receipt, October 10: implementation e1ea896f993d9a626798ab5accf16bcd6a6ca0e4
passed formatting and diff checks. A separate read-only review found no
actionable defects in ownership, simultaneous entry, replay or later departure.
The existing Haley guard queue now targets this source for primitive execution
and Limited test-target compilation; the whole-host Pauper replay reservation
still prevents admission. The unregistered card cases have not executed.
