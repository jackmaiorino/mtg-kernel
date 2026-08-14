# Live Challenge deck-gate calibration, 2026-08-14

## Scope

One supervised visible click selected the current Modern Challenge listing from
the signed-in Event Browser. The client exposed its event summary, deck gate,
and payment choices without entering the event. Two further supervised clicks
selected the already-loaded 60-card test deck through the owned Modern deck
chooser. The client was then returned to ordinary Modern Quick Play.

No ticket or Play Point choice was selected. No Open Entry Review control was
exposed, no event was entered, no queue was joined, and no resources were
spent.

## Local previews

All previews use client version `3.4.158.4691` at `1550 x 925` and 120 DPI,
except the owned deck chooser, which is `885 x 734`. They remain local outside
the repository and are pending visual review.

| State | Frame PNG SHA-256 | Manifest SHA-256 |
| --- | --- | --- |
| Challenge selected, deck missing | `e84dab792e4ba466fe21774df8ffa05b878f32d64f5b698927c483d06ef2493d` | `e9136be0c0f7c3195596906e81c66920759b15738629b0262ec663b8d77c605a` |
| Owned Modern deck chooser | `3b0609ce49a2c9fc4223cea429298404cf90397c487f1d537bf8b6c3bbccbf0f` | `6e9276a41a2e4a5eb8e84d3ee55feb75d9992a4fd09cf5a77fe72ffb36200b29` |
| Test deck selected in chooser | `57b1760a5288748ba3b8d91934f4032686c73050a9e51e19864a3ac06302652b` | `615c4932572e139f8aa2e7c86a75a39243b3a89c97ac4a171dd9d582d5f603ee` |
| Challenge with compatible deck selected | `6723045087098e2cb91166f91188c6c38416cf6f90c2e07ac9ca95775b5c445c` | `6014a2a6bf30d63d8938e4eee61185122082899f3e64931ecbda5ec4b39b7910` |
| Quick Play restored | `a5538e3beb59c9767a341069a09b06cf7e0c47a57be3a52ab281ffaf62adb252` | `984f203728698db65d1bffd3e6dbfcd4e4469678fb7d673cf8a45256dc72e640` |

Every manifest marks the pixels unsafe for semantic evidence, OCR, policy
scoring, input, queue entry, and spending. Account labels and the visible
numeric event identity are deliberately not reproduced here.

## Sanitized visible states

The first selected-listing frame has the exact structural shape of
`AwaitingCompatibleDeckSelection`:

- the Challenge label, start time, countdown, and participant panel are
  visible;
- `Please Select a Deck` is visible;
- `Change Deck`, `More Info`, `View Prizes`, and `View Event` are visible;
- 30 Event Tickets and 300 Play Points are displayed as separate entry
  choices;
- neither entry choice is selected;
- no Open Entry Review control is visible.

The owned deck chooser contains one Modern deck. Selecting it exposes the
exact 60-card deck contents and enables the chooser's `Submit` control. That
Submit only confirms the deck choice and returns to the selected listing.

The final selected-listing frame has the exact structural shape of
`CompatibleDeckSelected`:

- the same Challenge remains selected;
- the exact test deck label and art replace the missing-deck prompt;
- the same two entry choices remain visible and unselected;
- no Open Entry Review control is visible.

This directly confirms that deck selection and payment selection are separate
gates in the current client. The third
`OpenEntryReviewAvailable` state still requires choosing an entry option and
therefore remains closed pending an informed owner decision.

## Non-claims

This is a narrow live source for the Challenge selected-listing and deck-gate
corpora. It is not a reviewed classifier result and does not populate any
production semantic or authorization root. It does not authorize an entry
choice, Open Entry Review, event entry, spending, queueing, or a human match.
