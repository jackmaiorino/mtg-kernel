# Reveal-name constraints for fair search

`export_reveal_v4` opts into `x_kernel_infoset_reveal_v4`. It inherits the
event-time visibility and recovery checks from `x_kernel_infoset_v3`, which
continues to carry the #579 recovery contract. The v1, v2 and v3 exports keep
their original refill behavior.

The exporter reduces the selected viewer's event-time `x_reveal_meaning_v3`
stream. It uses a conservative minimum per named printing. A public arrival
counts once, a reveal binds only to its current operation, and an unlinked
reveal can overlap an arrival. Departures reduce the possible surviving name
counts. Neither the live copy ledger nor an unknown hand slot selects a name.

The allocator reserves named canonical hand occupants, subtracts them from
the public refill pool and fills the remaining slots through the existing
canonical allocation. It refuses a bound beyond the hand size or available
pool. A bound that may overlap an unresolved public pin also refuses until the
allocator can represent that overlap. Existing ambiguous historical-reference
refusals still apply. The imported reveal-v4 state uses name-level hand
knowledge for subsequent simulated moves.

The reveal-v4 schema and canonical occupants bind the new payload digest. The
new branch has source tests that vary hidden composition and the physical copy
shown while preserving the visible stream. Native acceptance and bridge
wiring are pending. The bridge must consume the same viewer-local stream it
sends to the trusted adapter, with a separate opt-in from the frozen W1 path.
