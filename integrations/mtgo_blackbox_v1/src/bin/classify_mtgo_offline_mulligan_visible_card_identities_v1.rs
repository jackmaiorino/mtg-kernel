use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1,
    check_untrusted_offline_visible_card_template_profile_v1,
    classify_untrusted_offline_mulligan_visible_card_identities_v1,
    MtgoOfflineVisibleCardTemplateProfileV1,
};
use serde_json::json;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let artifact_directory = PathBuf::from(env::args().nth(1).ok_or(
        "usage: classify_mtgo_offline_mulligan_visible_card_identities_v1 <artifact-directory> <profile-json>",
    )?);
    let profile_path = PathBuf::from(env::args().nth(2).ok_or(
        "usage: classify_mtgo_offline_mulligan_visible_card_identities_v1 <artifact-directory> <profile-json>",
    )?);
    let manifest = fs::read(artifact_directory.join("manifest.json"))?;
    let canonical = fs::read(artifact_directory.join("frame.bgra"))?;
    let preview = fs::read(artifact_directory.join("frame.png"))?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(&manifest, &canonical, &preview)?;
    let profile: MtgoOfflineVisibleCardTemplateProfileV1 =
        serde_json::from_slice(&fs::read(profile_path)?)?;
    let profile = check_untrusted_offline_visible_card_template_profile_v1(profile)?;
    let candidate = classify_untrusted_offline_mulligan_visible_card_identities_v1(
        &checked, &canonical, &profile,
    )?;
    let identities: Vec<_> = candidate
        .identities()
        .iter()
        .map(|identity| {
            json!({
                "ordinal": identity.ordinal(),
                "visible_card_name": identity.visible_card_name(),
                "winning_template_id": identity.winning_template_id(),
                "mean_absolute_difference_milli": identity.mean_absolute_difference_milli(),
                "runner_up_distinct_name": identity.runner_up_distinct_name(),
                "runner_up_mean_absolute_difference_milli": identity.runner_up_mean_absolute_difference_milli(),
                "distinct_name_margin_milli": identity.distinct_name_margin_milli(),
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "checked_untrusted_offline_measurement_only",
            "classification": format!("{:?}", candidate.classification()),
            "source_manifest_sha256": candidate.source_manifest_sha256(),
            "source_frame_sha256": candidate.source_frame_sha256(),
            "source_ladder_commitment_sha256": candidate.source_ladder_commitment_sha256(),
            "profile_commitment_sha256": candidate.profile_commitment_sha256(),
            "prospective_keep_size": candidate.prospective_keep_size(),
            "visible_hand_count": candidate.visible_hand_count(),
            "matched_identity_count": candidate.matched_identity_count(),
            "identities": identities,
            "candidate_commitment_sha256": candidate.candidate_commitment_sha256(),
            "safe_for_semantic_evidence": candidate.safe_for_semantic_evidence(),
            "safe_for_observation_v5": candidate.safe_for_observation_v5(),
            "safe_for_policy_scoring": candidate.safe_for_policy_scoring(),
            "safe_for_input": candidate.safe_for_input(),
        }))?
    );
    Ok(())
}
