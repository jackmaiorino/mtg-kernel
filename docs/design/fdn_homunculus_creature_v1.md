# FDN Homunculus Horde

Homunculus Horde is a blue 2/2 Homunculus for `{3}{U}`. Its controller's
second successful draw each turn creates a token copy. The token has the
same name, mana cost, color, type, subtype, printed 2/2 and second-draw
ability. It is a token and cannot be admitted to a mainboard.

The supported pool has no permanent-copy modifications to Horde's
copiable values. A distinct token definition with the original object name
therefore represents those values exactly. Counters, damage, tapped state,
temporary boosts and attachments are not copied. Existing draw-event
ordinals and entry snapshots ensure a newly created copy does not trigger
retroactively for the draw that created it. On a later turn, each copy can
trigger and create another copy. A queued trigger retains its controller
and can create its copy after its original source has left.

The token leaves other zones through existing token state-based actions.
Tests must cover printed and token characteristics, ordinary casting,
multi-card draws, no retroactive triggers, subsequent turns, multiple
sources, counters/damage/tapped state, departed sources and pending-trigger
restore. Matching XMage comparisons must execute before parity is claimed.

The rules reference is the pinned XMage `HomunculusHorde.java` and
`CreateTokenCopySourceEffect.java` at
`a5c90fe180021e70e2a644ade00eeab07f857a40`.
Registry definitions append at IDs 193 and 194. The new Homunculus subtype
appends after Cleric, preserving existing subtype IDs. The next catalog
version is v41, generated hash `8f1dc68e65c24306`. Historical v40 remains
`fbef8128c0ddad96`; its records remain readable and are refused for mutation
against the new build. Broad checks, production boundaries, external replay
and hosted CI must pass before the batch is delivered as verified.

The first external check reached nine simultaneous second-draw triggers
and exposed the policy adapter's seven-trigger permutation cap. In the
Foundations custom session, groups above that cap now select one remaining
trigger at a time in bottom-to-top stack order. Each action names the
trigger index, source and selected prefix. The engine receives the complete
permutation once, after the last choice; no placement, priority or resolution
occurs during prefix selection. Snapshot/restore and the environment binding
retain that prefix. Policy budgeting reserves all choices before starting.
Groups of seven or fewer retain their existing menus, as do catalog-based
V5/V6 sessions and the frozen flat paths. Tests cover nine real Horde
triggers, nine different instances from one source, a reversed final order,
unchanged engine state between picks, stale-answer refusal, mid-prefix
restoration and refusal to start a group exceeding the remaining step cap.

This slice adds one of the original UG fixture's missing names. It does
not change the full goal's remaining cards, mulligans or final gameplay,
replay and CI requirements.
