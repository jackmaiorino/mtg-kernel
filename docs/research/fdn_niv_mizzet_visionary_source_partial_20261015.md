# Niv-Mizzet, Visionary source preparation

This is an unregistered source-only preparation for the later Niv-Mizzet, Visionary printing, reserved kernel ID 461 after the separate Artist ID 460 work. It does not add a `CardDef`, printing registration, deck entry, model route, or qualification claim. The source code remains in the existing hashed `event.rs`, `trigger.rs`, and `engine.rs` files so a future identity closure cannot omit a new child module.

## Pinned source and semantics

The pinned Mage source is commit `a5c90fe180021e70e2a644ade00eeab07f857a40`, `Mage.Sets/src/mage/cards/n/NivMizzetVisionary.java`. It defines a `{4}{U}{R}` legendary 5/5 Dragon Wizard with flying, a static ability that gives its controller no maximum hand size, and “Whenever a source you control deals noncombat damage to an opponent, you draw that many cards.” The damage trigger uses the final amount, so prevention can reduce it to zero and damage multipliers change the draw count. Combat damage, damage to a creature or its controller, and damage to its own controller do not satisfy the trigger.

## Bounded source work

The event path now captures only positive, noncombat damage to a player after replacement and prevention. Before committing that damage, it records each active Niv observer controlled by the damage source's controller, together with the exact observer and source incarnation contracts. It appends a definition-specific receipt after the matching committed Damage event. Trigger collection validates that receipt and creates an appended receipt-bound draw effect containing only the indexed damage event. When the trigger is stacked, a second history receipt binds its observer and damage index to that exact stack-item ID. Resolution rechecks both bindings and reads the full `i32` amount from the committed event, so editing the queued effect or inserting a loose fixed count cannot redirect or change the draw.

For the no-maximum-hand-size ability, cleanup snapshots the active player's current Niv status at entry, before the existing cleanup path expires temporary effects and control changes. It uses that value at the existing discard check. This preserves no-Niv behavior and prevents an effect that expires during cleanup from incorrectly changing the discard limit for that same cleanup.

The receipts follow the engine's existing mutable `GameState` and event-history trust boundary. Structural checks bind observer/source contracts, committed damage, and the consuming stack item; coordinated edits to the raw state and event history are outside native validation. A future sealed owner must preserve the state/snapshot privacy boundary before exposing this recipe. The legacy rules-vector meaning slice keeps the new leaf opaque because it cannot express this receipt binding; this source work does not grant a model route.

## Remaining source work

The current canonical card registry does not yet contain Niv-Mizzet, Visionary, and the Artist ID 460 prerequisite is still in progress. Therefore this checkpoint intentionally has no Niv card registration or genuine Niv paid fixture. The implementation is inline in existing hashed `event.rs`, `trigger.rs`, `engine.rs`, `effect.rs`, `effect_refs.rs`, and rules-vector meaning files. After the real Artist prerequisite lands, register the exact printing and add both-seat fixtures for doubled, prevented/zero, combat, self-damage, source and controller departure, countered trigger, full-width draw/exhaustion, suppression/control at cleanup, restoration, and receipt/effect tampering. The written source has not been compiled or run.
