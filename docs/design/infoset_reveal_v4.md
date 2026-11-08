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
refusals still apply. The digest binds a compact operation-boundary ledger
that retains the separate arrival count and present minimum for each name.
Simulated reveal and departure hooks apply the same max-overlap and survivor
arithmetic as the transcript reducer. The sampler sees deterministic rows
for the resulting minima inside each hypothetical world.

The reveal-v4 schema and canonical occupants bind the new payload digest. The
new branch has source tests that vary hidden composition and the physical copy
shown while preserving the visible stream. The bridge's
`--x-kernel-infoset-reveal-v4` profile uses the flat-action lockstep state for
canonical row mapping and the same selected-viewer journal it sends to the
trusted adapter. It advertises a separate extension and receipt identity.
The payload digest includes the public minima by name, which the adapter and
entrant can compare with the selected transcript. The `s4a-serve-reveal-v4`
search mode and its worker require the reveal-v4 schema and use a separate
seed namespace. Native acceptance and the successor entrant are pending.
