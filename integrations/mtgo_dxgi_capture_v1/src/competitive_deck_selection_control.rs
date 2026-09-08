use crate::actuator::{
    halt_before_direct_visible_input_attempt_v1, halt_direct_visible_input_gate_v1,
    release_confirmed_direct_visible_input_pending_v1,
    release_unattempted_direct_visible_input_gate_v1,
    require_matching_direct_visible_input_pending_v1, reserve_direct_visible_input_gate_v1,
    send_exactly_one_competitive_deck_control_click_v1, set_direct_visible_input_pending_v1,
};
use crate::probe::{
    classify_competitive_deck_chooser_open_state_v1,
    classify_competitive_deck_chooser_selected_successor_v1,
    confirm_competitive_deck_submit_visible_v1, MtgoCompetitiveDeckChooserStateV1,
    MtgoCompetitiveDeckControlTargetV1, MtgoCompetitiveNavigationFrameIdentityV1,
    OpaqueMtgoAdmittedCompetitiveNavigationFrameV1, OpaqueMtgoClassifiedCompetitiveDeckChooserV1,
    OpaqueMtgoClassifiedCompetitiveDeckGateV1, OpaqueMtgoConfirmedCompetitiveDeckSubmitVisibleV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
};
use mtgo_blackbox_v1::{
    competitive_event_listing_target_commitment_v1, competitive_mode_authorization_commitment_v1,
    CheckedUntrustedMtgoAuthorizationCorrespondenceV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveDeckGateStateV1, MtgoCompetitiveDeckSelectionTransitionCommitmentsV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveEventListingTargetV1,
    ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

const COMPETITIVE_DECK_SELECTION_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-selection-authorization-v1";
const COMPETITIVE_DECK_SELECTION_SCOPE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-selection-scope-v1";
const COMPETITIVE_DECK_SELECTION_SESSION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-selection-session-v1";
const COMPETITIVE_DECK_SELECTION_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-selection-preparation-v1";
const COMPETITIVE_DECK_SELECTION_INPUT_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-selection-input-receipt-v1";
const COMPETITIVE_DECK_SELECTION_CONFIRMATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-selection-confirmation-v1";

// This root is deliberately empty. Daybreak's transport-neutral permission is
// recorded separately from the account owner's still-required production
// correspondence and exact reviewed League or Challenge target ratification.
const RATIFIED_COMPETITIVE_DECK_SELECTION_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveDeckSelectionControlV1 {
    SelectDeck,
    ExactDeckRow,
    SubmitSelectedDeck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveDeckSelectionRatificationCandidateV1 {
    pub correspondence_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub classifier_runtime_identity_commitment_sha256: String,
    pub target_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub deck_display_label_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub control_scope_commitment_sha256: String,
    pub ratification_commitment_sha256: String,
}

impl MtgoReviewedCompetitiveDeckSelectionRatificationCandidateV1 {
    pub fn grants_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

struct RatifiedMtgoCompetitiveDeckSelectionAuthorizationV1 {
    _correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    _scope: MtgoAuthorizationScopeV1,
    target: MtgoCompetitiveEventListingTargetV1,
    commitments: MtgoReviewedCompetitiveDeckSelectionRatificationCandidateV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveDeckSelectionSessionCommitmentsV1 {
    pub authorization_ratification_commitment_sha256: String,
    pub target_commitment_sha256: String,
    pub session_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub next_control: MtgoCompetitiveDeckSelectionControlV1,
    pub confirmed_control_count: u8,
    pub last_frame_id: u64,
    pub last_frame_sequence: u64,
    pub last_captured_at_unix_millis: u128,
}

enum MtgoCompetitiveDeckSelectionStageV1 {
    SelectDeck(OpaqueMtgoClassifiedCompetitiveDeckGateV1),
    ExactDeckRow(OpaqueMtgoClassifiedCompetitiveDeckChooserV1),
    SubmitSelectedDeck(OpaqueMtgoClassifiedCompetitiveDeckChooserV1),
}

/// One move-only non-entry session for the exact visible Select Deck, exact
/// deck row, and Submit sequence. It never owns an entry choice, Entry Review
/// control, fee, event-entry operation, or spending operation.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveDeckSelectionSessionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveDeckSelectionSessionV1>();
/// ```
pub struct OpaqueMtgoCompetitiveDeckSelectionSessionV1 {
    authorization: RatifiedMtgoCompetitiveDeckSelectionAuthorizationV1,
    stage: MtgoCompetitiveDeckSelectionStageV1,
    commitments: MtgoCompetitiveDeckSelectionSessionCommitmentsV1,
}

impl OpaqueMtgoCompetitiveDeckSelectionSessionV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveDeckSelectionSessionCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn next_control_v1(&self) -> MtgoCompetitiveDeckSelectionControlV1 {
        self.commitments.next_control
    }

    pub fn safe_for_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveDeckSelectionControlCommitmentsV1 {
    pub authorization_ratification_commitment_sha256: String,
    pub session_commitment_sha256: String,
    pub target_commitment_sha256: String,
    pub control_binding_commitment_sha256: String,
    pub preparation_commitment_sha256: String,
    pub control: MtgoCompetitiveDeckSelectionControlV1,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub source_captured_at_unix_millis: u128,
}

pub struct OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1 {
    session: OpaqueMtgoCompetitiveDeckSelectionSessionV1,
    pointer_target: MtgoCompetitiveDeckControlTargetV1,
    commitments: MtgoPreparedCompetitiveDeckSelectionControlCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveDeckSelectionControlCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPendingCompetitiveDeckSelectionControlCommitmentsV1 {
    pub preparation_commitment_sha256: String,
    pub input_receipt_sha256: String,
    pub control: MtgoCompetitiveDeckSelectionControlV1,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub cursor_parked_outside_client: bool,
}

pub struct OpaqueMtgoPendingCompetitiveDeckSelectionControlV1 {
    prepared: OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1,
    commitments: MtgoPendingCompetitiveDeckSelectionControlCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitiveDeckSelectionControlV1 {
    pub fn commitments_v1(&self) -> MtgoPendingCompetitiveDeckSelectionControlCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveDeckSelectionCommitmentsV1 {
    pub authorization_ratification_commitment_sha256: String,
    pub input_receipt_sha256: String,
    pub confirmation_receipt_sha256: String,
    pub visible_transition: MtgoCompetitiveDeckSelectionTransitionCommitmentsV1,
    pub after_captured_at_unix_millis: u128,
}

pub struct OpaqueMtgoConfirmedCompetitiveDeckSelectionV1 {
    _authorization: RatifiedMtgoCompetitiveDeckSelectionAuthorizationV1,
    visible: OpaqueMtgoConfirmedCompetitiveDeckSubmitVisibleV1,
    commitments: MtgoConfirmedCompetitiveDeckSelectionCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveDeckSelectionV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitiveDeckSelectionCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn into_after_gate_v1(self) -> OpaqueMtgoClassifiedCompetitiveDeckGateV1 {
        self.visible.into_after_gate_v1()
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

pub fn review_competitive_deck_selection_ratification_candidate_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_gate: &OpaqueMtgoClassifiedCompetitiveDeckGateV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: &MtgoCompetitiveEventListingTargetV1,
    visible_account_alias: &str,
) -> Result<MtgoReviewedCompetitiveDeckSelectionRatificationCandidateV1, String> {
    if source_gate.state_v1() != MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection {
        return Err("deck-selection ratification requires the exact missing-deck gate".to_owned());
    }
    let gate = source_gate.commitments_v1();
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(target.event_kind)
        .map_err(|error| format!("derive deck-selection correspondence scope: {error}"))?;
    let mode_authorization_commitment_sha256 =
        competitive_mode_authorization_commitment_v1(&scope, target.event_kind)
            .map_err(|error| format!("validate deck-selection mode authorization: {error}"))?;
    let visible_alias_sha256 = sha256_hex_v1(visible_account_alias.as_bytes());
    let target_commitment_sha256 = competitive_event_listing_target_commitment_v1(target, deck)
        .map_err(|error| format!("validate deck-selection target: {error}"))?;
    if visible_alias_sha256 != scope.account_alias_sha256
        || target.approved_account_alias_sha256 != scope.account_alias_sha256
        || gate
            .source_navigation
            .source_frame
            .approved_account_alias_sha256
            != scope.account_alias_sha256
        || gate.source_navigation.event_kind != target.event_kind
        || gate.target_commitment_sha256 != target_commitment_sha256
        || target.deck_list_sha256 != deck.deck_list_sha256()
        || target.deck_manifest_commitment_sha256 != deck.manifest_commitment_sha256()
        || target.deck_format_sha256 != deck.format_sha256()
    {
        return Err(
            "deck-selection correspondence, account, mode, target, or exact deck differs"
                .to_owned(),
        );
    }
    let control_scope_commitment_sha256 = competitive_deck_selection_scope_commitment_v1();
    let ratification_commitment_sha256 = commitment_v1(
        COMPETITIVE_DECK_SELECTION_AUTHORIZATION_DOMAIN_V1,
        &[
            correspondence.correspondence_sha256().as_bytes(),
            correspondence.review_commitment_sha256().as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            scope.account_alias_sha256.as_bytes(),
            gate.source_navigation
                .source_frame
                .profile_commitment_sha256
                .as_bytes(),
            gate.source_navigation
                .source_frame
                .profile_admission_commitment_sha256
                .as_bytes(),
            gate.runtime_identity_commitment_sha256.as_bytes(),
            target_commitment_sha256.as_bytes(),
            target.event_identity_sha256.as_bytes(),
            target.deck_display_label_sha256.as_bytes(),
            deck.deck_list_sha256().as_bytes(),
            deck.manifest_commitment_sha256().as_bytes(),
            deck.format_sha256().as_bytes(),
            target.policy_deployment_commitment_sha256.as_bytes(),
            control_scope_commitment_sha256.as_bytes(),
            event_kind_tag_v1(target.event_kind),
            b"exact_three_control_deck_selection_no_entry_choice_no_fee_no_event_entry_no_spending",
        ],
    );
    Ok(
        MtgoReviewedCompetitiveDeckSelectionRatificationCandidateV1 {
            correspondence_sha256: correspondence.correspondence_sha256().to_owned(),
            permission_review_commitment_sha256: correspondence
                .review_commitment_sha256()
                .to_owned(),
            mode_authorization_commitment_sha256,
            approved_account_alias_sha256: scope.account_alias_sha256,
            navigation_profile_commitment_sha256: gate
                .source_navigation
                .source_frame
                .profile_commitment_sha256,
            navigation_profile_admission_commitment_sha256: gate
                .source_navigation
                .source_frame
                .profile_admission_commitment_sha256,
            classifier_runtime_identity_commitment_sha256: gate.runtime_identity_commitment_sha256,
            target_commitment_sha256,
            event_kind: target.event_kind,
            event_identity_sha256: target.event_identity_sha256.clone(),
            deck_display_label_sha256: target.deck_display_label_sha256.clone(),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: target.policy_deployment_commitment_sha256.clone(),
            control_scope_commitment_sha256,
            ratification_commitment_sha256,
        },
    )
}

pub fn begin_ratified_competitive_deck_selection_session_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_gate: OpaqueMtgoClassifiedCompetitiveDeckGateV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    visible_account_alias: String,
) -> Result<OpaqueMtgoCompetitiveDeckSelectionSessionV1, String> {
    let candidate = review_competitive_deck_selection_ratification_candidate_v1(
        &correspondence,
        &source_gate,
        deck,
        &target,
        &visible_account_alias,
    )?;
    let expected = RATIFIED_COMPETITIVE_DECK_SELECTION_AUTHORIZATION_COMMITMENT_V1
        .ok_or("the exact competitive deck-selection permission is not ratified in this build")?;
    if candidate.ratification_commitment_sha256 != expected {
        return Err(
            "competitive deck-selection candidate differs from the production ratification root"
                .to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(target.event_kind)
        .map_err(|error| format!("derive ratified deck-selection scope: {error}"))?;
    let gate = source_gate.commitments_v1();
    let session_commitment_sha256 = commitment_v1(
        COMPETITIVE_DECK_SELECTION_SESSION_DOMAIN_V1,
        &[
            candidate.ratification_commitment_sha256.as_bytes(),
            candidate.target_commitment_sha256.as_bytes(),
            gate.observation_commitment_sha256.as_bytes(),
            gate.source_navigation.frame_id.to_be_bytes().as_slice(),
            gate.source_navigation
                .frame_sequence
                .to_be_bytes()
                .as_slice(),
            b"awaiting_exact_select_deck_control",
        ],
    );
    Ok(OpaqueMtgoCompetitiveDeckSelectionSessionV1 {
        authorization: RatifiedMtgoCompetitiveDeckSelectionAuthorizationV1 {
            _correspondence: correspondence,
            _scope: scope,
            target,
            commitments: candidate.clone(),
        },
        stage: MtgoCompetitiveDeckSelectionStageV1::SelectDeck(source_gate),
        commitments: MtgoCompetitiveDeckSelectionSessionCommitmentsV1 {
            authorization_ratification_commitment_sha256: candidate.ratification_commitment_sha256,
            target_commitment_sha256: candidate.target_commitment_sha256,
            session_commitment_sha256,
            event_kind: candidate.event_kind,
            event_identity_sha256: candidate.event_identity_sha256,
            next_control: MtgoCompetitiveDeckSelectionControlV1::SelectDeck,
            confirmed_control_count: 0,
            last_frame_id: gate.source_navigation.frame_id,
            last_frame_sequence: gate.source_navigation.frame_sequence,
            last_captured_at_unix_millis: gate
                .source_navigation
                .source_frame
                .source_capture
                .captured_at_unix_millis,
        },
    })
}

pub fn prepare_next_competitive_deck_selection_control_v1(
    session: OpaqueMtgoCompetitiveDeckSelectionSessionV1,
) -> Result<OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1, String> {
    let control = session.commitments.next_control;
    let pointer_target = match &session.stage {
        MtgoCompetitiveDeckSelectionStageV1::SelectDeck(gate) => {
            if control != MtgoCompetitiveDeckSelectionControlV1::SelectDeck {
                return Err("deck-selection session stage and next control differ".to_owned());
            }
            gate.select_deck_control_target_v1()?
        }
        MtgoCompetitiveDeckSelectionStageV1::ExactDeckRow(chooser) => {
            if control != MtgoCompetitiveDeckSelectionControlV1::ExactDeckRow {
                return Err("deck-selection session stage and next control differ".to_owned());
            }
            chooser.exact_deck_row_control_target_v1()?
        }
        MtgoCompetitiveDeckSelectionStageV1::SubmitSelectedDeck(chooser) => {
            if control != MtgoCompetitiveDeckSelectionControlV1::SubmitSelectedDeck {
                return Err("deck-selection session stage and next control differ".to_owned());
            }
            chooser.submit_control_target_v1()?
        }
    };
    let preparation_commitment_sha256 = commitment_v1(
        COMPETITIVE_DECK_SELECTION_PREPARATION_DOMAIN_V1,
        &[
            session
                .commitments
                .authorization_ratification_commitment_sha256
                .as_bytes(),
            session.commitments.session_commitment_sha256.as_bytes(),
            session.commitments.target_commitment_sha256.as_bytes(),
            pointer_target.control_binding_commitment_sha256.as_bytes(),
            pointer_target.source_capture_commitment_sha256.as_bytes(),
            control_tag_v1(control),
            pointer_target.source_frame_id.to_be_bytes().as_slice(),
            pointer_target
                .source_frame_sequence
                .to_be_bytes()
                .as_slice(),
            pointer_target
                .source_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"fresh_private_pointer_one_click_then_exact_visible_confirmation",
        ],
    );
    let commitments = MtgoPreparedCompetitiveDeckSelectionControlCommitmentsV1 {
        authorization_ratification_commitment_sha256: session
            .commitments
            .authorization_ratification_commitment_sha256
            .clone(),
        session_commitment_sha256: session.commitments.session_commitment_sha256.clone(),
        target_commitment_sha256: session.commitments.target_commitment_sha256.clone(),
        control_binding_commitment_sha256: pointer_target.control_binding_commitment_sha256.clone(),
        preparation_commitment_sha256,
        control,
        source_frame_id: pointer_target.source_frame_id,
        source_frame_sequence: pointer_target.source_frame_sequence,
        source_captured_at_unix_millis: pointer_target.source_captured_at_unix_millis,
    };
    Ok(OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1 {
        session,
        pointer_target,
        commitments,
    })
}

pub fn execute_prepared_competitive_deck_selection_control_v1(
    prepared: OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1,
) -> Result<OpaqueMtgoPendingCompetitiveDeckSelectionControlV1, String> {
    reserve_direct_visible_input_gate_v1()?;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_direct_visible_input_gate_v1()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if input_sent_at_unix_millis < prepared.commitments.source_captured_at_unix_millis
        || input_sent_at_unix_millis - prepared.commitments.source_captured_at_unix_millis > 2_000
    {
        release_unattempted_direct_visible_input_gate_v1()?;
        return Err(
            "deck-selection immediate pre-input capture is stale or future-dated".to_owned(),
        );
    }
    halt_before_direct_visible_input_attempt_v1()?;
    let cursor_parked_outside_client = send_exactly_one_competitive_deck_control_click_v1(
        &prepared.pointer_target.pointer_target,
    )?;
    let input_receipt_sha256 = commitment_v1(
        COMPETITIVE_DECK_SELECTION_INPUT_RECEIPT_DOMAIN_V1,
        &[
            prepared
                .commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared
                .commitments
                .authorization_ratification_commitment_sha256
                .as_bytes(),
            prepared.commitments.session_commitment_sha256.as_bytes(),
            prepared.commitments.target_commitment_sha256.as_bytes(),
            prepared
                .commitments
                .control_binding_commitment_sha256
                .as_bytes(),
            control_tag_v1(prepared.commitments.control),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            &[u8::from(cursor_parked_outside_client)],
            b"exactly_one_left_click_shared_gate_pending_exact_visible_successor",
        ],
    );
    set_direct_visible_input_pending_v1(&input_receipt_sha256)?;
    let commitments = MtgoPendingCompetitiveDeckSelectionControlCommitmentsV1 {
        preparation_commitment_sha256: prepared.commitments.preparation_commitment_sha256.clone(),
        input_receipt_sha256,
        control: prepared.commitments.control,
        source_frame_id: prepared.commitments.source_frame_id,
        source_frame_sequence: prepared.commitments.source_frame_sequence,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    };
    Ok(OpaqueMtgoPendingCompetitiveDeckSelectionControlV1 {
        prepared,
        commitments,
    })
}

pub fn confirm_pending_competitive_select_deck_v1(
    pending: OpaqueMtgoPendingCompetitiveDeckSelectionControlV1,
    after_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    identity: MtgoCompetitiveNavigationFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveDeckSelectionSessionV1, String> {
    require_matching_direct_visible_input_pending_v1(&pending.commitments.input_receipt_sha256)?;
    if pending.commitments.control != MtgoCompetitiveDeckSelectionControlV1::SelectDeck {
        return halt_confirmation_v1(
            "Select Deck confirmation received a different pending control",
        );
    }
    let OpaqueMtgoPendingCompetitiveDeckSelectionControlV1 {
        prepared,
        commitments: input,
    } = pending;
    let OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1 {
        session,
        pointer_target: _,
        commitments: prepared_commitments,
    } = prepared;
    let OpaqueMtgoCompetitiveDeckSelectionSessionV1 {
        authorization,
        stage,
        commitments: session_commitments,
    } = session;
    let source_gate = match stage {
        MtgoCompetitiveDeckSelectionStageV1::SelectDeck(gate) => gate,
        _ => return halt_confirmation_v1("Select Deck confirmation changed session stage"),
    };
    let chooser = match classify_competitive_deck_chooser_open_state_v1(
        source_gate,
        after_frame,
        deck,
        authorization.target.clone(),
        runtime,
        identity,
        timeout_ms,
    ) {
        Ok(value) => value,
        Err(error) => {
            return halt_confirmation_v1(&format!(
                "Select Deck visible confirmation failed: {error}"
            ))
        }
    };
    let visible = chooser.commitments_v1();
    if visible.state != MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection
        || visible.target_commitment_sha256 != authorization.commitments.target_commitment_sha256
        || visible.captured_at_unix_millis <= input.input_sent_at_unix_millis
        || visible.frame_sequence <= prepared_commitments.source_frame_sequence
    {
        return halt_confirmation_v1("Select Deck successor changed target, state, or freshness");
    }
    let next = advance_session_v1(
        authorization,
        session_commitments,
        MtgoCompetitiveDeckSelectionStageV1::ExactDeckRow(chooser),
        MtgoCompetitiveDeckSelectionControlV1::ExactDeckRow,
        &input,
        &visible.classification_result_commitment_sha256,
        visible.frame_id,
        visible.frame_sequence,
        visible.captured_at_unix_millis,
    )?;
    release_confirmed_direct_visible_input_pending_v1(&input.input_receipt_sha256)?;
    Ok(next)
}

pub fn confirm_pending_competitive_exact_deck_row_v1(
    pending: OpaqueMtgoPendingCompetitiveDeckSelectionControlV1,
    after_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    identity: MtgoCompetitiveNavigationFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveDeckSelectionSessionV1, String> {
    require_matching_direct_visible_input_pending_v1(&pending.commitments.input_receipt_sha256)?;
    if pending.commitments.control != MtgoCompetitiveDeckSelectionControlV1::ExactDeckRow {
        return halt_confirmation_v1("deck-row confirmation received a different pending control");
    }
    let OpaqueMtgoPendingCompetitiveDeckSelectionControlV1 {
        prepared,
        commitments: input,
    } = pending;
    let OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1 {
        session,
        pointer_target: _,
        commitments: prepared_commitments,
    } = prepared;
    let OpaqueMtgoCompetitiveDeckSelectionSessionV1 {
        authorization,
        stage,
        commitments: session_commitments,
    } = session;
    let chooser = match stage {
        MtgoCompetitiveDeckSelectionStageV1::ExactDeckRow(chooser) => chooser,
        _ => return halt_confirmation_v1("deck-row confirmation changed session stage"),
    };
    let selected = match classify_competitive_deck_chooser_selected_successor_v1(
        chooser,
        after_frame,
        deck,
        runtime,
        identity,
        timeout_ms,
    ) {
        Ok(value) => value,
        Err(error) => {
            return halt_confirmation_v1(&format!("deck-row visible confirmation failed: {error}"))
        }
    };
    let visible = selected.commitments_v1();
    if visible.state != MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected
        || visible.target_commitment_sha256 != authorization.commitments.target_commitment_sha256
        || visible.captured_at_unix_millis <= input.input_sent_at_unix_millis
        || visible.frame_sequence <= prepared_commitments.source_frame_sequence
    {
        return halt_confirmation_v1("deck-row successor changed target, state, or freshness");
    }
    let next = advance_session_v1(
        authorization,
        session_commitments,
        MtgoCompetitiveDeckSelectionStageV1::SubmitSelectedDeck(selected),
        MtgoCompetitiveDeckSelectionControlV1::SubmitSelectedDeck,
        &input,
        &visible.classification_result_commitment_sha256,
        visible.frame_id,
        visible.frame_sequence,
        visible.captured_at_unix_millis,
    )?;
    release_confirmed_direct_visible_input_pending_v1(&input.input_receipt_sha256)?;
    Ok(next)
}

pub fn confirm_pending_competitive_deck_submit_v1(
    pending: OpaqueMtgoPendingCompetitiveDeckSelectionControlV1,
    after_gate: OpaqueMtgoClassifiedCompetitiveDeckGateV1,
) -> Result<OpaqueMtgoConfirmedCompetitiveDeckSelectionV1, String> {
    require_matching_direct_visible_input_pending_v1(&pending.commitments.input_receipt_sha256)?;
    if pending.commitments.control != MtgoCompetitiveDeckSelectionControlV1::SubmitSelectedDeck {
        return halt_confirmation_v1(
            "deck Submit confirmation received a different pending control",
        );
    }
    let OpaqueMtgoPendingCompetitiveDeckSelectionControlV1 {
        prepared,
        commitments: input,
    } = pending;
    let OpaqueMtgoPreparedCompetitiveDeckSelectionControlV1 {
        session,
        pointer_target: _,
        commitments: prepared_commitments,
    } = prepared;
    let OpaqueMtgoCompetitiveDeckSelectionSessionV1 {
        authorization,
        stage,
        commitments: session_commitments,
    } = session;
    let selected = match stage {
        MtgoCompetitiveDeckSelectionStageV1::SubmitSelectedDeck(chooser) => chooser,
        _ => return halt_confirmation_v1("deck Submit confirmation changed session stage"),
    };
    let visible = match confirm_competitive_deck_submit_visible_v1(selected, after_gate) {
        Ok(value) => value,
        Err(error) => {
            return halt_confirmation_v1(&format!(
                "deck Submit visible confirmation failed: {error}"
            ))
        }
    };
    let transition = visible.transition_commitments_v1();
    let after_captured_at_unix_millis = visible.after_captured_at_unix_millis_v1();
    if transition.target_commitment_sha256 != authorization.commitments.target_commitment_sha256
        || transition.event_kind != authorization.commitments.event_kind
        || transition.event_identity_sha256 != authorization.commitments.event_identity_sha256
        || transition.before_frame_sequence > prepared_commitments.source_frame_sequence
        || transition.after_frame_sequence <= prepared_commitments.source_frame_sequence
        || after_captured_at_unix_millis <= input.input_sent_at_unix_millis
        || session_commitments.confirmed_control_count != 2
    {
        return halt_confirmation_v1("deck Submit successor changed exact lineage or freshness");
    }
    let confirmation_receipt_sha256 = commitment_v1(
        COMPETITIVE_DECK_SELECTION_CONFIRMATION_DOMAIN_V1,
        &[
            input.input_receipt_sha256.as_bytes(),
            prepared_commitments
                .preparation_commitment_sha256
                .as_bytes(),
            authorization
                .commitments
                .ratification_commitment_sha256
                .as_bytes(),
            transition.transition_commitment_sha256.as_bytes(),
            transition.after_frame_id.to_be_bytes().as_slice(),
            transition.after_frame_sequence.to_be_bytes().as_slice(),
            after_captured_at_unix_millis.to_be_bytes().as_slice(),
            b"three_exact_visible_controls_confirmed_no_entry_choice_no_fee_no_entry_no_spending",
        ],
    );
    release_confirmed_direct_visible_input_pending_v1(&input.input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveDeckSelectionV1 {
        _authorization: authorization,
        visible,
        commitments: MtgoConfirmedCompetitiveDeckSelectionCommitmentsV1 {
            authorization_ratification_commitment_sha256: prepared_commitments
                .authorization_ratification_commitment_sha256,
            input_receipt_sha256: input.input_receipt_sha256,
            confirmation_receipt_sha256,
            visible_transition: transition,
            after_captured_at_unix_millis,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn advance_session_v1(
    authorization: RatifiedMtgoCompetitiveDeckSelectionAuthorizationV1,
    mut commitments: MtgoCompetitiveDeckSelectionSessionCommitmentsV1,
    stage: MtgoCompetitiveDeckSelectionStageV1,
    next_control: MtgoCompetitiveDeckSelectionControlV1,
    input: &MtgoPendingCompetitiveDeckSelectionControlCommitmentsV1,
    visible_result_commitment_sha256: &str,
    frame_id: u64,
    frame_sequence: u64,
    captured_at_unix_millis: u128,
) -> Result<OpaqueMtgoCompetitiveDeckSelectionSessionV1, String> {
    let next_count = commitments
        .confirmed_control_count
        .checked_add(1)
        .ok_or("deck-selection confirmed-control count overflow")?;
    commitments.session_commitment_sha256 = commitment_v1(
        COMPETITIVE_DECK_SELECTION_SESSION_DOMAIN_V1,
        &[
            commitments.session_commitment_sha256.as_bytes(),
            input.input_receipt_sha256.as_bytes(),
            visible_result_commitment_sha256.as_bytes(),
            control_tag_v1(input.control),
            control_tag_v1(next_control),
            frame_id.to_be_bytes().as_slice(),
            frame_sequence.to_be_bytes().as_slice(),
            captured_at_unix_millis.to_be_bytes().as_slice(),
            &[next_count],
            b"exact_visible_successor_releases_gate_and_advances_one_control",
        ],
    );
    commitments.next_control = next_control;
    commitments.confirmed_control_count = next_count;
    commitments.last_frame_id = frame_id;
    commitments.last_frame_sequence = frame_sequence;
    commitments.last_captured_at_unix_millis = captured_at_unix_millis;
    Ok(OpaqueMtgoCompetitiveDeckSelectionSessionV1 {
        authorization,
        stage,
        commitments,
    })
}

fn halt_confirmation_v1<T>(message: &str) -> Result<T, String> {
    halt_direct_visible_input_gate_v1()?;
    Err(format!(
        "{message}; the process-wide MTGO input gate is halted"
    ))
}

fn competitive_deck_selection_scope_commitment_v1() -> String {
    commitment_v1(
        COMPETITIVE_DECK_SELECTION_SCOPE_DOMAIN_V1,
        &[
            b"select_deck",
            b"exact_deck_row",
            b"submit_selected_deck",
            b"no_entry_choice",
            b"no_open_entry_review",
            b"no_event_entry",
            b"no_spending",
        ],
    )
}

fn event_kind_tag_v1(event_kind: MtgoCompetitiveEventKindV1) -> &'static [u8] {
    match event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    }
}

fn control_tag_v1(control: MtgoCompetitiveDeckSelectionControlV1) -> &'static [u8] {
    match control {
        MtgoCompetitiveDeckSelectionControlV1::SelectDeck => b"select_deck",
        MtgoCompetitiveDeckSelectionControlV1::ExactDeckRow => b"exact_deck_row",
        MtgoCompetitiveDeckSelectionControlV1::SubmitSelectedDeck => b"submit_selected_deck",
    }
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_deck_selection_authority_is_empty_and_scope_is_non_entry() {
        assert_eq!(
            RATIFIED_COMPETITIVE_DECK_SELECTION_AUTHORIZATION_COMMITMENT_V1,
            None
        );
        assert_eq!(competitive_deck_selection_scope_commitment_v1().len(), 64);
        let candidate = MtgoReviewedCompetitiveDeckSelectionRatificationCandidateV1 {
            correspondence_sha256: "a".repeat(64),
            permission_review_commitment_sha256: "b".repeat(64),
            mode_authorization_commitment_sha256: "c".repeat(64),
            approved_account_alias_sha256: "d".repeat(64),
            navigation_profile_commitment_sha256: "e".repeat(64),
            navigation_profile_admission_commitment_sha256: "f".repeat(64),
            classifier_runtime_identity_commitment_sha256: "0".repeat(64),
            target_commitment_sha256: "1".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: "2".repeat(64),
            deck_display_label_sha256: "3".repeat(64),
            deck_list_sha256: "4".repeat(64),
            deck_manifest_commitment_sha256: "5".repeat(64),
            deck_format_sha256: "6".repeat(64),
            policy_deployment_commitment_sha256: "7".repeat(64),
            control_scope_commitment_sha256: "8".repeat(64),
            ratification_commitment_sha256: "9".repeat(64),
        };
        assert!(!candidate.grants_live_input_v1());
        assert!(!candidate.permits_event_entry_v1());
        assert!(!candidate.permits_spending_v1());
    }
}
