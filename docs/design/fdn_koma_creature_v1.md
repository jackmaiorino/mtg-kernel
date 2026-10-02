# FDN Koma, World-Eater

Koma is a legendary blue and green 8/12 Serpent for `{3}{G}{G}{U}{U}`.
Its spell cannot be countered. On the battlefield it has trample and
ward `{4}`. Each time it deals combat damage to a player, its controller
creates four 3/3 blue Serpent tokens named Koma's Coil. The tokens are
not legendary and have no additional abilities.

The rules reference is pinned XMage `KomaWorldEater.java` and
`KomasCoilToken.java` at `a5c90fe180021e70e2a644ade00eeab07f857a40`.
This implementation follows the Horde batch and retains the original UG
and WG fixtures. Append registry definitions for the creature and token;
publish a new FDN catalog while preserving old read-only identities and
the default catalog and frozen flat files.

The existing legend rule, generic-mana ward, trample damage choices and
combat-damage-to-player trigger can be reused. The new primitive is the
spell's inability to be countered. Counterspell remains a legal targeting
action against Koma, but its resolution leaves Koma on the stack. This
protection applies to the spell, not its triggered abilities or an ordinary
post-resolution stack departure. Generic counter handling must also cover
counter-unless-payment effects and spell copies without suppressing legal
choices. Preserve the default build's existing generated programs and
catalog identity; bind the changed FDN programs to the new FDN identity.

Pinned XMage `CounterUnlessPaysEffect.java` offers a payable additional
cost before attempting to counter, even when the spell cannot be
countered. The kernel must preserve that choice and check protection at
the actual counter attempt. The nine executed reference cases in
[Mage PR #7](https://github.com/jackmaiorino/mage/pull/7) include declining
Force Spike's payable one-mana cost, all three ward paths, and a trample
damage trigger whose Koma source dies during simultaneous damage. All nine
passed with strict choices; kernel counterparts remain to be implemented.

| Required check | Evidence |
| --- | --- |
| Printed definitions | Exact cost, colors, legendary status, Serpent subtype, 8/12, trample, ward four and spell protection; token 3/3, blue, Serpent, zero mana value and mainboard rejection. |
| Casting and counters | Exact colored/generic payment; Counterspell can target Koma but cannot remove it; counter-unless-pay interaction; successful resolution and normal graveyard departure of the opposing counterspell. |
| Ward | Opponent targets Koma, then pays four or declines; a paid spell resolves and an unpaid spell is countered. Own-controller targeting does not trigger ward. Restore at the payment decision. |
| Combat | Unblocked and trample-to-player damage each create exactly four Coils; fully blocked damage and noncombat damage do not. Real attack/block/damage choices remain available. |
| Multiple events and departed source | Each eligible combat event creates one four-token batch under the recorded trigger controller; queued damage triggers survive source departure. |
| Persistence and regressions | Restore pending damage triggers and ward choices; focused XMage counterparts execute; catalog, prior gameplay, Python, production boundaries, external natural-terminal replay and CI pass. |

This is a rules implementation slice. No training, formal evaluation,
playing-strength claim or expansion of the full fixture goal is included.
