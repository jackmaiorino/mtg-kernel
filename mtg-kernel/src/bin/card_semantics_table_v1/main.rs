//! Card semantics feature table, Pauper pool (arm E of the card
//! representation design; `docs/design/card_semantics_table_v1.md`).
//!
//! Walks the engine's own typed definition of every card in
//! `data/pauper_support_v1.json` and writes a fixed-width integer vector per
//! card, bound to `KERNEL_CARDDB_HASH`. Nothing in the crate reads the table.
//!
//! Usage (from the repository root or anywhere; paths are fixed):
//!   card_semantics_table_v1 --write   regenerate the table and the dump
//!   card_semantics_table_v1 --check   fail unless the committed files equal
//!                                     the generator's output and every check
//!                                     (twins, census, no reader) holds

mtg_kernel::fast_global_allocator!();

mod checks;
mod extract;
mod mapping;
mod table;

use std::path::{Path, PathBuf};

use mtg_kernel::card_def::KERNEL_CARDDB_HASH;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate sits in the repository root")
        .to_path_buf()
}

fn read(repo: &Path, relative: &str) -> Result<String, String> {
    std::fs::read_to_string(repo.join(relative)).map_err(|e| format!("{relative}: {e}"))
}

struct Generated {
    table_text: String,
    dump_text: String,
    report: String,
    failures: Vec<String>,
}

/// Generates the table and dump text and runs the twin and census checks.
/// `prior` is the committed vocabulary, if any.
fn generate(repo: &Path, prior: &[(String, String)]) -> Result<Generated, String> {
    let pool_text = read(repo, table::POOL_PATH)?;
    let mapping_text = read(repo, table::MAPPING_PATH)?;
    let mapping = mapping::parse(&mapping_text)?;
    let built = table::build(&pool_text, &mapping_text, &mapping, prior);
    let names = table::pool_names(&pool_text);
    let before = table::rows(&names, None);

    let (census_text, mut failures) = checks::census_report(&before, &built.rows);
    let mapping_text_report = checks::mapping_report(&built.rows, &mapping);
    let (pairs_text, pair_failures) = checks::pairs(&built);
    failures.extend(pair_failures);

    let mut debug_heads = std::collections::BTreeSet::new();
    for row in &built.rows {
        debug_heads.extend(row.tokens.debug_heads.iter().cloned());
    }
    let mut census_block = String::new();
    census_block.push_str(&census_text);
    census_block.push_str(&mapping_text_report);
    census_block.push_str("similarity (Jaccard of nonzero (column, value) pairs):\n");
    census_block.push_str(&pairs_text);
    census_block.push_str(&format!(
        "variant names read from a Debug head (enums without serde): {}\n",
        debug_heads.len()
    ));
    for head in &debug_heads {
        census_block.push_str(&format!("  {head}\n"));
    }

    let table_text = table::render(&built);
    let dump_text = table::render_dump(&built, &census_block);
    Ok(Generated {
        table_text,
        dump_text,
        report: format!(
            "kernel_carddb_hash {KERNEL_CARDDB_HASH:016x}\nvector_width {}\ncards {}\n{census_block}",
            built.vocabulary.len(),
            built.rows.len()
        ),
        failures,
    })
}

fn committed_prior(repo: &Path) -> Vec<(String, String)> {
    match read(repo, table::TABLE_PATH) {
        Ok(text) => table::committed_vocabulary(&text),
        Err(_) => Vec::new(),
    }
}

/// Every check, as failures; empty means the committed files are current.
fn check(repo: &Path) -> Result<(String, Vec<String>), String> {
    let committed_table = read(repo, table::TABLE_PATH)?;
    let committed_dump = read(repo, table::DUMP_PATH)?;
    let generated = generate(repo, &table::committed_vocabulary(&committed_table))?;
    let mut failures = generated.failures;
    if !table::content_hash_holds(&committed_table) {
        failures.push(format!(
            "{}: content_sha256 does not hold",
            table::TABLE_PATH
        ));
    }
    let recorded: serde_json::Value =
        serde_json::from_str(&committed_table).map_err(|e| e.to_string())?;
    let hash = format!("{KERNEL_CARDDB_HASH:016x}");
    if recorded["kernel_carddb_hash"].as_str() != Some(hash.as_str()) {
        failures.push(format!(
            "the table records card database {}, the engine is {hash}: regenerate with --write",
            recorded["kernel_carddb_hash"]
        ));
    }
    if generated.table_text != committed_table {
        failures.push(format!(
            "{} differs from the generator output: regenerate with --write",
            table::TABLE_PATH
        ));
    }
    if generated.dump_text != committed_dump {
        failures.push(format!(
            "{} differs from the generator output: regenerate with --write",
            table::DUMP_PATH
        ));
    }
    for hit in checks::unreferenced(repo) {
        failures.push(format!("{hit} names the card semantics table"));
    }
    Ok((generated.report, failures))
}

fn main() -> Result<(), String> {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let repo = repo_root();
    match mode.as_str() {
        "--write" => {
            let generated = generate(&repo, &committed_prior(&repo))?;
            std::fs::create_dir_all(repo.join("data/card_semantics_v1"))
                .map_err(|e| e.to_string())?;
            std::fs::write(repo.join(table::TABLE_PATH), &generated.table_text)
                .map_err(|e| e.to_string())?;
            std::fs::write(repo.join(table::DUMP_PATH), &generated.dump_text)
                .map_err(|e| e.to_string())?;
            print!("{}", generated.report);
            for failure in &generated.failures {
                println!("CHECK FAILED: {failure}");
            }
            Ok(())
        }
        "--check" => {
            let (report, failures) = check(&repo)?;
            print!("{report}");
            if failures.is_empty() {
                println!("card semantics table v1: all checks passed");
                Ok(())
            } else {
                for failure in &failures {
                    println!("CHECK FAILED: {failure}");
                }
                Err(format!("{} check(s) failed", failures.len()))
            }
        }
        _ => Err("usage: card_semantics_table_v1 --write | --check".to_string()),
    }
}

// The table binds the default (Pauper) catalog; feature builds that append
// other registries change `KERNEL_CARDDB_HASH` and are out of its scope.
#[cfg(all(test, not(feature = "limited-fdn-fixtures")))]
mod tests {
    use super::*;

    #[test]
    fn committed_table_is_current_and_checks_hold() {
        let (report, failures) = check(&repo_root()).expect("committed files readable");
        println!("{report}");
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn jaccard_is_exact() {
        let j = checks::jaccard(&[1, 0, 3, 2], &[1, 5, 3, 0]);
        assert_eq!((j.shared, j.union), (2, 4));
        assert!(j.at_least(50) && j.at_most(50));
        let k = checks::jaccard(&[2], &[3]);
        assert_eq!((k.shared, k.union), (0, 2));
    }

    #[test]
    fn mapping_rejects_unjustified_entries() {
        let text = r#"{"schema":"card_semantics_leaf_mapping/v1","rules":[],"entries":[{"source":"leaf:X","emit":[{"token":"leaf:DealDamage"}],"why":" "}]}"#;
        assert!(mapping::parse(text).is_err());
    }
}
