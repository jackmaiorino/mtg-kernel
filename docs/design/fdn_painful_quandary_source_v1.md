# Painful Quandary source preparation

Issue #110 candidate, based on `884aea84927cc498ec6c21658e39ad4a756e82fe`.
This source registers the actual printing as **Partial**. All new Rust source
and fixtures are uncompiled and unrun. Full admission, model admission, fair
search qualification and successor consumer qualification remain open.

Pinned Mage checkout: `a5c90fe180021e70e2a644ade00eeab07f857a40`.
The Git blob `Mage.Sets/src/mage/cards/p/PainfulQuandary.java` has SHA-256
`215f9c3943b2f2c91f941623508d63d50aa66c45988c753397d0ed0d4c24458f`.
The local CRLF working file has SHA-256
`27610761fe3ae8824619eb722a812f9a320a83a36e37e5fa7a088188b2c7c050`;
these hashes identify different line-ending representations.
The frozen booster pool identifies FDN 179, Scryfall
`05757669-e8a6-4a2f-8479-d4e2ade822ca`, Oracle
`c37051cc-6683-4dbb-b5ff-5c3a5bdab1df`.

The printed spell costs `{3}{B}{B}`. Actual opposing `SpellCast` events create
the battlefield trigger. Its definition-owned program is an opponent-controlled
optional exact-one hand discard with an explicit unpaid five-life-loss child.
Declining remains legal with cards in hand; an empty hand executes the unpaid
child immediately. Copies are not casts. Once published, the trigger retains
its original controller and source contract after the enchantment leaves.

`MayDiscardCardOtherwiseV1`, `DiscardPaymentV1` choice purpose and answer frame
are appended enum variants. Their hand bindings, payer, structural path, unpaid
program and trailing instructions are checked against the printed definition.
The remaining instructions are reconstructed from the program rather than
trusted from a duplicated saved payload. Payment uses the ordinary discard
event/replacement path. Payer candidates are private; the opponent projection
redacts them and uses the existing neutral card-selection category. The legacy
rules vector reports opaque meaning for the new operation and trigger rather
than presenting incomplete semantics as model admission.

Five fixture bodies prepare actual mana-paid casts for both seats, optional
payment/decline, empty hand, own casts, genuine spell copies, source departure,
serialized restore, private projection, and payer/hand/path/branch/frame
forgeries. They are preparation, with no reported runtime passes.
The paid helper advances through cast finalization before asserting the spent
five black mana, completed cast and spell stack entry. All five fixture groups
cover both seats. Trigger program and constant use the same pure-Limited
feature condition as their registry dispatch arm.

Observed source checks: `cargo +1.94.1 fmt --all`, `git diff --check`, JSON/deck
consistency, unchanged predecessor enum bodies, and unchanged 252-row FDN
registry prefix. No Rust build, check, test, clippy, native execution, GPU or
paid work was performed. Parent-owned serial integration must reconcile the
provisional local definition ID 444 with sibling additions, generate the real
catalog hash and update compatible profiles. No catalog hash is predicted.
The source registry canaries expect 445 total definitions / 253 FDN rows and
exact Partial IDs Fake 442, Ravenous 443 and Quandary 444, with Full-only deck
preflight refusal. The affected Python deck suite passed 61 tests and the
frozen booster-pool suite passed 6 tests. Existing prefix hashes remain intact;
the added Quandary canary binds its actual printing and prior 252-row prefix.
