//! Rules vector v1: a fixed per-card feature vector derived only from the
//! engine's own card programs.
//!
//! Every input is a rules fact the engine executes (zones, players, event
//! kinds, costs, targets). Card names, registry labels (`mechanics`,
//! `complexity`, `java_file`), recipe strings and enum variant identifiers
//! are never features. See `docs/rules_vector_v1.md` for the design, its
//! review and the pre-registered audit checks.
//!
//! Engineering only: nothing here is read by a model, trainer or search.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub mod chain;
pub(crate) mod facets;
pub(crate) mod meaning;
pub mod report;
pub mod sources;
pub mod vectorize;

#[cfg(test)]
mod tests;

use crate::card_def::CARD_DEFS;
use sources::PAUPER_REGISTRY_LEN_V1;
use vectorize::FeatureCounts;

pub const RULES_VECTOR_SCHEMA_V1: &str = "mtg-kernel/rules-vector/v1";

/// The extractor's own source files, hashed into every output so a table
/// always names the exact meaning tables that produced it.
const EXTRACTOR_SOURCES_V1: &[(&str, &str)] = &[
    ("rules_vector_v1.rs", include_str!("rules_vector_v1.rs")),
    (
        "rules_vector_v1/facets.rs",
        include_str!("rules_vector_v1/facets.rs"),
    ),
    (
        "rules_vector_v1/sources.rs",
        include_str!("rules_vector_v1/sources.rs"),
    ),
    (
        "rules_vector_v1/vectorize.rs",
        include_str!("rules_vector_v1/vectorize.rs"),
    ),
    (
        "rules_vector_v1/chain.rs",
        include_str!("rules_vector_v1/chain.rs"),
    ),
    (
        "rules_vector_v1/meaning/mod.rs",
        include_str!("rules_vector_v1/meaning/mod.rs"),
    ),
    (
        "rules_vector_v1/meaning/effect_a.rs",
        include_str!("rules_vector_v1/meaning/effect_a.rs"),
    ),
    (
        "rules_vector_v1/meaning/effect_b.rs",
        include_str!("rules_vector_v1/meaning/effect_b.rs"),
    ),
    (
        "rules_vector_v1/meaning/effect_c.rs",
        include_str!("rules_vector_v1/meaning/effect_c.rs"),
    ),
    (
        "rules_vector_v1/meaning/effect_d.rs",
        include_str!("rules_vector_v1/meaning/effect_d.rs"),
    ),
    (
        "rules_vector_v1/meaning/effect_e.rs",
        include_str!("rules_vector_v1/meaning/effect_e.rs"),
    ),
    (
        "rules_vector_v1/meaning/effect_f.rs",
        include_str!("rules_vector_v1/meaning/effect_f.rs"),
    ),
    (
        "rules_vector_v1/meaning/reads.rs",
        include_str!("rules_vector_v1/meaning/reads.rs"),
    ),
    (
        "rules_vector_v1/meaning/targets.rs",
        include_str!("rules_vector_v1/meaning/targets.rs"),
    ),
    (
        "rules_vector_v1/meaning/triggers_costs.rs",
        include_str!("rules_vector_v1/meaning/triggers_costs.rs"),
    ),
];

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// sha256 over every extractor source file, in a fixed order.
pub fn extractor_sha256() -> String {
    let mut hasher = Sha256::new();
    for (path, text) in EXTRACTOR_SOURCES_V1 {
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update(text.as_bytes());
        hasher.update([0u8]);
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// One card's row: sparse feature counts keyed by feature string.
#[derive(Debug, Clone, Serialize)]
pub struct CardRowV1 {
    pub id: u16,
    pub features: FeatureCounts,
    /// sha256 of the card's canonical rules record (no name).
    pub record_sha256: String,
}

/// The full table for the current build.
#[derive(Debug, Clone, Serialize)]
pub struct RulesTableV1 {
    pub schema: &'static str,
    /// `pauper` or `pauper+fdn`.
    pub build: &'static str,
    pub extractor_sha256: String,
    pub kernel_carddb_hash: String,
    /// Features of the Pauper registry; the frozen v1 vocabulary.
    pub vocabulary: Vec<String>,
    pub cards: Vec<CardRowV1>,
}

pub fn build_name() -> &'static str {
    if cfg!(feature = "limited-fdn-fixtures") {
        "pauper+fdn"
    } else {
        "pauper"
    }
}

/// Extracts every card in `CARD_DEFS`.
pub fn build_table() -> RulesTableV1 {
    let cards: Vec<CardRowV1> = (0..CARD_DEFS.len())
        .map(|id| {
            let id = id as u16;
            let rules = sources::card_rules(id);
            let record = serde_json::to_vec(&rules.record).expect("record serializes");
            CardRowV1 {
                id,
                features: vectorize::card_feature_counts(id),
                record_sha256: sha256_hex(&record),
            }
        })
        .collect();
    let mut vocabulary = BTreeSet::new();
    for row in cards.iter().take(PAUPER_REGISTRY_LEN_V1) {
        vocabulary.extend(row.features.keys().cloned());
    }
    RulesTableV1 {
        schema: RULES_VECTOR_SCHEMA_V1,
        build: build_name(),
        extractor_sha256: extractor_sha256(),
        kernel_carddb_hash: format!("{:#018x}", crate::card_def::KERNEL_CARDDB_HASH),
        vocabulary: vocabulary.into_iter().collect(),
        cards,
    }
}

impl RulesTableV1 {
    /// Canonical bytes: compact JSON with sorted maps (serde keeps BTreeMap order).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("table serializes")
    }

    pub fn output_sha256(&self) -> String {
        sha256_hex(&self.canonical_bytes())
    }
}

/// Deck membership for the nine runtime decks: deck id -> unique card ids.
pub fn runtime_deck_cards() -> BTreeMap<&'static str, BTreeSet<u16>> {
    crate::runtime_decks::RUNTIME_DECKS
        .iter()
        .map(|deck| (deck.id, deck.card_ids.iter().copied().collect()))
        .collect()
}

/// Cards with identical feature counts but different rules records: the
/// extractor lost a distinction the engine makes.
pub fn lossy_collisions(table: &RulesTableV1, ids: &BTreeSet<u16>) -> Vec<Vec<u16>> {
    let mut by_features: BTreeMap<&FeatureCounts, Vec<&CardRowV1>> = BTreeMap::new();
    for row in &table.cards {
        if ids.contains(&row.id) {
            by_features.entry(&row.features).or_default().push(row);
        }
    }
    by_features
        .into_values()
        .filter(|rows| {
            rows.iter()
                .map(|r| &r.record_sha256)
                .collect::<BTreeSet<_>>()
                .len()
                > 1
        })
        .map(|rows| rows.iter().map(|r| r.id).collect())
        .collect()
}

/// Groups of cards with identical rules records (functional reprints).
pub fn reprint_classes(table: &RulesTableV1, ids: &BTreeSet<u16>) -> Vec<Vec<u16>> {
    let mut by_record: BTreeMap<&str, Vec<u16>> = BTreeMap::new();
    for row in &table.cards {
        if ids.contains(&row.id) {
            by_record
                .entry(&row.record_sha256)
                .or_default()
                .push(row.id);
        }
    }
    by_record
        .into_values()
        .filter(|ids| ids.len() > 1)
        .collect()
}

/// Share of non-token cards outside the Pauper registry with at least one
/// feature the Pauper vocabulary lacks.
pub fn oov_card_share(table: &RulesTableV1) -> (usize, usize) {
    let vocab: BTreeSet<&String> = table.vocabulary.iter().collect();
    let mut with_oov = 0;
    let mut total = 0;
    for row in table.cards.iter().skip(PAUPER_REGISTRY_LEN_V1) {
        if CARD_DEFS[usize::from(row.id)].is_token {
            continue;
        }
        total += 1;
        if row.features.keys().any(|k| !vocab.contains(k)) {
            with_oov += 1;
        }
    }
    (with_oov, total)
}

/// Share of Pauper non-token cards that hold at least one feature no other
/// card has (a card-specific part, which transfers like an ID).
pub fn unique_part_share(table: &RulesTableV1) -> (usize, usize) {
    let mut holders: BTreeMap<&String, usize> = BTreeMap::new();
    let rows: Vec<&CardRowV1> = table
        .cards
        .iter()
        .take(PAUPER_REGISTRY_LEN_V1)
        .filter(|r| !CARD_DEFS[usize::from(r.id)].is_token)
        .collect();
    for row in &rows {
        for key in row.features.keys() {
            *holders.entry(key).or_insert(0) += 1;
        }
    }
    let unique = rows
        .iter()
        .filter(|r| r.features.keys().any(|k| holders[k] == 1))
        .count();
    (unique, rows.len())
}
