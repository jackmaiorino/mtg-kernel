use super::*;

const BOTTOMING_ACTION_PLAN_DOMAIN_V5: &[u8] = b"mtgo-bottoming-action-plan-v5";
const BOTTOMING_CONTROL_PROFILE_DOMAIN_V5: &[u8] = b"mtgo-bottoming-control-profile-v5";
const BOTTOMING_PLAN_HISTORY_DOMAIN_V5: &[u8] = b"mtgo-bottoming-plan-bound-history-v5";
const BOTTOMING_CANCEL_RESET_HISTORY_DOMAIN_V5: &[u8] = b"mtgo-bottoming-cancel-reset-history-v5";
const BOTTOMING_SUBMIT_CONFIRMATION_DOMAIN_V5: &[u8] = b"mtgo-bottoming-submit-confirmation-v5";

const REVIEWED_CLIENT_WIDTH_V5: u32 = 1_550;
const REVIEWED_CLIENT_HEIGHT_V5: u32 = 925;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "postcondition_kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MtgoPlannedBottomingPostconditionV5 {
    OneCardRemoved {
        expected_selected_count: u8,
        expected_visible_hand_count: u8,
    },
    AllSelectionsReset {
        prior_selected_count: u8,
        expected_selected_count: u8,
        expected_visible_hand_count: u8,
    },
    GameplayFirstMainAfterSubmit {
        bottomed_card_count: u8,
        began_game_hand_size: u8,
    },
}

struct BottomingActionPlanPartsV5 {
    control_profile_commitment_sha256: String,
    #[allow(dead_code)]
    control_id: String,
    #[allow(dead_code)]
    control_rect_client_px: MtgoRectPxV1,
    #[allow(dead_code)]
    target_point_client_px: ClientPointV3,
    observed_control_region_sha256: String,
    source_transition_identity_sha256: String,
    planned_postcondition: MtgoPlannedBottomingPostconditionV5,
    action_plan_commitment_sha256: String,
}

/// A coordinate-private plan for one model-selected London-bottom action.
/// The plan binds the exact opaque session, model selection, reflowed current
/// card, Cancel control, or Done control, source pixels, and required visible
/// postcondition. It has no actuator or live-input conversion. A planned
/// Cancel is available only after one or more selections and requires a newer
/// complete seven-card, zero-selected identity measurement.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoBottomingActionPlanV5;
/// let _forged = OpaqueMtgoBottomingActionPlanV5 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoBottomingActionPlanV5;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoBottomingActionPlanV5>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoBottomingActionPlanV5;
/// fn coordinate_escape(value: &OpaqueMtgoBottomingActionPlanV5) {
///     let _ = value.target_point_client_px_v5();
/// }
/// ```
pub struct OpaqueMtgoBottomingActionPlanV5 {
    selection: OpaqueMtgoCardAwareBottomingModelSelectionV5,
    parts: BottomingActionPlanPartsV5,
}

impl OpaqueMtgoBottomingActionPlanV5 {
    pub fn selected_semantic_v5(&self) -> &MtgoOfflineBottomingActionSemanticV1 {
        self.selection.selected_semantic_v5()
    }

    pub fn planned_postcondition_v5(&self) -> &MtgoPlannedBottomingPostconditionV5 {
        &self.parts.planned_postcondition
    }

    pub fn control_profile_commitment_sha256_v5(&self) -> &str {
        &self.parts.control_profile_commitment_sha256
    }

    pub fn observed_control_region_sha256_v5(&self) -> &str {
        &self.parts.observed_control_region_sha256
    }

    pub fn source_capture_commitment_sha256_v5(&self) -> &str {
        &self.selection.request.source_capture_commitment_sha256
    }

    pub fn selection_commitment_sha256_v5(&self) -> &str {
        self.selection.selection_commitment_sha256_v5()
    }

    pub fn action_plan_commitment_sha256_v5(&self) -> &str {
        &self.parts.action_plan_commitment_sha256
    }

    pub fn safe_for_live_input_v5(&self) -> bool {
        false
    }

    pub fn safe_for_purchase_v5(&self) -> bool {
        false
    }

    pub fn safe_for_queue_entry_v5(&self) -> bool {
        false
    }
}

/// A visible first-main confirmation for a planned London-bottom Submit. It
/// proves exact request, plan, process, window, layout, frame progression, and
/// current first-main classification binding only. It does not prove that one
/// particular input caused the transition and cannot enable another input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedBottomingSubmitV5;
/// let _forged = OpaqueMtgoConfirmedBottomingSubmitV5 {};
/// ```
pub struct OpaqueMtgoConfirmedBottomingSubmitV5 {
    plan: OpaqueMtgoBottomingActionPlanV5,
    after: OpaqueMtgoDxgiFirstMainMeasurementV3,
    confirmation_commitment_sha256: String,
}

impl OpaqueMtgoConfirmedBottomingSubmitV5 {
    pub fn source_action_plan_commitment_sha256_v5(&self) -> &str {
        self.plan.action_plan_commitment_sha256_v5()
    }

    pub fn resulting_measurement_commitment_sha256_v5(&self) -> &str {
        self.after.measurement_commitment_sha256_v3()
    }

    pub fn confirmation_commitment_sha256_v5(&self) -> &str {
        &self.confirmation_commitment_sha256
    }

    pub fn bottomed_card_count_v5(&self) -> u8 {
        6
    }

    pub fn began_game_hand_size_v5(&self) -> u8 {
        1
    }

    pub fn safe_for_live_input_v5(&self) -> bool {
        false
    }
}

pub fn build_card_aware_bottoming_action_plan_v5(
    selection: OpaqueMtgoCardAwareBottomingModelSelectionV5,
) -> Result<OpaqueMtgoBottomingActionPlanV5, String> {
    validate_session_state_v5(&selection.session)?;
    let source = &selection.session.current_state.source_frame;
    let capture = source.commitments_v3();
    let source_transition_identity_sha256 =
        pregame_transition_identity_commitment_v3(&source.manifest)?;
    let parts = build_bottoming_action_plan_parts_v5(
        selection.selected_semantic_v5(),
        &selection.request,
        selection.selection_commitment_sha256_v5(),
        &capture,
        &source_transition_identity_sha256,
        &source.canonical_bgra8,
    )?;
    Ok(OpaqueMtgoBottomingActionPlanV5 { selection, parts })
}

pub fn confirm_card_aware_bottoming_selection_plan_v5(
    plan: OpaqueMtgoBottomingActionPlanV5,
    after: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
) -> Result<OpaqueMtgoCardAwareBottomingSessionV5, String> {
    let OpaqueMtgoBottomingActionPlanV5 { selection, parts } = plan;
    let (expected_selected_count, expected_visible_hand_count) = match parts.planned_postcondition {
        MtgoPlannedBottomingPostconditionV5::OneCardRemoved {
            expected_selected_count,
            expected_visible_hand_count,
        } => (expected_selected_count, expected_visible_hand_count),
        _ => {
            return Err(
                "only a selected-card bottoming plan can confirm a one-card transition".to_owned(),
            )
        }
    };
    let action_plan_commitment_sha256 = parts.action_plan_commitment_sha256;
    let mut session = confirm_card_aware_bottom_selection_v5(selection, after)?;
    if session.selected_count_v5() != expected_selected_count
        || session.visible_card_count_v5() != expected_visible_hand_count
    {
        return Err("confirmed bottoming plan did not reach its exact declared count".to_owned());
    }

    #[derive(Serialize)]
    struct PlanBoundHistoryRecordV5<'a> {
        action_plan_commitment_sha256: &'a str,
        confirmed_history_commitment_sha256: &'a str,
        resulting_state_measurement_commitment_sha256: &'a str,
        expected_selected_count: u8,
        expected_visible_hand_count: u8,
    }
    let plan_bound_history = canonical_json_commitment_v3(
        BOTTOMING_PLAN_HISTORY_DOMAIN_V5,
        &PlanBoundHistoryRecordV5 {
            action_plan_commitment_sha256: &action_plan_commitment_sha256,
            confirmed_history_commitment_sha256: &session.history_commitment_sha256,
            resulting_state_measurement_commitment_sha256: session
                .current_state
                .measurement_commitment_sha256_v3(),
            expected_selected_count,
            expected_visible_hand_count,
        },
    )?;
    session.history_commitment_sha256 = plan_bound_history;
    validate_session_state_v5(&session)?;
    Ok(session)
}

pub fn confirm_card_aware_bottoming_submit_plan_v5(
    plan: OpaqueMtgoBottomingActionPlanV5,
    after: OpaqueMtgoDxgiFirstMainMeasurementV3,
) -> Result<OpaqueMtgoConfirmedBottomingSubmitV5, String> {
    let source_capture = plan
        .selection
        .session
        .current_state
        .source_capture_commitments_v3();
    let after_capture = after.source_capture_commitments_v3();
    let after_transition_identity_sha256 =
        pregame_transition_identity_commitment_v3(&after.source_frame.manifest)?;
    let confirmation_commitment_sha256 = validate_bottoming_submit_postcondition_parts_v5(
        plan.action_plan_commitment_sha256_v5(),
        plan.selected_semantic_v5(),
        plan.planned_postcondition_v5(),
        &source_capture,
        &plan.parts.source_transition_identity_sha256,
        after.classification_v3(),
        after.profile_commitment_sha256_v3(),
        after.measurement_commitment_sha256_v3(),
        &after_capture,
        &after_transition_identity_sha256,
    )?;
    Ok(OpaqueMtgoConfirmedBottomingSubmitV5 {
        plan,
        after,
        confirmation_commitment_sha256,
    })
}

pub fn confirm_card_aware_bottoming_cancel_plan_v5(
    plan: OpaqueMtgoBottomingActionPlanV5,
    after: OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3,
) -> Result<OpaqueMtgoCardAwareBottomingSessionV5, String> {
    let OpaqueMtgoBottomingActionPlanV5 { selection, parts } = plan;
    let prior_selected_count = match parts.planned_postcondition {
        MtgoPlannedBottomingPostconditionV5::AllSelectionsReset {
            prior_selected_count,
            expected_selected_count: 0,
            expected_visible_hand_count: 7,
        } if (1..=6).contains(&prior_selected_count) => prior_selected_count,
        _ => {
            return Err(
                "only a calibrated nonzero-stage Cancel plan can confirm a reset".to_owned(),
            )
        }
    };
    let OpaqueMtgoCardAwareBottomingModelSelectionV5 {
        session,
        selected_semantic,
        ..
    } = selection;
    validate_session_state_v5(&session)?;
    if selected_semantic != MtgoOfflineBottomingActionSemanticV1::CancelBottoming
        || session.selected_count_v5() != prior_selected_count
    {
        return Err("bottoming Cancel plan does not bind the current selected count".to_owned());
    }

    let source_capture = session.current_state.source_capture_commitments_v3();
    let after_capture = after.source.source_capture_commitments_v3();
    let source_transition_identity_sha256 = parts.source_transition_identity_sha256;
    let after_transition_identity_sha256 =
        pregame_transition_identity_commitment_v3(&after.source.source_frame.manifest)?;
    validate_pregame_capture_progression_v3(
        &source_capture,
        &source_transition_identity_sha256,
        &after_capture,
        &after_transition_identity_sha256,
    )?;

    let fresh = start_card_aware_bottoming_session_v5(after)?;
    if fresh.profile.profile_commitment_sha256() != session.profile.profile_commitment_sha256() {
        return Err("bottoming Cancel reset changed the reviewed card profile".to_owned());
    }

    let OpaqueMtgoCardAwareBottomingSessionV5 {
        current_state: _,
        profile,
        current_identity: _,
        current_cards,
        confirmed_bottomed_cards,
        history_commitment_sha256,
    } = session;
    let restored_cards = restore_original_cards_after_cancel_v5(
        current_cards,
        confirmed_bottomed_cards,
        &fresh.current_cards,
    )?;
    let prior_history_commitment_sha256 = history_commitment_sha256;
    let after_measurement_commitment_sha256 = fresh
        .current_state
        .measurement_commitment_sha256_v3()
        .to_owned();
    let after_identity_commitment_sha256 = fresh
        .current_identity
        .as_ref()
        .ok_or("bottoming Cancel reset requires all seven visible identities")?
        .candidate_commitment_sha256()
        .to_owned();

    #[derive(Serialize)]
    struct CancelResetHistoryRecordV5<'a> {
        prior_history_commitment_sha256: &'a str,
        action_plan_commitment_sha256: &'a str,
        prior_selected_count: u8,
        source_capture_commitment_sha256: &'a str,
        source_transition_identity_sha256: &'a str,
        after_capture_commitment_sha256: &'a str,
        after_transition_identity_sha256: &'a str,
        after_measurement_commitment_sha256: &'a str,
        after_identity_commitment_sha256: &'a str,
        restored_cards: &'a [StableBottomingCardV5],
    }
    let reset_history_commitment_sha256 = canonical_json_commitment_v3(
        BOTTOMING_CANCEL_RESET_HISTORY_DOMAIN_V5,
        &CancelResetHistoryRecordV5 {
            prior_history_commitment_sha256: &prior_history_commitment_sha256,
            action_plan_commitment_sha256: &parts.action_plan_commitment_sha256,
            prior_selected_count,
            source_capture_commitment_sha256: &source_capture.capture_commitment_sha256,
            source_transition_identity_sha256: &source_transition_identity_sha256,
            after_capture_commitment_sha256: &after_capture.capture_commitment_sha256,
            after_transition_identity_sha256: &after_transition_identity_sha256,
            after_measurement_commitment_sha256: &after_measurement_commitment_sha256,
            after_identity_commitment_sha256: &after_identity_commitment_sha256,
            restored_cards: &restored_cards,
        },
    )?;
    let reset = OpaqueMtgoCardAwareBottomingSessionV5 {
        current_state: fresh.current_state,
        profile,
        current_identity: fresh.current_identity,
        current_cards: restored_cards,
        confirmed_bottomed_cards: Vec::new(),
        history_commitment_sha256: reset_history_commitment_sha256,
    };
    validate_session_state_v5(&reset)?;
    if reset.selected_count_v5() != 0 || reset.visible_card_count_v5() != 7 {
        return Err("bottoming Cancel did not restore the complete initial hand".to_owned());
    }
    Ok(reset)
}

fn build_bottoming_action_plan_parts_v5(
    selected_semantic: &MtgoOfflineBottomingActionSemanticV1,
    request: &MtgoCardAwareBottomingScoringRequestV5,
    selection_commitment_sha256: &str,
    source_capture: &MtgoDxgiFrameCommitmentsV3,
    source_transition_identity_sha256: &str,
    canonical_bgra8: &[u8],
) -> Result<BottomingActionPlanPartsV5, String> {
    validate_card_aware_bottoming_scoring_request_v5(request)?;
    for digest in [
        selection_commitment_sha256,
        &request.source_capture_commitment_sha256,
        &request.state_measurement_commitment_sha256,
        &request.history_commitment_sha256,
        &source_capture.capture_commitment_sha256,
        &source_capture.canonical_bgra8_sha256,
        source_transition_identity_sha256,
    ] {
        require_lower_sha256_v3(digest, "bottoming action plan commitment")?;
    }
    if request.source_capture_commitment_sha256 != source_capture.capture_commitment_sha256 {
        return Err("bottoming action plan request does not bind the source capture".to_owned());
    }
    let client_size_px = MtgoSizePxV1 {
        width: source_capture.canonical_width,
        height: source_capture.canonical_height,
    };
    if client_size_px.width != REVIEWED_CLIENT_WIDTH_V5
        || client_size_px.height != REVIEWED_CLIENT_HEIGHT_V5
    {
        return Err("bottoming action plan requires the reviewed 1550 by 925 client".to_owned());
    }
    let expected_len = usize::try_from(client_size_px.width)
        .ok()
        .and_then(|width| {
            usize::try_from(client_size_px.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("bottoming action plan pixel length overflow")?;
    if canonical_bgra8.len() != expected_len
        || format!("{:x}", Sha256::digest(canonical_bgra8)) != source_capture.canonical_bgra8_sha256
    {
        return Err("bottoming action plan pixels do not bind the source capture".to_owned());
    }

    let (control_id, control_rect_client_px, planned_postcondition) = match selected_semantic {
        MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
            adapter_object_id,
            selection_ordinal,
        } if request.selected_count < 6 && *selection_ordinal == request.selected_count + 1 => {
            let matches = request
                .ordered_visible_cards
                .iter()
                .filter(|card| card.adapter_object_id == *adapter_object_id)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err("selected bottoming object is not uniquely current".to_owned());
            }
            let card = matches[0];
            let visible_count = u8::try_from(request.ordered_visible_cards.len())
                .map_err(|_| "bottoming visible-card count overflow")?;
            let rect = bottoming_card_target_rect_v5(visible_count, card.current_visible_ordinal)?;
            (
                format!("select:{}", card.adapter_object_id),
                rect,
                MtgoPlannedBottomingPostconditionV5::OneCardRemoved {
                    expected_selected_count: request.selected_count + 1,
                    expected_visible_hand_count: visible_count - 1,
                },
            )
        }
        MtgoOfflineBottomingActionSemanticV1::SubmitBottoming if request.selected_count == 6 => (
            "submit_bottoming".to_owned(),
            done_control_rect_v5(),
            MtgoPlannedBottomingPostconditionV5::GameplayFirstMainAfterSubmit {
                bottomed_card_count: 6,
                began_game_hand_size: 1,
            },
        ),
        MtgoOfflineBottomingActionSemanticV1::CancelBottoming
            if (1..=6).contains(&request.selected_count) =>
        {
            (
                "cancel_bottoming".to_owned(),
                cancel_control_rect_v5(request.selected_count)?,
                MtgoPlannedBottomingPostconditionV5::AllSelectionsReset {
                    prior_selected_count: request.selected_count,
                    expected_selected_count: 0,
                    expected_visible_hand_count: 7,
                },
            )
        }
        _ => {
            return Err(
                "selected bottoming semantic does not match the current measured stage".to_owned(),
            )
        }
    };
    let target_point_client_px = ClientPointV3 {
        x: control_rect_client_px.x + control_rect_client_px.width / 2,
        y: control_rect_client_px.y + control_rect_client_px.height / 2,
    };
    let observed_control_region_sha256 =
        hash_bgra_region_for_plan_v3(canonical_bgra8, &client_size_px, &control_rect_client_px)?;
    let control_profile_commitment_sha256 = bottoming_control_profile_commitment_v5()?;

    #[derive(Serialize)]
    struct ActionPlanRecordV5<'a> {
        schema_version: u32,
        control_profile_commitment_sha256: &'a str,
        state_profile_commitment_sha256: &'a str,
        visible_card_profile_commitment_sha256: &'a str,
        deployment_commitment_sha256: &'a str,
        selection_commitment_sha256: &'a str,
        state_measurement_commitment_sha256: &'a str,
        identity_measurement_commitment_sha256: Option<&'a str>,
        history_commitment_sha256: &'a str,
        source_capture_commitment_sha256: &'a str,
        source_transition_identity_sha256: &'a str,
        selected_count: u8,
        selected_semantic: &'a MtgoOfflineBottomingActionSemanticV1,
        control_id: &'a str,
        control_rect_client_px: &'a MtgoRectPxV1,
        target_point_client_px: ClientPointV3,
        observed_control_region_sha256: &'a str,
        planned_postcondition: &'a MtgoPlannedBottomingPostconditionV5,
    }
    let action_plan_commitment_sha256 = canonical_json_commitment_v3(
        BOTTOMING_ACTION_PLAN_DOMAIN_V5,
        &ActionPlanRecordV5 {
            schema_version: MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5,
            control_profile_commitment_sha256: &control_profile_commitment_sha256,
            state_profile_commitment_sha256: &request.state_profile_commitment_sha256,
            visible_card_profile_commitment_sha256: &request.visible_card_profile_commitment_sha256,
            deployment_commitment_sha256: &request.deployment_commitment_sha256,
            selection_commitment_sha256,
            state_measurement_commitment_sha256: &request.state_measurement_commitment_sha256,
            identity_measurement_commitment_sha256: request
                .current_identity_measurement_commitment_sha256
                .as_deref(),
            history_commitment_sha256: &request.history_commitment_sha256,
            source_capture_commitment_sha256: &source_capture.capture_commitment_sha256,
            source_transition_identity_sha256,
            selected_count: request.selected_count,
            selected_semantic,
            control_id: &control_id,
            control_rect_client_px: &control_rect_client_px,
            target_point_client_px,
            observed_control_region_sha256: &observed_control_region_sha256,
            planned_postcondition: &planned_postcondition,
        },
    )?;
    Ok(BottomingActionPlanPartsV5 {
        control_profile_commitment_sha256,
        control_id,
        control_rect_client_px,
        target_point_client_px,
        observed_control_region_sha256,
        source_transition_identity_sha256: source_transition_identity_sha256.to_owned(),
        planned_postcondition,
        action_plan_commitment_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_bottoming_submit_postcondition_parts_v5(
    action_plan_commitment_sha256: &str,
    selected_semantic: &MtgoOfflineBottomingActionSemanticV1,
    planned_postcondition: &MtgoPlannedBottomingPostconditionV5,
    source_capture: &MtgoDxgiFrameCommitmentsV3,
    source_transition_identity_sha256: &str,
    after_classification: MtgoOfflineFirstMainClassificationV1,
    after_profile_commitment_sha256: &str,
    after_measurement_commitment_sha256: &str,
    after_capture: &MtgoDxgiFrameCommitmentsV3,
    after_transition_identity_sha256: &str,
) -> Result<String, String> {
    if selected_semantic != &MtgoOfflineBottomingActionSemanticV1::SubmitBottoming
        || planned_postcondition
            != &(MtgoPlannedBottomingPostconditionV5::GameplayFirstMainAfterSubmit {
                bottomed_card_count: 6,
                began_game_hand_size: 1,
            })
    {
        return Err("bottoming Submit confirmation requires the exact final-stage plan".to_owned());
    }
    if after_classification != MtgoOfflineFirstMainClassificationV1::Match {
        return Err("bottoming Submit did not reach the reviewed first-main state".to_owned());
    }
    for digest in [
        action_plan_commitment_sha256,
        after_measurement_commitment_sha256,
        after_profile_commitment_sha256,
        source_transition_identity_sha256,
        after_transition_identity_sha256,
    ] {
        require_lower_sha256_v3(digest, "bottoming Submit confirmation commitment")?;
    }
    validate_pregame_capture_progression_v3(
        source_capture,
        source_transition_identity_sha256,
        after_capture,
        after_transition_identity_sha256,
    )?;

    #[derive(Serialize)]
    struct ConfirmationRecordV5<'a> {
        action_plan_commitment_sha256: &'a str,
        selected_semantic: &'a MtgoOfflineBottomingActionSemanticV1,
        planned_postcondition: &'a MtgoPlannedBottomingPostconditionV5,
        source_capture_commitment_sha256: &'a str,
        source_frame_sha256: &'a str,
        after_capture_commitment_sha256: &'a str,
        after_frame_sha256: &'a str,
        source_transition_identity_sha256: &'a str,
        after_transition_identity_sha256: &'a str,
        after_profile_commitment_sha256: &'a str,
        after_measurement_commitment_sha256: &'a str,
    }
    canonical_json_commitment_v3(
        BOTTOMING_SUBMIT_CONFIRMATION_DOMAIN_V5,
        &ConfirmationRecordV5 {
            action_plan_commitment_sha256,
            selected_semantic,
            planned_postcondition,
            source_capture_commitment_sha256: &source_capture.capture_commitment_sha256,
            source_frame_sha256: &source_capture.canonical_bgra8_sha256,
            after_capture_commitment_sha256: &after_capture.capture_commitment_sha256,
            after_frame_sha256: &after_capture.canonical_bgra8_sha256,
            source_transition_identity_sha256,
            after_transition_identity_sha256,
            after_profile_commitment_sha256,
            after_measurement_commitment_sha256,
        },
    )
}

fn bottoming_card_target_rect_v5(
    visible_hand_count: u8,
    current_visible_ordinal: u8,
) -> Result<MtgoRectPxV1, String> {
    if !(1..=7).contains(&visible_hand_count) || current_visible_ordinal >= visible_hand_count {
        return Err(
            "bottoming target requires a current ordinal within one to seven cards".to_owned(),
        );
    }
    let left = if visible_hand_count == 7 {
        [323_u32, 442, 561, 679, 798, 917, 1_035][usize::from(current_visible_ordinal)]
    } else {
        323_u32
            .checked_add(128_u32 * u32::from(current_visible_ordinal))
            .ok_or("bottoming target x overflow")?
    };
    Ok(MtgoRectPxV1 {
        x: left + 7,
        y: 760,
        width: 96,
        height: 60,
    })
}

fn done_control_rect_v5() -> MtgoRectPxV1 {
    MtgoRectPxV1 {
        x: 30,
        y: 176,
        width: 53,
        height: 27,
    }
}

fn cancel_control_rect_v5(selected_count: u8) -> Result<MtgoRectPxV1, String> {
    match selected_count {
        1..=5 => Ok(MtgoRectPxV1 {
            x: 30,
            y: 176,
            width: 65,
            height: 27,
        }),
        6 => Ok(MtgoRectPxV1 {
            x: 92,
            y: 176,
            width: 64,
            height: 27,
        }),
        _ => Err("bottoming Cancel is not actionable at zero selected cards".to_owned()),
    }
}

fn restore_original_cards_after_cancel_v5(
    mut current_cards: Vec<StableBottomingCardV5>,
    confirmed_bottomed_cards: Vec<ConfirmedBottomedCardV5>,
    observed_cards: &[StableBottomingCardV5],
) -> Result<Vec<StableBottomingCardV5>, String> {
    current_cards.extend(
        confirmed_bottomed_cards
            .into_iter()
            .map(|record| record.card),
    );
    current_cards.sort_by_key(|card| card.original_hand_ordinal);
    if current_cards.len() != 7
        || observed_cards.len() != 7
        || current_cards.iter().enumerate().any(|(ordinal, card)| {
            usize::from(card.original_hand_ordinal) != ordinal
                || card.visible_card_name != observed_cards[ordinal].visible_card_name
        })
    {
        return Err(
            "bottoming Cancel reset did not restore the original ordered card identities"
                .to_owned(),
        );
    }
    Ok(current_cards)
}

fn bottoming_control_profile_commitment_v5() -> Result<String, String> {
    #[derive(Serialize)]
    struct ControlProfileV5 {
        profile_id: &'static str,
        client_size_px: MtgoSizePxV1,
        seven_card_left_edges: [u32; 7],
        reflow_left_edge: u32,
        reflow_stride: u32,
        card_art_offset_x: u32,
        card_art_y: u32,
        card_art_width: u32,
        card_art_height: u32,
        done_rect_client_px: MtgoRectPxV1,
        cancel_only_rect_client_px: MtgoRectPxV1,
        cancel_after_done_rect_client_px: MtgoRectPxV1,
        cancel_transition_calibrated: bool,
    }
    canonical_json_commitment_v3(
        BOTTOMING_CONTROL_PROFILE_DOMAIN_V5,
        &ControlProfileV5 {
            profile_id: "mtgo-bottoming-controls-1550x925-v5",
            client_size_px: MtgoSizePxV1 {
                width: REVIEWED_CLIENT_WIDTH_V5,
                height: REVIEWED_CLIENT_HEIGHT_V5,
            },
            seven_card_left_edges: [323, 442, 561, 679, 798, 917, 1_035],
            reflow_left_edge: 323,
            reflow_stride: 128,
            card_art_offset_x: 7,
            card_art_y: 760,
            card_art_width: 96,
            card_art_height: 60,
            done_rect_client_px: done_control_rect_v5(),
            cancel_only_rect_client_px: cancel_control_rect_v5(1)?,
            cancel_after_done_rect_client_px: cancel_control_rect_v5(6)?,
            cancel_transition_calibrated: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_v5(selected_count: u8) -> MtgoCardAwareBottomingScoringRequestV5 {
        let current_count = 7 - selected_count;
        let ordered_visible_cards = (0..current_count)
            .map(|ordinal| MtgoBottomingVisibleCardV5 {
                adapter_object_id: format!("object-{ordinal}"),
                original_hand_ordinal: ordinal,
                current_visible_ordinal: ordinal,
                visible_card_name: if ordinal % 2 == 0 {
                    "Plains".to_owned()
                } else {
                    "Island".to_owned()
                },
                identity_source: if selected_count < 6 {
                    MtgoBottomingCardIdentitySourceV5::CurrentVisibleTemplate
                } else {
                    MtgoBottomingCardIdentitySourceV5::ConfirmedActionHistory
                },
            })
            .collect::<Vec<_>>();
        let ordered_confirmed_bottomed_cards = (0..selected_count)
            .map(|index| MtgoBottomingConfirmedCardV5 {
                adapter_object_id: format!("object-{}", current_count + index),
                original_hand_ordinal: current_count + index,
                selection_ordinal: index + 1,
                visible_card_name: "Plains".to_owned(),
            })
            .collect::<Vec<_>>();
        let ordered_actions = canonical_actions_v5(&ordered_visible_cards, selected_count);
        MtgoCardAwareBottomingScoringRequestV5 {
            schema_version: MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5,
            source_capture_commitment_sha256: "a".repeat(64),
            state_measurement_commitment_sha256: "b".repeat(64),
            current_identity_measurement_commitment_sha256: (selected_count < 6)
                .then(|| "c".repeat(64)),
            state_profile_commitment_sha256: "d".repeat(64),
            visible_card_profile_commitment_sha256: "e".repeat(64),
            history_commitment_sha256: "f".repeat(64),
            required_bottom_count: 6,
            selected_count,
            ordered_visible_cards,
            ordered_confirmed_bottomed_cards,
            ordered_actions,
            deployment_commitment_sha256: "1".repeat(64),
        }
    }

    fn pixels_and_capture_v5() -> (Vec<u8>, MtgoDxgiFrameCommitmentsV3) {
        let pixels =
            vec![0_u8; (REVIEWED_CLIENT_WIDTH_V5 * REVIEWED_CLIENT_HEIGHT_V5 * 4) as usize];
        let frame_hash = format!("{:x}", Sha256::digest(&pixels));
        let capture = MtgoDxgiFrameCommitmentsV3 {
            capture_commitment_sha256: "a".repeat(64),
            canonical_bgra8_sha256: frame_hash,
            preview_png_sha256: "2".repeat(64),
            canonical_width: REVIEWED_CLIENT_WIDTH_V5,
            canonical_height: REVIEWED_CLIENT_HEIGHT_V5,
            client_rect_desktop_px: SignedRectV1 {
                left: 100,
                top: 100,
                right: 1_650,
                bottom: 1_025,
            },
            captured_at_unix_millis: 1_000,
        };
        (pixels, capture)
    }

    #[test]
    fn selection_and_submit_plans_bind_reflowed_controls() {
        let (pixels, capture) = pixels_and_capture_v5();
        let request = request_v5(0);
        let first = build_bottoming_action_plan_parts_v5(
            &request.ordered_actions[0],
            &request,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &pixels,
        )
        .unwrap();
        assert_eq!(
            first.control_rect_client_px,
            MtgoRectPxV1 {
                x: 330,
                y: 760,
                width: 96,
                height: 60,
            }
        );
        assert_eq!(
            first.planned_postcondition,
            MtgoPlannedBottomingPostconditionV5::OneCardRemoved {
                expected_selected_count: 1,
                expected_visible_hand_count: 6,
            }
        );

        let last_card = build_bottoming_action_plan_parts_v5(
            &request.ordered_actions[6],
            &request,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &pixels,
        )
        .unwrap();
        assert_eq!(last_card.control_rect_client_px.x, 1_042);

        let final_request = request_v5(6);
        let submit = build_bottoming_action_plan_parts_v5(
            &final_request.ordered_actions[0],
            &final_request,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &pixels,
        )
        .unwrap();
        assert_eq!(submit.control_rect_client_px, done_control_rect_v5());
        assert_eq!(
            submit.planned_postcondition,
            MtgoPlannedBottomingPostconditionV5::GameplayFirstMainAfterSubmit {
                bottomed_card_count: 6,
                began_game_hand_size: 1,
            }
        );
        assert_ne!(
            first.action_plan_commitment_sha256,
            submit.action_plan_commitment_sha256
        );
    }

    #[test]
    fn plans_bind_both_cancel_layouts_and_reject_zero_stage_cancel() {
        let (pixels, capture) = pixels_and_capture_v5();
        let request = request_v5(2);
        let cancel = request.ordered_actions.last().unwrap();
        let cancel_plan = build_bottoming_action_plan_parts_v5(
            cancel,
            &request,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &pixels,
        )
        .unwrap();
        assert_eq!(
            cancel_plan.control_rect_client_px,
            cancel_control_rect_v5(2).unwrap()
        );
        assert_eq!(
            cancel_plan.planned_postcondition,
            MtgoPlannedBottomingPostconditionV5::AllSelectionsReset {
                prior_selected_count: 2,
                expected_selected_count: 0,
                expected_visible_hand_count: 7,
            }
        );

        let final_request = request_v5(6);
        let final_cancel = build_bottoming_action_plan_parts_v5(
            final_request.ordered_actions.last().unwrap(),
            &final_request,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &pixels,
        )
        .unwrap();
        assert_eq!(
            final_cancel.control_rect_client_px,
            cancel_control_rect_v5(6).unwrap()
        );

        let zero = request_v5(0);
        assert!(!zero
            .ordered_actions
            .contains(&MtgoOfflineBottomingActionSemanticV1::CancelBottoming));
        assert!(build_bottoming_action_plan_parts_v5(
            &MtgoOfflineBottomingActionSemanticV1::CancelBottoming,
            &zero,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &pixels,
        )
        .is_err());
    }

    #[test]
    fn plans_reject_stale_object_wrong_stage_and_pixel_drift() {
        let (pixels, capture) = pixels_and_capture_v5();
        let request = request_v5(2);
        assert!(build_bottoming_action_plan_parts_v5(
            &MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
                adapter_object_id: "stale-object".to_owned(),
                selection_ordinal: 3,
            },
            &request,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &pixels,
        )
        .is_err());
        assert!(build_bottoming_action_plan_parts_v5(
            &MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
            &request,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &pixels,
        )
        .is_err());
        let mut changed_pixels = pixels.clone();
        changed_pixels[0] = 1;
        assert!(build_bottoming_action_plan_parts_v5(
            &request.ordered_actions[0],
            &request,
            &"3".repeat(64),
            &capture,
            &"4".repeat(64),
            &changed_pixels,
        )
        .is_err());
    }

    #[test]
    fn cancel_reset_restores_original_stable_objects_and_checks_labels() {
        fn card(ordinal: u8, name: &str, id_prefix: &str) -> StableBottomingCardV5 {
            StableBottomingCardV5 {
                adapter_object_id: format!("{id_prefix}-{ordinal}"),
                original_hand_ordinal: ordinal,
                visible_card_name: name.to_owned(),
            }
        }
        let names = [
            "Plains", "Island", "Plains", "Island", "Plains", "Island", "Plains",
        ];
        let current = (2..7)
            .map(|ordinal| card(ordinal, names[usize::from(ordinal)], "stable"))
            .collect();
        let confirmed = (0..2)
            .map(|ordinal| ConfirmedBottomedCardV5 {
                card: card(ordinal, names[usize::from(ordinal)], "stable"),
                selection_ordinal: ordinal + 1,
            })
            .collect();
        let observed = (0..7)
            .map(|ordinal| card(ordinal, names[usize::from(ordinal)], "fresh"))
            .collect::<Vec<_>>();
        let restored =
            restore_original_cards_after_cancel_v5(current, confirmed, &observed).unwrap();
        assert_eq!(restored.len(), 7);
        for (ordinal, restored) in restored.iter().enumerate() {
            assert_eq!(restored.adapter_object_id, format!("stable-{ordinal}"));
            assert_eq!(usize::from(restored.original_hand_ordinal), ordinal);
        }

        let current = (2..7)
            .map(|ordinal| card(ordinal, names[usize::from(ordinal)], "stable"))
            .collect();
        let confirmed = (0..2)
            .map(|ordinal| ConfirmedBottomedCardV5 {
                card: card(ordinal, names[usize::from(ordinal)], "stable"),
                selection_ordinal: ordinal + 1,
            })
            .collect();
        let mut mismatched = (0..7)
            .map(|ordinal| card(ordinal, names[usize::from(ordinal)], "fresh"))
            .collect::<Vec<_>>();
        mismatched[4].visible_card_name = "Mountain".to_owned();
        assert!(restore_original_cards_after_cancel_v5(current, confirmed, &mismatched).is_err());
    }

    #[test]
    fn submit_confirmation_requires_exact_new_first_main_state() {
        let (_, source) = pixels_and_capture_v5();
        let mut after = source.clone();
        after.capture_commitment_sha256 = "5".repeat(64);
        after.canonical_bgra8_sha256 = "6".repeat(64);
        after.captured_at_unix_millis += 1;
        let postcondition = MtgoPlannedBottomingPostconditionV5::GameplayFirstMainAfterSubmit {
            bottomed_card_count: 6,
            began_game_hand_size: 1,
        };
        let confirmed = validate_bottoming_submit_postcondition_parts_v5(
            &"7".repeat(64),
            &MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
            &postcondition,
            &source,
            &"8".repeat(64),
            MtgoOfflineFirstMainClassificationV1::Match,
            &"a".repeat(64),
            &"9".repeat(64),
            &after,
            &"8".repeat(64),
        )
        .unwrap();
        assert_eq!(confirmed.len(), 64);

        let mut stale = after.clone();
        stale.captured_at_unix_millis = source.captured_at_unix_millis;
        assert!(validate_bottoming_submit_postcondition_parts_v5(
            &"7".repeat(64),
            &MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
            &postcondition,
            &source,
            &"8".repeat(64),
            MtgoOfflineFirstMainClassificationV1::Match,
            &"a".repeat(64),
            &"9".repeat(64),
            &stale,
            &"8".repeat(64),
        )
        .is_err());
        assert!(validate_bottoming_submit_postcondition_parts_v5(
            &"7".repeat(64),
            &MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
            &postcondition,
            &source,
            &"8".repeat(64),
            MtgoOfflineFirstMainClassificationV1::NoMatch,
            &"a".repeat(64),
            &"9".repeat(64),
            &after,
            &"8".repeat(64),
        )
        .is_err());
    }
}
