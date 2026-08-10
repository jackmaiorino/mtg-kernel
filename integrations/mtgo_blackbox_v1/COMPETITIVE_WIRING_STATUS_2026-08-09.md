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
| Client identity | The preview path pins MTGO version, executable hash, signer, PID, start time, title, DPI, monitor, and visible window state. | Calibration only | Production DXGI Desktop Duplication attestor and a ratified version-specific calibration profile. |
| Visible capture | Composed-desktop preview and occlusion checks were exercised on the live client. | Not runtime-ready | Opaque live-frame admission with capture-time identity, occlusion, cursor, protected-content, and anchor verification. |
| Perception | Keep, Pass, PlayLand, and Island mana activation have supervised visible transitions with exact pixel-region hashes. | Narrow calibration | Measured recognizers for cards, zones, counters, prompts, phase, priority, stack, combat, timers, game log, and enabled controls across relevant layouts. |
| Duel reconstruction | A fixed ten-group audit identifies the exact `ObservationV5` and legal-action inventory. | Incomplete | Two-player participant and opponent state, stable object incarnations, card-database bindings, kernel history context, and a complete ordered legal-action builder. |
| Decision validation | `validate_observed_decision_v1` checks exact kernel types, all visible leaf provenance, current-frame evidence, confidence, object bindings, readiness, and action invariants. | Implemented offline and mock | Feed it only from the future trusted live-frame and measured perception path. |
| Model deployment binding | Commit `d33bd4f` binds run, checkpoint manifest, checkpoint payload, train state, model parameters, generation, and scorer contract to each scoring request. | Adapter side implemented | Replace test identities with the exact deployed checkpoint package. |
| External observation scoring | `MtgoExternalObservationScorerV1` receives immutable exact `ObservationV5` plus the ordered `ActionSemanticV1` vector. Responses require finite f32 logits and value and bind the exact request. | Adapter side implemented | Small mtg-kernel method that constructs the existing Flat V2 view from external exact inputs and invokes `NativeCheckpointInferenceV1`. |
| Action selection | Exact response bits are deterministically reduced with the kernel's `total_cmp` argmax and lower-index tie behavior. The result binds the source decision and can create only an offline intent. | Implemented offline | Pin the production policy selection rule if competitive deployment should sample instead of use argmax. |
| Visible control resolution | Commit `8b9f4d2` requires a complete current-frame candidate set, exact legal semantics, current pixel evidence, enabled state, confidence, and exactly one selected-semantic match. Coordinates remain opaque. | Structural resolver implemented | Measured per-frame control detector and support for all action families and multi-stage choices. |
| One-input discipline | `MtgoMockActuatorV1` permits one pending action and halts if the next validated decision does not show the declared visible leaf transition. | Mock implemented | Trusted Per-Monitor V2 actuator that rechecks HWND, PID, focus, prompt, timer, frame, and physical point immediately before one input. |
| Postcondition monitoring | Mock before/after leaf predicates are decision-bound and require a newer frame and changed decision commitment. | Mock implemented | Live frame ingestion and action-specific visible predicates for every supported semantic. |
| Match lifecycle | A coordinate-free visible state machine now covers pairing, game launch, sideboarding, match result, reconnect, and return to the event. It requires phase-specific visible facts, fresh frames, and event and match identity continuity. | Structural contract implemented | Version-specific detectors, deck selection, actual match acceptance, sideboard control resolution, timers, reconnect recovery, and safe exit. |
| League and Challenge lifecycle | Separate modes now share a generic visible transition contract from event browser through entry review, queue, pairings, matches, and event completion. Entry confirmation requires an additional exact account, permission, event, terms, resource, and amount authorization. No purchase action exists. | Structural contract implemented | Version-specific visible navigation and control detection, event status and standings parsing, and dry-run client calibration that stops before entry. |
| Verification | The isolated adapter suite passes 90 tests, including composed shadow gameplay, a complete structural League lifecycle, Challenge authorization separation, exact entry-resource gating, reconnect handling, and compile-fail checks that prevent pixel, context, score, coordinate, lifecycle, and input authority escapes. | Green for current offline scope | Live shadow-match corpus, perception accuracy gates, full semantic coverage, fault injection, timer tests, reconnect tests, and private-match end-to-end rehearsal before any prize mode. |

## Shortest critical path

1. Land the minimal external-observation scorer method in mtg-kernel and implement the adapter trait against one exact checkpoint.
2. Build the opaque DXGI live-frame attestor and ratify one current client calibration profile.
3. Run a continuously captured two-player no-stakes match to close observation, action-set, object-incarnation, and history reconstruction.
4. Implement measured perception and current-frame control resolution for the decision families observed in that corpus.
5. Connect the trusted one-input actuator and live postcondition monitor in shadow mode first, then private-match mode.
6. Add match and sideboard lifecycle handling and complete repeated private-match rehearsals without an unresolved state or unsafe input.
7. Add League and Challenge navigation behind their independent authorization flags and perform dry-run navigation that stops before entry or purchase.
8. Enable a prize mode only after the exact permission record, account, model, client profile, deck, and entry constraints are reviewed and pinned.

The current branch does not claim live gameplay readiness. It now has exact adapter contracts through model selection and current-frame semantic control resolution, which sharply narrows the remaining wiring work.
