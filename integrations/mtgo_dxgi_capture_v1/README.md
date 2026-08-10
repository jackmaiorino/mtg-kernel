# MTGO DXGI visible-frame candidate v1

This standalone Windows package captures the foreground MTGO client using DXGI Desktop Duplication. It does not use direct-window capture, accessibility enumeration, process memory, network inspection, client files other than the signed executable identity, hooks, injection, or keyboard input. Its mouse actuator is isolated in `src/actuator.rs` and is unreachable in the current production build because no exact permission correspondence is ratified.

The probe is intentionally separate from `mtgo_blackbox_v1`. It contains the Windows `unsafe` boundary needed for Win32, D3D11, DXGI, DWM, process, and Authenticode calls. The offline adapter remains `#![forbid(unsafe_code)]` and cannot call this executable as an input mechanism.

The command-line executable is a thin wrapper over the library capture engine. A successful in-process capture returns `OpaqueMtgoDxgiFrameCandidateV3`. Safe Rust callers cannot construct, clone, format, deserialize, or read pixels from that type. Its public view contains commitments and geometry only. Downstream live wiring must accept the opaque object itself, never a copied commitment or serialized manifest, if it needs proof that the real capture routine ran.

`capture_pinned_current_solitaire_visible_frame_v1` is the first source-pinned live-frame boundary. It constructs the capture request internally and accepts only the inspected MTGO executable hash, exact Daybreak signer certificate identity, local Freeform Solitaire title, 120 DPI, 1550 by 925 client, `DISPLAY2` output identity, SDR BGRA8 geometry, and all-false downstream authority flags. It then rechecks the opaque candidate's bytes and capture commitment before returning `OpaqueMtgoPinnedSolitaireVisibleFrameV1`. The profile commitment is `45f73bf432bbed42e1896c4f02e0670115bb891b69fdc7037c89dc780ac91fac`. A client update, account-title change, window-size change, DPI change, monitor move, GPU/output reorder, rotation, HDR transition, or format drift fails closed until a reviewed source update.

The pinned wrapper proves capture origin plus an exact identity and layout match only. It has no raw-pixel accessor and remains false for semantic evidence, `ObservationV5`, policy scoring, and input. Its only bridge into the existing classifiers is explicitly named `into_checked_untrusted_perception_candidate_v1`, which consumes and downgrades the wrapper to the old authority level. No semantic claim can inherit the profile merely by copying its commitments.

`measure_mtgo_dxgi_mulligan_ladder_candidate_v3` is the first direct perception consumer. It consumes the opaque frame in-process, serializes the probe-owned manifest, rechecks the manifest, PNG, and canonical pixels through `mtgo_blackbox_v1`, then runs the current dark-core binary-ink seven-state London mulligan prompt classifier. The opaque measurement retains the source frame and exposes only the checked-untrusted classification, optional prospective keep size, adapter-local action labels, and commitments. It has no pixel, coordinate, `ObservationV5`, scorer, action-intent, or input conversion.

`measure_mtgo_dxgi_mulligan_visible_hand_candidate_v3` composes that prompt measurement with the checked-untrusted deck template profile and requires all seven displayed cards to match before exposing ordered labels. MTGO displays seven cards at every London mulligan prompt while the prospective keep size descends from seven to one. The combined opaque value retains the frame, pixels, and profile bytes privately and exposes only labels, prompt semantics, and commitments. It grants no semantic, observation, trusted-policy, or input authority.

The original v3 pregame scoring contract binds only the prompt state and actions. It is retained for compatibility but does not contain the visible hand, so it is not sufficient for a meaningful Keep or Mulligan policy. The card-aware v4 contract instead binds the combined opaque measurement, all seven ordered visible card labels, the prospective keep size, the exact ordered Mulligan then Keep actions, both perception-profile commitments, and the same validated native-checkpoint deployment commitment used by gameplay scoring. An external scorer returns finite `f32` bit patterns bound to the exact request. Validation deterministically selects the greatest logit, with the first action winning ties. Stale responses, wrong card or action counts, reordered cards, non-finite values, and unmatched measurements fail closed. `build_card_aware_pregame_action_plan_v4` now carries that exact opaque selection into the existing coordinate-private plan and visible-postcondition path. If that plan later reaches the authorization-gated actuator, its immediate fresh recapture must reproduce all seven exact visible card identities as well as the prompt and selected control pixels. The builder does not itself perform or authorize input, and the production actuator ratification remains empty.

The current mtg-kernel checkpoint lineage has no Keep, Mulligan, or London-bottom action representation, no pregame observation schema, and no pregame training examples. `MtgoNonModelPregameHeuristicV1` is an explicit deterministic wiring stopgap, not a model claim. It scores v4 Keep versus Mulligan from a source-committed card feature table using retained land range, cheap castable spells, and color coverage. It implements the v5 bottoming scorer by choosing the lowest card-retention value and choosing Submit only after six selections. Its historical deployment-envelope fields contain domain-separated `not-a-checkpoint` sentinels that bind the exact heuristic profile. `is_model_backed_v1` and `safe_for_live_input_v1` are always false. The fixed 30 Plains and 30 Island profile exists only for the current no-cost Solitaire wiring exercise. A trained pregame head remains required for competitive play.

The card-aware bottoming v5 contract carries that model boundary through every London-bottom selection stage. It starts only from the complete seven-card, zero-selected opaque measurement and assigns stable adapter-local object IDs to the initial hand. Each request binds the current opaque capture, current state and identity measurements, exact remaining visible order, confirmed bottomed-card history, complete ordered action set, both perception profiles, and deployment commitment. The zero-selected action set is exactly the seven card selections because the displayed Cancel control is a no-op. Stages one through five add Cancel after the current card selections. A selected card can advance the opaque session only after a strictly newer one-count state transition from the same process, match window, and layout. Those stages reclassify every remaining visible card and require the action-bound order and labels to match. At stage six, the renderer dims the survivor enough to defeat the reviewed art template, so the request explicitly labels that survivor from confirmed action history instead of claiming a fresh visual identity. The final action set is exactly Submit then Cancel. `OpaqueMtgoCardAwareBottomingModelSelectionV5` has no actuator or input conversion, and this tranche does not submit the bottomed cards.

`build_card_aware_bottoming_action_plan_v5` is a later coordinate-private boundary over that exact opaque selection. A card selection binds the current reflowed card-art region, its observed pixel hash, the stable object ID, and the expected next selected and visible-card counts. Submit binds the current Done control and requires a strictly newer first-main measurement from the same process, match window, and layout. Cancel is plannable only at selected counts one through six. It binds the calibrated Cancel-only or Done-plus-Cancel layout and requires a strictly newer complete seven-card, zero-selected identity measurement. Confirmation verifies the same process, match window, layout, perception profile, and ordered card labels, then restores the original stable object IDs, clears the selected-card history, and folds the plan into a new reset-history commitment. The zero-selected no-op Cancel is not an action and cannot be planned. The action plan has no preparation or actuator function, all input safety accessors remain false, and its coordinates cannot be read through the public API.

`build_pregame_action_plan_v3` consumes that opaque selection and binds it to the reviewed 1550 by 925 Solitaire control layout, the exact source capture, and the observed pixels of the selected control region. The target rectangle and point remain private. A planned Mulligan requires the next current ladder prompt on a strictly newer changed frame with the same process, match-window, and output identity. A seven-card Keep can be confirmed by the current Turn 1 first-main profile, which uses binary prompt and Combat-control ink plus exact Turn 1 and empty-battlefield regions. A one-card Keep can be confirmed by `measure_mtgo_dxgi_bottom_six_initial_candidate_v3`, which uses the current binary-control state gate and requires selected count zero. Keeps at prospective sizes two through six remain blocked until their required bottom counts are separately calibrated. Zero-card Mulligan is also rejected until its prompt is calibrated.

`measure_mtgo_dxgi_bottom_six_state_candidate_v3` consumes an opaque DXGI frame and runs the current binary-control state classifier for each reviewed bottom-six selection stage from zero through six selected cards. The returned `OpaqueMtgoDxgiBottomSixStateMeasurementV3` retains the frame, pixels, and private probe geometry. It exposes only immutable commitments, stage counts, Done visibility, and action cardinality. It does not identify cards and all observation, scoring, and input authority remains false.

`measure_mtgo_dxgi_bottom_six_reflow_candidate_v3` consumes two consecutive opaque bottom-six measurements and recognizes exactly one order-preserving visible-card deletion. It rechecks both endpoints with that same current binary-control state classifier instead of falling back to the legacy raw-control gate. The returned `OpaqueMtgoDxgiBottomSixReflowMeasurementV3` exposes only commitments, stage counts, the removed before-frame ordinal, retained-card difference measurements, and the number of passing deletion candidates. It retains both source frames and pixels, rejects ambiguous visual duplicates and skipped stages, does not identify card names, and grants no observation, scoring, or input authority.

`measure_mtgo_dxgi_bottom_six_visible_card_identities_candidate_v3` consumes one opaque bottom-six measurement plus one checked-untrusted deck template profile. It rechecks the exact source pixels in-process with the current binary-control state and full-hand identity classifiers, and returns identities only when every visible ordinal clears the fixed art distance and distinct-name margin. Source pixels and local template pixels remain private. The labels are still caller supplied and unratified, no `CardStableRefV1` binding is created, and all semantic, observation, scoring, and input authority remains false.

The isolated pregame actuator consumes that opaque plan plus an opaque private-match authorization ratification. It immediately recaptures the visible prompt, rechecks the same legal actions, process, window, layout, selected-control pixels, exact account title, and DPI, then verifies the physical point is still owned by the same MTGO process before emitting exactly one left-click. It moves the cursor outside the client afterward and creates an opaque pending receipt. A process-wide gate blocks another input until the exact newer Mulligan, seven-card Keep, or one-card Keep postcondition confirms. Failed confirmation halts the gate for the process. The pending value exposes no handle or coordinates and remains false for next-input, purchase, and queue-entry safety.

Production authorization ratification is deliberately `None`. Runtime booleans and arbitrary permission hashes cannot enable the actuator. A later reviewed commit must bind the exact account alias hash and exact Daybreak correspondence hash for private-match input only. League, Challenge, Open Play, and prize-event flags must remain disabled in that token. No input was sent while implementing or testing this path.

## Admission performed by the probe

Before and after capture, the probe requires:

- Per-Monitor V2 DPI awareness;
- exactly one running `MTGO.exe` process;
- the foreground window to be a visible, responsive, uncloaked, unminimized root window owned by that process;
- the exact expected executable SHA-256;
- successful native `WinVerifyTrust` validation plus the exact pinned signer thumbprint and X.500 subject hash;
- the configured visible title rule;
- DWM composition and `WDA_NONE` display affinity;
- a nonempty physical client rectangle inside its DWM frame;
- no visible top-level window above the target intersecting the client;
- the visible cursor outside the client;
- identical process, signature, focus, geometry, cursor, and z-order snapshots before and after capture.

The DXGI path requires one output to wholly contain the client. It rejects rotation, HDR or noncanonical color space, non-BGRA8 surfaces, pointer-only updates, timeouts, access loss, protected-content masking, and source-texture geometry drift. Canonical output is tightly packed, top-down BGRA8. Staging-row padding is removed before hashing.

Artifacts are written to a new absolute directory outside the repository through a private partial directory and one final rename. The output contains `frame.bgra`, a separately hashed `frame.png` for manual review, and `manifest.json`.

Candidate schema v2 assigns every capture one non-interchangeable visible role:

- `main_client` with role `navigation` and no game format;
- `solitaire_game` with role `acting_player_solitaire` and an exact visible format;
- `spectator_game` with role `spectator` and an exact visible format.

Solitaire titles must visibly identify one participant. Spectator titles must visibly identify two. The expected format is part of the title rule, so a Standard spectator window cannot be accepted as Freeform or as an acting-player window. The adapter continues to read the earlier main-client-only v1 artifact but rejects any v1 role fields.

## Deliberate nonclaim

Every artifact is marked `checked_untrusted_not_admitted`, and all OCR, semantic-evidence, policy-scoring, and input flags are false. The opaque v3 candidate and mulligan measurement do not change those flags. The pinned wrapper separately establishes one exact capture-origin and identity/layout profile, but does not change any semantic or action flag. These types close the specific caller-forged manifest and safety-boolean path, but Authenticode and frame checks do not prove that a calibration label matches the pixels. Pre/post z-order audits also cannot eliminate a transient occluder that appears and disappears entirely during one frame acquisition. The exact binary-ink prompt templates have only a tiny offline brittleness screen and are not a general accuracy estimate. The dormant actuator requires those exact templates and adds immediate recapture checks, but it does not make perception general or prove that a click caused the later visible state.

The capture API does not focus MTGO. An attended operator must place MTGO in the foreground and park the cursor before capture. The dormant actuator has no keyboard, purchase, queue-entry, event-entry, or focus-stealing path.

## Usage

```powershell
cargo run --release -- `
  --output 'C:\absolute\path\outside\the\repository\new-directory' `
  --expected-exe-sha256 '<lowercase-sha256>' `
  --expected-signer-thumbprint '<lowercase-sha1>' `
  --expected-signer-subject-sha256 '<lowercase-sha256>' `
  --window-mode main_client `
  --timeout-ms 10000
```

An already visible foreground Solitaire window uses:

```powershell
cargo run --release -- `
  --output 'C:\absolute\path\outside\the\repository\new-directory' `
  --expected-exe-sha256 '<lowercase-sha256>' `
  --expected-signer-thumbprint '<lowercase-sha1>' `
  --expected-signer-subject-sha256 '<lowercase-sha256>' `
  --window-mode solitaire_game `
  --expected-game-format Freeform `
  --timeout-ms 10000
```

## Verification

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
```
