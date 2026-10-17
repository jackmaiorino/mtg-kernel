# Tinybones native source preparation

Reserved printing: FDN72 Tinybones, Bauble Burglar, Scryfall
`ff3d85bc-ef2d-4251-baf4-a14bd0cee61e`, Oracle
`fd335b2f-e6a2-45ac-949e-a67278b21cb6`. Authority is the frozen pool and
Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/t/TinybonesBaubleBurglar.java`.

The printed card is a legendary 1/3 Skeleton Rogue for 1B. Its mandatory
opponent-discard trigger exiles that exact card from the graveyard with a
stash counter. The trigger is not a targeted graveyard choice. A card that
did not reach the graveyard, or that left and returned, is not the original
discarded incarnation. Madness still causes a discard trigger, even though
the card is unavailable in the graveyard when that trigger resolves.

During its controller's turn an active Tinybones permits playing cards
that player does not own with stash counters in exile. The permission is
not linked to the source that placed a counter. It follows current source
control and printed ability availability, normal spell timing, spell forms,
costs and land allowance. Its as-though mana rule applies to those foreign
stash cards and retains actual restricted-mana and Phyrexian-life choices.
Its 3B plus tap ability makes each opponent discard one card, only at
sorcery timing, with ordinary tap sickness and paid stack responses.

The first unregistered prerequisite adds sparse native stash storage.
Zero preserves predecessor counter serialization and hashing. Generic
counter transfer, replacement, departure LKI, counter tests and Greatwurm's
all-kind payment roster include this distinct kind. Existing public
seventeen-counter shapes explicitly refuse nonzero stash rather than
silently dropping it. No predecessor DTO, printed CardDef field, existing
enum ordinal or frozen sample payload is changed.

Native discard occurrence ownership, stashing resolution, continuous
play/mana permission, actual ID490 registration and paid printing fixtures
are still being authored. This preparation is not a registered printing or
an implementation-completeness claim. All Rust checks and execution are
UNRUN. V72 is reserved for a later distinct complete consumer; no Full,
owning, model, sampler, Human or transport admission is granted.
