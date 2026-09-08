# Live Modern spectator result calibration, 2026-08-14

## Scope

One supervised click on a currently visible `WATCH` control opened a no-cost
Modern game from the signed-in main client. The approved account did not join
the match. No event was opened, no queue was joined, no entry choice was made,
and no resources were spent.

The selected game had already ended by the time the first admitted preview was
captured. One supervised close-window click returned the client to the main
lobby after inspection.

## Local preview

- client product version: `3.4.158.4691`;
- frame size: `1550 x 925` at 120 DPI;
- frame PNG SHA-256:
  `c6f6dbd7208dd484fdb5d560c56dfec002369471eacde0be03794e697ee0abfe`;
- preview manifest SHA-256:
  `56fd42781f7f92ced52069eed620ab30390981e831fee7b5f845736996e8813f`;
- status: `pending_visual_review`;
- artifact kind: `mtgo_visible_spectator_gameplay_calibration_preview_v1`;
- title identity:
  `single_opponent_only_spectator_role_not_title_proven`.

The PNG and manifest remain local outside the repository. They contain visible
account and opponent labels and are not committed. The manifest marks the
preview unsafe for semantic evidence, OCR, policy scoring, and input.

## Sanitized visible facts

Manual inspection of the composed pixels found:

- a modal result panel stating that the match has a winner;
- one enabled `PLAY LOBBY` return control;
- a battlefield status panel stating that the game has ended;
- spectator-only `Draw A Card` and `Reveal Hand` controls;
- a rendered Game Log showing two die rolls, two game joins, one concession,
  one game win, one lost-connection record, and a visible 1-0 match lead;
- the full phase bar and both 25-minute player clocks still rendered on the
  terminal screen;
- no cards on the visible battlefield and no visible hand suitable for acting
  player calibration.

No player label, match ID, game ID, card name, raw Game Log line, file path, or
client object identifier is reproduced in this record.

## Title drift

The native foreground title exposed by `GetWindowText` contained only the
format and one opponent label. The painted title bar also displayed match and
game numbers. The previous spectator preview rule required two participant
labels in the native title and therefore rejected before saving pixels.

The preview gate now accepts the single-opponent native shape only when the
caller explicitly requests `ForegroundSpectatorGame`. It records the weak
identity above and states that the spectator role is not proven by the title.
The artifact remains non-authoritative and cannot be promoted into an
acting-player source.

## What this closes and does not close

This supplies one real, no-cost terminal-result layout sample and one concrete
client-version title-drift case. It is useful for the future lifecycle and
rendered Game Log corpora.

It does not qualify duel perception, acting-player priority, legal actions,
private hand reconstruction, model scoring, event entry, or input. The current
production semantic and authorization roots remain empty.
