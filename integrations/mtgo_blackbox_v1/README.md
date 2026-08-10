# MTGO black-box adapter v1

This directory is an isolated, offline-first boundary between player-visible MTGO state and the existing `mtg-kernel` policy types. The Rust contract contains no live capture, UI Automation enumeration, process inspection, network inspection, client-file parsing, synthetic input, or checkpoint scoring. A separate calibration-preview script performs the narrow live operation documented below.

The read-only installation findings and capture implications are recorded in `CLIENT_INVENTORY_2026-08-08.md`.

The first live Standard spectator observations and exact non-claims are recorded in `LIVE_SPECTATOR_FINDINGS_2026-08-09.md`.

The first acting-player Freeform Solitaire observation is recorded in `LIVE_SOLITAIRE_FINDINGS_2026-08-09.md`.

## Live calibration preview

`scripts/capture_visible_mtgo_preview_v1.ps1` creates only a local, composed-desktop calibration preview. It crops the visible desktop to the MTGO client area. It does not use direct window capture, UI Automation, process memory, network data, client logs, hidden client state, or input.

The script requires one responsive MTGO process. Its default main-client mode requires exactly one visible top-level MTGO window; the gameplay exception is described below. It pins the exact client version, executable SHA-256, Authenticode leaf certificate, title rule, and DPI. It requires the target window to be foreground, visible, uncloaked, unminimized, entirely within one monitor, free of intersecting windows above it, and free of the visible cursor. It captures only after entering Per-Monitor V2 DPI awareness, then repeats identity, geometry, foreground, monitor, occlusion, and signature checks before persisting anything.

Those before-and-after checks cannot prove that a very brief cursor, notification, tooltip, or other occluder did not appear and disappear during the copy. This unresolved race is another reason the preview is not evidence. A production backend needs capture-time frame correlation in addition to the same conservative checks.

Every output is marked `pending_visual_review` and explicitly unsafe for semantic evidence, OCR, policy scoring, and input. The destination must be a new absolute directory outside this repository. The first reviewed image should establish a client-version, DPI, physical-size, and image-anchor calibration profile. A production evidence backend should use DXGI Desktop Duplication and must remain a separate later tranche.

The default `MainClient` mode retains the original single-window requirement. `ForegroundSpectatorGame` mode is only for a manually selected spectated 1-on-1 game. `ForegroundSolitaireGame` is only for a one-player game created through Custom Match. Both gameplay modes require the main client to remain visible, select only the foreground top-level window owned by the same verified MTGO process, pin the expected format, validate the mode-specific title structure, commit the complete visible MTGO top-level window set, and repeat those checks after capture. MTGO sometimes visibly renders numeric match and game IDs in its title bar without including them in `GetWindowText`; that case is recorded as a less specific title identity rather than pretending the IDs were independently verified. Other MTGO panes may remain visible behind the game, but any window intersecting the game above it still rejects the capture.

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

## Supervised pregame transition

`MtgoPregameCalibrationTraceV1` records a digest-only, adapter-local Keep or Mulligan calibration transition. It binds distinct before and after preview manifests and frames, the visible action-control region before the manual action, and multiple changed visible regions after it. Keep requires changed prompt, player-count, game-log, and phase-bar regions. Mulligan requires changed prompt, player-count, and game-log regions.

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

## External model scoring envelope

The adapter now defines the coordinate-free half of step 6. `MtgoExternalScoringRequestV1` binds one validated decision commitment, the exact `ObservationV5`, the complete ordered `ActionSemanticV1` vector, action count, and an expected checkpoint deployment commitment. The deployment identity includes the run, checkpoint manifest, checkpoint payload, train-state, model-parameter, generation, and scorer-contract identities exposed by the native checkpoint handle.

`MtgoExternalModelScoreResponseV1` returns exact f32 policy-logit and value bits bound to that request. Validation requires one finite logit per legal action and a finite value, then uses the kernel scorer's deterministic `total_cmp` argmax with lower-index ties. The resulting opaque selection can create only an offline intent for the exact source decision. It has no coordinates or live-input authority. The remaining core work is an implementation of `MtgoExternalObservationScorerV1` backed by the native checkpoint scorer.

## Deliberate seam after this tranche

The existing checkpoint shadow service owns a simulated `FastActorSessionV1`. It cannot score an arbitrary observation reconstructed from MTGO. Its flat scoring view and inference output accessors are crate-private.

The next kernel-facing change is now narrower: expose a small native checkpoint method that consumes an exact `ObservationV5` plus ordered `ActionSemanticV1` vector and returns finite logits and value. The adapter-side request, deployment, response, deterministic selection, and offline-intent bindings are already implemented. The core change should be made only after Fable's current science branch is reconciled because it touches core scoring code.

## Isolation and merge

This standalone workspace changes only `integrations/mtgo_blackbox_v1/**`. It does not modify the root Cargo workspace, root lockfile, trainer, Store, rollout, scorer, or experiment files. Before landing, rebase this branch onto the then-current `main`, compare changed paths with Fable's landing diff, and merge as a focused commit or pull request.

Focused validation from this directory:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'mtgo-blackbox-v1-target'
cargo test -p mtgo-blackbox-v1 --test contract_v1
```
