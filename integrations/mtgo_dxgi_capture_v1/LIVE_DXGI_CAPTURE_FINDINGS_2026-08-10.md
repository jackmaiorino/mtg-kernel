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

## Nonclaims

This proves that the local backend can acquire and crop a player-visible composed desktop frame while checking the declared live conditions. It does not admit the pixels for OCR, semantic evidence, model scoring, or input. It does not eliminate the transient-occluder race between the two z-order audits. No League, Challenge, purchase, queue, or gameplay action was performed.
