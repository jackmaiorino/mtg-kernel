//! Offline projection of a recorded automated trajectory. This never creates
//! a live decision binding, loads a model, or authorizes an action in a session.
use crate::human_bo3_v1::project_recorded_decision_v1;
use crate::phase1_agent_v1::{
    ActorVisibleDecisionV1, Bo3TrainingTrajectoryV1, CompleteAgentPackageV1,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const MAX_RECORDED_REPLAY_INPUT_BYTES_V2: usize = 256 * 1024 * 1024;

/// Project the actual sampled actor choices through the same menu sorting and
/// visibility code used by the live human interface. The resulting offline
/// menu still has to match the real current prompt before a replay submits it.
/// Both-seat output is private coordinator data, never a human response. This
/// validates recorded structure and digest, not producer/runtime provenance.
pub fn prepare_recorded_human_replay_v2(input: &str) -> Result<Value, String> {
    if input.len() > MAX_RECORDED_REPLAY_INPUT_BYTES_V2 {
        return Err("Recorded replay input exceeds 256 MiB.".into());
    }
    let mut document = crate::rl::parse_strict_json_value(input).map_err(|e| e.to_string())?;
    if document.get("schema").and_then(Value::as_str)
        != Some(crate::phase1_bo3_collection_v1::BO3_COLLECTION_RESULT_SCHEMA_V1)
    {
        return Err("Recorded collection schema differs.".into());
    }
    let packages = document
        .get_mut("packages")
        .ok_or("Recorded packages are missing.")?
        .take();
    let packages: [CompleteAgentPackageV1; 2] = serde_json::from_value(packages)
        .map_err(|_| "Recorded packages are missing or invalid.".to_owned())?;
    let collected = document
        .get_mut("collected")
        .ok_or("Recorded collection is missing.")?;
    let expected_digest = collected
        .get("trajectory_sha256")
        .and_then(Value::as_str)
        .ok_or("Recorded trajectory digest is missing.")?
        .to_owned();
    let trajectory = collected
        .get_mut("trajectory")
        .ok_or("Recorded trajectory is missing.")?
        .take();
    let trajectory: Bo3TrainingTrajectoryV1 = serde_json::from_value(trajectory)
        .map_err(|_| "Recorded BO3 trajectory is missing or invalid.".to_owned())?;
    trajectory.validate_v1(packages.each_ref())?;
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&trajectory).map_err(|e| e.to_string())?)
    );
    if digest != expected_digest {
        return Err("Recorded trajectory digest differs.".into());
    }
    let mut games = Vec::new();
    for game in &trajectory.games {
        let mut decisions = Vec::new();
        for record in &game.decisions {
            if let ActorVisibleDecisionV1::Gameplay {
                observation,
                ordered_actions,
            } = &record.visible
            {
                let (decision, selected_public_index) = project_recorded_decision_v1(
                    observation,
                    ordered_actions,
                    record.actor,
                    0,
                    record.behavior.selected_index_v1(),
                )
                .map_err(|e| format!("Recorded human projection: {e}"))?;
                decisions.push(
                    json!({"decision_index":record.decision_index,"actor":record.actor,
                    "selected_public_index":selected_public_index,"decision":decision}),
                );
            }
        }
        games.push(json!({"game_index":game.game_index,"decisions":decisions}));
    }
    Ok(json!({"schema":"mtg-kernel-recorded-human-replay/v2",
        "input_sha256":format!("{:x}",Sha256::digest(input.as_bytes())),
        "live_session":false,"games":games}))
}

#[cfg(test)]
mod tests {
    #[test]
    fn malformed_recording_is_rejected_without_panicking() {
        for input in [
            "null",
            "[]",
            "{}",
            r#"{"schema":"mtg-kernel-bo3-collection-result/v1"}"#,
            r#"{"schema":"mtg-kernel-bo3-collection-result/v1","packages":null,"collected":null}"#,
        ] {
            assert!(super::prepare_recorded_human_replay_v2(input).is_err());
        }
    }
}
