#[cfg(not(target_os = "windows"))]
compile_error!("qualify_mtgo_direct_visible_spectator_source_v1 is Windows-only");

use mtgo_dxgi_capture_v1::{
    qualify_attested_direct_visible_source_current_spectator_v1,
    verify_direct_visible_source_runtime_v1,
};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct SpectatorQualificationSummaryV1 {
    schema: &'static str,
    status: &'static str,
    qualification_role: &'static str,
    abstention_reason: Option<String>,
    runtime_identity_commitment_sha256: String,
    broker_binary_sha256: String,
    producer_binary_sha256: String,
    before_capture_commitment_sha256: String,
    after_capture_commitment_sha256: String,
    sanitized_result_sha256: String,
    qualification_commitment_sha256: String,
    producer_execution_attested: bool,
    acting_player_authority: bool,
    safe_for_live_semantic_evidence: bool,
    safe_for_model_scoring: bool,
    safe_for_input: bool,
    permits_event_entry: bool,
    permits_spending: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_DIRECT_VISIBLE_SPECTATOR_QUALIFICATION_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(
            "expected GAME_FORMAT BROKER_PATH BOOTSTRAP_PATH PRODUCER_PATH VALIDATOR_PATH"
                .to_owned(),
        );
    }
    let game_format = args[0].to_str().ok_or("game format is not valid Unicode")?;
    let runtime = verify_direct_visible_source_runtime_v1(
        Path::new(&args[1]),
        Path::new(&args[2]),
        Path::new(&args[3]),
        Path::new(&args[4]),
    )?;
    let observation = qualify_attested_direct_visible_source_current_spectator_v1(
        game_format,
        &runtime,
        2_000,
        10_000,
    )?;
    let commitments = observation.commitments_v1();
    let abstention_reason = observation
        .abstention_reason_v1()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| format!("serialize abstention reason: {error}"))?
        .and_then(|value| value.as_str().map(str::to_owned));
    let status = if abstention_reason.is_some() {
        "spectator_visible_equivalent_abstention"
    } else {
        "spectator_visible_candidate_retained_privately"
    };
    let summary = SpectatorQualificationSummaryV1 {
        schema: "mtgo-direct-visible-spectator-qualification-summary/v1",
        status,
        qualification_role: "spectator_only",
        abstention_reason,
        runtime_identity_commitment_sha256: commitments.runtime_identity_commitment_sha256,
        broker_binary_sha256: commitments.broker_binary_sha256,
        producer_binary_sha256: commitments.producer_binary_sha256,
        before_capture_commitment_sha256: commitments.before_capture_commitment_sha256,
        after_capture_commitment_sha256: commitments.after_capture_commitment_sha256,
        sanitized_result_sha256: commitments.sanitized_result_sha256,
        qualification_commitment_sha256: commitments.qualification_commitment_sha256,
        producer_execution_attested: observation.producer_execution_attested_v1(),
        acting_player_authority: false,
        safe_for_live_semantic_evidence: observation.safe_for_live_semantic_evidence_v1(),
        safe_for_model_scoring: observation.safe_for_model_scoring_v1(),
        safe_for_input: observation.safe_for_input_v1(),
        permits_event_entry: observation.permits_event_entry_v1(),
        permits_spending: observation.permits_spending_v1(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .map_err(|error| format!("serialize spectator qualification summary: {error}"))?
    );
    Ok(())
}
