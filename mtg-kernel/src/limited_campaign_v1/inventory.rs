//! The 80-deck DraftZero inventory with its pins (design Sections 1.2 and
//! 1.3), read at run time from `data/limited/draftzero_sample_v1`.
//!
//! `sample_sha256_in_pool_order` is computed exactly as
//! `python/tools/limited_training_inventory_v1.py` computes it: SHA-256 over
//! each deck's file name and file bytes in `pool.tsv` order, then the name
//! `pool.tsv` and the file's bytes with CRLF line endings (the form the pin
//! was taken over, whatever line endings the checkout gave the file). The
//! eval decks' bytes enter that digest, but only `train` decks are parsed and
//! resolved into mainboards: no campaign deal can reach an `eval` deck.
use super::sha256_hex;
use crate::limited_match_v1::{DeckFileV1, SessionKindV1};
use crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

type Result<T> = std::result::Result<T, String>;

pub const SAMPLE_DIR_V1: &str = "data/limited/draftzero_sample_v1";
pub const SAMPLE_SHA256_IN_POOL_ORDER_V1: &str =
    "c7e9c271e15150c245aeabf5f2d10d73fbc41aa1586645101c64198d5562d23b";
/// `data/cards_v1.json` and `data/limited/fdn_v1/cards_v1.json`.
pub const CARD_FILES_V1: [(&str, &str); 2] = [
    (
        "data/cards_v1.json",
        "ef738001c373076039f211582f5fd07b11903788e70641ec7e23c84ba4f69b14",
    ),
    (
        "data/limited/fdn_v1/cards_v1.json",
        "91dcd869bb339450d0adc3e8cfa1a8320f560e53c337f21fb263a7a059839e13",
    ),
];
/// The four registrations above 40 cards (design Section 1.3): name, split,
/// cards and file SHA-256.
pub const OVERSIZE_DECKS_V1: [(&str, &str, usize, &str); 4] = [
    (
        "FDN_top_09446_URpG",
        "train",
        41,
        "4fdce6de44c8e2217b8e43e3381ab9ebc446813dbd71ef4805fb7bbf7385ba42",
    ),
    (
        "FDN_top_03718_UR",
        "eval",
        41,
        "6c1166822ee4b535c0ecd7f3e7221385da7571ce8b4239f8a85285cdf0a349ee",
    ),
    (
        "FDN_top_24789_BGpW",
        "eval",
        41,
        "5aabe4f9a807145b806eac130834b526773382e9ba4622002253016b75be2b9b",
    ),
    (
        "FDN_top_21511_WUR",
        "eval",
        49,
        "dc7384f3c03a319c1610ed60153b576f8dcd9f35a0bdd0dfd13d3b0b3012261e",
    ),
];
/// The `train` decks with Drake Hatcher (design Section 1.1).
pub const DRAKE_HATCHER_TRAIN_DECKS_V1: [&str; 4] = [
    "FDN_top_01766_UR",
    "FDN_top_02819_WU",
    "FDN_top_10624_UB",
    "FDN_top_18039_WUpR",
];
pub const DECKS_PER_SPLIT_V1: usize = 40;
const POOL_HEADER_V1: [&str; 5] = ["deck", "split", "colors", "main_colors", "cards"];

/// `pool.tsv` as hashed: CRLF line endings on every platform, as
/// `canonical_pool_bytes` of the inventory tool makes them.
pub fn canonical_pool_bytes_v1(data: &[u8]) -> Vec<u8> {
    let mut lf = Vec::with_capacity(data.len());
    let mut index = 0;
    while index < data.len() {
        if data[index] == b'\r' && data.get(index + 1) == Some(&b'\n') {
            index += 1;
            continue;
        }
        lf.push(data[index]);
        index += 1;
    }
    let mut crlf = Vec::with_capacity(lf.len() + lf.len() / 16);
    for byte in lf {
        if byte == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(byte);
    }
    crlf
}

/// One `pool.tsv` row with its deck file's SHA-256.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PoolRowV1 {
    pub deck: String,
    pub split: String,
    pub cards: usize,
    pub source_sha256: String,
}

/// The pinned inventory: every row in pool order and the verified digests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InventoryV1 {
    pub sample_sha256_in_pool_order: String,
    pub card_file_sha256: [String; 2],
    pub rows: Vec<PoolRowV1>,
}

/// A resolved `train` deck: the mainboard in row and copy order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeckV1 {
    pub name: String,
    pub cards: Vec<u16>,
    pub source_sha256: String,
}

impl DeckV1 {
    /// The sorted card multiset (for overlap checks).
    pub fn multiset(&self) -> Vec<u16> {
        let mut cards = self.cards.clone();
        cards.sort_unstable();
        cards
    }
}

fn read(root: &Path, relative: &str) -> Result<Vec<u8>> {
    let path = root.join(relative);
    std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Reads `pool.tsv` and every deck file's bytes under `repo_root` and refuses
/// unless the pool has the five-column header, 40 `train` and 40 `eval`
/// rows with distinct names, every row's deck file exists, the four
/// oversize registrations have their pinned counts and digests, and the
/// sample digest and both card-file digests equal their pins.
pub fn read_inventory_v1(repo_root: &Path) -> Result<InventoryV1> {
    let pool_bytes = read(repo_root, &format!("{SAMPLE_DIR_V1}/pool.tsv"))?;
    let text = std::str::from_utf8(&pool_bytes).map_err(|_| "pool.tsv is not UTF-8")?;
    let mut lines = text.lines().filter(|line| !line.is_empty());
    let header = lines.next().ok_or("pool.tsv is empty")?;
    if header.split('\t').collect::<Vec<_>>() != POOL_HEADER_V1 {
        return Err(format!("pool.tsv header {header:?} is not the v1 header"));
    }
    let mut digest = Sha256::new();
    let mut rows = Vec::new();
    let mut names = BTreeSet::new();
    for line in lines {
        let columns = line.split('\t').collect::<Vec<_>>();
        let [deck, split, _, _, cards] = columns[..] else {
            return Err(format!("pool.tsv row {line:?} does not have five columns"));
        };
        if !matches!(split, "train" | "eval") {
            return Err(format!("pool.tsv row {deck}: unknown split {split:?}"));
        }
        if !names.insert(deck.to_string()) {
            return Err(format!("pool.tsv names {deck} twice"));
        }
        let cards = cards
            .parse::<usize>()
            .map_err(|_| format!("pool.tsv row {deck}: card count {cards:?}"))?;
        let file_name = format!("{deck}.json");
        let bytes = read(repo_root, &format!("{SAMPLE_DIR_V1}/decks/{file_name}"))?;
        digest.update(file_name.as_bytes());
        digest.update(&bytes);
        rows.push(PoolRowV1 {
            deck: deck.into(),
            split: split.into(),
            cards,
            source_sha256: sha256_hex(&bytes),
        });
    }
    digest.update(b"pool.tsv");
    digest.update(canonical_pool_bytes_v1(&pool_bytes));
    let sample = format!("{:x}", digest.finalize());
    if sample != SAMPLE_SHA256_IN_POOL_ORDER_V1 {
        return Err(format!(
            "sample_sha256_in_pool_order {sample} is not the pinned {SAMPLE_SHA256_IN_POOL_ORDER_V1}"
        ));
    }
    for split in ["train", "eval"] {
        let count = rows.iter().filter(|row| row.split == split).count();
        if count != DECKS_PER_SPLIT_V1 {
            return Err(format!("pool.tsv has {count} {split} decks, not 40"));
        }
    }
    for (name, split, cards, sha256) in OVERSIZE_DECKS_V1 {
        let row = rows
            .iter()
            .find(|row| row.deck == name)
            .ok_or_else(|| format!("pool.tsv lacks {name}"))?;
        if row.split != split || row.cards != cards || row.source_sha256 != sha256 {
            return Err(format!("{name} differs from its pinned registration"));
        }
    }
    let mut card_file_sha256 = [String::new(), String::new()];
    for (slot, (path, pin)) in card_file_sha256.iter_mut().zip(CARD_FILES_V1) {
        let observed = sha256_hex(&read(repo_root, path)?);
        if observed != pin {
            return Err(format!("{path} SHA-256 {observed} is not the pinned {pin}"));
        }
        *slot = observed;
    }
    Ok(InventoryV1 {
        sample_sha256_in_pool_order: sample,
        card_file_sha256,
        rows,
    })
}

/// Parses and resolves the 40 `train` decks in pool order for the FullPool
/// session. Each deck's file must still have the digest the inventory read,
/// name its own row, and resolve to exactly the row's card count, which must
/// lie in `40..=MAX_CUSTOM_DECK_CARDS_V1`. `eval` rows are never opened.
pub fn load_train_decks_v1(repo_root: &Path, inventory: &InventoryV1) -> Result<Vec<DeckV1>> {
    let mut decks = Vec::with_capacity(DECKS_PER_SPLIT_V1);
    for row in inventory.rows.iter().filter(|row| row.split == "train") {
        let bytes = read(
            repo_root,
            &format!("{SAMPLE_DIR_V1}/decks/{}.json", row.deck),
        )?;
        if sha256_hex(&bytes) != row.source_sha256 {
            return Err(format!("{} changed after the inventory was read", row.deck));
        }
        let file: DeckFileV1 = serde_json::from_slice(&bytes)
            .map_err(|error| format!("deck {}: {error}", row.deck))?;
        if file.name.as_deref() != Some(row.deck.as_str()) {
            return Err(format!("deck {} names another deck", row.deck));
        }
        let cards = file
            .resolve_for(SessionKindV1::FullPoolV1)
            .map_err(|error| format!("deck {}: {error}", row.deck))?;
        if cards.len() != row.cards || !(40..=MAX_CUSTOM_DECK_CARDS_V1).contains(&cards.len()) {
            return Err(format!(
                "deck {} resolves to {} cards, pool.tsv registers {}",
                row.deck,
                cards.len(),
                row.cards
            ));
        }
        decks.push(DeckV1 {
            name: row.deck.clone(),
            cards,
            source_sha256: row.source_sha256.clone(),
        });
    }
    Ok(decks)
}

/// The repository root this binary was built in.
pub fn default_repo_root_v1() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}
