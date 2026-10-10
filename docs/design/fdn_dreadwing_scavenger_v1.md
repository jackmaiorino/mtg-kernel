# Dreadwing Scavenger source preparation

This issue110 preparation follows the retained IDs338-363 and tentatively
uses ID364 in the later coherent admission. It does not register the card or
advance accepted coverage. V64 and v65 retain their serial admission order.

[Pinned XMage DreadwingScavenger.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/d/DreadwingScavenger.java)
defines a1UB2/2 Nightmare Bird with Flying. Entering or attacking draws one
card, then discards one. At seven controller graveyard cards, it gets +1/+1
and deathtouch. The primary source was read at that exact commit.

Two trigger definitions reuse Icewind Elemental's sequential draw/discard.
Existing live static-self-boost and threshold-keyword readers supply the
conditional abilities. They read current controller/card count, battlefield
zone and printed-ability suppression. The build recipe binds the new static
and trigger behavior; existing registered definitions remain unchanged.
Nightmare and Bird already exist. No new enum, state or projection field is
introduced. The rules-vector source inventory records the new name branches.

Five prepared cases cover exact metadata, colored payment rollback, both
seats, pending-cast replay, actual ETB and multi-creature attack declarations,
mandatory draw-then-discard, source departure, restored discard, Madness,
six/seven-card threshold, tokens excluded, current control, nonbattlefield
exclusion and Witness Protection's removal of both static and triggered
abilities. They await registration before gameplay execution. Source review
and test compilation are next; no paid/formal experiment is included.

Read-only review of58b0c17a found one shared threshold gap: the count excluded
definition tokens and virtual spells but counted runtime token copies that
retain ordinary card definitions before their next SBA. The prepared reader
now also excludes the existing object token marker. The registered threshold
unit case and unregistered Dreadwing fixture exercise that transient boundary.
This correction belongs to the later prepared admission; it does not alter
PR210. Native source compilation of58b0c17a is already running; the correction
requires the affected native threshold case afterward.
