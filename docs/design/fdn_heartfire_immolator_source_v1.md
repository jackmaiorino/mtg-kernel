# Heartfire Immolator source preparation

This source is based on `9c43b100a1594564d8e55c25692c214a2aec616d` and remains unregistered. It contains no FDN catalog ID, capability assertion, profile pin, native result or PR. The prepared metadata is not a generator input. Parent issue #110 owns actual admission and qualification.

## Printed source and program

Pinned primary Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`, `Mage.Sets/src/mage/cards/h/HeartfireImmolator.java`, lines 25-44, defines a `{1}{R}` 2/2 Human Wizard with prowess and `{R}` plus source sacrifice. The effect uses `SourcePermanentPowerValue.NOT_NEGATIVE` and `TargetCreatureOrPlaneswalker`. It has no tap cost or sorcery restriction. Negative power deals no damage.

The generated activation reuses ordinary mana payment, source sacrifice and `CreatureOrPlaneswalker` target ordinal 42. Prowess reuses the existing exact triggered source-incarnation boost and its cleanup expiry. The one appended `StandardOpV1::SourceLastPowerDamageV1` leaf reads a committed source-power payment; existing operations and target IDs are unchanged.

## Departure and restored authority

The Standard `CreatureStatsLkiV1` cache is sampled at departure, after actual mana payment. A narrow non-Standard hook samples it only for a live definition that owns the new leaf. Existing Standard hooks remain unchanged. Other default and FDN definitions emit no new cache or receipt.

The narrow departure hook independently commits `SourceDamageBeforeLeavingV1`, capturing exact source/controller, color, lifelink, deathtouch and Werewolf subtype facts. Atomic activation publication appends a history-only `SourcePowerActivationCommittedV1` receipt containing the complete finalized stack item, signed departing P/T and those damage facts. No GameState or StackState field is added. The common stack boundary requires the unique exact committed item, its captured controller/source/targets, the exact departure-cache incarnation and identical cached P/T, and an identical independent departure capture. Removing, duplicating or rewriting mutable stack/cache authority refuses before Step/Pass. The interpreter uses the committed power, clamped to zero, even if the physical source later leaves its graveyard or reenters with different counters. Actual resolution, fizzle and counter completion commit SourcePowerActivationFinishedV1; the common continuation boundary checks receipt-to-live-stack completeness and rejects erased ownership or resurrected finished payloads. New event variants are appended to preserve all predecessor discriminants, and none enters transient trigger/public event_log.

Only this new opcode sets the ephemeral `DamageProposed.historical_source` stack ID. Replacement and damage code authenticates that ID against independent captures, then uses captured color for prevention, captured controller for lifelink and Standard red-damage accounting, captured deathtouch for lethal marking, and historical color/subtype facts for protection and colored hexproof. Existing damage proposals leave this field absent. `DamageProposed` is neither serialized nor hashed; committed `Damage` has exactly its predecessor fields. Both-seat physical granted-keyword cases demonstrate stolen-controller gain, damage-keyword persistence and prevention/protection after source reentry with different color and no current grant.

## Fixtures and remaining consumer scope

Future Limited-only, non-Standard printed fixtures require actual registration and use real casts, prowess, creature-cast negative, counters, red payment, sacrifice, opponent response, target protection, owner/controller distinction, departed/reentered source and target, pending targeting restore, finalized restore and read-only coherent forgeries. A registered Elf unit fixture separately exercises signed current/last stats across departure/reentry and verifies no source-power receipt on that predecessor path. These fixtures have not been compiled or executed.

Existing ordinary activation/target public actions are sufficient to express the fixed target and costs; no new action ID is introduced. Exact last-known-power provenance is a new owning stack/event semantic, so sampler, public observation and native feature consumers require qualification before support claims. The history reference visitor is exhaustive for the new variant. Historical frozen model/tensor vocabularies and weights are unchanged.

Frozen RulesMeaning emits `Atom::Opaque` for `EffectOp::StandardV1` (`rules_vector_v1/meaning/mod.rs`), retaining the payload but lacking typed semantic coverage. A rooted successor must describe signed source power at departure, nonnegative damage, source sacrifice and creature-or-planeswalker targeting. No frozen meaning vocabulary is widened here. Granted copies of this activation on definitions without the departure-capture hook are explicitly refused at pending validation; a grant/copy capture successor is separate. Existing damage proposals and Standard/default trajectories retain their old source rules when the ephemeral historical extension is absent.

Parent admission must install the actual printed FDN definition, run native all-test typechecking and the printed/primitive fixtures, qualify source/target sampling and privacy, and choose current successor catalog/profile identities. No frozen evidence is edited by this preparation.

## Mandatory receipt repair after Native49

At current composition 95bedc54d1eab8c14cc89f024963c8eefa54a0e7, Native49's
printed forgery fixture failed case4: erasing SourcePowerActivationCommittedV1
allowed Action::Pass. The shared action validator enumerated only surviving
receipts, so no receipt meant no validation until later resolution.

The repair independently validates each live stack item before receipt-ledger
reconciliation. SourceLastPowerDamageV1 intrinsically requires its unique
committed payment; absence is an error before any action changes state.
Captured definition and selected activation index identify the same operation
if its mutable inline effect is erased or replaced. A retained immutable
receipt likewise prevents erasing both mutable selectors to escape authority.
Ordinary unrelated activations retain the predecessor path, without new events,
serialized fields, schema/profile identities or state mutation during checks.

The printed both-seat forgery fixture now restores each damaged snapshot,
checks readonly refusal, and retries against the identical retained state.
Added erasure/replacement of the inline root together with receipt removal,
and erasure of both inline/source selectors while a receipt remains.
A real registered Shivan Dragon activation verifies unrelated receipt-free
validation, unchanged bytes/hash, and identical ordinary pass after restore.
Pinned formatting and diff checks are run in the owned repair tree.
Native rerun remains parent-owned; this repair does not claim a runtime pass.
