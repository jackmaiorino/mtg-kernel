# Volrath's Shapeshifter: layer-3 source preparation

Scope: #460, following #565. Volrath remains absent from the registry and
Full English Breakfast remains unplayable. This branch is source-only;
native compilation, formatting and tests await the lead's capacity allocation.
No public schema, catalog hash, card capability or existing action changes here.
The internal optional stack producer is omitted and contributes no hash input
when absent, preserving historical states.

The official [November 6, 2026 Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt)
establish the relevant boundaries: full text changes the characteristics in
612.6; text changes belong to layer 3; copy effects exclude text changes
(707.2); abilities on the stack survive their sources (113.7a); owners order
simultaneous graveyard arrivals (404.3). Therefore changing `GameObject.card_def`
to the graveyard card would incorrectly change what a subsequent copy copies.

Oracle behavior was cross-checked against the local XMage implementation,
`Mage.Sets/src/mage/cards/v/VolrathsShapeshifter.java`, repository head
`a5c90fe180021e70e2a644ade00eeab07f857a40`, source SHA-256
`f30f8378ea118a47bb9beac0320147938b9b1caad4f211defa460f0f194a7ebf`.
That implementation places the effect in `TextChangingEffects_3` and retains
a mana-paid activation whose resolving effect discards one card.

`full_text_v1` supplies a private, read-only layer view. The caller must establish
the static ability first. The view keeps the battlefield host's ordinary source
contract and the top graveyard card's text definition separate. It reads the
current controller's public graveyard only. It does not suppress layer 3 because
abilities are removed in layer 6. Before payment it checks the exact source and
donor incarnations, then freezes a battlefield activated ability separately from
the physical source. The retained discard has its own typed selector, avoiding
collision with existing appended Equipment and Cauldron action slots. This
prepayment value can produce a separate serializable stack record. Both native
stack validation paths resolve its frozen producer without rereading the current
top. Existing observation v1 and the shared v2/policy storage paths refuse these
records, including a pending activation or resolving effect continuation.
The private `activate_full_text_retained_discard` owner now stages the retained
ability through ordinary pending validation, shared mana payment and stack
publication. It supports only the mana-only retained discard while a top creature
is present. No registered card or public action menu invokes it. The caller must
establish the full-text static ability first; tests supply the abstract layer
primitive directly on an ordinary host. Pending lookups retain the text producer
even after the pending item is removed from `EngineState` for payment.

Remaining implementation, in dependency order:

1. Route actual ability offer, announcement, payment, stack validation and
   resolution through both source identities. Retained-discard payment and stack
   plumbing are prepared. Connect the real card/action offer and donated printed
   ability payments, including costs that change the top. Preserve
   ordinary Equipment and Cauldron grants and ability-instance activation limits.
2. Core characteristic lookups select the layer-3 donor for effective name,
   value, colors, types, subtypes, supertypes, printed keywords and printed base
   power/toughness. The exact physical card gate is inactive until registration.
   Route donor CDAs, static, mana, trigger and replacement consumers through the
   same separation, and keep copy constructors on layer 1.
3. The existing P2c entry counter rule reads layer-3 text after materializing the
   battlefield object and uses its normal replacement and ability-removal checks.
   Palinchron entry and Devourer state triggers now freeze the donor and physical
   host in the native event history. The stack and resumable interpreter recover
   the original program without consulting the current top. Route the remaining
   trigger families and entry replacements and preserve exact leaving and dying
   source snapshots. Changing the top alone does not create an enters event.
4. Add owner-selected ordering for every simultaneous graveyard batch before
   state-based actions or triggers observe it. Sorting by arena ID or accepting
   the current commit-loop order changes Volrath's text and available abilities.
   The frozen public schemas must refuse this unrepresented choice until their
   complete successor exists. Hidden-origin batch contents stay owner-private
   until their zone changes actually make them public.
   The session profile must enable this behavior before any card moves, since
   consulting a concealed Shapeshifter to decide whether to prompt would expose
   deck contents. Effect batches have an authoritative post-replacement survivor
   list at `event::commit_batch_survivors`. SBA deaths use
   `event::commit_sba_death_batch_v1` and its separate direct plan and frozen
   departure information. An owned checkpoint must retain these two native
   routes, group only cards that will reach each owner's graveyard, and resume
   the original batch after the exact per-owner permutations are chosen.
5. The frozen projection refuses active text and retained producer records.
   Add the complete owning projection, then native card tests, affected
   copy/activation tests and catalog sessions. Register Full only after all
   reachable semantics and consumers qualify.

Prepared unit-test filter `full_text_v1::tests::` with
`--features eternal-fixtures --lib`. The tests use an ordinary battlefield
host to exercise the layer primitive and existing token-copy constructor. The
retained-discard tests use the real native payment/publication path, then resolve
after donor and host departure, or with an empty hand. Other stack tests begin
after payment and cover producer tampering and old-observer refusal. These are
unrun source tests, not Volrath gameplay evidence. `git diff --check` passed;
no Cargo, rustfmt or native test execution has been performed.

After capacity allocation, run the targeted library filter first, followed by
the affected ordinary activation, copy, and frozen observation/hash tests. The
first resource constraint is validation of this prepared native path. Further
donated-ability wiring should use the resulting diagnostics instead of assuming
that this uncompiled source is qualified.
