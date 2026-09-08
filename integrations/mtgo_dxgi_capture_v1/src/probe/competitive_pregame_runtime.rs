use super::{
    choose_cursor_park_point_v3, competitive_entry_window_continuity_commitment_for_frame_v1,
    frame_id_from_capture_commitment_v1, invoke_verified_competitive_pregame_process_v1,
    invoke_verified_competitive_pregame_public_context_process_v1,
    mtgo_process_continuity_commitment_for_frame_v1, sha256_hex_v1,
    verify_duel_perception_runtime_identity_now_v1, MtgoAdmittedDuelVisibleFrameCommitmentsV1,
    MtgoDuelPerceptionFrameIdentityV1, OpaqueMtgoAdmittedDuelVisibleFrameV1,
    OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
};
use mtgo_blackbox_v1::{
    bind_untrusted_competitive_pregame_model_context_v1,
    check_untrusted_competitive_pregame_classifier_request_v1,
    check_untrusted_competitive_pregame_classifier_response_v1,
    check_untrusted_competitive_pregame_public_context_classifier_request_v1,
    check_untrusted_competitive_pregame_public_context_classifier_response_v1,
    AdmittedMtgoCompetitivePregameProfileV1, AdmittedMtgoCompetitivePregamePublicContextProfileV1,
    AdmittedMtgoDuelPerceptionProfileV1, CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    CheckedUntrustedMtgoCompetitivePregameModelContextV1,
    CheckedUntrustedMtgoCompetitivePregamePublicContextV1,
    MtgoCompetitivePregameClassifierRequestHeaderV1, MtgoCompetitivePregameClassifierResponseV1,
    MtgoCompetitivePregamePlayDrawV1, MtgoCompetitivePregamePublicContextClassifierRequestHeaderV1,
    MtgoCompetitivePregamePublicContextClassifierResponseV1, MtgoCompetitivePregameStageLabelV1,
    MtgoCompetitivePregameVisibleControlV1, MTGO_COMPETITIVE_PREGAME_CLASSIFIER_PROTOCOL_V1,
    MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1,
    MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_PROTOCOL_V1,
    MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_SCHEMA_V1,
};
use std::time::Duration;

/// Copyable commitments for one exact duel frame retained through the pinned
/// pregame classifier protocol. This telemetry is not a capture or input
/// capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifiedCompetitivePregameFrameCommitmentsV1 {
    pub source_frame: MtgoAdmittedDuelVisibleFrameCommitmentsV1,
    pub classifier_runtime_identity_commitment_sha256: String,
    pub pregame_evaluation_commitment_sha256: String,
    pub pregame_profile_admission_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub classification_commitment_sha256: String,
    pub visible_interaction_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub stage: MtgoCompetitivePregameStageLabelV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifiedCompetitivePregameModelContextCommitmentsV1 {
    pub source: MtgoClassifiedCompetitivePregameFrameCommitmentsV1,
    pub public_context_evaluation_commitment_sha256: String,
    pub public_context_profile_admission_commitment_sha256: String,
    pub public_context_request_commitment_sha256: String,
    pub public_context_result_commitment_sha256: String,
    pub public_context_commitment_sha256: String,
    pub model_context_binding_commitment_sha256: String,
    pub game_number: u8,
    pub play_draw: MtgoCompetitivePregamePlayDrawV1,
    pub acting_player_games_won: u8,
    pub opponent_games_won: u8,
}

#[cfg(test)]
pub(crate) fn competitive_pregame_model_context_commitments_for_tests_v1(
    stage: MtgoCompetitivePregameStageLabelV1,
    visible_interaction_commitment_sha256: String,
    game_number: u8,
    play_draw: MtgoCompetitivePregamePlayDrawV1,
    acting_player_games_won: u8,
    opponent_games_won: u8,
) -> MtgoClassifiedCompetitivePregameModelContextCommitmentsV1 {
    use super::{MtgoDxgiFrameCommitmentsV3, SignedRectV1};

    MtgoClassifiedCompetitivePregameModelContextCommitmentsV1 {
        source: MtgoClassifiedCompetitivePregameFrameCommitmentsV1 {
            source_frame: MtgoAdmittedDuelVisibleFrameCommitmentsV1 {
                perception_profile_commitment_sha256: "1".repeat(64),
                perception_profile_admission_commitment_sha256: "2".repeat(64),
                frame_profile_binding_sha256: "3".repeat(64),
                source_capture: MtgoDxgiFrameCommitmentsV3 {
                    capture_commitment_sha256: "4".repeat(64),
                    canonical_bgra8_sha256: "5".repeat(64),
                    preview_png_sha256: "6".repeat(64),
                    canonical_width: 1_550,
                    canonical_height: 925,
                    client_rect_desktop_px: SignedRectV1 {
                        left: 0,
                        top: 0,
                        right: 1_550,
                        bottom: 925,
                    },
                    captured_at_unix_millis: 1_000,
                },
            },
            classifier_runtime_identity_commitment_sha256: "7".repeat(64),
            pregame_evaluation_commitment_sha256: "8".repeat(64),
            pregame_profile_admission_commitment_sha256: "9".repeat(64),
            request_commitment_sha256: "a".repeat(64),
            classification_commitment_sha256: "b".repeat(64),
            visible_interaction_commitment_sha256,
            frame_id: 10,
            frame_sequence: 20,
            captured_at_unix_millis: 1_000,
            stage,
        },
        public_context_evaluation_commitment_sha256: "c".repeat(64),
        public_context_profile_admission_commitment_sha256: "d".repeat(64),
        public_context_request_commitment_sha256: "e".repeat(64),
        public_context_result_commitment_sha256: "f".repeat(64),
        public_context_commitment_sha256: "0".repeat(64),
        model_context_binding_commitment_sha256: "1".repeat(64),
        game_number,
        play_draw,
        acting_player_games_won,
        opponent_games_won,
    }
}

/// One in-process DXGI frame retained through the exact profile-pinned
/// classifier and pixel-backed pregame protocol. It is move-only and exposes
/// no pixels, rectangles, path, process handle, event entry, or input method.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitivePregameFrameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoClassifiedCompetitivePregameFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitivePregameFrameV1;
/// fn cannot_extract(value: &OpaqueMtgoClassifiedCompetitivePregameFrameV1) {
///     let _ = value.pixels();
///     let _ = value.control_rect();
/// }
/// ```
pub struct OpaqueMtgoClassifiedCompetitivePregameFrameV1 {
    pub(super) _source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    pub(super) _checked_classification: CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    pub(super) _response: MtgoCompetitivePregameClassifierResponseV1,
    commitments: MtgoClassifiedCompetitivePregameFrameCommitmentsV1,
}

/// One exact classified pregame frame and a second, independently evaluated
/// public-context classification over the same retained pixels. The model
/// context is move-only and exposes no pixels, rectangles, process handle,
/// action target, event-entry method, or input method.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitivePregameModelContextV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoClassifiedCompetitivePregameModelContextV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitivePregameModelContextV1;
/// fn cannot_extract(value: &OpaqueMtgoClassifiedCompetitivePregameModelContextV1) {
///     let _ = value.pixels_v1();
///     let _ = value.visible_cards_v1();
///     let _ = value.control_rect_v1();
/// }
/// ```
pub struct OpaqueMtgoClassifiedCompetitivePregameModelContextV1 {
    _source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    _pregame_response: MtgoCompetitivePregameClassifierResponseV1,
    _public_context_response: MtgoCompetitivePregamePublicContextClassifierResponseV1,
    _model_context: CheckedUntrustedMtgoCompetitivePregameModelContextV1,
    commitments: MtgoClassifiedCompetitivePregameModelContextCommitmentsV1,
}

pub(crate) struct OpaqueMtgoCompetitivePregamePublicContextWitnessV1 {
    _checked_public_context: CheckedUntrustedMtgoCompetitivePregamePublicContextV1,
    _response: MtgoCompetitivePregamePublicContextClassifierResponseV1,
    commitments: MtgoClassifiedCompetitivePregameModelContextCommitmentsV1,
}

impl OpaqueMtgoCompetitivePregamePublicContextWitnessV1 {
    pub(crate) fn commitments_v1(
        &self,
    ) -> &MtgoClassifiedCompetitivePregameModelContextCommitmentsV1 {
        &self.commitments
    }
}

impl OpaqueMtgoClassifiedCompetitivePregameModelContextV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitivePregameModelContextCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn game_number_v1(&self) -> u8 {
        self.commitments.game_number
    }

    pub fn play_draw_v1(&self) -> MtgoCompetitivePregamePlayDrawV1 {
        self.commitments.play_draw
    }

    pub fn match_score_v1(&self) -> (u8, u8) {
        (
            self.commitments.acting_player_games_won,
            self.commitments.opponent_games_won,
        )
    }

    pub fn safe_for_native_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub(crate) fn response_v1(&self) -> &MtgoCompetitivePregameClassifierResponseV1 {
        &self._pregame_response
    }

    pub(crate) fn into_classified_frame_and_public_context_witness_v1(
        self,
    ) -> (
        OpaqueMtgoClassifiedCompetitivePregameFrameV1,
        OpaqueMtgoCompetitivePregamePublicContextWitnessV1,
    ) {
        let OpaqueMtgoClassifiedCompetitivePregameModelContextV1 {
            _source_frame,
            _pregame_response,
            _public_context_response,
            _model_context,
            commitments,
        } = self;
        let (_checked_classification, _checked_public_context) =
            _model_context.into_checked_parts_v1();
        let frame = OpaqueMtgoClassifiedCompetitivePregameFrameV1 {
            _source_frame,
            _checked_classification,
            _response: _pregame_response,
            commitments: commitments.source.clone(),
        };
        let witness = OpaqueMtgoCompetitivePregamePublicContextWitnessV1 {
            _checked_public_context,
            _response: _public_context_response,
            commitments,
        };
        (frame, witness)
    }

    pub(crate) fn process_continuity_commitment_sha256_v1(&self) -> String {
        mtgo_process_continuity_commitment_for_frame_v1(&self._source_frame.source_frame)
    }

    pub(crate) fn window_continuity_commitment_sha256_v1(&self) -> Result<String, String> {
        competitive_entry_window_continuity_commitment_for_frame_v1(
            &self._source_frame.source_frame,
        )
    }

    pub(crate) fn require_immediate_successor_v1(
        &self,
        prior_frame_id: u64,
        prior_frame_sequence: u64,
        prior_source_capture_commitment_sha256: &str,
        prior_captured_at_unix_millis: u128,
    ) -> Result<(), String> {
        validate_immediate_successor_identity_v1(
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: &self
                    .commitments
                    .source
                    .source_frame
                    .source_capture
                    .capture_commitment_sha256,
                captured_at_unix_millis: self.commitments.source.captured_at_unix_millis,
                frame_id: self.commitments.source.frame_id,
                frame_sequence: self.commitments.source.frame_sequence,
            },
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: prior_source_capture_commitment_sha256,
                captured_at_unix_millis: prior_captured_at_unix_millis,
                frame_id: prior_frame_id,
                frame_sequence: prior_frame_sequence,
            },
        )
    }
}

pub(crate) struct MtgoCompetitivePregamePointerTargetV1 {
    pub hwnd: u64,
    pub process_id: u32,
    pub process_start_filetime_100ns: u64,
    pub dpi: u32,
    pub client_rect_desktop_px: crate::SignedRectV1,
    pub target_x_desktop_px: i32,
    pub target_y_desktop_px: i32,
    pub park_x_desktop_px: i32,
    pub park_y_desktop_px: i32,
}

impl OpaqueMtgoClassifiedCompetitivePregameFrameV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitivePregameFrameCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn stage_v1(&self) -> MtgoCompetitivePregameStageLabelV1 {
        self.commitments.stage
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub(crate) fn process_continuity_commitment_sha256_v1(&self) -> String {
        mtgo_process_continuity_commitment_for_frame_v1(&self._source_frame.source_frame)
    }

    pub(crate) fn response_v1(&self) -> &MtgoCompetitivePregameClassifierResponseV1 {
        &self._response
    }

    pub(crate) fn resolve_visible_control_pointer_target_v1(
        &self,
        control: &MtgoCompetitivePregameVisibleControlV1,
    ) -> Result<MtgoCompetitivePregamePointerTargetV1, String> {
        if self
            ._response
            .visible_controls
            .iter()
            .filter(|candidate| *candidate == control)
            .count()
            != 1
        {
            return Err(
                "competitive pregame pointer target is not one exact current visible control"
                    .to_owned(),
            );
        }
        let raw = &self._source_frame.source_frame;
        let width = raw.manifest.frame.canonical_width;
        let height = raw.manifest.frame.canonical_height;
        let rect = &control.rect_client_px;
        let right = rect
            .x
            .checked_add(rect.width)
            .ok_or("competitive pregame control x overflow")?;
        let bottom = rect
            .y
            .checked_add(rect.height)
            .ok_or("competitive pregame control y overflow")?;
        if rect.width == 0 || rect.height == 0 || right > width || bottom > height {
            return Err("competitive pregame control is outside the immediate client".to_owned());
        }
        let target_x_client_px = rect
            .x
            .checked_add(rect.width / 2)
            .ok_or("competitive pregame control center x overflow")?;
        let target_y_client_px = rect
            .y
            .checked_add(rect.height / 2)
            .ok_or("competitive pregame control center y overflow")?;
        let client_rect = raw.manifest.pre.client_rect_desktop_px;
        let target_x_desktop_px = client_rect
            .left
            .checked_add(
                i32::try_from(target_x_client_px)
                    .map_err(|_| "competitive pregame target x does not fit the desktop")?,
            )
            .ok_or("competitive pregame target x overflow")?;
        let target_y_desktop_px = client_rect
            .top
            .checked_add(
                i32::try_from(target_y_client_px)
                    .map_err(|_| "competitive pregame target y does not fit the desktop")?,
            )
            .ok_or("competitive pregame target y overflow")?;
        if !client_rect.contains_point(target_x_desktop_px, target_y_desktop_px) {
            return Err(
                "competitive pregame target point is outside the immediate client".to_owned(),
            );
        }
        let (park_x_desktop_px, park_y_desktop_px) =
            choose_cursor_park_point_v3(&client_rect, &raw.manifest.output.bounds_desktop_px)?;
        let pre = &raw.manifest.pre;
        Ok(MtgoCompetitivePregamePointerTargetV1 {
            hwnd: pre.hwnd,
            process_id: pre.process_id,
            process_start_filetime_100ns: pre.process_start_filetime_100ns,
            dpi: pre.dpi,
            client_rect_desktop_px: client_rect,
            target_x_desktop_px,
            target_y_desktop_px,
            park_x_desktop_px,
            park_y_desktop_px,
        })
    }

    pub(crate) fn window_continuity_commitment_sha256_v1(&self) -> Result<String, String> {
        competitive_entry_window_continuity_commitment_for_frame_v1(
            &self._source_frame.source_frame,
        )
    }

    pub(crate) fn require_immediate_successor_v1(
        &self,
        prior_frame_id: u64,
        prior_frame_sequence: u64,
        prior_source_capture_commitment_sha256: &str,
        prior_captured_at_unix_millis: u128,
    ) -> Result<(), String> {
        validate_immediate_successor_identity_v1(
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: &self
                    .commitments
                    .source_frame
                    .source_capture
                    .capture_commitment_sha256,
                captured_at_unix_millis: self.commitments.captured_at_unix_millis,
                frame_id: self.commitments.frame_id,
                frame_sequence: self.commitments.frame_sequence,
            },
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: prior_source_capture_commitment_sha256,
                captured_at_unix_millis: prior_captured_at_unix_millis,
                frame_id: prior_frame_id,
                frame_sequence: prior_frame_sequence,
            },
        )
    }
}

#[derive(Clone, Copy)]
struct ImmediateFrameIdentityViewV1<'a> {
    source_capture_commitment_sha256: &'a str,
    captured_at_unix_millis: u128,
    frame_id: u64,
    frame_sequence: u64,
}

fn validate_immediate_successor_identity_v1(
    current: ImmediateFrameIdentityViewV1<'_>,
    prior: ImmediateFrameIdentityViewV1<'_>,
) -> Result<(), String> {
    let expected_frame_sequence = prior
        .frame_sequence
        .checked_add(1)
        .ok_or("competitive pregame frame sequence overflow")?;
    let expected_frame_id = frame_id_from_capture_commitment_v1(
        current.source_capture_commitment_sha256,
        prior.frame_id,
    )?;
    if current.source_capture_commitment_sha256 == prior.source_capture_commitment_sha256
        || current.captured_at_unix_millis <= prior.captured_at_unix_millis
        || current.frame_sequence != expected_frame_sequence
        || current.frame_id != expected_frame_id
    {
        return Err(
            "competitive pregame frame is not the immediate capture-bound successor".to_owned(),
        );
    }
    Ok(())
}

/// Invokes the exact duel-profile classifier in its competitive-pregame mode.
/// Production cannot reach this function while either profile's independent
/// ratification root is empty. The returned source remains non-actionable;
/// event identity and input require later exact-lineage bindings.
pub fn classify_admitted_mtgo_competitive_pregame_frame_v1(
    source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    duel_profile: &AdmittedMtgoDuelPerceptionProfileV1,
    pregame_profile: &AdmittedMtgoCompetitivePregameProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    identity: MtgoDuelPerceptionFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitivePregameFrameV1, String> {
    if identity.frame_id == 0 || identity.frame_sequence == 0 {
        return Err("competitive pregame frame identity must be nonzero".to_owned());
    }
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("competitive pregame timeout must be between 100 and 60000 ms".to_owned());
    }

    let source_commitments = source_frame.commitments_v1();
    let runtime_commitments = runtime.commitments_v1();
    if source_commitments.perception_profile_commitment_sha256
        != duel_profile.perception_profile_commitment_sha256()
        || source_commitments.perception_profile_admission_commitment_sha256
            != duel_profile.admission_commitment_sha256()
        || runtime_commitments.perception_profile_commitment_sha256
            != duel_profile.perception_profile_commitment_sha256()
        || runtime_commitments.perception_profile_admission_commitment_sha256
            != duel_profile.admission_commitment_sha256()
        || pregame_profile.duel_perception_profile_commitment_sha256()
            != duel_profile.perception_profile_commitment_sha256()
        || pregame_profile.duel_perception_profile_admission_commitment_sha256()
            != duel_profile.admission_commitment_sha256()
    {
        return Err(
            "competitive pregame source, runtime, duel profile, and pregame profile differ"
                .to_owned(),
        );
    }
    verify_duel_perception_runtime_identity_now_v1(runtime)?;

    let source = &source_frame.source_frame;
    let width = source.manifest.frame.canonical_width;
    let height = source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("competitive pregame canonical stride overflow")?;
    if width != duel_profile.client_size_px().width
        || height != duel_profile.client_size_px().height
        || source.manifest.frame.canonical_stride != stride
        || source.manifest.frame.canonical_byte_length != source.canonical_bgra8.len()
        || source.manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&source.canonical_bgra8)
    {
        return Err(
            "competitive pregame source pixels differ from the admitted duel profile".to_owned(),
        );
    }

    let header = MtgoCompetitivePregameClassifierRequestHeaderV1 {
        schema_version: MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1,
        protocol: MTGO_COMPETITIVE_PREGAME_CLASSIFIER_PROTOCOL_V1.to_owned(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: source.canonical_bgra8.len(),
        canonical_bgra8_sha256: source.manifest.frame.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: source.capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: source_frame.frame_profile_binding_sha256.clone(),
        duel_perception_profile_commitment_sha256: duel_profile
            .perception_profile_commitment_sha256()
            .to_owned(),
        duel_perception_profile_admission_commitment_sha256: duel_profile
            .admission_commitment_sha256()
            .to_owned(),
        pregame_evaluation_commitment_sha256: pregame_profile
            .evaluation_commitment_sha256()
            .to_owned(),
        pregame_profile_admission_commitment_sha256: pregame_profile
            .admission_commitment_sha256()
            .to_owned(),
        classifier_runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256
            .clone(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize competitive pregame request: {error}"))?;
    let checked_request = check_untrusted_competitive_pregame_classifier_request_v1(
        duel_profile,
        pregame_profile,
        &header_json,
        &source.canonical_bgra8,
    )
    .map_err(|error| format!("check competitive pregame request: {error}"))?;
    let response = invoke_verified_competitive_pregame_process_v1(
        runtime,
        &header_json,
        &source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_duel_perception_runtime_identity_now_v1(runtime)?;
    let checked_classification = check_untrusted_competitive_pregame_classifier_response_v1(
        pregame_profile,
        &checked_request,
        &source.canonical_bgra8,
        &response,
    )
    .map_err(|error| format!("check competitive pregame response: {error}"))?;
    let response_record: MtgoCompetitivePregameClassifierResponseV1 =
        serde_json::from_slice(&response)
            .map_err(|error| format!("parse checked competitive pregame response: {error}"))?;

    if checked_classification.source_capture_commitment_sha256() != source.capture_commitment_sha256
        || checked_classification.source_frame_profile_binding_sha256()
            != source_frame.frame_profile_binding_sha256
        || checked_classification.classifier_runtime_identity_commitment_sha256()
            != runtime_commitments.runtime_identity_commitment_sha256
        || checked_classification.frame_id() != identity.frame_id
        || checked_classification.frame_sequence() != identity.frame_sequence
    {
        return Err("competitive pregame classification changed source lineage".to_owned());
    }
    let commitments = MtgoClassifiedCompetitivePregameFrameCommitmentsV1 {
        source_frame: source_commitments,
        classifier_runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256,
        pregame_evaluation_commitment_sha256: checked_classification
            .pregame_evaluation_commitment_sha256()
            .to_owned(),
        pregame_profile_admission_commitment_sha256: checked_classification
            .pregame_profile_admission_commitment_sha256()
            .to_owned(),
        request_commitment_sha256: checked_classification
            .request_commitment_sha256()
            .to_owned(),
        classification_commitment_sha256: checked_classification
            .classification_commitment_sha256()
            .to_owned(),
        visible_interaction_commitment_sha256: checked_classification
            .visible_interaction_commitment_sha256()
            .to_owned(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
        captured_at_unix_millis: source.manifest.captured_at_unix_millis,
        stage: checked_classification.stage(),
    };
    Ok(OpaqueMtgoClassifiedCompetitivePregameFrameV1 {
        _source_frame: source_frame,
        _checked_classification: checked_classification,
        _response: response_record,
        commitments,
    })
}

/// Runs the exact evaluation-bound public-context mode over the same retained
/// BGRA8 frame as the checked hand, prompt, and controls. The evaluated binary
/// digest must equal the already verified runtime binary digest. This produces
/// a typed future-model input but grants no current scoring or input authority.
pub fn classify_competitive_pregame_public_context_v1(
    source: OpaqueMtgoClassifiedCompetitivePregameFrameV1,
    profile: &AdmittedMtgoCompetitivePregamePublicContextProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitivePregameModelContextV1, String> {
    if !(100..=60_000).contains(&timeout_ms) {
        return Err(
            "competitive pregame public-context timeout must be between 100 and 60000 ms"
                .to_owned(),
        );
    }
    let runtime_commitments = runtime.commitments_v1();
    let source_commitments = source.commitments_v1();
    if source_commitments.classifier_runtime_identity_commitment_sha256
        != runtime_commitments.runtime_identity_commitment_sha256
        || source_commitments.pregame_evaluation_commitment_sha256
            != profile.pregame_evaluation_commitment_sha256()
        || source_commitments.pregame_profile_admission_commitment_sha256
            != profile.pregame_profile_admission_commitment_sha256()
        || runtime_commitments.perception_pipeline_binary_sha256
            != profile.classifier_binary_sha256()
    {
        return Err(
            "competitive pregame source, evaluated public context, and runtime differ".to_owned(),
        );
    }
    verify_duel_perception_runtime_identity_now_v1(runtime)?;

    let raw = &source._source_frame.source_frame;
    let width = raw.manifest.frame.canonical_width;
    let height = raw.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("competitive pregame public-context stride overflow")?;
    if raw.manifest.frame.canonical_stride != stride
        || raw.manifest.frame.canonical_byte_length != raw.canonical_bgra8.len()
        || raw.manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&raw.canonical_bgra8)
    {
        return Err("competitive pregame public-context source pixels changed".to_owned());
    }
    let header = MtgoCompetitivePregamePublicContextClassifierRequestHeaderV1 {
        schema_version: MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_SCHEMA_V1,
        protocol: MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_PROTOCOL_V1.to_owned(),
        frame_id: source_commitments.frame_id,
        frame_sequence: source_commitments.frame_sequence,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: raw.canonical_bgra8.len(),
        canonical_bgra8_sha256: raw.manifest.frame.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: raw.capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: source
            ._source_frame
            .frame_profile_binding_sha256
            .clone(),
        pregame_classification_commitment_sha256: source_commitments
            .classification_commitment_sha256
            .clone(),
        visible_interaction_commitment_sha256: source_commitments
            .visible_interaction_commitment_sha256
            .clone(),
        pregame_evaluation_commitment_sha256: source_commitments
            .pregame_evaluation_commitment_sha256
            .clone(),
        pregame_profile_admission_commitment_sha256: source_commitments
            .pregame_profile_admission_commitment_sha256
            .clone(),
        public_context_evaluation_commitment_sha256: profile
            .evaluation_commitment_sha256()
            .to_owned(),
        public_context_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        classifier_binary_sha256: profile.classifier_binary_sha256().to_owned(),
        classifier_runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256
            .clone(),
    };
    let header_json = serde_json::to_vec(&header).map_err(|error| {
        format!("serialize competitive pregame public-context request: {error}")
    })?;
    let checked_request = check_untrusted_competitive_pregame_public_context_classifier_request_v1(
        &source._checked_classification,
        profile,
        &header_json,
        &raw.canonical_bgra8,
    )
    .map_err(|error| format!("check competitive pregame public-context request: {error}"))?;
    let response = invoke_verified_competitive_pregame_public_context_process_v1(
        runtime,
        &header_json,
        &raw.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_duel_perception_runtime_identity_now_v1(runtime)?;
    let checked_context =
        check_untrusted_competitive_pregame_public_context_classifier_response_v1(
            profile,
            &checked_request,
            &raw.canonical_bgra8,
            &response,
        )
        .map_err(|error| format!("check competitive pregame public-context response: {error}"))?;
    if checked_context.classifier_binary_sha256()
        != runtime_commitments.perception_pipeline_binary_sha256
        || checked_context.classifier_runtime_identity_commitment_sha256()
            != runtime_commitments.runtime_identity_commitment_sha256
        || checked_context.frame_id() != source_commitments.frame_id
        || checked_context.frame_sequence() != source_commitments.frame_sequence
    {
        return Err("competitive pregame public-context result changed source lineage".to_owned());
    }
    let context_response: MtgoCompetitivePregamePublicContextClassifierResponseV1 =
        serde_json::from_slice(&response)
            .map_err(|error| format!("parse checked pregame public-context response: {error}"))?;
    let public_context_request_commitment_sha256 =
        checked_context.request_commitment_sha256().to_owned();
    let public_context_result_commitment_sha256 =
        checked_context.result_commitment_sha256().to_owned();
    let public_context_commitment_sha256 = checked_context
        .public_context_commitment_sha256()
        .to_owned();
    let game_number = checked_context.game_number();
    let play_draw = checked_context.play_draw();
    let (acting_player_games_won, opponent_games_won) = checked_context.match_score();

    let OpaqueMtgoClassifiedCompetitivePregameFrameV1 {
        _source_frame,
        _checked_classification,
        _response,
        commitments: _,
    } = source;
    let model_context = bind_untrusted_competitive_pregame_model_context_v1(
        _checked_classification,
        checked_context.into_public_context_v1(),
    )
    .map_err(|error| format!("bind competitive pregame model context: {error}"))?;
    let model_context_binding_commitment_sha256 =
        model_context.binding_commitment_sha256().to_owned();
    Ok(OpaqueMtgoClassifiedCompetitivePregameModelContextV1 {
        _source_frame,
        _pregame_response: _response,
        _public_context_response: context_response,
        _model_context: model_context,
        commitments: MtgoClassifiedCompetitivePregameModelContextCommitmentsV1 {
            source: source_commitments,
            public_context_evaluation_commitment_sha256: profile
                .evaluation_commitment_sha256()
                .to_owned(),
            public_context_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            public_context_request_commitment_sha256,
            public_context_result_commitment_sha256,
            public_context_commitment_sha256,
            model_context_binding_commitment_sha256,
            game_number,
            play_draw,
            acting_player_games_won,
            opponent_games_won,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immediate_successor_identity_is_capture_bound_and_strictly_newer() {
        let prior_capture = "1".repeat(64);
        let current_capture = "0123456789abcdef".to_owned() + &"0".repeat(48);
        let prior_frame_id = 77;
        let expected_frame_id =
            frame_id_from_capture_commitment_v1(&current_capture, prior_frame_id).unwrap();
        validate_immediate_successor_identity_v1(
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: &current_capture,
                captured_at_unix_millis: 1_001,
                frame_id: expected_frame_id,
                frame_sequence: 41,
            },
            ImmediateFrameIdentityViewV1 {
                source_capture_commitment_sha256: &prior_capture,
                captured_at_unix_millis: 1_000,
                frame_id: prior_frame_id,
                frame_sequence: 40,
            },
        )
        .unwrap();

        for (capture, captured_at, frame_id, frame_sequence) in [
            (prior_capture.as_str(), 1_001, expected_frame_id, 41),
            (current_capture.as_str(), 1_000, expected_frame_id, 41),
            (current_capture.as_str(), 1_001, expected_frame_id ^ 1, 41),
            (current_capture.as_str(), 1_001, expected_frame_id, 42),
        ] {
            assert!(validate_immediate_successor_identity_v1(
                ImmediateFrameIdentityViewV1 {
                    source_capture_commitment_sha256: capture,
                    captured_at_unix_millis: captured_at,
                    frame_id,
                    frame_sequence,
                },
                ImmediateFrameIdentityViewV1 {
                    source_capture_commitment_sha256: &prior_capture,
                    captured_at_unix_millis: 1_000,
                    frame_id: prior_frame_id,
                    frame_sequence: 40,
                },
            )
            .is_err());
        }
    }
}
