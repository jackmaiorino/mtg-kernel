# MTGO live Solitaire findings, 2026-08-09

## Scope

This was a free one-player Freeform game on the authorized account. Jack authorized visible collection inspection, creation of a no-cost deck from already available cards, and launch of Solitaire. No trade, purchase, League, Challenge, queue entry fee, or prize-event action was performed.

The original saved decks were preserved. Two new local deck objects were created:

- `mtgo-solitaire-legacy-burn-v1` imported a published starter list, but MTGO marked cards as missing. It was not used and no missing cards were purchased.
- `mtgo-solitaire-owned-lands-v1` contains 280 basic lands, 56 of each type, selected through MTGO's Add Basic Lands dialog. MTGO marked this Freeform deck playable.

The first collection `Play Now` click hosted a free 1-on-1 Freeform match instead of opening setup. The adapter canceled it immediately while the opponent slot was still visibly open. Solitaire was then created through `Custom Match` with one player, `Only Buddies`, and `No Watchers`.

The client identity was:

- Product and file version: `3.4.158.4689`
- Executable SHA-256: `A672755DAD7FE8CD08C7986216D0D0FB2C4DBAFE669AD3D2AFF2BFA2C21B9C69`
- Authenticode signer thumbprint: `E9D9E2B989F90555B04C506FDDF889C7ABA7AC30`
- DPI: `120`

## Strict opening-hand preview

The strict acting-player preview is outside the repository at `D:\mtgo-solitaire-opening-strict-20260809-1940`.

- Artifact kind: `mtgo_visible_solitaire_gameplay_calibration_preview_v1`
- Capture role: `acting_player_solitaire`
- Frame dimensions: `1550 x 925`
- Frame SHA-256: `67B90282864D2A57710DB6C18D2D607B4A4CC0D1C45F51447285CAC064B5DA05`
- Manifest SHA-256: `EC2E8DC5EAAB2D467BDC6BECFEF65E4E111C509954E702B850D42FA1C0E1C311`
- Visible MTGO top-level windows: `2`, the main client and the foreground Solitaire game
- Occluding windows above the game: `0`
- Cursor inside the game client: `false`
- Title identity returned by Windows: `participants_only`

The manifest remains `pending_visual_review`. `safe_for_ocr`, `safe_for_semantic_evidence`, `safe_for_policy_scoring`, and `safe_for_input` are all `false`.

## Visible layout observations

- The current decision is explicit in the upper-left prompt: keep seven cards or mulligan to six.
- The seven-card hand is fully visible along the bottom and contains only basic lands in this calibration game.
- The local player's life, hand count, library count, and avatar are visible in the left panel.
- The phase bar and turn number are visible immediately above the hand.
- The local battlefield is the large lower center region. The opponent-side region remains present above it even in one-player mode.
- The visible game log is a separate right-side panel.
- Hover shortcuts and auto-yield controls visibly overlap the hand region. A perception system must distinguish persistent cards from transient help overlays before interpreting card text or targets.
- The rendered title bar includes numeric match and game IDs, but Windows returned only `(Solitaire): Freeform: Vs. UnbuckledPie`. The manifest therefore does not claim independent title-text verification of those IDs.

## Pipeline consequence

This capture establishes a distinct acting-player layout and a safe free environment for perception and action calibration. It does not establish OCR accuracy, semantic reconstruction, complete legal-action recovery, model scoring, or input safety.

The next implementation sequence is:

1. Define and manually review stable anchors for the exact client version, DPI, window size, and Solitaire layout.
2. Implement the trusted DXGI Desktop Duplication producer. The current composed-screen copy remains preview-only.
3. Measure prompt, hand, phase, life, library, battlefield, and visible-log perception against manually labeled frames.
4. Reconcile a complete visible `ObservationV5` and ordered `ActionSemanticV1` set for the opening keep-or-mulligan decision.
5. Add the kernel external-observation scorer seam after Fable's current scoring work is reconciled.
6. Resolve one selected semantic action against a fresh trusted frame and require its visible postcondition before another input.

Solitaire is suitable for initial perception, decision, and input calibration. It is not an AI opponent and cannot by itself establish competitive playing strength.
