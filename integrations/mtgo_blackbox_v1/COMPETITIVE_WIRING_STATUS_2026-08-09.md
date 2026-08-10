# MTGO competitive wiring status, 2026-08-09

## Target

The wiring target is an end-to-end loop capable of joining and completing MTGO League and Challenge matches with the selected mtg-kernel checkpoint, while using only information and controls available to the player. A mode is not wiring-complete until the same runtime can:

1. load exact account, permission, client, calibration, card-database, and model identities;
2. observe every decision in a two-player match without hidden channels;
3. reconstruct an exact `ObservationV5` and complete ordered legal-action vector;
4. score that exact decision with the pinned checkpoint;
5. resolve the selected semantic to one fresh visible control;
6. perform at most one authorized input and visibly confirm its postcondition;
7. handle mulligans, priority, combat, modal choices, targeting, sideboarding, clocks, match transitions, reconnects, and terminal results;
8. stop without input on every unresolved, stale, ambiguous, or low-confidence state.

League and Challenge navigation may share the gameplay loop, but each remains a distinct authorization and lifecycle mode.

## Current evidence

| Wiring segment | Current evidence | Status | Missing proof or implementation |
| --- | --- | --- | --- |
| Authorization modes | `MtgoAuthorizationScopeV1` separately gates private, open, League, Challenge, and other prize-event input and permanently requires visible-only channels. | Structural contract complete | Load and hash the exact written permission and account record. Keep League and Challenge flags false until the correspondence is reviewed as expressly covering those modes. |
| Client identity | The preview path pins MTGO version and layout. The standalone DXGI candidate additionally pins the executable hash, exact Authenticode signer, unique process, PID, start time, title, DPI, output, and visible window state before and after capture. | Live candidate implemented, not admitted | Ratified version-specific calibration profile and a trusted capture attestation boundary. |
| Visible capture | The standalone DXGI Desktop Duplication backend acquired real foreground main-client and acting-player Solitaire crops, rejected pointer-only updates, verified current desktop presentations and no protected-content masking, removed staging padding, and emitted separately hashed BGRA8, PNG, and manifest files. V2 makes navigation, acting-player Solitaire, and spectator roles non-interchangeable. The adapter strictly checked the live v2 Freeform artifact as `ActingPlayerSolitaire` and proved PNG-to-BGRA identity. That one exact inspected artifact is now source-ratified for offline acting-player calibration only. | Offline calibration source admitted, live runtime not ready | Reusable ratified profile and anchors, trusted opaque live-frame admission, and closure or explicit operational treatment of the transient-occluder race. |
| Perception | Keep, Pass, PlayLand, and Island mana activation have supervised visible transitions with exact pixel-region hashes. The Keep transition now has independently checked acting-player DXGI artifacts and byte-measured prompt, counts, game-log, phase-bar, and hand changes. | Narrow calibration | Measured recognizers for cards, zones, counters, prompts, phase, priority, stack, combat, timers, game log, and enabled controls across relevant layouts. One manually labeled pair is not an accuracy result. |
| Duel reconstruction | A fixed ten-group audit identifies the exact `ObservationV5` and legal-action inventory. | Incomplete | Two-player participant and opponent state, stable object incarnations, card-database bindings, kernel history context, and a complete ordered legal-action builder. |
| Decision validation | `validate_observed_decision_v1` checks exact kernel types, all visible leaf provenance, current-frame evidence, confidence, object bindings, readiness, and action invariants. | Implemented offline and mock | Feed it only from the future trusted live-frame and measured perception path. |
| Model deployment binding | Commit `d33bd4f` binds run, checkpoint manifest, checkpoint payload, train state, model parameters, generation, and scorer contract to each scoring request. | Adapter side implemented | Replace test identities with the exact deployed checkpoint package. |
| External observation scoring | `MtgoExternalObservationScorerV1` receives immutable exact `ObservationV5` plus the ordered `ActionSemanticV1` vector. Responses require finite f32 logits and value and bind the exact request. | Adapter side implemented | Small mtg-kernel method that constructs the existing Flat V2 view from external exact inputs and invokes `NativeCheckpointInferenceV1`. |
| Action selection | Exact response bits are deterministically reduced with the kernel's `total_cmp` argmax and lower-index tie behavior. The result binds the source decision and can create only an offline intent. | Implemented offline | Pin the production policy selection rule if competitive deployment should sample instead of use argmax. |
| Visible control resolution | Commit `8b9f4d2` requires a complete current-frame candidate set, exact legal semantics, current pixel evidence, enabled state, confidence, and exactly one selected-semantic match. Coordinates remain opaque. | Structural resolver implemented | Measured per-frame control detector and support for all action families and multi-stage choices. |
| One-input discipline | `MtgoMockActuatorV1` permits one pending action and halts if the next validated decision does not show the declared visible leaf transition. A separate attended helper emitted the first live Keep click while rechecking exact process, signer, focus, DPI, geometry, hit-test root, and cursor position. | Supervised gameplay primitive demonstrated, not autonomous | Bind a trusted Per-Monitor V2 actuator to one validated semantic, a fresh admitted frame, the current prompt and timer, and an action-specific visible postcondition. The current helper has no semantic or postcondition authority. |
| Postcondition monitoring | Mock predicates are decision-bound and require a newer frame and changed decision commitment. The live Keep pair now has a byte-checked newer acting-player frame with required prompt, player-counts, game-log, phase-bar, and hand changes. | One live calibration transition, not runtime monitoring | Trusted live-frame ingestion, semantic recognizers, and action-specific visible predicates for every supported semantic. |
| Match lifecycle | A coordinate-free visible state machine now covers pairing, game launch, sideboarding, match result, reconnect, and return to the event. It requires phase-specific visible facts, fresh frames, and event and match identity continuity. | Structural contract implemented | Version-specific detectors, deck selection, actual match acceptance, sideboard control resolution, timers, reconnect recovery, and safe exit. |
| League and Challenge lifecycle | Separate modes now share a generic visible transition contract from event browser through entry review, queue, pairings, matches, and event completion. Entry confirmation requires an additional exact account, permission, event, terms, resource, and amount authorization. No purchase action exists. | Structural contract implemented | Version-specific visible navigation and control detection, event status and standings parsing, and dry-run client calibration that stops before entry. |
| Verification | The isolated adapter suite passes 114 tests, including strict DXGI role, artifact tamper, exact offline-ratification, guarded Keep transition, dialog-inspection, and one-click source-policy checks, composed shadow gameplay, a complete structural League lifecycle, Challenge authorization separation, exact entry-resource gating, reconnect handling, and compile-fail checks that prevent pixel, context, score, coordinate, lifecycle, and input authority escapes. The separate capture crate passes eight tests and clean clippy. | Green for current offline and checked-untrusted capture scope | Live shadow-match corpus, perception accuracy gates, full semantic coverage, fault injection, timer tests, reconnect tests, and private-match end-to-end rehearsal before any prize mode. |

## Shortest critical path

1. Land the minimal external-observation scorer method in mtg-kernel and implement the adapter trait against one exact checkpoint.
2. Ratify one current client calibration profile and promote the checked DXGI artifact through a trusted opaque live-frame attestor.
3. Run a continuously captured two-player no-stakes match to close observation, action-set, object-incarnation, and history reconstruction.
4. Implement measured perception and current-frame control resolution for the decision families observed in that corpus.
5. Connect the trusted one-input actuator and live postcondition monitor in shadow mode first, then private-match mode.
6. Add match and sideboard lifecycle handling and complete repeated private-match rehearsals without an unresolved state or unsafe input.
7. Add League and Challenge navigation behind their independent authorization flags and perform dry-run navigation that stops before entry or purchase.
8. Enable a prize mode only after the exact permission record, account, model, client profile, deck, and entry constraints are reviewed and pinned.

The current branch does not claim live gameplay readiness. It now has a working visible-pixel capture candidate plus exact adapter contracts through artifact checking, model selection, and current-frame semantic control resolution, which sharply narrows the remaining wiring work.
