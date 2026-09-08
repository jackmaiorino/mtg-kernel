# Live DXGI capture findings, 2026-08-10

## Result

The probe successfully captured the running foreground MTGO client through DXGI Desktop Duplication without sending MTGO input. Manual inspection confirmed that the PNG contains the unobscured MTGO client crop, aligned to the visible client edges, with the cursor outside the crop. The visible client was on Constructed Quick Play and showed no deck selected.

## Pinned live identity

- process count: 1;
- executable SHA-256: `a672755dad7fe8cd08c7986216d0d0fb2c4dbafe669ad3d2aff2bfa2c21b9c69`;
- signer thumbprint: `e9d9e2b989f90555b04c506fddf889c7aba7ac30`;
- signer subject SHA-256: `89e095d976048cdd8da11e2ff312231867f79e521fa3b5aa6415d2aa59b79cfc`;
- signer subject: `CN=Daybreak Game Company LLC, O=Daybreak Game Company LLC, L=San Diego, S=California, C=US`;
- DPI: 120;
- output: `\\.\DISPLAY2`, 2560 by 1440, identity rotation, SDR sRGB;
- client crop: 1550 by 925;
- pixel format: tightly packed top-down BGRA8.

The final signer-pinned runtime sample had successful Authenticode checks before and after capture, one real desktop presentation, no protected-content masking, no cursor intersection, no intersecting window above the target, and identical pre/post snapshots. Its artifact remains outside the repository and is not a durable fixture.

## Role-explicit v2 follow-up

The candidate now emits a v2 schema that separates main-client navigation, acting-player Solitaire, and spectator capture roles. Each game role binds an exact visible format and a role-specific title structure. The adapter accepts both the legacy main-client-only v1 artifact and the new v2 role tuples, while preventing a spectator frame from being relabeled as an acting-player frame.

The running main client was deliberately presented to the probe as `solitaire_game` with `Freeform`. The title gate rejected it before frame acquisition and persisted no artifact. A separate main-client v2 attempt reached Desktop Duplication but the static lobby produced no new desktop presentation within the timeout, so it also persisted nothing.

A no-cost one-player Custom Match was then created with the existing 280-basic-land deck, Only Buddies, and No Watchers. The strict v2 probe captured the visible Freeform duel at its opening mulligan decision. The adapter independently decoded and compared the PNG to the canonical BGRA bytes and returned:

- role: `ActingPlayerSolitaire`;
- artifact: `C:\Users\Jack\AppData\Local\Temp\mtgo-dxgi-solitaire-v2-20260810-005340-041`;
- canonical BGRA8 SHA-256: `72005a726e339c1c803ca7c4604d3b182629e65a792de235ae36e10bfa68162d`;
- PNG SHA-256: `16bdb4dd6a0fb2e724adcd7962486fa730ce939da9fd75a9731bc7f703877b34`;
- manifest SHA-256: `af62f6392454c74a81ada7ea9bc0f0111164bd95b84d114f03e7d416fc27aee3`;
- output identity SHA-256: `89c86876d12827c79ef4d746b9cd88c8decaf3e4fae33b41f6ab57c94bf222a6`;
- client crop: 1550 by 925;
- status: `checked_untrusted_not_admitted`;
- all semantic-evidence, OCR, policy-scoring, and input safety flags: false.

Manual inspection confirmed the acting-player layout, opening seven-card hand, mulligan prompt, local totals, phase bar, battlefield, and visible game log. No mulligan, keep, card, phase, purchase, queue, League, or Challenge action was sent for this capture.

## Offline calibration ratification

The exact acting-player artifact above is now source-ratified for one scope only: `ActingPlayerSolitaireOnlyV1` offline calibration. The admission commitment is `9b0aef61a6fc31ee6050d9e381fba4c3e1a6b887c62319c8b3e1e13e9523e291`. The production admission checker revalidated the exact manifest, raw BGRA8, decoded PNG, output identity, dimensions, timestamp, and role before returning an opaque value.

The opaque value retains pixels only for crate-internal offline calibration code. It has no public pixel accessor and remains false for live OCR, semantic evidence, policy scoring, and input. This is not a reusable profile, live-frame attestation, or authorization for any action.

## Strict DXGI Keep transition

A fresh role-correct frame was captured at the same opening mulligan decision, followed by one guarded click on the visibly inspected Keep control and one role-correct post-frame:

| State | Artifact | Canonical BGRA8 SHA-256 | Manifest SHA-256 |
| --- | --- | --- | --- |
| Before Keep | `C:\Users\Jack\AppData\Local\Temp\mtgo-dxgi-before-keep-v2-20260810-010502-316` | `124e37ee6c431a7b40d13a49c5c830a630580741350ce799e482ddc4522ccf31` | `e5f6f170764f6f374587e45a0e5c3c3fbbc98849bd091b305a2e318bbfede49d` |
| After Keep | `C:\Users\Jack\AppData\Local\Temp\mtgo-dxgi-after-keep-v2-20260810-010607-906` | `4b33db6c50945a1a07320709be7cf2ada1ec513b69748e2e386e1f119864f514` | `018d955305e253460f2e0fd0e51d850f1f6e8684cbf2b7415ce5446d2a818adb` |

Both artifacts independently check as `ActingPlayerSolitaire`, share the exact client and output identity, and remain unsafe for semantic evidence, scoring, and input. The after-frame visibly shows the first-main prompt, Combat control, eight cards in hand, first-main phase highlight, and new game-log entries for keeping seven and drawing a card.

The digest-only fixture `fixtures/dxgi_keep_transition_20260810_v1.json` binds both artifacts, the visible Keep control region, and canonical prompt, player-counts, game-log, phase-bar, and hand regions. The byte checker recomputes each artifact, verifies the exact raw bytes, requires a strictly newer after-frame, and requires at least one percent of pixels to change in every named postcondition. The checked transition commitment is `89bfe0ec667457fe6cb59b5737461b0bf167016b61e64f557d6079cd220f766c`.

The region names remain manual labels. The checker proves byte changes at those locations, not semantic recognition or a general Keep detector. It exposes no pixels or coordinates and grants no subsequent input authority.

## Exact offline opening-hand recognition

The separately ratified pre-Keep frame now passes one compiled offline-only pixel-to-semantic calibration profile. The profile checks exact hashes for four reviewed regions: prompt text, Mulligan control, Keep control, and hand count. It returns a seven-card opening-hand decision with ordered adapter-local actions Mulligan to six then Keep.

- profile commitment: `f57076b8e73261a07fc71a80f6b54eaba0bcae725aa1980869a95f6dbc0392e6`;
- recognition commitment: `eecfc224dccf5abb1a8e70926a453ba89086e142d5ae87592a14a82304e56726`;
- source manifest: `af62f6392454c74a81ada7ea9bc0f0111164bd95b84d114f03e7d416fc27aee3`.

The recognized value retains no pixels or coordinates and is false for live-frame, semantic-evidence, `ObservationV5`, policy-scoring, and input use. This is one exact reviewed example, not a reusable recognizer or an accuracy result.

The profile was then applied as a checked-untrusted offline classifier. A second true frame exposed that the original broad prompt region included an animated cyan glow and matched only 3 of 4 regions. The prompt anchor was narrowed to text-only pixels before the final screen.

The final corpus contains five manually inspected frames from two no-cost Freeform Solitaire games: three seven-card opening-hand positives, one first-main post-Keep negative, and one six-card mulligan negative. The classifier produced 3 true positives, 2 true negatives, and no observed errors. All artifacts are independently byte-checked and all classifier authority flags remain false. Five frames from two games establish a useful wiring and brittleness result, not general perception accuracy.

## Nonclaims

This proves that the local backend can acquire and crop a player-visible composed desktop frame while checking the declared live conditions. It does not admit the pixels for OCR, semantic evidence, model scoring, or input. It does not eliminate the transient-occluder race between the two z-order audits. No League, Challenge, purchase, queue, or gameplay action was performed in the captures described above.

## Source-pinned live-frame boundary follow-up

The current source now fixes the inspected executable, exact Daybreak signer, local Freeform Solitaire title, 120 DPI, 1550 by 925 client layout, `DISPLAY2` output identity, SDR color space, and canonical BGRA8 geometry in profile commitment `45f73bf432bbed42e1896c4f02e0670115bb891b69fdc7037c89dc780ac91fac`. Only the in-process DXGI candidate can reach the private binder. The returned wrapper cannot be constructed, cloned, formatted, serialized, or used to read pixels, and all semantic, observation, policy, and input accessors remain false.

The opt-in live test was attempted against the running bottom-six Solitaire prompt. A static screen timed out because Desktop Duplication had no fresh desktop presentation. A retry around one reversible card selection reached an MTGO-owned foreground surface whose title did not match the exact acting-player rule. Both attempts returned no wrapper. The calibrated Cancel click restored all seven visible cards at zero selected, and a separate pending-review preview confirmed that state. These are useful fail-closed runtime checks, not a successful live admission claim.

## Current signed-client offline rehearsal, 2026-08-12

The installed MTGO executable changed to SHA-256
`bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92`
while retaining a valid Daybreak signature with the same pinned signer
thumbprint. A strict DXGI Solitaire capture from that build was manually
inspected and replayed through the offline pregame wiring:

- artifact: `C:\Users\Jack\AppData\Local\Temp\mtgo-solitaire-dxgi-20260811-205435661`;
- canonical BGRA8 SHA-256: `91faa56cd2c334225d91628674f31bb325b522c160a781b6c7ef298311b35446`;
- PNG SHA-256: `e2e7817121666b3afb67a01b75cbde8b4cb4ab3be2133e5d39f6a39e71148edf`;
- manifest SHA-256: `93b159548c532fc64ae4e9402affdd148a87e24b34ad628a741055f6c8d2eda9`;
- source profile commitment: `91acc6100d325a412951222eb5283de256c0a906fb910dbf43b251459c3a8427`;
- visible-card template file SHA-256: `0fcd905fa541ebb97dba5a1f8ae7f2bff8d7c05b3017cac5bda605a49207b3a4`.

The replay recognized the visible ordered hand as Island, Forest, Forest,
Forest, Island, Island, Forest; reconstructed exactly Mulligan to six and
Keep; and produced a Keep plan with the expected first-main postcondition.
The scorer was the deterministic non-model wiring heuristic, not MTG-kernel's
RL policy. The artifact and action plan remained false for live input,
purchase, and queue entry. This is current-build capture, recognition, and
semantic-planning compatibility evidence only. It is not competitive model
scoring, general perception accuracy, or live-input authority.
