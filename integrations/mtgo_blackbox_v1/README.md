# MTGO black-box adapter v1

This directory is an isolated, offline-first boundary between player-visible MTGO state and the existing `mtg-kernel` policy types. The Rust contract contains no live capture, UI Automation enumeration, process inspection, network inspection, client-file parsing, synthetic input, or checkpoint scoring. A separate calibration-preview script performs the narrow live operation documented below.

The read-only installation findings and capture implications are recorded in `CLIENT_INVENTORY_2026-08-08.md`.

## Live calibration preview

`scripts/capture_visible_mtgo_preview_v1.ps1` creates only a local, composed-desktop calibration preview. It crops the visible desktop to the MTGO client area. It does not use direct window capture, UI Automation, process memory, network data, client logs, hidden client state, or input.

The script requires one responsive MTGO process and exactly one visible top-level window. It pins the exact client version, executable SHA-256, Authenticode leaf certificate, title, and DPI. It requires the window to be foreground, visible, uncloaked, unminimized, entirely within one monitor, free of intersecting windows above it, and free of the visible cursor. It captures only after entering Per-Monitor V2 DPI awareness, then repeats identity, geometry, foreground, monitor, occlusion, and signature checks before persisting anything.

Those before-and-after checks cannot prove that a very brief cursor, notification, tooltip, or other occluder did not appear and disappear during the copy. This unresolved race is another reason the preview is not evidence. A production backend needs capture-time frame correlation in addition to the same conservative checks.

Every output is marked `pending_visual_review` and explicitly unsafe for semantic evidence, OCR, policy scoring, and input. The destination must be a new absolute directory outside this repository. The first reviewed image should establish a client-version, DPI, physical-size, and image-anchor calibration profile. A production evidence backend should use DXGI Desktop Duplication and must remain a separate later tranche.

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
