# Niv-Mizzet, Visionary source preparation

This document preserves the initial unregistered source preparation. Later reviewed source registers Niv-Mizzet, Visionary as Partial ID 461 after Artist ID 460 and adds genuine paid fixtures; the current receipt is [the source registration record](../design/fdn_niv_mizzet_visionary_source_v1.md). Registration and source review do not establish Rust runtime qualification or a model route.

## Pinned source and semantics

The pinned Mage source is commit `a5c90fe180021e70e2a644ade00eeab07f857a40`, `Mage.Sets/src/mage/cards/n/NivMizzetVisionary.java`. It defines a `{4}{U}{R}` legendary 5/5 Dragon Wizard with flying, a static ability that gives its controller no maximum hand size, and “Whenever a source you control deals noncombat damage to an opponent, you draw that many cards.” The damage trigger uses the final amount, so prevention can reduce it to zero and damage multipliers change the draw count. Combat damage, damage to a creature or its controller, and damage to its own controller do not satisfy the trigger.

## Bounded source work

The event path now captures only positive, noncombat damage to a player after replacement and prevention. Before committing that damage, it records each active Niv observer controlled by the damage source's controller, together with the exact observer and source incarnation contracts. It appends a definition-specific receipt after the matching committed Damage event. Trigger collection validates that receipt and creates an appended receipt-bound draw effect containing only the indexed damage event. When the trigger is stacked, a second history receipt binds its observer and damage index to that exact stack-item ID. Resolution rechecks both bindings and reads the full `i32` amount from the committed event, so editing the queued effect or inserting a loose fixed count cannot redirect or change the draw.

For the no-maximum-hand-size ability, cleanup snapshots the active player's current Niv status at entry, before the existing cleanup path expires temporary effects and control changes. It uses that value at the existing discard check. This preserves no-Niv behavior and prevents an effect that expires during cleanup from incorrectly changing the discard limit for that same cleanup.

The receipts follow the engine's existing mutable `GameState` and event-history trust boundary. Structural checks bind observer/source contracts, committed damage, and the consuming stack item; coordinated edits to the raw state and event history are outside native validation. A future sealed owner must preserve the state/snapshot privacy boundary before exposing this recipe. The legacy rules-vector meaning slice keeps the new leaf opaque because it cannot express this receipt binding; this source work does not grant a model route.

## Remaining qualification and coverage

The Artist prerequisite, genuine Partial registration and three paid both-seat source groups are now composed. They cover queued control changes, Murder before Archer damage, control before source departure, native snapshot restoration and the cleanup hand-limit predicate. They do not establish the complete doubled, prevented/zero, combat, self-damage, countered-trigger, full-width/exhaustion, suppression or receipt-tampering coverage listed in this initial plan. Native qualification and complete owning, model, Human and transport admission remain required. The written Rust source has not been compiled or run.
