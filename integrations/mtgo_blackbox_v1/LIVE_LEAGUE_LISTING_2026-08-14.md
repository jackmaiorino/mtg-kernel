# Live League selected-listing calibration, 2026-08-14

## Scope

The signed-in client was navigated through the visible Modern event list until
the Modern League tile was present. One supervised visible click selected that
tile. The client was then returned to Home without choosing an entry option or
opening an entry review.

No event was entered, no queue was joined, no human match was started, and no
resources were spent.

## Local previews

All previews use client version `3.4.158.4691`, `1550 x 925`, and 120 DPI.

| State | Frame PNG SHA-256 | Manifest SHA-256 |
| --- | --- | --- |
| Modern League tile visible in Event Browser | `3c047c81a9a6d80230464e7768bea05081ff45be8238d809fb2ff6b9f18d93ae` | `4b05a62ff709f910685269192a5a6182ab33498186fcf5dbd22440b6249bdf00` |
| Modern League selected | `3e9afe682149a71d08114d4d0a3f4185ff799382705be612b03de3a8d011b67d` | `c71dcb49431d2fea0f0e4ea4d50e752ae451b08b4fb4c7d1cfb2964eeeb40445` |
| Home restored | `76c9300512226336a9e765a716f07bbc1168316076a303ec61e0e2596e2066d8` | `8947f6f9e0df08db68d076ef823c79f1cc21d36120dacdda08d5898b8efe1e9e` |

The PNGs and manifests remain local outside the repository. Each is pending
visual review and unsafe for semantic evidence, OCR, policy scoring, input,
queue entry, or spending.

## Sanitized visible facts

The selected League screen exposes:

- the Modern League event label and closing countdown;
- the exact already-selected 60-card test deck;
- `Change Deck`, `More Info`, `View Prizes`, and `Undefeated Decks` controls;
- a zero-trophy account status;
- separate entry choices of 10 Event Tickets and 100 Play Points;
- a visible public leaderboard and trophy table;
- no Open Entry Review, Join, or entered-event control before an entry option
  is selected.

The public leaderboard contains screen names in the local pixels. No account
label, leaderboard name, numeric event identity, or raw text region is copied
into this committed record.

## Consequence

Together with `LIVE_CHALLENGE_DECK_GATE_2026-08-14.md`, this supplies one live
source for each required selected-listing mode slice. It also confirms that
League and Challenge have distinct content and price layouts while sharing the
same payment-choice boundary.

These are calibration sources, not reviewed classifier cases. The two-slice
selected-listing semantic evaluation and its production root remain empty.
The third deck-gate state remains closed because it requires a separate entry
option choice.
