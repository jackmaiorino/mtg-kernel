# Zimone native Partial source

Base: `2ee0fb7e91438adde9dbfdbd39c6c5ef21bfb72f`. Printed reference:
FDN 126, pinned Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/z/ZimoneParadoxSculptor.java`.

The own beginning-of-combat trigger puts one +1/+1 counter on each of zero
to two distinct controlled creatures. The activated ability costs GU and
tap, and doubles every counter kind on zero to two distinct controlled
artifacts and/or creatures. Selection and resolution use native effective
types, the captured ability controller, and the original target incarnations.
An illegal original slot is skipped without moving another target into it.

Doubling adds the current count of each kind through the existing native
placement replacements, including Doubling Season and Talent. The added
count and the resulting ledger use checked native widths. All 17 native
kinds are covered, including unsigned loyalty, time and finality. A zero
kind stays absent. Both targets are staged together, so overflow halts with
`InvalidEffectContinuation` without partially changing either counter ledger.
The original transfer implementation and its older callers remain unchanged.

Six genuine paid both-seat fixture functions in
`zimone_paradox_sculptor_v1::tests` cover printed cast/activation costs,
summoning sickness, zero/one/two target selection, pending and stack restore,
current counts, all native counter kinds, paid Season replacement, overflow,
invalid and duplicate choices, source departure/reentry/control change, and
simultaneous target control/incarnation changes plus current effective type
loss. Existing support printings, counter placements, control changes,
zone changes and face-down/face-up setup are explicitly labelled native
primitives; they are not represented as paid spells or printed turn-up
abilities.

This sibling appends Zimone as local definition 464, Partial. Drakuseth is
independently appending 464 in its own sibling, so composition must remap this
append and its current-inventory assertions after that commit. Original 272
extension rows and the frozen booster pool remain intact. The Full-only deck
resolver refuses the Zimone reference deck. New target meaning remains opaque
under the frozen facet vocabulary. No complete observation, owner, model,
transport, export or Full admission is claimed.

Rust source is explicitly uncompiled and unrun. Formatting and Python/static
checks do not establish runtime correctness or performance.
