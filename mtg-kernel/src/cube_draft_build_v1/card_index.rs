//! M4 task D1: the cube card index, the feature table loader and the dense
//! fields.
//!
//! * [`CardIndexV1`]: the ordered registry ids M4 knows. Row `r` of every M4
//!   per-card table is the card at position `r`. Ids are strictly ascending;
//!   ascending id carries no ranking.
//! * [`CubeTableSourceV1`]: the engine feature table
//!   (`card_semantics_table/v1`) read and checked (checks 1 to 7 of
//!   [`CubeTableV1::load_v1`]), with the per-column divisors taken over all
//!   of its rows.
//! * [`CubeTableV1`]: the source bound to an index (check 8: every index card
//!   has a row; a card without a row is refused, never given a zero row), the
//!   scaled table S, the dense fields D and the binding digest.
//!
//! Arithmetic: the table holds integers only. Each scaled value is one f32
//! division by a power of two, so it is exact and platform independent. Every
//! order is canonical (ids ascending, columns by vocabulary index) and every
//! digest is over a stated little-endian byte layout. No hash-map iteration
//! reaches any output.

use crate::card_def::{CardType, CARD_DEFS};
use crate::cube_draft_v1::{basic_land_ids, CubePool, DRAFTED_CARDS};
use crate::mana::ManaColor;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::path::Path;

/// The schema tag the loader accepts.
pub const TABLE_SCHEMA_V1: &str = "card_semantics_table/v1";
/// The largest table file the loader reads, in bytes.
pub const TABLE_FILE_CAP_V1: u64 = 64 * 1024 * 1024;
/// The largest `vector_width` the loader accepts.
pub const MAX_VECTOR_WIDTH_V1: usize = 65_536;
/// Domain tag of [`CardIndexV1::pin_sha256`].
pub const INDEX_PIN_DOMAIN_V1: &[u8] = b"m4-card-index-v1";
/// Domain tag of [`CubeTableV1::binding_sha256`].
pub const TABLE_BINDING_DOMAIN_V1: &[u8] = b"m4-cube-table-binding-v1";

/// Token prefixes of the dense fields D. A token starting with one of them
/// is in D.
pub const DENSE_PREFIXES_V1: [&str; 9] = [
    // Colour.
    "color:",
    // Printed cost.
    "cost:pip:",
    "cost:hybrid:",
    "cost:phyrexian:",
    // Types.
    "type:",
    // Supertypes.
    "supertype:",
    // Produced mana.
    "ability:mana.color=",
    "flag:produces_mana=",
    // Fetchable basic land types (the fetch family; absent before the frozen
    // table, where it adds five columns).
    FETCH_SUBTYPE_PREFIX_V1,
];

/// Exact tokens of the dense fields D.
pub const DENSE_EXACT_TOKENS_V1: [&str; 16] = [
    // Printed cost.
    "cost:generic",
    "cost:x",
    // Mana value and stats.
    "stat:mana_value",
    "stat:power",
    "stat:toughness",
    "stat:loyalty",
    // Types.
    "flag:land",
    "flag:permanent",
    // Basic land types.
    "subtype:Plains",
    "subtype:Island",
    "subtype:Swamp",
    "subtype:Mountain",
    "subtype:Forest",
    // Produced mana.
    "ability:mana",
    // Entry.
    "flag:enters_tapped",
    "flag:enters_tapped_unless",
];

/// Prefix of the fetch family: `fetch:subtype=Plains` to
/// `fetch:subtype=Forest`.
pub const FETCH_SUBTYPE_PREFIX_V1: &str = "fetch:subtype=";

/// The produced-colour token of `color` (`ability:mana.color=W` and so on).
pub fn produced_colour_token_v1(color: ManaColor) -> &'static str {
    match color {
        ManaColor::W => "ability:mana.color=W",
        ManaColor::U => "ability:mana.color=U",
        ManaColor::B => "ability:mana.color=B",
        ManaColor::R => "ability:mana.color=R",
        ManaColor::G => "ability:mana.color=G",
        ManaColor::C => "ability:mana.color=C",
    }
}

/// Whether a vocabulary token belongs to the dense fields D: it starts with
/// one of [`DENSE_PREFIXES_V1`] or equals one of [`DENSE_EXACT_TOKENS_V1`].
pub fn is_dense_token_v1(token: &str) -> bool {
    DENSE_EXACT_TOKENS_V1.contains(&token)
        || DENSE_PREFIXES_V1
            .iter()
            .any(|prefix| token.starts_with(prefix))
}

/// One dense column: its vocabulary index and token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenseColumnV1 {
    pub index: usize,
    pub token: String,
}

/// The dense columns of a vocabulary (tokens in vocabulary order): every
/// matching column, ascending.
pub fn dense_columns_for_tokens_v1(tokens: &[String]) -> Vec<DenseColumnV1> {
    tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| is_dense_token_v1(token))
        .map(|(index, token)| DenseColumnV1 {
            index,
            token: token.clone(),
        })
        .collect()
}

/// Whether a card can produce colourless mana: any of the four sources
/// [`crate::cube_draft_v1::land_mana_bits`] reads (`mana_ability_choices`,
/// `produces_mana`, `additional_mana_abilities`, `restricted_mana_abilities`)
/// includes [`ManaColor::C`].
pub fn land_produces_colourless_v1(id: u16) -> bool {
    let def = &CARD_DEFS[usize::from(id)];
    def.mana_ability_choices.contains(&ManaColor::C)
        || def.produces_mana.contains(&ManaColor::C)
        || def
            .additional_mana_abilities
            .iter()
            .any(|ability| ability.colors.contains(&ManaColor::C))
        || def
            .restricted_mana_abilities
            .iter()
            .any(|ability| ability.colors.contains(&ManaColor::C))
}

/// Whether the registry marks a card as a land.
pub fn is_land_v1(id: u16) -> bool {
    CARD_DEFS[usize::from(id)].types.contains(&CardType::Land)
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A card the index needs and the table has no row for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingCardV1 {
    pub card_id: u16,
    pub name: String,
}

/// Refusals of D1. None of them is a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardIndexErrorV1 {
    /// Check 1: the table path is not a regular file of at most 64 MiB, or
    /// it cannot be read.
    File(String),
    /// Check 2: the SHA-256 of the file bytes differs from the pin.
    Sha256Mismatch { expected: String, actual: String },
    /// The table bytes are not the expected JSON shape.
    Json(String),
    /// Check 3: the schema tag differs.
    Schema(String),
    /// Check 4: the recorded card database hash differs from the compiled
    /// one.
    CardDbHash { table: String, compiled: String },
    /// Check 5: `vector_width`, `card_count` or the vocabulary is invalid.
    Dimensions(String),
    /// Check 6: a row's width, a duplicate card id or an unknown card id.
    Row(String),
    /// Check 7: a table integer is not exactly representable in f32.
    NotExactInF32 {
        card_id: u16,
        column: usize,
        value: i32,
    },
    /// Check 8: index cards without a table row, ascending by id.
    MissingCards(Vec<MissingCardV1>),
    /// An index id list that is not strictly ascending.
    IndexNotAscending {
        position: usize,
        previous: u16,
        id: u16,
    },
    /// An index id the registry does not know.
    UnknownIndexId(u16),
    /// A cube list that is not the expected JSON.
    CubeList(String),
    /// A cube or basic land name the registry does not know.
    UnknownCardName(String),
    /// The provisional pool has fewer cards than one draft opens.
    PoolTooSmall { have: usize, need: usize },
}

impl fmt::Display for CardIndexErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CardIndexErrorV1::File(message) => write!(f, "cube table file: {message}"),
            CardIndexErrorV1::Sha256Mismatch { expected, actual } => write!(
                f,
                "cube table sha256 differs: expected {expected}, file has {actual}"
            ),
            CardIndexErrorV1::Json(message) => write!(f, "cube table JSON: {message}"),
            CardIndexErrorV1::Schema(schema) => write!(
                f,
                "cube table schema is {schema:?}, expected {TABLE_SCHEMA_V1:?}"
            ),
            CardIndexErrorV1::CardDbHash { table, compiled } => write!(
                f,
                "cube table kernel_carddb_hash {table} differs from the compiled {compiled}"
            ),
            CardIndexErrorV1::Dimensions(message) => {
                write!(f, "invalid cube table dimensions: {message}")
            }
            CardIndexErrorV1::Row(message) => write!(f, "invalid cube table row: {message}"),
            CardIndexErrorV1::NotExactInF32 {
                card_id,
                column,
                value,
            } => write!(
                f,
                "cube table integer {value} (card {card_id}, column {column}) is not exact in f32"
            ),
            CardIndexErrorV1::MissingCards(cards) => {
                write!(f, "{} index cards have no table row:", cards.len())?;
                for card in cards {
                    write!(f, " {} {:?};", card.card_id, card.name)?;
                }
                Ok(())
            }
            CardIndexErrorV1::IndexNotAscending {
                position,
                previous,
                id,
            } => write!(
                f,
                "card index is not strictly ascending: id {id} at position {position} follows {previous}"
            ),
            CardIndexErrorV1::UnknownIndexId(id) => {
                write!(f, "card index id {id} is not a registry id")
            }
            CardIndexErrorV1::CubeList(message) => write!(f, "invalid cube list: {message}"),
            CardIndexErrorV1::UnknownCardName(name) => write!(f, "unknown card {name:?}"),
            CardIndexErrorV1::PoolTooSmall { have, need } => write!(
                f,
                "provisional pool has {have} cards, a draft opens {need}"
            ),
        }
    }
}

impl std::error::Error for CardIndexErrorV1 {}

fn lowercase_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Card index
// ---------------------------------------------------------------------------

/// The ordered registry ids M4 knows: strictly ascending, no duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardIndexV1 {
    ids: Vec<u16>,
}

impl CardIndexV1 {
    /// An index from ids already in strictly ascending order. Sorts nothing:
    /// a list out of order, a duplicate or an id the registry does not know
    /// is refused.
    pub fn from_ids(ids: Vec<u16>) -> Result<Self, CardIndexErrorV1> {
        for (position, pair) in ids.windows(2).enumerate() {
            if pair[1] <= pair[0] {
                return Err(CardIndexErrorV1::IndexNotAscending {
                    position: position + 1,
                    previous: pair[0],
                    id: pair[1],
                });
            }
        }
        if let Some(&id) = ids.iter().find(|&&id| usize::from(id) >= CARD_DEFS.len()) {
            return Err(CardIndexErrorV1::UnknownIndexId(id));
        }
        Ok(Self { ids })
    }

    /// The provisional index: every card of the cube list (all names; the
    /// gap record is not applied) that has a row in `table`, plus the five
    /// basic lands, ascending. Also returns the cube cards left out for lack
    /// of a row, ascending. A cube name the registry does not know is an
    /// error.
    pub fn provisional_v1(
        cube_list_text: &str,
        table: &CubeTableSourceV1,
    ) -> Result<(Self, Vec<u16>), CardIndexErrorV1> {
        let cube = cube_list_ids_v1(cube_list_text)?;
        // The basics are always in the index (a basic without a row is then
        // refused when the table is bound), so a cube-listed basic is never
        // also left out.
        let mut ids: BTreeSet<u16> = basic_ids_v1()?.into_iter().collect();
        let mut left_out = Vec::new();
        for id in cube {
            if table.has_row(id) {
                ids.insert(id);
            } else if !ids.contains(&id) {
                left_out.push(id);
            }
        }
        Ok((Self::from_ids(ids.into_iter().collect())?, left_out))
    }

    /// The full index: every cube-list card plus the five basic lands,
    /// ascending.
    pub fn full_v1(cube_list_text: &str) -> Result<Self, CardIndexErrorV1> {
        let mut ids = cube_list_ids_v1(cube_list_text)?;
        ids.extend(basic_ids_v1()?);
        Self::from_ids(ids.into_iter().collect())
    }

    /// The ids in row order.
    pub fn ids(&self) -> &[u16] {
        &self.ids
    }

    /// The number of rows.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the index has no rows.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The row of a card id, if the index has it.
    pub fn row_of(&self, id: u16) -> Option<usize> {
        self.ids.binary_search(&id).ok()
    }

    /// Whether the index has a card id.
    pub fn contains(&self, id: u16) -> bool {
        self.row_of(id).is_some()
    }

    /// SHA-256 (lowercase hex) over the ASCII bytes `m4-card-index-v1`, one
    /// zero byte, the row count as u64 little-endian, then each id as u16
    /// little-endian in row order.
    pub fn pin_sha256(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(INDEX_PIN_DOMAIN_V1);
        hash.update([0u8]);
        hash.update((self.ids.len() as u64).to_le_bytes());
        for id in &self.ids {
            hash.update(id.to_le_bytes());
        }
        lowercase_hex(&hash.finalize())
    }
}

/// The registry ids of every name of a cube list
/// (`{"cards": [{"name": ...}, ...]}`).
fn cube_list_ids_v1(cube_list_text: &str) -> Result<BTreeSet<u16>, CardIndexErrorV1> {
    let cube: Value = serde_json::from_str(cube_list_text)
        .map_err(|error| CardIndexErrorV1::CubeList(error.to_string()))?;
    let cards = cube["cards"]
        .as_array()
        .ok_or_else(|| CardIndexErrorV1::CubeList("no cards array".to_owned()))?;
    let mut ids = BTreeSet::new();
    for card in cards {
        let name = card["name"]
            .as_str()
            .ok_or_else(|| CardIndexErrorV1::CubeList("a card without a name".to_owned()))?;
        let id = crate::card_def::card_id_by_name(name)
            .ok_or_else(|| CardIndexErrorV1::UnknownCardName(name.to_owned()))?;
        ids.insert(id);
    }
    Ok(ids)
}

fn basic_ids_v1() -> Result<[u16; 5], CardIndexErrorV1> {
    basic_land_ids().map_err(|error| CardIndexErrorV1::UnknownCardName(error.to_string()))
}

// ---------------------------------------------------------------------------
// Provisional pool
// ---------------------------------------------------------------------------

/// The provisional pool: the pool's ids minus every id not in `index`, in
/// the pool's own order, and the removed ids in the pool's order. A result
/// under [`DRAFTED_CARDS`] cards (one draft's opening) is refused.
pub fn provisional_pool_v1(
    pool: &CubePool,
    index: &CardIndexV1,
) -> Result<(Vec<u16>, Vec<u16>), CardIndexErrorV1> {
    filter_pool_ids_v1(pool.ids(), index)
}

fn filter_pool_ids_v1(
    pool_ids: &[u16],
    index: &CardIndexV1,
) -> Result<(Vec<u16>, Vec<u16>), CardIndexErrorV1> {
    let (kept, removed): (Vec<u16>, Vec<u16>) =
        pool_ids.iter().partition(|&&id| index.contains(id));
    if kept.len() < DRAFTED_CARDS {
        return Err(CardIndexErrorV1::PoolTooSmall {
            have: kept.len(),
            need: DRAFTED_CARDS,
        });
    }
    Ok((kept, removed))
}

// ---------------------------------------------------------------------------
// Table source (checks 1 to 7)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TableFileV1 {
    schema: String,
    kernel_carddb_hash: String,
    vector_width: usize,
    card_count: usize,
    vocabulary: Vec<VocabularyEntryV1>,
    cards: Vec<TableRowV1>,
}

#[derive(Deserialize)]
struct VocabularyEntryV1 {
    index: u64,
    token: String,
}

#[derive(Deserialize)]
struct TableRowV1 {
    card_id: u64,
    name: String,
    vector: Vec<i32>,
}

/// One checked table row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceRowV1 {
    name: String,
    vector: Vec<i32>,
}

/// The engine feature table, read and checked (checks 1 to 7), with its
/// column divisors. It is not yet bound to an index.
#[derive(Debug, Clone, PartialEq)]
pub struct CubeTableSourceV1 {
    file_sha256: String,
    tokens: Vec<String>,
    rows: BTreeMap<u16, SourceRowV1>,
    divisors: Vec<f32>,
}

impl CubeTableSourceV1 {
    /// Reads `path` (check 1: a regular file of at most 64 MiB, read with a
    /// cap of 64 MiB plus one byte so growth during the read is caught) and
    /// checks its bytes ([`CubeTableSourceV1::from_bytes_v1`]).
    pub fn read_v1(path: &Path, expected_sha256: &str) -> Result<Self, CardIndexErrorV1> {
        let file_error = |error: std::io::Error| CardIndexErrorV1::File(error.to_string());
        let metadata = std::fs::metadata(path).map_err(file_error)?;
        if !metadata.is_file() || metadata.len() > TABLE_FILE_CAP_V1 {
            return Err(CardIndexErrorV1::File(
                "the table must be a regular file of at most 64 MiB".to_owned(),
            ));
        }
        let file = std::fs::File::open(path).map_err(file_error)?;
        if !file.metadata().map_err(file_error)?.is_file() {
            return Err(CardIndexErrorV1::File(
                "the table must be a regular file".to_owned(),
            ));
        }
        let mut bytes = Vec::new();
        file.take(TABLE_FILE_CAP_V1 + 1)
            .read_to_end(&mut bytes)
            .map_err(file_error)?;
        if bytes.len() as u64 > TABLE_FILE_CAP_V1 {
            return Err(CardIndexErrorV1::File(
                "the table exceeds 64 MiB".to_owned(),
            ));
        }
        Self::from_bytes_v1(&bytes, expected_sha256)
    }

    /// Checks 2 to 7 on the exact file bytes, then the per-column divisors
    /// over all rows.
    pub fn from_bytes_v1(bytes: &[u8], expected_sha256: &str) -> Result<Self, CardIndexErrorV1> {
        // Check 2.
        let file_sha256 = lowercase_hex(&Sha256::digest(bytes));
        if file_sha256 != expected_sha256 {
            return Err(CardIndexErrorV1::Sha256Mismatch {
                expected: expected_sha256.to_owned(),
                actual: file_sha256,
            });
        }
        let table: TableFileV1 = serde_json::from_slice(bytes)
            .map_err(|error| CardIndexErrorV1::Json(error.to_string()))?;
        // Check 3.
        if table.schema != TABLE_SCHEMA_V1 {
            return Err(CardIndexErrorV1::Schema(table.schema));
        }
        // Check 4.
        let compiled = format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH);
        if table.kernel_carddb_hash != compiled {
            return Err(CardIndexErrorV1::CardDbHash {
                table: table.kernel_carddb_hash,
                compiled,
            });
        }
        // Check 5.
        let width = table.vector_width;
        if width == 0 || width > MAX_VECTOR_WIDTH_V1 {
            return Err(CardIndexErrorV1::Dimensions(format!(
                "vector_width {width} is outside 1..={MAX_VECTOR_WIDTH_V1}"
            )));
        }
        if table.card_count != table.cards.len() {
            return Err(CardIndexErrorV1::Dimensions(format!(
                "card_count {} differs from the {} rows",
                table.card_count,
                table.cards.len()
            )));
        }
        if table.vocabulary.len() != width {
            return Err(CardIndexErrorV1::Dimensions(format!(
                "vocabulary has {} entries, vector_width is {width}",
                table.vocabulary.len()
            )));
        }
        let mut token_set: BTreeSet<&str> = BTreeSet::new();
        for (position, entry) in table.vocabulary.iter().enumerate() {
            if entry.index != position as u64 {
                return Err(CardIndexErrorV1::Dimensions(format!(
                    "vocabulary entry {position} has index {}",
                    entry.index
                )));
            }
            if !token_set.insert(entry.token.as_str()) {
                return Err(CardIndexErrorV1::Dimensions(format!(
                    "vocabulary token {:?} is repeated",
                    entry.token
                )));
            }
        }
        let tokens: Vec<String> = table
            .vocabulary
            .into_iter()
            .map(|entry| entry.token)
            .collect();
        // Check 6.
        let mut rows: BTreeMap<u16, SourceRowV1> = BTreeMap::new();
        for row in table.cards {
            if row.vector.len() != width {
                return Err(CardIndexErrorV1::Row(format!(
                    "card {} has {} entries, vector_width is {width}",
                    row.card_id,
                    row.vector.len()
                )));
            }
            let id = u16::try_from(row.card_id)
                .ok()
                .filter(|&id| usize::from(id) < CARD_DEFS.len())
                .ok_or_else(|| {
                    CardIndexErrorV1::Row(format!("card id {} is not a registry id", row.card_id))
                })?;
            if rows.contains_key(&id) {
                return Err(CardIndexErrorV1::Row(format!(
                    "card id {id} has a duplicate row"
                )));
            }
            rows.insert(
                id,
                SourceRowV1 {
                    name: row.name,
                    vector: row.vector,
                },
            );
        }
        // Check 7.
        for (&card_id, row) in &rows {
            for (column, &value) in row.vector.iter().enumerate() {
                if (value as f32) as f64 != f64::from(value) {
                    return Err(CardIndexErrorV1::NotExactInF32 {
                        card_id,
                        column,
                        value,
                    });
                }
            }
        }
        // Divisors: per column, the next power of two of the largest
        // absolute value over all rows, with a minimum of 1.
        let mut maxima = vec![1u32; width];
        for row in rows.values() {
            for (maximum, &value) in maxima.iter_mut().zip(&row.vector) {
                *maximum = (*maximum).max(value.unsigned_abs());
            }
        }
        let divisors = maxima
            .into_iter()
            .map(|maximum| u64::from(maximum).next_power_of_two() as f32)
            .collect();
        Ok(Self {
            file_sha256,
            tokens,
            rows,
            divisors,
        })
    }

    /// The SHA-256 of the file bytes, lowercase hex.
    pub fn file_sha256(&self) -> &str {
        &self.file_sha256
    }

    /// F, the number of columns.
    pub fn vector_width(&self) -> usize {
        self.tokens.len()
    }

    /// The vocabulary tokens in column order.
    pub fn tokens(&self) -> &[String] {
        &self.tokens
    }

    /// The column of a token, if the vocabulary has it.
    pub fn column_of(&self, token: &str) -> Option<usize> {
        self.tokens.iter().position(|candidate| candidate == token)
    }

    /// The per-column divisors (powers of two), in column order.
    pub fn divisors(&self) -> &[f32] {
        &self.divisors
    }

    /// Whether the table has a row for a card id.
    pub fn has_row(&self, id: u16) -> bool {
        self.rows.contains_key(&id)
    }

    /// The card ids with a row, ascending.
    pub fn row_ids(&self) -> Vec<u16> {
        self.rows.keys().copied().collect()
    }

    /// The name a row records, if the table has the card.
    pub fn row_name(&self, id: u16) -> Option<&str> {
        self.rows.get(&id).map(|row| row.name.as_str())
    }

    /// The raw integers of a row, if the table has the card.
    pub fn row_vector(&self, id: u16) -> Option<&[i32]> {
        self.rows.get(&id).map(|row| row.vector.as_slice())
    }

    /// Binds the table to an index (check 8), scales the index rows and
    /// selects the dense fields.
    pub fn bind_v1(&self, index: &CardIndexV1) -> Result<CubeTableV1, CardIndexErrorV1> {
        // Check 8: every index card has a row. Never a zero row.
        let missing: Vec<MissingCardV1> = index
            .ids()
            .iter()
            .filter(|&&id| !self.has_row(id))
            .map(|&id| MissingCardV1 {
                card_id: id,
                name: CARD_DEFS[usize::from(id)].name.to_owned(),
            })
            .collect();
        if !missing.is_empty() {
            return Err(CardIndexErrorV1::MissingCards(missing));
        }
        let width = self.vector_width();
        let mut scaled = Vec::with_capacity(index.len() * width);
        for id in index.ids() {
            let row = &self.rows[id];
            for (&value, &divisor) in row.vector.iter().zip(&self.divisors) {
                scaled.push(value as f32 / divisor);
            }
        }
        let dense_columns = dense_columns_for_tokens_v1(&self.tokens);
        let mut dense = Vec::with_capacity(index.len() * dense_columns.len());
        for row in scaled.chunks_exact(width) {
            for column in &dense_columns {
                dense.push(row[column.index]);
            }
        }
        let index_pin = index.pin_sha256();
        let binding = binding_sha256_v1(
            &self.file_sha256,
            &index_pin,
            width,
            &dense_columns,
            &scaled,
        );
        Ok(CubeTableV1 {
            file_sha256: self.file_sha256.clone(),
            index: index.clone(),
            index_pin,
            tokens: self.tokens.clone(),
            divisors: self.divisors.clone(),
            scaled,
            dense_columns,
            dense,
            binding,
        })
    }
}

/// SHA-256 (lowercase hex) over the ASCII bytes `m4-cube-table-binding-v1`,
/// one zero byte, the 64 ASCII characters of the file digest, the 64 ASCII
/// characters of the index pin, F as u64 LE, the dense column count as u64 LE
/// then each dense column index as u32 LE, then every value of S as f32 bits
/// LE in row-major order.
fn binding_sha256_v1(
    file_sha256: &str,
    index_pin: &str,
    width: usize,
    dense_columns: &[DenseColumnV1],
    scaled: &[f32],
) -> String {
    let mut hash = Sha256::new();
    hash.update(TABLE_BINDING_DOMAIN_V1);
    hash.update([0u8]);
    hash.update(file_sha256.as_bytes());
    hash.update(index_pin.as_bytes());
    hash.update((width as u64).to_le_bytes());
    hash.update((dense_columns.len() as u64).to_le_bytes());
    for column in dense_columns {
        // Width is at most 65,536, so every column index fits u32.
        hash.update((column.index as u32).to_le_bytes());
    }
    for value in scaled {
        hash.update(value.to_bits().to_le_bytes());
    }
    lowercase_hex(&hash.finalize())
}

// ---------------------------------------------------------------------------
// Bound table (check 8, S, D, binding)
// ---------------------------------------------------------------------------

/// The feature table bound to a card index: the scaled table S
/// (`index.len()` rows by F columns, row-major), the dense fields D
/// (`index.len()` rows by the dense column count) and the binding.
#[derive(Debug, Clone, PartialEq)]
pub struct CubeTableV1 {
    file_sha256: String,
    index: CardIndexV1,
    index_pin: String,
    tokens: Vec<String>,
    divisors: Vec<f32>,
    scaled: Vec<f32>,
    dense_columns: Vec<DenseColumnV1>,
    dense: Vec<f32>,
    binding: String,
}

impl CubeTableV1 {
    /// Loads the table at `path`, pinned by `expected_sha256` (lowercase
    /// hex), against `index`. Checks, in order, each a refusal:
    ///
    /// 1. a regular file of at most 64 MiB;
    /// 2. the SHA-256 of the exact bytes equals the pin;
    /// 3. the schema is `card_semantics_table/v1`;
    /// 4. `kernel_carddb_hash` equals the compiled card database hash;
    /// 5. `vector_width` in 1..=65,536, `card_count` equals the rows, the
    ///    vocabulary has `vector_width` entries indexed 0, 1, 2 ... with
    ///    unique tokens;
    /// 6. each row has `vector_width` entries, card ids are unique registry
    ///    ids;
    /// 7. every integer is exact in f32;
    /// 8. every index card has a row ([`CardIndexErrorV1::MissingCards`]).
    pub fn load_v1(
        path: &Path,
        expected_sha256: &str,
        index: &CardIndexV1,
    ) -> Result<Self, CardIndexErrorV1> {
        CubeTableSourceV1::read_v1(path, expected_sha256)?.bind_v1(index)
    }

    /// The SHA-256 of the table file, lowercase hex.
    pub fn file_sha256(&self) -> &str {
        &self.file_sha256
    }

    /// The index the table is bound to.
    pub fn index(&self) -> &CardIndexV1 {
        &self.index
    }

    /// The index pin ([`CardIndexV1::pin_sha256`]).
    pub fn index_pin(&self) -> &str {
        &self.index_pin
    }

    /// F, the number of columns of S.
    pub fn vector_width(&self) -> usize {
        self.tokens.len()
    }

    /// The vocabulary tokens in column order.
    pub fn tokens(&self) -> &[String] {
        &self.tokens
    }

    /// The column of a token, if the vocabulary has it.
    pub fn column_of(&self, token: &str) -> Option<usize> {
        self.tokens.iter().position(|candidate| candidate == token)
    }

    /// The per-column divisors (powers of two), in column order.
    pub fn divisors(&self) -> &[f32] {
        &self.divisors
    }

    /// S, row-major: `index().len()` rows by [`CubeTableV1::vector_width`].
    pub fn scaled_rows(&self) -> &[f32] {
        &self.scaled
    }

    /// Row `row` of S.
    pub fn scaled_row(&self, row: usize) -> &[f32] {
        let width = self.vector_width();
        &self.scaled[row * width..(row + 1) * width]
    }

    /// The dense columns: vocabulary indices ascending, with their tokens.
    pub fn dense_columns(&self) -> &[DenseColumnV1] {
        &self.dense_columns
    }

    /// D, row-major: `index().len()` rows by `dense_columns().len()`, the
    /// same bits as the matching entries of S.
    pub fn dense_rows(&self) -> &[f32] {
        &self.dense
    }

    /// Row `row` of D.
    pub fn dense_row(&self, row: usize) -> &[f32] {
        let width = self.dense_columns.len();
        &self.dense[row * width..(row + 1) * width]
    }

    /// The binding digest later checkpoints record.
    pub fn binding_sha256(&self) -> &str {
        &self.binding
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    /// The index pin of `[1, 2, 3]`, recorded by the program session after
    /// the first run. UNSET: the test comparing against it fails until it is
    /// filled in.
    const RECORDED_PIN_OF_IDS_1_2_3: &str = "";

    fn compiled_hash() -> String {
        format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH)
    }

    fn sha(bytes: &[u8]) -> String {
        lowercase_hex(&Sha256::digest(bytes))
    }

    /// A table value with the given tokens and rows `(card_id, vector)`.
    fn table_value(tokens: &[&str], rows: &[(u64, Vec<i64>)]) -> Value {
        json!({
            "schema": TABLE_SCHEMA_V1,
            "pool_path": "synthetic",
            "kernel_carddb_hash": compiled_hash(),
            "vector_width": tokens.len(),
            "card_count": rows.len(),
            "vocabulary": tokens
                .iter()
                .enumerate()
                .map(|(index, token)| json!({
                    "index": index, "token": token, "kind": "count", "carriers": 1
                }))
                .collect::<Vec<_>>(),
            "cards": rows
                .iter()
                .map(|(id, vector)| json!({
                    "card_id": id, "name": format!("card {id}"), "vector": vector
                }))
                .collect::<Vec<_>>(),
        })
    }

    fn source(value: &Value) -> Result<CubeTableSourceV1, CardIndexErrorV1> {
        let bytes = serde_json::to_vec(value).unwrap();
        CubeTableSourceV1::from_bytes_v1(&bytes, &sha(&bytes))
    }

    fn index(ids: &[u16]) -> CardIndexV1 {
        CardIndexV1::from_ids(ids.to_vec()).unwrap()
    }

    /// Four columns: one with a negative value (not a dense token), one whose
    /// maximum is exactly a power of two, one all zero, one with entries of
    /// both signs. Card 3 is outside the index used below; it raises column
    /// 0's maximum.
    fn scaling_table() -> Value {
        table_value(
            &[
                "ability:activated",
                "cost:generic",
                "flag:land",
                "type:Creature",
            ],
            &[
                (1, vec![112, 16, 0, 1]),
                (2, vec![-32, 5, 0, -1]),
                (3, vec![200, 3, 0, 0]),
            ],
        )
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "m4-card-index-{label}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Writes a table file and returns its path and pin.
    fn write_table(dir: &TempDir, name: &str, value: &Value) -> (PathBuf, String) {
        let bytes = serde_json::to_vec_pretty(value).unwrap();
        let path = dir.0.join(name);
        std::fs::write(&path, &bytes).unwrap();
        (path, sha(&bytes))
    }

    // Test 1: a card without a row is refused, naming it.
    #[test]
    fn missing_card_is_refused_with_its_id_and_name() {
        let dir = TempDir::new("missing");
        let value = table_value(&["stat:power"], &[(1, vec![1]), (3, vec![2])]);
        let (path, pin) = write_table(&dir, "table.json", &value);
        let error = CubeTableV1::load_v1(&path, &pin, &index(&[1, 2, 3])).unwrap_err();
        assert_eq!(
            error,
            CardIndexErrorV1::MissingCards(vec![MissingCardV1 {
                card_id: 2,
                name: CARD_DEFS[2].name.to_owned(),
            }])
        );
        assert!(error.to_string().contains(CARD_DEFS[2].name));
        // The same table loads against an index it covers.
        let table = CubeTableV1::load_v1(&path, &pin, &index(&[1, 3])).unwrap();
        assert_eq!(table.scaled_rows().len(), 2);
    }

    // Test 2: scaling, bit for bit against hand-computed values.
    #[test]
    fn scaling_is_exact_and_uses_every_table_row() {
        let table = source(&scaling_table())
            .unwrap()
            .bind_v1(&index(&[1, 2]))
            .unwrap();
        // Column 0: max |v| over all rows is 200 (card 3, outside the index),
        // so the divisor is 256. Column 1: max 16, a power of two, divisor
        // 16. Column 2: all zero, divisor 1. Column 3: max 1, divisor 1.
        let divisors: Vec<u32> = table.divisors().iter().map(|d| d.to_bits()).collect();
        let expected_divisors: Vec<u32> = [256.0f32, 16.0, 1.0, 1.0]
            .iter()
            .map(|d| d.to_bits())
            .collect();
        assert_eq!(divisors, expected_divisors);
        let expected: [f32; 8] = [
            0.4375, 1.0, 0.0, 1.0, // card 1: 112/256, 16/16, 0/1, 1/1
            -0.125, 0.3125, 0.0, -1.0, // card 2: -32/256, 5/16, 0/1, -1/1
        ];
        let got: Vec<u32> = table.scaled_rows().iter().map(|v| v.to_bits()).collect();
        let want: Vec<u32> = expected.iter().map(|v| v.to_bits()).collect();
        assert_eq!(got, want);
        assert_eq!(table.vector_width(), 4);
        assert_eq!(table.scaled_row(1)[1].to_bits(), 0.3125f32.to_bits());
    }

    // Test 2: the binding moves with one integer, the index, and nothing
    // else.
    #[test]
    fn binding_changes_with_one_integer_and_with_the_index() {
        let base = source(&scaling_table())
            .unwrap()
            .bind_v1(&index(&[1, 2]))
            .unwrap();
        let mut changed = scaling_table();
        changed["cards"][1]["vector"][1] = 6.into();
        let changed = source(&changed).unwrap().bind_v1(&index(&[1, 2])).unwrap();
        assert_ne!(base.binding_sha256(), changed.binding_sha256());
        let other_index = source(&scaling_table())
            .unwrap()
            .bind_v1(&index(&[1, 2, 3]))
            .unwrap();
        assert_ne!(base.binding_sha256(), other_index.binding_sha256());
        assert_eq!(base.binding_sha256().len(), 64);
    }

    // Test 2: the binding follows its stated byte layout.
    #[test]
    fn binding_follows_the_stated_layout() {
        let table = source(&scaling_table())
            .unwrap()
            .bind_v1(&index(&[1, 2]))
            .unwrap();
        let mut hash = Sha256::new();
        hash.update(b"m4-cube-table-binding-v1\0");
        hash.update(table.file_sha256().as_bytes());
        hash.update(index(&[1, 2]).pin_sha256().as_bytes());
        hash.update(4u64.to_le_bytes());
        // Dense columns: cost:generic (1), flag:land (2), type:Creature (3).
        hash.update(3u64.to_le_bytes());
        for column in [1u32, 2, 3] {
            hash.update(column.to_le_bytes());
        }
        for value in [0.4375f32, 1.0, 0.0, 1.0, -0.125, 0.3125, 0.0, -1.0] {
            hash.update(value.to_bits().to_le_bytes());
        }
        assert_eq!(table.binding_sha256(), lowercase_hex(&hash.finalize()));
    }

    // Test 2, check 1: a directory, a missing file and an oversized file.
    #[test]
    fn check_1_refuses_a_non_regular_or_oversized_file() {
        let dir = TempDir::new("check1");
        let ids = index(&[1]);
        let directory = CubeTableV1::load_v1(&dir.0, "", &ids).unwrap_err();
        assert!(
            matches!(&directory, CardIndexErrorV1::File(message) if message.contains("regular file")),
            "{directory}"
        );
        let absent = CubeTableV1::load_v1(&dir.0.join("absent.json"), "", &ids).unwrap_err();
        assert!(matches!(absent, CardIndexErrorV1::File(_)), "{absent}");
        let large = dir.0.join("large.json");
        let file = std::fs::File::create(&large).unwrap();
        file.set_len(TABLE_FILE_CAP_V1 + 1).unwrap();
        drop(file);
        let oversized = CubeTableV1::load_v1(&large, "", &ids).unwrap_err();
        assert!(
            matches!(&oversized, CardIndexErrorV1::File(message) if message.contains("64 MiB")),
            "{oversized}"
        );
    }

    // Test 2, check 2: a changed pin refuses the file.
    #[test]
    fn check_2_refuses_a_changed_sha256() {
        let dir = TempDir::new("check2");
        let (path, pin) = write_table(&dir, "table.json", &scaling_table());
        assert!(CubeTableV1::load_v1(&path, &pin, &index(&[1])).is_ok());
        let mut wrong = pin.clone().into_bytes();
        wrong[0] = if wrong[0] == b'0' { b'1' } else { b'0' };
        let wrong = String::from_utf8(wrong).unwrap();
        let error = CubeTableV1::load_v1(&path, &wrong, &index(&[1])).unwrap_err();
        assert_eq!(
            error,
            CardIndexErrorV1::Sha256Mismatch {
                expected: wrong,
                actual: pin,
            }
        );
    }

    // Test 2, checks 3 to 7: one refusal per case, each on a table that is
    // valid but for the one change.
    #[test]
    fn checks_3_to_7_refuse_each_invalid_table() {
        assert!(source(&scaling_table()).is_ok());
        let unknown_id = CARD_DEFS.len() as u64;
        type Edit = fn(&mut Value, u64);
        type Expect = fn(&CardIndexErrorV1) -> bool;
        let cases: [(&str, Edit, Expect); 13] = [
            (
                "schema",
                |v, _| v["schema"] = "card_semantics_table/v2".into(),
                |e| matches!(e, CardIndexErrorV1::Schema(_)),
            ),
            (
                "carddb hash",
                |v, _| v["kernel_carddb_hash"] = "0000000000000000".into(),
                |e| matches!(e, CardIndexErrorV1::CardDbHash { .. }),
            ),
            (
                "width zero",
                |v, _| v["vector_width"] = 0.into(),
                |e| matches!(e, CardIndexErrorV1::Dimensions(m) if m.contains("vector_width 0")),
            ),
            (
                "width too large",
                |v, _| v["vector_width"] = 65_537.into(),
                |e| matches!(e, CardIndexErrorV1::Dimensions(m) if m.contains("65537")),
            ),
            (
                "card count",
                |v, _| v["card_count"] = 4.into(),
                |e| matches!(e, CardIndexErrorV1::Dimensions(m) if m.contains("card_count")),
            ),
            (
                "vocabulary length",
                |v, _| {
                    v["vocabulary"].as_array_mut().unwrap().pop();
                },
                |e| matches!(e, CardIndexErrorV1::Dimensions(m) if m.contains("vocabulary has")),
            ),
            (
                "vocabulary index order",
                |v, _| v["vocabulary"][2]["index"] = 3.into(),
                |e| matches!(e, CardIndexErrorV1::Dimensions(m) if m.contains("entry 2 has index 3")),
            ),
            (
                "repeated token",
                |v, _| v["vocabulary"][3]["token"] = "flag:land".into(),
                |e| matches!(e, CardIndexErrorV1::Dimensions(m) if m.contains("repeated")),
            ),
            (
                "row width",
                |v, _| v["cards"][0]["vector"] = json!([1, 2, 3]),
                |e| matches!(e, CardIndexErrorV1::Row(m) if m.contains("3 entries")),
            ),
            (
                "duplicate id",
                |v, _| v["cards"][1]["card_id"] = 1.into(),
                |e| matches!(e, CardIndexErrorV1::Row(m) if m.contains("duplicate")),
            ),
            (
                "unknown id",
                |v, unknown| v["cards"][1]["card_id"] = unknown.into(),
                |e| matches!(e, CardIndexErrorV1::Row(m) if m.contains("not a registry id")),
            ),
            (
                "id beyond u16",
                |v, _| v["cards"][1]["card_id"] = 65_536.into(),
                |e| matches!(e, CardIndexErrorV1::Row(m) if m.contains("65536")),
            ),
            (
                "not exact in f32",
                |v, _| v["cards"][2]["vector"][1] = 16_777_217.into(),
                |e| {
                    *e == CardIndexErrorV1::NotExactInF32 {
                        card_id: 3,
                        column: 1,
                        value: 16_777_217,
                    }
                },
            ),
        ];
        for (name, edit, expected) in cases {
            let mut value = scaling_table();
            edit(&mut value, unknown_id);
            let error = source(&value).unwrap_err();
            assert!(expected(&error), "{name}: {error}");
        }
        // 2^24 itself is exact and loads.
        let mut exact = scaling_table();
        exact["cards"][2]["vector"][1] = 16_777_216.into();
        assert!(source(&exact).is_ok());
    }

    // Test 3: the dense rule on a synthetic vocabulary, including appended
    // tokens and neighbours the prefixes must not catch.
    #[test]
    fn dense_rule_selects_by_prefix_and_exact_token() {
        let tokens: Vec<String> = [
            "ability:mana",
            "ability:mana.additional",
            "ability:mana.color=U",
            "ability:mana.cost=TapSelf",
            "ability:mana.amount=Fixed",
            "flag:permanent",
            "flag:permanent_entry",
            "costpart:mana:pip:W",
            "cost:pip:W",
            "subtype:Island",
            "subtype:Elf",
            "stat:power",
            "stat:powerless",
            "flag:enters_tapped",
            "cost:hybrid:WR",
            "fetch:subtype=Island",
        ]
        .iter()
        .map(|token| token.to_string())
        .collect();
        let selected: Vec<(usize, &str)> = dense_columns_for_tokens_v1(&tokens)
            .iter()
            .map(|c| (c.index, tokens[c.index].as_str()))
            .collect();
        assert_eq!(
            selected,
            vec![
                (0, "ability:mana"),
                (2, "ability:mana.color=U"),
                (5, "flag:permanent"),
                (8, "cost:pip:W"),
                (9, "subtype:Island"),
                (11, "stat:power"),
                (13, "flag:enters_tapped"),
                (14, "cost:hybrid:WR"),
                (15, "fetch:subtype=Island"),
            ]
        );
        for token in [
            "flag:permanent_entry",
            "ability:mana.additional",
            "ability:mana.cost=TapSelf",
            "ability:mana.amount=Fixed",
        ] {
            assert!(!is_dense_token_v1(token), "{token}");
        }
    }

    // Test 3: D carries the same bits as S, through a loaded table.
    #[test]
    fn dense_rows_are_the_matching_columns_of_s() {
        let table = source(&scaling_table())
            .unwrap()
            .bind_v1(&index(&[1, 2, 3]))
            .unwrap();
        let columns: Vec<usize> = table.dense_columns().iter().map(|c| c.index).collect();
        assert_eq!(columns, vec![1, 2, 3]);
        assert_eq!(table.dense_rows().len(), 3 * 3);
        for row in 0..3 {
            let from_s: Vec<u32> = columns
                .iter()
                .map(|&j| table.scaled_row(row)[j].to_bits())
                .collect();
            let dense: Vec<u32> = table.dense_row(row).iter().map(|v| v.to_bits()).collect();
            assert_eq!(dense, from_s, "row {row}");
        }
    }

    // Test 7: the index pin against its stated byte layout.
    #[test]
    fn index_pin_follows_the_stated_layout() {
        let ids = [1u16, 2, 3];
        let mut bytes = b"m4-card-index-v1".to_vec();
        bytes.push(0);
        bytes.extend_from_slice(&3u64.to_le_bytes());
        for id in ids {
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        assert_eq!(index(&ids).pin_sha256(), sha(&bytes));
        assert_ne!(index(&ids).pin_sha256(), index(&[1, 2]).pin_sha256());
    }

    // Test 7: the index pin against the literal the program session records.
    #[test]
    fn index_pin_matches_the_recorded_literal() {
        assert!(
            !RECORDED_PIN_OF_IDS_1_2_3.is_empty(),
            "record the pin of [1, 2, 3] in RECORDED_PIN_OF_IDS_1_2_3: {}",
            index(&[1, 2, 3]).pin_sha256()
        );
        assert_eq!(index(&[1, 2, 3]).pin_sha256(), RECORDED_PIN_OF_IDS_1_2_3);
    }

    #[test]
    fn from_ids_refuses_unordered_duplicate_or_unknown_ids() {
        assert_eq!(
            CardIndexV1::from_ids(vec![1, 3, 2]).unwrap_err(),
            CardIndexErrorV1::IndexNotAscending {
                position: 2,
                previous: 3,
                id: 2,
            }
        );
        assert!(matches!(
            CardIndexV1::from_ids(vec![1, 1]).unwrap_err(),
            CardIndexErrorV1::IndexNotAscending { .. }
        ));
        let unknown = u16::try_from(CARD_DEFS.len()).unwrap();
        assert_eq!(
            CardIndexV1::from_ids(vec![1, unknown]).unwrap_err(),
            CardIndexErrorV1::UnknownIndexId(unknown)
        );
        let ids = index(&[1, 5, 9]);
        assert_eq!(ids.row_of(5), Some(1));
        assert_eq!(ids.row_of(4), None);
    }

    // Test 7: the refusal under one draft's opening fires on a small pool,
    // and the kept ids keep the pool's order.
    #[test]
    fn provisional_pool_refuses_fewer_cards_than_a_draft_opens() {
        let cube = json!({"cards": [
            {"name": "Island"}, {"name": "Plains"}, {"name": "Forest"}
        ]})
        .to_string();
        let pool = CubePool::from_json_texts(&cube, "{}").unwrap();
        let [plains, island, ..] = basic_land_ids().unwrap();
        let mut ids = vec![plains, island];
        ids.sort_unstable();
        let error = provisional_pool_v1(&pool, &index(&ids)).unwrap_err();
        assert_eq!(
            error,
            CardIndexErrorV1::PoolTooSmall {
                have: 2,
                need: DRAFTED_CARDS,
            }
        );
        // Below the refusal, the filter itself: pool order, removed listed.
        let pool_ids: Vec<u16> = (1..=400).rev().collect();
        let kept_ids: Vec<u16> = (1..=370).collect();
        let (kept, removed) = filter_pool_ids_v1(&pool_ids, &index(&kept_ids)).unwrap();
        assert_eq!(kept, (1..=370).rev().collect::<Vec<u16>>());
        assert_eq!(removed, (371..=400).rev().collect::<Vec<u16>>());
    }

    // Test 8: the same file twice gives the same bits.
    #[test]
    fn loading_twice_gives_the_same_bits() {
        let dir = TempDir::new("twice");
        let (path, pin) = write_table(&dir, "table.json", &scaling_table());
        let ids = index(&[1, 2, 3]);
        let first = CubeTableV1::load_v1(&path, &pin, &ids).unwrap();
        let second = CubeTableV1::load_v1(&path, &pin, &ids).unwrap();
        let bits = |values: &[f32]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(first.scaled_rows()), bits(second.scaled_rows()));
        assert_eq!(bits(first.dense_rows()), bits(second.dense_rows()));
        assert_eq!(first.binding_sha256(), second.binding_sha256());
    }

    #[test]
    fn provisional_and_full_index_include_the_basics_and_refuse_unknown_names() {
        let value = table_value(&["stat:power"], &[]);
        let table = source(&value).unwrap();
        let basics = basic_land_ids().unwrap();
        let cube = json!({"cards": [{"name": "Island"}]}).to_string();
        let (provisional, left_out) = CardIndexV1::provisional_v1(&cube, &table).unwrap();
        let mut expected = basics.to_vec();
        expected.sort_unstable();
        assert_eq!(provisional.ids(), expected.as_slice());
        // Island has no row, but it is a basic: it enters the index and is
        // not listed as left out. A cube card without a row is left out.
        assert!(left_out.is_empty());
        let with_spell =
            json!({"cards": [{"name": "Island"}, {"name": "Lightning Bolt"}]}).to_string();
        let bolt = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
        let (with_spell_index, left_out) =
            CardIndexV1::provisional_v1(&with_spell, &table).unwrap();
        assert_eq!(left_out, vec![bolt]);
        assert!(!with_spell_index.contains(bolt));
        assert_eq!(
            CardIndexV1::full_v1(&cube).unwrap().ids(),
            expected.as_slice()
        );
        let unknown = json!({"cards": [{"name": "No Such Card Exists"}]}).to_string();
        assert_eq!(
            CardIndexV1::full_v1(&unknown).unwrap_err(),
            CardIndexErrorV1::UnknownCardName("No Such Card Exists".to_owned())
        );
    }
}
