# MageZero Standard completion

The opt-in `standard-magezero-fixtures` build implements all 225 distinct nonbasic
cards in the sixteen original MageZero Standard decks. The accepted baseline
`f7d115fa` had 91 Full, 21 Partial and 113 missing cards, with no complete deck.
All sixteen unchanged deck lists now resolve and reach natural terminals through
the public session interface. Mono-U remains 62 cards and every sideboard is empty.

## Implementation and compatibility

The catalog has 254 definitions appended after the frozen 192-definition Pauper
prefix, including 32 token or masked-face definitions and the shared FDN Witness
Protection regression fixture. Its identity is `kernel_carddb_standard/v7`,
hash `0xd237d07494e53952`. The original Pauper and FDN registry files, prior IDs,
and frozen Python feature authorities remain unchanged.

The implementation covers lands and restricted mana, removal and card selection,
set keywords and alternative costs, creature triggers and statics, planeswalkers,
Rooms, Vehicles, craft, copies, transforming cards, and five-color legends.
Spells cast during resolution retain their parent continuation and exact source
incarnations. Restored children authenticate the parent and authorizing choice
before paying costs or placing lands. Rejected spell copies leave the state intact.

Public observations mask face-down counterspell targets and crew payments while
preserving physical identities internally for rules validation. Revealed departures
retain the captured historical identity. Frozen policy encoders reject unsupported
Standard state and unrepresented private identity knowledge.

## Executed verification, 2026-10-11

Source `6f0bb03a2cfc830ede813ce6c28e110451ffa0e9` passed:

| Check | Result |
| --- | --- |
| Eleven Standard integration targets | 349 passed |
| Focused Standard library checks | 80 passed |
| Default engine and frozen-policy compatibility | 69 passed |
| Pinned Python deck, feature and generated-contract checks | 43 passed |
| Pinned Rust formatting and diff checks | Passed |

The default checks ran at `1a7cdced`, whose production Rust and data inputs are
identical to the final source; the only subsequent change fixes serialized-state
mutation in a Standard integration test. Default catalog identity, prepared payments,
joined serialization, scorer packets, tensorizer/checkpoint bindings and flat-policy
behavioral hashes passed. The Standard library checks include public privacy,
restore refusal and valid restore controls, free-cast routes, copy atomicity,
Case permission generations, legacy observation hashes and activation payments.

Eight fixed paired-deck games cover all sixteen decks. Every deck cast spells,
every game reached a natural terminal, and each replay produced an identical
transcript hash and cast counts. Seeds are 91621388255232 through 91621388255239.
The [small manifest](standard_completion_manifest_v1.json) binds the exact source
archive, input bytes, toolchain, guard, output logs and all eight transcript hashes.
These checks verify rules and replay. Standard policy training is separate research work.

Verification used Haley's cores 14 and 15 through the supported host-slot launcher,
BelowNormal priority, Cargo `-j2`, a 3 GiB target cap and a 60 GiB free-space floor.
No GPU was used. Bulk logs and build caches remain outside Git under
`D:/e-scratch/magezero-standard-completion` and
`C:/mtg-node/codex-standard-completion-20261011` on Haley. Earlier failing runs and
build preflights are retained there; only the results above constitute acceptance.

Source reviews repaired historical-source, payment, privacy and continuation
findings. Delivery, required CI, maintainer review and default-branch acceptance
are recorded on [PR #218](https://github.com/jackmaiorino/mtg-kernel/pull/218) and
[issue #217](https://github.com/jackmaiorino/mtg-kernel/issues/217).
# Integration with accepted FDN v69

The delivery integration includes main `a957c358db9326cb3eacff819d6bcea38678aeb3` and preserves its accepted FDN enum prefixes, definitions, profile identity and launcher fixes. The Standard library filters follow the accepted instant/sorcery filter with distinct continuation fingerprint tags 8, 9 and 10. A focused regression fixes the accepted tag 7 and rejects cross-filter fingerprint collisions. Rules-vector source binding now includes the already dispatched `effect_h.rs` and `effect_i.rs` modules, so future extractor outputs identify those meanings. Existing frozen outputs, qualification inputs and receipts remain unchanged.

Both generated policy-contract checks, Rust formatting and diff checks pass locally. All 49 affected FDN, Standard deck and frozen-feature refusal Python tests pass using the existing dependency environment. The existing 349/80/69 source qualification remains retained; current-head hosted native checks, including Standard sessions and the collision regression, are required before integration acceptance. No additional experiment or owner job restart was launched.
