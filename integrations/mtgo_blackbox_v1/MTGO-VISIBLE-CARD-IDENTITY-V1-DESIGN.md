# MTGO visible-card identity v1 design

## Objective

Bind a card rendered in the visible MTGO client to one semantic card from the
predeclared deck without reading process memory, network traffic, client data
files, hidden controls, or any other nonvisual source. Any missing or ambiguous
identity stops observation construction and input.

## Calibration input

Each deck-specific calibration profile must bind:

- the exact deck manifest and kernel card-database commitment;
- the MTGO product version, 1550 by 925 client layout, DPI, and capture-profile
  commitment;
- a manually reviewed visible MTGO card name and art crop;
- the public print identifier, Oracle identifier, set code, collector number,
  and SHA-256 of the locally cached public reference image;
- a runtime template derived from the reviewed visible MTGO crop; and
- the matcher version and fixed similarity and distinct-name margin thresholds.

Public reference images remain in a local cache and are not committed to this
repository. Match-time operation is offline. The profile contains only the
minimum template descriptor needed by the matcher plus commitments to the
reviewed source material.

## Calibration procedure

1. Start with the exact predeclared decklist. It is the candidate universe, not
   a prediction from the screen.
2. Display each printing in the visible client at the reviewed layout. A human
   confirms the visible name and unobscured artwork.
3. Corroborate that manual label against a pinned public card-print image. The
   first probe uses average per-channel zero-normalized correlation across a
   fixed crop search. A public-image match is calibration evidence only.
4. Store a compact descriptor derived from the visible MTGO crop. Bind its hash
   to the manual review, print metadata, deck commitment, and client profile.
5. Reject calibration if the best distinct-name candidate does not clear both
   the absolute similarity floor and the margin over the runner-up.

## Runtime procedure

The runtime matcher accepts only an opaque admitted DXGI frame and an opaque
ratified deck calibration. It first requires a recognized current hand layout.
For every visible card ordinal, it compares the fixed art region against every
template in the deck profile. Multiple print templates with the same semantic
card name collapse to one candidate. Every ordinal must have one winning name
under the fixed distance ceiling and above the fixed distinct-name margin.

The initial runtime distance is mean absolute BGR difference over a 96 by 60
visible art crop. The one-game bottom-six reflow trace observed retained-card
distances from 0.433 through 19.571 intensity levels, while exploratory wrong
pairings began near 46.9. The current 25.000 ceiling is a calibration starting
point, not a production accuracy claim. A broader multi-game, multi-ordinal,
multi-card corpus must freeze the final ceiling and margin.

The output is an adapter-local object ID, semantic card-name candidate, print
template commitment, source-frame commitment, ordinal, and confidence
measurements. It does not create `CardStableRefV1`. A separate reviewed binding
must map the semantic name to the exact kernel card database and create a new
zone-change incarnation whenever the object changes zones.

## Fail-closed rules

- No network access occurs during a match.
- A card absent from the deck profile is `NoMatch`.
- Equal or insufficiently separated distinct-name candidates are `Ambiguous`.
- Missing, stale, occluded, cursor-covered, differently sized, or differently
  laid-out frames are rejected before identity matching.
- Partial hand identity cannot produce an observation or legal-action set.
- OCR may corroborate large visible prompt or log text, but it cannot establish
  card identity at the reviewed card-name scale.
- Identity, observation, model scoring, and input authority are separate opaque
  transitions. No runtime boolean can mint any of them.

## Evidence and remaining gates

The first public-reference probe resolves all seven cards in one visible
opening hand against six manually reviewed print templates. The weakest top
correlation is 0.6441 and the weakest distinct-template margin is 0.2105. This
is a feasibility result on one hand, not an error-rate estimate.

Before autonomous no-cost play, collect at least two games with every semantic
card in the chosen deck, every hand ordinal, battlefield renderings, tapped and
selected appearances, stack cards, graveyard rows, and duplicate-card cases.
Freeze thresholds only after a held-out confusion matrix. League and Challenge
entry remain separately disabled until the exact permission correspondence is
reviewed as covering those modes and the full observer, scorer, actuator, and
visible-postcondition loop passes no-cost play.
