# MTGO black-box adapter v1

Current end-to-end competitive wiring status and the shortest critical path are tracked in `COMPETITIVE_WIRING_STATUS_2026-08-09.md`.

The exact two-player observation, visible-history, legal-control, and card-coverage plan is `DUEL-RECONSTRUCTION-V1-DESIGN.md`.

This directory is an isolated, offline-first boundary between player-visible MTGO state and the existing `mtg-kernel` policy types. The Rust contract contains no live capture, UI Automation enumeration, process inspection, network inspection, client-file parsing, or synthetic input. It can score an already validated exact public decision through an independently pinned immutable native checkpoint handle, but that path has no capture or input authority. A separate calibration-preview script and sibling DXGI candidate executable perform the narrow live operations documented below.

The read-only installation findings and capture implications are recorded in `CLIENT_INVENTORY_2026-08-08.md`.

The first live Standard spectator observations and exact non-claims are recorded in `LIVE_SPECTATOR_FINDINGS_2026-08-09.md`.

The first acting-player Freeform Solitaire observation is recorded in `LIVE_SOLITAIRE_FINDINGS_2026-08-09.md`.

## Live calibration preview

`scripts/capture_visible_mtgo_preview_v1.ps1` creates only a local, composed-desktop calibration preview. It crops the visible desktop to the MTGO client area. It does not use direct window capture, UI Automation, process memory, network data, client logs, hidden client state, or input.

The script requires one responsive MTGO process. Its default main-client mode requires exactly one visible top-level MTGO window; the gameplay exception is described below. It pins the exact client version, executable SHA-256, Authenticode leaf certificate, title rule, and DPI. It requires the target window to be foreground, visible, uncloaked, unminimized, entirely within one monitor, free of intersecting windows above it, and free of the visible cursor. It captures only after entering Per-Monitor V2 DPI awareness, then repeats identity, geometry, foreground, monitor, occlusion, and signature checks before persisting anything.

Those before-and-after checks cannot prove that a very brief cursor, notification, tooltip, or other occluder did not appear and disappear during the copy. This unresolved race is another reason the preview is not evidence. A production backend needs capture-time frame correlation in addition to the same conservative checks.

Every output is marked `pending_visual_review` and explicitly unsafe for semantic evidence, OCR, policy scoring, and input. The destination must be a new absolute directory outside this repository. The first reviewed image should establish a client-version, DPI, physical-size, and image-anchor calibration profile. A production evidence backend should use DXGI Desktop Duplication and must remain a separate later tranche.

The default `MainClient` mode retains the original single-window requirement. `ForegroundSpectatorGame` mode is only for a manually selected spectated 1-on-1 game. `ForegroundDuelGame` is only for the approved account acting in a 1-on-1 game. `ForegroundSolitaireGame` is only for a one-player game created through Custom Match. All gameplay modes require the main client to remain visible, select only the foreground top-level window owned by the same verified MTGO process, pin the expected format, validate the mode-specific title structure, commit the complete visible MTGO top-level window set, and repeat those checks after capture. MTGO sometimes visibly renders numeric match and game IDs in its title bar without including them in `GetWindowText`; that case is recorded as a less specific title identity rather than pretending the IDs were independently verified. Other MTGO panes may remain visible behind the game, but any window intersecting the game above it still rejects the capture.

`ForegroundOwnedDialog` is a separate navigation-only inspection mode for visible MTGO-owned dialogs such as deck selection and Custom Match. It requires the verified main client plus a distinct foreground root window owned by the same process. It records the exact visible window set and repeats every identity, geometry, focus, cursor, and occlusion check. Its output remains an unsafe inspection preview and cannot be consumed as game evidence.

Example after independently verifying the current identity values and placing MTGO unobscured in the foreground with the cursor outside its client area:

```powershell
.\scripts\capture_visible_mtgo_preview_v1.ps1 `
  -OutputDirectory (Join-Path $env:TEMP 'mtgo-preview-YYYYMMDD-HHMMSS') `
  -ExpectedProductVersion '<exact-product-version>' `
  -ExpectedExecutableSha256 '<exact-executable-sha256>' `
  -ExpectedSignerThumbprint '<exact-leaf-certificate-thumbprint>' `
  -ExpectedSignerSubject '<exact-leaf-certificate-subject>' `
  -ExpectedDpi 120
```

Example for an already-open spectated Standard game:

```powershell
.\scripts\capture_visible_mtgo_preview_v1.ps1 `
  -OutputDirectory (Join-Path $env:TEMP 'mtgo-gameplay-preview-YYYYMMDD-HHMMSS') `
  -ExpectedProductVersion '<exact-product-version>' `
  -ExpectedExecutableSha256 '<exact-executable-sha256>' `
  -ExpectedSignerThumbprint '<exact-leaf-certificate-thumbprint>' `
  -ExpectedSignerSubject '<exact-leaf-certificate-subject>' `
  -ExpectedDpi 120 `
  -TargetWindowMode ForegroundSpectatorGame `
  -ExpectedGameFormat Standard
```

Spectator-game output uses artifact kind `mtgo_visible_spectator_gameplay_calibration_preview_v1` and records `capture_role = spectator`. It is still only a local calibration preview. It is not accepted by the reviewed desktop-preview contract and grants no OCR, evidence, scoring, or input authority. A spectator frame may inform duel-window identity and coarse battlefield layout, but it must not calibrate player hand, prompt, priority, legal-action, target-selection, or input regions.

Example for an already-open acting-player duel, including a League or Challenge match only after a separately approved non-spending launch:

```powershell
.\scripts\capture_visible_mtgo_preview_v1.ps1 `
  -OutputDirectory (Join-Path $env:TEMP 'mtgo-duel-preview-YYYYMMDD-HHMMSS') `
  -ExpectedProductVersion '<exact-product-version>' `
  -ExpectedExecutableSha256 '<exact-executable-sha256>' `
  -ExpectedSignerThumbprint '<exact-leaf-certificate-thumbprint>' `
  -ExpectedSignerSubject '<exact-leaf-certificate-subject>' `
  -ExpectedDpi 120 `
  -TargetWindowMode ForegroundDuelGame `
  -ExpectedGameFormat Modern
```

Acting-player duel output uses artifact kind `mtgo_visible_acting_player_duel_gameplay_calibration_preview_v1` and records `capture_role = acting_player_duel`. It remains pending manual review and unsafe for OCR, evidence, scoring, or input. The mode only gives the later calibration review the correct 1-on-1 window identity; it does not launch a match or grant control authority.

The retained spectator reconstruction audit makes that separation executable:
it can close visible duel participants and map the turn/phase layout, but it
keeps acting-player priority, private knowledge, and the complete legal-action
set blocked for every spectator source. The current Solitaire first-main
calibration is also not a duel-state substitute. Solitaire has eight cards at
that screen, while the kernel's trained starting-player first main correctly
has seven after the first-turn draw skip.

Example for an already-open Freeform Solitaire game:

```powershell
.\scripts\capture_visible_mtgo_preview_v1.ps1 `
  -OutputDirectory (Join-Path $env:TEMP 'mtgo-solitaire-preview-YYYYMMDD-HHMMSS') `
  -ExpectedProductVersion '<exact-product-version>' `
  -ExpectedExecutableSha256 '<exact-executable-sha256>' `
  -ExpectedSignerThumbprint '<exact-leaf-certificate-thumbprint>' `
  -ExpectedSignerSubject '<exact-leaf-certificate-subject>' `
  -ExpectedDpi 120 `
  -TargetWindowMode ForegroundSolitaireGame `
  -ExpectedGameFormat Freeform
```

Solitaire-game output uses artifact kind `mtgo_visible_solitaire_gameplay_calibration_preview_v1` and records `capture_role = acting_player_solitaire`. The scope label distinguishes an acting-player layout from a spectator layout, but it grants no OCR, evidence, scoring, or input authority. Any future action path still requires a separately trusted live-frame backend, a fully reconciled current decision, explicit authorization, and visible postcondition confirmation.

The stricter DXGI v2 artifact checker also recognizes the exact tuple `window_mode = duel_game`, `capture_role = acting_player_duel`, and a nonempty visible game format. Its `(1-on-1)` title rule requires one visible opponent, while the spectator rule requires two visible participants. A duel artifact remains checked-untrusted and cannot be promoted through the Solitaire calibration path or converted into OCR, policy, or input authority.

`validate_dxgi_bound_observation_reconstruction_audit_v1` binds a reconstruction-readiness audit to the exact checked DXGI manifest hash, canonical pixel hash, client size, gameplay kind, and capture role. Navigation sources and zero frame sequences reject. The result still exposes only readiness and blockers. Caller-supplied region labels and completeness declarations do not prove that their semantic interpretation matches the pixels, so this bridge grants no observation, scoring, or input authority.

`check_untrusted_dxgi_observed_decision_candidate_v1` binds that exact acting-player duel source and complete untrusted audit to a structurally valid one-frame observed decision. Its commitment includes the capture manifest, canonical pixels, output identity, capture time, audit, perception-profile commitment, and base decision. The opaque result exposes commitments only and cannot reach the model scorer or input path. This prevents the real source chain from being silently reduced to the legacy mock-frame commitment while perception remains unadmitted.

`check_untrusted_duel_perception_runtime_profile_v1` gives that profile identity a concrete runtime meaning. Its commitment binds the exact MTGO executable and signer, DPI, client size, monitor-output identity, game format, canonical pixel format, perception binary, classifier assets, card-database profile, and supported action families. A duel decision candidate rejects unless its checked DXGI source matches those runtime facts exactly.

`evaluate_untrusted_duel_perception_profile_v1` is the offline accuracy gate for that exact runtime profile. It validates each human-expected decision, binds any prediction to the same source frame and profile, recomputes exact observation, ordered legal-action, object-binding, and whole-payload matches, counts abstentions, and verifies that the evaluated action families exactly match the profile scope. Semantic accuracy is fail-closed at exact agreement for every non-abstained prediction; the declared prediction-coverage threshold must be at least 95 percent. Corpus, annotation protocol, evaluator binary, cases, predictions, and outcomes all enter one evaluation commitment.

The result remains checked-untrusted because corpus labels and provenance are external review inputs. `admit_ratified_duel_perception_profile_v1` has a private production ratification set to `None`, so no current profile is admitted. A future reviewed commit may pin one exact evaluation commitment after the acting-player duel corpus exists. Even that admitted type grants profile identity only. Trusted capture-time attestation and a separate source-bound live-decision admission are still required before model scoring, and input remains a later gate.

`evaluate_untrusted_competitive_pregame_profile_v1` defines the separate accuracy gate for the visible mulligan, London-bottoming, and GameplayReady states that occur before the kernel policy can act. The canonical surface contains all 44 valid stage/count combinations. Every state must have the declared number of unique, manually reviewed acting-player duel frames, every state has its own prediction-coverage floor, and every emitted prediction must exactly match its reviewed label. The profile is deliberately mode-independent: no-stakes acting-player duel captures may supply calibration examples when they match the exact duel-perception profile, while League or Challenge identity remains attached later by the paid-event runtime. The production evaluation root remains `None`, and even a future admitted profile grants only semantic identity, not live classification, event entry, scoring, or input authority.

`check_untrusted_competitive_pregame_public_context_v1` adds the visible play-or-draw and public best-of-three match-score facts needed by a model-backed pregame policy. It rehashes three canonical, nonoverlapping regions from the exact classified pregame frame and rejects impossible game and score combinations. `evaluate_untrusted_competitive_pregame_public_context_profile_v1` covers all eight legal play/draw and game-score states with unique reviewed captures, full per-state prediction coverage, and exact per-state labels and commitments. Its production ratification root is also `None`. The evaluation separately binds the evaluator and classifier binary hashes. The checked and admitted identities expose no pixels, coordinates, model-scoring route, or input authority.

`check_untrusted_competitive_pregame_public_context_classifier_request_v1` and its response checker define the second exact-pixel protocol. Its canonical request binds the existing stage classification, visible interaction, source frame, exact pixels, eight-state context evaluation, classifier binary, and runtime identity. The response must echo the exact request and supply one context candidate whose three regions rehash against the same frame. A later same-frame binder consumes both checked classifications into one move-only model context. The protocol result remains false for model scoring and input until the opaque Windows producer proves the exact evaluated binary was invoked.

`COMPETITIVE_SIDEBOARD_MODEL_INTERFACE_2026-08-11.md` records the remaining changed-sideboard checkpoint gap. The adapter already has the coordinate-free exact-inventory validator and the production-disabled one-card transfer and Submit Deck chain, but the kernel has no best-of-three match episode, public inter-game history, sideboard action vocabulary, or sideboard head. The required native surface uses a bounded local transfer-and-submit deliberation, emits no UI input while deciding, trains only from terminal match win or loss, and must be the sole production source of a changed target configuration.

`CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1` closes the adapter-owned portion of the public-history gap. It accepts only gameplay model selections whose exact newer visible postconditions were confirmed, sanitizes each state and selected action into the same player-visible duel contract used for current scoring, and privately retains the frame lineage and decision, selection, deployment, and postcondition commitments for one exact event, match, and game. The history is bounded, ordered, move-only, coordinate-free, and exposes only the sanitized player-visible decision to a future kernel match encoder. It still cannot score sideboarding or authorize input, and it deliberately makes no completeness claim for opponent-only transitions or the terminal game result. Those facts require separately classified player-visible sources before the native match head may consume a complete game history.

`parse_checked_untrusted_mtgo_visible_game_log_v1` is the first direct player-view source under the clarified information boundary. It accepts only the exact persisted `Match_GameLog_*.dat` framing observed in the local corpus, retains only the text MTGO renders in the Game Log, strips player-link markers, and converts card links to visible card names while discarding the source UUID, timestamps, flags, raw markup, and numeric card or object identifiers. Unknown versions, channels, flags, encoding, record order, and markup reject. The checked projection is not yet current-game evidence: a future Windows source binder must prove the selected file belongs to the exact open match before its projected text can extend competitive match memory.

The Windows binder is `probe_mtgo_process_epoch_visible_game_log_v1`. It brackets the read with two admitted unchanged seated-player game-window captures, rejects spectator and main-client windows, derives the ClickOnce data root and deployment from that signed MTGO process, requires exactly one canonical Game Log file created after that process started, verifies its filename and payload source commitments, and rejects a file that changes before the second capture completes. MTGO documents that the Game Log lets a player review previous game actions, so the binder may expose all rendered lines in that exact file rather than only the current scroll viewport. V1 deliberately rejects once a second Game Log is created in the same process; a future event lifecycle must provide an opaque match-start boundary before consecutive League or Challenge matches in one client process.

`classify_checked_untrusted_mtgo_visible_game_log_semantics_v1` conservatively converts only recognized rendered statements into typed public facts: joining the game, turns, opening-hand and mulligan counts, public draws, plays, casts, activated abilities, discards, attacks, concessions, game wins, match wins with visible score, and forced completion. It reduces player aliases to acting-player or opponent roles and retains only card names that were explicit visible card links. Unknown wording is counted but not guessed. The live binder exposes this classifier only over its already source-bound projection. The result supplements visual state but is not a complete observation, legal-action source, scoring capability, or input capability.

`begin_checked_untrusted_player_visible_game_log_action_baseline_v1` and `corroborate_checked_untrusted_player_visible_game_log_action_v1` use that typed projection as an independent visible postcondition for actions the rendered log states precisely enough. V1 supports playing a named land, casting a named spell, discarding an exact visible card multiset, and declaring an exact visible attacker multiset. The after snapshot must retain a private commitment over every exact prior rendered record, including unrecognized wording, and append only recognized records containing the matching seated-player fact. Passes, abilities, targets, modes, menu choices, blockers, and other unstated interactions reject instead of being inferred. The Windows crate provides narrow wrappers over its opaque competitive Game Log snapshot; paths, source identifiers, timestamps, raw text, link metadata, and transport commitments remain inaccessible. A corroboration result is evidence only and never authorizes input, event entry, or spending.

`summarize_mtgo_visible_game_log_semantics_v1` is an offline corpus-coverage command. It reports only file and record counts plus typed event-kind counts. It emits no rendered text, aliases, source identifiers, or card names. It counts logs whose visible opening says the account started watching as spectator sources and excludes them from acting-player coverage. Other files outside the current two-player semantic assumptions are counted as unsupported and skipped rather than weakened into a guessed projection.

`check_untrusted_competitive_pregame_classifier_exchange_v1` defines the exact pixel protocol for that future classifier. The canonical request binds one acting-player duel frame, exact client geometry and BGRA8 bytes, source-capture and frame-profile commitments, the duel and pregame profile admissions, and one classifier-runtime identity. The response must bind that exact request and provide the exact stage-specific visible fact set: prompt, hand, and Mulligan controls; prompt, hand, and bottoming controls; or prompt and gameplay surface. Every distinct region is rehashed from the request pixels at the normal game-information confidence floor. The checked result discards pixels and rectangles and exposes only the stage and commitments. It remains untrusted until a future opaque capture/runtime producer proves it invoked the pinned executable, and it has no event, scoring, or input conversion.

`score_and_select_profile_bound_duel_candidate_v1` closes the offline compatibility-scoring side of that later gate. It consumes one source-bound duel decision candidate, requires the exact admitted perception-profile commitment, calls the existing external-observation checkpoint seam with the candidate's private validated decision, and binds source, profile admission, deployment, response, and deterministic selection in one move-only result. That kernel seam still tensorizes simulator bookkeeping from the complete observation, so it is not a player-visible-only competitive scorer and the top-level operator must not dispatch it. The result exposes the selected semantic for later visible-control resolution but cannot create input or enter an event. Because the candidate remains checked-untrusted rather than an opaque in-process capture proof, the Windows-side live bridge must separately preserve the opaque DXGI frame through perception before any future replacement scorer can become actionable.

`build_player_visible_duel_decision_input_v1` defines the exact owned replacement input at the adapter boundary. It projects a validated current decision into turn, phase, active and priority player, initiative, life, mana, zone counts, visible public zones, visible battlefield state, stack, combat assignments, visible object relations, the seated player's hand, legitimately known cards, and the exact ordered visible legal choices. Players are expressed only as `seated_player` or `opponent`, and every two-player array plus object traversal is reordered to that same relative perspective. Object links use decision-local ordinals assigned from this relative visible traversal and action order. Opponent face-down plotted cards remain visible exile objects but have no card name. A stack source name remains absent until the perception contract carries its separately reconstructed visible label, rather than being inferred from a private card-database ID. Ability, mode, effect-option, and trigger-order choices use legal-list or visible-object ordering rather than copied kernel indices. Kernel seat labels, stable references, card-database numbers, face indices, zone-incarnation counters, and owner or controller identifiers embedded in those references are not copied into the output. The input also contains no `ObservationV5`, engine, harness, or policy context, contract metadata, frame or source lineage, adapter binding IDs, process data, pixels, coordinates, or authority. Its commitment is invariant under kernel-seat swapping and changes to excluded kernel bookkeeping, and changes under visible facts or legal-action order. A future kernel scorer should accept this field contract or an equivalent kernel-owned type rather than the complete observation.

`MtgoPlayerVisibleDuelScorerV1` is the transport-independent replacement scorer seam. It receives only that sanitized input in exact visible action order and returns one finite logit per visible action plus a finite value estimate. `score_and_select_player_visible_profile_bound_duel_candidate_v1` verifies the exact input and deployment commitments, selects a deterministic maximum with first-action tie breaking, and privately maps the selected visible index back to the source semantic. `resolve_player_visible_profile_bound_selected_visible_control_v1` then proves that semantic maps to exactly one enabled control on the same current frame. Its result exposes only the selected visible action and false authority flags. Control IDs, coordinates, frame metadata, capture lineage, source commitments, and the kernel semantic remain private. These checked-untrusted seams grant no input, event-entry, or spending authority. The current kernel deployment still lacks the corresponding player-visible-only checkpoint implementation.

For the direct producer path, `refresh_direct_visible_selection_before_dispatch_v1` requires a second exact producer observation after scoring. The complete serialized visible state and ordered legal-action bytes must be identical, and the selected visible action must remain at the exact index. The in-process producer then independently rebuilds the sanitized decision and private action binding on MTGO's WPF dispatcher before its exact decision-hash check. Thus a changed prompt, board, action order, or abstention cannot carry a stale model selection into the private dispatch seam. The refreshed wrapper is still coordinate-free and non-authorizing; it exposes no client action, dispatch method, process handle, event-entry, or spending capability.

[`PLAYER_VISIBLE_NATIVE_SCORER_V1.md`](PLAYER_VISIBLE_NATIVE_SCORER_V1.md) specifies the smallest kernel-owned implementation for that missing scorer. It maps exact visible card names and decision-local object ordinals into a private compatibility tensor, fixes unavailable simulator-only features to documented neutral values, imports confirmed actions and rendered Game Log facts as separate ordered streams, and forbids reconstruction of `ObservationV5`. The existing checkpoint can use this path only as an explicitly measured compatibility scorer; it must report that public history is not model-consumed until a history-trained deployment exists.

[`PLAYER_VISIBLE_AUXILIARY_HEADS_V1.md`](PLAYER_VISIBLE_AUXILIARY_HEADS_V1.md) specifies the missing checkpoint-backed mulligan, London-bottoming, and sideboard heads plus their opaque adapter return contract. It closes a strategic omission in the current safe payloads: game-two and game-three decisions need the sanitized facts visibly observed in every completed prior game, not only our deck and the public score. The shared history preserves confirmed decisions and rendered Game Log facts as separate ordered streams, excludes hidden opponent information and transport metadata, and feeds both auxiliary heads without reconstructing `ObservationV5`. Terminal match win or loss remains the only reward and promotion measure.

`MtgoPlayerVisibleDuelGesturePlanV1` is the downstream coordinate-free replacement for the legacy kernel-object gesture schema. It carries the exact selected player-visible action and a complete primitive sequence. Object selection and trigger ordering use only the sanitized `visible_ordinal` references already present in that action. The plan contains no kernel stable references, card-database IDs, frame metadata, source commitments, control IDs, coordinates, authorization, or input method. Validation covers direct activation, context menus, calibrated play-area drag, compound visible-object selection, trigger ordering, and Submit while granting no authority.

`resolve_profile_bound_selected_visible_control_v1` carries that same move-only chain into the existing complete visible-control resolver. It requires one unique enabled current-frame control for the selected legal semantic, preserves source, profile admission, deployment, model-selection, and control-resolution commitments, and keeps the rectangle private. The result is still checked-untrusted and cannot produce input or competitive entry.

`validate_profile_bound_duel_gesture_plan_v1` binds a complete coordinate-free gesture blueprint to that exact selected control. The grammar covers all eleven duel action families with direct single or double activation, a fresh-frame context-menu continuation, calibrated play-area drag, exact semantic object selection, ordered trigger placement, and explicit submit. Every primitive is its own stage. Stage zero is tied to the source decision frame, every later stage requires a strictly newer visible continuation frame, and only the final stage may declare the selected semantic complete. Discard, attacker, blocker, and trigger plans must name exactly the objects and order carried by the selected kernel semantic. The checked wrapper remains move-only and has no coordinates, timing, input API, or event-entry authority.

`bind_visible_duel_gesture_stage_v1` resolves one of those stages against a complete same-frame target set. The target vocabulary is closed to the primary semantic control, semantic menu choice, exact bound object, calibrated play area, exact order slot, and submit control. Every target must be enabled, high-confidence current-frame pixel evidence; each required role must resolve exactly once, and drag endpoints must be visibly distinct. The source stage must use the original decision frame, while a continuation must be distinct and strictly newer with the selected semantic still legal. Rectangles remain private and the checked result cannot input.

`evaluate_untrusted_duel_gesture_profile_v1` provides the reviewed-corpus gate for the gesture plan and target detector. It requires all eleven action families in canonical order, unique source frames, explicit visual review, a manually demonstrated gesture sequence, and a confirmed visible postcondition for every case. Prediction coverage is enforced separately for every family at a declared threshold of at least 95 percent, and every non-abstained predicted plan must exactly equal the reviewed plan. One wrong stage, primitive, object order, or source binding fails the gate. The production evaluation root is empty, and even the admitted profile type is identity-only and false for live input.

`check_untrusted_player_visible_gameplay_postcondition_v1` is the all-family replacement postcondition contract. It binds the exact sanitized player-visible decision, complete coordinate-free visible gesture, exact primitive stage, strictly newer frame, and a complete nonoverlapping set of changed player-visible regions. The required region categories differ by action family, so an unrelated repaint cannot confirm an action. Optional Game Log corroboration is accepted only for the final primitive, exact pre-input baseline, exact player-visible decision, and supported rendered event kind. The checked result can extend the player-visible game-history ledger but remains structurally untrusted and always false for more input, event entry, or spending. A future opaque Windows capture producer must recompute the region hashes from retained before and after DXGI pixels before this contract can participate in a live session.

`check_untrusted_player_visible_gameplay_before_input_v1` fixes that complete action-specific visible region set before any gesture. `complete_untrusted_player_visible_gameplay_postcondition_v1` then consumes the checked declaration and accepts only the identical ordered kinds and client rectangles from a strictly newer frame. A producer cannot add, remove, relabel, move, or substitute a region after input, and every declared region must visibly change. The before-input wrapper exposes only its commitment and false authority flags. This is the transport-independent half of the opaque Windows before/after producer; structural records alone remain untrusted.

`prepare_direct_visible_gameplay_before_dispatch_v1` applies the same action-specific visible-change rule to the direct client-action path. It consumes the exact League or Challenge match-bound refreshed selection and fixes the source frame plus complete nonoverlapping player-visible region set before dispatch. Its coordinate-free commitment view gives the Windows operator only the exact event, authorization, model, producer-result, selected-index, binary, and source-frame commitments needed to join the action to its opaque session and process-wide input gate. It exposes no client object, pixels, rectangle, process handle, or independent input authority. `complete_direct_visible_gameplay_postcondition_v1` then requires a strictly newer composed frame, identical ordered visible regions, and a changed hash for every declared region. Supported rendered Game Log facts may corroborate the result but never replace the composed-frame transition. The confirmed direct result extends `CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1` through the same sanitized state and selected-action record as the gesture path, without inventing mouse primitives or recording the private client transport.

`prepare_profile_bound_action_postcondition_plan_v1` binds that resolved action to its complete calibrated visible change set before any future input. `check_untrusted_profile_bound_action_postcondition_v1` then requires the selected control and every action-specific region to change on a strictly newer byte-distinct frame with the same output identity. The offline result remains non-actionable because its producer-facing after-frame hashes are not an opaque live DXGI proof. A Windows actuator must retain the opaque before and after frames and perform the same checks before allowing another input.

`bind_profile_bound_action_plan_to_competitive_match_v1` joins gameplay wiring to the separately checked competitive lifecycle. It requires the lifecycle and model action plans to interpret the exact same match-in-progress frame, then binds the event kind, event and match identities, game number, account, written permission, exact entry authorization, and an owner launch authorization with a frame-sequence expiry. League and Challenge mode scopes remain distinct. The result remains checked-untrusted and has no input or entry conversion.

The coordinate-free `MtgoProfileBoundActionPostconditionPlanCommitmentsV1` view exposes the exact decision, selection, control resolution, source manifest, raw frame, output, geometry, profile admission, deployment, and postcondition-plan commitments without exposing a region or coordinate. The Windows capture crate uses that view to prove that its retained opaque DXGI-to-control chain and the separately checked competitive plan describe the same action before retaining them together.

The attended navigation helper `scripts/invoke_supervised_mtgo_click_v1.ps1` emits exactly one left-click after checking the exact process ID and start time, executable and signer identity, DPI, title, client size, foreground ownership, client-to-screen transform, visible hit-test root, and cursor position. An explicitly empty expected title is accepted only for the exact foreground MTGO-owned window because MTGO uses untitled top-level windows for some visible deck dialogs. It has no keyboard or text-input path. It does not identify the semantic control at the point, does not verify any postcondition, and is explicitly unsafe for autonomous input, purchases, or queue entry. Every use must be bracketed by manually inspected visible previews and must stop on an unexpected state.

## Supervised pregame transition

`MtgoPregameCalibrationTraceV1` records a digest-only, adapter-local Keep or Mulligan calibration transition. It binds distinct before and after preview manifests and frames, the visible action-control region before the manual action, and multiple changed visible regions after it. Keep requires changed prompt, player-count, game-log, and phase-bar regions. Mulligan requires changed prompt, player-count, and game-log regions.

The stricter DXGI Keep path is `check_untrusted_dxgi_keep_transition_v1`. It takes two already checked role-explicit artifacts plus their canonical raw bytes and a digest-only transition record. Both frames must be `ActingPlayerSolitaire`, share client and output identity, and be strictly ordered. The Keep control must lie inside the prompt region. Prompt, player counts, visible game log, phase bar, and hand regions must appear in canonical order, be nonoverlapping, and each change by at least one percent of pixels. The checked result retains no pixels or coordinates and remains unsafe for semantic evidence, scoring, or input. The first live record is `fixtures/dxgi_keep_transition_20260810_v1.json`.

The checked result is deliberately named `CheckedUntrustedMtgoPregameCalibrationV1`. It exposes only the adapter-local pregame semantic, source frame hashes, and a transition commitment. It has no region, pixel, kernel-evidence, policy, or input accessor. The source previews remain non-actionable, and caller-provided region labels do not prove visual correctness.

This local semantic is separate because the current kernel `ActionSemanticV1` does not represent Keep or Mulligan. The first live trace therefore establishes adapter calibration and a visible postcondition, not model support for mulligan decisions.

`MtgoGameplayCalibrationTraceV1` records the first adapter label that maps to a kernel semantic. V1 accepts only a local-seat `Pass`, binds the visible action-control region, and requires distinct prompt and phase-bar postconditions. Its strict adapter wrapper rejects coordinates or unknown fields inside the action label before constructing the exact kernel `ActionSemanticV1::Pass` value.

The checked gameplay trace is still not a validated decision. It lacks a complete `ObservationV5`, complete ordered legal-action set, object bindings, trusted pixels, and measured semantic recognition. In particular, the MTGO Combat phase shortcut may correspond to more than one internal priority transition in some states. The current trace is a supervised alignment candidate, not proof of universal one-to-one action equivalence.

`MtgoVisibleObjectActionCalibrationTraceV1` records the next supervised slice: a visible Island moved from hand to battlefield after one left-click. The trace requires changes in the prompt, player counts, battlefield, hand, and visible game log. It keeps the source as an adapter-local object ID plus visible card name and does not mint a kernel `CardStableRefV1`. A future complete observation must create that exact binding, and the battlefield object must be a new zone-change incarnation.

The same trace contract also covers a supervised Island mana activation. It requires both a visible tapped-object change and a visible mana-pool change. The action records `mana_choice = null` for the single-output interaction plus the visibly added blue mana, while still withholding a kernel semantic until the source has an exact observation binding.

The live input check also demonstrated that Windows input geometry must be Per-Monitor V2 DPI-aware before any coordinate is interpreted. A future actuator must additionally bind the physical point to the expected foreground MTGO HWND and PID with `WindowFromPoint` immediately before input. The calibration record contains no input coordinate or callable input path.

## Observation reconstruction audit

`MtgoObservationReconstructionAuditV1` turns the next integration gap into a fixed ten-group inventory. It separately accounts for duel participants, turn/phase/priority, public player state, public objects and zones, acting-player private knowledge, stack/combat/pending choices, kernel decision-history context, object incarnations and card-database bindings, the complete ordered legal-action set, and local kernel contract metadata.

Each group must be visibly complete, locally derived complete only where explicitly permitted, or incomplete with stable reason codes. The validator requires canonical group order, source-frame and region commitments, exact readiness declarations, and all preview safety flags false. A Solitaire topology can never claim a complete two-player observation or model-scoring readiness.

The live Solitaire audit is `fixtures/solitaire_observation_reconstruction_audit_v1.json`. It records six blockers: no distinct opponent, no second-player public state, no distinct opponent zones, missing reconstruction of kernel decision-history context, incomplete stable object incarnations, and no proof of the complete ordered legal-action set. The checked wrapper exposes those blocker categories but cannot produce an `ObservationV5`, actions, pixels, scores, or input.

This audit also makes a core compatibility issue explicit. `ObservationV5` contains kernel-specific history fields such as priority-pass history, recent stack and mana activity, and policy-surface context. Those are not all directly visible in a single MTGO frame. A production adapter needs a versioned history-reconstruction profile based only on prior visible frames and confirmed visible actions, with any unreconciled field blocking scoring.

`CheckedUntrustedMtgoVisibleHistoryV1` now joins already checked calibration transitions into a caller-ordered visible-history digest. Every adjacent transition must either share the exact intermediate frame hash or declare a reason-coded capture gap. The live sequence contains four supervised actions, two gaps, and a trailing two-action exact frame chain from PlayLand through Island mana activation. Source order across a gap remains a caller declaration, and even a fully exact pixel chain remains unsafe for kernel context because unchanged pixels cannot prove that no unobserved transition occurred. The type has no engine-context, observation, scoring, or input conversion.

## Reviewed capture contract

The Rust crate now defines an untrusted structural contract for the later production capture backend without implementing that backend. A calibration profile binds the exact bytes and decoded pixels of one manually reviewed preview, exact client identity, DPI, client size, monitor-output identity, canonical BGRA8 format, and stable pixel anchors. A separate review record must bind the profile and source preview and explicitly confirm that the preview is client-only, unobscured, cursor-free, identity-matched, and anchor-reviewed.

The public checkers deliberately return `CheckedUntrusted...` types. They recompute preview, whole-frame, and anchor hashes, require at least four substantial anchors distributed across all four client quadrants, parse and order real UTC timestamps, and check structural DXGI, identity, layout, freshness, and safety claims. They expose no raw pixels and have no conversion to OCR, mock evidence, policy input, action intent, or input authority.

This structural checker is not proof that a human performed the review or that a producer truthfully evaluated each capture-time assertion. The crate now has a narrow reviewed-preview admission seam, but its private production ratification is deliberately `None`. It therefore admits no current artifact. A later separately reviewed commit may pin one domain-separated commitment binding the exact profile, review, manifest, PNG, and decoded BGRA pixels after manual inspection.

An admitted reviewed-preview value is opaque, retains the exact pixels without exposing them, and has only fixed offline-calibration-preview scope. It is not serializable, cloneable, or debuggable and has no conversion to OCR, semantic evidence, policy scoring, a live frame, an action intent, or input authority. Caller-provided review booleans cannot create it. A production DXGI probe still needs a separate opaque live-frame admission type and trusted capture-time evidence before OCR.

## DXGI candidate artifact ingestion

The separate `../mtgo_dxgi_capture_v1` executable now produces a real composed-desktop DXGI capture candidate with exact executable and signer identity, foreground and geometry snapshots, conservative occlusion and cursor checks, DXGI presentation metadata, tightly packed BGRA8 pixels, and a PNG preview. The probe has no input API and marks every output unsafe for semantic evidence, OCR, policy scoring, and input.

`check_untrusted_dxgi_capture_artifact_v1` strictly parses that manifest, requires identical pre/post snapshots, recomputes both file hashes, decodes the PNG internally, and proves that it represents the exact canonical BGRA8 pixels. It returns only an opaque checked-untrusted value containing commitments, dimensions, and one role. Legacy v1 is main-client navigation only. V2 distinguishes navigation, acting-player Solitaire, and spectator frames with exact role and format title rules, so those sources cannot be interchanged. The checked type retains no pixels and has no conversion to calibration, OCR, evidence, scoring, an action, or input. A small read-only checker binary exercises the full file boundary:

```powershell
cargo run --bin check_mtgo_dxgi_artifact_v1 -- 'C:\absolute\artifact-directory'
```

This closes the raw producer-to-adapter file-schema gap. It does not close the trust gap. The manifest safety assertions remain producer claims, the pre/post z-order checks retain a transient-occluder race, and no current profile or live frame is ratified for perception.

The exact manually inspected Freeform acting-player artifact from 2026-08-10 is separately source-ratified as `ActingPlayerSolitaireOnlyV1` for offline calibration. Its domain-separated admission commitment binds the exact manifest, canonical BGRA8, PNG, output identity, dimensions, timestamp, and role. The opaque admitted type retains the pixels only for crate-internal offline calibration code and exposes no raw pixel accessor. It remains unsafe for live OCR, semantic evidence, policy scoring, action selection, and input. Runtime manifests or caller-provided review assertions cannot ratify another artifact.

The read-only admission checker exercises that exact boundary:

```powershell
cargo run --bin admit_mtgo_dxgi_offline_calibration_v1 -- 'C:\absolute\artifact-directory'
```

This admits one immutable image for measuring perception. It does not ratify a reusable calibration profile, a later frame, or the producer's live assertions.

`recognize_ratified_offline_opening_hand_v1` is the first exact pixel-to-semantic calibration consumer. It is compiled against four reviewed regions in that one admitted frame: prompt text, Mulligan control, Keep control, and hand count. It verifies every region hash and returns a seven-card opening-hand decision with ordered adapter-local actions `Mulligan { next_hand_size: 6 }` then `KeepOpeningHand`. The result retains no pixels or coordinates and remains unsafe for a live frame, semantic evidence, `ObservationV5`, policy scoring, or input. It is one exact example, not an accuracy measurement or a reusable recognizer.

The read-only recognizer exercises that exact boundary:

```powershell
cargo run --bin recognize_mtgo_offline_opening_hand_v1 -- 'C:\absolute\artifact-directory'
```

`classify_untrusted_offline_opening_hand_candidate_v1` applies the same four-region profile to later role-correct DXGI artifacts with the exact reviewed client size and output identity. Raw bytes must still match the checked artifact. A result contains only Match or NoMatch, matched-region count, and commitments, with every runtime authority flag false. The first broad prompt region failed on a second true frame because it included an animated cyan glow; the current text-only region removes that unstable pixel area.

The checked-untrusted classifier can be run on an offline artifact with:

```powershell
cargo run --bin classify_mtgo_offline_opening_hand_candidate_v1 -- 'C:\absolute\artifact-directory'
```

The first deliberately narrow corpus is `fixtures/offline_opening_hand_classifier_corpus_20260810_v1.json`: five manually inspected frames from two no-cost Freeform Solitaire games. It contains three seven-card opening-hand positives, one post-Keep first-main negative, and one six-card mulligan negative. The exact template produced 3 true positives, 2 true negatives, and no observed errors. This is a wiring and brittleness screen, not a general accuracy claim.

`classify_untrusted_offline_mulligan_ladder_candidate_v1` extends that offline-only slice across every visible London mulligan choice from prospective keep seven through prospective keep one. MTGO continues to display seven cards while the prospective keep size decreases, so the semantic state comes from the visible prompt rather than the displayed hand count. The fixed classifier hashes an exact binary ink mask from the prompt-text interior using a BGR sum threshold of 384 and excluding the animated cyan border. This removes same-side grayscale anti-alias intensity changes, but any changed mask bit still fails closed. The checked capture artifact binds the acting-player Solitaire role, exact client size, output identity, and raw-frame hash before classification.

`classify_untrusted_offline_mulligan_ladder_candidate_v2` preserves the predecessor and uses a separately committed dark-core prompt profile. Its BGR sum threshold of 182 is the midpoint of the widest same-label stability interval measured across a three-game development corpus. It matched all 21 development prompts, rejected four gameplay screens, and then matched every prospective keep size in one later heldout fixed-deck Solitaire game. The retained heldout record is `fixtures/offline_mulligan_ladder_classifier_corpus_20260810_v2.json`. This remains a one-layout brittleness check and grants no runtime authority.

```powershell
cargo run --bin classify_mtgo_offline_mulligan_ladder_candidate_v2 -- 'C:\absolute\artifact-directory'
```

`classify_untrusted_offline_first_main_candidate_v1` recognizes one narrow post-Keep outcome. Its fixed exact-pixel profile binds the visible first-main prompt, Combat control, Turn 1 label, and empty upper battlefield. Those four regions were identical in two reviewed acting-player Solitaire captures: one after a seven-card Keep and one after completing a six-card London bottoming sequence. The result is checked-untrusted, retains no pixels or coordinates, and does not create `ObservationV5`, policy, or input authority.

`classify_untrusted_offline_first_main_candidate_v2` preserves the exact Turn 1 and empty-battlefield regions and replaces only the prompt and Combat control with dark-core binary ink masks at threshold 182. The two raw regions drifted in a later fixed-deck first-main capture while both binary masks and the other two exact regions remained unchanged.

`classify_untrusted_offline_first_main_visible_card_identities_v1` accepts only that exact v2 first-main result, the same checked capture, and a checked-untrusted visible-card template profile. It evaluates the reviewed eight-card overlap geometry after the acting-player draw and exposes names only when every ordinal clears both fixed template thresholds. Partial matches expose only a count. The current one-game supervised check matched all eight fixed-deck basics, but it remains a caller-labelled calibration result with no semantic-evidence, `ObservationV5`, model-scoring, object-binding, coordinate, or input authority.

`check_untrusted_first_main_kernel_card_coverage_v1` composes that complete eight-card identity result with the compile-bound kernel correspondence profile without creating bindings. It reports full support for the five visible Islands and `MissingFromKernelRegistry` at the three visible Plains ordinals in the retained first-main hand. Coverage therefore remains incomplete and the result exposes no object-binding, `ObservationV5`, scoring, or input authority.

The read-only classifier can be run on an offline artifact with:

```powershell
cargo run --bin classify_mtgo_offline_first_main_candidate_v2 -- 'C:\absolute\artifact-directory'
```

The historical exact-pixel corpus remains `fixtures/offline_first_main_classifier_corpus_20260810_v1.json`. The v2 corpus is `fixtures/offline_first_main_classifier_corpus_20260810_v2.json`: two positive first-main frames across two games and four opening, mulligan, or bottoming negatives across three games total. The later fixed-deck positive is the renderer-drift frame that v1 rejected. All six v2 classifications matched their supervised labels. This is wiring and narrow robustness evidence, not an accuracy estimate or evidence of generalization. The exact empty-battlefield anchor deliberately limits this profile to the current basic-land Solitaire calibration scenario.

Exactly one of the seven prompt profiles must match before the result exposes a prospective keep size and ordered adapter-local actions. A zero-match or multi-match result exposes no actions. Every result retains no pixels or coordinates and remains unsafe for a live frame, semantic evidence, `ObservationV5`, policy scoring, or input.

The read-only classifier can be run on an offline artifact with:

```powershell
cargo run --bin classify_mtgo_offline_mulligan_ladder_candidate_v1 -- 'C:\absolute\artifact-directory'
```

The ladder corpus is `fixtures/offline_mulligan_ladder_classifier_corpus_20260810_v1.json`: twelve manually inspected frames from three no-cost Freeform Solitaire games. It covers every prospective keep size from seven through one, three additional seven-card frames, one additional six-card frame, and one post-Keep first-main negative. All eleven prompt frames matched their human label, the gameplay negative did not match, and no result was ambiguous. The added six-card frame exposed 17 raw anti-alias pixel changes that caused the earlier exact-pixel matcher to reject the visibly identical prompt; the binary ink mask had zero changed bits and recovered the label. Sizes five through one still have only their source frames, so this is wiring coverage and a small brittleness screen, not an accuracy estimate or evidence of generalization.

The next visible state after keeping fewer than seven is a sequential London bottoming interface. MTGO keeps the instruction prompt visible, removes each clicked card immediately, and reflows the remaining hand. After the required number of selections, a distinct Done control appears. The prompt states that the last selected card becomes the bottom card of the library. A correct adapter must therefore preserve stable object identities and click order while rebuilding the complete legal-action set from the current visible hand after every selection. Reusing stale card slots or coordinates would target the wrong object after the first reflow.

`validate_untrusted_offline_london_bottoming_trace_v1` checks one complete offline calibration sequence against eight exact checked-untrusted acting-player Solitaire artifacts: seven selection states and the post-Done frame. It requires seven original adapter objects, a distinct `selected_for_bottom_click_order`, one disappearing object per state, preserved relative order for every remaining object, strictly increasing frame timestamps, exact manifest and raw-frame bindings, one shared client and output layout, and a complete current action declaration at every step. The click-order name is intentional because it does not claim final library order. At zero selected, the action set contains only the seven card selections because the visible Cancel control is a no-op. Selection states one through five add Cancel after the current card selections. The final state generates Submit then Cancel.

The live fixture is `fixtures/offline_london_bottom_six_trace_20260810_v1.json`, SHA-256 `d7b51c3e449eedfcaa2a41335ce41ac15b112c929f7d594ab2532f402f7cd19f`. Its supervised path selected the first six original hand objects in visible order and kept the seventh. The visible postcondition says six cards were put on the bottom, the Solitaire game began with one card, and a Solitaire draw produced a two-card first-main hand. That draw behavior is recorded as Solitaire-specific and is not generalized to competitive play. The exact trace commitment is `4ab9b5b8710cb017e3734d72c20756768a5bb536ff1a1eb97b7779542d5e9645`.

`classify_untrusted_offline_bottom_six_initial_candidate_v1` closes one narrower perception boundary at the start of that trace. It requires the exact visible bottom-six prompt, Cancel-only control, Turn 1 label, and structural occupancy in the seventh hand slot. The occupancy predicate counts pixels whose brightest color channel exceeds 48 and requires at least 200 of 900 pixels, so it distinguishes the reviewed zero-selected state from the first reflow without binding to one card name or artwork. A Match exposes only `required_bottom_count = 6` and `selected_count = 0`; every runtime authority remains false.

```powershell
cargo run --bin classify_mtgo_offline_bottom_six_initial_candidate_v1 -- 'C:\absolute\artifact-directory'
```

The corresponding corpus is `fixtures/offline_bottom_six_initial_classifier_corpus_20260810_v1.json`: one positive state from one Solitaire game and four negatives spanning one-selected, six-selected, an opening hand, and first main. It had no observed error in those five frames. This is wiring and brittleness evidence, not an accuracy estimate, and it does not support required bottom counts one through five.

`classify_untrusted_offline_bottom_six_state_candidate_v1` extends structural recognition across all seven reviewed selection stages for required bottom count six. It requires the exact bottom-six prompt and Turn 1 label, the stage-appropriate controls, and a seven-probe thermometer pattern over the visible hand. Exactly the first `visible_hand_count` probes must contain at least 900 bright pixels out of 2,160, and every later probe must be empty. A Match exposes selected count, visible hand count, whether Done is visible, and the complete action cardinality. It deliberately does not expose card identities, card coordinates, or input authority.

```powershell
cargo run --bin classify_mtgo_offline_bottom_six_state_candidate_v1 -- 'C:\absolute\artifact-directory'
```

The corresponding corpus is `fixtures/offline_bottom_six_state_classifier_corpus_20260810_v1.json`. It contains all seven positive stages from one Solitaire game plus opening-hand and first-main negatives from two games total. It had no observed error in those nine frames. Because all positive stages come from one game, this is structural wiring evidence only, not an accuracy estimate or evidence of generalization to another game, layout, deck, or required bottom count.

The exact prompt pixels changed slightly in a later Solitaire game even though the visible text and all binary ink decisions were unchanged. `classify_untrusted_offline_bottom_six_state_candidate_v2` preserves the v1 role, layout, raw-frame, Turn 1, control, and occupancy measurements but checks the instruction text with an exact binary ink mask at a fixed threshold. `fixtures/offline_bottom_six_state_classifier_corpus_20260810_v2.json` records all seven bottom-six stages from that calibration game, one independently captured zero-selected positive from a second positive game, and four nearby negatives across three games total. It had no observed error in those twelve frames. The held-out positive supports narrow prompt robustness across a second game. It is not an accuracy estimate or evidence for another layout, deck, required bottom count, or later reflow stage in the held-out game.

`classify_untrusted_offline_bottom_six_state_candidate_v3` also replaces the raw Cancel and Done control hashes with exact binary control-ink masks. It matched all seven retained selection stages, two earlier independent zero-selected states, both fixed-deck zero-selected states, and rejected four nearby non-bottom-six states. Live Cancel calibration then corrected one semantic detail: the displayed Cancel control does nothing at zero selected, so that stage exposes seven actionable card selections rather than eight visible controls. The earlier v2 corpus remains an explicit historical record of the pre-calibration count. The current `fixtures/offline_bottom_six_cancel_corpus_20260810_v1.json` binds a newer fixed-deck game in which the visible zero-stage Cancel was a no-op and Cancel from every selected count one through six restored zero selected and seven visible cards. The state output still contains only counts, commitments, and false authority flags.

`classify_untrusted_offline_bottom_six_reflow_candidate_v1` compares two consecutive matched stages using only the unchanged visible card-art band above the keyboard overlay. It evaluates every possible order-preserving single-card deletion. A Match requires exactly one deletion whose every remaining-card mean absolute BGR difference is at most 25.000 intensity levels. No deletion or multiple plausible deletions fails closed. The six observed transitions had matched differences from 0.433 through 19.571 while incorrect pairings in the exploratory matrix began above 46. The corresponding corpus is `fixtures/offline_bottom_six_reflow_corpus_20260810_v1.json`.

`classify_untrusted_offline_bottom_six_reflow_candidate_v2` preserves that deletion rule but requires both endpoints to pass the current v3 binary-control state gate. This prevents an in-process state accepted by the current classifier from being rejected or reinterpreted by the legacy raw-control classifier during the next transition check.

`fixtures/offline_visible_card_public_reference_probe_20260810_v1.json` records the first visible-card identity feasibility probe. Seven manually read card labels in one opening hand were compared against six pinned public print templates. Every ordinal selected the corresponding visible name; the weakest winning correlation was 0.6441 and the weakest margin over another template was 0.2105. The public image hashes and print identifiers are recorded, but the images remain in a local cache and are not committed. The source capture is still pending visual review, the candidate universe is manually constrained, and every authority flag remains false. This is not an accuracy estimate or a kernel card binding. The proposed calibration and runtime boundary is specified in `MTGO-VISIBLE-CARD-IDENTITY-V1-DESIGN.md`.

`resolve_checked_untrusted_kernel_card_correspondence_v1` is the next fail-closed card-database seam. It accepts only an exact, untrimmed kernel card name, requires the compile-bound definition to have `Full` capability, distinguishes tokens from deck cards, and binds the result to a profile commitment over the complete supported subset and `KERNEL_CARDDB_HASH`. The current registry has 136 definitions and only 49 fully supported entries, including four tokens. Unknown names, unsupported registered cards, case changes, whitespace changes, and control characters reject. The caller-provided name is not visual evidence, so the result cannot create a stable object reference, `ObservationV5`, score, or input. Open League and Challenge opponents can expose cards outside this supported subset, which remains a separate model and engine coverage blocker even after UI wiring is complete.

`CheckedUntrustedMtgoVisibleObjectLedgerV1` implements the first narrow object-incarnation slice over the retained Solitaire transitions. Canonically ordered seeds receive deterministic adapter-local arena identifiers after exact card correspondence. The checked PlayLand fixture preserves the Island's arena identifier, replaces its visible object identifier, moves Hand to Battlefield, and increments `zone_change_count` from zero to one. The immediately following checked mana activation preserves that battlefield reference without another increment. Frame, actor, controller, card name, source zone, replacement identifier, and action shape mismatches reject. The ledger is still checked-untrusted calibration state: it exposes only counts and commitments, cannot yield object bindings outside crate tests, and grants no observation, scoring, or input authority.

`classify_untrusted_offline_play_land_hand_reflow_v1` now binds that bookkeeping rule to the exact retained 1550 by 925 PlayLand manifests and PNGs. It verifies the pending-preview identity, chronology, foreground, occlusion, cursor, and no-authority fields, decodes the legacy PNGs internally, and compares all eight pre-action card-art regions with all seven post-action regions. Exactly one order-preserving deletion passes: ordinal zero, matching the declared Island source. The widest surviving-card distance is 26.989, the best alternative deletion needs 66.022, and the 39.033 separation exceeds the fixed 10.000 margin. The record is `fixtures/offline_play_land_hand_reflow_20260810_v1.json`.

The historical hand contains kernel-unsupported survivors, so this retained pair calibrates geometry and source-slot alignment only. The opaque result can advance an eight-card supported Kernel Basics ledger in tests, preserving all seven survivor arena identifiers and moving only the played Island to a new battlefield incarnation. A future live supported-deck transition must provide its own complete exact card coverage before that bridge can contribute to an observation. Every semantic-evidence, observation, scoring, and input flag remains false.

```powershell
cargo run --bin classify_mtgo_offline_play_land_hand_reflow_v1 -- `
  'C:\absolute\solitaire_play_land_transition_v1.json' `
  'C:\absolute\before-preview-directory' `
  'C:\absolute\after-preview-directory'
```

`check_untrusted_offline_visible_card_template_profile_v1` validates the first local runtime-profile shape. It fixes the reviewed layout, a 96 by 60 BGR template, the 25.000 mean absolute difference ceiling, and a 10.000 distinct-name margin. Template bytes are bound to their visible calibration capture, public print metadata, public-image hash, and deck commitment. `classify_untrusted_offline_bottom_six_visible_card_identities_v1` then requires a matched bottom-six frame and identifies the complete visible hand or returns `NoMatch`; partial results and distinct-name ties are withheld. The checked profile and result expose no template pixels or coordinates, cannot create `CardStableRefV1`, and keep all semantic, observation, scoring, and input authority false. Caller-supplied labels remain untrusted until a separate reviewed deck-profile ratification exists.

The offline CLI accepts one retained DXGI artifact and one bounded local profile JSON:

```powershell
cargo run --bin classify_mtgo_offline_bottom_six_visible_card_identities_v1 -- `
  C:\absolute\artifact-directory `
  C:\absolute\visible-card-profile.json
```

It performs no network access and emits names, ordinals, distances, margins, and commitments only. The CLI output is checked-untrusted calibration data and all runtime authority remains false.

`fixtures/offline_visible_card_identity_reflow_corpus_20260810_v1.json` records the CLI result across every retained bottom-six state in the same Solitaire game. All 28 visible card positions matched the expected order as the leftmost card was removed, with maximum mean absolute BGR difference 19.666 and minimum distinct-name margin 38.386. The seven templates were derived from the first frame and cover only that observed hand from a visible 280-card deck. This is wiring and threshold-separation evidence, not an accuracy estimate, a complete deck profile, or authority for semantic evidence, scoring, or input.

`classify_untrusted_offline_bottom_six_visible_card_identities_v2` uses the corrected binary-ink bottom-six state gate and evaluates every visible card slot before deciding whether the complete hand matches. Partial coverage exposes only a count and clears all identity labels. `fixtures/offline_visible_card_identity_coverage_corpus_20260810_v2.json` records two later seven-card hands from games that did not supply the templates. The deliberately incomplete seven-template profile covered 2 of 7 and 4 of 7 visible slots, so neither hand matched and no labels were exposed. This isolates complete deck-template coverage as the current identity bottleneck.

The fixed no-cost deck `mtgo-solitaire-fixed-basics-v1` narrows that universe to 30 copies each of DSK Plains collector 277 and DSK Island collector 279, both already present in the reviewed seven-template profile. `classify_untrusted_offline_bottom_six_visible_card_identities_v3` uses the v3 state gate. It matched all 14 cards across two independent initial bottom-six hands, including a later heldout game, and directly preserved every visible identity through five leftmost-card reflows in one game. MTGO dims the final remaining card when Done appears, so direct art matching intentionally returns `NoMatch` at that stage. The prior order-preserving reflow history still identifies the survivor, but the direct classifier does not claim that inference. The complete record is `fixtures/offline_fixed_basic_deck_identity_corpus_20260810_v1.json`.

`classify_untrusted_offline_mulligan_visible_card_identities_v1` composes the current v2 mulligan-ladder prompt gate with the same fixed-deck template profile. It evaluates the seven visible card-art regions at every prospective keep size and withholds all labels unless the prompt has exactly one match and every card identity passes. The retained corpus `fixtures/offline_mulligan_visible_card_identity_corpus_20260810_v1.json` covers all seven prospective keep sizes in two fixed-deck Solitaire games. All 14 prompts and all 98 visible card positions matched. This is narrow one-layout wiring evidence for two known prints, not an accuracy estimate or competitive-play evidence.

`fixtures/offline_kernel_basics_opening_hand_coverage_20260810_v1.json` records the first opening hand from the no-cost 30 Forest plus 30 Island deck `mtgo-solitaire-kernel-basics-v1`. Four exact DSK print templates identify all seven visible cards, and every exact visible name resolves to a fully supported kernel deck-card definition. Three templates were calibrated on this same frame, so the record proves complete wiring coverage only. It is not a heldout identity result and creates no object binding, observation, model decision, or input authority.

`start_checked_untrusted_first_main_hand_object_ledger_v1` is the narrow bridge from a complete eight-card first-main kernel-coverage result into the existing incarnation ledger. It requires exact deck-card correspondence at every ordinal and binds the source coverage commitment into the ledger commitment. Synthetic identifiers are deterministic hand-slot labels, and the ledger remains checked-untrusted with no public object-binding, observation, scoring, or input surface.

`derive_checked_untrusted_first_main_legal_actions_v1` closes the next supported-deck bookkeeping gap. It accepts only the unchanged, source-bound local P0 ledger for eight exact Forest or Island hand objects. It produces the kernel surface order internally: eight distinct `PlayLand` semantics in arena order followed by `Pass`. The opaque result exposes counts and commitments only. It does not expose the stable references or actions, and it remains unsafe for `ObservationV5`, policy scoring, or input because Solitaire still cannot supply a complete two-player observation.

`refine_checked_untrusted_first_main_reconstruction_v1` binds that complete action result and the eight-object ledger back to the exact six-blocker Solitaire reconstruction audit for the same manifest and frame. It closes only `object_incarnations_and_card_db` and `complete_ordered_legal_actions`. The remaining blockers are distinct duel participants, second-player public state, distinct opponent zones, and kernel decision-history context. The refined wrapper still cannot create an observation, a model request, or input authority.

`derive_checked_untrusted_first_main_kernel_context_v1` closes the kernel decision-history group only for that same exact reset-state first-main slice. Its engine, harness-surface, and policy-surface reset templates are pinned by a test against the kernel's own first `Main1` decision. It exposes only context hashes and leaves the three distinct-opponent observation groups blocked, so model readiness and input safety remain false.

```powershell
cargo run --bin classify_mtgo_offline_mulligan_visible_card_identities_v1 -- `
  C:\absolute\artifact-directory `
  C:\absolute\visible-card-profile.json
```

```powershell
cargo run --bin classify_mtgo_offline_bottom_six_visible_card_identities_v3 -- `
  C:\absolute\artifact-directory `
  C:\absolute\visible-card-profile.json
```

The predecessor mulligan-ladder corpus contains 25 prompt frames plus one gameplay negative across five games. The v2 dark-core development and heldout records add fourteen fixed-deck prompts across two later games. All seven states matched in the later heldout game. This is still a small, one-layout renderer screen rather than an accuracy estimate.

```powershell
cargo run --bin classify_mtgo_offline_bottom_six_reflow_candidate_v2 -- `
  'C:\absolute\before-artifact-directory' `
  'C:\absolute\after-artifact-directory'
```

All six positive transitions come from one game and remove the leftmost visible card. The one non-consecutive negative is rejected. This is threshold-separation and wiring evidence, not an accuracy estimate or evidence for another removed ordinal, visually indistinguishable duplicate cards, another deck, or another layout. A Match identifies the removed visible ordinal only. It does not identify the card name.

The record and checked wrapper retain no pixels or coordinates. Manual card labels are not proven by the structural checker, and all live-frame, semantic-evidence, `ObservationV5`, policy-scoring, and input flags remain false. The kernel runtime `ActionSemanticV1` has no Keep, Mulligan, bottom-selection, or bottom-submit variants, so this pregame path currently needs a separate model-supported pregame policy seam before autonomous play is possible.

`validate_untrusted_offline_london_pregame_episode_v1` joins the maximum-depth path into one exact offline episode: seven choice prompts from prospective keep seven through one, Keep at one, seven bottoming states from zero through six selected cards, Submit, and the visible first-main completion. It binds fifteen unique checked artifacts, seven exact choice-classifier commitments, the exact bottoming-trace commitment, one shared layout, strict capture order, and fourteen declared transition actions. Every declared action must be legal in its exact source state and must produce the canonical next stage. The resulting per-stage action counts are `[2,2,2,2,2,2,2,7,7,6,5,4,3,2,0]`.

The episode fixture is `fixtures/offline_london_pregame_episode_20260810_v1.json`, SHA-256 `bb30440d8be3d0133dd43551dd899119389bc619dd4d58d78c3b6dd7d57b9105`. Its exact fifteen-artifact commitment is `03ae235024f2f1482cbf85ffca08c857a57f70bed1ce56c630a9c784b82e68ed`. Swapping the first two source artifacts is rejected. Action and stage labels remain supervised declarations rather than facts proven from pixels, and the checked episode grants no live-frame, observation, scoring, or input authority.

The live Keep pair can be rechecked without persisting any additional pixels:

```powershell
cargo run --bin check_mtgo_dxgi_keep_transition_v1 -- `
  'C:\absolute\dxgi_keep_transition_v1.json' `
  'C:\absolute\before-artifact-directory' `
  'C:\absolute\after-artifact-directory'
```

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

All live-input flags default to false. On 2026-08-10, the account owner reported that Daybreak expressly extended the same visible-only approval to League and Challenge play on the approved main account. That permits those two modes to be wired, but a runtime scope must still bind the exact account and exact permission-message bytes, and each actual entry remains separately gated below. This repository does not contain those private message bytes and does not itself authorize an entry. Other prize-event input remains false unless separately confirmed. The no-hidden-information condition is never configurable away.

`check_untrusted_authorization_correspondence_v1` is the offline import boundary for that reply. The project owner reports that the written permission now covers both League and Challenge on the approved main account under the same visible-only, no-hidden-information, no-reverse-engineering, and no-cheating limits. The checker recomputes the SHA-256 of up to 4 MiB of exact exported correspondence bytes and the exact visible account alias, then binds them to a human review of the approved competitive modes and the retained restrictions plus attended-entry and attended-spending conditions. The approved mode list must be canonical and can derive only one-mode League or Challenge scopes. The wrapper is intentionally checked-untrusted because code cannot prove that a human paraphrase accurately reflects private message text. It retains no correspondence bytes, cannot ratify input, enter an event, or spend resources, and its derived scope remains ordinary coordinate-free data. The exact new reply has not yet been imported, and the Windows actuator's separate compile-pinned review commitments remain empty.

`validate_visible_competitive_event_listing_selection_v1` closes the coordinate-free semantic gap immediately before Entry Review. It requires one complete Event Browser lifecycle frame, one exact League or Challenge target, the approved account, the selected event identity and visible label, the exact deck list and deck manifest, the format, the policy deployment, and distinct in-bounds visible label and enabled-control regions. The one-mode authorization must match exactly. A successful offline intent can be confirmed only by a strictly newer Entry Review frame that visibly names the same event. The checked selection, intent, and arrival expose no coordinates or input primitive and cannot confirm entry or spend resources. No live parser, accuracy corpus, Windows pixel rehash, immediate recapture, actuation, or postcondition runtime exists for this pre-entry selection yet.

`validate_competitive_deck_manifest_v1`, `validate_visible_competitive_sideboard_snapshot_v1`, and `validate_competitive_sideboard_selection_v1` define the coordinate-free sideboarding contract for either approved competitive mode. The manifest binds the exact starting mainboard, starting sideboard, deck-list commitment, format commitment, and policy deployment. A complete visible between-game snapshot must match that inventory, the exact lifecycle frame, two nonoverlapping partition zones, and an empty visible drop target in each zone. Every visible card row must lie in its declared partition. A model selection is accepted only when its target mainboard and sideboard conserve every card count, keep at least the starting mainboard size, respect the starting sideboard capacity, and remain bound to the same event, match, game, deck, and policy. The deterministic public transfer plan contains visible card names, counts, and directions only and is ordered by visible name, not by a local database identifier. After a strictly newer visible snapshot exactly matches the selected target, `confirm_competitive_sideboard_target_visible_v1` produces a coordinate-free ready state. None of these values can expose coordinates, submit the sideboard, authorize input, or claim format legality. The sibling Windows crate rejects Submit Sideboard through its generic lifecycle route, including an explicitly visible unchanged state. Changed submission requires the proof-specific exact target-ready path, and unchanged submission requires a future opaque native model decision binder. A separate commitment-only corpus and evaluator requires all four canonical sideboard slices: League unchanged, League changed, Challenge unchanged, and Challenge changed. It measures overall and worst-slice prediction coverage plus exact full-snapshot accuracy, rejects source, profile, account, deck, format, policy, and annotation substitutions, and produces only a reviewed ratification candidate. Its production evaluation root remains empty. The sibling Windows crate requires an admitted exact evaluation before it can compute the separately ratified input candidate for immediate recapture, exactly one visible card drag, a strictly newer exact inventory confirmation, and a proof-specific changed-sideboard Submit Deck route. Both production roots are empty, so that path is dormant.

## Intended pipeline

1. Import the exact private correspondence bytes, review their conditions against the approved account, and derive one local League or Challenge scope. Separately compile-ratify that exact review before any input path can use it.
2. Acquire only the information available to the seated player. Visible pixels, the rendered Game Log's persisted representation, or another direct client source are eligible only after a narrow adapter discards every field that is not shown to that player. The sealed adapter may transiently inspect process or backing-object state, but raw objects, hidden cards, RNG state, private protocol-only fields, and non-rendered identifiers must not reach model, operator, diagnostics, logs, or training.
3. Parse visible zones, cards, counters, prompts, phase, priority, game log, and timers. Accessibility or direct client data may assist only when the exported fact is player-visible; private identifiers may exist transiently inside the adapter solely to join a visible fact to its current control and must never cross a public, model, operator, diagnostic, or training boundary.
4. Reconcile the visible state into the player-visible decision contract and complete ordered visible legal choices. Legacy `ObservationV5` reconstruction remains an offline compatibility and audit tool, not the competitive model boundary. Synthetic object incarnations remain adapter-private, and a zone change creates a new incarnation.

`build_checked_untrusted_acting_player_duel_calibration_corpus_v1` creates the first path-free corpus index for later duel-perception review. It accepts only fully checked acting-player duel DXGI artifacts with one privately checked executable, signer, DPI, client size, output identity, and visible format. It sorts by commitments to the visible pixels, rejects duplicate visible frames, and emits only those visible-frame commitments, opaque source-manifest commitments required by the existing evaluation join, corpus-local sequence numbers, and the visible game format. Source directories, pixels, capture times, window titles, player names, match identifiers, process identifiers, process paths, executable identity, signer identity, monitor identity, DPI, and geometry are absent. The index is explicitly bound to `player_visible_ui_facts_only_v1` and grants no OCR, semantic-evidence, scoring, or input authority. The matching CLI prints the deterministic manifest and commitments without reading or controlling the live MTGO client: `cargo run --bin build_mtgo_acting_player_duel_calibration_corpus_v1 -- <corpus-id> <absolute-artifact-directory> [absolute-artifact-directory ...]`.

`check_untrusted_player_visible_duel_annotation_set_v1` binds one complete human annotation set to that exact corpus. Each entry must match the corpus-local case ID and all three immutable source commitments, and must affirm review of the exact source frame, current visible state, and complete visible legal-action set. The annotation protocol is fixed by code and is narrower than the general authorization: every label must be derivable from the exact bound source-frame pixels alone. A persisted rendered Game Log may be parsed through its separate checked source path, but cannot supply an uncommitted fact to this frame-only evaluation. The expected payload type cannot represent `ObservationV5`, kernel or MTGO identifiers, card-database identity, capture lineage, hidden opponent zones, paths, player aliases, or raw transport metadata. It also rejects inconsistent visible counts, absent or noncanonical object references, duplicate choices, malformed card names, and invalid relation, combat, target, or action shapes. The v2 perception evaluator consumes this checked annotation set directly and includes its commitment in the evaluation commitment, so a label change necessarily changes the evaluation commitment. Both values remain checked-untrusted, offline-only, and grant no live evidence, scoring, event, spending, or input authority.

The local validator accepts the annotation JSON and the exact source artifact directories, then prints only commitments and counts:

```text
cargo run --bin check_mtgo_player_visible_duel_annotations_v1 -- <corpus-id> <absolute-annotation-set.json> <absolute-artifact-directory> [absolute-artifact-directory ...]
```

`check_untrusted_player_visible_direct_duel_projection_set_v1` defines an offline, transport-neutral measurement seam for a future direct client source. The raw client representation is producer-private. The first exported value must already be reduced to `MtgoPlayerVisibleDuelDecisionInputV1`, with hidden zones, future draws, RNG, private opponent state, process and protocol metadata, paths, card-database IDs, and internal object IDs discarded. Each prediction or explicit abstention is bound to one exact reviewed UI frame. `evaluate_untrusted_player_visible_direct_duel_projection_v1` then measures exact visible-state and ordered-visible-action agreement against the checked human annotations. The producer digest, discard assertions, and source-frame association remain caller declarations, so a passing report measures only the submitted corpus output. It does not attest producer execution or raw-input handling and grants no live evidence, scoring, or input authority.

`LIVE_DIRECT_SOURCE_PRODUCER_V1.md` specifies the later production trust boundary. The reported permission is transport-neutral: an audited broker and sealed producer may transiently inspect direct client objects or other process-resident state, but their first exported success value must already be the player-visible decision schema. Raw objects, hidden or unrendered facts, private transport details, and internal identifiers never reach the model, operator, diagnostics, logs, or training. The producer otherwise abstains, and a second unchanged admitted frame closes the bracket.

`bind_refreshed_direct_visible_selection_to_competitive_match_v1` binds one exact post-score direct refresh to the existing competitive scope without converting it into input. The bracket must retain the acting-player role, exact client and producer identities, two ordered composed frames, an unchanged commitment for all decision-relevant visible regions, and the newest exact lifecycle frame. The lifecycle and authorization must name the same approved account, permission, League or Challenge event, match, game, and deployment as the model selection. Its move-only result exposes only sanitized player-visible state and action plus commitments. It contains no MTGO object, identifier, process handle, dispatch command, event-entry, spending, or live-input authority. The Windows operator still must own one sealed client call and a newer visible postcondition before another decision can be scored.

`VISIBLE_DUEL_VIEWMODEL_AUDIT_2026-08-12.md` records a metadata-only audit of the installed MTGO 3.4.158.4691 WPF presentation layer. It identifies a plausible direct player-visible projection surface and the co-located backing-model properties that make unrestricted reflection ineligible. The proposed route is an exact compile-time property allowlist whose first exported value is the visible duel decision schema, with composed pixels and the narrow on-screen UI Automation probe retained as qualification and drift checks. The audit accessed no live objects or game state and grants no runtime authority.

`mtgo_visible_duel_viewmodel_candidate_surface_v1` freezes that first metadata candidate list in code. It binds the signed `MTGO.exe`, ClickOnce manifests, `DuelScene.dll`, and `Card.dll` for client 3.4.158.4691; lists each candidate getter with its player-visible context and intended projection fields; and lists backing game, player, zone, card, target, action, identifier, timestamp, account, and card-database properties that are forbidden. Every candidate remains subject to exact reviewed-frame qualification. The checked wrapper has no live-value accessor and every authority flag is false. Print the deterministic review artifact without touching the running client with:

```text
cargo run --bin print_mtgo_visible_duel_viewmodel_candidate_surface_v1
```

The corresponding broker protocol accepts only immutable producer, client, and visible-frame commitments. Its response can contain only a strictly validated `MtgoPlayerVisibleDuelDecisionInputV1` or a fixed abstention reason. The before and after frames must retain the same commitment to all qualified visible projection regions. The schema cannot represent raw view-model objects, internal IDs, paths, or free-form diagnostics, and its checked wrapper remains non-authorizing until a separate audited live producer exists.

The sibling `integrations/mtgo_visible_duel_producer_v1` assembly supplies the in-process producer seam. Version 1.21 locates exactly one public WPF `DuelScene`, requires its `DataContext` to be the exact duel view-model type, and verifies the complete 89-getter public allowlist, thirty-four private visible-source-bound getter joins, and two private rendered-combat join fields against the pinned loaded presentation assemblies. Its sanitized success slice covers bounded noncombat priority decisions during upkeep, draw, Main 1, Main 2, and the end step. The stack may be empty or contain only face-up, non-copy spells controlled by the seated player with an empty rendered association collection. It also exports separately typed attacker, single-attacker blocker, multi-attacker blocker, and blocker-target selection moments. Multiple-attacker target choices come only from cards visibly marked targeting or targetable by MTGO while its target prompt is rendered. The producer verifies the backing target-set shape but never reads or exports its candidate list. Rendered blocker assignments are joined transaction-locally to visible cards, and those backing references are discarded before serialization. It exports no action object or target identifier and has no blocker dispatcher. Private backing values are used only as transient one-way guards and joins and are never exported. It never enumerates either library, an opponent's hidden hand or action collection, or a closed revealed zone, and never reads a face-down card name. The synthetic WPF fixture validates ordinary priority, the narrow stack, attacker observation and sealed execution, single and multi-attacker blocker observation with zero execution authority, a rendered existing assignment, the visible target prompt, and populated public zones. The emitted results pass the strict Rust validator. Version 1.17 remains loaded in the current MTGO broker process. Version 1.21 has not been loaded into MTGO, and the live Pass, stack, attacker, and blocker mappings remain unconfirmed. Complete live semantic evidence, model scoring, and general input authorization therefore remain absent.

`VISIBLE_DUEL_LATER_STATE_METADATA_AUDIT_2026-08-13.md` records the installed-client proof that each public `PlayerViewModel.BattlefieldCards` collection is filtered by MTGO to the cards rendered for that player, including attachment-root controller and Battle protector behavior. It also records presentation candidates for combat, stack, dungeon progress, Monarch, and Initiative. The audit reads no live values and keeps Initiative and all later-state output closed until a reviewed visible corpus proves the exact mappings.

`VISIBLE_DUEL_STACK_COMBAT_DIRECT_SOURCE_DESIGN_2026-08-13.md` turns the approved transport-neutral information boundary into the next implementation contract. It records the installed-client proof for rendered stack order, stack kind and controller presentation, hover-visible target associations, visual attacker and blocker state, and attack-selection menu construction. It permits backing identities only as transaction-local reference-equality joins between already visible objects. Raw client objects and IDs can never leave the producer. Targeted stack items remain closed. Attacker execution is synthetic-qualified. Single-attacker blocker observation and scoring are synthetic-qualified, but blocker input remains closed pending a visible postcondition and sealed dispatcher.

`begin_player_visible_attacker_deliberation_v1` implements the pure bridge
between MTGO's visible attacker toggles plus `Done` and the checkpoint's
sequential include-or-exclude surface. It validates seated-player battlefield
order, the current visible attack lane, exactly one opposite-state toggle per
candidate, an uncomplicated declare-attackers state, and a unique visible
enabled completion control. Each model subdecision exposes exactly `false`
then `true` and only local context derived from visible order and earlier model
choices. No MTGO input occurs during the scan. Completion returns a
coordinate-free desired set and only the toggles that differ from the captured
visible selection; it remains unsafe for input, event entry, and spending.

`prepare_player_visible_attacker_execution_plan_v1` binds that completed model
plan to the exact sanitized producer bytes, candidate count, and a canonical
64-bit desired-selection mask. `reobserve_player_visible_attacker_execution_step_v1`
accepts only another complete sanitized attacker result with the same public
state and candidate identities in the same visible order, then identifies the
first remaining toggle or `Done`. The version 1.21 producer is the sealed
transaction owner: it reobserves on every call, submits at most one action,
requires the next visible state to equal the prior intended toggle, and permits
`Done` only when the committed set is visibly complete. Plan and source hashes
are single-use per game. The Rust wrappers expose no command or input method
and all live-authority flags remain false. Live deployment and attended
qualification are still absent.

The isolated `integrations/mtgo_visible_duel_broker_v1` prototype supplies a bounded local-memory output channel plus native load and CLR-invocation path. Its end-to-end qualification targets only a disposable synthetic .NET Framework process; that build explicitly rejects MTGO before process mutation. This proves the loader mechanism without creating a live broker identity or runtime attestation. Exact signed-client and deployment pins, before and after visible-frame bracketing, and live abstention qualification remain required before a separate MTGO-targeting broker can exist.

The local evaluator hashes its own binary and prints only commitments, counts, action families, and fixed false authority flags:

```text
cargo run --bin evaluate_mtgo_visible_direct_duel_projection_v1 -- <corpus-id> <absolute-annotation-set.json> <absolute-projection-set.json> <absolute-artifact-directory> [absolute-artifact-directory ...]
```

`evaluate_untrusted_player_visible_duel_perception_profile_v2` is the competitive evaluation path over that checked corpus. Its annotation is exactly `MtgoPlayerVisibleDuelDecisionInputV1`, whose type cannot represent a full kernel observation, kernel or MTGO object identity, card-database identity, capture lineage, or a hidden opponent zone. The spec must bind the canonical corpus-manifest hash, the profile format must equal the visible corpus format, and the case list must contain exactly one case per corpus sample in canonical order. Each case ID, source manifest commitment, and visible BGRA commitment must match the corresponding corpus sample before current visible state and ordered visible legal actions are compared. The older `evaluate_untrusted_duel_perception_profile_v1` remains available only for offline full-reconstruction diagnostics and its production ratification root is permanently empty. The v2 production root is also empty until a real player-visible corpus is labeled, evaluated, and separately reviewed.
5. Validate per-leaf provenance, readiness, actor agreement, action bindings, and confidence through this crate.
6. Score the validated observation and ordered legal actions through the exact native checkpoint external-observation scorer seam.
7. Resolve the selected semantic action against current visible evidence. Do not use fixed screen coordinates.
8. Reconfirm focus, prompt, and timer, perform exactly one authorized input, then require a visible postcondition before another input.
9. Stop on layout drift, an unknown prompt, low confidence, timer danger, focus loss, action-set disagreement, or missing postcondition.

## External model scoring envelope

The declare-attackers slice uses a separate player-visible-only scoring
boundary because MTGO presents independent attacker toggles while the trained
policy expects a sequential false-or-true inclusion scan.
`MtgoPlayerVisibleAttackerScorerV1` receives only the complete visible duel
state, the visible battlefield-order candidates, the current candidate, the
earlier choices made by the model in this same local scan, and the exact ordered
`false` then `true` visible action pair. It cannot receive `ObservationV5`,
`ActionSemanticV1`, MTGO client objects, client identifiers, process data, or
simulator pending-combat state. The adapter validates two finite logits and a
finite value, applies lower-index tie breaking, commits the exact input,
deployment, response, and selected visible action, then consumes the move-only
scan into its next candidate or final attacker plan. The boundary has no input,
event-entry, or spending conversion. A real checkpoint implementation remains
blocked on the kernel-owned player-visible scorer described in
`PLAYER_VISIBLE_NATIVE_SCORER_V1.md`.

The single-attacker declare-blockers slice has the matching pure scoring
boundary. `MtgoPlayerVisibleSingleAttackerBlockerScorerV1` receives one visible
attacker, visible battlefield-order blocker candidates, earlier model choices,
and exact `false` then `true` actions. It rejects multiple attackers,
preexisting assignments, incomplete `Block` actions, and nonfinite or wrongly
sized model output. It contains no MTGO target identifier and cannot produce
input. A sealed blocker dispatcher remains deliberately absent.

The multi-attacker declare-blockers slice uses a two-stage visible-only scoring
boundary. First, the model chooses `Done` or one unassigned blocker from visible
battlefield order. Once MTGO renders its target prompt, the model chooses only
among opponent attackers visibly marked targetable. Existing assignments come
from rendered blocker order. The scorer cannot receive the backing target set,
client objects, client identifiers, coordinates, or input authority. Both
stages reject incomplete, reordered, duplicated, or nonfinite inputs, and lower
visible order wins exact score ties. A kernel-owned scorer implementation and a
sealed blocker dispatcher remain separate work.

The adapter now defines the coordinate-free half of step 6. `MtgoExternalScoringRequestV1` binds one validated decision commitment, the exact `ObservationV5`, the complete ordered `ActionSemanticV1` vector, action count, and an expected checkpoint deployment commitment. The deployment identity includes the run, checkpoint manifest, checkpoint payload, train-state, model-parameter, generation, and scorer-contract identities exposed by the native checkpoint handle.

`MtgoExternalModelScoreResponseV1` returns exact f32 policy-logit and value bits bound to that request. Validation requires one finite logit per legal action and a finite value, then uses the kernel scorer's deterministic `total_cmp` argmax with lower-index ties. The resulting opaque selection can create only an offline intent for the exact source decision. It has no coordinates or live-input authority.

`MtgoNativeCheckpointObservationScorerV1` is the concrete implementation. Construction requires an independently supplied expected deployment whose run, checkpoint manifest, checkpoint payload, train state, model parameters, generation, and scorer-contract digest all match the immutable loaded handle. It calls the kernel's external public-observation seam, returns exact score bits, and rebinds them to the adapter request. It cannot derive or self-approve an expected deployment.

`load_mtgo_native_checkpoint_deployment_v1` is the deployment loader for that scorer. It reads the selected native Store's `run.json`, validates the complete Store through its latest pointer, rewalks the chain through the requested generation, constructs the unchanged native inference handle, and rechecks every checkpoint and scorer identity against an independently supplied strict deployment manifest. The returned value is move-only and exposes only the scorer plus immutable identity commitments. It has no capture, account, event-entry, match, or live-input authority.

`LoadedMtgoNativeCheckpointDeploymentV1::score_validated_decision_v1` is the composed runtime entry point. It accepts only an existing `ValidatedMtgoObservedDecisionV1`, scores its exact observation and ordered legal actions through the bound checkpoint, and applies the deterministic adapter selection rule. It returns the existing opaque checked-untrusted selection, which can create only a coordinate-free offline intent.

The checked-in `fixtures/provisional_promoted2_mtgo_deployment_20260810_v1.json` pins promoted(2), seed 920012, generation 384 as a provisional wiring checkpoint. This is an exact deployable package identity, not a claim that it is the final or strongest policy. Validate that package from this directory with:

```powershell
$env:CARGO_TARGET_DIR = 'D:\mtgo-model-deployment-target'
cargo run --bin check_mtgo_model_deployment_v1 -- `
  'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary' `
  'fixtures\provisional_promoted2_mtgo_deployment_20260810_v1.json'
```

The command succeeds only after the complete Store walk and emits the exact deployment commitment with `safe_for_live_input` and `permits_match_entry` both false.

An opt-in real-Store regression also validates one deterministic mock-evidence decision, sends its external public observation with ordered Pass and PlayLand actions through the selected checkpoint, selects PlayLand, and creates the exact coordinate-free offline intent. It pins both action logits and the value by exact f32 bits while requiring all input and match authority flags to remain false:

```powershell
$env:CARGO_TARGET_DIR = 'D:\mtgo-model-deployment-target'
$env:MTGO_NATIVE_STORE_ROOT_V1 = 'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary'
cargo test --lib `
  model_deployment::tests::real_provisional_checkpoint_scores_external_public_decision -- `
  --ignored --exact
```

This is a kernel-generated external-observation wiring probe with explicitly mock frame evidence. It proves the composed validated-decision-to-offline-intent path; it is not evidence that any MTGO pixels have yet been reconstructed into that observation.

For one-shot artifact wiring, `score_mtgo_observed_decision_v1` accepts the native Store root, strict deployment manifest, and a strict declared-offline mock envelope. The envelope schema is `mtgo-offline-mock-observed-decision-input/v1`, its `source_kind` must be `synthetic_mock_v1`, and its `decision` is the strict serialized `MtgoObservedDecisionV1`. A raw decision or any declared DXGI source label rejects before Store loading. The executable then emits only identity commitments, exact score bits, the selected semantic, and the coordinate-free offline intent metadata:

```powershell
cargo run --bin score_mtgo_observed_decision_v1 -- `
  'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary' `
  'fixtures\provisional_promoted2_mtgo_deployment_20260810_v1.json' `
  '<offline-mock-decision-envelope.json>'
```

This envelope prevents accidental direct routing of the source-bound DXGI type into the offline CLI, but a caller declaration cannot prove that arbitrary JSON originated from a synthetic fixture. It is not a live-source admission boundary. A future live scorer must consume a separately admitted source-bound decision, never reinterpret `MtgoMockFrameV1` as a checked DXGI source.

This command has no input backend and always reports `safe_for_live_input = false` and `permits_match_entry = false`. A long-running deployment should load the Store once and call the in-process method for each subsequent validated decision.

## Current-frame semantic control resolution

The adapter-side half of step 7 is also defined. `MtgoVisibleActionControlSetV1` binds a complete, prompt-reconciled set of enabled controls to the exact decision commitment and newest frame. Every candidate carries an exact legal `ActionSemanticV1`, at least 95 percent confidence, and a distinct current-frame pixel-evidence region. Resolution succeeds only when exactly one visible control matches the model-selected semantic.

The resolved control is opaque and retains its rectangle only inside the crate. It exposes no coordinate accessor and remains explicitly unsafe for live input. This prevents callers from turning a stale or ambiguous semantic label into a click. A later trusted actuator must consume the opaque result and independently recheck the live frame, window, prompt, timer, and physical point immediately before one input.

## Competitive lifecycle envelope

`MtgoVisibleCompetitiveLifecycleSnapshotV1` defines the visible states surrounding gameplay: event browser, entry review, entered queue, pairing, match, sideboarding, match result, event result, and reconnect. Every state requires phase-specific current-frame facts, complete-state declaration, exact event and match identities where applicable, at least 95 percent confidence, and a fresh committed frame. The checked wrapper remains structurally untrusted. It exposes visible-region commitments only so an opaque frame owner can rehash them, and it exposes no input method.

User-driven transitions and server-driven transitions are separate. User actions include opening or cancelling entry review, confirming an entry, accepting a pairing, submitting a sideboard, continuing after a match, resuming after reconnect, and closing a completed event. Server events include pairing arrival, game end, match end, event end, and connection interruption. Every transition requires a changed strictly newer frame and preserves event or match identity where required.

League and Challenge mode authorization remain independent. Entry confirmation additionally requires a separate exact `MtgoCompetitiveEntryAuthorizationV1` bound to the same account and written permission, the visible event identity, the exact visible entry terms, and one existing-resource amount. The contract contains no purchase action or resource-acquisition path. All lifecycle intents are offline and coordinate-free, and even a checked transition reports `safe_for_live_input = false`.

`MtgoCompetitiveEventListingTargetV1` and `MtgoVisibleCompetitiveEventListingSelectionV1` define the exact Event Browser selection before Entry Review. The target binds the approved account, one League or Challenge event, its visible label, the deck list and canonical deck manifest, format, and policy deployment. The selection binds that target to one complete Event Browser lifecycle frame plus separate label and enabled Open Entry Review regions. The reviewed-corpus evaluator requires both mode slices, unique source captures, complete visual and configuration review, overall and per-mode coverage, and 100 percent exact selected-event semantics and localization for every non-abstained prediction. Parser trace IDs and valid confidence values are not scored as semantics. Source, profile, account, event, deck, format, policy, annotation, or pixel-region substitution fails the gate. The evaluator produces only a ratification candidate, its production root is empty, and every result remains false for live classification, Open Entry Review, event entry, spending, and input.

`MtgoVisibleCompetitiveEventRecordV1` adds the event-summary state used outside matches. It is valid only for waiting, pairing-ready, between-match, and event-complete lifecycle frames. League progress carries a visible win-loss-draw record and optional match total. Challenge progress separately carries round progress, match points, and an optional rank plus field size. The validator binds the record to the exact lifecycle commitment, frame, event identity, and approved-account digest, requires internally consistent totals and mode-specific schemas, and requires visible status and progress regions plus standing or result regions when those values are present. `bind_classified_navigation_frame_to_visible_event_record_v1` performs the Windows-side same-frame pixel rehash before returning coordinate-free progress. The result remains checked-untrusted because structural binding cannot prove that a supplied label matches the pixels, and it grants no input, entry, spending, or gameplay authority.

The event-record evaluation contract prevents those caller-supplied labels from being mistaken for measured parser accuracy. A reviewed corpus must cover eight canonical slices: waiting, pairing-ready, between-match, and event-complete for League and for Challenge. Every case binds one unique navigation capture, exact lifecycle and event-record commitments, the approved account, and explicit visual confirmations. Evaluation measures coverage per slice and requires exact accuracy on every non-abstained lifecycle, event identity, mode, status, match record, schedule progress, Challenge points, Challenge standing, and completion field. A wrong field or an uncovered slice fails the declared gate. The result remains checked-untrusted and has no production ratification or runtime authority.

## External Flat V2 core seam

`NativeCheckpointInferenceV1::score_external_observation_v1` accepts one exact full `ObservationV5` and ordered `ActionSemanticV1` vector. The producer validates the kernel and card-database identity, visible projection hash, policy-stage structure, action uniqueness and ranges, and every referenced object against the public observation. It creates only the existing scorer-visible Flat V2 view. It cannot create a `PolicyActionV5`, fast-actor session binding, consume token, coordinate, input, or match-entry authority.

The native inference implementation is unchanged. A parity test proves all eleven Flat V2 table families exactly equal the simulator-owned encoder for the same decision, and a real fixture checkpoint produces bit-identical logits and value. Invalid observation hashes, empty action sets, forged stable references, and action kinds absent from the executable training path fail before inference.

## Isolation and merge

Most adapter work remains isolated under `integrations/mtgo_blackbox_v1/**` and `integrations/mtgo_dxgi_capture_v1/**`. The external scorer tranche also adds one reviewed public-observation sibling seam in `mtg-kernel` and advances the source-bound Flat V2 overlay digest. It does not change tensor shapes, mappings, inference, trainer, Store, rollout, or experiment code. Before landing, rebase this branch onto the then-current `main`, compare changed paths with Fable's landing diff, and merge as focused commits or a pull request.

Focused validation from this directory:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'mtgo-blackbox-v1-target'
cargo test -p mtgo-blackbox-v1 --test contract_v1
```
