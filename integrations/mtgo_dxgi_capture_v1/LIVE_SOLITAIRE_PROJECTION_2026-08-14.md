# MTGO Solitaire projection check, 2026-08-14

## Manifest

- Adapter source commit for the producer: `0610a11e7537874cdafaf5950cf3bf5e2f9de1d0`
- .NET SDK: `9.0.200`
- Rust: `rustc 1.94.1 (e408947bf 2026-03-25)`
- Cargo: `cargo 1.94.1 (29ea6fb6a 2026-03-24)`
- MSVC: `19.50.35725` x64 with `/Brepro`
- Seeds: not applicable
- GPU ordinal: not applicable
- MTGO process start: `2026-08-13T08:18:17.0229803Z`
- MTGO executable SHA-256: `bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92`
- Observe-only broker SHA-256: `e83e1f08260cdbd80e68527964de54afd2774beed8f344b03f609fdd75a34fab`
- Dormant dispatch broker SHA-256: `95dcfe3eb38006e8dd26800776ef2e329aef1a9dbf265ba5a0bfe179247b9e49`
- Bootstrap SHA-256: `1d764382d56fe27aa845acf10b92ee8b9effd79d161baeaace1294a2d01c8c9b`
- Producer SHA-256: `3b19cd76514350f33a1d96ce7a69b7168f394a54cd7b5c70422d0f61b39f1596`
- Strict validator SHA-256: `4e0eea73bf592a0a80aa5e42a05f191640b4f1f46d917f1b1bca80a81815334c`

The producer was built twice from the recorded source commit with identical
bytes. The native broker was built twice in separate output directories with
identical observe-only and dormant-dispatch bytes. The broker source policy
test passed. The release Rust adapter and review-capture binary compiled with
the updated pins.

## Visible run

MTGO opened a no-cost one-player Freeform Solitaire game with deck
`mtgo-solitaire-kernel-basics-v1`. The rendered opening prompt asked whether to
mulligan to six or keep seven. UI Automation exposed five Islands, two Forests,
and exactly one enabled `KeepButton`. Invoking that exact button advanced the
rendered client to Turn 1, first main phase. The rendered Game Log reported the
turn and one draw, and the prompt exposed land play and pass controls.

The release-pinned observation-only broker then returned exactly:

`{"result_kind":"abstained","reason":"projection_incomplete"}`

The strict review-artifact writer rejected that abstention and created no
output directory. Source inspection explains the result: the competitive
projection deliberately requires exactly two visible player panels, while the
Solitaire surface rendered only the local player panel. No missing opponent was
synthesized and no one-player state was mislabeled as a two-player decision.

## Visible control trace

After the failed projection produced no artifact, one pointer calibration was
performed entirely against visible accessibility bounds. A double-click at the
center of the leftmost visible `Image-Forest` hand card, bounds
`x=1039,y=879,w=100,h=74`, produced all three required rendered
postconditions: Forests in hand decreased from three to two, one Forest
appeared on the battlefield, and the rendered Game Log added
`UnbuckledPie plays Forest.`

The footer text `OK / Pass` did not expose an invoke pattern and a click on its
label produced no visible change, so it was not treated as a confirmed action.
The prompt instead exposed an enabled `CombatButton` with an invoke pattern.
Invoking that exact button advanced from first main to second main and exposed
one enabled `End StepButton`. Invoking `End StepButton` advanced through the
remaining empty turn to Turn 2 first main. The rendered Game Log added the Turn
2 and draw entries, and the hand returned to eight cards. This is a complete
visible one-player turn-boundary trace with no event entry or spending.

## Visible accessibility catalog cases

A first-main catalog probe found two exact visible `Combat` matches and zero
matches for `Keep`, `Mulligan`, `Cancel`, and `Submit Deck`. The paired DXGI
review artifact was manually checked against the full before and after client
frames and both `Combat` crops. Its artifact commitment is
`401adad0e54dbb2be9f7bfba3c05910eea282d9fa31c2d187e5be819b23279e2`.
The completed review commitment is
`7a9f0bd51860b4b2d6ee35c803780c3e2894e50b9726e6b1bef00126bf1f8af3`,
and the completed review file SHA-256 is
`56425b2f7bc15f79a1e0df1ea21cfb134c50e6dbf44dd8f19b976dc8e271a322`.
The case remains non-ratified and unsafe for semantic evidence, policy
scoring, or input.

That live artifact exposed two probe defects. A visible zero-area Windows shell
island caused the occlusion audit to fail even though it could not cover a
pixel. The audit now records successful empty-area DWM bounds as
non-occluding, while failed or inverted bounds still reject. The persisted
crop loader also compared a coordinate-bound region commitment to a plain
pixel hash. It now rediscovers an identical before/after crop position and
recomputes the exact coordinate-bound commitment. Focused tests cover both
fixes. A one-case corpus evaluation then rejected as designed because the
corpus does not yet contain both reviewed presence and reviewed absence for
every fixed catalog label.

The first Solitaire game was ended through the exact visible `Concede Match`
confirmation. A second no-cost one-player game was created through the visible
`Custom Match` dialog with one player and the same test deck. Its opening-hand
catalog probe found two exact visible `Keep` matches, two exact visible
`Mulligan` matches, and zero matches for the other three catalog entries. A
static Desktop Duplication frame was not available within the fixed timeout,
so no pregame pixel-review artifact was written. The exact enabled
`KeepButton` was then invoked. Both pregame controls disappeared and one
enabled `CombatButton` appeared, proving the visible pregame-to-first-main
control transition without granting model or input authority.

## Relogin repeat

After a later relogin to the same signed client process and executable, a new
no-cost one-player Custom Match was opened with deck
`mtgo-kernel-modern-basics-v1`. The foreground title was
`(Solitaire): Freeform: Vs. UnbuckledPie`, and the rendered client remained at
the opening Keep or Mulligan prompt. No prompt action was invoked.

The native observe-only broker and strict validator were rebuilt from their
recorded source commits with the recorded toolchains. The observe-only broker
reproduced SHA-256
`e83e1f08260cdbd80e68527964de54afd2774beed8f344b03f609fdd75a34fab`.
The strict validator reproduced SHA-256
`4e0eea73bf592a0a80aa5e42a05f191640b4f1f46d917f1b1bca80a81815334c`.
The validator build required the recorded adapter source path as well as the
recorded source commit and `/Brepro`; rebuilding the same commit from another
absolute worktree path produced different bytes and was rejected.

The full duel qualifier rejected before observation because its fixed title
gate accepts only `(1-on-1)` windows. The separately pinned observe-only broker
was then invoked twice without any dispatch arguments. Both runs returned the
exact sanitized result:

`{"result_kind":"abstained","reason":"projection_incomplete"}`

The canonical UTF-8 result SHA-256 was
`4bcf3ea8c2287b476d6207c2bba7a35024e0a3fe4f9fd0f0124b1d7e5f8d8029`.
The MTGO process remained responsive. This repeats the intended fail-closed
one-player behavior and also identifies that the attended capture qualifier is
duel-only rather than a general practice-surface qualifier.

## Six-card Keep calibration source

The same no-cost game supplied one missing practice-only transition without
entering an event or playing a human. One exact visible enabled
`MulliganButton` was invoked from the reviewed seven-card opening prompt. The
first expected postcondition sentence was incorrect, so no further input was
sent until a new review-only capture and an exact visible-text query confirmed
the actual six-card prompt:

`Mulligan to five cards or keep six of these? (Cards not kept will be put onto the bottom of your library.)`

That pending-review frame has SHA-256
`f6a7d7b985d25b8d1f607ddba85eb11cd5e444bb90a70632062fbdd92afe58dc`.
One exact visible enabled `KeepButton` was then invoked. The initial check
incorrectly expected a visible Done control, so input again stopped until a
second review-only capture and exact visible-text query confirmed the actual
zero-selected, bottom-one state:

`Put a card on the bottom of your library. (Click Done to proceed. The last card clicked will be the bottom card of your library.)`

That pending-review frame has SHA-256
`fbffec826a8367e0d41ceba219e8c98e64596ba5c0ba6bb27c494557d82bd9cb`.
It visibly contains Cancel, no Keep or Mulligan control, and seven cards. Done
is absent until one card is selected. The two captures remain explicitly unsafe
for semantic evidence, OCR, policy scoring, or input. They are calibration
sources only. The generic competitive pregame response contract already
represents `required_bottom_count = 1` and `selected_bottom_count = 0`, but the
reviewed capture profile and evaluated runtime for that state remain missing.

## Bottom-one Select and Done calibration trace

After a fresh signed-in identity check, the same no-cost Solitaire game remained
paused at the zero-selected, bottom-one prompt. The installed client reported
product and file version `3.4.158.4691`; the pinned executable hash and signer
identity were unchanged. An exact visible-text query found one bottom-one
prompt, one Cancel control, and zero Done controls. Its sanitized report
commitment is
`9b7c7125ef1687adb037dffeb64c82b2b70b3e2a8e42d94c99f5ba19f57cf911`.
The current composed-desktop calibration frame has SHA-256
`b18cb0c4c431ab208e83c0ee005b2e9615ad9f940041093a9d8857bef3a9d9c4`.

The exact accessibility names used for ordinary hand-card play did not match
the bottoming cards, so no accessibility target was guessed. The full visible
frame was reviewed, and one supervised click selected the visibly leftmost
Island at client point `(380, 785)`. The newer frame showed six remaining
cards, the unchanged bottom-one prompt, and both Done and Cancel. The exact
visible-text report commitment is
`5af61bee461d2471b1eb6264dfb7b0078994ec490ae49a816780ab28fc81b8a9`,
and the selected-state frame SHA-256 is
`aee789bea0cddc946eae12b6922d2140a7e240096855204e7a4473a8e448d192`.

One supervised click then selected the visibly rendered Done control at client
point `(55, 170)`. On the newer state, the bottom-one prompt, Done, and Cancel
were all absent, and one exact visible Combat control was present. The rendered
Game Log also stated that the player put a card on the bottom of the library
and began the game with six cards in hand. The final exact visible-text report
commitment is
`fcc0c3b48d017fd9bbe9d574e84f48e50de520d391334a1facc42a72b2c9ca0e`,
and the first-main frame SHA-256 is
`ff64f1fe4ae041c14f4b5b68296d504416c8aeed583e88f3c511e68777a21845`.

All three images remain local pending-review calibration previews and are
explicitly unsafe for semantic evidence, OCR, policy scoring, or input. This
trace confirms the player-visible Select, Done, and action-specific
postcondition shapes for bottom-one practice wiring. It does not ratify the
production capture backend, choose a competitive action, or grant event-entry
authority.

The legacy process-epoch Game Log probe then rejected because this long-lived
MTGO process had created three persisted logs rather than its required sole
candidate. A diagnostic selected only the uniquely most recently modified
process-epoch log and passed it to the path- and text-withholding semantic
summary command. The first run identified one exact grammar defect:
`visible_game_log_count`. The rendered bottoming record uses `a card`, while
the parser admitted only number words such as `one card`.

The grammar now accepts `a` as one only in the singular bottomed-opening-hand
record; `a cards` still rejects. On the same private source, the sanitized
summary then reported five classified and zero unsupported or unclassified
records: one joined-game, one mulligan, one bottomed-opening-hand, one
turn-started, and one draw event. It emitted no raw text, aliases, source IDs,
card names, or paths, and remained unsafe for model scoring and input. The
unique-latest selection was a calibration diagnostic, not a replacement for
the competitive Pairing Ready baseline and match-scoped Game Log lease.

## Nonclaims

This check proves the current signed client, exact rebuilt observer chain,
one-player navigation, one visible land-play postcondition, one complete
one-player turn boundary, one reviewed positive `Combat` catalog case, live
positive `Keep` and `Mulligan` accessibility matches, one complete visible
bottom-one Select and Done transition, and fail-closed two-player projection
requirement. The relogin repeat additionally proves that the pinned observer
and validator remain reproducible and return the same abstention in a fresh
one-player Custom Match. It does not qualify the accessibility catalog, a
data-bearing two-player observation, model scoring, ordinary or combat
dispatch, a competitive pregame classifier, match-history import, event entry,
or spending. All production source and dispatch ratification roots remain
empty.
