use mtgo_blackbox_v1::{
    check_untrusted_visible_duel_viewmodel_candidate_surface_v1,
    mtgo_visible_duel_viewmodel_candidate_surface_v1,
};
use serde_json::json;

fn main() -> Result<(), String> {
    let manifest = mtgo_visible_duel_viewmodel_candidate_surface_v1();
    let checked = check_untrusted_visible_duel_viewmodel_candidate_surface_v1(manifest.clone())
        .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "manifest": manifest,
            "candidate_surface_commitment_sha256": checked.commitment_sha256_v1(),
            "candidate_property_count": checked.candidate_property_count_v1(),
            "forbidden_property_count": checked.forbidden_property_count_v1(),
            "complete_duel_projection_demonstrated": checked.complete_duel_projection_demonstrated_v1(),
            "live_producer_attested": checked.live_producer_attested_v1(),
            "safe_for_live_semantic_evidence": checked.safe_for_live_semantic_evidence_v1(),
            "safe_for_model_scoring": checked.safe_for_model_scoring_v1(),
            "safe_for_input": checked.safe_for_input_v1(),
            "permits_event_entry": checked.permits_event_entry_v1(),
            "permits_spending": checked.permits_spending_v1(),
        }))
        .map_err(|error| format!("serialize candidate surface: {error}"))?
    );
    Ok(())
}
