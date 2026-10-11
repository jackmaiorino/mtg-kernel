# MageZero Standard completion

The opt-in `standard-magezero-fixtures` build implements all 225 distinct nonbasic
cards in the sixteen original MageZero Standard decks. The starting baseline
`f7d115fa` had 91 Full, 21 Partial and 113 missing cards, with no complete deck.
All sixteen unchanged deck lists now resolve and reach natural terminals through
the public session interface. Mono-U remains 62 cards and every sideboard is empty.

## Implementation and compatibility

The catalog has 254 definitions appended after the frozen 192-definition Pauper
prefix, including 32 token or masked-face definitions and the shared FDN Witness
Protection regression fixture. Its identity is `kernel_carddb_standard/v7`,
hash `0xd237d07494e53952`. Relative to accepted main `a957c358`, the original
deck files, Pauper and FDN registries, prior IDs and frozen Python feature
authorities remain unchanged. The integrated FDN v69 catalog retains its published
hash `0xb0338cda1845f330`.

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

The FDN v69 integration preserves the published effect and trigger variant order.
Its saved-search filter retains tag 7; Standard filters use distinct tags 8, 9 and
10. Regression checks cover unique fingerprints, coherent filter substitution in
restored Micromancer searches, and valid Repurposing Bay restore equivalence.

Rules-vector output provenance now binds both new meaning modules. Incubate token
identity comes from printed token type data, preserving its existing extracted
meaning without using a card-name label. Newly generated rules-vector provenance
and output hashes change; frozen policy contracts and prior artifacts are preserved.

## Executed verification, 2026-10-11

Final source `9e33d1759466bb82fff81072d340cf58a87d7adb` is qualified by:

| Check | Result |
| --- | --- |
| Eleven Standard integration targets | 349 passed |
| Focused Standard library checks | 80 passed |
| FDN catalog, filtered search and Micromancer compatibility | 58 passed |
| Default engine and frozen-policy compatibility | 69 passed |
| Pinned Python deck, feature and generated-contract checks | 77 passed |
| Rules-vector checks in default, FDN and affected Standard paths | 48 passed; 1 existing ignored gate |
| Both flat-policy generated contracts, pinned Rust formatting and diff checks | Passed |

The FDN and Standard integration checks and Python checks ran on
`2dbb091435c20fb278bb49c5bf334ac40bc09925`. The final source changes only rules-vector extraction,
its tests and documentation. The Standard library, default compatibility and
affected rules-vector checks ran on the final source. Default catalog identity,
prepared payments, joined serialization, scorer packets, tensorizer/checkpoint
bindings and flat-policy behavioral hashes passed. Standard library checks include
public privacy, restore refusal and valid restore controls, free-cast routes,
copy atomicity, Case permission generations, legacy observation hashes and
activation payments.

The ignored FDN rules-vector research gate is
`at_most_five_percent_of_fdn_cards_have_an_unseen_part`. It preserves the originally
failed result described in `docs/rules_vector_v1.md` and is not counted as passed.

Eight fixed paired-deck games cover all sixteen decks. Every deck cast spells,
every game reached a natural terminal, and each replay produced an identical
transcript hash and cast counts. Seeds are 91621388255232 through 91621388255239.
The [small manifest](standard_completion_manifest_v1.json) binds the exact source
archive, accepted integration baseline, input bytes, toolchain, guard, output log
and all eight transcript hashes. These checks verify rules and replay. Standard
policy training is separate research work.

Verification used Haley's cores 10 and 11 through the supported host-slot launcher,
BelowNormal priority, Cargo `-j2`, a 3 GiB target cap and a 60 GiB free-space floor.
No GPU was used. Bulk logs and build caches remain outside Git under
`D:/e-scratch/magezero-standard-completion` and
`C:/mtg-node/codex-standard-completion-20261011` on Haley. Earlier failing runs and
build preflights are retained there; only the results above constitute acceptance.
The guard interrupted the first library compilation at the cache cap after the
FDN and Standard integration suites passed. Removing owned rebuildable artifacts
while compilers were idle allowed the remaining checks to complete in job010.

Source reviews repaired historical-source, payment, privacy and continuation
findings. Delivery, required CI, maintainer review and default-branch acceptance
are recorded on [PR #218](https://github.com/jackmaiorino/mtg-kernel/pull/218) and
[issue #217](https://github.com/jackmaiorino/mtg-kernel/issues/217).
