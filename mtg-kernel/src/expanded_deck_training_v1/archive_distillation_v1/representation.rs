//! RC1's representation arms (collab
//! LANES/representation-comparison-20261018, design v3 section 4), chosen
//! by a phase-1 cell's optional `representation`:
//!
//! - `rules`: the rules pathway ([`RulesPathwayV1`]) over each card's rules
//!   vector, `rules_vector_v1`'s table as log(1 + count), L2-normalized, in
//!   binary32. The cell pins the SHA-256 of these true rows (card order,
//!   little-endian bits), which is what the network reads. The table's own
//!   output SHA-256 is recorded but not pinned: it covers each card's rules
//!   record, whose hash for one card differs between builds of the same
//!   source while every feature count, hence every row, is the same.
//! - `wrong_rules`: the same pathway, card c reading card π(c)'s row. π is
//!   the cyclic successor in SHA-256 order of `rc1-perm|{salt}|{card id}`,
//!   with the first salt from 0 under which no card reads a row equal to its
//!   own.
//! - `fresh_state_codes`: no pathway; in evidence batches only, each row's
//!   96 state hash floats are replaced at every presentation by the existing
//!   digest encoding of SHA-512 of
//!   `rc1-noise|{cell}|{game}|{decision step}|{optimizer step}|{block}`,
//!   blocks 0 to 5, on a copy. Anchor rows and every scored row keep their
//!   true inputs.
//!
//! A rules checkpoint holds the effective card table (so every loader and
//! scorer reads it unchanged) and carries, in its provenance, the pathway's
//! weight, moments and the network's own card rows; its loader rebuilds the
//! rows and requires the effective table bit for bit. Ordinary training
//! continued from a rules checkpoint optimizes the folded table as an
//! ordinary embedding: the policy is the same, but it is not a resumed rules
//! pathway (W and its moments are not carried into ordinary training).

use super::*;
use crate::native_flat_tensorizer_v2::{
    digest_block_features_v1, NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2,
    NATIVE_FLAT_STATE_FEATURE_DIM_V2,
};
use crate::native_policy_train_step_v1::{RulesPathwayStateBitsV1, RulesPathwayV1};
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use crate::native_policy_value_net_v1::CARD_EMBEDDING_DIM_V1;
use crate::rules_vector_v1::vectorize::dense_vector;
use sha2::Sha512;
use std::sync::OnceLock;

pub(super) const RULES_PATHWAY_SCHEMA_V1: &str = "mtg-kernel-archive-distillation-rules-pathway/v1";
const FRESH_CODE_BLOCKS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveDistillationRepresentationArmV1 {
    Rules,
    WrongRules,
    FreshStateCodes,
}

impl ArchiveDistillationRepresentationArmV1 {
    pub(super) fn has_pathway(self) -> bool {
        matches!(self, Self::Rules | Self::WrongRules)
    }
}

/// A cell's representation arm. The rules arms name the SHA-256 of the true
/// rules rows; the fresh-code arm names none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationRepresentationV1 {
    pub arm: ArchiveDistillationRepresentationArmV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_rows_sha256: Option<String>,
}

pub(super) fn validate_representation_v1(
    representation: &ArchiveDistillationRepresentationV1,
) -> Result<(), String> {
    match (
        representation.arm.has_pathway(),
        &representation.rules_rows_sha256,
    ) {
        (true, Some(sha)) => ensure(
            sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "a rules rows SHA-256 is 64 lowercase hex digits",
        ),
        (false, None) => Ok(()),
        _ => Err("only the rules arms name rules rows".into()),
    }
}

/// The pathway's fixed rows and their identity.
#[derive(Clone, Debug)]
pub(super) struct RulesRowsV1 {
    pub(super) rows: Vec<f32>,
    pub(super) cards: usize,
    pub(super) width: usize,
    /// The table's output SHA-256 (recorded, not pinned) and the true rows'.
    pub(super) table_sha256: String,
    pub(super) true_rows_sha256: String,
    /// The arm's rows (permuted for the wrong-association control).
    pub(super) rows_sha256: String,
    /// The wrong-association control's salt and π (card c reads π[c]).
    pub(super) salt: Option<u64>,
    pub(super) mapping: Option<Vec<u16>>,
}

impl RulesRowsV1 {
    pub(super) fn identity_v1(&self) -> Value {
        json!({"rules_table_sha256":self.table_sha256,"true_rows_sha256":self.true_rows_sha256,
            "rows_sha256":self.rows_sha256,
            "cards":self.cards,"width":self.width,"permutation_salt":self.salt,
            "mapping":self.mapping})
    }
}

fn rows_sha256_v1(rows: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(rows.len() * 4);
    for v in rows {
        bytes.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    sha(&bytes)
}

type TrueRowsV1 = (Vec<f32>, usize, usize, String, String);

/// The rules table's dense rows in card id order, their width, the table's
/// output SHA-256 and the rows' SHA-256, built once per process.
fn true_rows_v1() -> &'static Result<TrueRowsV1, String> {
    static ROWS: OnceLock<Result<TrueRowsV1, String>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let table = crate::rules_vector_v1::build_table();
        let cards = table.cards.len();
        ensure(
            cards == crate::card_def::CARD_DEFS.len()
                && table.cards.iter().enumerate().all(|(i, r)| usize::from(r.id) == i),
            "the rules table lists every card in id order",
        )?;
        let width = table.vocabulary.len();
        let mut rows = Vec::with_capacity(cards * width);
        for card in &table.cards {
            let (dense, _) = dense_vector(&card.features, &table.vocabulary);
            rows.extend(dense.into_iter().map(|v| v as f32));
        }
        let rows_sha256 = rows_sha256_v1(&rows);
        Ok((rows, cards, width, table.output_sha256(), rows_sha256))
    })
}

fn permutation_v1(salt: u64, cards: usize) -> Vec<u16> {
    let mut order: Vec<u16> = (0..cards as u16).collect();
    order.sort_by_key(|id| sha(format!("rc1-perm|{salt}|{id}").as_bytes()));
    let mut mapping = vec![0u16; cards];
    for (i, &c) in order.iter().enumerate() {
        mapping[usize::from(c)] = order[(i + 1) % cards];
    }
    mapping
}

/// The arm's rows, requiring the named table.
pub(super) fn rules_rows_v1(
    representation: &ArchiveDistillationRepresentationV1,
) -> Result<RulesRowsV1, String> {
    let (rows, cards, width, table_sha256, true_rows_sha256) =
        true_rows_v1().as_ref().map_err(Clone::clone)?;
    let (rows, cards, width) = (rows.clone(), *cards, *width);
    if representation.rules_rows_sha256.as_deref() != Some(true_rows_sha256.as_str()) {
        return Err(format!(
            "the rules rows differ from the cell's pinned rows: this build's are {true_rows_sha256}"
        ));
    }
    let row = |c: usize| &rows[c * width..(c + 1) * width];
    let (rows, salt, mapping) = match representation.arm {
        ArchiveDistillationRepresentationArmV1::Rules => (rows.clone(), None, None),
        ArchiveDistillationRepresentationArmV1::WrongRules => {
            let (salt, mapping) = (0..1024u64)
                .map(|salt| (salt, permutation_v1(salt, cards)))
                .find(|(_, m)| {
                    (0..cards).all(|c| bits(row(c)) != bits(row(usize::from(m[c]))))
                })
                .ok_or("no salt below 1024 moves every card's row")?;
            let permuted = (0..cards)
                .flat_map(|c| row(usize::from(mapping[c])).to_vec())
                .collect();
            (permuted, Some(salt), Some(mapping))
        }
        ArchiveDistillationRepresentationArmV1::FreshStateCodes => {
            return Err("the fresh-code arm has no rules rows".into())
        }
    };
    Ok(RulesRowsV1 {
        rows_sha256: rows_sha256_v1(&rows),
        rows,
        cards,
        width,
        table_sha256: table_sha256.clone(),
        true_rows_sha256: true_rows_sha256.clone(),
        salt,
        mapping,
    })
}

/// Every object's card token reads a row: none past the table.
pub(super) fn require_covered_v1<'a>(
    cards: usize,
    tensors: impl Iterator<Item = &'a NativeFlatDecisionTensorV2>,
) -> Result<Value, String> {
    let mut seen: BTreeMap<i64, u64> = BTreeMap::new();
    for tensor in tensors {
        for &token in &tensor.object_card_ids {
            ensure(
                (0..=cards as i64).contains(&token),
                "an object's card is outside the pinned rules table",
            )?;
            *seen.entry(token).or_default() += 1;
        }
    }
    let padding = seen.remove(&0).unwrap_or(0);
    Ok(json!({"distinct_card_tokens":seen.len(),"padding_objects":padding,
        "objects_by_token":seen.iter().map(|(t, n)| (t.to_string(), *n)).collect::<BTreeMap<_, _>>()}))
}

/// The evidence row's copy with fresh state codes for `optimizer_step`.
pub(super) fn fresh_state_codes_v1(
    tensor: &NativeFlatDecisionTensorV2,
    cell: &str,
    game: &str,
    decision_step: u64,
    optimizer_step: u64,
) -> Result<NativeFlatDecisionTensorV2, String> {
    ensure(
        tensor.state.len() == NATIVE_FLAT_STATE_FEATURE_DIM_V2,
        "the state vector has the contract's width",
    )?;
    let blocks: Vec<[u8; 64]> = (0..FRESH_CODE_BLOCKS)
        .map(|block| {
            Sha512::digest(
                format!("rc1-noise|{cell}|{game}|{decision_step}|{optimizer_step}|{block}")
                    .as_bytes(),
            )
            .into()
        })
        .collect();
    let mut copy = tensor.clone();
    let begin = NATIVE_FLAT_STATE_FEATURE_DIM_V2 - NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2;
    digest_block_features_v1(&blocks, &mut copy.state[begin..]);
    Ok(copy)
}

/// A rules checkpoint's pathway record, carried in its provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationRulesPathwayV1 {
    pub schema: String,
    pub arm: ArchiveDistillationRepresentationArmV1,
    pub true_rows_sha256: String,
    pub rows_sha256: String,
    pub cards: u64,
    pub width: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permutation_salt: Option<u64>,
    pub steps: u64,
    /// The network's own card rows, tokens 1 to `cards`, as binary32 bits.
    pub base_card_rows: Vec<u32>,
    /// W, row-major `[16][width]`, and its Adam moments, as binary32 bits.
    pub weight: Vec<u32>,
    pub first_moments: Vec<u32>,
    pub second_moments: Vec<u32>,
}

pub(super) fn sidecar_v1(
    arm: ArchiveDistillationRepresentationArmV1,
    rows: &RulesRowsV1,
    steps: u64,
    base: &[NativeNamedParameterV1],
    pathway: &RulesPathwayV1,
) -> ArchiveDistillationRulesPathwayV1 {
    let table = &base[0].values;
    let state = pathway.state_bits_v1();
    ArchiveDistillationRulesPathwayV1 {
        schema: RULES_PATHWAY_SCHEMA_V1.into(),
        arm,
        true_rows_sha256: rows.true_rows_sha256.clone(),
        rows_sha256: rows.rows_sha256.clone(),
        cards: rows.cards as u64,
        width: rows.width as u64,
        permutation_salt: rows.salt,
        steps,
        base_card_rows: bits(
            &table[CARD_EMBEDDING_DIM_V1..(rows.cards + 1) * CARD_EMBEDDING_DIM_V1],
        ),
        weight: state.weight,
        first_moments: state.first_moments,
        second_moments: state.second_moments,
    }
}

/// Per card token 1 to `cards`: the pathway's added embedding ‖W x_c‖ and
/// the network's own row's move from the parent ‖e_c − e_c(parent)‖,
/// summarized over the cards (mean, median, max).
pub(super) fn displacement_v1(
    pathway: &RulesPathwayV1,
    base: &[NativeNamedParameterV1],
    parent: &[NativeNamedParameterV1],
) -> Value {
    let norm = |v: &[f32]| v.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>().sqrt();
    let (table, parent_table) = (&base[0].values, &parent[0].values);
    let mut added = Vec::new();
    let mut moved = Vec::new();
    for token in 1..=pathway.cards_v1() {
        let range = token * CARD_EMBEDDING_DIM_V1..(token + 1) * CARD_EMBEDDING_DIM_V1;
        added.push(norm(&pathway.delta_v1(token)));
        let diff: Vec<f32> = table[range.clone()]
            .iter()
            .zip(&parent_table[range])
            .map(|(a, b)| a - b)
            .collect();
        moved.push(norm(&diff));
    }
    let summary = |v: &[f64]| {
        let mut s = v.to_vec();
        s.sort_by(f64::total_cmp);
        json!({"mean":s.iter().sum::<f64>() / s.len() as f64,"median":s[s.len() / 2],
            "max":s[s.len() - 1]})
    };
    json!({"cards":added.len(),"pathway_added_norm":summary(&added),
        "own_row_moved_norm":summary(&moved)})
}

/// A rules checkpoint's loader check: the sidecar matches the cell, its
/// rows rebuild to the recorded identity, and the network's card rows plus
/// the pathway give the checkpoint's card table bit for bit.
pub(super) fn validate_rules_checkpoint_v1(
    representation: &ArchiveDistillationRepresentationV1,
    sidecar: &ArchiveDistillationRulesPathwayV1,
    steps: u64,
    card_table_bits: &[u32],
) -> Result<(), String> {
    validate_representation_v1(representation)?;
    let rows = rules_rows_v1(representation)?;
    let size = CARD_EMBEDDING_DIM_V1 * rows.width;
    ensure(
        sidecar.schema == RULES_PATHWAY_SCHEMA_V1
            && sidecar.arm == representation.arm
            && sidecar.true_rows_sha256 == rows.true_rows_sha256
            && sidecar.rows_sha256 == rows.rows_sha256
            && sidecar.cards == rows.cards as u64
            && sidecar.width == rows.width as u64
            && sidecar.permutation_salt == rows.salt
            && sidecar.steps == steps
            && sidecar.base_card_rows.len() == rows.cards * CARD_EMBEDDING_DIM_V1
            && sidecar.weight.len() == size
            && sidecar.first_moments.len() == size
            && sidecar.second_moments.len() == size,
        "a rules checkpoint's sidecar differs from its cell",
    )?;
    let mut pathway = RulesPathwayV1::new_v1(rows.rows, rows.cards, rows.width).map_err(err)?;
    pathway
        .restore_v1(&RulesPathwayStateBitsV1 {
            weight: sidecar.weight.clone(),
            first_moments: sidecar.first_moments.clone(),
            second_moments: sidecar.second_moments.clone(),
        })
        .map_err(|e| format!("rules sidecar state: {e:?}"))?;
    ensure(
        card_table_bits.len() > (rows.cards + 1) * CARD_EMBEDDING_DIM_V1,
        "a rules checkpoint's card table is too short",
    )?;
    for token in 1..=rows.cards {
        let delta = pathway.delta_v1(token);
        for (k, d) in delta.iter().enumerate() {
            let mut value = f32::from_bits(sidecar.base_card_rows[(token - 1) * CARD_EMBEDDING_DIM_V1 + k]);
            if *d != 0.0 {
                value += d;
            }
            ensure(
                value.to_bits() == card_table_bits[token * CARD_EMBEDDING_DIM_V1 + k],
                "a rules checkpoint's card table is not its rows plus the pathway",
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
