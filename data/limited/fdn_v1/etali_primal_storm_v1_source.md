# Etali, Primal Storm native source prerequisite

Frozen FDN194 printing: Scryfall b6af9894-95b5-4c8e-902f-a9ba70f02e4a,
Oracle 078def07-ae5d-4591-8db6-d156834aab97. Exact pinned Mage input is
`a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/e/EtaliPrimalStorm.java`: legendary Elder Dinosaur,
4RR, 6/6; whenever it attacks, exile the top card of each player's library,
then its original trigger controller may cast any number of nonland cards
exiled this way without paying their mana costs.

This is an UNREGISTERED source prerequisite on a255 plus the a4cb native
source-input tuple repair. No printing ID, JSON row, deck reference, metadata,
catalog hash, profile or consumer permission changes. Registration must wait
for the actual contiguous Progenitus480 prerequisite and root coordination.

One appended EffectOp captures both original library tops before one native
zone batch. Empty libraries contribute no move; lands are exiled but not cast.
The existing Standard Etali Conqueror exile-batch continuation is reused without
changing its serialized fields or Hash shape. Its exact definition-owned root
selects top-only behavior. Original until-hit scan behavior is retained. Exact
player set, root/path, original library prefix, committed move segment, selected
nonland incarnation, ordered casts and remaining continuation are validated.

Native resolution casting retains original physical owner, captured trigger
controller, normal targets and additional/optional costs, free X=0, arbitrary
cast order and optional decline. This grants no later permission to cast cards
left in exile. The new root requires the existing lexical NativeExecutionOwner at begin and
a distinct consumed ready-row owner at resume. The latter binds the full saved
pending input and remaining stack before removal; unlike a stack loan, it allows
the existing resolution-cast machinery to add the genuinely authorized child. Public begin/direct resume
refuse it; genuine granted/other rows remain authenticated by their own exact
row/context rather than an operation flag. Existing direct-interpreter rejection, Crystal closed
eligibility and old complete consumer refusal are preserved. Frozen v1 meaning
of the new leaf is Opaque, not an invented complete predecessor meaning.

`effect/standard_exile.rs` is an explicit source input in each actual existing
producer closure that already binds the Omniscience/native engine changes;
original input order, repetitions and flat/path-source framing are retained.

Ten future genuine paid both-seat fixture functions are prepared in
`mtg-kernel/src/effect/etali_primal_storm_v1_tests.rs`. The file is deliberately
not declared as a module until genuine registration. It has no absent-printing
success fallback. It covers real land-funded cast, sickness and next-turn real
attack, zero/land tops, optional decline, both free-cast orders, independent
serialized answering and changed continuation/history/incarnation refusal.
These are prepared source, not selected or executed tests. Additional functions cover real Murder response, original departed-source
controller, public API row theft refusal, free X=0, paid mandatory discard and
independent prepaid hidden-library menu equality. Current-control response and
further optional-cost forms remain registration follow-up qualification scopes.
All Rust, build-script generation and native behavior remain UNCOMPILED/UNRUN.
Only pinned formatting, static source audits and diff checks are reported.
