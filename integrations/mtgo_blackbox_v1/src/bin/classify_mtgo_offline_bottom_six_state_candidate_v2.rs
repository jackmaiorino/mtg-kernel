use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1,
    classify_untrusted_offline_bottom_six_state_candidate_v2,
};
use serde_json::json;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(env::args().nth(1).ok_or(
        "usage: classify_mtgo_offline_bottom_six_state_candidate_v2 <artifact-directory>",
    )?);
    let manifest = fs::read(directory.join("manifest.json"))?;
    let canonical = fs::read(directory.join("frame.bgra"))?;
    let preview = fs::read(directory.join("frame.png"))?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(&manifest, &canonical, &preview)?;
    let candidate = classify_untrusted_offline_bottom_six_state_candidate_v2(&checked, &canonical)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_measurement_only",
            "classification": format!("{:?}", candidate.classification()),
            "profile_commitment_sha256": candidate.profile_commitment_sha256(),
            "source_manifest_sha256": candidate.source_manifest_sha256(),
            "source_frame_sha256": candidate.source_frame_sha256(),
            "legacy_candidate_commitment_sha256": candidate.legacy_candidate_commitment_sha256(),
            "observed_prompt_ink_sha256": candidate.observed_prompt_ink_sha256(),
            "prompt_matches": candidate.prompt_matches(),
            "turn_one_matches": candidate.turn_one_matches(),
            "observed_control_sha256": candidate.observed_control_sha256(),
            "occupancy_bright_pixel_counts": candidate.occupancy_bright_pixel_counts(),
            "required_bottom_count": candidate.required_bottom_count(),
            "visible_hand_count": candidate.visible_hand_count(),
            "selected_count": candidate.selected_count(),
            "done_visible": candidate.done_visible(),
            "legal_action_count": candidate.legal_action_count(),
            "candidate_commitment_sha256": candidate.candidate_commitment_sha256(),
            "safe_for_live_frame": candidate.safe_for_live_frame(),
            "safe_for_semantic_evidence": candidate.safe_for_semantic_evidence(),
            "safe_for_observation_v5": candidate.safe_for_observation_v5(),
            "safe_for_policy_scoring": candidate.safe_for_policy_scoring(),
            "safe_for_input": candidate.safe_for_input(),
        }))?
    );
    Ok(())
}
