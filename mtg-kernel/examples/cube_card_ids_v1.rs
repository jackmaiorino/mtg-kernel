//! Export kernel card ids and static metadata for a cube list plus the five basic lands.
//!
//! Read-only inspection of `mtg_kernel::card_def` under the `eternal-fixtures` build.

use mtg_kernel::card_def;
use mtg_kernel::mana::ManaColor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

const DEFAULT_CUBE_LIST: &str = "data/cube_v1/mtgo_vintage_cube_20261007.json";
const USAGE: &str = "usage: cube_card_ids_v1 [--cube-list PATH | PATH]";
const BASICS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

#[derive(Deserialize)]
struct CubeList {
    cards: Vec<CubeListCard>,
}

#[derive(Deserialize)]
struct CubeListCard {
    name: String,
}

#[derive(Serialize)]
struct CardIdEntry {
    name: String,
    card_id: Option<u16>,
    colors: &'static [ManaColor],
    is_land: bool,
    has_definition: bool,
}

#[derive(Serialize)]
struct CubeCardIdsExport {
    schema: &'static str,
    kernel_carddb_hash: String,
    cards: Vec<CardIdEntry>,
}

fn parse_cube_list_path() -> Result<PathBuf, String> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => Ok(PathBuf::from(DEFAULT_CUBE_LIST)),
        [flag, path] if flag == "--cube-list" => Ok(PathBuf::from(path)),
        [path] => {
            if path.to_str().is_some_and(|s| s.starts_with('-')) {
                return Err(USAGE.to_owned());
            }
            Ok(PathBuf::from(path))
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn entry_for(name: String) -> CardIdEntry {
    match card_def::card_id_by_name(&name) {
        Some(id) => {
            let def = &card_def::CARD_DEFS[usize::from(id)];
            CardIdEntry {
                name,
                card_id: Some(id),
                colors: def.colors,
                is_land: def.is_land,
                has_definition: true,
            }
        }
        None => CardIdEntry {
            name,
            card_id: None,
            colors: &[],
            is_land: false,
            has_definition: false,
        },
    }
}

fn run() -> Result<(), String> {
    let cube_list_path = parse_cube_list_path()?;
    let bytes = fs::read(&cube_list_path)
        .map_err(|err| format!("failed to read {}: {err}", cube_list_path.display()))?;
    let cube_list: CubeList = serde_json::from_slice(&bytes)
        .map_err(|err| format!("failed to parse {}: {err}", cube_list_path.display()))?;

    let mut seen = BTreeSet::new();
    let mut cards = Vec::with_capacity(cube_list.cards.len() + BASICS.len());
    for card in cube_list.cards {
        if !seen.insert(card.name.clone()) {
            return Err(format!("cube list names a card twice: {}", card.name));
        }
        cards.push(entry_for(card.name));
    }
    for basic in BASICS {
        if seen.insert(basic.to_owned()) {
            cards.push(entry_for(basic.to_owned()));
        }
    }

    let export = CubeCardIdsExport {
        schema: "cube_card_ids/v1",
        kernel_carddb_hash: format!("{:016x}", card_def::KERNEL_CARDDB_HASH),
        cards,
    };
    let json = serde_json::to_string_pretty(&export).map_err(|err| err.to_string())?;
    println!("{json}");
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("cube_card_ids_v1: {err}");
        std::process::exit(1);
    }
}
