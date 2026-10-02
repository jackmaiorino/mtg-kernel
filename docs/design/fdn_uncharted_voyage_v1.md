# FDN Uncharted Voyage

Uncharted Voyage is a blue instant for `{3}{U}`, mana value four. Target
creature's owner chooses the top or bottom of their library, then the
caster surveils one. Reference: XMage
`a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/u/UnchartedVoyage.java`.

Add a top-or-bottom consumer of the existing owner-library choice
interpreter. Deem Inferior already binds an exact object incarnation and
authenticates its owner's second-from-top-or-bottom choice, selected
placement and remaining frames. Share that machinery while preserving the
older operation, choice serialization and hash identities. Do not change
Deem Inferior's second-from-top behavior.

Add a private one-card surveil operation. Reuse the generic resumable
card-selection contract: keep the bound top card or choose it for the
graveyard. Bind the chooser, original library and exact top incarnation,
and authenticate those fields on restore and action application. Reuse
the current chooser-private library projection. Scry/Preordain currently
provide private library bindings, but bottom placement is different from
surveil's graveyard move and cannot substitute for it.

Compose the two effects in printed order without a priority or state-based
action window between them. The first chooser is the target's owner even
when control differs. The second chooser is the spell's controller. Losing
the spell's only target before resolution skips the entire sequence,
including surveil. A token target's departure and any later library choice
need explicit reference checks rather than assumptions about physical
library membership. Surveil binds the complete library but selects its
first non-token card, matching CR 111.6. A departed token remains indexed
until the normal state-based sweep; surveil cannot move it a second time.

Required checks: exact characteristics and four-mana payment; owner versus
controller, both library placements and exact order; own/opposing targets,
ward and incarnation changes; token departure; keep/graveyard and empty
library surveil; private observations for each player; no intervening
priority; invalid actions without mutation; restore at both pending
choices. Then register a new catalog profile, preserve all older profiles
and default v32, run relevant XMage and regressions, and verify natural
external completion/replay and CI before delivering the slice.

The card, interpreter and v46 catalog successor are implemented. Nineteen
focused rules cases, eleven strict XMage comparisons, affected regressions,
default compatibility and lint pass. Two custom-deck games finish naturally
and replay identically. Release mutation boundaries and CI remain pending;
see `../reports/fdn_uncharted_voyage_v1_validation.md`. Both original decks remain unchanged. This is
routine rules engineering, with no training or playing-strength claim.
