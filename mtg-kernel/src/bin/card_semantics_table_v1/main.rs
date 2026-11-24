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

/// The writer's lock file, inside the table's staging directory. It is never
/// a tracked data file, so locking it blocks no reader.
///
/// The file is permanent: no writer deletes it, so the path names one file
/// for good and every writer that opens it contends for the same lock. (A
/// lock file that is unlinked on release can be locked, already dead, by a
/// writer that opened it earlier, while a later writer locks its replacement.)
/// The name falls under the repository's `*.tmp` ignore rule, so the file
/// that stays behind is never an untracked change.
const LOCK_FILE: &str = "writer.lock.tmp";
/// The dump saved before publication; it survives only a failed rollback or
/// an interrupted publication, and then it is the only copy of the old dump.
const PREVIOUS_DUMP_FILE: &str = "previous.txt";

fn staging_dir(repo: &Path, table_rel: &str) -> PathBuf {
    repo.join(table_rel).with_extension("staging")
}

/// Joins cleanup errors to the primary result without hiding it.
fn with_cleanup_errors<T>(result: Result<T, String>, cleanup: Vec<String>) -> Result<T, String> {
    if cleanup.is_empty() {
        return result;
    }
    let cleanup = cleanup.join("; ");
    match result {
        Ok(_) => Err(format!("cleanup failed: {cleanup}")),
        Err(e) => Err(format!("{e}; cleanup also failed: {cleanup}")),
    }
}

/// Opens one table's writer lock file without locking it, creating the file
/// and its staging directory on the first write. The file is never truncated.
fn open_writer_lock(repo: &Path, table_rel: &str) -> Result<(PathBuf, std::fs::File), String> {
    use std::fs;
    let stage = staging_dir(repo, table_rel);
    fs::create_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    let lock_path = stage.join(LOCK_FILE);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| format!("{}: {e}", lock_path.display()))?;
    Ok((lock_path, lock))
}

/// Takes the exclusive lock on an opened writer lock file and runs `body`
/// holding it. However long ago the file was opened, it is still the file at
/// its path, because no writer removes it.
fn run_locked<T>(
    lock_path: &Path,
    lock: std::fs::File,
    body: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if let Err(e) = lock.try_lock() {
        return Err(format!(
            "cannot lock {} for regeneration (another writer holds it): {e}",
            lock_path.display()
        ));
    }
    let result = body();
    // Closing the file releases the lock. The file and its directory stay.
    drop(lock);
    result
}

/// Runs `body` holding the writer lock of one table. The lock is taken before
/// the committed prior is read and held through publication and cleanup.
fn with_writer_lock<T>(
    repo: &Path,
    table_rel: &str,
    body: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let (lock_path, lock) = open_writer_lock(repo, table_rel)?;
    run_locked(&lock_path, lock, body)
}

/// Regenerates one table under its writer lock: `produce` reads the committed
/// prior and generates; hard failures stop before any output file is opened.
fn regenerate(
    repo: &Path,
    table_rel: &str,
    dump_rel: &str,
    produce: impl FnOnce() -> Result<Generated, String>,
) -> Result<(), String> {
    with_writer_lock(repo, table_rel, || {
        let generated = produce()?;
        print!("{}", generated.report);
        require_checks(&generated.failures)?;
        write_outputs_at(repo, table_rel, dump_rel, &generated)
    })
}

fn rename_file(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}

fn write_outputs_at(
    repo: &Path,
    table_rel: &str,
    dump_rel: &str,
    generated: &Generated,
) -> Result<(), String> {
    publish_staged(repo, table_rel, dump_rel, generated, rename_file)
}

// Publish the canonical JSON last. A failed rename leaves the previous JSON
// intact; restore its companion dump before removing the staged files.
// No fsync or directory sync: outputs are tracked in git; --check detects a mismatched pair.
// The caller holds the writer lock (`with_writer_lock`).
fn publish_staged(
    repo: &Path,
    table_rel: &str,
    dump_rel: &str,
    generated: &Generated,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), String> {
    use std::fs;
    let table = repo.join(table_rel);
    let dump = repo.join(dump_rel);
    let stage = staging_dir(repo, table_rel);
    let next_table = stage.join("table.json");
    let next_dump = stage.join("dump.txt");
    let old_dump = stage.join(PREVIOUS_DUMP_FILE);
    match fs::symlink_metadata(&old_dump) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(format!(
                "{} holds the dump retained by an earlier failed or interrupted publication; \
                 reconcile {} and {} with it (restore it as the dump or delete it), then rerun",
                old_dump.display(),
                dump.display(),
                table.display()
            ));
        }
        Err(e) => return Err(format!("{}: {e}", old_dump.display())),
    }
    fs::create_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    let at = |path: &Path, e: io::Error| format!("{}: {e}", path.display());
    let mut preserve_old_dump = false;
    let result = (|| -> Result<(), String> {
        fs::write(&next_table, &generated.table_text).map_err(|e| at(&next_table, e))?;
        fs::write(&next_dump, &generated.dump_text).map_err(|e| at(&next_dump, e))?;
        let had_dump = match fs::copy(&dump, &old_dump) {
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(format!("{} -> {}: {e}", dump.display(), old_dump.display())),
        };
        rename(&next_dump, &dump).map_err(|e| at(&dump, e))?;
        if let Err(e) = rename(&next_table, &table) {
            let rollback = if had_dump {
                rename(&old_dump, &dump)
            } else {
                fs::remove_file(&dump)
            };
            if let Err(rollback_error) = rollback {
                preserve_old_dump = had_dump;
                let retained = if had_dump {
                    old_dump.display().to_string()
                } else {
                    "nothing (no previous dump existed)".to_string()
                };
                return Err(format!(
                    "{}: {e}; could not restore {}: {rollback_error}; retained {retained}",
                    table.display(),
                    dump.display()
                ));
            }
            return Err(at(&table, e));
        }
        Ok(())
    })();
    // Attempt every step; only the generator's own staged files are removed.
    // The staging directory stays: it holds the permanent writer lock.
    let mut cleanup = Vec::new();
    let mut owned = vec![&next_table, &next_dump];
    if !preserve_old_dump {
        owned.push(&old_dump);
    }
    for file in owned {
        match fs::remove_file(file) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => cleanup.push(at(file, e)),
        }
    }
    with_cleanup_errors(result, cleanup)
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
            Mode::Write => regenerate(repo, table::CUBE_TABLE_PATH, table::CUBE_DUMP_PATH, || {
                let prior = committed_prior_at(repo, table::CUBE_TABLE_PATH)?;
                generate_cube(repo, &prior)
            }),
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
        Mode::Write => regenerate(repo, table::TABLE_PATH, table::DUMP_PATH, || {
            let prior = committed_prior_at(repo, table::TABLE_PATH)?;
            generate(repo, &prior)
        }),
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

    fn generated(table_text: &str, dump_text: &str) -> Generated {
        Generated {
            table_text: table_text.into(),
            dump_text: dump_text.into(),
            report: String::new(),
            failures: Vec::new(),
        }
    }

    fn locked_write(repo: &Path, generated: &Generated) -> Result<(), String> {
        with_writer_lock(repo, table::TABLE_PATH, || {
            write_outputs_at(repo, table::TABLE_PATH, table::DUMP_PATH, generated)
        })
    }

    /// Another writer holding the Pauper table's lock, opened as a writer
    /// opens it.
    fn hold_lock(repo: &Path) -> std::fs::File {
        let (_, lock) = open_writer_lock(repo, table::TABLE_PATH).unwrap();
        lock.try_lock().unwrap();
        lock
    }

    /// The names in a table's staging directory, sorted.
    fn staged(repo: &Path, table_rel: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(staging_dir(repo, table_rel))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn table_replacement_failure_restores_the_previous_dump() {
        let temp = scratch_repo("restore");
        std::fs::create_dir(temp.join(table::TABLE_PATH)).unwrap();
        std::fs::write(temp.join(table::DUMP_PATH), "original").unwrap();
        assert!(locked_write(&temp, &generated("new table", "new dump")).is_err());
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "original");
        assert_eq!(staged(&temp, table::TABLE_PATH), [LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn table_replacement_failure_without_a_previous_dump_removes_the_new_dump() {
        let temp = scratch_repo("restore-none");
        std::fs::create_dir(temp.join(table::TABLE_PATH)).unwrap();
        let error = locked_write(&temp, &generated("new table", "new dump")).unwrap_err();
        assert!(!error.contains("could not restore"), "{error}");
        assert!(!temp.join(table::DUMP_PATH).exists());
        assert!(temp.join(table::TABLE_PATH).is_dir());
        assert_eq!(staged(&temp, table::TABLE_PATH), [LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn successful_write_leaves_only_the_writer_lock_in_staging() {
        let temp = scratch_repo("fresh");
        locked_write(&temp, &generated("new table", "new dump")).unwrap();
        assert_eq!(read(&temp, table::TABLE_PATH).unwrap(), "new table");
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "new dump");
        assert_eq!(staged(&temp, table::TABLE_PATH), [LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn successful_write_replaces_existing_outputs_and_drops_the_saved_dump() {
        let temp = scratch_repo("replace");
        std::fs::write(temp.join(table::TABLE_PATH), "old table").unwrap();
        std::fs::write(temp.join(table::DUMP_PATH), "old dump").unwrap();
        let stage = staging_dir(&temp, table::TABLE_PATH);
        let mut renames = 0;
        with_writer_lock(&temp, table::TABLE_PATH, || {
            publish_staged(
                &temp,
                table::TABLE_PATH,
                table::DUMP_PATH,
                &generated("new table", "new dump"),
                |from: &Path, to: &Path| {
                    renames += 1;
                    // The old dump is saved before the first rename, and the
                    // writer lock is still held during publication.
                    assert_eq!(
                        std::fs::read_to_string(stage.join(PREVIOUS_DUMP_FILE)).unwrap(),
                        "old dump"
                    );
                    let nested = with_writer_lock(&temp, table::TABLE_PATH, || Ok(()));
                    assert!(nested.unwrap_err().contains("cannot lock"));
                    std::fs::rename(from, to)
                },
            )
        })
        .unwrap();
        assert_eq!(renames, 2);
        assert_eq!(read(&temp, table::TABLE_PATH).unwrap(), "new table");
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "new dump");
        assert_eq!(staged(&temp, table::TABLE_PATH), [LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn failed_rollback_retains_the_previous_dump_and_a_retry_refuses_to_touch_it() {
        let temp = scratch_repo("failed-rollback");
        std::fs::write(temp.join(table::TABLE_PATH), "old table").unwrap();
        std::fs::write(temp.join(table::DUMP_PATH), "old dump").unwrap();
        let stage = staging_dir(&temp, table::TABLE_PATH);
        let previous = stage.join(PREVIOUS_DUMP_FILE);
        let mut renames = 0;
        let error = with_writer_lock(&temp, table::TABLE_PATH, || {
            publish_staged(
                &temp,
                table::TABLE_PATH,
                table::DUMP_PATH,
                &generated("new table", "new dump"),
                |from: &Path, to: &Path| {
                    renames += 1;
                    if renames == 1 {
                        std::fs::rename(from, to)
                    } else {
                        Err(io::Error::other("injected rename failure"))
                    }
                },
            )
        })
        .unwrap_err();
        assert_eq!(renames, 3, "publish dump, publish table, restore dump");
        assert!(error.contains("could not restore"), "{error}");
        assert!(error.contains(PREVIOUS_DUMP_FILE), "{error}");
        assert_eq!(read(&temp, table::TABLE_PATH).unwrap(), "old table");
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "new dump");
        assert_eq!(std::fs::read_to_string(&previous).unwrap(), "old dump");
        // The staged table is gone; the retained dump and the lock file stay.
        assert_eq!(
            staged(&temp, table::TABLE_PATH),
            [PREVIOUS_DUMP_FILE, LOCK_FILE]
        );

        // A retry, even one whose own staging would fail, never overwrites or
        // deletes the retained dump and names it.
        std::fs::create_dir(stage.join("table.json")).unwrap();
        let retry = locked_write(&temp, &generated("newer table", "newer dump")).unwrap_err();
        assert!(retry.contains("reconcile"), "{retry}");
        assert!(retry.contains(&previous.display().to_string()), "{retry}");
        assert_eq!(std::fs::read_to_string(&previous).unwrap(), "old dump");
        assert_eq!(read(&temp, table::TABLE_PATH).unwrap(), "old table");
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "new dump");
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn cleanup_continues_past_a_failed_first_step_and_keeps_the_primary_error() {
        let temp = scratch_repo("cleanup-errors");
        let stage = staging_dir(&temp, table::TABLE_PATH);
        // A non-empty directory where the staged table goes. The staged table
        // is the first file written and the first file cleanup removes, so
        // writing it fails (the primary error) and so does the first cleanup
        // step.
        let blocked = stage.join("table.json");
        std::fs::create_dir_all(&blocked).unwrap();
        std::fs::write(blocked.join("keep"), "x").unwrap();
        // A staged dump left by an interrupted run: a later cleanup step.
        std::fs::write(stage.join("dump.txt"), "interrupted").unwrap();
        let write_error = std::fs::write(&blocked, "probe").unwrap_err();
        let remove_error = std::fs::remove_file(&blocked).unwrap_err();

        let error = locked_write(&temp, &generated("new table", "new dump")).unwrap_err();
        // The primary error is kept, with the cleanup error beside it.
        assert_eq!(
            error,
            format!(
                "{path}: {write_error}; cleanup also failed: {path}: {remove_error}",
                path = blocked.display()
            )
        );
        // Cleanup went on after its first step failed: the later staged file
        // is gone, and only the entry that could not be removed is left.
        assert!(!stage.join("dump.txt").exists());
        assert_eq!(staged(&temp, table::TABLE_PATH), ["table.json", LOCK_FILE]);
        assert!(blocked.join("keep").is_file());
        assert!(!temp.join(table::TABLE_PATH).exists());
        assert!(!temp.join(table::DUMP_PATH).exists());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn publication_error_survives_a_failed_first_cleanup_step_and_later_files_are_removed() {
        let temp = scratch_repo("cleanup-later-files");
        std::fs::write(temp.join(table::TABLE_PATH), "old table").unwrap();
        std::fs::write(temp.join(table::DUMP_PATH), "old dump").unwrap();
        let stage = staging_dir(&temp, table::TABLE_PATH);
        let blocked = stage.join("table.json");
        let mut renames = 0;
        let error = with_writer_lock(&temp, table::TABLE_PATH, || {
            publish_staged(
                &temp,
                table::TABLE_PATH,
                table::DUMP_PATH,
                &generated("new table", "new dump"),
                |_from: &Path, _to: &Path| {
                    renames += 1;
                    // Everything is staged by the first rename.
                    for name in ["table.json", "dump.txt", PREVIOUS_DUMP_FILE] {
                        assert!(stage.join(name).is_file(), "{name} is not staged");
                    }
                    // The staged table, the first file cleanup removes, turns
                    // into a non-empty directory that cannot be removed, and
                    // publication fails (the primary error).
                    std::fs::remove_file(&blocked).unwrap();
                    std::fs::create_dir(&blocked).unwrap();
                    std::fs::write(blocked.join("keep"), "x").unwrap();
                    Err(io::Error::other("injected rename failure"))
                },
            )
        })
        .unwrap_err();
        assert_eq!(renames, 1);
        let remove_error = std::fs::remove_file(&blocked).unwrap_err();
        // The primary error is kept, with the cleanup error beside it.
        assert_eq!(
            error,
            format!(
                "{}: injected rename failure; cleanup also failed: {}: {remove_error}",
                temp.join(table::DUMP_PATH).display(),
                blocked.display()
            )
        );
        // Cleanup went on after its first step failed: the staged dump and
        // the saved previous dump are gone, and nothing was published.
        assert_eq!(staged(&temp, table::TABLE_PATH), ["table.json", LOCK_FILE]);
        assert_eq!(read(&temp, table::TABLE_PATH).unwrap(), "old table");
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "old dump");
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn cleanup_error_after_success_is_reported() {
        assert_eq!(with_cleanup_errors(Ok(()), Vec::new()), Ok(()));
        assert_eq!(
            with_cleanup_errors(Ok(()), vec!["a: busy".into()]),
            Err::<(), _>("cleanup failed: a: busy".into())
        );
        assert_eq!(
            with_cleanup_errors::<()>(Err("primary".into()), vec!["a".into(), "b".into()]),
            Err("primary; cleanup also failed: a; b".into())
        );
    }

    #[test]
    fn failed_write_keeps_foreign_staging_files() {
        let temp = scratch_repo("foreign-failed");
        let stage = staging_dir(&temp, table::TABLE_PATH);
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("local.txt"), "local work").unwrap();
        std::fs::create_dir(temp.join(table::TABLE_PATH)).unwrap();
        std::fs::write(temp.join(table::DUMP_PATH), "original").unwrap();
        assert!(locked_write(&temp, &generated("new table", "new dump")).is_err());
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "original");
        assert_eq!(
            std::fs::read_to_string(stage.join("local.txt")).unwrap(),
            "local work"
        );
        assert_eq!(staged(&temp, table::TABLE_PATH), ["local.txt", LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn interrupted_staging_is_reused_after_the_writer_releases_its_lock() {
        let temp = scratch_repo("retry");
        let table = temp.join(table::TABLE_PATH);
        let stage = staging_dir(&temp, table::TABLE_PATH);
        std::fs::create_dir(&stage).unwrap();
        std::fs::write(stage.join("table.json"), "interrupted").unwrap();
        std::fs::write(stage.join("local.txt"), "local work").unwrap();
        let lock = hold_lock(&temp);
        let blocked = locked_write(&temp, &generated("new table", "new dump"));
        assert!(blocked.unwrap_err().contains("cannot lock"));
        assert_eq!(
            std::fs::read_to_string(stage.join("table.json")).unwrap(),
            "interrupted"
        );
        drop(lock);
        locked_write(&temp, &generated("new table", "new dump")).unwrap();
        assert_eq!(std::fs::read_to_string(table).unwrap(), "new table");
        assert_eq!(read(&temp, table::DUMP_PATH).unwrap(), "new dump");
        assert_eq!(
            std::fs::read_to_string(stage.join("local.txt")).unwrap(),
            "local work"
        );
        assert_eq!(staged(&temp, table::TABLE_PATH), ["local.txt", LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn writer_lock_is_a_staging_file_taken_before_the_prior_is_read() {
        let temp = scratch_repo("lock-order");
        let lock = hold_lock(&temp);
        let mut produced = false;
        let error = regenerate(&temp, table::TABLE_PATH, table::DUMP_PATH, || {
            produced = true;
            Ok(generated("new table", "new dump"))
        })
        .unwrap_err();
        assert!(error.contains("cannot lock"), "{error}");
        assert!(error.contains(LOCK_FILE), "{error}");
        assert!(!produced, "the prior was read without the writer lock");
        assert!(!temp.join(table::TABLE_PATH).exists());
        drop(lock);

        regenerate(&temp, table::TABLE_PATH, table::DUMP_PATH, || {
            // Held while generating; the tracked mapping input stays readable.
            let nested = with_writer_lock(&temp, table::TABLE_PATH, || Ok(()));
            assert!(nested.unwrap_err().contains("cannot lock"));
            let mapping = std::fs::File::open(temp.join(table::MAPPING_PATH)).unwrap();
            mapping.try_lock().unwrap();
            drop(mapping);
            read(&temp, table::MAPPING_PATH).unwrap();
            Ok(generated("new table", "new dump"))
        })
        .unwrap();
        assert_eq!(read(&temp, table::TABLE_PATH).unwrap(), "new table");
        assert_eq!(staged(&temp, table::TABLE_PATH), [LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn lock_file_is_not_replaced_between_a_writers_open_and_lock() {
        let temp = scratch_repo("lock-identity");
        let stage = staging_dir(&temp, table::TABLE_PATH);
        let lock_path = stage.join(LOCK_FILE);
        // Mark the lock file: a replacement would not carry the mark.
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(&lock_path, "the first lock file").unwrap();

        // Writer B opens the lock file while writer A holds it, and is
        // delayed before it locks. A publishes and releases.
        let mut delayed = None;
        with_writer_lock(&temp, table::TABLE_PATH, || {
            delayed = Some(open_writer_lock(&temp, table::TABLE_PATH)?);
            write_outputs_at(
                &temp,
                table::TABLE_PATH,
                table::DUMP_PATH,
                &generated("table a", "dump a"),
            )
        })
        .unwrap();
        let (delayed_path, delayed_lock) = delayed.unwrap();
        assert_eq!(delayed_path, lock_path);
        // A left the lock file alone: the path still names the file B opened.
        assert_eq!(
            std::fs::read_to_string(&lock_path).unwrap(),
            "the first lock file"
        );

        // Writer C starts and takes the lock. B then locks what it opened:
        // that is the file C holds, so B is refused and does not run.
        let held = hold_lock(&temp);
        let mut ran = false;
        let error = run_locked(&delayed_path, delayed_lock, || {
            ran = true;
            Ok(())
        })
        .unwrap_err();
        assert!(error.contains("cannot lock"), "{error}");
        assert!(!ran, "two writers held the writer lock at once");
        drop(held);
        assert_eq!(read(&temp, table::TABLE_PATH).unwrap(), "table a");
        assert_eq!(staged(&temp, table::TABLE_PATH), [LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn permanent_writer_lock_is_covered_by_a_repository_ignore_rule() {
        // The lock file outlives every run, and builds record whether the
        // work tree has untracked files, so its name has to be an ignored one.
        let ignore = read(&repo_root(), ".gitignore").unwrap();
        let extension = Path::new(LOCK_FILE)
            .extension()
            .and_then(|extension| extension.to_str())
            .expect("the lock file name has an extension");
        let rule = format!("*.{extension}");
        assert!(
            ignore.lines().any(|line| line.trim() == rule),
            ".gitignore has no {rule} rule to cover {LOCK_FILE}"
        );
    }

    #[test]
    fn only_a_missing_committed_table_is_an_empty_prior() {
        let temp = scratch_repo("prior");
        let path = temp.join(table::TABLE_PATH);
        assert_eq!(committed_prior_at(&temp, table::TABLE_PATH), Ok(Vec::new()));
        std::fs::write(&path, [0xffu8, 0xfe, 0x00]).unwrap();
        assert!(committed_prior_at(&temp, table::TABLE_PATH).is_err());
        std::fs::write(&path, "{ not json").unwrap();
        assert!(committed_prior_at(&temp, table::TABLE_PATH).is_err());
        std::fs::write(&path, "{\"vocabulary\": 3}").unwrap();
        assert!(committed_prior_at(&temp, table::TABLE_PATH).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(committed_prior_at(&temp, table::TABLE_PATH).is_err());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn unexpected_arguments_get_the_usage_error() {
        for argv in [
            args(&[]),
            args(&["--write", "--unknown"]),
            args(&["--check", "extra"]),
            args(&["--write", "--check"]),
            args(&["--write", "--write"]),
            args(&["--cube", "--cube", "--write"]),
            args(&["--cube", "--write", "--check"]),
            args(&["--cube", "--unknown"]),
            args(&["write"]),
        ] {
            assert_eq!(parse_args(&argv).unwrap_err(), USAGE, "{argv:?}");
            assert_eq!(run_args(Path::new("unused"), &argv).unwrap_err(), USAGE);
        }
        let write = parse_args(&args(&["--write"])).unwrap();
        assert!(!write.cube && write.mode == Mode::Write);
        let check = parse_args(&args(&["--check"])).unwrap();
        assert!(!check.cube && check.mode == Mode::Check);
        for (argv, mode) in [
            (args(&["--cube", "--write"]), Mode::Write),
            (args(&["--write", "--cube"]), Mode::Write),
            (args(&["--cube", "--check"]), Mode::Check),
            (args(&["--check", "--cube"]), Mode::Check),
        ] {
            match parse_args(&argv) {
                Ok(cli) => {
                    if !cfg!(feature = "eternal-fixtures") {
                        panic!("the default build accepted {argv:?}");
                    }
                    assert!(cli.cube && cli.mode == mode, "{argv:?}");
                }
                Err(error) => {
                    if cfg!(feature = "eternal-fixtures") {
                        panic!("the eternal build refused {argv:?}: {error}");
                    }
                    assert_eq!(error, CUBE_FEATURE_ERR);
                }
            }
        }
    }

    fn tokens(items: &[(&str, extract::TokenKind, i64)]) -> extract::CardTokens {
        let mut out = extract::CardTokens::default();
        for &(token, kind, amount) in items {
            let (count, value) = match kind {
                extract::TokenKind::Count => (amount, 0),
                extract::TokenKind::Value => (1, amount),
            };
            out.tokens.insert(
                token.to_string(),
                extract::Acc {
                    kind,
                    count,
                    value,
                    via_mapping: false,
                    direct: true,
                },
            );
        }
        out
    }

    #[test]
    fn regeneration_keeps_a_reordered_prior_and_appends_only_new_tokens() {
        use super::extract::TokenKind::{Count, Value};
        let entry = |token: &str, kind: &str| (token.to_string(), kind.to_string());
        // Shared prefix, then cube-only columns out of sorted order, then a
        // column no card carries any more.
        let prior = vec![
            entry("type:Creature", "count"),
            entry("kw:FLYING", "count"),
            entry("z:cube_only", "count"),
            entry("a:cube_only", "value"),
            entry("leaf:Retired", "count"),
        ];
        let rows = vec![table::CardRow {
            name: "Fixture".into(),
            id: 0,
            tokens: tokens(&[
                ("type:Creature", Count, 1),
                ("z:cube_only", Count, 2),
                ("a:cube_only", Value, 7),
                ("m:new", Count, 1),
                ("b:new", Count, 3),
            ]),
        }];
        let built = table::build_from_rows(table::TableBuildInput {
            pool_path: table::CUBE_POOL_PATH,
            pool_text: "",
            mapping_text: "",
            rows,
            prior: &prior,
            pauper_vocabulary: None,
        })
        .unwrap();
        let order: Vec<(&str, &str, usize)> = built
            .vocabulary
            .iter()
            .map(|e| (e.token.as_str(), e.kind, e.carriers))
            .collect();
        assert_eq!(
            order,
            vec![
                ("type:Creature", "count", 1),
                ("kw:FLYING", "count", 0),
                ("z:cube_only", "count", 1),
                ("a:cube_only", "value", 1),
                ("leaf:Retired", "count", 0),
                ("b:new", "count", 1),
                ("m:new", "count", 1),
            ]
        );
        assert_eq!(built.vectors, vec![vec![1, 0, 2, 7, 0, 3, 1]]);
    }

    #[test]
    fn no_effect_reasons_state_only_what_the_definition_establishes() {
        use super::checks::{no_effect_token_reason_for as reason, DefinitionFacts};
        let mana_creature = DefinitionFacts {
            creature: true,
            mana_ability: true,
            ..DefinitionFacts::default()
        };
        assert_eq!(
            reason(mana_creature),
            "nonland permanent with only a mana ability in its definition"
        );
        // A mana creature with a keyword (a flying mana bird) is not mana-only.
        let with_keyword = DefinitionFacts {
            keywords: true,
            ..mana_creature
        };
        let flying = reason(with_keyword);
        assert!(!flying.contains("only"), "{flying}");
        assert!(flying.contains("mana ability") && flying.contains("keywords"));
        for other in [
            DefinitionFacts {
                static_rules: true,
                ..mana_creature
            },
            DefinitionFacts {
                abilities: true,
                ..mana_creature
            },
            DefinitionFacts {
                attachment: true,
                ..mana_creature
            },
        ] {
            assert_eq!(reason(other), flying);
        }
        let basic = DefinitionFacts {
            land: true,
            mana_ability: true,
            ..DefinitionFacts::default()
        };
        assert_eq!(
            reason(basic),
            "land with only a mana ability in its definition"
        );
        let entry_rule_land = DefinitionFacts {
            static_rules: true,
            ..basic
        };
        assert!(!reason(entry_rule_land).contains("only"));
        let no_mana_land = DefinitionFacts {
            land: true,
            ..DefinitionFacts::default()
        };
        assert_eq!(
            reason(no_mana_land),
            "land with no mana ability in its definition"
        );
        let vanilla = DefinitionFacts {
            creature: true,
            ..DefinitionFacts::default()
        };
        let keyword_creature = DefinitionFacts {
            keywords: true,
            ..vanilla
        };
        assert!(reason(vanilla).starts_with("creature with no keywords"));
        assert!(reason(keyword_creature).starts_with("creature with keywords"));
    }

    #[test]
    fn no_registry_card_with_other_rules_is_reported_as_mana_only() {
        for (index, def) in mtg_kernel::card_def::CARD_DEFS.iter().enumerate() {
            let id = u16::try_from(index).unwrap();
            let has_triggers = !mtg_kernel::trigger::triggers_for(id).is_empty();
            let facts = checks::DefinitionFacts::of(def, has_triggers);
            let reason = checks::no_effect_token_reason(id);
            // Keywords, static rules, attachments and abilities all count.
            if facts.has_other_rules() {
                assert!(!reason.contains("only"), "{}: {reason}", def.name);
            }
        }
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
        // Nothing was staged: only the writer lock was taken, and released.
        assert_eq!(staged(&temp, table::TABLE_PATH), [LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(not(feature = "limited-fdn-fixtures"))]
    #[test]
    fn write_fails_on_an_unreadable_committed_table_instead_of_starting_over() {
        let temp = scratch_repo("unreadable-prior");
        let original = repo_root();
        for path in [table::POOL_PATH, table::DUMP_PATH] {
            std::fs::copy(original.join(path), temp.join(path)).unwrap();
        }
        for bytes in [[0xffu8, 0xfe, 0x00].as_slice(), b"{ not json".as_slice()] {
            std::fs::write(temp.join(table::TABLE_PATH), bytes).unwrap();
            let error = run_args(&temp, &args(&["--write"])).unwrap_err();
            assert!(!error.contains("check(s) failed"), "{error}");
            assert_eq!(std::fs::read(temp.join(table::TABLE_PATH)).unwrap(), bytes);
            assert_eq!(
                std::fs::read(original.join(table::DUMP_PATH)).unwrap(),
                std::fs::read(temp.join(table::DUMP_PATH)).unwrap()
            );
            assert_eq!(staged(&temp, table::TABLE_PATH), [LOCK_FILE]);
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
        let sentinel_table =
            r#"{"vocabulary":[{"index":0,"token":"wrong:prefix","kind":"count","carriers":0}]}"#;
        std::fs::write(temp.join(table::CUBE_TABLE_PATH), sentinel_table).unwrap();
        std::fs::write(temp.join(table::CUBE_DUMP_PATH), "sentinel dump").unwrap();
        let err = run_args(&temp, &args(&["--cube", "--write"])).unwrap_err();
        assert!(err.contains("Pauper vocabulary"), "{err}");
        assert_eq!(read(&temp, table::CUBE_TABLE_PATH).unwrap(), sentinel_table);
        assert_eq!(read(&temp, table::CUBE_DUMP_PATH).unwrap(), "sentinel dump");
        assert_eq!(staged(&temp, table::CUBE_TABLE_PATH), [LOCK_FILE]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn cube_write_fails_on_a_malformed_committed_cube_table() {
        let temp = scratch_repo("cube-malformed-prior");
        let original = repo_root();
        for path in [table::CUBE_POOL_PATH, table::TABLE_PATH] {
            std::fs::copy(original.join(path), temp.join(path)).unwrap();
        }
        std::fs::write(temp.join(table::CUBE_TABLE_PATH), "{ not json").unwrap();
        let err = run_args(&temp, &args(&["--cube", "--write"])).unwrap_err();
        assert!(err.contains("committed table JSON"), "{err}");
        assert_eq!(read(&temp, table::CUBE_TABLE_PATH).unwrap(), "{ not json");
        assert!(!temp.join(table::CUBE_DUMP_PATH).exists());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn cube_regeneration_keeps_a_reordered_prior_and_appends_only_new_tokens() {
        let repo = repo_root();
        let pauper =
            table::try_committed_vocabulary(&read(&repo, table::TABLE_PATH).unwrap()).unwrap();
        let fresh = generate_cube(&repo, &[]).unwrap();
        let fresh = table::try_committed_vocabulary(&fresh.table_text).unwrap();
        let suffix = &fresh[pauper.len()..];
        assert!(
            suffix.len() >= 3,
            "too few cube-only columns: {}",
            suffix.len()
        );
        // A prior that reorders the cube-only columns, keeps a column no card
        // uses, and lacks one token the cards now carry.
        let new_token = suffix[0].clone();
        let mut prior = pauper.clone();
        prior.extend(suffix[1..].iter().rev().cloned());
        prior.push(("leaf:RetiredCubeFixture".to_string(), "count".to_string()));
        let generated = generate_cube(&repo, &prior).unwrap();
        assert!(generated.failures.is_empty(), "{:#?}", generated.failures);
        let mut expected = prior.clone();
        expected.push(new_token);
        let vocab = table::try_committed_vocabulary(&generated.table_text).unwrap();
        assert_eq!(vocab, expected);
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn mana_creature_with_a_keyword_is_not_reported_as_mana_only() {
        let id = mtg_kernel::card_def::card_id_by_name("Birds of Paradise").unwrap();
        let reason = checks::no_effect_token_reason(id);
        assert!(!reason.contains("only"), "{reason}");
        assert!(reason.contains("mana ability"), "{reason}");
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
            failures
                .iter()
                .any(|f| f.contains("card database") && f.contains(CUBE_REGEN_CMD)),
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
