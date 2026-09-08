use crate::{
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, CheckedUntrustedMtgoOfflineLondonBottomingTraceV1,
    CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1, MtgoContractErrorV1,
    MtgoDxgiCaptureRoleV2, MtgoOfflineBottomingActionSemanticV1,
    MtgoOfflineMulliganLadderClassificationV1, MtgoPregameActionSemanticV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_OFFLINE_LONDON_PREGAME_EPISODE_SCHEMA_V1: u32 = 1;

const OFFLINE_PREGAME_EPISODE_DOMAIN_V1: &[u8] = b"mtgo-offline-london-pregame-episode-v1";
const CHOICE_STAGE_COUNT_V1: usize = 7;
const BOTTOMING_STAGE_COUNT_V1: usize = 7;
const COMPLETE_STAGE_COUNT_V1: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoOfflinePregameEpisodeStageV1 {
    MulliganChoice { prospective_keep_size: u8 },
    Bottoming { selected_count: u8 },
    GameplayFirstMain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflinePregameEpisodeFrameV1 {
    pub stage: MtgoOfflinePregameEpisodeStageV1,
    pub source_manifest_sha256: String,
    pub source_frame_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoOfflinePregameEpisodeActionV1 {
    Mulligan {
        next_hand_size: u8,
    },
    KeepOpeningHand,
    SelectForBottom {
        adapter_object_id: String,
        selection_ordinal: u8,
    },
    SubmitBottoming,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineLondonPregameEpisodeV1 {
    pub schema_version: u32,
    pub episode_id: String,
    pub frames: Vec<MtgoOfflinePregameEpisodeFrameV1>,
    pub declared_transition_actions: Vec<MtgoOfflinePregameEpisodeActionV1>,
    pub choice_candidate_commitments_sha256: Vec<String>,
    pub bottoming_trace_commitment_sha256: String,
}

/// One checked-untrusted, offline-only maximum-depth London pregame episode.
///
/// The episode joins seven visible mulligan choices, Keep at prospective one,
/// seven bottoming states, Submit, and the first visible gameplay frame. It
/// checks exact child commitments, exact frame order, and one source-legal
/// declared action per edge. Action, stage, and card labels remain manual, and
/// producer claims remain checked-untrusted.
///
/// This type retains no pixels or coordinates, implements neither `Debug` nor
/// `Clone`, and cannot create a model request, kernel action, or input command.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineLondonPregameEpisodeV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoOfflineLondonPregameEpisodeV1) {
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineLondonPregameEpisodeV1;
/// fn coordinate_escape(value: &CheckedUntrustedMtgoOfflineLondonPregameEpisodeV1) {
///     let _ = value.control_rects_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoOfflineLondonPregameEpisodeV1 {
    episode_commitment_sha256: String,
    stage_action_counts: Vec<usize>,
}

impl CheckedUntrustedMtgoOfflineLondonPregameEpisodeV1 {
    pub fn episode_commitment_sha256(&self) -> &str {
        &self.episode_commitment_sha256
    }

    pub fn stage_count(&self) -> usize {
        self.stage_action_counts.len()
    }

    pub fn declared_transition_count(&self) -> usize {
        self.stage_action_counts.len().saturating_sub(1)
    }

    pub fn stage_action_counts(&self) -> &[usize] {
        &self.stage_action_counts
    }

    pub fn safe_for_live_frame(&self) -> bool {
        false
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn validate_untrusted_offline_london_pregame_episode_v1(
    record: MtgoOfflineLondonPregameEpisodeV1,
    choice_candidates: &[&CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1],
    bottoming: &CheckedUntrustedMtgoOfflineLondonBottomingTraceV1,
    frames: &[&CheckedUntrustedMtgoDxgiCaptureArtifactV1],
) -> Result<CheckedUntrustedMtgoOfflineLondonPregameEpisodeV1, MtgoContractErrorV1> {
    validate_record_structure_v1(&record)?;
    if choice_candidates.len() != CHOICE_STAGE_COUNT_V1 || frames.len() != COMPLETE_STAGE_COUNT_V1 {
        return Err(error_v1(
            "offline_pregame_episode_input_count",
            "seven choice candidates and fifteen checked frames are required",
        ));
    }
    validate_frame_sequence_v1(&record, frames)?;
    validate_choice_sequence_v1(&record, choice_candidates, frames)?;
    let bottom_action_counts = validate_bottoming_sequence_v1(&record, bottoming, frames)?;

    let mut stage_action_counts = vec![2; CHOICE_STAGE_COUNT_V1];
    stage_action_counts.extend(bottom_action_counts);
    stage_action_counts.push(0);

    let encoded = serde_json::to_vec(&record).map_err(|error| {
        error_v1(
            "offline_pregame_episode_serialization",
            format!("serialize episode: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(OFFLINE_PREGAME_EPISODE_DOMAIN_V1);
    update_hash_part_v1(&mut hasher, &encoded);
    for candidate in choice_candidates {
        update_hash_part_v1(
            &mut hasher,
            candidate.candidate_commitment_sha256().as_bytes(),
        );
    }
    update_hash_part_v1(&mut hasher, bottoming.trace_commitment_sha256().as_bytes());
    for frame in frames {
        for part in [
            frame.manifest_sha256().as_bytes(),
            frame.canonical_bgra8_sha256().as_bytes(),
            frame.preview_png_sha256().as_bytes(),
            frame.output_identity_sha256().as_bytes(),
            &frame.captured_at_unix_millis().to_be_bytes(),
            &frame.client_size_px().width.to_be_bytes(),
            &frame.client_size_px().height.to_be_bytes(),
        ] {
            update_hash_part_v1(&mut hasher, part);
        }
    }

    Ok(CheckedUntrustedMtgoOfflineLondonPregameEpisodeV1 {
        episode_commitment_sha256: format!("{:x}", hasher.finalize()),
        stage_action_counts,
    })
}

fn validate_record_structure_v1(
    record: &MtgoOfflineLondonPregameEpisodeV1,
) -> Result<(), MtgoContractErrorV1> {
    if record.schema_version != MTGO_OFFLINE_LONDON_PREGAME_EPISODE_SCHEMA_V1 {
        return Err(error_v1(
            "offline_pregame_episode_schema",
            "unsupported offline London pregame episode schema",
        ));
    }
    validate_identifier_v1(&record.episode_id, "offline_pregame_episode_id")?;
    if record.frames.len() != COMPLETE_STAGE_COUNT_V1
        || record.declared_transition_actions.len() != COMPLETE_STAGE_COUNT_V1 - 1
        || record.choice_candidate_commitments_sha256.len() != CHOICE_STAGE_COUNT_V1
    {
        return Err(error_v1(
            "offline_pregame_episode_shape",
            "the maximum-depth episode requires fifteen frames, fourteen declared actions, and seven choice commitments",
        ));
    }
    for (index, frame) in record.frames.iter().enumerate() {
        let expected_stage = expected_stage_v1(index);
        if frame.stage != expected_stage {
            return Err(error_v1(
                "offline_pregame_episode_stage_order",
                "stages must be choices seven through one, bottoming zero through six, then first main",
            ));
        }
        require_sha256_v1(
            &frame.source_manifest_sha256,
            "offline_pregame_episode_manifest",
        )?;
        require_sha256_v1(&frame.source_frame_sha256, "offline_pregame_episode_frame")?;
    }
    for commitment in &record.choice_candidate_commitments_sha256 {
        require_sha256_v1(commitment, "offline_pregame_episode_choice_commitment")?;
    }
    require_sha256_v1(
        &record.bottoming_trace_commitment_sha256,
        "offline_pregame_episode_bottoming_commitment",
    )?;
    validate_declared_path_shape_v1(record)?;
    Ok(())
}

fn validate_declared_path_shape_v1(
    record: &MtgoOfflineLondonPregameEpisodeV1,
) -> Result<(), MtgoContractErrorV1> {
    for index in 0..6 {
        let expected = MtgoOfflinePregameEpisodeActionV1::Mulligan {
            next_hand_size: u8::try_from(6 - index).unwrap_or(0),
        };
        if record.declared_transition_actions[index] != expected {
            return Err(error_v1(
                "offline_pregame_episode_transition_order",
                "the declared path must Mulligan from seven through two",
            ));
        }
    }
    if record.declared_transition_actions[6] != MtgoOfflinePregameEpisodeActionV1::KeepOpeningHand
        || record.declared_transition_actions[13]
            != MtgoOfflinePregameEpisodeActionV1::SubmitBottoming
    {
        return Err(error_v1(
            "offline_pregame_episode_transition_order",
            "the declared path must Keep at one and Submit after six selections",
        ));
    }
    let mut selected_ids = HashSet::new();
    for index in 0..6 {
        match &record.declared_transition_actions[7 + index] {
            MtgoOfflinePregameEpisodeActionV1::SelectForBottom {
                adapter_object_id,
                selection_ordinal,
            } if usize::from(*selection_ordinal) == index + 1 => {
                validate_identifier_v1(
                    adapter_object_id,
                    "offline_pregame_episode_adapter_object_id",
                )?;
                if !selected_ids.insert(adapter_object_id.as_str()) {
                    return Err(error_v1(
                        "offline_pregame_episode_transition_order",
                        "bottom selections must use six distinct object IDs",
                    ));
                }
            }
            _ => {
                return Err(error_v1(
                    "offline_pregame_episode_transition_order",
                    "bottom selections must have canonical ordinals one through six",
                ));
            }
        }
    }
    Ok(())
}

fn expected_stage_v1(index: usize) -> MtgoOfflinePregameEpisodeStageV1 {
    if index < CHOICE_STAGE_COUNT_V1 {
        return MtgoOfflinePregameEpisodeStageV1::MulliganChoice {
            prospective_keep_size: u8::try_from(CHOICE_STAGE_COUNT_V1 - index).unwrap_or(0),
        };
    }
    if index < CHOICE_STAGE_COUNT_V1 + BOTTOMING_STAGE_COUNT_V1 {
        return MtgoOfflinePregameEpisodeStageV1::Bottoming {
            selected_count: u8::try_from(index - CHOICE_STAGE_COUNT_V1).unwrap_or(u8::MAX),
        };
    }
    MtgoOfflinePregameEpisodeStageV1::GameplayFirstMain
}

fn validate_frame_sequence_v1(
    record: &MtgoOfflineLondonPregameEpisodeV1,
    frames: &[&CheckedUntrustedMtgoDxgiCaptureArtifactV1],
) -> Result<(), MtgoContractErrorV1> {
    let first = frames[0];
    let mut seen_manifests = HashSet::new();
    let mut seen_frames = HashSet::new();
    let mut previous_timestamp = None;
    for (binding, frame) in record.frames.iter().zip(frames) {
        if frame.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire {
            return Err(error_v1(
                "offline_pregame_episode_capture_role",
                "every source must have the acting-player Solitaire role",
            ));
        }
        if frame.client_size_px() != first.client_size_px()
            || frame.output_identity_sha256() != first.output_identity_sha256()
        {
            return Err(error_v1(
                "offline_pregame_episode_capture_layout",
                "every source must share client size and output identity",
            ));
        }
        if binding.source_manifest_sha256 != frame.manifest_sha256()
            || binding.source_frame_sha256 != frame.canonical_bgra8_sha256()
        {
            return Err(error_v1(
                "offline_pregame_episode_frame_binding",
                "every stage must bind its exact checked source artifact",
            ));
        }
        if !seen_manifests.insert(frame.manifest_sha256())
            || !seen_frames.insert(frame.canonical_bgra8_sha256())
        {
            return Err(error_v1(
                "offline_pregame_episode_capture_duplicate",
                "every source manifest and canonical frame must be distinct",
            ));
        }
        if previous_timestamp.is_some_and(|previous| frame.captured_at_unix_millis() <= previous) {
            return Err(error_v1(
                "offline_pregame_episode_capture_order",
                "source capture timestamps must be strictly increasing",
            ));
        }
        previous_timestamp = Some(frame.captured_at_unix_millis());
    }
    Ok(())
}

fn validate_choice_sequence_v1(
    record: &MtgoOfflineLondonPregameEpisodeV1,
    choice_candidates: &[&CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1],
    frames: &[&CheckedUntrustedMtgoDxgiCaptureArtifactV1],
) -> Result<(), MtgoContractErrorV1> {
    for (index, (candidate, frame)) in choice_candidates.iter().zip(frames).enumerate() {
        let keep_size = u8::try_from(CHOICE_STAGE_COUNT_V1 - index).unwrap_or(0);
        let expected_actions = [
            MtgoPregameActionSemanticV1::Mulligan {
                next_hand_size: keep_size - 1,
            },
            MtgoPregameActionSemanticV1::KeepOpeningHand,
        ];
        if candidate.classification() != MtgoOfflineMulliganLadderClassificationV1::Match
            || candidate.matched_profile_count() != 1
            || candidate.evaluated_profile_count() != CHOICE_STAGE_COUNT_V1
            || candidate.prospective_keep_size() != Some(keep_size)
            || candidate.ordered_actions() != expected_actions
            || candidate.source_manifest_sha256() != frame.manifest_sha256()
            || candidate.source_frame_sha256() != frame.canonical_bgra8_sha256()
            || candidate.candidate_commitment_sha256()
                != record.choice_candidate_commitments_sha256[index]
        {
            return Err(error_v1(
                "offline_pregame_episode_choice",
                "every choice must be a unique exact match with canonical Mulligan then Keep actions",
            ));
        }
        let expected_declared = if keep_size > 1 {
            MtgoOfflinePregameEpisodeActionV1::Mulligan {
                next_hand_size: keep_size - 1,
            }
        } else {
            MtgoOfflinePregameEpisodeActionV1::KeepOpeningHand
        };
        if record.declared_transition_actions[index] != expected_declared {
            return Err(error_v1(
                "offline_pregame_episode_choice_transition",
                "the declared path must Mulligan from seven through two, then Keep at one",
            ));
        }
    }
    Ok(())
}

fn validate_bottoming_sequence_v1(
    record: &MtgoOfflineLondonPregameEpisodeV1,
    bottoming: &CheckedUntrustedMtgoOfflineLondonBottomingTraceV1,
    frames: &[&CheckedUntrustedMtgoDxgiCaptureArtifactV1],
) -> Result<Vec<usize>, MtgoContractErrorV1> {
    if bottoming.required_bottom_count() != 6
        || bottoming.began_game_hand_size() != 1
        || bottoming.state_count() != BOTTOMING_STAGE_COUNT_V1
        || bottoming.trace_commitment_sha256() != record.bottoming_trace_commitment_sha256
    {
        return Err(error_v1(
            "offline_pregame_episode_bottoming",
            "Keep at one must bind the exact bottom-six child trace",
        ));
    }
    let bottom_frames = &frames[CHOICE_STAGE_COUNT_V1..CHOICE_STAGE_COUNT_V1 + 7];
    let mut action_counts = Vec::with_capacity(BOTTOMING_STAGE_COUNT_V1);
    for (index, frame) in bottom_frames.iter().enumerate() {
        if bottoming.state_source_manifest_sha256(index) != Some(frame.manifest_sha256())
            || bottoming.state_source_frame_sha256(index) != Some(frame.canonical_bgra8_sha256())
        {
            return Err(error_v1(
                "offline_pregame_episode_bottoming_frame",
                "every bottoming stage must match the child trace source",
            ));
        }
        let actions = bottoming.legal_actions_for_state(index).ok_or_else(|| {
            error_v1(
                "offline_pregame_episode_bottoming_actions",
                "every bottoming stage requires a complete action set",
            )
        })?;
        let expected_count = match index {
            0 => 7,
            1..=5 => 8 - index,
            _ => 2,
        };
        if actions.len() != expected_count
            || (index < 6
                && !matches!(
                    actions.first(),
                    Some(MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
                        selection_ordinal,
                        ..
                    }) if usize::from(*selection_ordinal) == index + 1
                ))
            || (index == 6
                && actions.first() != Some(&MtgoOfflineBottomingActionSemanticV1::SubmitBottoming))
        {
            return Err(error_v1(
                "offline_pregame_episode_bottoming_actions",
                "bottoming actions must progress through six selections and Submit",
            ));
        }
        let declared_index = CHOICE_STAGE_COUNT_V1 + index;
        let expected_declared = if index < 6 {
            let adapter_object_id = bottoming.selected_for_bottom_click_order()[index].clone();
            let selection_ordinal = u8::try_from(index + 1).unwrap_or(u8::MAX);
            let expected_child_action = MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
                adapter_object_id: adapter_object_id.clone(),
                selection_ordinal,
            };
            if !actions.contains(&expected_child_action) {
                return Err(error_v1(
                    "offline_pregame_episode_bottoming_transition",
                    "every selected object must be legal in its exact source state",
                ));
            }
            MtgoOfflinePregameEpisodeActionV1::SelectForBottom {
                adapter_object_id,
                selection_ordinal,
            }
        } else {
            MtgoOfflinePregameEpisodeActionV1::SubmitBottoming
        };
        if record.declared_transition_actions[declared_index] != expected_declared {
            return Err(error_v1(
                "offline_pregame_episode_bottoming_transition",
                "the declared path must select the child trace click order, then Submit",
            ));
        }
        action_counts.push(actions.len());
    }
    let completion = frames[14];
    if bottoming.completion_source_manifest_sha256() != completion.manifest_sha256()
        || bottoming.completion_source_frame_sha256() != completion.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "offline_pregame_episode_completion_frame",
            "the first-main frame must match the child trace completion",
        ));
    }
    Ok(action_counts)
}

fn validate_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(error_v1(code, "invalid bounded ASCII identifier"));
    }
    Ok(())
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(error_v1(code, "expected lowercase SHA-256"));
    }
    Ok(())
}

fn update_hash_part_v1(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(label: &str) -> String {
        format!("{:x}", Sha256::digest(label.as_bytes()))
    }

    fn fixture_v1() -> MtgoOfflineLondonPregameEpisodeV1 {
        MtgoOfflineLondonPregameEpisodeV1 {
            schema_version: 1,
            episode_id: "synthetic-full-london-pregame-v1".to_owned(),
            frames: (0..COMPLETE_STAGE_COUNT_V1)
                .map(|index| MtgoOfflinePregameEpisodeFrameV1 {
                    stage: expected_stage_v1(index),
                    source_manifest_sha256: digest(&format!("manifest-{index}")),
                    source_frame_sha256: digest(&format!("frame-{index}")),
                })
                .collect(),
            declared_transition_actions: (0..COMPLETE_STAGE_COUNT_V1 - 1)
                .map(|index| {
                    if index < 6 {
                        MtgoOfflinePregameEpisodeActionV1::Mulligan {
                            next_hand_size: u8::try_from(6 - index).unwrap(),
                        }
                    } else if index == 6 {
                        MtgoOfflinePregameEpisodeActionV1::KeepOpeningHand
                    } else if index < 13 {
                        MtgoOfflinePregameEpisodeActionV1::SelectForBottom {
                            adapter_object_id: format!("opening-hand:{}", index - 7),
                            selection_ordinal: u8::try_from(index - 6).unwrap(),
                        }
                    } else {
                        MtgoOfflinePregameEpisodeActionV1::SubmitBottoming
                    }
                })
                .collect(),
            choice_candidate_commitments_sha256: (0..CHOICE_STAGE_COUNT_V1)
                .map(|index| digest(&format!("choice-{index}")))
                .collect(),
            bottoming_trace_commitment_sha256: digest("bottoming"),
        }
    }

    #[test]
    fn maximum_depth_stage_sequence_is_canonical() {
        let record = fixture_v1();
        validate_record_structure_v1(&record).unwrap();
        assert_eq!(
            record.frames[0].stage,
            MtgoOfflinePregameEpisodeStageV1::MulliganChoice {
                prospective_keep_size: 7
            }
        );
        assert_eq!(
            record.frames[6].stage,
            MtgoOfflinePregameEpisodeStageV1::MulliganChoice {
                prospective_keep_size: 1
            }
        );
        assert_eq!(
            record.frames[13].stage,
            MtgoOfflinePregameEpisodeStageV1::Bottoming { selected_count: 6 }
        );
        assert_eq!(
            record.frames[14].stage,
            MtgoOfflinePregameEpisodeStageV1::GameplayFirstMain
        );
    }

    #[test]
    fn swapped_or_missing_stage_fails_closed() {
        let mut record = fixture_v1();
        record.frames.swap(0, 1);
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_pregame_episode_stage_order"
        );

        let mut record = fixture_v1();
        record.frames.pop();
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_pregame_episode_shape"
        );
    }

    #[test]
    fn unknown_fields_and_bad_commitments_fail_closed() {
        let record = fixture_v1();
        let encoded = serde_json::to_string(&record).unwrap();
        let altered = encoded.replacen('{', "{\"unknown\":true,", 1);
        assert!(serde_json::from_str::<MtgoOfflineLondonPregameEpisodeV1>(&altered).is_err());

        let mut record = fixture_v1();
        record.choice_candidate_commitments_sha256[3] = "not-a-digest".to_owned();
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_pregame_episode_choice_commitment"
        );
    }

    #[test]
    fn missing_declared_transition_fails_closed() {
        let mut record = fixture_v1();
        record.declared_transition_actions.pop();
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_pregame_episode_shape"
        );
    }

    #[test]
    fn wrong_declared_path_action_fails_closed() {
        let mut record = fixture_v1();
        record.declared_transition_actions[2] = MtgoOfflinePregameEpisodeActionV1::KeepOpeningHand;
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_pregame_episode_transition_order"
        );

        let mut record = fixture_v1();
        record.declared_transition_actions[8] = record.declared_transition_actions[7].clone();
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_pregame_episode_transition_order"
        );
    }
}
