//! Cycle-4 arm run-record CLI (`docs/native_cycle4_arm_launcher_v1.md`
//! Section 1). One invocation builds ONE arm's `run.json` and exits.
//!
//! Everything the record contains comes from the arm kind, the pinned parent
//! Store, and the compiled cycle-4 literals; there are no defaults, no
//! optional content flags, and no environment variable is read for
//! configuration. Two invocations with the same flags against the same
//! parent Store therefore write byte-identical output, which is what makes
//! the printed `run_sha256` a stable campaign identity rather than a
//! per-invocation accident.
//!
//! Strict flag parsing follows `cycle4_refresh_build_v1.rs`: order-
//! independent name/value pairs, no positional arguments, every flag at most
//! once, and one value-less marker (`--force`) that may appear at most once.
//!
//! `--output` is written atomically -- a create-new temporary in the
//! output's own directory, written, flushed, synced, then renamed into place
//! -- and the temporary is removed on any failure, so a failed run never
//! leaves a partial run record behind. An existing `--output` is NOT
//! overwritten unless `--force` is given: a run record is a campaign
//! identity, and silently replacing one under a running campaign would
//! re-key every manifest, locator and origin record bound to it. When the
//! existing file's bytes already equal what this build produced the run is
//! a no-op success without `--force`, since there is nothing to replace.
//!
//! Exit codes: 0 success. 2 usage (flags malformed, missing, duplicated, or
//! unparseable). 3 everything else once the command line is well-formed --
//! an unreadable parent Store, a rejected parent record, a rejected
//! assembled record, or a refused overwrite. The governing contract names
//! only 0/2/3 for the launcher-level bins, so I/O failure is folded into 3
//! alongside content rejection rather than adding a fourth code.

use mtg_kernel::native_cycle4_arm_v1::Cycle4ArmKindV1;
use mtg_kernel::native_cycle4_run_record_v1::{
    build_cycle4_arm_run_record_v1, Cycle4RunRecordRequestV1,
};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

fn usage_v1() -> ! {
    eprintln!(
        "usage: cycle4_run_record_v1 --arm (control-r|static-rb|treatment-rb) --parent-store-root PATH --parent-generation N --output PATH [--force]"
    );
    std::process::exit(2);
}

struct ParsedArgsV1 {
    arm: Cycle4ArmKindV1,
    parent_store_root: PathBuf,
    parent_generation: u64,
    output: PathBuf,
    force: bool,
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    if raw.is_empty() {
        return Err(());
    }
    let mut arm: Option<Cycle4ArmKindV1> = None;
    let mut parent_store_root: Option<PathBuf> = None;
    let mut parent_generation: Option<u64> = None;
    let mut output: Option<PathBuf> = None;
    let mut force = false;

    let mut index = 0;
    while index < raw.len() {
        let flag = raw[index].to_str().ok_or(())?;
        if flag == "--force" {
            if force {
                return Err(());
            }
            force = true;
            index += 1;
            continue;
        }
        let value = raw.get(index + 1).ok_or(())?;
        match flag {
            "--arm" if arm.is_none() => {
                arm = Some(Cycle4ArmKindV1::from_wire_v1(value.to_str().ok_or(())?).ok_or(())?);
            }
            "--parent-store-root" if parent_store_root.is_none() => {
                parent_store_root = Some(PathBuf::from(value));
            }
            "--parent-generation" if parent_generation.is_none() => {
                parent_generation = Some(value.to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
            }
            "--output" if output.is_none() => {
                output = Some(PathBuf::from(value));
            }
            _ => return Err(()),
        }
        index += 2;
    }

    Ok(ParsedArgsV1 {
        arm: arm.ok_or(())?,
        parent_store_root: parent_store_root.ok_or(())?,
        parent_generation: parent_generation.ok_or(())?,
        output: output.ok_or(())?,
        force,
    })
}

/// Writes `bytes` to `output` atomically, following
/// `cycle4_refresh_build_v1.rs`'s writer exactly: a create-new temporary in
/// `output`'s own directory, written, flushed, and synced, then renamed into
/// place; the temporary is removed on any failure.
fn write_output_atomically_or_exit_v1(output: &Path, bytes: &[u8]) {
    let parent = output.parent().filter(|path| !path.as_os_str().is_empty());
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let file_name = output.file_name().map_or_else(
        || "cycle4-run-record".to_owned(),
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
            "cycle4_run_record_v1: failed writing --output ({}): {error}",
            output.display()
        );
        std::process::exit(3);
    }

    if let Err(error) = std::fs::rename(&temp_path, output) {
        let _ = std::fs::remove_file(&temp_path);
        eprintln!(
            "cycle4_run_record_v1: failed writing --output ({}): {error}",
            output.display()
        );
        std::process::exit(3);
    }
}

/// Whether the build still has to write. `Some(true)` write, `Some(false)`
/// the identical record is already there, `None` refuse.
fn overwrite_decision_v1(existing: Option<&[u8]>, built: &[u8], force: bool) -> Option<bool> {
    match existing {
        None => Some(true),
        Some(bytes) if bytes == built => Some(false),
        Some(_) if force => Some(true),
        Some(_) => None,
    }
}

fn main() {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let args = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());

    let outcome = build_cycle4_arm_run_record_v1(&Cycle4RunRecordRequestV1 {
        arm: args.arm,
        parent_store_root: args.parent_store_root.clone(),
        parent_generation: args.parent_generation,
    })
    .unwrap_or_else(|error| {
        eprintln!("cycle4_run_record_v1: {error}");
        std::process::exit(3);
    });

    let existing = std::fs::read(&args.output).ok();
    match overwrite_decision_v1(existing.as_deref(), &outcome.canonical_bytes, args.force) {
        Some(true) => write_output_atomically_or_exit_v1(&args.output, &outcome.canonical_bytes),
        Some(false) => {}
        None => {
            eprintln!(
                "cycle4_run_record_v1: {} already holds a DIFFERENT run record; refusing to re-key a campaign identity (pass --force only when no Store, chain or manifest binds the existing one)",
                args.output.display()
            );
            std::process::exit(3);
        }
    }

    println!(
        "arm={} base_seed={} run_sha256={} parent_run_sha256={} parent_generation={}",
        outcome.arm_kind,
        outcome.base_seed,
        outcome.run_sha256,
        outcome.parent_run_sha256,
        outcome.parent_generation
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_v1(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_every_required_flag_v1() {
        let parsed = parse_args_v1(args_v1(&[
            "--arm",
            "treatment-rb",
            "--parent-store-root",
            "E:\\parent\\store",
            "--parent-generation",
            "896",
            "--output",
            "run.json",
        ]))
        .expect("well-formed command line parses");
        assert_eq!(parsed.arm, Cycle4ArmKindV1::TreatmentRb);
        assert_eq!(parsed.parent_generation, 896);
        assert!(!parsed.force);
        assert_eq!(parsed.output, PathBuf::from("run.json"));
    }

    #[test]
    fn rejects_unknown_duplicate_missing_and_malformed_flags_v1() {
        // Unknown arm kind.
        assert!(parse_args_v1(args_v1(&[
            "--arm",
            "control",
            "--parent-store-root",
            "p",
            "--parent-generation",
            "896",
            "--output",
            "o",
        ]))
        .is_err());
        // Duplicated flag.
        assert!(parse_args_v1(args_v1(&[
            "--arm",
            "control-r",
            "--arm",
            "static-rb",
            "--parent-store-root",
            "p",
            "--parent-generation",
            "896",
            "--output",
            "o",
        ]))
        .is_err());
        // Duplicated marker.
        assert!(parse_args_v1(args_v1(&[
            "--force",
            "--force",
            "--arm",
            "control-r",
            "--parent-store-root",
            "p",
            "--parent-generation",
            "896",
            "--output",
            "o",
        ]))
        .is_err());
        // Unknown flag.
        assert!(parse_args_v1(args_v1(&[
            "--arm",
            "control-r",
            "--not-a-flag",
            "x",
            "--parent-store-root",
            "p",
            "--parent-generation",
            "896",
            "--output",
            "o",
        ]))
        .is_err());
        // Unparseable generation.
        assert!(parse_args_v1(args_v1(&[
            "--arm",
            "control-r",
            "--parent-store-root",
            "p",
            "--parent-generation",
            "eight-ninety-six",
            "--output",
            "o",
        ]))
        .is_err());
        // Missing required flag, truncated command line, and no arguments.
        assert!(parse_args_v1(args_v1(&["--arm", "control-r"])).is_err());
        assert!(parse_args_v1(args_v1(&["--arm"])).is_err());
        assert!(parse_args_v1(args_v1(&[])).is_err());
    }

    #[test]
    fn overwrite_is_refused_unless_identical_or_forced_v1() {
        assert_eq!(overwrite_decision_v1(None, b"new", false), Some(true));
        assert_eq!(
            overwrite_decision_v1(Some(b"new"), b"new", false),
            Some(false)
        );
        assert_eq!(overwrite_decision_v1(Some(b"old"), b"new", false), None);
        assert_eq!(
            overwrite_decision_v1(Some(b"old"), b"new", true),
            Some(true)
        );
    }
}
