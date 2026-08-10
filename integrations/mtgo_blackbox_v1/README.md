# MTGO black-box adapter v1

Current end-to-end competitive wiring status and the shortest critical path are tracked in `COMPETITIVE_WIRING_STATUS_2026-08-09.md`.

The exact two-player observation, visible-history, legal-control, and card-coverage plan is `DUEL-RECONSTRUCTION-V1-DESIGN.md`.

This directory is an isolated, offline-first boundary between player-visible MTGO state and the existing `mtg-kernel` policy types. The Rust contract contains no live capture, UI Automation enumeration, process inspection, network inspection, client-file parsing, synthetic input, or checkpoint scoring. A separate calibration-preview script and sibling DXGI candidate executable perform the narrow live operations documented below.

The read-only installation findings and capture implications are recorded in `CLIENT_INVENTORY_2026-08-08.md`.

The first live Standard spectator observations and exact non-claims are recorded in `LIVE_SPECTATOR_FINDINGS_2026-08-09.md`.

The first acting-player Freeform Solitaire observation is recorded in `LIVE_SOLITAIRE_FINDINGS_2026-08-09.md`.

## Live calibration preview

`scripts/capture_visible_mtgo_preview_v1.ps1` creates only a local, composed-desktop calibration preview. It crops the visible desktop to the MTGO client area. It does not use direct window capture, UI Automation, process memory, network data, client logs, hidden client state, or input.

The script requires one responsive MTGO process. Its default main-client mode requires exactly one visible top-level MTGO window; the gameplay exception is described below. It pins the exact client version, executable SHA-256, Authenticode leaf certificate, title rule, and DPI. It requires the target window to be foreground, visible, uncloaked, unminimized, entirely within one monitor, free of intersecting windows above it, and free of the visible cursor. It captures only after entering Per-Monitor V2 DPI awareness, then repeats identity, geometry, foreground, monitor, occlusion, and signature checks before persisting anything.

Those before-and-after checks cannot prove that a very brief cursor, notification, tooltip, or other occluder did not appear and disappear during the copy. This unresolved race is another reason the preview is not evidence. A production backend needs capture-time frame correlation in addition to the same conservative checks.

Every output is marked `pending_visual_review` and explicitly unsafe for semantic evidence, OCR, policy scoring, and input. The destination must be a new absolute directory outside this repository. The first reviewed image should establish a client-version, DPI, physical-size, and image-anchor calibration profile. A production evidence backend should use DXGI Desktop Duplication and must remain a separate later tranche.

The default `MainClient` mode retains the original single-window requirement. `ForegroundSpectatorGame` mode is only for a manually selected spectated 1-on-1 game. `ForegroundSolitaireGame` is only for a one-player game created through Custom Match. Both gameplay modes require the main client to remain visible, select only the foreground top-level window owned by the same verified MTGO process, pin the expected format, validate the mode-specific title structure, commit the complete visible MTGO top-level window set, and repeat those checks after capture. MTGO sometimes visibly renders numeric match and game IDs in its title bar without including them in `GetWindowText`; that case is recorded as a less specific title identity rather than pretending the IDs were independently verified. Other MTGO panes may remain visible behind the game, but any window intersecting the game above it still rejects the capture.

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

The attended navigation helper `scripts/invoke_supervised_mtgo_click_v1.ps1` emits exactly one left-click after checking the exact process ID and start time, executable and signer identity, DPI, title, client size, foreground ownership, client-to-screen transform, visible hit-test root, and cursor position. It has no keyboard or text-input path. It does not identify the semantic control at the point, does not verify any postcondition, and is explicitly unsafe for autonomous input, purchases, or queue entry. Every use must be bracketed by manually inspected visible previews and must stop on an unexpected state.

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

## Current-frame semantic control resolution

The adapter-side half of step 7 is also defined. `MtgoVisibleActionControlSetV1` binds a complete, prompt-reconciled set of enabled controls to the exact decision commitment and newest frame. Every candidate carries an exact legal `ActionSemanticV1`, at least 95 percent confidence, and a distinct current-frame pixel-evidence region. Resolution succeeds only when exactly one visible control matches the model-selected semantic.

The resolved control is opaque and retains its rectangle only inside the crate. It exposes no coordinate accessor and remains explicitly unsafe for live input. This prevents callers from turning a stale or ambiguous semantic label into a click. A later trusted actuator must consume the opaque result and independently recheck the live frame, window, prompt, timer, and physical point immediately before one input.

## Competitive lifecycle envelope

`MtgoVisibleCompetitiveLifecycleSnapshotV1` defines the coordinate-free visible states surrounding gameplay: event browser, entry review, entered queue, pairing, match, sideboarding, match result, event result, and reconnect. Every state requires phase-specific current-frame facts, complete-state declaration, exact event and match identities where applicable, at least 95 percent confidence, and a fresh committed frame. The checked wrapper remains structurally untrusted and exposes no pixel regions or input method.

User-driven transitions and server-driven transitions are separate. User actions include opening or cancelling entry review, confirming an entry, accepting a pairing, submitting a sideboard, continuing after a match, resuming after reconnect, and closing a completed event. Server events include pairing arrival, game end, match end, event end, and connection interruption. Every transition requires a changed strictly newer frame and preserves event or match identity where required.

League and Challenge mode authorization remain independent. Entry confirmation additionally requires a separate exact `MtgoCompetitiveEntryAuthorizationV1` bound to the same account and written permission, the visible event identity, the exact visible entry terms, and one existing-resource amount. The contract contains no purchase action or resource-acquisition path. All lifecycle intents are offline and coordinate-free, and even a checked transition reports `safe_for_live_input = false`.

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
