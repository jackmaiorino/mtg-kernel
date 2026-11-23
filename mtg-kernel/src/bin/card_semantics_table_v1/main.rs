//! Card semantics feature tables (arm E of the card representation design;
//! `docs/design/card_semantics_table_v1.md`).
//!
//! Walks the engine's own typed definition of every card in the Pauper pool
//! (`data/pauper_support_v1.json`, default build) or the Vintage Cube list
//! (`data/cube_v1/mtgo_vintage_cube_20261007.json`, `eternal-fixtures` build)
//! and writes a fixed-width integer vector per card, bound to
//! `KERNEL_CARDDB_HASH`. Nothing in the crate reads either table.
//!
//! Usage (from the repository root or anywhere; paths are fixed):
//!   card_semantics_table_v1 --write          regenerate the Pauper table and dump (default build)
//!   card_semantics_table_v1 --check          verify the Pauper table and dump (default build)
//!   card_semantics_table_v1 --cube --write   regenerate the cube table and dump (eternal-fixtures)
//!   card_semantics_table_v1 --cube --check   verify the cube table and dump (eternal-fixtures)

mtg_kernel::fast_global_allocator!();

mod checks;
mod extract;
mod mapping;
mod table;

use std::io;
use std::path::{Path, PathBuf};

use mtg_kernel::card_def::KERNEL_CARDDB_HASH;

const CUBE_FEATURE_ERR: &str = "the cube table needs the eternal-fixtures build";
const CUBE_REGEN_CMD: &str = "cargo run --release -p mtg-kernel --features eternal-fixtures --bin card_semantics_table_v1 -- --cube --write";
const USAGE: &str = "usage: card_semantics_table_v1 [--cube] (--write | --check)";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate sits in the repository root")
        .to_path_buf()
}

fn read(repo: &Path, relative: &str) -> Result<String, String> {
    std::fs::read_to_string(repo.join(relative)).map_err(|e| format!("{relative}: {e}"))
}

fn read_optional(repo: &Path, relative: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(repo.join(relative)) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{relative}: {e}")),
    }
}

struct Generated {
    table_text: String,
    dump_text: String,
    report: String,
    failures: Vec<String>,
}

/// Generates the Pauper table and dump text and runs the twin and census checks.
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

/// Generates the Vintage Cube table and dump text and collects hard validation
/// failures before any output file is opened.
fn generate_cube(repo: &Path, cube_prior: &[(String, String)]) -> Result<Generated, String> {
    let cube_text = read(repo, table::CUBE_POOL_PATH)?;
    let mapping_text = read(repo, table::MAPPING_PATH)?;
    let pauper_text = read(repo, table::TABLE_PATH)?;

    let cube_names = table::parse_cube_names(&cube_text)?;
    let mapping = mapping::parse(&mapping_text)?;
    let pauper_vocab = table::try_committed_vocabulary(&pauper_text)?;

    let mut failures = Vec::new();
    if !table::content_hash_holds(&pauper_text) {
        failures.push(format!(
            "{}: content_sha256 does not hold",
            table::TABLE_PATH
        ));
    }
    if pauper_vocab.is_empty() {
        failures.push(format!("{}: vocabulary is empty", table::TABLE_PATH));
    }

    let pauper_vocab_sha256 = table::vocabulary_sha256(&pauper_vocab);
    let effective_prior: &[(String, String)] = if cube_prior.is_empty() {
        &pauper_vocab
    } else if cube_prior.starts_with(&pauper_vocab) {
        cube_prior
    } else {
        failures.push(format!(
            "{}: committed vocabulary does not start with the Pauper vocabulary",
            table::CUBE_TABLE_PATH
        ));
        &pauper_vocab
    };

    let selection = table::select_cube_rows(&cube_names, &mapping)?;
    let built = table::build_from_rows(table::TableBuildInput {
        pool_path: table::CUBE_POOL_PATH,
        pool_text: &cube_text,
        mapping_text: &mapping_text,
        rows: selection.rows,
        prior: effective_prior,
        pauper_vocabulary: Some(table::PauperVocabularyRef {
            path: table::TABLE_PATH,
            sha256: pauper_vocab_sha256.clone(),
            len: pauper_vocab.len(),
        }),
    })?;

    if built.vocabulary.len() < pauper_vocab.len()
        || built
            .vocabulary
            .iter()
            .zip(&pauper_vocab)
            .any(|(entry, (token, kind))| entry.token != *token || entry.kind != kind.as_str())
    {
        failures.push("generated cube vocabulary does not extend the Pauper vocabulary".into());
    }

    let report_block = checks::cube_report(
        &built,
        &selection.before_mapping,
        &mapping,
        pauper_vocab.len(),
        &selection.not_implemented,
    );
    let table_text = table::render(&built);
    if !table::content_hash_holds(&table_text) {
        failures.push("generated cube table failed content_sha256 self-check".into());
    }
    let dump_text = table::render_cube_dump(&built, &report_block);

    Ok(Generated {
        table_text,
        dump_text,
        report: format!(
            "kernel_carddb_hash {KERNEL_CARDDB_HASH:016x}\npauper_vocabulary_sha256 {pauper_vocab_sha256}\n{report_block}"
        ),
        failures,
    })
}

fn committed_prior_at(repo: &Path, relative: &str) -> Result<Vec<(String, String)>, String> {
    match read_optional(repo, relative)? {
        Some(text) => table::try_committed_vocabulary(&text),
        None => Ok(Vec::new()),
    }
}

/// Every Pauper check, as failures; empty means the committed files are current.
fn check(repo: &Path) -> Result<(String, Vec<String>), String> {
    let committed_table = read(repo, table::TABLE_PATH)?;
    let committed_dump = read(repo, table::DUMP_PATH)?;
    let prior = table::try_committed_vocabulary(&committed_table)?;
    let generated = generate(repo, &prior)?;
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

/// Every cube check, as failures; empty means the committed files are current.
fn check_cube(repo: &Path) -> Result<(String, Vec<String>), String> {
    let committed_table = read(repo, table::CUBE_TABLE_PATH)?;
    let committed_dump = read(repo, table::CUBE_DUMP_PATH)?;
    let prior = table::try_committed_vocabulary(&committed_table)?;
    let generated = generate_cube(repo, &prior)?;
    let mut failures = generated.failures;
    if !table::content_hash_holds(&committed_table) {
        failures.push(format!(
            "{}: content_sha256 does not hold",
            table::CUBE_TABLE_PATH
        ));
    }
    let recorded: serde_json::Value =
        serde_json::from_str(&committed_table).map_err(|e| e.to_string())?;
    let hash = format!("{KERNEL_CARDDB_HASH:016x}");
    if recorded["kernel_carddb_hash"].as_str() != Some(hash.as_str()) {
        failures.push(format!(
            "the cube table records card database {}, the engine is {hash}: regenerate with `{CUBE_REGEN_CMD}`",
            recorded["kernel_carddb_hash"]
        ));
    }
    let pauper_text = read(repo, table::TABLE_PATH)?;
    let pauper_vocab = table::try_committed_vocabulary(&pauper_text)?;
    let expected_pauper_sha = table::vocabulary_sha256(&pauper_vocab);
    if recorded["pauper_vocabulary_sha256"].as_str() != Some(expected_pauper_sha.as_str()) {
        failures.push(format!(
            "the cube table records Pauper vocabulary {}, the committed Pauper table has {expected_pauper_sha}: regenerate with `{CUBE_REGEN_CMD}`",
            recorded["pauper_vocabulary_sha256"]
        ));
    }
    if generated.table_text != committed_table {
        failures.push(format!(
            "{} differs from the generator output: regenerate with `{CUBE_REGEN_CMD}`",
            table::CUBE_TABLE_PATH
        ));
    }
    if generated.dump_text != committed_dump {
        failures.push(format!(
            "{} differs from the generator output: regenerate with `{CUBE_REGEN_CMD}`",
            table::CUBE_DUMP_PATH
        ));
    }
    for hit in checks::unreferenced(repo) {
        failures.push(format!("{hit} names the card semantics table"));
    }
    Ok((generated.report, failures))
}

fn require_checks(failures: &[String]) -> Result<(), String> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} check(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

// Publish the canonical JSON last. A failed rename leaves the previous JSON
// intact; restore its companion dump before releasing the staging directory.
fn write_outputs_at(
    repo: &Path,
    table_rel: &str,
    dump_rel: &str,
    generated: &Generated,
) -> Result<(), String> {
    use std::fs;
    let table = repo.join(table_rel);
    let dump = repo.join(dump_rel);
    let stage = table.with_extension("staging");
    // This generator always has a mapping file. Lock its existing handle so
    // interruption releases ownership and a retry can reuse leftover staging.
    let mapping = fs::File::open(repo.join(table::MAPPING_PATH)).map_err(|e| e.to_string())?;
    mapping
        .try_lock()
        .map_err(|e| format!("cannot lock the card semantics mapping for regeneration: {e}"))?;
    fs::create_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    let next_table = stage.join("table.json");
    let next_dump = stage.join("dump.txt");
    let old_dump = stage.join("previous.txt");
    let mut preserve_stage = false;
    let result = (|| -> io::Result<()> {
        fs::write(&next_table, &generated.table_text)?;
        fs::write(&next_dump, &generated.dump_text)?;
        let had_dump = match fs::copy(&dump, &old_dump) {
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        fs::rename(&next_dump, &dump)?;
        if let Err(e) = fs::rename(&next_table, &table) {
            let rollback = if had_dump {
                fs::rename(&old_dump, &dump)
            } else {
                fs::remove_file(&dump)
            };
            if let Err(rollback_error) = rollback {
                preserve_stage = true;
                return Err(io::Error::other(format!(
                    "{e}; could not restore {}: {rollback_error}; retained {}",
                    dump.display(),
                    stage.display()
                )));
            }
            return Err(e);
        }
        Ok(())
    })();
    if !preserve_stage {
        for file in [&next_table, &next_dump, &old_dump] {
            match fs::remove_file(file) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", file.display())),
            }
        }
        match fs::remove_dir(&stage) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => {}
            Err(e) => return Err(format!("{}: {e}", stage.display())),
        }
    }
    result.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Write,
    Check,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cli {
    cube: bool,
    mode: Mode,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut cube = false;
    let mut mode: Option<Mode> = None;
    for arg in args {
        match arg.as_str() {
            "--cube" => {
                if cube {
                    return Err(USAGE.to_string());
                }
                cube = true;
            }
            "--write" => {
                if mode.replace(Mode::Write).is_some() {
                    return Err(USAGE.to_string());
                }
            }
            "--check" => {
                if mode.replace(Mode::Check).is_some() {
                    return Err(USAGE.to_string());
                }
            }
            _ => return Err(USAGE.to_string()),
        }
    }
    if cube && !cfg!(feature = "eternal-fixtures") {
        return Err(CUBE_FEATURE_ERR.to_string());
    }
    let mode = mode.ok_or_else(|| USAGE.to_string())?;
    Ok(Cli { cube, mode })
}

fn run_args(repo: &Path, args: &[String]) -> Result<(), String> {
    let cli = parse_args(args)?;
    if cli.cube {
        return match cli.mode {
            Mode::Write => {
                let prior = committed_prior_at(repo, table::CUBE_TABLE_PATH)?;
                let generated = generate_cube(repo, &prior)?;
                print!("{}", generated.report);
                require_checks(&generated.failures)?;
                write_outputs_at(repo, table::CUBE_TABLE_PATH, table::CUBE_DUMP_PATH, &generated)
            }
            Mode::Check => {
                let (report, failures) = check_cube(repo)?;
                print!("{report}");
                require_checks(&failures)?;
                println!("card semantics table v1 (cube): all checks passed");
                Ok(())
            }
        };
    }

    if cfg!(feature = "eternal-fixtures") {
        return match cli.mode {
            Mode::Write => Err(
                "the Pauper table requires the default build; pass --cube --write for the cube table"
                    .to_string(),
            ),
            Mode::Check => {
                println!(
                    "the Pauper table is checked in the default build; pass --cube --check for the cube table"
                );
                Ok(())
            }
        };
    }

    if cfg!(feature = "limited-fdn-fixtures") {
        return Err("card semantics table v1 requires the default Pauper catalog".to_string());
    }

    match cli.mode {
        Mode::Write => {
            let prior = committed_prior_at(repo, table::TABLE_PATH)?;
            let generated = generate(repo, &prior)?;
            print!("{}", generated.report);
            require_checks(&generated.failures)?;
            write_outputs_at(repo, table::TABLE_PATH, table::DUMP_PATH, &generated)
        }
        Mode::Check => {
            let (report, failures) = check(repo)?;
            print!("{report}");
            require_checks(&failures)?;
            println!("card semantics table v1: all checks passed");
            Ok(())
        }
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run_args(&repo_root(), &args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn scratch_repo(label: &str) -> PathBuf {
        let temp = std::env::temp_dir().join(format!("cardsem-{label}-{}", std::process::id()));
        if temp.exists() {
            std::fs::remove_dir_all(&temp).unwrap();
        }
        std::fs::create_dir(&temp).unwrap();
        std::fs::create_dir_all(temp.join(table::DUMP_PATH).parent().unwrap()).unwrap();
        std::fs::create_dir_all(temp.join(table::CUBE_POOL_PATH).parent().unwrap()).unwrap();
        std::fs::copy(
            repo_root().join(table::MAPPING_PATH),
            temp.join(table::MAPPING_PATH),
        )
        .unwrap();
        temp
    }

    #[test]
    fn table_replacement_failure_restores_the_previous_dump() {
        let temp = scratch_repo("restore");
        std::fs::create_dir(temp.join(table::TABLE_PATH)).unwrap();
        std::fs::write(temp.join(table::DUMP_PATH), "original").unwrap();
        let generated = Generated {
            table_text: "new table".into(),
            dump_text: "new dump".into(),
            report: String::new(),
            failures: Vec::new(),
        };
        assert!(write_outputs_at(&temp, table::TABLE_PATH, table::DUMP_PATH, &generated).is_err());
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "original");
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn interrupted_staging_is_reused_after_the_writer_releases_its_lock() {
        let temp = scratch_repo("retry");
        let table = temp.join(table::TABLE_PATH);
        let stage = table.with_extension("staging");
        std::fs::create_dir(&stage).unwrap();
        std::fs::write(stage.join("table.json"), "interrupted").unwrap();
        std::fs::write(stage.join("local.txt"), "local work").unwrap();
        let lock = std::fs::File::open(temp.join(table::MAPPING_PATH)).unwrap();
        lock.try_lock().unwrap();
        let generated = Generated {
            table_text: "new table".into(),
            dump_text: "new dump".into(),
            report: String::new(),
            failures: Vec::new(),
        };
        assert!(
            write_outputs_at(&temp, table::TABLE_PATH, table::DUMP_PATH, &generated)
                .unwrap_err()
                .contains("cannot lock")
        );
        assert_eq!(
            std::fs::read_to_string(stage.join("table.json")).unwrap(),
            "interrupted"
        );
        drop(lock);
        write_outputs_at(&temp, table::TABLE_PATH, table::DUMP_PATH, &generated).unwrap();
        assert_eq!(std::fs::read_to_string(table).unwrap(), "new table");
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "new dump");
        assert_eq!(
            std::fs::read_to_string(stage.join("local.txt")).unwrap(),
            "local work"
        );
        assert_eq!(std::fs::read_dir(stage).unwrap().count(), 1);
        std::fs::remove_dir_all(temp).unwrap();
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
    fn overlap_and_low_distinctiveness_match_exact_definition() {
        assert_eq!(checks::overlap(&[1, 0, 3, 2], &[1, 5, 3, 2]), (3, 3));
        assert_eq!(checks::overlap(&[1, 5, 3, 2], &[1, 0, 3, 2]), (3, 4));
        assert!(checks::overlap_meets_threshold(3, 4));
        assert!(!checks::overlap_meets_threshold(2, 3));
        assert!(!checks::overlap_meets_threshold(0, 0));

        // 11 identical 4-feature rows -> each has 10 other rows at 4/4 >= 0.75.
        // One extra row matches 3 of their 4 features -> overlaps them at 3/4,
        // and they overlap it at 3/4 (11 neighbors for the 11 rows, 11 for it).
        // One distinct row matches only 2 of 4 -> 0 neighbors >= 0.75.
        let mut rows = Vec::new();
        let mut vectors = Vec::new();
        for i in 0..11 {
            rows.push(table::CardRow {
                name: format!("Clone {i:02}"),
                id: i as u16,
                tokens: extract::CardTokens::default(),
            });
            vectors.push(vec![1, 2, 3, 4, 0]);
        }
        rows.push(table::CardRow {
            name: "ThreeQuarter".into(),
            id: 11,
            tokens: extract::CardTokens::default(),
        });
        vectors.push(vec![1, 2, 3, 0, 5]);
        rows.push(table::CardRow {
            name: "Distinct".into(),
            id: 12,
            tokens: extract::CardTokens::default(),
        });
        vectors.push(vec![1, 2, 9, 9, 9]);

        let synthetic = table::Table {
            pool_path: table::CUBE_POOL_PATH,
            pool_sha256: String::new(),
            mapping_sha256: String::new(),
            pauper_vocabulary: None,
            vocabulary: Vec::new(),
            rows,
            vectors,
        };
        let flagged = checks::low_distinctiveness(&synthetic);
        assert_eq!(flagged.len(), 12);
        assert!(flagged.iter().all(|row| row.name != "Distinct"));
        for row in &flagged {
            assert_eq!(row.neighbors, 11, "{row:?}");
            assert_eq!(row.nonzero, 4, "{row:?}");
        }
    }

    #[test]
    fn vocabulary_sha256_depends_on_order_and_kind() {
        let a = vec![
            ("color:R".into(), "count".into()),
            ("stat:mana_value".into(), "value".into()),
        ];
        let b = vec![
            ("stat:mana_value".into(), "value".into()),
            ("color:R".into(), "count".into()),
        ];
        let c = vec![
            ("color:R".into(), "value".into()),
            ("stat:mana_value".into(), "value".into()),
        ];
        assert_ne!(table::vocabulary_sha256(&a), table::vocabulary_sha256(&b));
        assert_ne!(table::vocabulary_sha256(&a), table::vocabulary_sha256(&c));
    }

    #[test]
    fn mapping_rejects_unjustified_entries() {
        let text = r#"{"schema":"card_semantics_leaf_mapping/v1","rules":[],"entries":[{"source":"leaf:X","emit":[{"token":"leaf:DealDamage"}],"why":" "}]}"#;
        assert!(mapping::parse(text).is_err());
    }

    #[cfg(not(feature = "eternal-fixtures"))]
    #[test]
    fn non_eternal_build_refuses_cube_flag() {
        for argv in [
            args(&["--cube"]),
            args(&["--cube", "--check"]),
            args(&["--cube", "--write"]),
            args(&["--write", "--cube"]),
        ] {
            assert_eq!(
                run_args(Path::new("unused"), &argv).unwrap_err(),
                CUBE_FEATURE_ERR
            );
        }
    }

    #[cfg(all(feature = "limited-fdn-fixtures", not(feature = "eternal-fixtures")))]
    #[test]
    fn extended_catalog_refuses_both_modes_before_reading_the_pool() {
        for mode in ["--check", "--write"] {
            assert_eq!(
                run_args(Path::new("unused"), &args(&[mode])).unwrap_err(),
                "card semantics table v1 requires the default Pauper catalog"
            );
        }
    }

    // The Pauper table binds the default catalog; feature builds that append
    // other registries change `KERNEL_CARDDB_HASH` and are out of its scope.
    #[cfg(not(feature = "limited-fdn-fixtures"))]
    #[test]
    fn write_refuses_a_changed_pair_before_replacing_outputs() {
        let temp = scratch_repo("changed-pair");
        let original = repo_root();
        for path in [table::POOL_PATH, table::TABLE_PATH, table::DUMP_PATH] {
            std::fs::copy(original.join(path), temp.join(path)).unwrap();
        }
        std::fs::write(
            temp.join(table::MAPPING_PATH),
            r#"{"schema":"card_semantics_leaf_mapping/v1","rules":[],"entries":[{"source":"leaf:DealDamage","emit":[{"token":"leaf:ReviewFixture"}],"why":"Changed mapping fixture"}]}"#,
        )
        .unwrap();
        let error = run_args(&temp, &args(&["--write"])).unwrap_err();
        assert!(error.contains("differs from pinned"), "{error}");
        for path in [table::TABLE_PATH, table::DUMP_PATH] {
            assert_eq!(
                std::fs::read(original.join(path)).unwrap(),
                std::fs::read(temp.join(path)).unwrap()
            );
        }
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(not(feature = "limited-fdn-fixtures"))]
    #[test]
    fn committed_table_is_current_and_checks_hold() {
        let (report, failures) = check(&repo_root()).expect("committed files readable");
        println!("{report}");
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn eternal_build_refuses_pauper_write_and_allows_pauper_check() {
        assert!(run_args(Path::new("unused"), &args(&["--write"])).is_err());
        assert!(run_args(Path::new("unused"), &args(&["--check"])).is_ok());
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn cube_write_refuses_hard_failure_before_replacing_outputs() {
        let temp = scratch_repo("cube-hard-fail");
        let original = repo_root();
        for path in [table::CUBE_POOL_PATH, table::TABLE_PATH] {
            std::fs::copy(original.join(path), temp.join(path)).unwrap();
        }
        let sentinel_table = r#"{"vocabulary":[{"index":0,"token":"wrong:prefix","kind":"count","carriers":0}]}"#;
        std::fs::write(temp.join(table::CUBE_TABLE_PATH), sentinel_table).unwrap();
        std::fs::write(temp.join(table::CUBE_DUMP_PATH), "sentinel dump").unwrap();
        let err = run_args(&temp, &args(&["--cube", "--write"])).unwrap_err();
        assert!(err.contains("Pauper vocabulary"), "{err}");
        assert_eq!(
            read(&temp, table::CUBE_TABLE_PATH).unwrap(),
            sentinel_table
        );
        assert_eq!(
            read(&temp, table::CUBE_DUMP_PATH).unwrap(),
            "sentinel dump"
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn cube_carddb_mismatch_fails_check_naming_regeneration_command() {
        let temp = scratch_repo("cube-carddb-mismatch");
        let original = repo_root();
        for path in [table::CUBE_POOL_PATH, table::TABLE_PATH] {
            std::fs::copy(original.join(path), temp.join(path)).unwrap();
        }
        run_args(&temp, &args(&["--cube", "--write"])).unwrap();
        let current_hash = format!("\"kernel_carddb_hash\": \"{KERNEL_CARDDB_HASH:016x}\"");
        let stale_hash = "\"kernel_carddb_hash\": \"0000000000000000\"";
        let table_text = read(&temp, table::CUBE_TABLE_PATH).unwrap();
        assert!(table_text.contains(&current_hash));
        std::fs::write(
            temp.join(table::CUBE_TABLE_PATH),
            table_text.replacen(&current_hash, stale_hash, 1),
        )
        .unwrap();
        let (_, failures) = check_cube(&temp).unwrap();
        assert!(
            failures.iter().any(|f| f.contains("card database") && f.contains(CUBE_REGEN_CMD)),
            "{failures:#?}"
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn cube_write_and_check_roundtrip_preserves_pauper_prefix_and_is_idempotent() {
        let temp = scratch_repo("cube-roundtrip");
        let original = repo_root();
        for path in [table::CUBE_POOL_PATH, table::TABLE_PATH] {
            std::fs::copy(original.join(path), temp.join(path)).unwrap();
        }
        run_args(&temp, &args(&["--cube", "--write"])).unwrap();
        let first_table = read(&temp, table::CUBE_TABLE_PATH).unwrap();
        let first_dump = read(&temp, table::CUBE_DUMP_PATH).unwrap();
        assert!(table::content_hash_holds(&first_table));

        let pauper_vocab =
            table::try_committed_vocabulary(&read(&temp, table::TABLE_PATH).unwrap()).unwrap();
        let cube_vocab = table::try_committed_vocabulary(&first_table).unwrap();
        assert!(cube_vocab.len() >= pauper_vocab.len());
        assert_eq!(&cube_vocab[..pauper_vocab.len()], pauper_vocab.as_slice());

        let parsed: serde_json::Value = serde_json::from_str(&first_table).unwrap();
        let card_names: std::collections::BTreeSet<&str> = parsed["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        for land in table::BASIC_LANDS {
            assert!(card_names.contains(land), "missing basic land {land}");
        }
        assert_eq!(
            parsed["pauper_vocabulary_sha256"].as_str(),
            Some(table::vocabulary_sha256(&pauper_vocab).as_str())
        );

        let (_, failures) = check_cube(&temp).unwrap();
        assert!(failures.is_empty(), "{failures:#?}");

        // Second regeneration must keep exact column order and produce byte-identical output.
        run_args(&temp, &args(&["--cube", "--write"])).unwrap();
        assert_eq!(read(&temp, table::CUBE_TABLE_PATH).unwrap(), first_table);
        assert_eq!(read(&temp, table::CUBE_DUMP_PATH).unwrap(), first_dump);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn committed_cube_table_is_current_and_checks_hold() {
        let (report, failures) = check_cube(&repo_root()).expect("committed cube files readable");
        println!("{report}");
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
