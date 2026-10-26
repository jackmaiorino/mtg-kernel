# Pauper, Legacy and Vintage card support

Status: design, 2026-10-26. Owner: the Claude session assigned "general Pauper, Legacy and
Vintage card support". Codex's FDN work (#110, PR #308) is unaffected.

## Goal and measure

"Support a format" means the kernel can play the decks people actually play in it. The
measure is the **playable metagame share**: the summed metagame percentage of archetypes
whose whole mainboard is Full in one build. Best-of-one mainboards come first, as in the
roadmap; sideboards follow once a format's mainboards are playable.

`python/tools/eternal_meta_census_v1.py` computes it from a committed snapshot
(`data/eternal_v1/meta_snapshot_mtggoldfish_20261026.json`: 153 archetypes, one
representative MTGGoldfish paper list each). The snapshot is refreshed when the metagame
moves, and the census numbers in each PR come from it.

## Baseline (snapshot 2026-10-26)

| Format | Archetypes | Distinct cards | Full now | Program exists in FDN or Standard | Missing | Playable share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Pauper | 60 | 535 | 164 | 13 | 358 | 20.6% |
| Legacy | 60 | 563 | 38 | 25 | 500 | 0% |
| Vintage | 33 | 427 | 22 | 17 | 388 | 0% |
| All three | | 1,225 | 164 | 42 | 1,019 | |

Completing decks greedily by share per new card gives these card counts:

| Format | 50% of field | 75% | 90% |
| --- | ---: | ---: | ---: |
| Pauper | 15 | 79 | 154 |
| Legacy | 102 | 190 | 289 |
| Vintage | 84 | 110 | 159 |

Legacy and Vintage share most of their staples, so the joint count is lower than the sum.
For scale, the MageZero Standard completion delivered 225 cards in four days.

## Catalog

All three formats share one build, `eternal-fixtures`. Its registry is the frozen 192-card
Pauper prefix, then the Standard registry unchanged, then `data/eternal_v1/cards_v1.json`.

- The feature enables `standard-magezero-fixtures` (and through it FDN rules code), as
  Standard enables FDN. Every existing Standard card keeps its ID and program, so no
  Standard-gated code sees a different catalog.
- Cards whose program exists only in FDN are copied into the eternal file, as Standard
  copied Burst Lightning.
- The default Pauper build, its hash and every frozen store are unchanged. New Pauper cards
  go in the eternal file, since the Pauper registry is frozen.
- Eternal IDs are not frozen until a model trains on them. If Standard ever appends a card,
  eternal IDs shift; that is acceptable before the first eternal model and is recorded in
  the catalog identity.

## Work order

Waves are ordered by share gained per card. Each wave is one PR (one coherent deliverable),
built and tested on CI or HaleysPC while the desktop is reserved.

| Wave | Content | Cards | Result |
| --- | --- | ---: | --- |
| E0 | Feature, empty eternal registry, catalog identity, deck-resolution test, CI job, census | 0 | Infrastructure |
| E1 | Pauper close-ins: Mono-Red Madness, Rally, Grixis Affinity, Dimir Faeries, Grixis and Dimir Control | 16 | Pauper 21% to about 53% |
| E2 | Eternal staples: fetchlands, original and modern duals, Wasteland/Strip Mine, Force of Will/Negation, Daze, Misstep, Thoughtseize, Swords, Fatal Push, Dismember, Moxen, Lotus, Sol Ring, Crypt, Vault, Ancient Tomb, Ancestral, Time Walk, Demonic Tutor, Bauble | about 60 | Shared base for both formats |
| E3 | Legacy decks greedily: Dimir Tempo, Izzet Delver, Doomsday, Sneak and Show, Aluren, Eldrazi, Boros Energy | about 100 | Legacy about 50% |
| E4 | Vintage decks: Initiative, Jewel and Raker Shops, Lurrus control lists | about 60 | Vintage about 55% |
| E5 | Pauper to 90%, Legacy and Vintage to 75%, then sideboards | about 350 | |

Rules primitives the waves need, beyond what the engine has: two-subtype intrinsic land
mana; pay-life and pitch alternative costs with "not your turn" conditions; extra turns;
naming a card (Pithing Needle, Disruptor Flute, Cavern of Souls); restricted mana
(Workshop); "can't be activated" and cost-tax statics (Null Rod, Trinisphere,
Thalia-style); companions (Lurrus); coin flips (Mana Crypt, seeded); delayed draws
(Mishra's Bauble); Leyline opening-hand placement. Each is built once, in the first wave
that needs it.

## Card contract and verification

Each card follows the existing contract: an explicit `engine_capability` of Full only when
every reachable branch is implemented, a typed program (no name-only behavior), and native
rules tests for each new mechanic family. Every wave adds:

- Python deck resolution of every newly playable snapshot deck (no build).
- A public-session test that plays each newly playable deck against a field opponent to a
  natural terminal for fixed seeds, with transcript hashes for deterministic replay, like
  `standard_sessions_completion_v1`.
- The census output for the wave, in the PR description.

Gameplay support is engineering, not playing-strength evidence. Policy encoders, training
and Spellbench entries for eternal formats are separate work; frozen Pauper encoders keep
refusing unsupported state.

## Execution

Family workers implement bounded card groups in their own worktrees; one integrator owns
cross-family repairs, CI and the wave PR. Builds and tests run on hosted CI or under a
HaleysPC reservation; no local build runs while `host_slots` shows the desktop claimed.
Cards absent from the local XMage data (59, mostly 2026 sets) take Oracle text from
Scryfall at implementation time.

Out of scope here: the long tail of cards outside the snapshot (a later Oracle-template
generator can cover vanilla and templated cards), Commander-only mechanics, and model work.
