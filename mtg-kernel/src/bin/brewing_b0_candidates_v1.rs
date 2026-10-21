//! B0 candidate lists (collab LANES/brewing-b0-20261020/DESIGN.md section 4).
//!
//!   brewing_b0_candidates_v1 singles --deck ID --in-cards IN.json --out OUT.json
//!   brewing_b0_candidates_v1 multi --deck ID --singles SINGLES.json --parents I,J,... --out OUT.json
//!
//! IN.json is `{"in_cards": [{"card_id": u16, "primary": bool}, ...]}`. `multi`
//! takes M2 parents as M1 indices in rank order. Outputs are written once
//! (never overwritten) and list complete executable registrations.

use mtg_kernel::sideboard::checked_in_pauper_registered_deck_by_id_v1;
use mtg_kernel::sideboard_search_campaign_v1::pool_swaps::{
    build_multi_swap_neighborhood_v1, generate_pool_swap_singles_v1, sorted_mainboard_sha256_v1,
    PoolInCardV1, PoolSwapCandidateV1, POOL_SWAP_CANDIDATES_SCHEMA_V1, POOL_SWAP_HASH_CONVENTION_V1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Write;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InCards {
    in_cards: Vec<PoolInCardV1>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Incumbent {
    deck_id: String,
    mainboard: Vec<u16>,
    sideboard: Vec<u16>,
    mainboard_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    schema: String,
    hash_convention: String,
    stage: String,
    incumbent: Incumbent,
    candidates: Vec<PoolSwapCandidateV1>,
}

fn arg(args: &[String], name: &str) -> Result<String, String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("missing {name}"))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().cloned().ok_or("usage: singles|multi ...")?;
    let deck_id = arg(&args, "--deck")?;
    let registered = checked_in_pauper_registered_deck_by_id_v1(&deck_id).map_err(|e| format!("{e:?}"))?;
    let configuration = registered.registered_configuration();
    let incumbent = Incumbent {
        deck_id: deck_id.clone(),
        mainboard: configuration.mainboard().to_vec(),
        sideboard: configuration.sideboard().to_vec(),
        mainboard_sha256: sorted_mainboard_sha256_v1(configuration.mainboard()),
    };
    let candidates = match mode.as_str() {
        "singles" => {
            let raw = std::fs::read_to_string(arg(&args, "--in-cards")?).map_err(|e| e.to_string())?;
            let in_cards: InCards = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            generate_pool_swap_singles_v1(&registered, &in_cards.in_cards)
        }
        "multi" => {
            let raw = std::fs::read_to_string(arg(&args, "--singles")?).map_err(|e| e.to_string())?;
            let singles: Output = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            if singles.stage != "singles" || singles.incumbent.mainboard_sha256 != incumbent.mainboard_sha256 {
                return Err("--singles is not this deck's singles list".to_owned());
            }
            let parents = arg(&args, "--parents")?
                .split(',')
                .filter(|part| !part.is_empty())
                .map(|part| {
                    let index: usize = part.parse().map_err(|_| format!("bad parent index {part}"))?;
                    singles.candidates.get(index).cloned().ok_or(format!("no single {index}"))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut seen: BTreeSet<String> =
                singles.candidates.iter().map(|c| c.mainboard_sha256.clone()).collect();
            seen.insert(incumbent.mainboard_sha256.clone());
            build_multi_swap_neighborhood_v1(&registered, &parents, singles.candidates.len() as u32, &seen)
        }
        other => return Err(format!("unknown mode {other}")),
    };
    let output = Output {
        schema: POOL_SWAP_CANDIDATES_SCHEMA_V1.to_owned(),
        hash_convention: POOL_SWAP_HASH_CONVENTION_V1.to_owned(),
        stage: mode,
        incumbent,
        candidates,
    };
    let path = arg(&args, "--out")?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("{path}: {e}"))?;
    let bytes = serde_json::to_vec_pretty(&output).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    println!("{{\"candidates\":{}}}", output.candidates.len());
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("brewing_b0_candidates_v1: {error}");
        std::process::exit(2);
    }
}
