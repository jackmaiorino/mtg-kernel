# Muldrotha native source appendix

Native Partial ID489 is frozen FDN243, Scryfall
`51710b19-68e3-4853-901f-e618bde61161`, Oracle
`e4625704-1d52-44e4-804f-2f45644d76ac`. The reference is Mage commit
`a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/m/MuldrothaTheGravetide.java`.

The printing genuinely casts for 3BGU and enters as a legendary 6/6
Elemental Avatar. Its active static permission applies only during its
controller's turns, to physical permanent cards in that player's own
graveyard. The native pre-announcement menu identifies the exact Muldrotha
battlefield incarnation and one unused permanent type. A multitype card
spends only that choice. Distinct source incarnations have independent
budgets; control changes preserve an incarnation's spent types, reentry
gets a new incarnation, and each new turn gets a fresh budget. Source
ability suppression removes future permission.

The route rejoins ordinary casting for real mana, targets, spell forms,
additional costs, responses and resolution. It grants no activation
permission. A separate native land route retains the actual per-turn land
allowance and empty-stack main-phase timing, including the existing land
as-enters color stage. Successful casting consumes the allowance even
when countered. A source may depart after successful casting without
rewriting the paid spell. Other existing graveyard routes remain distinct;
selecting Greatwurm's printed permission does not consume Muldrotha's
allowance.

Three appended owning history kinds bind the original graveyard card,
source/type options, selected permission and actual successful cast or
land play. Current pending and finalized paths check original contracts;
history validation rejects erased pending choices, missing consumption,
duplicate budgets and reordered/foreign selection. The optional engine
owner contributes no serialization or hash bytes while absent. The
original 237 sample payloads are unchanged; the three new kinds have no
fabricated samples. Shared native meaning digests include the new child
source, directly or through the existing Genesis Wave source closure.

Thirteen native Rust source groups cover both seats and genuinely paid
casts: exact printing, all five represented permanent types, multitype
choices, land allowance, countering, source departure/reentry/control,
actual next-turn reset, suppression, distinct Greatwurm cost route,
serialized restore, stale/erased/foreign ownership and independent prepaid
hidden worlds. Initial resources, initial graveyards, control changes and
negative mutations are identified setup/boundary primitives. The multiple
source case genuinely pays for a second Muldrotha from the graveyard, checks
independent type/source budgets at the actual legend choice, then pays
under the surviving new source. It does not claim two legendary copies can
survive ordinary SBAs together.

All Rust compilation and execution remain UNRUN. Pinned Python 3.13.14
passes 109 affected metadata/deck/frozen-target cases. The reference deck
has 40 cards and is correctly rejected by Full-only resolution. Pinned
Rustfmt 1.94.1 and static prefix/diff checks are source checks only.
Muldrotha remains Partial. V71 is reserved, with complete owning/public,
model/Human/transport and sampler support pending. No Full, production
deck, performance or playing-strength admission is claimed.
