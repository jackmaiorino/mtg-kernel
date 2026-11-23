# Vintage Cube halt census v1

How often a 40-card Vintage Cube game fails to end naturally on the kernel, by cause, with a replayable game for each cause. Part of #710. Measurement only: no engine change.

**Setup.** Main at `f7bc11ddd`, `eternal-fixtures` build, card database `9d7ddc19b3906513`. The driver is `mtg-kernel/examples/cube_census_v1.rs`. Each game deals two random 40-card decks from the cube cards the gap record does not list: two colours per seat, 23 nonland cards drawn inside those colours (colourless allowed), 2 nonbasic lands drawn from all cube lands regardless of colour, and 15 basics split 8 and 7 between the two colours. No cube card is in both decks; basics can be. The games run through the custom-deck JSON session at schema 5, uniform over the legal menu, with seed 20261122 for games 0 to 239,999. They ran in 12 separate worker processes, so a process death costs one game.

Natural 226,265 (94.28%); not natural 13,735 (5.72%). 4,183 games per core-hour (12 worker processes).

| Cause | Games | Share | Cards in most of these games | Last action's source | Replay game |
| --- | ---: | ---: | --- | --- | ---: |
| `fail_closed:observation:extra turns require their selected public/Human successor` | 6,941 | 2.892% | Time Walk 99%, Coveted Jewel 25%, Lotus Petal 25% | Time Walk 4860, Island 599, Mountain 224 | 1 |
| `fail_closed:observation:policy schema v5 cannot represent a staged Escape graveyard-exile prefix; a versioned object-cost successor is required` | 4,499 | 1.875% | Underworld Breach 58%, Nethergoyf 36%, Engineered Explosives 28% | Nethergoyf 1441, Phlage, Titan of Fire's Fury 249, Uro, Titan of Nature's Wrath 215 | 75 |
| `engine_halted:InvalidEffectContinuation:source:*` | 1,078 | 0.449% | Sage of the Skies 48%, Aang's Iceberg 33%, Sol Ring 29% | Aang's Iceberg 253, Talon Gates of Madara 82, Chain Lightning 71 | 213 |
| `fail_closed:observation:life-payment Ward requires its typed observation successor` | 976 | 0.407% | Sedgemoor Witch 100%, Manifold Key 31%, Expedition Map 27% | Manifold Key 115, Pyrokinesis 40, Dead // Gone 36 | 90 |
| `process dies: stack overflow (0xC00000FD)` (#892) | 84 | 0.035% | Brain Freeze 100%, Snapcaster Mage 82%, Sword of the Meek 37% | Brain Freeze 83, Displacer Kitten 1 | 746 |
| `fail_closed:observation:invalid stack target metadata: stack target contract is structurally malformed` | 68 | 0.028% | Candelabra of Tawnos 76%, Thespian's Stage 47%, Manifold Key 40% | Talon Gates of Madara 12, Forest 9, Mountain 7 | 2353 |
| `fail_closed:observation:invalid stack target metadata: ability source contract is structurally malformed` | 30 | 0.013% | Talon Gates of Madara 100%, Coveted Jewel 40%, Retrofitter Foundry 37% | Talon Gates of Madara 28, Enlightened Tutor 1, Forest 1 | 3756 |
| `fail_closed:observation:legacy observation cannot identify the Ward-bound stack item unambiguously; the Ward observation successor is required` | 20 | 0.008% | Kappa Cannoneer 100%, Sensei's Divining Top 45%, Umezawa's Jitte 45% | Parallax Wave 3, Goblin Welder 2, Swords to Plowshares 2 | 25828 |
| `fail_closed:observation:live linked-exile source lost its public object relation` | 7 | 0.003% | Talon Gates of Madara 100%, Deep-Cavern Bat 100%, Pentad Prism 71% | Talon Gates of Madara 6, Mountain 1 | 11877 |
| `fail_closed:observation:Tinybones requires a distinct complete V72 consumer` | 7 | 0.003% | Embereth Shieldbreaker 71%, Mind Stone 71%, Sol Ring 57% | Embereth Shieldbreaker 5, Brazen Borrower 2 | 24240 |
| `panic: transformed battlefield return requested an undefined face` (#893) | 7 | 0.003% | Mox Ruby 57%, Expedition Map 57%, Chromatic Star 57% |  | 48985 |
| `fail_closed:nonterminal decision produced zero legal actions` | 6 | 0.003% | Kappa Cannoneer 100%, Mishra's Workshop 83%, Coalition Relic 83% | Kappa Cannoneer 6 | 24000 |
| `session error stale_environment_binding: selected action no longer matches the active policy environment: cast mode chosen before prior cast stages completed` | 4 | 0.002% | Phyrexian Dragon Engine 100%, Force of Vigor 100%, Basalt Monolith 75% | Force of Vigor 4 | 60269 |
| `fail_closed:observation:invalid stack target metadata: physical stack target lost same-incarnation membership` | 3 | 0.001% | Pompous Battlemage 100%, Mystic Confluence 100%, Timetwister 67% | Lose Focus 1, The Fantasticar 1, Island 1 | 65777 |
| `session error stale_environment_binding: selected action no longer matches the active policy environment: modal trigger placement must complete ordering before other actions` | 2 | 0.001% | Suplex 100%, Crop Rotation 100%, Thought Scour 100% | Godless Shrine 1, Painter's Servant 1 | 134025 |
| `driver_decision_cap` | 2 | 0.001% | Forensic Gadgeteer 100%, Basalt Monolith 100%, Worldspine Wurm 100% | Basalt Monolith 2 | 172721 |
| `session error stale_environment_binding: selected action no longer matches the active policy environment: ordered trigger group failed placement validation` | 1 | 0.000% | Timetwister 100%, Tinker 100%, Spellseeker 100% | Emperor of Bones 1 | 191424 |

Replay any game: `cube_census_v1.exe --mode jsonl40 --cube data/cube_v1/mtgo_vintage_cube_20261007.json --gaps data/cube_v1/mtgo_vintage_cube_20261007_gaps.json --seed 20261122 --workers 1 --first-game N --games 1 --out FILE --trace TRACE` with the pinned binary named in the manifest.

Process deaths have no row. They were found by the worker that died, and each was rerun alone with `--trace`.

**Reading.** Three known observation gaps (extra turns, staged Escape, life-payment Ward) cause 90% of the non-natural games, the same as on 11-19. `InvalidEffectContinuation` is the largest engine halt. It is spread over many sources, with Sage of the Skies and Aang's Iceberg most common. Only Brain Freeze kills the process. A training run on cube decks must fence the cards behind these causes until they are fixed. Uniform play reaches lines no test wrote down, and a trained policy reaches some of them more often (Time Walk is cast on purpose) and others less.

**Run it.** `cargo build --release -p mtg-kernel --features eternal-fixtures --example cube_census_v1`, then one invocation per chunk of games (`--first-game`, `--games`, `--workers 1`, `--out`). Rerun a single game with `--trace` to get every action.
