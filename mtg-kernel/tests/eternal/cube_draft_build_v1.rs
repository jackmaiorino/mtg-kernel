//! M4 task D1 tests on the real cube registry and pinned provisional table.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::CARD_DEFS;
use mtg_kernel::cube_draft_build_v1::card_index::{
    is_land_v1, land_produces_colourless_v1, produced_colour_token_v1, provisional_pool_v1,
    CardIndexErrorV1, CardIndexV1, CubeTableSourceV1, CubeTableV1, DENSE_EXACT_TOKENS_V1,
    FETCH_SUBTYPE_PREFIX_V1, PROVISIONAL_TABLE_SHA256_V1,
};
use mtg_kernel::cube_draft_v1::{
    basic_land_ids, land_fetch_bits, land_mana_bits, CubePool, BASIC_LAND_NAMES, COLOUR_ORDER,
    DRAFTED_CARDS,
};
use mtg_kernel::mana::ManaColor;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

const CUBE_LIST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007.json"
);
const GAP_RECORD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"
);
/// The regenerated provisional cube table pinned by D0 revision 9.
const TABLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/card_semantics_v1/cube_table_v1.json"
);
const INDEX_SHA256: &str = "fb2f6fa80f612fb100be983b1c2a4947f8c3add61aa2d81b187d8e49ea526b63";
const PROVISIONAL_ROWS: usize = 545;
const DENSE_FIELDS_AND_SCALES: &[u8] =
    include_bytes!("../data/cube_draft_build_v1/dense_fields_and_scales_v1.json");
const DENSE_FIELDS_AND_SCALES_SHA256: &str =
    "36a79a1919f4f15f4774409b8b0acbdaea5903187a5d4475d604530592deb5da";

/// The produced-colour columns: W, U, B, R, G ([`COLOUR_ORDER`]) then C.
const PRODUCED: [ManaColor; 6] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
    ManaColor::C,
];

fn cube_text() -> String {
    std::fs::read_to_string(CUBE_LIST).expect("cube list")
}

fn source() -> CubeTableSourceV1 {
    CubeTableSourceV1::read_v1(Path::new(TABLE), PROVISIONAL_TABLE_SHA256_V1)
        .expect("pinned cube table")
}

fn provisional() -> (CardIndexV1, Vec<u16>) {
    CardIndexV1::provisional_v1(&cube_text(), &source()).expect("provisional index")
}

fn provisional_table() -> CubeTableV1 {
    let (index, _) = provisional();
    CubeTableV1::load_v1(Path::new(TABLE), PROVISIONAL_TABLE_SHA256_V1, &index)
        .expect("table on the index")
}

fn name(id: u16) -> &'static str {
    CARD_DEFS[usize::from(id)].name
}

fn column(table: &CubeTableV1, token: &str) -> usize {
    table
        .column_of(token)
        .unwrap_or_else(|| panic!("the pinned table has no {token} column"))
}

#[test]
fn full_index_refuses_a_synthetic_missing_row() {
    let full = CardIndexV1::full_v1(&cube_text()).expect("full index");
    let mut fixture: serde_json::Value =
        serde_json::from_slice(&std::fs::read(TABLE).expect("pinned cube table")).unwrap();
    let cards = fixture["cards"].as_array_mut().unwrap();
    let removed = cards.pop().unwrap();
    fixture["card_count"] = cards.len().into();
    let bytes = serde_json::to_vec(&fixture).unwrap();
    let incomplete =
        CubeTableSourceV1::from_bytes_v1(&bytes, &format!("{:x}", Sha256::digest(&bytes))).unwrap();

    let error = incomplete.bind_v1(&full).unwrap_err();
    let CardIndexErrorV1::MissingCards(missing) = &error else {
        panic!("expected the missing-card refusal, got: {error}");
    };
    let id = u16::try_from(removed["card_id"].as_u64().unwrap()).unwrap();
    assert_eq!(missing.len(), 1, "{error}");
    assert_eq!(missing[0].card_id, id);
    assert_eq!(missing[0].name, name(id));
}

// Test 3: the dense columns and divisors match the pin.
#[test]
fn dense_columns_and_divisors_match_the_pin() {
    let table = provisional_table();
    let columns: Vec<usize> = table.dense_columns().iter().map(|c| c.index).collect();
    let pin: serde_json::Value = serde_json::from_slice(DENSE_FIELDS_AND_SCALES).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(DENSE_FIELDS_AND_SCALES)),
        DENSE_FIELDS_AND_SCALES_SHA256
    );
    assert_eq!(serde_json::json!(columns), pin["dense_columns"]);
    let divisors: Vec<u32> = table.divisors().iter().map(|&value| value as u32).collect();
    assert_eq!(serde_json::json!(divisors), pin["column_divisors"]);
    let token_at = |j: usize| table.tokens()[j].as_str();
    for (j, token) in [
        (12, "ability:mana.color=B"),
        (13, "ability:mana.color=C"),
        (14, "ability:mana.color=G"),
        (15, "ability:mana.color=R"),
        (16, "ability:mana.color=U"),
        (17, "ability:mana.color=W"),
        (390, "subtype:Forest"),
        (399, "subtype:Island"),
        (405, "subtype:Mountain"),
        (413, "subtype:Plains"),
        (425, "subtype:Swamp"),
    ] {
        assert_eq!(token_at(j), token);
    }
    for c in table.dense_columns() {
        assert_eq!(c.token, token_at(c.index));
    }
    assert_eq!(table.dense_rows().len(), table.index().len() * 56);
    assert!(!table
        .tokens()
        .iter()
        .any(|t| t.starts_with(FETCH_SUBTYPE_PREFIX_V1)));
}

// Land test 1: the presence of each produced colour matches the engine.
// Red and green can appear in either the ability column or an auxiliary flag.
#[test]
fn land_produced_colours_match_the_engine() {
    let table = provisional_table();
    let columns = PRODUCED.map(|color| column(&table, produced_colour_token_v1(color)));
    let mut mismatches: Vec<String> = Vec::new();
    let auxiliary = PRODUCED.map(|color| match color {
        ManaColor::G => Some(column(&table, "flag:produces_mana=G")),
        ManaColor::R => Some(column(&table, "flag:produces_mana=R")),
        _ => None,
    });
    let produced = |values: &[f32], k: usize| {
        values[columns[k]] != 0.0 || auxiliary[k].is_some_and(|j| values[j] != 0.0)
    };
    let mut lands = 0usize;
    for (row, &id) in table.index().ids().iter().enumerate() {
        if !is_land_v1(id) {
            continue;
        }
        lands += 1;
        let bits = land_mana_bits(id);
        let colourless = land_produces_colourless_v1(id);
        let values = table.scaled_row(row);
        for (k, color) in COLOUR_ORDER.iter().enumerate() {
            let engine = bits & (1 << k) != 0;
            let in_table = produced(values, k);
            if engine != in_table {
                mismatches.push(format!(
                    "{} ({id}): {color:?} engine {engine}, table {in_table}",
                    name(id)
                ));
            }
        }
        let in_table = values[columns[5]] != 0.0;
        if colourless != in_table {
            mismatches.push(format!(
                "{} ({id}): C engine {colourless}, table {in_table}",
                name(id)
            ));
        }
    }
    assert!(lands > 5, "only {lands} lands in the index");
    assert!(
        mismatches.is_empty(),
        "{} produced-colour mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    // Volcanic Island and Steam Vents: blue and red, nothing else.
    for land in ["Volcanic Island", "Steam Vents"] {
        let id = mtg_kernel::card_def::card_id_by_name(land).expect(land);
        let row = table.index().row_of(id).expect(land);
        assert_eq!(land_mana_bits(id), 0b0_1010, "{land}");
        let values = table.scaled_row(row);
        let nonzero: [bool; 6] = std::array::from_fn(|k| produced(values, k));
        assert_eq!(nonzero, [false, true, false, true, false, false], "{land}");
    }
}

// Land test 2: the fetch family equals the engine's fetch colours.
#[test]
#[ignore = "waits for the fetch family in the regenerated table (D15)"]
fn land_fetch_columns_match_the_engine() {
    let table = provisional_table();
    let family = table
        .tokens()
        .iter()
        .filter(|t| t.starts_with(FETCH_SUBTYPE_PREFIX_V1))
        .count();
    assert_eq!(family, 5, "the fetch family must have 5 columns");
    let fetch: Vec<usize> = BASIC_LAND_NAMES
        .iter()
        .map(|basic| column(&table, &format!("{FETCH_SUBTYPE_PREFIX_V1}{basic}")))
        .collect();
    let mut mismatches: Vec<String> = Vec::new();
    for (row, &id) in table.index().ids().iter().enumerate() {
        if !is_land_v1(id) {
            continue;
        }
        let bits = land_fetch_bits(id);
        let values = table.scaled_row(row);
        for (k, basic) in BASIC_LAND_NAMES.iter().enumerate() {
            let engine = bits & (1 << k) != 0;
            let in_table = values[fetch[k]] != 0.0;
            if engine != in_table {
                mismatches.push(format!(
                    "{} ({id}): fetch {basic} engine {engine}, table {in_table}",
                    name(id)
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

// Land test 3: every card of the index has a row, and every exact dense
// token exists in the pinned table. D15 reruns this on the full index.
#[test]
fn index_and_dense_tokens_are_complete() {
    let table_source = source();
    let (index, _) = provisional();
    let without_row: Vec<u16> = index
        .ids()
        .iter()
        .copied()
        .filter(|&id| !table_source.has_row(id))
        .collect();
    assert!(without_row.is_empty(), "{without_row:?}");
    let tokens: BTreeSet<&str> = table_source.tokens().iter().map(String::as_str).collect();
    let absent: Vec<&str> = DENSE_EXACT_TOKENS_V1
        .iter()
        .copied()
        .filter(|token| !tokens.contains(token))
        .collect();
    assert!(absent.is_empty(), "{absent:?}");
    assert!(CubeTableV1::load_v1(Path::new(TABLE), PROVISIONAL_TABLE_SHA256_V1, &index).is_ok());
}

// Test 7: the provisional index and pool at the pinned table.
#[test]
fn provisional_index_and_pool() {
    let table_source = source();
    let (index, left_out) = provisional();
    assert_eq!(index.len(), PROVISIONAL_ROWS);
    assert_eq!(index.pin_sha256(), INDEX_SHA256);
    assert_eq!(index, CardIndexV1::full_v1(&cube_text()).unwrap());
    assert!(index.ids().windows(2).all(|pair| pair[0] < pair[1]));
    for basic in basic_land_ids().unwrap() {
        assert!(index.contains(basic), "{}", name(basic));
    }
    assert!(left_out.is_empty(), "{left_out:?}");
    // Each row names the registry card of its id.
    for id in table_source.row_ids() {
        assert_eq!(table_source.row_name(id), Some(name(id)), "card {id}");
    }
    // Exercise the gap-record pool without a fence.
    let pool = CubePool::from_json_texts(
        &cube_text(),
        &std::fs::read_to_string(GAP_RECORD).expect("gap record"),
        None,
    )
    .expect("draftable pool");
    let (kept, removed) = provisional_pool_v1(&pool, &index).expect("provisional pool");
    assert_eq!(kept.len(), 540);
    assert!(kept.len() >= DRAFTED_CARDS);
    assert_eq!(kept.len() + removed.len(), pool.card_count());
    assert!(kept.iter().all(|&id| index.contains(id)));
    assert!(removed.is_empty());
}

// Test 8: the same file twice gives the same bits.
#[test]
fn loading_the_pinned_table_twice_gives_the_same_bits() {
    let first = provisional_table();
    let second = provisional_table();
    let bits = |values: &[f32]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(first.scaled_rows()), bits(second.scaled_rows()));
    assert_eq!(bits(first.dense_rows()), bits(second.dense_rows()));
    assert_eq!(first.binding_sha256(), second.binding_sha256());
    assert_eq!(first.index_pin(), first.index().pin_sha256());
}
