# MTGO black-box adapter v1

This directory is an isolated, offline-first boundary between player-visible MTGO state and the existing `mtg-kernel` policy types. It contains no live capture, UI Automation enumeration, process inspection, network inspection, client-file parsing, synthetic input, or checkpoint scoring.

The read-only installation findings and capture implications are recorded in `CLIENT_INVENTORY_2026-08-08.md`.

## Current result

The crate accepts a proposed MTGO decision only when all of these conditions hold:

- The payload uses the kernel's exact `ObservationV5`, `ActionSemanticV1`, `CardStableRefV1`, and `PlayerSeatV1` types.
- Every game-information JSON leaf has exactly one provenance record, a matching SHA-256, at least one permitted visible evidence source, and at least 95 percent declared confidence.
- Every evidence chain terminates in a nonempty in-bounds region of a committed client frame. The decision frame must be newest. Every current observation, object-binding, and legal-action leaf must cite it. Only explicitly persistent revealed-hand and known-library facts may rely on an older frame without additional current-frame support.
- Visible game-log text, accessibility text, and manual annotations directly cite a visible pixel region. Accessibility evidence must declare the same frame, `is_offscreen=false`, and control bounds contained in the corroborating pixel region. Raw text is not stored in this contract, only digests.
- The observation, complete legal-action set, and current client prompt are declared reconciled.
- Observation schema, kernel version, surface versions, card database hash, substep structure, policy-surface context, action counts and indices, and the kernel's visible-projection hash are revalidated.
- Every action actor matches the acting player, every referenced card has one exact adapter binding, and ambiguous or aggregate combat actions are absent.
- The resulting offline action intent contains only a decision commitment, frame ID, selected action index, and exact kernel semantic action. It contains no coordinates, process handles, or input mechanism.

The mock actuator takes a validated source decision and selected index, then creates the intent internally. Every submission requires one visible observation leaf with distinct declared before and after digests, and the before digest must match the source decision. It will not accept another action until a strictly newer validated frame, a different decision commitment, and the declared after digest are all present. A missing postcondition permanently halts that mock instance.

Provenance proves that a claim was attributed to a permitted player-visible channel. It cannot prove that OCR, an annotation, or another perception component interpreted the pixels correctly. Replay labels and perception accuracy therefore need their own measured validation.

## Authorization boundary

`MtgoAuthorizationScopeV1` binds any non-offline mode to digests of the authorized account alias and written permission. The scope always requires visible-only channels. Private matches, open play, Leagues, Challenges, and other prize events have separate flags.

All live-input flags default to false. League, Challenge, and other prize-event input must remain false until Daybreak's reply expressly confirms those modes. The no-hidden-information condition is never configurable away.

## Intended pipeline

1. Load a local authorization scope bound to the final correspondence and exact account.
2. Capture only pixels that a player can see in the MTGO client.
3. Parse visible zones, cards, counters, prompts, phase, priority, game log, and timers. Accessibility text may assist only when the same content is visibly corroborated.
4. Reconcile the visible state into `ObservationV5`, a complete ordered `ActionSemanticV1` vector, and synthetic object incarnations. A zone change creates a new `zone_change_count`.
5. Validate per-leaf provenance, readiness, actor agreement, action bindings, and confidence through this crate.
6. Score the validated observation and ordered legal actions through a future external-observation scorer seam.
7. Resolve the selected semantic action against current visible evidence. Do not use fixed screen coordinates.
8. Reconfirm focus, prompt, and timer, perform exactly one authorized input, then require a visible postcondition before another input.
9. Stop on layout drift, an unknown prompt, low confidence, timer danger, focus loss, action-set disagreement, or missing postcondition.

## Deliberate seam after this tranche

The existing checkpoint shadow service owns a simulated `FastActorSessionV1`. It cannot score an arbitrary observation reconstructed from MTGO. Its flat scoring view and inference output accessors are crate-private.

The next kernel-facing change should be a small external-observation scoring API that consumes a validated `ObservationV5` plus the exact ordered `ActionSemanticV1` vector and returns scores bound to the decision commitment. That change should be made only after Fable's current science branch is reconciled, because it touches core scoring code.

## Isolation and merge

This standalone workspace changes only `integrations/mtgo_blackbox_v1/**`. It does not modify the root Cargo workspace, root lockfile, trainer, Store, rollout, scorer, or experiment files. Before landing, rebase this branch onto the then-current `main`, compare changed paths with Fable's landing diff, and merge as a focused commit or pull request.

Focused validation from this directory:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'mtgo-blackbox-v1-target'
cargo test -p mtgo-blackbox-v1 --test contract_v1
```
