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
//! `--output` is published through the repository's durable move
//! primitives (`durable_move_publication_v2`), not a hand-rolled rename: an
//! absent destination is a create-new immutable publication, and a forced
//! replacement is `replace_file_by_move_v2`, which is the Windows
//! `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` path.
//! A plain `std::fs::rename` does NOT replace an existing destination on
//! Windows, so `--force` over a differing record would otherwise always
//! fail. An existing `--output` is NOT
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

use mtg_kernel::durable_move_publication_v2::{
    publish_immutable_file_by_move_v2, replace_file_by_move_v2,
};
use mtg_kernel::durable_publication_v1::{
    capture_existing_publication_parent_v1, DurableFileExpectationV1,
};
use mtg_kernel::native_cycle4_arm_v1::Cycle4ArmKindV1;
use mtg_kernel::native_cycle4_run_record_v1::{
    build_cycle4_arm_run_record_v1, Cycle4RunRecordRequestV1,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn usage_v1() -> ! {
    eprintln!(
        "usage: cycle4_run_record_v1 --arm (control-r|static-rb|treatment-rb) --parent-store-root PATH --parent-generation N --arm-executable PATH --output PATH [--force]"
    );
    std::process::exit(2);
}

struct ParsedArgsV1 {
    arm: Cycle4ArmKindV1,
    parent_store_root: PathBuf,
    parent_generation: u64,
    arm_executable: PathBuf,
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
    let mut arm_executable: Option<PathBuf> = None;
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
            "--arm-executable" if arm_executable.is_none() => {
                arm_executable = Some(PathBuf::from(value));
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
        arm_executable: arm_executable.ok_or(())?,
        output: output.ok_or(())?,
        force,
    })
}

/// Publishes `bytes` at `output` through the repository's durable move
/// primitives.
///
/// `replacing` picks which one: a create-new immutable publication when the
/// destination is absent, and `replace_file_by_move_v2` when a forced
/// replacement is called for. The distinction is not cosmetic on Windows --
/// `std::fs::rename` maps to a no-replace move, so a forced overwrite
/// written that way always fails -- and the replace primitive additionally
/// stages, verifies the exact length and digest, and write-throughs the
/// move, which a hand-rolled rename does not.
fn publish_output_or_exit_v1(output: &Path, bytes: &[u8], replacing: bool) {
    let parent = output.parent().filter(|path| !path.as_os_str().is_empty());
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let final_name = output.file_name().unwrap_or_else(|| {
        eprintln!(
            "cycle4_run_record_v1: --output names no file: {}",
            output.display()
        );
        std::process::exit(3);
    });
    let stage_name = format!(
        "{}.stage-{}",
        final_name.to_string_lossy(),
        std::process::id()
    );

    let fail = |detail: String| -> ! {
        eprintln!(
            "cycle4_run_record_v1: failed writing --output ({}): {detail}",
            output.display()
        );
        std::process::exit(3);
    };

    let captured = capture_existing_publication_parent_v1(parent)
        .unwrap_or_else(|error| fail(error.to_string()));
    let expectation =
        DurableFileExpectationV1::from_bytes(bytes).unwrap_or_else(|error| fail(error.to_string()));

    // A staging file left by an interrupted attempt would make the
    // create-new publication below collide with its own debris.
    let staged = parent.join(&stage_name);
    if staged.exists() {
        let _ = std::fs::remove_file(&staged);
    }

    let result = if replacing {
        replace_file_by_move_v2(&captured, &stage_name, final_name, bytes, expectation).map(|_| ())
    } else {
        publish_immutable_file_by_move_v2(&captured, &stage_name, final_name, bytes, expectation)
            .map(|_| ())
    };
    if let Err(error) = result {
        let _ = std::fs::remove_file(&staged);
        fail(error.to_string());
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
        arm_executable: args.arm_executable.clone(),
    })
    .unwrap_or_else(|error| {
        eprintln!("cycle4_run_record_v1: {error}");
        std::process::exit(3);
    });

    let existing = std::fs::read(&args.output).ok();
    match overwrite_decision_v1(existing.as_deref(), &outcome.canonical_bytes, args.force) {
        Some(true) => {
            publish_output_or_exit_v1(&args.output, &outcome.canonical_bytes, existing.is_some())
        }
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

    fn required_flags_v1() -> Vec<&'static str> {
        vec![
            "--arm",
            "treatment-rb",
            "--parent-store-root",
            "E:\\parent\\store",
            "--parent-generation",
            "896",
            "--arm-executable",
            "D:\\release\\cycle4_arm_v1.exe",
            "--output",
            "run.json",
        ]
    }

    #[test]
    fn parses_every_required_flag_v1() {
        let parsed =
            parse_args_v1(args_v1(&required_flags_v1())).expect("well-formed command line parses");
        assert_eq!(parsed.arm, Cycle4ArmKindV1::TreatmentRb);
        assert_eq!(parsed.parent_generation, 896);
        assert!(!parsed.force);
        assert_eq!(
            parsed.arm_executable,
            PathBuf::from("D:\\release\\cycle4_arm_v1.exe")
        );
        assert_eq!(parsed.output, PathBuf::from("run.json"));
    }

    /// The arm launcher is required: without it the record could only
    /// inherit somebody else's provenance, which is the defect this flag
    /// exists to close.
    #[test]
    fn the_arm_executable_is_required_v1() {
        let mut flags = required_flags_v1();
        let index = flags
            .iter()
            .position(|flag| *flag == "--arm-executable")
            .expect("flag present");
        flags.drain(index..index + 2);
        assert!(parse_args_v1(args_v1(&flags)).is_err());
    }

    /// A create-new publication, then a FORCED replacement of a differing
    /// file, both through the durable primitives. On Windows a plain
    /// `std::fs::rename` cannot replace an existing destination, so this is
    /// the case that used to always exit 3.
    #[test]
    fn a_forced_replacement_replaces_a_differing_file_v1() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mtg-kernel-cycle4-run-record-force-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root).expect("create temp dir");
        let output = root.join("run.json");

        publish_output_or_exit_v1(&output, b"first-record-bytes", false);
        assert_eq!(
            std::fs::read(&output).expect("read output"),
            b"first-record-bytes"
        );

        publish_output_or_exit_v1(&output, b"second-record-bytes", true);
        assert_eq!(
            std::fs::read(&output).expect("read replaced output"),
            b"second-record-bytes"
        );

        let leftover: Vec<_> = std::fs::read_dir(&root)
            .expect("list dir")
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".stage-"))
            .collect();
        assert!(
            leftover.is_empty(),
            "no staging file should remain after a publication"
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
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
            "--arm-executable",
            "a.exe",
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
