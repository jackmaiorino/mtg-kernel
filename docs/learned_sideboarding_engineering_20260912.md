# Learned sideboarding engineering result, September 12, 2026

The first complete integration runs a real frozen Net8 player through cross-deck BO3, learns a separate sideboard policy from completed-game teacher traces, reloads that checkpoint, and applies its legal exchanges between games. Work is isolated on `codex/learned-sideboarding-integration-v1`, based on completed card wave1 at `1ebb3b6b`; unfinished Urzatron work and the active science campaign are separate. This branch has not landed in main.

## Completed checks

- 119 focused checks passed: head learning/serialization/legality, actor-relative privacy, paired sampling, acceptance direction, natural-terminal summaries, flat action invariants, real checkpoint import/scoring replay, CLI identity preflight and transactional BO3 configuration tests.
- Rally/Burn static teaching covered all four ordered pairings, including both mirrors: four naturally completed matches, nine games, ten acting-seat examples and 70 teacher decisions. Registrations and all eight game 2/game 3 teaching rows were checked against exact card identities and 75-card inventory. No cards were substituted or excluded.
- CPU imitation used a fixed 1000 epochs, learning rate 0.01 and seed 41002. Action agreement on those teacher decisions rose from 11/70 to 68/70; cross entropy fell from 2.05855 to 0.13313. No measured value targets were supplied. The play model and its card embeddings remained fixed.
- Reloaded checkpoint `914eb1d89bbc10b446b06d0f6de82cb1eea82ba37fb57302ae8bf8d417dfaa71` completed all four Rally/Burn pairing cases, eight games, with distinct learned instances in both seats and legal postboard configurations. These reuse engineering seeds, not reserved evaluation seeds.
- Earlier Rally/Affinity teaching completed four matches/ten games. Its smaller 100-epoch head reached 29/72 training action agreement. Learned replay completed three matches/seven games before the Affinity Map-source failure below. The failed batch remains incomplete.

The teacher is the existing fact-checked hand-authored table, explicitly labelled `hand_authored_warm_start`. It is not search-ratified. Training agreement, completed games and deterministic replay do not establish held-out sideboarding strength, a useful value estimate, BO3 promotion, MTGO readiness, or a trained brewing model. No GPU training or paid allocation was launched; original Stores and measurements were untouched.

## What broader deck execution exposed

| Path | Engineering finding | Disposition |
| --- | --- | --- |
| Hand/graveyard activated abilities | Flat validation incorrectly required Battlefield, rejecting landcycling and embalm | Fixed using the declared ability activation zone, retaining exact origin and visibility checks; regression tests cover Lorien Revealed, Generous Ent and Sacred Cat |
| Affinity Map Token | A sacrificed token ceases to occupy the graveyard while its public ability resolves; flat V2 cannot bind that historical source | Confirmed real replay and rules-path regression; retain rejection and add a versioned historical-source representation |
| Elves library search | After the activation fix, a real game reaches HiddenActionReference; the mapper only supports persistent library knowledge with true library positions | Versioned decision-local unordered search objects are required; never publish hidden library order to pass validation |
| Terror Sleep of the Dead | V5 intentionally rejects a staged Escape graveyard-exile prefix; flat V2 shares that builder | Requires an explicitly typed object-cost observation successor |

Full card-rule flags and catalog membership do not establish neural observation support or checkpoint training exposure. No unsupported action was filtered and no halted game was converted into a result. See `learned_sideboarding_v1.md` for exact reproductions, source locations and the minimum rich-policy/flat-format migration.

## Continuation

The next broad-deck dependency is the versioned observation work above. Then expose the play policy to cross-deck and postboard configurations before interpreting sideboard search preferences. Freeze that qualified player for bounded target generation and matched no-change/static/learned BO3 comparisons, with reserved seeds/configurations and analysis gates settled before measurement. The old W6 manifest still has placeholders and M60 versus Jack's M20 ruling; it was not silently rewritten or executed. Rust/Python estimator cross-check remains a W6 prerequisite.

Brewing stays in scope through pooled card-set inputs, legal card movement and explicit configurations, rather than a learned fixed list of deck classes. This v1 policy exchanges within a registered 75. Searching new registrations and representing previously unexposed cards remain distinct future work.

Evidence/config root: `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-001`. Execution root: `E:/mtg-kernel-learned-sideboarding-evidence/engineering-001`. Each execution preserves config/input pins, binary and compiled-source hashes, completion or failure, and per-match records. The final small manifest binds the final source commit, toolchain/linker, seeds and output hashes.

Independent Fable reviews `b70ff719-f81a-4594-b6e0-5c434cf3456e`, `35b71f5d-5910-4692-bbdd-6f4e1579df23` and focused encoder review `991cbc38-0df2-4caf-ae78-c74766c4beb5` all failed HTTP429 weekly quota with zero reads/tokens. There is no Fable feedback or endorsement. Independent Codex checks led to accepted fixes for natural-terminal handling, separate seat state, hidden endpoint omission, truthful exposure, harmful-candidate rejection and early checkpoint compatibility. Authorized engineering proceeded with that review limitation recorded; no scientific promotion decision was finalized.
