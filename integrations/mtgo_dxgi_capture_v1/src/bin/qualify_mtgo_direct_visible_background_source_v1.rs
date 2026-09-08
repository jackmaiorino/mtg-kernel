#[cfg(not(target_os = "windows"))]
compile_error!("qualify_mtgo_direct_visible_background_source_v1 is Windows-only");

use mtgo_dxgi_capture_v1::{
    qualify_stable_background_direct_visible_source_v1, verify_direct_visible_source_runtime_v1,
};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct QualificationSummaryV1 {
    schema: &'static str,
    status: &'static str,
    abstention_reason: Option<String>,
    runtime_identity_commitment_sha256: String,
    broker_binary_sha256: String,
    producer_binary_sha256: String,
    sanitized_result_sha256: String,
    stability_commitment_sha256: String,
    producer_execution_attested: bool,
    sanitized_visible_projection_stable: bool,
    foreground_window_required: bool,
    pixel_capture_required: bool,
    safe_for_live_semantic_evidence: bool,
    safe_for_model_scoring: bool,
    safe_for_input: bool,
    permits_event_entry: bool,
    permits_spending: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_DIRECT_VISIBLE_BACKGROUND_QUALIFICATION_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("expected BROKER_PATH BOOTSTRAP_PATH PRODUCER_PATH VALIDATOR_PATH".to_owned());
    }
    let runtime = verify_direct_visible_source_runtime_v1(
        Path::new(&args[0]),
        Path::new(&args[1]),
        Path::new(&args[2]),
        Path::new(&args[3]),
    )?;
    let observation = qualify_stable_background_direct_visible_source_v1(&runtime, 10_000)?;
    let commitments = observation.commitments_v1();
    let abstention_reason = observation
        .abstention_reason_v1()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| format!("serialize abstention reason: {error}"))?
        .and_then(|value| value.as_str().map(str::to_owned));
    let status = if abstention_reason.is_some() {
        "stable_visible_equivalent_abstention"
    } else {
        "stable_visible_equivalent_candidate_retained_privately"
    };
    let summary = QualificationSummaryV1 {
        schema: "mtgo-direct-visible-background-qualification-summary/v1",
        status,
        abstention_reason,
        runtime_identity_commitment_sha256: commitments.runtime_identity_commitment_sha256,
        broker_binary_sha256: commitments.broker_binary_sha256,
        producer_binary_sha256: commitments.producer_binary_sha256,
        sanitized_result_sha256: commitments.sanitized_result_sha256,
        stability_commitment_sha256: commitments.stability_commitment_sha256,
        producer_execution_attested: observation.producer_execution_attested_v1(),
        sanitized_visible_projection_stable: observation.sanitized_visible_projection_stable_v1(),
        foreground_window_required: observation.requires_foreground_window_v1(),
        pixel_capture_required: observation.requires_pixel_capture_v1(),
        safe_for_live_semantic_evidence: observation.safe_for_live_semantic_evidence_v1(),
        safe_for_model_scoring: observation.safe_for_model_scoring_v1(),
        safe_for_input: observation.safe_for_input_v1(),
        permits_event_entry: observation.permits_event_entry_v1(),
        permits_spending: observation.permits_spending_v1(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .map_err(|error| format!("serialize qualification summary: {error}"))?
    );
    Ok(())
}
