//! Thin CLI wrapping `native_population_refresh_builder_cycle4_v1`: read the
//! plain-file inputs a wrapper (or a human operator) already produced --
//! the whole refresh chain directory (genesis through the tip, per the
//! chain directory naming scheme
//! `native_population_refresh_builder_cycle4_v1` documents), the new payoff
//! panel evaluating the chain tip, and the next boundary's slot identities
//! -- run the pure builder, and write the resulting manifest's exact
//! canonical bytes to `--output` atomically: a create-new temporary file in
//! `--output`'s own directory, written, flushed, and synced, then renamed
//! into place; the temporary is removed on any failure so a failed run
//! never leaves a partial manifest at `--output`. Follows the
//! `checkpoint_shadow_stdio_v1.rs` parsing shape.
//!
//! Exit codes: 0 success. 2 usage (the flags themselves are malformed,
//! missing, duplicated, mutually exclusive, or fail to parse as their
//! primitive type). 3 everything else that goes wrong once the command line
//! is well-formed -- a file that cannot be read or written, a chain
//! directory that does not follow the naming scheme, or the pure builder
//! rejecting the assembled content (contract rejection). The governing
//! contract (`docs/native_cycle4_arm_launcher_v1.md` section 5) only names
//! 0/2/3 for this bin, so I/O failure is folded into 3 alongside content
//! rejection rather than adding an undocumented fourth code.

use mtg_kernel::native_population_refresh_builder_cycle4_v1::{
    build_cycle4_genesis_refresh_v1, build_cycle4_next_refresh_v1,
    cycle4_chain_manifest_filename_v1, cycle4_chain_panel_filename_v1, Cycle4ChainLinkV1,
    Cycle4RefreshBuildResultV1,
};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Highest refresh index this campaign ever chains to (mirrors the
/// pre-registered "refresh indices 0..=16",
/// `docs/native_population_refresh_manifest_cycle4_v1.md`); bounds the
/// chain-directory walk below so a misconfigured directory cannot cause an
/// unbounded read loop.
const CHAIN_WALK_MAX_REFRESH_INDEX_V1: u64 = 16;

fn usage_v1() -> ! {
    eprintln!(
        "usage: cycle4_refresh_build_v1 --trainee-run-sha256 HEX --trainee-base-seed N --slot-identities PATH --output PATH (--genesis | --chain-dir PATH --panel PATH --next-generation N)"
    );
    std::process::exit(2);
}

/// The two ways a boundary can be built, matching the builder module's own
/// two entry points exactly: genesis takes no chain directory, no panel,
/// and no refresh index (it is always refresh 0); every later boundary
/// requires all three.
enum RefreshModeV1 {
    Genesis,
    NextRefresh {
        chain_dir: PathBuf,
        panel: PathBuf,
        next_generation: u64,
    },
}

struct ParsedArgsV1 {
    mode: RefreshModeV1,
    trainee_run_sha256: String,
    trainee_base_seed: u64,
    slot_identities: PathBuf,
    output: PathBuf,
}

#[allow(clippy::too_many_lines)]
fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    let mut genesis = false;
    let mut chain_dir: Option<PathBuf> = None;
    let mut panel: Option<PathBuf> = None;
    let mut next_generation: Option<u64> = None;
    let mut trainee_run_sha256: Option<String> = None;
    let mut trainee_base_seed: Option<u64> = None;
    let mut slot_identities: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;

    let mut index = 0;
    while index < raw.len() {
        let flag = raw[index].to_str().ok_or(())?;
        if flag == "--genesis" {
            if genesis {
                return Err(());
            }
            genesis = true;
            index += 1;
            continue;
        }
        let value = raw.get(index + 1).ok_or(())?;
        match flag {
            "--chain-dir" if chain_dir.is_none() => {
                chain_dir = Some(PathBuf::from(value));
            }
            "--panel" if panel.is_none() => {
                panel = Some(PathBuf::from(value));
            }
            "--trainee-run-sha256" if trainee_run_sha256.is_none() => {
                trainee_run_sha256 = Some(value.to_str().ok_or(())?.to_owned());
            }
            "--trainee-base-seed" if trainee_base_seed.is_none() => {
                trainee_base_seed = Some(value.to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
            }
            "--next-generation" if next_generation.is_none() => {
                next_generation = Some(value.to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
            }
            "--slot-identities" if slot_identities.is_none() => {
                slot_identities = Some(PathBuf::from(value));
            }
            "--output" if output.is_none() => {
                output = Some(PathBuf::from(value));
            }
            _ => return Err(()),
        }
        index += 2;
    }

    let mode = match (genesis, chain_dir, panel, next_generation) {
        (true, None, None, None) => RefreshModeV1::Genesis,
        (false, Some(chain_dir), Some(panel), Some(next_generation)) => {
            RefreshModeV1::NextRefresh {
                chain_dir,
                panel,
                next_generation,
            }
        }
        // Genesis combined with any next-refresh flag, or a next-refresh
        // request missing one of its three required flags, is contradictory
        // or incomplete usage -- fail closed rather than guessing intent.
        _ => return Err(()),
    };

    Ok(ParsedArgsV1 {
        mode,
        trainee_run_sha256: trainee_run_sha256.ok_or(())?,
        trainee_base_seed: trainee_base_seed.ok_or(())?,
        slot_identities: slot_identities.ok_or(())?,
        output: output.ok_or(())?,
    })
}

fn read_file_or_exit_v1(path: &Path, flag: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| {
        eprintln!(
            "cycle4_refresh_build_v1: failed reading {flag} ({}): {error}",
            path.display()
        );
        std::process::exit(3);
    })
}

/// Reads the whole refresh chain from `chain_dir`, genesis through the tip,
/// per the fixed naming scheme (`refresh-NN.manifest.json`, plus
/// `refresh-NN.panel.json` for `NN >= 1`): stops at the first missing
/// `refresh-NN.manifest.json`, so the chain must be contiguous from index 0
/// with no gaps -- a manifest present at some later index past a gap is
/// simply never reached. A directory with no `refresh-00.manifest.json` at
/// all yields an empty chain (rejected downstream by the builder as
/// `EmptyChain`; next-refresh mode always requires an existing genesis).
fn read_chain_dir_or_exit_v1(chain_dir: &Path) -> Vec<Cycle4ChainLinkV1> {
    let mut chain = Vec::new();
    for refresh_index in 0..=CHAIN_WALK_MAX_REFRESH_INDEX_V1 {
        let manifest_path = chain_dir.join(cycle4_chain_manifest_filename_v1(refresh_index));
        if !manifest_path.is_file() {
            break;
        }
        let manifest_bytes = read_file_or_exit_v1(&manifest_path, "--chain-dir manifest");
        let panel_bytes = if refresh_index == 0 {
            None
        } else {
            let panel_path = chain_dir.join(cycle4_chain_panel_filename_v1(refresh_index));
            Some(read_file_or_exit_v1(&panel_path, "--chain-dir panel"))
        };
        chain.push(Cycle4ChainLinkV1 {
            manifest_bytes,
            panel_bytes,
        });
    }
    chain
}

/// Writes `bytes` to `output` atomically: a create-new temporary file in
/// `output`'s own directory, written, flushed, and synced, then renamed
/// into place (`std::fs::rename` replaces any pre-existing file at
/// `output`, exactly as the plain `std::fs::write` this replaces already
/// did -- only the atomicity and the never-partial guarantee are new). The
/// temporary is removed on any failure (create, write, sync, or rename) so
/// a failed run never leaves a partial manifest at `output`.
fn write_output_atomically_or_exit_v1(output: &Path, bytes: &[u8]) {
    let parent = output.parent().filter(|path| !path.as_os_str().is_empty());
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let file_name = output.file_name().map_or_else(
        || "cycle4-refresh-manifest".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let temp_path = parent.join(format!("{file_name}.tmp-{}", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temp_path);
        eprintln!(
            "cycle4_refresh_build_v1: failed writing --output ({}): {error}",
            output.display()
        );
        std::process::exit(3);
    }

    if let Err(error) = std::fs::rename(&temp_path, output) {
        let _ = std::fs::remove_file(&temp_path);
        eprintln!(
            "cycle4_refresh_build_v1: failed writing --output ({}): {error}",
            output.display()
        );
        std::process::exit(3);
    }
}

fn main() {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let args = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());

    let slot_identities_bytes = read_file_or_exit_v1(&args.slot_identities, "--slot-identities");

    let build_result: Result<Cycle4RefreshBuildResultV1, _> = match &args.mode {
        RefreshModeV1::Genesis => build_cycle4_genesis_refresh_v1(
            &args.trainee_run_sha256,
            args.trainee_base_seed,
            &slot_identities_bytes,
        ),
        RefreshModeV1::NextRefresh {
            chain_dir,
            panel,
            next_generation,
        } => {
            let chain = read_chain_dir_or_exit_v1(chain_dir);
            let panel_bytes = read_file_or_exit_v1(panel, "--panel");
            build_cycle4_next_refresh_v1(
                &chain,
                &panel_bytes,
                *next_generation,
                &args.trainee_run_sha256,
                args.trainee_base_seed,
                &slot_identities_bytes,
            )
        }
    };

    let result = build_result.unwrap_or_else(|error| {
        eprintln!("cycle4_refresh_build_v1: manifest rejected: {error}");
        std::process::exit(3);
    });

    write_output_atomically_or_exit_v1(&args.output, &result.canonical_bytes);

    println!(
        "refresh_index={} trainee_local_generation={} manifest_sha256={}",
        result.refresh_index, result.trainee_local_generation, result.manifest_sha256
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_v1(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    /// A fresh, uniquely-named directory under the OS temp root, created for
    /// one test's exclusive use; the caller removes it when done (matching
    /// the convention `native_checkpoint_shadow_stdio_v1.rs`'s tests use).
    fn fresh_temp_dir_v1(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mtg-kernel-cycle4-refresh-build-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root).expect("create temp dir");
        root
    }

    #[test]
    fn genesis_mode_parses_and_forbids_refresh_flags_v1() {
        let parsed = parse_args_v1(args_v1(&[
            "--genesis",
            "--trainee-run-sha256",
            "ab",
            "--trainee-base-seed",
            "977002",
            "--slot-identities",
            "slots.json",
            "--output",
            "out.json",
        ]))
        .expect("genesis parses");
        assert!(matches!(parsed.mode, RefreshModeV1::Genesis));
        assert_eq!(parsed.trainee_base_seed, 977_002);

        assert!(parse_args_v1(args_v1(&[
            "--genesis",
            "--chain-dir",
            "chain",
            "--trainee-run-sha256",
            "ab",
            "--trainee-base-seed",
            "977002",
            "--slot-identities",
            "slots.json",
            "--output",
            "out.json",
        ]))
        .is_err());
    }

    #[test]
    fn next_refresh_mode_requires_all_three_refresh_flags_v1() {
        let parsed = parse_args_v1(args_v1(&[
            "--chain-dir",
            "chain",
            "--panel",
            "panel.json",
            "--next-generation",
            "1",
            "--trainee-run-sha256",
            "ab",
            "--trainee-base-seed",
            "977002",
            "--slot-identities",
            "slots.json",
            "--output",
            "out.json",
        ]))
        .expect("next refresh parses");
        assert!(matches!(
            parsed.mode,
            RefreshModeV1::NextRefresh {
                next_generation: 1,
                ..
            }
        ));

        assert!(parse_args_v1(args_v1(&[
            "--chain-dir",
            "chain",
            "--panel",
            "panel.json",
            "--trainee-run-sha256",
            "ab",
            "--trainee-base-seed",
            "977002",
            "--slot-identities",
            "slots.json",
            "--output",
            "out.json",
        ]))
        .is_err());

        assert!(parse_args_v1(args_v1(&[
            "--trainee-run-sha256",
            "ab",
            "--trainee-base-seed",
            "977002",
            "--slot-identities",
            "slots.json",
            "--output",
            "out.json",
        ]))
        .is_err());
    }

    #[test]
    fn rejects_duplicate_unknown_and_malformed_numeric_flags_v1() {
        assert!(parse_args_v1(args_v1(&[
            "--genesis",
            "--genesis",
            "--trainee-run-sha256",
            "ab",
            "--trainee-base-seed",
            "1",
            "--slot-identities",
            "s.json",
            "--output",
            "o.json",
        ]))
        .is_err());
        assert!(parse_args_v1(args_v1(&[
            "--genesis",
            "--trainee-run-sha256",
            "ab",
            "--trainee-run-sha256",
            "cd",
            "--trainee-base-seed",
            "1",
            "--slot-identities",
            "s.json",
            "--output",
            "o.json",
        ]))
        .is_err());
        assert!(parse_args_v1(args_v1(&[
            "--genesis",
            "--not-a-real-flag",
            "x",
            "--trainee-run-sha256",
            "ab",
            "--trainee-base-seed",
            "1",
            "--slot-identities",
            "s.json",
            "--output",
            "o.json",
        ]))
        .is_err());
        assert!(parse_args_v1(args_v1(&[
            "--genesis",
            "--trainee-run-sha256",
            "ab",
            "--trainee-base-seed",
            "not-a-number",
            "--slot-identities",
            "s.json",
            "--output",
            "o.json",
        ]))
        .is_err());
        // A flag with no trailing value is a truncated command line.
        assert!(parse_args_v1(args_v1(&["--genesis", "--trainee-run-sha256"])).is_err());
    }

    #[test]
    fn rejects_missing_required_flags_v1() {
        assert!(parse_args_v1(args_v1(&["--genesis"])).is_err());
        assert!(parse_args_v1(args_v1(&[])).is_err());
    }

    #[test]
    fn read_chain_dir_reads_genesis_and_later_links_v1() {
        let root = fresh_temp_dir_v1("chain-happy");
        std::fs::write(root.join("refresh-00.manifest.json"), b"genesis-bytes")
            .expect("write genesis");
        std::fs::write(root.join("refresh-01.manifest.json"), b"refresh-one-bytes")
            .expect("write refresh 1");
        std::fs::write(root.join("refresh-01.panel.json"), b"panel-one-bytes")
            .expect("write panel 1");

        let chain = read_chain_dir_or_exit_v1(&root);

        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].manifest_bytes, b"genesis-bytes");
        assert!(chain[0].panel_bytes.is_none());
        assert_eq!(chain[1].manifest_bytes, b"refresh-one-bytes");
        assert_eq!(
            chain[1].panel_bytes.as_deref(),
            Some(&b"panel-one-bytes"[..])
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn read_chain_dir_stops_at_the_first_gap_v1() {
        let root = fresh_temp_dir_v1("chain-gap");
        std::fs::write(root.join("refresh-00.manifest.json"), b"genesis-bytes")
            .expect("write genesis");
        // refresh-01 is missing entirely; refresh-02 exists but must never
        // be read since the walk is contiguous from genesis.
        std::fs::write(root.join("refresh-02.manifest.json"), b"orphan-bytes")
            .expect("write orphan");

        let chain = read_chain_dir_or_exit_v1(&root);

        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].manifest_bytes, b"genesis-bytes");

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn read_chain_dir_on_an_empty_directory_yields_an_empty_chain_v1() {
        let root = fresh_temp_dir_v1("chain-empty");
        assert!(read_chain_dir_or_exit_v1(&root).is_empty());
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn write_output_atomically_writes_exact_bytes_and_leaves_no_temp_file_v1() {
        let root = fresh_temp_dir_v1("output-write");
        let output = root.join("refresh-01.manifest.json");

        write_output_atomically_or_exit_v1(&output, b"final-manifest-bytes");

        assert_eq!(
            std::fs::read(&output).expect("read output"),
            b"final-manifest-bytes"
        );
        let leftover: Vec<_> = std::fs::read_dir(&root)
            .expect("list dir")
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(
            leftover.is_empty(),
            "no temporary file should remain after a successful write"
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn write_output_atomically_replaces_an_existing_file_v1() {
        let root = fresh_temp_dir_v1("output-replace");
        let output = root.join("refresh-01.manifest.json");
        std::fs::write(&output, b"stale-bytes").expect("seed stale output");

        write_output_atomically_or_exit_v1(&output, b"fresh-manifest-bytes");

        assert_eq!(
            std::fs::read(&output).expect("read output"),
            b"fresh-manifest-bytes"
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
