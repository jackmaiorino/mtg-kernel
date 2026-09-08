use crate::{
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1, MtgoDxgiCaptureRoleV2,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_OFFLINE_LONDON_BOTTOMING_TRACE_SCHEMA_V1: u32 = 1;

const OFFLINE_BOTTOMING_TRACE_DOMAIN_V1: &[u8] = b"mtgo-offline-london-bottoming-trace-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoOfflineBottomingActionSemanticV1 {
    SelectForBottom {
        adapter_object_id: String,
        selection_ordinal: u8,
    },
    SubmitBottoming,
    CancelBottoming,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineBottomingOriginalCardV1 {
    pub adapter_object_id: String,
    pub visible_card_name: String,
    pub original_hand_ordinal: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineBottomingStateV1 {
    pub selected_count: u8,
    pub source_manifest_sha256: String,
    pub source_frame_sha256: String,
    pub visible_remaining_object_ids: Vec<String>,
    pub done_visible: bool,
    pub cancel_visible: bool,
    pub prompt_reconciled: bool,
    pub legal_action_set_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineBottomingCompletionV1 {
    pub source_manifest_sha256: String,
    pub source_frame_sha256: String,
    pub bottomed_card_count: u8,
    pub began_game_hand_size: u8,
    pub solitaire_draw_observed: bool,
    pub resulting_visible_hand_size: u8,
    pub first_main_visible: bool,
    pub visible_game_log_reconciled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineLondonBottomingTraceV1 {
    pub schema_version: u32,
    pub trace_id: String,
    pub required_bottom_count: u8,
    pub original_hand: Vec<MtgoOfflineBottomingOriginalCardV1>,
    pub selected_for_bottom_click_order: Vec<String>,
    pub states: Vec<MtgoOfflineBottomingStateV1>,
    pub completion: MtgoOfflineBottomingCompletionV1,
}

/// A structurally checked, offline-only calibration of one visible MTGO London
/// mulligan bottoming sequence.
///
/// The source labels are manual and the DXGI artifacts remain checked-untrusted.
/// The trace proves only that exact artifacts were bound in increasing order,
/// one original hand object disappeared per selection, the current hand was
/// rebuilt after every reflow, and the final visible completion facts agree
/// with the declared count. It does not prove that labels match pixels.
///
/// This type retains no pixels or coordinates, implements neither `Debug` nor
/// `Clone`, and is not serializable. Its adapter-local legal actions cannot be
/// converted to kernel actions, model requests, or input commands.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineLondonBottomingTraceV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoOfflineLondonBottomingTraceV1) {
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineLondonBottomingTraceV1;
/// fn coordinate_escape(value: &CheckedUntrustedMtgoOfflineLondonBottomingTraceV1) {
///     let _ = value.card_rects_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoOfflineLondonBottomingTraceV1 {
    record: MtgoOfflineLondonBottomingTraceV1,
    trace_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineLondonBottomingTraceV1 {
    pub fn trace_commitment_sha256(&self) -> &str {
        &self.trace_commitment_sha256
    }

    pub fn required_bottom_count(&self) -> u8 {
        self.record.required_bottom_count
    }

    pub fn began_game_hand_size(&self) -> u8 {
        self.record.completion.began_game_hand_size
    }

    pub fn selected_for_bottom_click_order(&self) -> &[String] {
        &self.record.selected_for_bottom_click_order
    }

    pub fn original_hand(&self) -> &[MtgoOfflineBottomingOriginalCardV1] {
        &self.record.original_hand
    }

    pub fn state_count(&self) -> usize {
        self.record.states.len()
    }

    pub fn state_source_manifest_sha256(&self, state_index: usize) -> Option<&str> {
        self.record
            .states
            .get(state_index)
            .map(|state| state.source_manifest_sha256.as_str())
    }

    pub fn state_source_frame_sha256(&self, state_index: usize) -> Option<&str> {
        self.record
            .states
            .get(state_index)
            .map(|state| state.source_frame_sha256.as_str())
    }

    pub fn completion_source_manifest_sha256(&self) -> &str {
        &self.record.completion.source_manifest_sha256
    }

    pub fn completion_source_frame_sha256(&self) -> &str {
        &self.record.completion.source_frame_sha256
    }

    pub fn legal_actions_for_state(
        &self,
        state_index: usize,
    ) -> Option<Vec<MtgoOfflineBottomingActionSemanticV1>> {
        self.record
            .states
            .get(state_index)
            .map(legal_actions_for_state_v1)
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

pub fn validate_untrusted_offline_london_bottoming_trace_v1(
    record: MtgoOfflineLondonBottomingTraceV1,
    state_frames: &[&CheckedUntrustedMtgoDxgiCaptureArtifactV1],
    completion_frame: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
) -> Result<CheckedUntrustedMtgoOfflineLondonBottomingTraceV1, MtgoContractErrorV1> {
    validate_record_structure_v1(&record)?;
    if state_frames.len() != record.states.len() {
        return Err(error_v1(
            "offline_bottoming_frame_count",
            "one checked DXGI artifact is required for every bottoming state",
        ));
    }

    let all_frames: Vec<_> = state_frames
        .iter()
        .copied()
        .chain(std::iter::once(completion_frame))
        .collect();
    let first = all_frames[0];
    let mut seen_manifests = HashSet::new();
    let mut seen_frames = HashSet::new();
    let mut previous_timestamp = None;
    for frame in &all_frames {
        if frame.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire {
            return Err(error_v1(
                "offline_bottoming_capture_role",
                "every source must have the acting-player Solitaire role",
            ));
        }
        if frame.client_size_px() != first.client_size_px()
            || frame.output_identity_sha256() != first.output_identity_sha256()
        {
            return Err(error_v1(
                "offline_bottoming_capture_layout",
                "every source must share client size and output identity",
            ));
        }
        if !seen_manifests.insert(frame.manifest_sha256())
            || !seen_frames.insert(frame.canonical_bgra8_sha256())
        {
            return Err(error_v1(
                "offline_bottoming_capture_duplicate",
                "every source manifest and canonical frame must be distinct",
            ));
        }
        if previous_timestamp.is_some_and(|previous| frame.captured_at_unix_millis() <= previous) {
            return Err(error_v1(
                "offline_bottoming_capture_order",
                "source capture timestamps must be strictly increasing",
            ));
        }
        previous_timestamp = Some(frame.captured_at_unix_millis());
    }

    for (state, frame) in record.states.iter().zip(state_frames) {
        require_frame_binding_v1(
            &state.source_manifest_sha256,
            &state.source_frame_sha256,
            frame,
        )?;
    }
    require_frame_binding_v1(
        &record.completion.source_manifest_sha256,
        &record.completion.source_frame_sha256,
        completion_frame,
    )?;

    let encoded = serde_json::to_vec(&record).map_err(|error| {
        error_v1(
            "offline_bottoming_serialization",
            format!("serialize trace: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(OFFLINE_BOTTOMING_TRACE_DOMAIN_V1);
    update_hash_part_v1(&mut hasher, &encoded);
    for frame in all_frames {
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

    Ok(CheckedUntrustedMtgoOfflineLondonBottomingTraceV1 {
        record,
        trace_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn validate_record_structure_v1(
    record: &MtgoOfflineLondonBottomingTraceV1,
) -> Result<(), MtgoContractErrorV1> {
    if record.schema_version != MTGO_OFFLINE_LONDON_BOTTOMING_TRACE_SCHEMA_V1 {
        return Err(error_v1(
            "offline_bottoming_schema",
            "unsupported London bottoming trace schema",
        ));
    }
    validate_identifier_v1(&record.trace_id, "offline_bottoming_trace_id")?;
    if !(1..=6).contains(&record.required_bottom_count)
        || record.original_hand.len() != 7
        || record.selected_for_bottom_click_order.len() != usize::from(record.required_bottom_count)
        || record.states.len() != usize::from(record.required_bottom_count) + 1
    {
        return Err(error_v1(
            "offline_bottoming_shape",
            "trace requires seven original cards, one through six selections, and every intermediate state",
        ));
    }

    let mut original_ids = Vec::with_capacity(7);
    let mut unique_ids = HashSet::new();
    for (index, card) in record.original_hand.iter().enumerate() {
        validate_identifier_v1(
            &card.adapter_object_id,
            "offline_bottoming_adapter_object_id",
        )?;
        validate_visible_card_name_v1(&card.visible_card_name)?;
        if card.original_hand_ordinal != u8::try_from(index).unwrap_or(u8::MAX)
            || !unique_ids.insert(card.adapter_object_id.as_str())
        {
            return Err(error_v1(
                "offline_bottoming_original_hand",
                "original ordinals and adapter object IDs must be unique and canonical",
            ));
        }
        original_ids.push(card.adapter_object_id.as_str());
    }

    let selected_ids: Vec<_> = record
        .selected_for_bottom_click_order
        .iter()
        .map(String::as_str)
        .collect();
    if selected_ids.iter().any(|id| !unique_ids.contains(id))
        || selected_ids.iter().copied().collect::<HashSet<_>>().len() != selected_ids.len()
    {
        return Err(error_v1(
            "offline_bottoming_selected_order",
            "selected bottom order must contain distinct original hand objects",
        ));
    }

    for (index, state) in record.states.iter().enumerate() {
        let expected_selected_count = u8::try_from(index).unwrap_or(u8::MAX);
        if state.selected_count != expected_selected_count
            || !state.cancel_visible
            || !state.prompt_reconciled
            || !state.legal_action_set_complete
            || state.done_visible != (state.selected_count == record.required_bottom_count)
        {
            return Err(error_v1(
                "offline_bottoming_state_readiness",
                "state counts, controls, prompt, and complete legal-action declarations must agree",
            ));
        }
        require_sha256_v1(
            &state.source_manifest_sha256,
            "offline_bottoming_state_manifest",
        )?;
        require_sha256_v1(&state.source_frame_sha256, "offline_bottoming_state_frame")?;
        let selected_prefix: HashSet<_> = selected_ids[..index].iter().copied().collect();
        let expected_remaining: Vec<_> = original_ids
            .iter()
            .copied()
            .filter(|id| !selected_prefix.contains(id))
            .collect();
        let actual_remaining: Vec<_> = state
            .visible_remaining_object_ids
            .iter()
            .map(String::as_str)
            .collect();
        if actual_remaining != expected_remaining {
            return Err(error_v1(
                "offline_bottoming_state_remaining_hand",
                "every state must preserve original order after removing the selected prefix",
            ));
        }
        let actions = legal_actions_for_state_v1(state);
        if state.selected_count < record.required_bottom_count {
            let expected_count =
                state.visible_remaining_object_ids.len() + usize::from(state.selected_count > 0);
            if actions.len() != expected_count
                || (state.selected_count == 0
                    && actions.contains(&MtgoOfflineBottomingActionSemanticV1::CancelBottoming))
                || (state.selected_count > 0
                    && actions.last()
                        != Some(&MtgoOfflineBottomingActionSemanticV1::CancelBottoming))
            {
                return Err(error_v1(
                    "offline_bottoming_state_actions",
                    "selection states require one action per remaining card and Cancel only after a selection",
                ));
            }
        } else if actions
            != [
                MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
                MtgoOfflineBottomingActionSemanticV1::CancelBottoming,
            ]
        {
            return Err(error_v1(
                "offline_bottoming_state_actions",
                "the complete state requires Submit then Cancel",
            ));
        }
    }

    let completion = &record.completion;
    require_sha256_v1(
        &completion.source_manifest_sha256,
        "offline_bottoming_completion_manifest",
    )?;
    require_sha256_v1(
        &completion.source_frame_sha256,
        "offline_bottoming_completion_frame",
    )?;
    let expected_began_hand_size = 7 - record.required_bottom_count;
    let expected_resulting_hand_size = expected_began_hand_size + 1;
    if completion.bottomed_card_count != record.required_bottom_count
        || completion.began_game_hand_size != expected_began_hand_size
        || !completion.solitaire_draw_observed
        || completion.resulting_visible_hand_size != expected_resulting_hand_size
        || !completion.first_main_visible
        || !completion.visible_game_log_reconciled
    {
        return Err(error_v1(
            "offline_bottoming_completion",
            "completion must reconcile the bottom count, Solitaire draw, resulting hand, first main, and visible game log",
        ));
    }
    Ok(())
}

fn legal_actions_for_state_v1(
    state: &MtgoOfflineBottomingStateV1,
) -> Vec<MtgoOfflineBottomingActionSemanticV1> {
    if state.done_visible {
        return vec![
            MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
            MtgoOfflineBottomingActionSemanticV1::CancelBottoming,
        ];
    }
    let selection_ordinal = state.selected_count + 1;
    let mut actions = state
        .visible_remaining_object_ids
        .iter()
        .map(
            |adapter_object_id| MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
                adapter_object_id: adapter_object_id.clone(),
                selection_ordinal,
            },
        )
        .collect::<Vec<_>>();
    if state.selected_count > 0 {
        actions.push(MtgoOfflineBottomingActionSemanticV1::CancelBottoming);
    }
    actions
}

fn require_frame_binding_v1(
    manifest_sha256: &str,
    frame_sha256: &str,
    frame: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
) -> Result<(), MtgoContractErrorV1> {
    if manifest_sha256 != frame.manifest_sha256() || frame_sha256 != frame.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "offline_bottoming_frame_binding",
            "recorded manifest and canonical frame must match the checked artifact",
        ));
    }
    Ok(())
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

fn validate_visible_card_name_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty() || value.len() > 96 || value.chars().any(char::is_control) {
        return Err(error_v1(
            "offline_bottoming_visible_card_name",
            "visible card name must be bounded printable text",
        ));
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

    fn hash(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn fixture_v1() -> MtgoOfflineLondonBottomingTraceV1 {
        let original_hand: Vec<_> = [
            "Forest", "Plains", "Plains", "Swamp", "Island", "Plains", "Swamp",
        ]
        .into_iter()
        .enumerate()
        .map(
            |(index, visible_card_name)| MtgoOfflineBottomingOriginalCardV1 {
                adapter_object_id: format!("opening-hand:{index}"),
                visible_card_name: visible_card_name.to_owned(),
                original_hand_ordinal: u8::try_from(index).unwrap(),
            },
        )
        .collect();
        let selected_for_bottom_click_order: Vec<_> = (0..6)
            .map(|index| format!("opening-hand:{index}"))
            .collect();
        let states = (0..=6)
            .map(|selected_count| MtgoOfflineBottomingStateV1 {
                selected_count,
                source_manifest_sha256: hash('a'),
                source_frame_sha256: hash('b'),
                visible_remaining_object_ids: (selected_count..7)
                    .map(|index| format!("opening-hand:{index}"))
                    .collect(),
                done_visible: selected_count == 6,
                cancel_visible: true,
                prompt_reconciled: true,
                legal_action_set_complete: true,
            })
            .collect();
        MtgoOfflineLondonBottomingTraceV1 {
            schema_version: 1,
            trace_id: "synthetic-bottom-six-v1".to_owned(),
            required_bottom_count: 6,
            original_hand,
            selected_for_bottom_click_order,
            states,
            completion: MtgoOfflineBottomingCompletionV1 {
                source_manifest_sha256: hash('c'),
                source_frame_sha256: hash('d'),
                bottomed_card_count: 6,
                began_game_hand_size: 1,
                solitaire_draw_observed: true,
                resulting_visible_hand_size: 2,
                first_main_visible: true,
                visible_game_log_reconciled: true,
            },
        }
    }

    #[test]
    fn complete_sequence_generates_current_legal_actions() {
        let record = fixture_v1();
        validate_record_structure_v1(&record).unwrap();
        let first = legal_actions_for_state_v1(&record.states[0]);
        assert_eq!(first.len(), 7);
        assert_eq!(
            first[0],
            MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
                adapter_object_id: "opening-hand:0".to_owned(),
                selection_ordinal: 1,
            }
        );
        assert!(!first.contains(&MtgoOfflineBottomingActionSemanticV1::CancelBottoming));
        assert_eq!(
            legal_actions_for_state_v1(&record.states[6]),
            [
                MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
                MtgoOfflineBottomingActionSemanticV1::CancelBottoming,
            ]
        );
    }

    #[test]
    fn reflow_order_and_selected_prefix_fail_closed() {
        let mut record = fixture_v1();
        record.states[3].visible_remaining_object_ids.swap(0, 1);
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_bottoming_state_remaining_hand"
        );

        let mut record = fixture_v1();
        record.selected_for_bottom_click_order[4] =
            record.selected_for_bottom_click_order[3].clone();
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_bottoming_selected_order"
        );
    }

    #[test]
    fn readiness_done_and_completion_fail_closed() {
        let mut record = fixture_v1();
        record.states[5].done_visible = true;
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_bottoming_state_readiness"
        );

        let mut record = fixture_v1();
        record.states[2].legal_action_set_complete = false;
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_bottoming_state_readiness"
        );

        let mut record = fixture_v1();
        record.completion.resulting_visible_hand_size = 1;
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_bottoming_completion"
        );
    }

    #[test]
    fn unknown_fields_and_bad_digests_fail_closed() {
        let record = fixture_v1();
        let encoded = serde_json::to_string(&record).unwrap();
        let altered = encoded.replacen('{', "{\"unknown\":true,", 1);
        assert!(serde_json::from_str::<MtgoOfflineLondonBottomingTraceV1>(&altered).is_err());

        let mut record = fixture_v1();
        record.states[0].source_frame_sha256 = "not-a-digest".to_owned();
        assert_eq!(
            validate_record_structure_v1(&record).err().unwrap().code(),
            "offline_bottoming_state_frame"
        );
    }
}
