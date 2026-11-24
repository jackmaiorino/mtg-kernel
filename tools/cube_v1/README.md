# Cube v1 pool and deal tools

Offline input tools for cube self-play training configs:

1. **Export kernel card ids** (requires `eternal-fixtures`):
   ```bash
   cargo run --release --features eternal-fixtures --example cube_card_ids_v1 -- \
     data/cube_v1/mtgo_vintage_cube_20261007.json > ids.json
   ```
   Emits `kernel_carddb_hash` and, for every cube card plus the five basics, `card_id`, `colors`, `is_land`, and `has_definition`.

2. **Build the active pool** (`tools/cube_v1/pool.py`):
   ```bash
   python tools/cube_v1/pool.py \
     --cube-list data/cube_v1/mtgo_vintage_cube_20261007.json \
     --gaps data/cube_v1/mtgo_vintage_cube_20261007_gaps.json \
     --id-export ids.json \
     --fence fence.json \
     --carddb-hash <16-hex-hash> \
     --out pool.json
   ```
   Separates `spells`, `nonbasic_lands`, and `basics` (each sorted by `card_id`), excludes gaps and fenced cards, and records `kernel_carddb_hash` plus the SHA-256 of every input. Refuses fenced names outside the cube list, pool cards without a definition, or a mismatched `--carddb-hash`.

3. **Deal episode schedules** (`tools/cube_v1/deal.py`):
   ```bash
   python tools/cube_v1/deal.py \
     --pool pool.json --constants constants.json \
     --namespace train --run cube-r1 --update-start 0 --update-end 8 \
     --out schedule.json
   ```
   All deal constants come from `--constants` (`deck_size`, `colours_per_seat`, `nonland_spells`, `nonbasic_lands`, `basic_split` = `"even_first_colour_extra"`, `pairs_per_update`, which must be even). Derives episode seeds via `qual.py`'s SHA-256-of-JSON `seed([namespace, run, update, slot])`, deals non-basic cube cards without overlap across seats (basic lands are exempt: both decks may hold copies of the same basic land type), splits basics by seat colour, and emits both seat orders in run-config `iterations` shape (or stamps `--base BASE.json`, swapping seat-indexed `mulligan_modes` with the decks). Deck A of a pair is drawn first. The learner holds deck A in both slots of an even pair and deck B in both slots of an odd pair; seat 0 starts, so each deck is on the play once per pair.

   **Read (b)** (`--read-b-deck DECK.json --index I` in place of the update range): `DECK.json` is a drafted deck D (a JSON list of card ids, or an object with a `mainboard` list); every id must be a card of the active pool or a basic land. Deals one opponent deck with the same constants from the active pool minus D's nonbasic cards, under the key prefix `["cube_deal_b_v1", namespace, run, index, seed(["cube_deal_b_v1", namespace, run, index])]`, and emits 4 games in one iteration: slots 0 and 2 with D at seat 0 (on the play), slots 1 and 3 with D at seat 1 (on the draw). `learner_seat` is D's seat, and game seeds are `seed(["cube_deal_b_v1", namespace, run, index, slot])`.

4. **Canonical deck hash** (`deal.canonical_decks_sha256`): hashes a list of decks, each given as card names (`deal.card_names_by_id` and `deal.deck_card_names` translate card ids through a pool). Each deck is written as sorted `name count` lines followed by one empty line, LF line endings, UTF-8, and SHA-256 runs over that text, so card order, file formatting, line endings and provenance fields cannot change it.

5. **M4 reads** (m4-interfaces/v1). Standard library only; the read constants come from a declaration file, never from defaults.
   - `pod_record.py` loads and validates `mtg-kernel-cube-pod-record/v1`: every field, type and key order of the interface's section 4, ids, the halt ledger and the score rules. Scores +1, 0, -1 read as win fractions 1, 0.5, 0.
   - `deal_common.py` deals the common opponent C(P): read (b)'s one-seat rule from the active pool minus all 45 cards of the pool P, key prefix `["cube_deal_common_v1", namespace, draft, seat, opponent, seed(...)]`, 4 game seeds `seed(["cube_deal_common_v1", namespace, draft, seat, opponent, slot])`. `deal.py` is unchanged.
   - `read_plans.py` writes one explicit plan per draft (`mtg-kernel-cube-read-plan/v1`, `seat_rule` schedule, `max_replays` 0): read (b) on A1.5's dealer, seeds and seats; reads (c) and (b') and dose reads as paired builds against C(P).
   - `readers.py --declaration DECL.json --read {a,a_early,b,b_prime,c,dose,aa} --look L --records ... [--plans ...]` writes one JSON report per read and look: input hashes, units, the statistic under the drop and both signed halt scorings, a draft- or pair-clustered percentile bootstrap, the void test and one verdict word (`win`, `loss`, `extend`, `not_won`, `void`; `symmetric` or `asymmetric` for the A/A; none for (b') and dose reads). A declaration without a signed A6 reads only the `test` namespace.

6. **Tests**:
   ```bash
   python -m unittest discover -v -s tools/cube_v1/tests -p 'test_*.py'
   ```
