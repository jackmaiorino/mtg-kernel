# Card semantics table v1 (Pauper pool)

A fixed integer feature vector for every card in `data/pauper_support_v1.json`, derived from the engine's own typed definition. It is the input of arm E of the card representation design (semantics beside the learned card id). **Nothing reads it yet**: no module, feature contract, digest or golden changes.

Files:

- `mtg-kernel/src/bin/card_semantics_table_v1/` (`main.rs` and its modules): the generator and its checks.
- `data/card_semantics_v1/pauper_table_v1.json`: the table.
- `data/card_semantics_v1/pauper_table_v1.txt`: the human-readable dump (tokens per card, mapping marks, vocabulary with carrier counts, census, pair results).
- `data/card_semantics_v1/leaf_mapping_v1.json`: the mapping layer.

Regenerate with `cargo run --release -p mtg-kernel --bin card_semantics_table_v1 -- --write`. Both modes require the default Pauper catalog. A failed census, pair pin, twin check or unrelated-pair ceiling leaves the previous files in place. `--check` also verifies the committed files against the generated output.

## Encoding

- One integer column per token. A count token says how often a feature occurs on the card; a value token carries the largest amount seen for it (clamped to plus or minus 1000) and always sits beside the count token naming its kind.
- Tokens come from a typed walk of `CardDef`: types, subtypes, colours, printed stats and keywords, mana costs (pips by colour, generic, X), alternative, additional, flashback, escape, madness and similar costs, target classes, every `EffectOp` leaf and combinator with its typed fields (serde field names and values, never `Debug` text), `EffectCond` conditions, activated, mana and triggered abilities, and permanent data (Equipment, Auras, animation, entry rules). Fixed target slots may appear in a card definition. The table contains no live object or history IDs.
- A few enums have no serde (`CostComponent`, `TriggerCondition`, `AltCostCondition`, `AttachmentDef`, the mana ability defs); they contribute only their variant name, from the head of their `Debug` text. The dump lists every such name (39).
- Integers only, identical on every platform. The table records `KERNEL_CARDDB_HASH`, the pool and mapping hashes and a content hash; a card database change fails `--check` until the table is regenerated.
- The vocabulary only grows: regeneration keeps the committed column order and appends new tokens.

## The first table

171 cards, 488 columns, card database `064a7c989255ab3c`. A card has 6 to 28 nonzero columns (median 14).

| Census | All tokens | Effect tokens (`leaf:`, `op:`, `cond:`) |
| --- | ---: | ---: |
| Distinct tokens | 488 | 241 |
| Tokens on exactly one card | 285 | 163 |
| Cards with a token no other pool card has | 120 | 75 |
| Cards with no effect token | 34 | 34 |

Pairs (Jaccard index of nonzero (column, value) pairs; every measured value is pinned):

| Pair | Expected | Measured |
| --- | --- | ---: |
| Llanowar Elves, Fyndhorn Elves, Elvish Mystic (three pairs) | equal | equal |
| Mental Note / Thought Scour | 0.60 or more | 0.77 |
| Lightning Bolt / Chain Lightning | 0.50 or more | 0.67 |
| Ponder / Preordain | 0.40 or more | 0.50 |
| Faerie Miscreant / Faerie Seer | 0.40 or more | 0.46 |
| Lightning Bolt / Fireblast | 0.40 or more | 0.35 (below) |
| Dispel / Spell Pierce | 0.40 or more | 0.33 (below) |
| Lead the Stampede / Winding Way | 0.30 or more | 0.19 (below) |
| Five unrelated pairs | at most 0.05 to 0.20 | 0.00 to 0.16 |

The floors were written before the first table was generated. Three like pairs fall below them; they are reported and pinned, not failed, and the floors were not moved. Equal twins and unrelated pairs are hard checks. Every like pair ranks at or above the 88th percentile of all pool pairs.

## The mapping layer

`leaf_mapping_v1.json` maps a card-named token (an `EffectOp` leaf, trigger condition, condition or cost part) to the shared tokens of the generic definition of the same rules, with a one-line justification each; only the generator reads it. **v1 has no entries.** 72 leaf, trigger, condition and cost kinds are carried by one pool card; most are generic engine leaves that one card happens to use, and each mapping needs that leaf's implementation read, so entries are follow-up work. A mapping change regenerates the table and moves the pinned census.

## Limits

The walk reads the definition, not the engine's behaviour: two leaves that do the same thing under different names stay different columns until mapped. A value token keeps the largest amount, so a card with two amounts of one kind loses the smaller.

## Cube table

A second generated table covers the Vintage Cube list (`data/cube_v1/mtgo_vintage_cube_20261007.json`) in the `eternal-fixtures` build. **Nothing reads the cube table yet**: no engine module, feature contract, digest or golden reads it.

Files:

- `data/card_semantics_v1/cube_table_v1.json`: the cube table. Rows are every card in the cube list that has an executable definition (`Full` or `Partial`) in the `eternal-fixtures` build, plus the five basic lands (`Plains`, `Island`, `Swamp`, `Mountain`, `Forest`). Cube cards without an executable definition are excluded from the rows and listed as `not implemented` in the dump and report.
- `data/card_semantics_v1/cube_table_v1.txt`: human-readable dump with the same sections as the Pauper dump (`Census`, `Cards`, `Vocabulary`, `Token families`), with the cube report in the census section.
- Shares `data/card_semantics_v1/leaf_mapping_v1.json`, the schema string family (`card_semantics_table/v1`), the typed token walk, and the integer-only count/value encoding (value tokens clamped to plus or minus 1000).

Regenerate and check (requires `--features eternal-fixtures`):

- Regenerate: `cargo run --release -p mtg-kernel --features eternal-fixtures --bin card_semantics_table_v1 -- --cube --write`
- Check: `cargo run --release -p mtg-kernel --features eternal-fixtures --bin card_semantics_table_v1 -- --cube --check`

The default build refuses `--cube`. Without `--cube`, the `eternal-fixtures` build refuses to write the Pauper table and notes that the Pauper table is checked in the default build. The cube table records the eternal build's `KERNEL_CARDDB_HASH`, the cube list SHA-256, the mapping SHA-256, the SHA-256 of the Pauper vocabulary it extends (`index\tkind\ttoken\n`), and a content SHA-256. A card database mismatch fails `--cube --check` and names the regeneration command above. Hard validation failures leave previous output files untouched.

Column rule:

- The cube vocabulary starts with the Pauper table's columns in the exact Pauper table order so column `i` means the same token in both tables. A Pauper column that no cube row carries is kept with 0 carriers (all zero).
- Cube-only tokens are appended after the Pauper prefix in sorted deterministic order. Regeneration preserves the committed cube column order and only appends new tokens.

Cube report (written in `cube_table_v1.txt` and printed by `--cube --check`; informational only, with no pass or fail thresholds; the Pauper pair checks are not applied to the cube table):

- Row count, vector width, Pauper prefix width, and number of columns new beyond the Pauper vocabulary.
- **Low-distinctiveness cards**: for rows `a` and `b`, `overlap(a, b)` is the count of nonzero `(column, value)` pairs of `a` that `b` also carries divided by the count of nonzero `(column, value)` pairs of `a`. The report counts and lists every row `a` for which at least 10 other cube rows `b` satisfy `overlap(a, b) >= 0.75` (exact integer arithmetic).
- **Cards with no usable features**: rows with no effect-tree token (`leaf:`, `op:`, `cond:` prefixes), grouped by a reason derived directly from `CardDef` (never by card-name lists), such as partial definition without a walked effect program, land with only a mana ability, nonland permanent with only a mana ability, saga chapter effects not walked, transform back face not walked, planeswalker loyalty abilities not walked, vanilla body or engine-side creature rules only, or noncreature permanent with engine-side or static rules only.
- **Census**: the same token census as the Pauper dump (distinct tokens, tokens on exactly one card, cards with a token no other row shares, cards with no effect-tree token), before and after mapping, for all tokens and for effect-tree tokens.
- **Not implemented**: cube list entries without an executable definition in the `eternal-fixtures` build, listed with reason `not implemented`.
