//! Cycle-4 routing CLI (section-6 mechanical amendment V2, sections B to
//! D). One invocation reads the two v4 arms' M3 reports and the M2
//! common-root panel, applies the mechanical selector, and publishes the
//! immutable routing record. It plays no game, opens no Store, and touches
//! no device.
//!
//! The CP7 guard runs FIRST, before the record is even assembled: section D
//! requires the routing output to be written before any M1 CP7 byte becomes
//! readable, and `--cp7-evidence-root` is the mechanical half of that. It
//! must name an existing directory, and that directory must hold no outcome
//! artifact naming any of the four endpoints. See
//! `native_cycle4_routing_v1::assert_cp7_evidence_absent_v1` for what the
//! guard looks at and what it cannot see.
//!
//! `--cycle3-g2048-run-sha256` and
//! `--cycle3-g2048-checkpoint-manifest-sha256` pin the NO CARRY parent: "any
//! next cycle starts from the cycle-3 g2048 checkpoint under the CONTROL-R
//! recipe". They are required on every invocation, not only the NO CARRY
//! branch, so the published record always states what the fallback would
//! have been, and are recorded verbatim.
//!
//! Strict flag parsing follows `cycle4_run_record_v1.rs`: order-independent
//! name/value pairs, no positional arguments, every flag at most once, no
//! environment variable read for configuration.
//!
//! Exit codes: 0 the routing record was published. 2 usage. 3 everything
//! else once the command line is well formed: a rejected panel or report, a
//! statistic that does not follow from the panel's own root table, a CP7
//! artifact already present, or a refused overwrite.

use mtg_kernel::durable_move_publication_v2::publish_immutable_file_by_move_v2;
use mtg_kernel::durable_publication_v1::{
    capture_existing_publication_parent_v1, DurableFileExpectationV1,
};
use mtg_kernel::native_cycle4_routing_v1::{
    assert_cp7_evidence_absent_v1, cp7_guard_identities_v1, decide_cycle4_routing_v1,
    decode_cycle4_m2_panel_v1, decode_cycle4_routing_record_v1, Cycle4RoutingInputsV1,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn usage_v1() -> ! {
    eprintln!(
        "usage: cycle4_routing_v1 --m2-panel PATH --m3-report-static-rb PATH \
         --m3-report-treatment-rb PATH --reference-document PATH --cp7-evidence-root PATH \
         --cycle3-g2048-run-sha256 HEX64 --cycle3-g2048-checkpoint-manifest-sha256 HEX64 \
         --output PATH"
    );
    std::process::exit(2);
}

struct ParsedArgsV1 {
    m2_panel: PathBuf,
    m3_report_static_rb: PathBuf,
    m3_report_treatment_rb: PathBuf,
    reference_document: PathBuf,
    cp7_evidence_root: PathBuf,
    cycle3_g2048_run_sha256: String,
    cycle3_g2048_checkpoint_manifest_sha256: String,
    output: PathBuf,
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    if raw.is_empty() {
        return Err(());
    }
    let mut m2_panel: Option<PathBuf> = None;
    let mut m3_report_static_rb: Option<PathBuf> = None;
    let mut m3_report_treatment_rb: Option<PathBuf> = None;
    let mut reference_document: Option<PathBuf> = None;
    let mut cp7_evidence_root: Option<PathBuf> = None;
    let mut cycle3_run: Option<String> = None;
    let mut cycle3_checkpoint: Option<String> = None;
    let mut output: Option<PathBuf> = None;

    let mut index = 0;
    while index < raw.len() {
        let flag = raw[index].to_str().ok_or(())?;
        let value = raw.get(index + 1).ok_or(())?;
        match flag {
            "--m2-panel" if m2_panel.is_none() => m2_panel = Some(PathBuf::from(value)),
            "--m3-report-static-rb" if m3_report_static_rb.is_none() => {
                m3_report_static_rb = Some(PathBuf::from(value));
            }
            "--m3-report-treatment-rb" if m3_report_treatment_rb.is_none() => {
                m3_report_treatment_rb = Some(PathBuf::from(value));
            }
            "--reference-document" if reference_document.is_none() => {
                reference_document = Some(PathBuf::from(value));
            }
            "--cp7-evidence-root" if cp7_evidence_root.is_none() => {
                cp7_evidence_root = Some(PathBuf::from(value));
            }
            "--cycle3-g2048-run-sha256" if cycle3_run.is_none() => {
                cycle3_run = Some(value.to_str().ok_or(())?.to_owned());
            }
            "--cycle3-g2048-checkpoint-manifest-sha256" if cycle3_checkpoint.is_none() => {
                cycle3_checkpoint = Some(value.to_str().ok_or(())?.to_owned());
            }
            "--output" if output.is_none() => output = Some(PathBuf::from(value)),
            _ => return Err(()),
        }
        index += 2;
    }

    Ok(ParsedArgsV1 {
        m2_panel: m2_panel.ok_or(())?,
        m3_report_static_rb: m3_report_static_rb.ok_or(())?,
        m3_report_treatment_rb: m3_report_treatment_rb.ok_or(())?,
        reference_document: reference_document.ok_or(())?,
        cp7_evidence_root: cp7_evidence_root.ok_or(())?,
        cycle3_g2048_run_sha256: cycle3_run.ok_or(())?,
        cycle3_g2048_checkpoint_manifest_sha256: cycle3_checkpoint.ok_or(())?,
        output: output.ok_or(())?,
    })
}

fn fail_v1(detail: impl std::fmt::Display) -> ! {
    eprintln!("cycle4_routing_v1: {detail}");
    std::process::exit(3);
}

fn read_or_exit_v1(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| fail_v1(format!("{}: {error}", path.display())))
}

fn lower_hex_v1(digest: [u8; 32]) -> String {
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// The routing record is immutable: an identical file already at `output` is
/// a no-op success (the replay case), a different one is refused, and there
/// is no --force. Section D calls this "an immutable, content-hashed lane
/// record"; a flag that replaced one would defeat the freeze it exists to
/// establish.
fn publish_or_exit_v1(output: &Path, bytes: &[u8]) {
    if let Ok(existing) = std::fs::read(output) {
        if existing == bytes {
            return;
        }
        fail_v1(format!(
            "{} already holds a DIFFERENT routing record; the routing record is the cycle's freeze \
             artifact and is never replaced",
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

    let panel_bytes = read_or_exit_v1(&args.m2_panel);

    // The guard runs before anything is decided, so a run that would have
    // produced a record after CP7 evidence existed produces nothing at all.
    let panel = decode_cycle4_m2_panel_v1(&panel_bytes).unwrap_or_else(|error| fail_v1(error));
    let identities = cp7_guard_identities_v1(&panel);
    assert_cp7_evidence_absent_v1(&args.cp7_evidence_root, &identities)
        .unwrap_or_else(|error| fail_v1(error));

    let bytes = decide_cycle4_routing_v1(&Cycle4RoutingInputsV1 {
        m2_panel_bytes: panel_bytes,
        m3_static_rb_bytes: read_or_exit_v1(&args.m3_report_static_rb),
        m3_treatment_rb_bytes: read_or_exit_v1(&args.m3_report_treatment_rb),
        reference_document_bytes: read_or_exit_v1(&args.reference_document),
        cycle3_g2048_run_sha256: args.cycle3_g2048_run_sha256.clone(),
        cycle3_g2048_checkpoint_manifest_sha256: args
            .cycle3_g2048_checkpoint_manifest_sha256
            .clone(),
    })
    .unwrap_or_else(|error| fail_v1(error));

    publish_or_exit_v1(&args.output, &bytes);

    let record = decode_cycle4_routing_record_v1(&bytes).unwrap_or_else(|error| fail_v1(error));
    let digest = lower_hex_v1(
        DurableFileExpectationV1::from_bytes(&bytes)
            .unwrap_or_else(|error| fail_v1(error))
            .sha256(),
    );
    println!(
        "outcome={} carried={} parent_checkpoint_manifest_sha256={} recipe={} rank_order={} record_sha256={digest}",
        record.outcome,
        record.carried_endpoint_id.as_deref().unwrap_or("-"),
        record.parent_checkpoint_manifest_sha256,
        record.recipe.arm_kind,
        record.rank_order.join(","),
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
            "--m2-panel",
            "m2.json",
            "--m3-report-static-rb",
            "m3-static.json",
            "--m3-report-treatment-rb",
            "m3-treatment.json",
            "--reference-document",
            "m3-reference.json",
            "--cp7-evidence-root",
            "E:\\cycle4\\cp7-evidence",
            "--cycle3-g2048-run-sha256",
            "aa",
            "--cycle3-g2048-checkpoint-manifest-sha256",
            "bb",
            "--output",
            "routing.json",
        ]
    }

    #[test]
    fn parses_every_required_flag_v1() {
        let parsed =
            parse_args_v1(args_v1(&required_flags_v1())).expect("well-formed command line parses");
        assert_eq!(parsed.m2_panel, PathBuf::from("m2.json"));
        assert_eq!(
            parsed.reference_document,
            PathBuf::from("m3-reference.json")
        );
        assert_eq!(parsed.cycle3_g2048_run_sha256, "aa");
        assert_eq!(parsed.output, PathBuf::from("routing.json"));
    }

    /// The CP7 evidence root is not optional: the guard is the mechanical
    /// half of section D's freeze order, so a caller cannot skip it by
    /// omitting the flag.
    #[test]
    fn the_cp7_evidence_root_is_required_v1() {
        let mut flags = required_flags_v1();
        let index = flags
            .iter()
            .position(|flag| *flag == "--cp7-evidence-root")
            .expect("flag present");
        flags.drain(index..index + 2);
        assert!(parse_args_v1(args_v1(&flags)).is_err());
    }

    #[test]
    fn rejects_unknown_duplicate_missing_and_truncated_flags_v1() {
        let mut duplicated = required_flags_v1();
        duplicated.extend_from_slice(&["--m2-panel", "other.json"]);
        assert!(parse_args_v1(args_v1(&duplicated)).is_err());

        let mut unknown = required_flags_v1();
        unknown.extend_from_slice(&["--force", "yes"]);
        assert!(parse_args_v1(args_v1(&unknown)).is_err());

        for removed in [
            "--m2-panel",
            "--m3-report-static-rb",
            "--m3-report-treatment-rb",
            "--reference-document",
            "--cycle3-g2048-run-sha256",
            "--cycle3-g2048-checkpoint-manifest-sha256",
            "--output",
        ] {
            let mut flags = required_flags_v1();
            let index = flags
                .iter()
                .position(|flag| *flag == removed)
                .expect("flag present");
            flags.drain(index..index + 2);
            assert!(parse_args_v1(args_v1(&flags)).is_err(), "{removed}");
        }

        assert!(parse_args_v1(args_v1(&["--m2-panel"])).is_err());
        assert!(parse_args_v1(args_v1(&[])).is_err());
    }
}
