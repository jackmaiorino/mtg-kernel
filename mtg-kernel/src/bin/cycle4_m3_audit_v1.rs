//! Cycle-4 M3 centering audit CLI (section-6 mechanical amendment V2,
//! section A). One invocation produces ONE canonical document and exits.
//!
//! Two modes, both read-only. Neither opens a Store handle, takes a Store
//! lock, or writes anything except its own `--output`.
//!
//! `--mode reference` walks the cycle-3 focal Store's final 512 updates on
//! the RAW residual `target - value` and publishes the reference dispersion
//! statistic as `mtg-kernel-cycle4-m3-reference/v2`. This mode exists
//! because the ratified audit artifact the amendment cites
//! (`OX_ADVANTAGE_BY_ROLE_AUDIT_RESULT_V1.md`) records per-role and
//! per-slot MEANS and winrates and no per-cell standard deviation at all, so
//! it cannot supply the dispersion reference; "the same statistic computed
//! on the cycle-3 focal store's final 512 updates" has to be computed, and
//! this is the code that computes it, the same way, from the same evidence
//! shape. `--audit-note` binds that ratified note's bytes by SHA-256 into
//! the reference document for provenance.
//!
//! `--mode audit` walks one v4 arm's Store and baseline chain over the same
//! window on the CENTERED residual `(target - value) - c_t`, applies the
//! amendment's total function against a reference document, and publishes
//! `mtg-kernel-cycle4-m3-audit/v2` carrying every number, every input hash,
//! and the verdict.
//!
//! Strict flag parsing follows `cycle4_run_record_v1.rs`: order-independent
//! name/value pairs, no positional arguments, every flag at most once, no
//! environment variable read for configuration, no defaults for anything
//! that identifies an input.
//!
//! Exit codes: 0 a report was published (PASS or FAIL alike -- a FAIL
//! verdict is a result, not a failure of this program, and the record is the
//! authority the routing selector reads). 2 usage. 3 everything else once
//! the command line is well formed: an unreadable Store, a broken sidecar
//! chain, a rejected reference document, or a refused overwrite. The
//! governing contract names only 0/2/3 for launcher-level bins, so I/O
//! failure folds into 3 alongside content rejection.

use mtg_kernel::durable_move_publication_v2::publish_immutable_file_by_move_v2;
use mtg_kernel::durable_publication_v1::{
    capture_existing_publication_parent_v1, DurableFileExpectationV1,
};
use mtg_kernel::native_cycle4_m3_audit_v1::{
    build_cycle4_m3_audit_report_v1, build_cycle4_m3_reference_document_v1,
    compute_cycle4_m3_window_v1, decode_cycle4_m3_reference_document_v1, Cycle4M3ResidualModeV1,
    Cycle4M3WindowRequestV1, CYCLE4_M3_WINDOW_UPDATES_V1,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn usage_v1() -> ! {
    eprintln!(
        "usage: cycle4_m3_audit_v1 --mode reference --store-root PATH --audit-note PATH --output PATH\n\
         \x20      cycle4_m3_audit_v1 --mode audit --arm (static-rb|treatment-rb) --store-root PATH --chain-dir PATH --reference-document PATH --output PATH"
    );
    std::process::exit(2);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModeV1 {
    Reference,
    Audit,
}

struct ParsedArgsV1 {
    mode: ModeV1,
    arm: Option<String>,
    store_root: PathBuf,
    chain_dir: Option<PathBuf>,
    reference_document: Option<PathBuf>,
    audit_note: Option<PathBuf>,
    output: PathBuf,
}

/// The two arm kinds whose eligibility section A decides. CONTROL-R is a v3
/// arm and "is always eligible", so it has no M3 report and this bin refuses
/// to produce one for it rather than emitting a meaningless document.
const AUDITABLE_ARMS_V1: [&str; 2] = ["static-rb", "treatment-rb"];

fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    if raw.is_empty() {
        return Err(());
    }
    let mut mode: Option<ModeV1> = None;
    let mut arm: Option<String> = None;
    let mut store_root: Option<PathBuf> = None;
    let mut chain_dir: Option<PathBuf> = None;
    let mut reference_document: Option<PathBuf> = None;
    let mut audit_note: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;

    let mut index = 0;
    while index < raw.len() {
        let flag = raw[index].to_str().ok_or(())?;
        let value = raw.get(index + 1).ok_or(())?;
        match flag {
            "--mode" if mode.is_none() => {
                mode = Some(match value.to_str().ok_or(())? {
                    "reference" => ModeV1::Reference,
                    "audit" => ModeV1::Audit,
                    _ => return Err(()),
                });
            }
            "--arm" if arm.is_none() => {
                let value = value.to_str().ok_or(())?;
                if !AUDITABLE_ARMS_V1.contains(&value) {
                    return Err(());
                }
                arm = Some(value.to_owned());
            }
            "--store-root" if store_root.is_none() => store_root = Some(PathBuf::from(value)),
            "--chain-dir" if chain_dir.is_none() => chain_dir = Some(PathBuf::from(value)),
            "--reference-document" if reference_document.is_none() => {
                reference_document = Some(PathBuf::from(value));
            }
            "--audit-note" if audit_note.is_none() => audit_note = Some(PathBuf::from(value)),
            "--output" if output.is_none() => output = Some(PathBuf::from(value)),
            _ => return Err(()),
        }
        index += 2;
    }

    let mode = mode.ok_or(())?;
    let parsed = ParsedArgsV1 {
        mode,
        arm,
        store_root: store_root.ok_or(())?,
        chain_dir,
        reference_document,
        audit_note,
        output: output.ok_or(())?,
    };
    // Mode-specific shape: a flag that means nothing in this mode is a usage
    // error, never silently ignored.
    match mode {
        ModeV1::Reference => {
            // `--audit-note` is REQUIRED, not optional: clarification V2.1
            // binds the ratified note's bytes into the reference document,
            // and the routing selector refuses a report whose reference did
            // not carry one, so a reference published without it is dead on
            // arrival.
            if parsed.arm.is_some()
                || parsed.chain_dir.is_some()
                || parsed.reference_document.is_some()
                || parsed.audit_note.is_none()
            {
                return Err(());
            }
        }
        ModeV1::Audit => {
            if parsed.arm.is_none()
                || parsed.chain_dir.is_none()
                || parsed.reference_document.is_none()
                || parsed.audit_note.is_some()
            {
                return Err(());
            }
        }
    }
    Ok(parsed)
}

fn fail_v1(detail: impl std::fmt::Display) -> ! {
    eprintln!("cycle4_m3_audit_v1: {detail}");
    std::process::exit(3);
}

fn lower_hex_v1(digest: [u8; 32]) -> String {
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// Publishes `bytes` at `output` through the durable create-new move
/// primitive. These documents are immutable by design: an existing file with
/// identical bytes is a no-op success (the replay case), and an existing
/// file with different bytes is refused. There is deliberately no --force:
/// a reference document or an audit report is an input the routing record
/// binds by hash, and replacing one under a published record would re-key
/// the freeze.
fn publish_or_exit_v1(output: &Path, bytes: &[u8]) {
    if let Ok(existing) = std::fs::read(output) {
        if existing == bytes {
            return;
        }
        fail_v1(format!(
            "{} already holds a DIFFERENT document; these artifacts are immutable, so publish to a \
             fresh path rather than replacing one a routing record may already bind",
            output.display()
        ));
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let final_name = output
        .file_name()
        .unwrap_or_else(|| fail_v1(format!("--output names no file: {}", output.display())));
    let stage_name = format!(
        "{}.stage-{}",
        final_name.to_string_lossy(),
        std::process::id()
    );
    let captured =
        capture_existing_publication_parent_v1(parent).unwrap_or_else(|error| fail_v1(error));
    let expectation =
        DurableFileExpectationV1::from_bytes(bytes).unwrap_or_else(|error| fail_v1(error));
    let staged = parent.join(&stage_name);
    if staged.exists() {
        let _ = std::fs::remove_file(&staged);
    }
    if let Err(error) =
        publish_immutable_file_by_move_v2(&captured, &stage_name, final_name, bytes, expectation)
    {
        let _ = std::fs::remove_file(&staged);
        fail_v1(error);
    }
}

fn main() {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let args = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());

    let residual_mode = match args.mode {
        ModeV1::Reference => Cycle4M3ResidualModeV1::Raw,
        ModeV1::Audit => Cycle4M3ResidualModeV1::Centered,
    };
    let window = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
        store_root: args.store_root.clone(),
        chain_dir: args.chain_dir.clone(),
        residual_mode,
        window_updates: CYCLE4_M3_WINDOW_UPDATES_V1,
    })
    .unwrap_or_else(|error| fail_v1(error));

    match args.mode {
        ModeV1::Reference => {
            let note_path = args.audit_note.as_ref().unwrap_or_else(|| usage_v1());
            let note_bytes = std::fs::read(note_path)
                .unwrap_or_else(|error| fail_v1(format!("{}: {error}", note_path.display())));
            let audit_note_sha256 = lower_hex_v1(
                DurableFileExpectationV1::from_bytes(&note_bytes)
                    .unwrap_or_else(|error| fail_v1(error))
                    .sha256(),
            );
            let bytes = build_cycle4_m3_reference_document_v1(&window, audit_note_sha256)
                .unwrap_or_else(|error| fail_v1(error));
            publish_or_exit_v1(&args.output, &bytes);
            let digest = lower_hex_v1(
                DurableFileExpectationV1::from_bytes(&bytes)
                    .unwrap_or_else(|error| fail_v1(error))
                    .sha256(),
            );
            println!(
                "mode=reference window={}..{} decisions={} cells={} reference_sha256={digest}",
                window.first_update_index(),
                window.last_update_index(),
                window.decision_count(),
                window.cells().len()
            );
        }
        ModeV1::Audit => {
            let reference_path = args
                .reference_document
                .as_ref()
                .unwrap_or_else(|| usage_v1());
            let reference_bytes = std::fs::read(reference_path)
                .unwrap_or_else(|error| fail_v1(format!("{}: {error}", reference_path.display())));
            let reference_sha256 = lower_hex_v1(
                DurableFileExpectationV1::from_bytes(&reference_bytes)
                    .unwrap_or_else(|error| fail_v1(error))
                    .sha256(),
            );
            let reference = decode_cycle4_m3_reference_document_v1(&reference_bytes)
                .unwrap_or_else(|error| fail_v1(error));
            let arm = args.arm.as_deref().unwrap_or_else(|| usage_v1());
            let (bytes, pass) =
                build_cycle4_m3_audit_report_v1(arm, &window, &reference, &reference_sha256)
                    .unwrap_or_else(|error| fail_v1(error));
            publish_or_exit_v1(&args.output, &bytes);
            let digest = lower_hex_v1(
                DurableFileExpectationV1::from_bytes(&bytes)
                    .unwrap_or_else(|error| fail_v1(error))
                    .sha256(),
            );
            println!(
                "mode=audit arm={arm} window={}..{} decisions={} cells={} verdict={} report_sha256={digest}",
                window.first_update_index(),
                window.last_update_index(),
                window.decision_count(),
                window.cells().len(),
                if pass { "PASS" } else { "FAIL" }
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_v1(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_both_modes_v1() {
        let reference = parse_args_v1(args_v1(&[
            "--mode",
            "reference",
            "--store-root",
            "E:\\cycle3\\store",
            "--output",
            "reference.json",
            "--audit-note",
            "note.md",
        ]))
        .expect("reference mode parses");
        assert_eq!(reference.mode, ModeV1::Reference);
        assert_eq!(reference.audit_note, Some(PathBuf::from("note.md")));

        let audit = parse_args_v1(args_v1(&[
            "--mode",
            "audit",
            "--arm",
            "treatment-rb",
            "--store-root",
            "E:\\cycle4\\treatment-rb\\store",
            "--chain-dir",
            "E:\\cycle4\\treatment-rb\\baseline-chain",
            "--reference-document",
            "reference.json",
            "--output",
            "m3.json",
        ]))
        .expect("audit mode parses");
        assert_eq!(audit.mode, ModeV1::Audit);
        assert_eq!(audit.arm.as_deref(), Some("treatment-rb"));
        assert!(audit.chain_dir.is_some());
    }

    #[test]
    fn rejects_wrong_shapes_v1() {
        // control-r has no M3 report: it is always eligible.
        assert!(parse_args_v1(args_v1(&[
            "--mode",
            "audit",
            "--arm",
            "control-r",
            "--store-root",
            "s",
            "--chain-dir",
            "c",
            "--reference-document",
            "r",
            "--output",
            "o",
        ]))
        .is_err());
        // audit without a chain directory.
        assert!(parse_args_v1(args_v1(&[
            "--mode",
            "audit",
            "--arm",
            "static-rb",
            "--store-root",
            "s",
            "--reference-document",
            "r",
            "--output",
            "o",
        ]))
        .is_err());
        // reference mode carrying an audit-only flag.
        assert!(parse_args_v1(args_v1(&[
            "--mode",
            "reference",
            "--store-root",
            "s",
            "--chain-dir",
            "c",
            "--audit-note",
            "n",
            "--output",
            "o",
        ]))
        .is_err());
        // reference mode WITHOUT the audit note: clarification V2.1 binds
        // the note's bytes, and routing refuses a reference that lacks one,
        // so publishing one without it is a usage error here.
        assert!(parse_args_v1(args_v1(&[
            "--mode",
            "reference",
            "--store-root",
            "s",
            "--output",
            "o",
        ]))
        .is_err());
        // duplicated flag, unknown flag, unknown mode, truncated, empty.
        assert!(parse_args_v1(args_v1(&[
            "--mode",
            "reference",
            "--mode",
            "audit",
            "--store-root",
            "s",
            "--output",
            "o",
        ]))
        .is_err());
        assert!(parse_args_v1(args_v1(&[
            "--mode",
            "reference",
            "--nope",
            "x",
            "--store-root",
            "s",
            "--output",
            "o",
        ]))
        .is_err());
        assert!(parse_args_v1(args_v1(&[
            "--mode",
            "gate",
            "--store-root",
            "s",
            "--output",
            "o",
        ]))
        .is_err());
        assert!(parse_args_v1(args_v1(&["--mode"])).is_err());
        assert!(parse_args_v1(args_v1(&[])).is_err());
    }
}
