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

## Nonclaims

This check proves the current signed client, exact rebuilt observer chain,
one-player navigation, one visible land-play postcondition, one complete
one-player turn boundary, and fail-closed two-player projection requirement. It
does not qualify a data-bearing two-player observation, model scoring, ordinary
or combat dispatch, match-history import, event entry, or spending. All
production source and dispatch ratification roots remain empty.
