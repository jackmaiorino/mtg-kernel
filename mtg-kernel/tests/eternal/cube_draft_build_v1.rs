//! M4 task D1 on the real cube registry and the provisional table pin: the
//! full index is refused for its cards without a row; the dense columns
//! equal the golden list; the land tests (produced colours, fetch,
//! completeness); the provisional index and pool; the same bits on a second
//! load.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::CARD_DEFS;
use mtg_kernel::cube_draft_build_v1::card_index::{
    is_land_v1, land_produces_colourless_v1, produced_colour_token_v1, provisional_pool_v1,
    CardIndexErrorV1, CardIndexV1, CubeTableSourceV1, CubeTableV1, DENSE_EXACT_TOKENS_V1,
    FETCH_SUBTYPE_PREFIX_V1,
};
use mtg_kernel::cube_draft_v1::{
    basic_land_ids, land_fetch_bits, land_mana_bits, CubePool, BASIC_LAND_NAMES, COLOUR_ORDER,
    DRAFTED_CARDS,
};
use mtg_kernel::mana::ManaColor;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const CUBE_LIST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007.json"
);
const GAP_RECORD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"
);
/// The provisional table pin: the committed cube table. D0 confirms or
/// replaces this path and digest.
const TABLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/card_semantics_v1/cube_table_v1.json"
);
const TABLE_SHA256: &str = "fc22ac488e9d67580889be8d0c4c30690f1e68f4f227a44de08887da757493ae";

/// Rows of the provisional index at the pinned table (504 cube cards plus 5
/// basics), and cube cards left out for lack of a row.
const PROVISIONAL_ROWS: usize = 509;
const LEFT_OUT: usize = 36;
/// Cube-list cards plus the 5 basics.
const FULL_ROWS: usize = 545;
/// The least provisional pool: 517 fenced cards minus at most 36.
const MIN_PROVISIONAL_POOL: usize = 481;

/// The dense columns the rule selects at the pinned table, in order.
const GOLDEN_DENSE: [usize; 56] = [
    6, 12, 13, 14, 15, 16, 17, 33, 34, 35, 36, 37, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 97, 98,
    108, 114, 117, 118, 369, 370, 371, 390, 399, 405, 413, 425, 435, 436, 482, 483, 484, 485, 486,
    487, 500, 501, 502, 503, 504, 505, 506, 824, 900, 932, 933, 938,
];

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
    CubeTableSourceV1::read_v1(Path::new(TABLE), TABLE_SHA256).expect("pinned cube table")
}

fn provisional() -> (CardIndexV1, Vec<u16>) {
    CardIndexV1::provisional_v1(&cube_text(), &source()).expect("provisional index")
}

fn provisional_table() -> CubeTableV1 {
    let (index, _) = provisional();
    CubeTableV1::load_v1(Path::new(TABLE), TABLE_SHA256, &index).expect("table on the index")
}

fn name(id: u16) -> &'static str {
    CARD_DEFS[usize::from(id)].name
}

fn column(table: &CubeTableV1, token: &str) -> usize {
    table
        .column_of(token)
        .unwrap_or_else(|| panic!("the pinned table has no {token} column"))
}

/// A land's id and its six produced-colour columns as f32 bits.
type LandColumnsV1 = (u16, [u32; 6]);

/// The six produced-colour columns of a row, as f32 bits.
fn produced_bits(table: &CubeTableV1, row: usize, columns: &[usize; 6]) -> [u32; 6] {
    columns.map(|j| table.scaled_row(row)[j].to_bits())
}

// Test 1, on the real table: the full index is refused, naming every cube
// card without a row.
#[test]
fn full_index_is_refused_for_its_cards_without_a_row() {
    let full = CardIndexV1::full_v1(&cube_text()).expect("full index");
    assert_eq!(full.len(), FULL_ROWS);
    let (_, left_out) = provisional();
    let error = CubeTableV1::load_v1(Path::new(TABLE), TABLE_SHA256, &full).unwrap_err();
    let CardIndexErrorV1::MissingCards(missing) = &error else {
        panic!("expected the missing-card refusal, got: {error}");
    };
    let missing_ids: Vec<u16> = missing.iter().map(|card| card.card_id).collect();
    assert_eq!(missing.len(), LEFT_OUT, "{error}");
    assert_eq!(missing_ids, left_out);
    for card in missing {
        assert_eq!(card.name, name(card.card_id));
    }
}

// Test 3: the dense columns at the pinned table equal the golden list.
#[test]
fn dense_columns_equal_the_golden_list() {
    let table = provisional_table();
    let columns: Vec<usize> = table.dense_columns().iter().map(|c| c.index).collect();
    assert_eq!(columns, GOLDEN_DENSE);
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

// Land test 1: for every index land, each produced-colour column is non-zero
// exactly when the engine says the land produces that colour; and two lands
// that produce the same colours have equal produced-colour columns.
#[test]
fn land_produced_colours_match_the_engine() {
    let table = provisional_table();
    let columns = PRODUCED.map(|color| column(&table, produced_colour_token_v1(color)));
    let mut mismatches: Vec<String> = Vec::new();
    let mut groups: BTreeMap<(u8, bool), Vec<LandColumnsV1>> = BTreeMap::new();
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
            let in_table = values[columns[k]] != 0.0;
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
        groups
            .entry((bits, colourless))
            .or_default()
            .push((id, produced_bits(&table, row, &columns)));
    }
    assert!(lands > 5, "only {lands} lands in the index");
    assert!(
        mismatches.is_empty(),
        "{} produced-colour mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    // The pairwise form, asserted directly.
    let mut pair_mismatches: Vec<String> = Vec::new();
    for ((bits, colourless), members) in &groups {
        let (first, first_values) = members[0];
        for &(id, values) in &members[1..] {
            if values != first_values {
                pair_mismatches.push(format!(
                    "bits {bits:05b} colourless {colourless}: {} ({first}) {first_values:?} \
                     differs from {} ({id}) {values:?}",
                    name(first),
                    name(id)
                ));
            }
        }
    }
    assert!(pair_mismatches.is_empty(), "{}", pair_mismatches.join("\n"));
    // Volcanic Island and Steam Vents: blue and red, nothing else.
    let mut pair = Vec::new();
    for land in ["Volcanic Island", "Steam Vents"] {
        let id = mtg_kernel::card_def::card_id_by_name(land).expect(land);
        let row = table.index().row_of(id).expect(land);
        assert_eq!(land_mana_bits(id), 0b0_1010, "{land}");
        let values = table.scaled_row(row);
        let nonzero: Vec<bool> = columns.iter().map(|&j| values[j] != 0.0).collect();
        assert_eq!(nonzero, [false, true, false, true, false, false], "{land}");
        pair.push(produced_bits(&table, row, &columns));
    }
    assert_eq!(pair[0], pair[1]);
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
    assert!(CubeTableV1::load_v1(Path::new(TABLE), TABLE_SHA256, &index).is_ok());
}

// Test 7: the provisional index and pool at the pinned table.
#[test]
fn provisional_index_and_pool() {
    let table_source = source();
    let (index, left_out) = provisional();
    assert_eq!(index.len(), PROVISIONAL_ROWS);
    assert!(index.ids().windows(2).all(|pair| pair[0] < pair[1]));
    for basic in basic_land_ids().unwrap() {
        assert!(index.contains(basic), "{}", name(basic));
    }
    assert_eq!(left_out.len(), LEFT_OUT);
    assert!(left_out.windows(2).all(|pair| pair[0] < pair[1]));
    for &id in &left_out {
        assert!(!table_source.has_row(id), "{} has a row", name(id));
        assert!(!index.contains(id), "{} is in the index", name(id));
    }
    // Each row names the registry card of its id.
    for id in table_source.row_ids() {
        assert_eq!(table_source.row_name(id), Some(name(id)), "card {id}");
    }
    // This fixture covers the gap-record pool.
    let pool = CubePool::from_json_texts(
        &cube_text(),
        &std::fs::read_to_string(GAP_RECORD).expect("gap record"),
        None,
    )
    .expect("draftable pool");
    let (kept, removed) = provisional_pool_v1(&pool, &index).expect("provisional pool");
    assert!(kept.len() >= MIN_PROVISIONAL_POOL, "{}", kept.len());
    assert!(kept.len() >= DRAFTED_CARDS);
    assert_eq!(kept.len() + removed.len(), pool.card_count());
    assert!(kept.iter().all(|&id| index.contains(id)));
    assert!(removed.iter().all(|&id| !index.contains(id)));
    let left_out: BTreeSet<u16> = left_out.into_iter().collect();
    assert!(removed.iter().all(|id| left_out.contains(id)));
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
