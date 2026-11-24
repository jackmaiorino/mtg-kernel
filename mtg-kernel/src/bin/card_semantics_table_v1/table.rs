//! Vocabulary, fixed-width vectors, the committed table text and its dump.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS, KERNEL_CARDDB_HASH};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::extract::{card_tokens, CardTokens, TokenKind, VALUE_LIMIT};
use super::mapping::Mapping;

pub const TABLE_SCHEMA: &str = "card_semantics_table/v1";
pub const POOL_PATH: &str = "data/pauper_support_v1.json";
pub const CUBE_POOL_PATH: &str = "data/cube_v1/mtgo_vintage_cube_20261007.json";
pub const MAPPING_PATH: &str = "data/card_semantics_v1/leaf_mapping_v1.json";
pub const TABLE_PATH: &str = "data/card_semantics_v1/pauper_table_v1.json";
pub const DUMP_PATH: &str = "data/card_semantics_v1/pauper_table_v1.txt";
pub const CUBE_TABLE_PATH: &str = "data/card_semantics_v1/cube_table_v1.json";
pub const CUBE_DUMP_PATH: &str = "data/card_semantics_v1/cube_table_v1.txt";
pub const BASIC_LANDS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];
const CONTENT_HASH_KEY: &str = "\"content_sha256\": ";

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        write!(out, "{byte:02x}").expect("string write");
    }
    out
}

/// Canonical SHA-256 of an ordered vocabulary (`index\tkind\ttoken\n`).
pub fn vocabulary_sha256(vocab: &[(String, String)]) -> String {
    let mut text = String::new();
    for (i, (token, kind)) in vocab.iter().enumerate() {
        writeln!(text, "{i}\t{kind}\t{token}").expect("string write");
    }
    sha256_hex(text.as_bytes())
}

/// What each token family means. The first matching prefix wins.
pub const FAMILIES: [(&str, &str); 22] = [
    ("type:", "printed card type"),
    ("supertype:", "printed supertype"),
    ("subtype:", "printed subtype"),
    ("color:", "card colour"),
    ("stat:", "printed number (mana value, power, toughness, loyalty)"),
    ("kw:", "printed keyword bit of card_def::Keywords"),
    ("cost:", "printed mana cost: pip:C counts coloured pips, generic and x are amounts"),
    ("kicker:", "kicker mana cost, same encoding as cost:"),
    ("madness:", "madness mana cost, same encoding as cost:"),
    ("plot:", "plot mana cost, same encoding as cost:"),
    ("adventure:", "adventure spell mana cost, same encoding as cost:"),
    ("omen:", "omen spell mana cost, same encoding as cost:"),
    ("bestow:", "bestow mana cost, same encoding as cost:"),
    ("costpart:", "typed CostComponent of an ability, alternative, additional, flashback or escape cost; .n is its amount; costpart:mana: is mana inside such a cost"),
    ("target:", "target class (TargetSpec) of the spell, a mode or an ability"),
    ("leaf:", "EffectOp leaf; leaf:K counts occurrences, leaf:K.field is a typed field: an integer is an amount, =X a unit variant, tag or keyword"),
    ("op:", "EffectOp combinator (conditional, choice, optional payment) and its fields"),
    ("cond:", "EffectCond condition inside a program"),
    ("ability:", "ability presence: activated (zone, timing, limit), triggered (zone, intervening if), mode, mana (colours, cost, amount), granted"),
    ("trigger:", "TriggerCondition of a triggered ability"),
    ("flag:", "presence flag or permanent data (Equipment, Aura, animation, alternative costs, entry rules)"),
    ("", "unclassified"),
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VocabEntry {
    pub index: usize,
    pub token: String,
    pub kind: &'static str,
    pub carriers: usize,
}

#[derive(Debug, Clone)]
pub struct CardRow {
    pub name: String,
    pub id: u16,
    pub tokens: CardTokens,
}

#[derive(Debug, Clone)]
pub struct PauperVocabularyRef {
    pub path: &'static str,
    pub sha256: String,
    pub len: usize,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub pool_path: &'static str,
    pub pool_sha256: String,
    pub mapping_sha256: String,
    pub pauper_vocabulary: Option<PauperVocabularyRef>,
    pub vocabulary: Vec<VocabEntry>,
    pub rows: Vec<CardRow>,
    pub vectors: Vec<Vec<i64>>,
}

pub struct CubeSelection {
    pub rows: Vec<CardRow>,
    pub before_mapping: Vec<CardRow>,
    pub not_implemented: Vec<String>,
}

pub struct TableBuildInput<'a> {
    pub pool_path: &'static str,
    pub pool_text: &'a str,
    pub mapping_text: &'a str,
    pub rows: Vec<CardRow>,
    pub prior: &'a [(String, String)],
    pub pauper_vocabulary: Option<PauperVocabularyRef>,
}

/// Card names of the Pauper pool, in file order.
pub fn pool_names(pool_text: &str) -> Vec<String> {
    let pool: serde_json::Value = serde_json::from_str(pool_text).expect("pool JSON");
    pool["cards"]
        .as_array()
        .expect("pool cards")
        .iter()
        .map(|row| row["name"].as_str().expect("card name").to_string())
        .collect()
}

/// Card names of the cube list, in file order, validating schema and uniqueness.
pub fn parse_cube_names(pool_text: &str) -> Result<Vec<String>, String> {
    let pool: serde_json::Value =
        serde_json::from_str(pool_text).map_err(|e| format!("{CUBE_POOL_PATH}: {e}"))?;
    if pool["schema"].as_str() != Some("cube_list/v1") {
        return Err(format!(
            "{CUBE_POOL_PATH}: schema {} is not cube_list/v1",
            pool["schema"]
        ));
    }
    let cards = pool["cards"]
        .as_array()
        .ok_or_else(|| format!("{CUBE_POOL_PATH}: missing cards array"))?;
    let mut names = Vec::with_capacity(cards.len());
    let mut seen = BTreeSet::new();
    for (i, row) in cards.iter().enumerate() {
        let name = row["name"]
            .as_str()
            .ok_or_else(|| format!("{CUBE_POOL_PATH}: card {i} has no string name"))?;
        if name.trim().is_empty() {
            return Err(format!("{CUBE_POOL_PATH}: card {i} has an empty name"));
        }
        if !seen.insert(name) {
            return Err(format!("{CUBE_POOL_PATH}: duplicate card name {name:?}"));
        }
        names.push(name.to_string());
    }
    Ok(names)
}

pub fn rows(names: &[String], mapping: Option<&Mapping>) -> Vec<CardRow> {
    names
        .iter()
        .map(|name| {
            let id = card_id_by_name(name).unwrap_or_else(|| panic!("{name} is not in CARD_DEFS"));
            CardRow {
                name: name.clone(),
                id,
                tokens: card_tokens(id, mapping),
            }
        })
        .collect()
}

/// Resolves cube list cards that have an executable definition (`Full` or
/// `Partial`), appends the five basic lands, and lists missing/no-effect
/// cube cards separately.
pub fn select_cube_rows(cube_names: &[String], mapping: &Mapping) -> Result<CubeSelection, String> {
    let mut rows = Vec::new();
    let mut before_mapping = Vec::new();
    let mut not_implemented = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();

    for name in cube_names {
        seen.insert(name.as_str());
        match card_id_by_name(name) {
            Some(id) if CARD_DEFS[usize::from(id)].is_executable() => {
                rows.push(CardRow {
                    name: name.clone(),
                    id,
                    tokens: card_tokens(id, Some(mapping)),
                });
                before_mapping.push(CardRow {
                    name: name.clone(),
                    id,
                    tokens: card_tokens(id, None),
                });
            }
            _ => not_implemented.push(name.clone()),
        }
    }
    not_implemented.sort();

    for land in BASIC_LANDS {
        if seen.contains(land) {
            continue;
        }
        let Some(id) = card_id_by_name(land) else {
            return Err(format!("basic land {land} is not in CARD_DEFS"));
        };
        if !CARD_DEFS[usize::from(id)].is_executable() {
            return Err(format!("basic land {land} is not executable in CARD_DEFS"));
        }
        rows.push(CardRow {
            name: land.to_string(),
            id,
            tokens: card_tokens(id, Some(mapping)),
        });
        before_mapping.push(CardRow {
            name: land.to_string(),
            id,
            tokens: card_tokens(id, None),
        });
    }

    Ok(CubeSelection {
        rows,
        before_mapping,
        not_implemented,
    })
}

/// Builds a table from already-extracted rows. `prior` fixes the leading
/// columns; new tokens are appended in sorted deterministic order.
pub fn build_from_rows(input: TableBuildInput<'_>) -> Result<Table, String> {
    let mut kinds: BTreeMap<String, TokenKind> = BTreeMap::new();
    for row in &input.rows {
        for (token, acc) in &row.tokens.tokens {
            if let Some(previous) = kinds.insert(token.clone(), acc.kind) {
                if previous != acc.kind {
                    return Err(format!("token {token} changed kind across cards"));
                }
            }
        }
    }
    let kind_name = |kind: TokenKind| match kind {
        TokenKind::Count => "count",
        TokenKind::Value => "value",
    };
    let mut order: Vec<(String, &'static str)> = Vec::new();
    let mut seen = BTreeSet::new();
    for (token, kind) in input.prior {
        let static_kind = match kind.as_str() {
            "count" => "count",
            "value" => "value",
            other => return Err(format!("prior token {token} has unknown kind {other:?}")),
        };
        if let Some(now) = kinds.get(token) {
            if kind_name(*now) != static_kind {
                return Err(format!(
                    "token {token} changed kind; a kind change needs a new table version"
                ));
            }
        }
        if !seen.insert(token.clone()) {
            return Err(format!("prior vocabulary lists token {token:?} twice"));
        }
        order.push((token.clone(), static_kind));
    }
    for (token, kind) in &kinds {
        if !seen.contains(token) {
            order.push((token.clone(), kind_name(*kind)));
        }
    }
    let index: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, (token, _))| (token.as_str(), i))
        .collect();
    let mut vectors = Vec::new();
    let mut carriers = vec![0usize; order.len()];
    for row in &input.rows {
        let mut vector = vec![0i64; order.len()];
        for (token, acc) in &row.tokens.tokens {
            let column = index[token.as_str()];
            vector[column] = acc.encoded();
            carriers[column] += 1;
        }
        vectors.push(vector);
    }
    let vocabulary = order
        .into_iter()
        .enumerate()
        .map(|(i, (token, kind))| VocabEntry {
            index: i,
            token,
            kind,
            carriers: carriers[i],
        })
        .collect();
    Ok(Table {
        pool_path: input.pool_path,
        pool_sha256: sha256_hex(input.pool_text.as_bytes()),
        mapping_sha256: sha256_hex(input.mapping_text.as_bytes()),
        pauper_vocabulary: input.pauper_vocabulary,
        vocabulary,
        rows: input.rows,
        vectors,
    })
}

/// Builds the Pauper table. `prior` is the vocabulary of the committed table:
/// its order is kept and new tokens are appended in sorted order, so a card
/// wave grows the vector without moving existing columns.
pub fn build(
    pool_text: &str,
    mapping_text: &str,
    mapping: &Mapping,
    prior: &[(String, String)],
) -> Table {
    let names = pool_names(pool_text);
    let rows = rows(&names, Some(mapping));
    build_from_rows(TableBuildInput {
        pool_path: POOL_PATH,
        pool_text,
        mapping_text,
        rows,
        prior,
        pauper_vocabulary: None,
    })
    .unwrap_or_else(|e| panic!("{e}"))
}

const ENCODING: [&str; 6] = [
    "One integer column per vocabulary token; a card's vector lists them in vocabulary order.",
    "kind count: how many times the feature occurs on the card (0 when absent).",
    "kind value: the largest amount seen for the feature, clamped to [-1000, 1000]; it always sits beside a count token naming its kind, so 0 with that count token present means amount zero.",
    "No floating point: integers only, identical on every platform.",
    "Vocabulary grows by appending: regeneration keeps the committed order and appends new tokens sorted; a token no card uses any more keeps its column with 0 carriers.",
    "Tokens marked by the mapping layer replace a card-named leaf with the shared tokens of the generic leaf of the same rules.",
];

/// The committed table text: JSON with one vocabulary entry and one card
/// per line, and a content hash over every other line.
pub fn render(table: &Table) -> String {
    const { assert!(VALUE_LIMIT > 0) };
    let mut body = String::new();
    body.push_str("{\n");
    let field = |body: &mut String, key: &str, value: String| {
        writeln!(body, "  {}: {value},", serde_json::to_string(key).unwrap()).unwrap();
    };
    field(&mut body, "schema", json(&TABLE_SCHEMA));
    field(&mut body, "pool_path", json(&table.pool_path));
    field(&mut body, "pool_sha256", json(&table.pool_sha256.as_str()));
    field(
        &mut body,
        "kernel_carddb_hash",
        json(&format!("{KERNEL_CARDDB_HASH:016x}").as_str()),
    );
    field(&mut body, "mapping_path", json(&MAPPING_PATH));
    field(
        &mut body,
        "mapping_sha256",
        json(&table.mapping_sha256.as_str()),
    );
    if let Some(pauper) = &table.pauper_vocabulary {
        field(&mut body, "pauper_table_path", json(&pauper.path));
        field(
            &mut body,
            "pauper_vocabulary_sha256",
            json(&pauper.sha256.as_str()),
        );
        field(&mut body, "pauper_vocabulary_len", pauper.len.to_string());
    }
    field(
        &mut body,
        "generator",
        json(&"mtg-kernel/src/bin/card_semantics_table_v1/main.rs"),
    );
    body.push_str("  \"encoding\": [\n");
    for (i, line) in ENCODING.iter().enumerate() {
        let comma = if i + 1 < ENCODING.len() { "," } else { "" };
        writeln!(body, "    {}{comma}", serde_json::to_string(line).unwrap()).unwrap();
    }
    body.push_str("  ],\n");
    field(
        &mut body,
        "vector_width",
        table.vocabulary.len().to_string(),
    );
    field(&mut body, "card_count", table.rows.len().to_string());
    body.push_str("  \"vocabulary\": [\n");
    for (i, entry) in table.vocabulary.iter().enumerate() {
        let comma = if i + 1 < table.vocabulary.len() {
            ","
        } else {
            ""
        };
        writeln!(body, "    {}{comma}", serde_json::to_string(entry).unwrap()).unwrap();
    }
    body.push_str("  ],\n");
    body.push_str("  \"cards\": [\n");
    for (i, (row, vector)) in table.rows.iter().zip(&table.vectors).enumerate() {
        let comma = if i + 1 < table.rows.len() { "," } else { "" };
        let line = serde_json::json!({"name": row.name, "card_id": row.id, "vector": vector});
        writeln!(body, "    {}{comma}", serde_json::to_string(&line).unwrap()).unwrap();
    }
    body.push_str("  ],\n");
    let hash = sha256_hex(body.as_bytes());
    let mut text = body;
    writeln!(text, "  {CONTENT_HASH_KEY}\"{hash}\"").unwrap();
    text.push_str("}\n");
    text
}

/// Verifies the content hash line of a rendered table.
pub fn content_hash_holds(text: &str) -> bool {
    let Some(at) = text.find(&format!("  {CONTENT_HASH_KEY}")) else {
        return false;
    };
    let body = &text[..at];
    let rest = &text[at + 2 + CONTENT_HASH_KEY.len()..];
    let recorded = rest.trim_start_matches('"').get(..64).unwrap_or("");
    sha256_hex(body.as_bytes()) == recorded
}

/// Reads and validates the vocabulary (token, kind) of a committed table.
pub fn try_committed_vocabulary(text: &str) -> Result<Vec<(String, String)>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("committed table JSON: {e}"))?;
    let entries = value["vocabulary"]
        .as_array()
        .ok_or_else(|| "committed table JSON: missing vocabulary array".to_string())?;
    let mut out = Vec::with_capacity(entries.len());
    let mut seen = BTreeSet::new();
    for (i, entry) in entries.iter().enumerate() {
        let index = entry["index"]
            .as_u64()
            .ok_or_else(|| format!("vocabulary entry {i}: missing integer index"))?;
        if index != i as u64 {
            return Err(format!(
                "vocabulary entry {i}: index {index} does not match position {i}"
            ));
        }
        let token = entry["token"]
            .as_str()
            .ok_or_else(|| format!("vocabulary entry {i}: missing token"))?;
        let kind = entry["kind"]
            .as_str()
            .ok_or_else(|| format!("vocabulary entry {i}: missing kind"))?;
        if kind != "count" && kind != "value" {
            return Err(format!("vocabulary entry {i}: unknown kind {kind:?}"));
        }
        if !seen.insert(token) {
            return Err(format!("vocabulary entry {i}: duplicate token {token:?}"));
        }
        out.push((token.to_string(), kind.to_string()));
    }
    Ok(out)
}

pub fn format_amount(acc: &super::extract::Acc) -> String {
    match acc.kind {
        TokenKind::Count if acc.count == 1 => String::new(),
        TokenKind::Count => format!(" x{}", acc.count),
        TokenKind::Value => format!(" = {}", acc.value),
    }
}

/// The human-readable dump committed beside the Pauper table.
pub fn render_dump(table: &Table, census_text: &str) -> String {
    render_dump_with_title(table, "Pauper pool", census_text)
}

/// The human-readable dump committed beside the Vintage Cube table.
pub fn render_cube_dump(table: &Table, census_text: &str) -> String {
    render_dump_with_title(table, "Vintage Cube pool", census_text)
}

fn render_dump_with_title(table: &Table, pool_title: &str, census_text: &str) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "Card semantics table v1, {pool_title}: human-readable dump"
    )
    .unwrap();
    writeln!(
        out,
        "Generated by mtg-kernel/src/bin/card_semantics_table_v1/main.rs; do not edit."
    )
    .unwrap();
    writeln!(out, "kernel_carddb_hash {KERNEL_CARDDB_HASH:016x}").unwrap();
    writeln!(out, "pool_sha256 {}", table.pool_sha256).unwrap();
    writeln!(out, "mapping_sha256 {}", table.mapping_sha256).unwrap();
    if let Some(pauper) = &table.pauper_vocabulary {
        writeln!(out, "pauper_vocabulary_sha256 {}", pauper.sha256).unwrap();
    }
    writeln!(out, "vector_width {}", table.vocabulary.len()).unwrap();
    writeln!(
        out,
        "Tokens marked [M] came only through the mapping layer, [M+] through it and directly."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "== Census").unwrap();
    out.push_str(census_text);
    writeln!(out).unwrap();
    writeln!(out, "== Cards").unwrap();
    for row in &table.rows {
        writeln!(out).unwrap();
        writeln!(out, "{} (card id {})", row.name, row.id).unwrap();
        if !row.tokens.mapped_sources.is_empty() {
            let sources: Vec<&str> = row
                .tokens
                .mapped_sources
                .iter()
                .map(String::as_str)
                .collect();
            writeln!(out, "  mapped: {}", sources.join(", ")).unwrap();
        }
        for (token, acc) in &row.tokens.tokens {
            let mark = match (acc.via_mapping, acc.direct) {
                (true, false) => " [M]",
                (true, true) => " [M+]",
                _ => "",
            };
            writeln!(out, "  {token}{}{mark}", format_amount(acc)).unwrap();
        }
    }
    writeln!(out).unwrap();
    writeln!(out, "== Vocabulary (index, kind, cards carrying it, token)").unwrap();
    for entry in &table.vocabulary {
        writeln!(
            out,
            "{:4} {:5} {:3} {}",
            entry.index, entry.kind, entry.carriers, entry.token
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "== Token families").unwrap();
    for (prefix, meaning) in FAMILIES.iter().filter(|(p, _)| !p.is_empty()) {
        writeln!(out, "{prefix:12} {meaning}").unwrap();
    }
    out
}

fn json<T: Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).expect("serializable")
}
