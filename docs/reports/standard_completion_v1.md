# MageZero Standard completion

Assignment: finish engine support for all sixteen unchanged decks in
`data/standard/magezero_v1/decks`. The baseline at `ddff546a` admits 91 Full,
21 Partial and 113 missing distinct nonbasic cards, and zero complete decks.

Acceptance requires all 225 distinct nonbasic deck cards to have complete
implemented behavior, all sixteen deck files to resolve, executable tests for
the added choices and interactions, and deterministic terminal gameplay through
the public session interface. Preserve the 62-card blue fixture and empty
sideboards. Passing inventory alone is insufficient. Existing frozen Pauper
and FDN identities and checkpoint interfaces remain unchanged.

## Work in progress

- Root: integrate the existing E/F branch `f3895c802`, repair its known rules
  limitations, add remaining noncreature permanents and planeswalkers, and
  integrate, verify, review and deliver the complete change.
- Keywords: family D missing cards and incomplete payment/targeting/timing
  choices, with shared face-down mechanics.
- Creatures: families G/H, last-known characteristics and restriction
  lifetimes, random bottom order, and shared FDN creature imports.
- Lands/spells: families B/C, utility lands, restricted mana, and public
  poison observations, including shared FDN spell imports.

The integrated E/F source is development work. Its imported capability flags
are not a verified support result. Catalog admission and the fixed hash must be
reconciled after rules review and affected tests. No new training, benchmark
campaign, paid compute, or playing-strength claim is part of this assignment.

## Verification and resources

Rust toolchain remains pinned at 1.94.1. Each worker queues at most one
BelowNormal core through `python/tools/host_slots_v1.py`, with its own Cargo
cache on D:. Existing host reservations remain binding. Root scratch is
`D:/e-scratch/magezero-standard-completion`; its code/build logs are reproducible,
with a 20 GiB cap and at least 60 GiB free reserved on D:. Current tests are
pending, not passed. Source references and tests stay in Git; bulk caches do not.

Generic public decision observations must describe the supported rules state.
Frozen Pauper policy encoders may refuse new Standard state rather than silently
drop it. Training a new Standard policy is a separate research task.
