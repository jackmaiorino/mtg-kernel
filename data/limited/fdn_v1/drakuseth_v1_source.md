# Drakuseth native Partial source

Frozen printing: FDN193, Drakuseth, Maw of Flames; Scryfall
`029b1edb-e1de-4f1c-81df-8d17f4920318`, Oracle
`060deaff-44d6-4f03-9568-bcb7add80255`. The unchanged frozen pool contains
286 names and24 token dependencies. This source appends Partial464 after
Banner463, yielding465 definitions and273 FDN rows, with245 Full,
13 Partial and28 Missing target names. No catalog/export/profile hash is guessed.

Printed rules and characteristics were read from local Mage commit
`a5c90fe180021e70e2a644ade00eeab07f857a40`:

| Path | SHA256 of pinned source bytes |
|---|---|
| `Mage.Sets/src/mage/cards/d/DrakusethMawOfFlames.java` | `85e800ff898e46f26b477a88fca8eee7b60a54c886cf8b1eeaabbf380f66ba25` |
| `Mage/src/main/java/mage/abilities/effects/common/DamageTargetAndTargetEffect.java` | `23bed77af2f847ef446bbb6c293ae9d3c1f8b38a50fb37eb828b628a34a6f27e` |
| `Mage/src/main/java/mage/filter/predicate/other/AnotherTargetPredicate.java` | `0bd31702560d09b13d609b0d6b4ca2a84fe7a880970798a8e1b3fd6b76f3e47c` |

The printing is a4RRR legendary7/7 Dragon with flying. Its actual attack
triggers four damage to a mandatory first any-target, plus three damage to
each of zero, one or two distinct other any-targets. Existing native attack
declaration/trigger generation captures the source incarnation and controller;
no journal is fabricated. Optional completion is unavailable before the first
target. The appended Standard target filter preserves all existing variants.
The appended damage leaf tests every original target slot and incarnation
before forming one native damage batch. An illegal first target does not give
four damage to a later slot; remaining legal targets still receive three.
If every target is illegal, ordinary native resolution fizzles the ability.

The typed source retains cardinality, positional amounts and complete program
payload. Frozen rules-vector facets explicitly remain opaque where they cannot
express mandatory-first/optional-other or simultaneous positional damage;
rules_vector_v1 remains disabled. No V59/model/Human/transport admission is added.
All new production behavior lives in existing semantic source-identity inputs.

Partial is deliberate. The shared damage-origin classifier recognizes the new
leaf so actual Niv provenance is carried through native damage. Crystal's
supported-root list is unchanged: this new multi-target root refuses its owned
replacement interpreter before damage rather than acquiring false permission.
Other existing native numerical and recipient-type limits remain. The current
kernel has no Battle card type. This is not full replacement, consumer, sampling,
search, training or playing-strength qualification.

Four native source groups cover both seats: actual seven-land mana payment,
cast, same-turn sickness and real later attack; zero/one/two optional targets;
mandatory-first and duplicate refusal; every selection checkpoint and published
restore; positional individual-target illegality and all-target fizzle; original
controller/source departure and reentry; and the explicit Crystal refusal.
Initial lands, recipient creatures and the Crystal shield are labelled support
operands. Control and zone responses use real native primitives and are not
claims of paid response spells. Full-only preflight still refuses the actual
40-card reference deck. All Rust remains uncompiled and unrun locally; pinned
formatting, source/static checks and lightweight Python are the available checks.
