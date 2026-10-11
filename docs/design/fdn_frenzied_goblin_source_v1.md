# Frenzied Goblin prepared source

Primary reference is local XMage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/f/FrenziedGoblin.java`: red {R} Goblin Berserker 1/1.
Whenever it attacks, you may pay {R}. If you do, target creature cannot block
this turn. XMage chooses `TargetCreaturePermanent` on the attack trigger and
uses `DoIfCostPaid(CantBlockTargetEffect(EndOfTurn), {R})` at resolution.
The exact name belongs to frozen `card_names.json` and was absent from both
actual Default and FDN registry inputs when this source was prepared.

## Prepared behavior

Base is Sower's reviewed `50108c2fb09c120e53400030dcead70b02a467af` over
`b62fa679a07c49167ef918da4a701e0df23a4b31`. Its append-only
`PreventTargetBlockingUntilEndOfTurn` program and incarnation-scoped
`ResolvedObjectCantBlock` record enforce both blocker menus and declarations.
Frenzied reuses those semantics without another opcode, subtype, or target ID.
Existing Goblin/Berserker vocabulary is sufficient. The new trigger mapping
uses existing `Attacks` and mandatory `Creature` targeting at placement.

The optional-payment program is an actual root `MayPayManaThen` for the
captured controller, one red and no generic mana. Target legality is rechecked
before resolution, so an illegal sole target fizzles without a mana prompt or
payment. Source departure, reentry or later control change does not cancel
the captured ability or change its payer. Recipient control changes do not
cancel the resolved restriction. Recipient departure ends it for that
incarnation. The shared cleanup and repeated-cleanup path expires it.

A scoped validator binds the source/controller/incarnation to an independently
committed latest exact-incarnation `DeclaredAttacker` in retained history.
An older attack by a different controller cannot authorize a current trigger;
ordinary gameplay drains the prior combat stack before a later declaration.
Generic definition, physical
owner, exact source and target contracts remain authoritative. Payment
continuations admit only the real root, exact red Boolean prompt, authenticated
paid frame, or empty declining/unpayable completion. A free consequence frame
and coherently rewritten Boolean cost/payer/consequence refuse. Existing
non-Frenzied roots pass through without additional state or event records.

## Fixtures and qualification

Future FDN-only fixtures require actual parent registration rather than skip or
fabricate a catalog. Both seats cast the printed spell, declare actual attackers,
select a target before the optional payment, pay/decline/resolve unpayable,
restore placement/payment/accepted-answer snapshots, test source control and
departure/reentry, target control/stale/reentry/hexproof/protection, actual legal
block menus and readonly declarations, natural cleanup, Witness suppression and
empty attacks. Physical haste, control and keyword grants are explicitly accepted
primitive setups, not claims of printed producers in the current frozen pool.
Registered unit fixtures check the precise recipe and unchanged absent-root path.

No registry, card ID, catalog/profile hash, frozen external input or model
vocabulary is changed. No native compilation or gameplay was run. Pinned
Rust 1.94.1 formatting and diff checks are the available source checks.
Parent owns serial composition, actual registration and native qualification.

## Exact remaining consumer gaps

Frozen RulesVector meaning sees the shared new leaf as opaque; it does not
precisely explain this attack-trigger root. A precise selected-root meaning
successor must authenticate the real generated attack/payment/target recipe.
Frozen public V8/V9 intentionally refuses active cannot-block records in the
reviewed Sower source. A successor must expose the exact public recipient
incarnation, effect duration and captured payment controller with correct Human
handles and retry/restore ownership. Existing optional Boolean transport is
not a claim that those new active records are represented. Copied historical
source-definition transport, sampler qualification, native/model features and
actual registered fixtures remain separate admission work. The prepared JSON
is documentation only and is not an input to the card generator.
