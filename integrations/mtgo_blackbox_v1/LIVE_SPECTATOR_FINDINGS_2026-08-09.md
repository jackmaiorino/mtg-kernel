# MTGO live spectator findings, 2026-08-09

## Scope

This was a free, read-only Standard spectator session on the authorized account. Jack completed login. The adapter selected `Constructed > Casual Games > Standard`, used a visible `Watch` control, and sent no deck selection, match action, trade, purchase, League, Challenge, or prize-event input.

The client identity was:

- Product and file version: `3.4.158.4689`
- Executable SHA-256: `A672755DAD7FE8CD08C7986216D0D0FB2C4DBAFE669AD3D2AFF2BFA2C21B9C69`
- Authenticode signer thumbprint: `E9D9E2B989F90555B04C506FDDF889C7ABA7AC30`
- DPI: `120`

## Captures

Two strict spectator previews were produced outside the repository:

| Capture | Frame SHA-256 | Result |
| --- | --- | --- |
| `D:\mtgo-spectator-gameplay-v1-20260809-182403958` | `357C020780A99FC12780C112133BDC25899020D81F9722B3BFF7723D74B135BA` | A triggered-ability stack overlay obscured part of the battlefield. |
| `D:\mtgo-spectator-gameplay-v1-20260809-182553870` | `299D29D92A7186F8608630FBD17B5C11F6775C0CE08A8679E0DC59492F2A8A61` | The overlay had resolved and the visible game advanced to the next turn. |

Both captures used a `1550 x 925` client at desktop origin `(0, 0)`. Both PNG hashes match their manifests. Window title, title-identity classification, window-set commitment, and geometry remained equal while the pixel commitments changed. Each capture recorded zero intersecting windows above the duel and no cursor inside the client.

Every manifest remains `pending_visual_review` with `safe_for_ocr`, `safe_for_semantic_evidence`, `safe_for_policy_scoring`, and `safe_for_input` set to `false`.

## Visible layout observations

- The duel uses stable coarse panels: player summaries on the left, the opponent battlefield above, the local-side battlefield below, a phase bar near the bottom, a game log on the right, and spectator priority/status text between the player summaries.
- Battlefield contents are not a fixed grid. Cards overlap, stack in piles, rotate when tapped, resize, carry counter badges, and may be partly covered by another permanent.
- The visible stack is a modal overlay that can cover a large part of the battlefield. A perception system must classify the active overlay before interpreting the underlying board.
- The game log is valuable chronological evidence, but only its visible scroll window is available. A later observer must track visible additions and deduplicate lines without reading client files or hidden controls.
- Card names and rules text appear at several scales. A robust parser will need region detection plus card-image or text recognition, not full-frame OCR alone.
- The title bar visibly rendered numeric match and game IDs, but `GetWindowText` returned only format and participant names in this session. The manifest therefore records `game_window_title_identity = participants_only`; the rendered IDs are not treated as independently verified metadata.
- Spectator layout is not a valid calibration for an acting player's hand, prompt, priority, legal actions, targets, or input controls. Those require a separate player-role capture in a free, explicitly authorized match.

## Pipeline consequence

The two-frame test establishes that composed visible pixels can be captured repeatedly with stable client identity and geometry while detecting real game-state change. It does not establish semantic correctness. The next implementation sequence is:

1. Jack visually reviews and explicitly approves a calibration image and anchor set.
2. Implement a trusted DXGI Desktop Duplication producer with capture-time frame correlation. The current screen-copy script remains preview-only.
3. Capture a separate acting-player layout in a free private match or open-play game. Do not use Leagues, Challenges, or other prize events without scope-specific written approval.
4. Build measured perception stages for overlay classification, visible game-log deltas, card and counter recognition, phase and priority, zones, and complete legal-action reconstruction.
5. Convert only fully reconciled visible state into the existing validated `ObservationV5` and ordered `ActionSemanticV1` contract.
6. Add the kernel external-observation scorer seam after Fable's scoring work is reconciled.
7. Resolve one selected semantic action against fresh visible evidence, send at most one authorized input, and require its declared visible postcondition before another input.

The spectator captures are suitable for layout research only. They are not evidence that the model can yet observe, score, or play an MTGO game.
