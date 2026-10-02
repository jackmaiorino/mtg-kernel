# FDN Luminous Rebuke

Luminous Rebuke is a white instant for `{4}{W}`, mana value five. It costs
three generic less if it targets a tapped creature, and destroys any target
creature. The tapped predicate determines casting cost, not resolution
legality. Reference: XMage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/l/LuminousRebuke.java`.

Extend the existing generic reducer with a chosen-target predicate. Derive
offer-time affordability from an actual legal target completion, filter the
target menu and validate target actions with that same helper, then derive
final payment from the selected targets. Reuse the existing source/target
contracts and pending-cast snapshot. Colored pips and printed mana value
remain unchanged. Older reducers/casting paths keep their prior fast path.

The destruction program uses the existing incarnation-safe target and
indestructible checks. Ward is a separate opponent-target trigger, paid
after the spell's discounted or full casting cost. It does not affect the
initial cast offer. An untap after casting neither refunds/recharges mana
nor invalidates the creature target. Losing/reentering a target cannot
destroy its new incarnation.

Required checks: exact cost/MV/color/definition; two-mana tapped-only menus,
five-mana unrestricted creature menus, white pip requirement, protected
targets excluded, own and legendary creatures admitted, invalid actions
without mutation, land payment, target loss/reentry, actual Quirion Ranger
untap response, pending-target and ward restore, and the Prowler death
interaction. Add XMage comparisons, v45 catalog history, default compatibility,
regressions, external natural-terminal replay and CI. Preserve both original
decks and keep the full fixture goal active.

Routine rules engineering only. No training or playing-strength claim.

Runtime verification: fifteen focused kernel cases, eleven strict XMage
comparisons, catalog/session/prior-gameplay and engine checks, default
v32 compatibility and warning-denied Limited/default Clippy pass. Two
custom-deck games finish naturally and replay identically. Release
production boundaries and hosted CI remain pending; see the validation
report for exact source, result hashes and corrected preparation attempts.
