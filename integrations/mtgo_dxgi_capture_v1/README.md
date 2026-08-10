# MTGO DXGI visible-frame candidate v1

This standalone Windows package captures the foreground MTGO client using DXGI Desktop Duplication. It does not use direct-window capture, accessibility enumeration, process memory, network inspection, client files other than the signed executable identity, hooks, injection, or keyboard input. Its mouse actuator is isolated in `src/actuator.rs` and is unreachable in the current production build because no exact permission correspondence is ratified.

The probe is intentionally separate from `mtgo_blackbox_v1`. It contains the Windows `unsafe` boundary needed for Win32, D3D11, DXGI, DWM, process, and Authenticode calls. The offline adapter remains `#![forbid(unsafe_code)]` and cannot call this executable as an input mechanism.

The command-line executable is a thin wrapper over the library capture engine. A successful in-process capture returns `OpaqueMtgoDxgiFrameCandidateV3`. Safe Rust callers cannot construct, clone, format, deserialize, or read pixels from that type. Its public view contains commitments and geometry only. Downstream live wiring must accept the opaque object itself, never a copied commitment or serialized manifest, if it needs proof that the real capture routine ran.

`measure_mtgo_dxgi_mulligan_ladder_candidate_v3` is the first direct perception consumer. It consumes the opaque frame in-process, serializes the probe-owned manifest, rechecks the manifest, PNG, and canonical pixels through `mtgo_blackbox_v1`, then runs the exact seven-state London mulligan prompt classifier. The opaque measurement retains the source frame and exposes only the checked-untrusted classification, optional prospective keep size, adapter-local action labels, and commitments. It has no pixel, coordinate, `ObservationV5`, scorer, action-intent, or input conversion.

The pregame scoring contract binds that opaque Match measurement, the exact ordered Mulligan then Keep actions, and the same validated native-checkpoint deployment commitment used by gameplay scoring. An external scorer returns finite `f32` bit patterns bound to the exact request. Validation deterministically selects the greatest logit, with the first action winning ties, and retains the opaque measurement in `OpaqueMtgoPregameModelSelectionV3`. Stale responses, wrong action counts or order, non-finite values, NoMatch, and Ambiguous measurements fail closed. The selection remains untrusted model output and has no live-input conversion.

`build_pregame_action_plan_v3` consumes that opaque selection and binds it to the reviewed 1550 by 925 Solitaire control layout, the exact source capture, and the observed pixels of the selected control region. The target rectangle and point remain private. A planned Mulligan requires the next exact ladder prompt on a strictly newer changed frame with the same process, match-window, and output identity. A seven-card Keep can be confirmed by the exact Turn 1 first-main profile shared by two reviewed games with different visible hand sizes. A one-card Keep can be confirmed by `measure_mtgo_dxgi_bottom_six_initial_candidate_v3`, which requires the exact reviewed bottom-six prompt at selected count zero. Keeps at prospective sizes two through six remain blocked until their required bottom counts are separately calibrated. Zero-card Mulligan is also rejected until its prompt is calibrated.

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

Every artifact is marked `checked_untrusted_not_admitted`, and all OCR, semantic-evidence, policy-scoring, and input flags are false. The opaque v3 candidate and mulligan measurement do not change those flags. They close the specific caller-forged manifest and safety-boolean path, but Authenticode and frame checks do not prove that a calibration label matches the pixels. Pre/post z-order audits also cannot eliminate a transient occluder that appears and disappears entirely during one frame acquisition. The exact prompt templates have only a tiny offline brittleness screen and are not a general accuracy estimate. The dormant actuator requires those exact templates and adds immediate recapture checks, but it does not make perception general or prove that a click caused the later visible state.

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
