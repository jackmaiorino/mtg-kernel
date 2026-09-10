# Deck-model harness plan: completion record (2026-09-10)

Plan: `docs/superpowers/plans/2026-09-09-deck-model-harness-and-search.md` (Tasks A to E), executed on branch `lead/deck-model-harness-v1` cut from the card lane at dde5d529. Design authority: `docs/superpowers/specs/2026-09-09-deck-model-sideboarding-and-brewing-design.md` (revision 4, panel-reviewed; Codex re-review requested after its usage window reopens on 2026-09-14). Every task had a fresh implementer, a multi-dimension panel review with adversarial refutation, and scoped re-reviews of fix rounds; the whole branch had a final review on the most capable available model.

## Commits

| Task | Commits | Review outcome |
| --- | --- | --- |
| A, explicit-deck seam (W1) | bae2bc98 | clean |
| B, paired BO1 estimator (W2) | 4b6c7d22 | clean; alpha deferred to E |
| C, removal and counterspell tag file (W4) | b2892356 | clean; ratification bundle for ruling 6 |
| D, GameSummaryV1 extractor (W3) | 07b7a0f2, 5fa750fa | one fix round (Critical: Targeted precedes SpellCast; resource-curve fields; incarnation scoping) |
| E, search campaign (W5) | 1a9941e6, b3e78746, 23b5f1be | two fix rounds (three Critical defects inherited from the plan's Step 13 code: fixed mainboard every game, hardcoded chooser, unused manifest alpha) |
| Final review | 25a099f6, a93f9ddd | one fix round (Important: opponent evidence gated on registered ids rather than the object's owner) |

## Facts established

- `CombatDamageToPlayer` is a marker copied from the same `Damage` events (engine.rs `combat_damage_wave`); damage totals fold from `Damage` only.
- `finalize_owned_cast` commits the cast's `Targeted` event before its `SpellCast`; the cast-target proxy searches a window bounded by the previous `SpellCast` and the cast's stack-leave event.
- `prepare_game_v1` applies the checked-in sideboard policy (no plans today) and cannot carry a candidate plan until W8a; the BO3 driver applies the plan under test itself for game_index 2 and above (game 1 registered for both seats).
- `bo3_match` rotates the play/draw chooser to the loser; the driver reads the chooser from the live phase. Pre-registered rule: the chooser always chooses Play.
- `opponent_evidence[seat]` is seat's evidence about its opponent, gated on the object's owner; registered decks share card ids (Burn and Rally share 66, 76, 127).
- Trace mainboard hashes use `DeckConfigurationV1::mainboard_sha256_v1`'s convention, named in a `hash_convention` field.

## Accepted residuals

- The cast-target window can misattribute an unrelated ability's `Targeted` event logged between the previous `SpellCast` and the cast (documented in code and design section 3).
- The real-episode owner test is a soundness check; the synthetic shared-card-id test carries the proof.
- The fold-local incarnation tracker can trail the engine's `zone_change_count` before battlefield residency (resynced from `CombatDamageToPlayer`).

## Before any W6 campaign

- Replace the manifest template's `calibration_mean_seconds_per_game` (0.0 placeholder; the measured 0.0546 s per trial is a debug-build, E-core, random-policy floor) with a release-build P-core measurement before deriving `n_per_cell`.
- Add the Rust versus Python paired-bootstrap cross-check test.
- Owner rulings: Task C's scoping (mechanics-based versus target_spec-based), the game_index 4 and above carry-forward rule, the sequencing exception's coverage of the search campaign, and whether the warm start draws from the fact-checked research draft.
