# MTGO DXGI visible-frame candidate v1

This standalone Windows probe captures the foreground MTGO client using DXGI Desktop Duplication. It does not use direct-window capture, accessibility enumeration, process memory, network inspection, client files other than the signed executable identity, hooks, injection, or input.

The probe is intentionally separate from `mtgo_blackbox_v1`. It contains the Windows `unsafe` boundary needed for Win32, D3D11, DXGI, DWM, process, and Authenticode calls. The offline adapter remains `#![forbid(unsafe_code)]` and cannot call this executable as an input mechanism.

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

Every artifact is marked `checked_untrusted_not_admitted`, and all OCR, semantic-evidence, policy-scoring, and input flags are false. Authenticode and frame checks do not prove that a caller-supplied calibration label matches the pixels. Pre/post z-order audits also cannot eliminate a transient occluder that appears and disappears entirely during one frame acquisition. A later adapter loader must recompute every file hash, bind a manually ratified client profile and anchors, and retain an opaque frame type before pixels may reach measured perception.

The probe has no focus, cursor, mouse, keyboard, purchase, queue-entry, or gameplay action API. An attended operator must place MTGO in the foreground and park the cursor before running it.

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
