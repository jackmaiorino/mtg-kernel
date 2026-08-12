use mtgo_blackbox_v1::{
    check_untrusted_offline_visible_card_template_profile_v1,
    MtgoOfflineVisibleCardTemplateProfileV1,
};
use mtgo_dxgi_capture_v1::{
    build_pinned_current_solitaire_pregame_action_plan_v1,
    load_pinned_current_solitaire_visible_frame_from_artifact_v1,
    measure_pinned_current_solitaire_mulligan_ladder_v1,
    measure_pinned_current_solitaire_mulligan_visible_hand_v1,
    score_and_select_pinned_current_solitaire_pregame_v1, MtgoNonModelPregameHeuristicV1,
};
use serde_json::json;
use std::{env, fs, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_SOLITAIRE_PREGAME_REHEARSAL_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let artifact_directory = PathBuf::from(arguments.next().ok_or(
        "usage: rehearse_mtgo_solitaire_pregame_v1 <artifact-directory> <visible-card-profile-json>",
    )?);
    let profile_path = PathBuf::from(arguments.next().ok_or(
        "usage: rehearse_mtgo_solitaire_pregame_v1 <artifact-directory> <visible-card-profile-json>",
    )?);
    if arguments.next().is_some() {
        return Err("exactly one artifact directory and one profile JSON are required".to_owned());
    }
    let source = load_pinned_current_solitaire_visible_frame_from_artifact_v1(&artifact_directory)?;
    let source_commitments = source.commitments_v1();
    let prompt = measure_pinned_current_solitaire_mulligan_ladder_v1(source)?;
    let prompt_commitment = prompt.measurement_commitment_sha256_v1().to_owned();
    let prospective_keep_size = prompt.prospective_keep_size_v1();
    let legal_actions = prompt
        .ordered_actions_v1()
        .iter()
        .map(|action| format!("{action:?}"))
        .collect::<Vec<_>>();

    let profile: MtgoOfflineVisibleCardTemplateProfileV1 = serde_json::from_slice(
        &fs::read(&profile_path)
            .map_err(|error| format!("read visible-card profile JSON: {error}"))?,
    )
    .map_err(|error| format!("parse visible-card profile JSON: {error}"))?;
    let profile = check_untrusted_offline_visible_card_template_profile_v1(profile)
        .map_err(|error| error.to_string())?;
    let visible_hand = measure_pinned_current_solitaire_mulligan_visible_hand_v1(prompt, profile)?;
    let visible_cards = visible_hand
        .identities_v1()
        .iter()
        .map(|identity| identity.visible_card_name().to_owned())
        .collect::<Vec<_>>();
    let visible_identity_commitment = visible_hand
        .visible_identity_measurement_commitment_sha256_v1()
        .to_owned();

    let scorer = MtgoNonModelPregameHeuristicV1::kernel_basic_lands_wiring_only_v1()?;
    let deployment = scorer.compatibility_deployment_v1().clone();
    let mut scorer = scorer;
    let selection = score_and_select_pinned_current_solitaire_pregame_v1(
        visible_hand,
        &deployment,
        &mut scorer,
    )?;
    let selected_index = selection.selected_index_v1();
    let selected_semantic = format!("{:?}", selection.selected_semantic_v1());
    let selection_commitment = selection.selection_commitment_sha256_v1().to_owned();
    let plan = build_pinned_current_solitaire_pregame_action_plan_v1(selection)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "offline_no_input_rehearsal_complete",
            "source_profile_id": source_commitments.profile_id,
            "source_profile_commitment_sha256": source_commitments.profile_commitment_sha256,
            "source_capture_commitment_sha256": source_commitments.source_capture.capture_commitment_sha256,
            "prompt_measurement_commitment_sha256": prompt_commitment,
            "prospective_keep_size": prospective_keep_size,
            "ordered_visible_card_names": visible_cards,
            "ordered_legal_actions": legal_actions,
            "visible_identity_measurement_commitment_sha256": visible_identity_commitment,
            "scorer_kind": "deterministic_non_model_wiring_only",
            "selected_index": selected_index,
            "selected_semantic": selected_semantic,
            "selection_commitment_sha256": selection_commitment,
            "planned_postcondition": format!("{:?}", plan.planned_postcondition_v1()),
            "action_plan_commitment_sha256": plan.action_plan_commitment_sha256_v1(),
            "input_sent": false,
            "client_focused_or_moved": false,
            "safe_for_live_input": plan.safe_for_live_input_v1(),
            "safe_for_purchase": plan.safe_for_purchase_v1(),
            "safe_for_queue_entry": plan.safe_for_queue_entry_v1()
        }))
        .map_err(|error| format!("serialize rehearsal result: {error}"))?
    );
    Ok(())
}
