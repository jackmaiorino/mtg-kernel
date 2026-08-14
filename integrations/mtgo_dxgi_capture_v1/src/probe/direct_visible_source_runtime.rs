use super::{
    capture_admitted_mtgo_duel_visible_frame_v1, capture_mtgo_dxgi_frame_candidate_v3,
    competitive_entry_window_continuity_commitment_for_frame_v1,
    frame_id_from_capture_commitment_v1, perceive_admitted_duel_frame_v1, sha256_hex_v1,
    CaptureWindowModeV2, MtgoDuelPerceptionFrameIdentityV1, MtgoDxgiCaptureRequestV3,
    OpaqueMtgoAdmittedDuelPerceptionV1, OpaqueMtgoAdmittedDuelVisibleFrameV1,
    OpaqueMtgoDxgiFrameCandidateV3, OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
};
use crate::actuator::{
    halt_before_direct_visible_input_attempt_v1, release_unattempted_direct_visible_input_gate_v1,
    require_matching_direct_visible_input_pending_v1, reserve_direct_visible_input_gate_v1,
    set_direct_visible_input_pending_v1,
};
use mtgo_blackbox_v1::{
    bind_refreshed_direct_visible_selection_to_competitive_match_v1,
    complete_direct_visible_gameplay_postcondition_v1,
    confirm_player_visible_combat_execution_transition_v1,
    join_player_visible_combat_rescore_trace_v1,
    parse_and_validate_visible_duel_producer_result_v1, player_visible_duel_action_family_v1,
    prepare_direct_visible_gameplay_before_dispatch_v1,
    prepare_player_visible_combat_execution_step_v1,
    refresh_direct_visible_selection_before_dispatch_v1,
    score_and_prepare_strict_visible_combat_producer_result_v1,
    score_and_select_strict_visible_duel_producer_result_v1, AdmittedMtgoDuelPerceptionProfileV1,
    CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1,
    CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1,
    CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1,
    CheckedUntrustedMtgoPlayerVisibleCombatDecisionTraceV1,
    CheckedUntrustedMtgoPlayerVisibleCombatExecutionStepV1,
    CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1,
    CheckedUntrustedMtgoPlayerVisibleCombatTransitionV1,
    CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
    CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveMatchGameplayAuthorizationV1, MtgoDirectVisibleCompetitiveObservationBracketV1,
    MtgoDirectVisibleGameplayBeforeDispatchRecordV1, MtgoDirectVisibleGameplayBeforeRegionV1,
    MtgoDuelActionFamilyV1, MtgoEvidenceSourceV1, MtgoPlayerVisibleCombatBrokerCommandV1,
    MtgoPlayerVisibleCombatScorerV1, MtgoPlayerVisibleCombatTransitionProgressV1,
    MtgoPlayerVisibleDuelScorerV1, MtgoPlayerVisibleGameplayPostconditionKindV1,
    MtgoPlayerVisiblePreparedCombatKindV1, MtgoRectPxV1, MtgoSizePxV1,
    MtgoVisibleDuelViewModelBrokerAbstentionReasonV1, MtgoVisibleDuelViewModelBrokerResultV1,
    MTGO_DIRECT_VISIBLE_COMPETITIVE_OBSERVATION_BRACKET_SCHEMA_V1,
    MTGO_DIRECT_VISIBLE_GAMEPLAY_BEFORE_DISPATCH_SCHEMA_V1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, FILETIME, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

const LIVE_BROKER_SHA256_V1: &str =
    "e83e1f08260cdbd80e68527964de54afd2774beed8f344b03f609fdd75a34fab";
const LIVE_DISPATCH_BROKER_SHA256_V1: &str =
    "95dcfe3eb38006e8dd26800776ef2e329aef1a9dbf265ba5a0bfe179247b9e49";
const LIVE_BOOTSTRAP_SHA256_V1: &str =
    "1d764382d56fe27aa845acf10b92ee8b9effd79d161baeaace1294a2d01c8c9b";
const LIVE_PRODUCER_SHA256_V1: &str =
    "3b19cd76514350f33a1d96ce7a69b7168f394a54cd7b5c70422d0f61b39f1596";
const LIVE_VALIDATOR_SHA256_V1: &str =
    "4e0eea73bf592a0a80aa5e42a05f191640b4f1f46d917f1b1bca80a81815334c";
const DIRECT_VISIBLE_SOURCE_RUNTIME_DOMAIN_V1: &[u8] = b"mtgo-direct-visible-source-runtime-v1";
const DIRECT_VISIBLE_SOURCE_OBSERVATION_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-source-observation-v1";
const DIRECT_VISIBLE_SOURCE_QUALIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-source-no-stakes-qualification-v1";
const DIRECT_VISIBLE_SPECTATOR_SOURCE_QUALIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-spectator-source-no-stakes-qualification-v1";
const DIRECT_VISIBLE_BACKGROUND_STABILITY_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-background-stability-v1";
const DIRECT_VISIBLE_BACKGROUND_REVIEW_ARTIFACT_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-background-review-artifact-v1";
const DIRECT_VISIBLE_BACKGROUND_REVIEW_PARTIAL_PREFIX_V1: &str =
    ".mtgo-direct-visible-background-review-partial-";
const DIRECT_VISIBLE_SOURCE_SCORED_REFRESH_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-source-scored-refresh-v1";
const DIRECT_VISIBLE_SOURCE_COMPETITIVE_BEFORE_DISPATCH_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-source-competitive-before-dispatch-v1";
const DIRECT_VISIBLE_SOURCE_EQUIVALENT_REGIONS_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-source-equivalent-regions-v1";
const DIRECT_VISIBLE_DISPATCH_RUNTIME_DOMAIN_V1: &[u8] = b"mtgo-direct-visible-dispatch-runtime-v1";
const DIRECT_VISIBLE_DISPATCH_RECEIPT_DOMAIN_V1: &[u8] = b"mtgo-direct-visible-dispatch-receipt-v1";
const DIRECT_VISIBLE_COMBAT_DISPATCH_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-combat-dispatch-receipt-v1";
const RATIFIED_DIRECT_VISIBLE_SOURCE_QUALIFICATION_COMMITMENT_V1: Option<&str> = None;
const RATIFIED_DIRECT_VISIBLE_COMBAT_SOURCE_QUALIFICATION_COMMITMENT_V1: Option<&str> = None;
const RATIFIED_BACKGROUND_DIRECT_VISIBLE_SOURCE_QUALIFICATION_COMMITMENT_V1: Option<&str> = None;
const RATIFIED_BACKGROUND_DIRECT_VISIBLE_COMBAT_SOURCE_QUALIFICATION_COMMITMENT_V1: Option<&str> =
    None;
const RATIFIED_DIRECT_VISIBLE_DISPATCH_RUNTIME_COMMITMENT_V1: Option<&str> = None;
const RATIFIED_DIRECT_VISIBLE_COMBAT_DISPATCH_RUNTIME_COMMITMENT_V1: Option<&str> = None;
const PINNED_MTGO_EXECUTABLE_SHA256_V1: &str =
    "bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92";
const PINNED_MTGO_SIGNER_THUMBPRINT_V1: &str = "e9d9e2b989f90555b04c506fddf889c7aba7ac30";
const PINNED_MTGO_SIGNER_SUBJECT_SHA256_V1: &str =
    "89e095d976048cdd8da11e2ff312231867f79e521fa3b5aa6415d2aa59b79cfc";
const MAX_ARTIFACT_BYTES_V1: u64 = 64 * 1024 * 1024;
const MAX_BROKER_STDOUT_BYTES_V1: usize = 1_048_568;
const MAX_BROKER_STDERR_BYTES_V1: usize = 64 * 1024;
const MAX_SOURCE_AGE_MILLIS_V1: u128 = 2_000;
const CREATE_NO_WINDOW_V1: u32 = 0x0800_0000;

/// Copyable identities for the exact release-pinned observe-only source.
/// These are adapter artifact commitments, not MTGO game information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoVerifiedDirectVisibleSourceRuntimeCommitmentsV1 {
    pub runtime_identity_commitment_sha256: String,
    pub broker_binary_sha256: String,
    pub bootstrap_binary_sha256: String,
    pub producer_binary_sha256: String,
    pub strict_validator_binary_sha256: String,
}

/// Exact local paths to the four release-pinned artifacts. The type is
/// move-only and has no dispatch method. The live broker itself is also built
/// with client action dispatch compile-disabled.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1;
/// fn cannot_dispatch(value: OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1) {
///     value.dispatch();
/// }
/// ```
pub struct OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1 {
    broker_path: PathBuf,
    bootstrap_path: PathBuf,
    producer_path: PathBuf,
    validator_path: PathBuf,
    commitments: MtgoVerifiedDirectVisibleSourceRuntimeCommitmentsV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoVerifiedDirectVisibleDispatchRuntimeCommitmentsV1 {
    pub dispatch_runtime_identity_commitment_sha256: String,
    pub dispatch_broker_binary_sha256: String,
    pub bootstrap_binary_sha256: String,
    pub producer_binary_sha256: String,
    pub strict_validator_binary_sha256: String,
}

/// Exact dispatch-capable native artifacts. This value is not input authority:
/// production dispatch additionally requires the separately compiled
/// ratification root, an attended match lease, a fresh source-attested visible
/// decision, the process-wide input gate, and a newer visible postcondition.
pub struct OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1 {
    broker_path: PathBuf,
    bootstrap_path: PathBuf,
    producer_path: PathBuf,
    validator_path: PathBuf,
    commitments: MtgoVerifiedDirectVisibleDispatchRuntimeCommitmentsV1,
}

impl OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1 {
    pub fn commitments_v1(&self) -> MtgoVerifiedDirectVisibleDispatchRuntimeCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1 {
    pub fn commitments_v1(&self) -> MtgoVerifiedDirectVisibleSourceRuntimeCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Commitments from one exact observe-only producer invocation bracketed by
/// two opaque composed frames. No process identifier, client path, raw object,
/// internal identifier, pixel buffer, or input capability is exposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAttestedDirectVisibleSourceObservationCommitmentsV1 {
    pub runtime_identity_commitment_sha256: String,
    pub broker_binary_sha256: String,
    pub producer_binary_sha256: String,
    pub before_frame_profile_binding_sha256: String,
    pub before_capture_commitment_sha256: String,
    pub after_frame_profile_binding_sha256: String,
    pub after_capture_commitment_sha256: String,
    pub sanitized_result_sha256: String,
    pub observation_commitment_sha256: String,
}

/// Commitments from one live no-stakes qualification observation. This is a
/// permanent qualification-only surface and is not a substitute for an
/// admitted duel-perception profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoQualifiedDirectVisibleSourceObservationCommitmentsV1 {
    pub runtime_identity_commitment_sha256: String,
    pub broker_binary_sha256: String,
    pub producer_binary_sha256: String,
    pub before_capture_commitment_sha256: String,
    pub after_capture_commitment_sha256: String,
    pub sanitized_result_sha256: String,
    pub qualification_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoRefreshedAttestedDirectVisibleSelectionCommitmentsV1 {
    pub runtime_identity_commitment_sha256: String,
    pub initial_observation_commitment_sha256: String,
    pub refreshed_observation_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub refresh_commitment_sha256: String,
    pub scored_refresh_commitment_sha256: String,
}

/// One player-visible change category to freeze for the action-specific
/// postcondition. The caller supplies no rectangle, evidence ID, or frame
/// identity. The capture owner derives the complete corresponding region from
/// the fresh classifier provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1 {
    pub kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
}

/// Complete action-specific region declaration. A partial declaration cannot
/// be promoted into the opaque competitive pre-dispatch owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAttestedDirectVisibleBeforeDispatchRegionSetV1 {
    pub candidate_set_complete: bool,
    pub regions: Vec<MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1>,
}

struct MtgoAttestedDirectVisibleCompetitiveBeforeDispatchCommitmentsV1 {
    _scored_refresh_commitment_sha256: String,
    _corroborating_perception_result_commitment_sha256: String,
    _corroborating_lifecycle_snapshot_commitment_sha256: String,
    _equivalent_visible_regions_commitment_sha256: String,
    _direct_competitive_scope_commitment_sha256: String,
    _before_dispatch_commitment_sha256: String,
    _binding_commitment_sha256: String,
}

/// One exact release-pinned producer execution. Both composed frames and the
/// sanitized result are retained in process. A visible decision remains a
/// checked-untrusted qualification candidate until the no-stakes duel corpus
/// and complete action surface are reviewed.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAttestedDirectVisibleSourceObservationV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoAttestedDirectVisibleSourceObservationV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAttestedDirectVisibleSourceObservationV1;
/// fn cannot_extract_or_act(value: OpaqueMtgoAttestedDirectVisibleSourceObservationV1) {
///     let _ = value.process_id();
///     let _ = value.raw_client_object();
///     value.dispatch();
/// }
/// ```
pub struct OpaqueMtgoAttestedDirectVisibleSourceObservationV1 {
    _before_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    _after_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    exact_result_bytes: ZeroingVecV1,
    result: MtgoVisibleDuelViewModelBrokerResultV1,
    commitments: MtgoAttestedDirectVisibleSourceObservationCommitmentsV1,
}

/// Qualification-only player-visible scoring outcome retaining the exact
/// admitted observation that produced it. This type stays crate-private so a
/// caller-supplied scorer can never become a production live-action source.
pub struct OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1 {
    _observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    outcome: CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1,
}

/// Qualification-only combat scoring outcome retaining the exact attested
/// observation whose sanitized player-visible bytes were scored. The model
/// receives only the transport-neutral combat schema. This wrapper grants no
/// input, event-entry, or spending authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1;
/// fn cannot_extract_or_dispatch(value: OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1) {
///     let _ = value.raw_client_object();
///     value.dispatch();
/// }
/// ```
pub struct OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1 {
    _observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    outcome: CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1,
}

impl OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1 {
    pub fn abstention_reason_v1(&self) -> Option<MtgoVisibleDuelViewModelBrokerAbstentionReasonV1> {
        match &self.outcome {
            CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { reason } => {
                Some(*reason)
            }
            CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(_) => None,
        }
    }

    pub fn prepared_kind_v1(&self) -> Option<MtgoPlayerVisiblePreparedCombatKindV1> {
        match &self.outcome {
            CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { .. } => None,
            CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared) => {
                Some(prepared.kind_v1())
            }
        }
    }

    pub fn bridge_commitment_sha256_v1(&self) -> Option<&str> {
        match &self.outcome {
            CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { .. } => None,
            CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared) => {
                Some(prepared.bridge_commitment_sha256_v1())
            }
        }
    }

    pub fn source_observation_commitment_sha256_v1(&self) -> &str {
        &self._observation.commitments.observation_commitment_sha256
    }

    pub fn model_selection_count_v1(&self) -> Option<usize> {
        match &self.outcome {
            CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { .. } => None,
            CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared) => {
                Some(prepared.model_selection_count_v1())
            }
        }
    }

    pub fn model_scoring_completed_v1(&self) -> bool {
        true
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// One exact model-prepared combat operation retained with the attested
/// player-visible source that produced it. The broker arguments are private
/// to this crate and the production combat-dispatch root is independently
/// empty. This value therefore cannot submit input by itself.
pub(crate) struct OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1 {
    source_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    checked: CheckedUntrustedMtgoPlayerVisibleCombatExecutionStepV1,
}

impl OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1 {
    pub(crate) fn operation_v1(
        &self,
    ) -> mtgo_blackbox_v1::MtgoPlayerVisibleCombatSubmittedOperationV1 {
        self.checked.operation_v1()
    }

    pub(crate) fn execution_step_commitment_sha256_v1(&self) -> &str {
        self.checked.execution_step_commitment_sha256_v1()
    }
}

/// Combat input was submitted once and the process-wide input gate remains
/// closed. Only a fresh, same-duel, source-attested visible result can consume
/// this value and reopen the gate.
pub(crate) struct OpaqueMtgoPendingAttestedDirectVisibleCombatDispatchV1 {
    source_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    checked: CheckedUntrustedMtgoPlayerVisibleCombatExecutionStepV1,
    dispatch_receipt_commitment_sha256: String,
    dispatch_submitted_at_unix_millis: u128,
}

impl OpaqueMtgoPendingAttestedDirectVisibleCombatDispatchV1 {
    pub(crate) fn dispatch_receipt_commitment_sha256_v1(&self) -> &str {
        &self.dispatch_receipt_commitment_sha256
    }
}

/// Exact intended combat transition confirmed by a strictly newer sanitized
/// player-visible producer result. The fresh observation remains private and
/// can be consumed only by the operator's continuation or rescore path.
pub(crate) struct OpaqueMtgoConfirmedAttestedDirectVisibleCombatTransitionV1 {
    fresh_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    transition: CheckedUntrustedMtgoPlayerVisibleCombatTransitionV1,
    pending_receipt_commitment_sha256: String,
}

impl OpaqueMtgoConfirmedAttestedDirectVisibleCombatTransitionV1 {
    pub(crate) fn progress_v1(&self) -> MtgoPlayerVisibleCombatTransitionProgressV1 {
        self.transition.progress_v1()
    }

    pub(crate) fn confirmation_commitment_sha256_v1(&self) -> &str {
        self.transition.confirmation_commitment_sha256_v1()
    }

    pub(crate) fn pending_receipt_commitment_sha256_v1(&self) -> &str {
        &self.pending_receipt_commitment_sha256
    }

    pub(crate) fn into_same_plan_continuation_v1(
        self,
    ) -> Result<OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1, String> {
        if self.transition.progress_v1()
            != MtgoPlayerVisibleCombatTransitionProgressV1::ContinueSamePlan
        {
            return Err(
                "this confirmed combat transition requires a fresh model decision".to_owned(),
            );
        }
        let prepared = self
            .transition
            .into_prepared_continuation_v1()
            .map_err(|error| format!("resume confirmed combat plan: {error}"))?;
        let checked = prepare_player_visible_combat_execution_step_v1(
            prepared,
            &self.fresh_observation.exact_result_bytes.0,
        )
        .map_err(|error| format!("prepare next confirmed combat step: {error}"))?;
        Ok(OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1 {
            source_observation: self.fresh_observation,
            checked,
        })
    }

    pub(crate) fn into_fresh_observation_and_trace_for_model_v1(
        self,
    ) -> Result<
        (
            OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
            CheckedUntrustedMtgoPlayerVisibleCombatDecisionTraceV1,
        ),
        String,
    > {
        if self.transition.progress_v1()
            != MtgoPlayerVisibleCombatTransitionProgressV1::AwaitFreshCombatModelDecision
        {
            return Err(
                "this confirmed combat transition does not require a fresh model decision"
                    .to_owned(),
            );
        }
        let trace = self
            .transition
            .into_pending_rescore_trace_v1()
            .map_err(|error| format!("retain confirmed combat rescore trace: {error}"))?;
        Ok((self.fresh_observation, trace))
    }

    pub(crate) fn into_confirmed_decision_v1(
        self,
    ) -> Result<OpaqueMtgoConfirmedAttestedDirectVisibleCombatDecisionV1, String> {
        if self.transition.progress_v1()
            != MtgoPlayerVisibleCombatTransitionProgressV1::CombatDeclarationComplete
        {
            return Err("this combat transition is not visibly complete".to_owned());
        }
        let confirmed = self
            .transition
            .into_confirmed_combat_decision_v1()
            .map_err(|error| format!("finalize confirmed visible combat decision: {error}"))?;
        Ok(OpaqueMtgoConfirmedAttestedDirectVisibleCombatDecisionV1 {
            fresh_observation: self.fresh_observation,
            confirmed,
        })
    }
}

/// Complete player-visible combat transaction retained with the exact fresh
/// attested observation that confirmed its final declared state.
pub(crate) struct OpaqueMtgoConfirmedAttestedDirectVisibleCombatDecisionV1 {
    fresh_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    confirmed: CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
}

impl OpaqueMtgoConfirmedAttestedDirectVisibleCombatDecisionV1 {
    pub(crate) fn decision_commitment_sha256_v1(&self) -> &str {
        self.confirmed.decision_commitment_sha256_v1()
    }

    pub(crate) fn after_capture_commitment_sha256_v1(&self) -> &str {
        &self
            .fresh_observation
            .commitments
            .after_capture_commitment_sha256
    }

    pub(crate) fn into_parts_v1(
        self,
    ) -> (
        OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
        CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
    ) {
        (self.fresh_observation, self.confirmed)
    }
}

pub(crate) fn join_attested_direct_visible_combat_rescore_trace_v1(
    scored: OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1,
    trace: CheckedUntrustedMtgoPlayerVisibleCombatDecisionTraceV1,
) -> Result<OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1, String> {
    let OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1 {
        _observation,
        outcome,
    } = scored;
    let prepared = match outcome {
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared) => prepared,
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { .. } => {
            return Err("an abstained combat rescore cannot join a pending transaction".to_owned())
        }
    };
    let prepared = join_player_visible_combat_rescore_trace_v1(trace, prepared)
        .map_err(|error| format!("join exact combat rescore trace: {error}"))?;
    Ok(
        OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1 {
            _observation,
            outcome: CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared),
        },
    )
}

pub(crate) fn prepare_attested_direct_visible_combat_step_v1(
    scored: OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1,
) -> Result<OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1, String> {
    let OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1 {
        _observation: source_observation,
        outcome,
    } = scored;
    let prepared = match outcome {
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared) => prepared,
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { .. } => {
            return Err("an abstained combat observation cannot prepare input".to_owned());
        }
    };
    let checked = prepare_player_visible_combat_execution_step_v1(
        prepared,
        &source_observation.exact_result_bytes.0,
    )
    .map_err(|error| format!("prepare source-attested combat step: {error}"))?;
    Ok(OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1 {
        source_observation,
        checked,
    })
}

impl OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1 {
    pub fn abstention_reason_v1(&self) -> Option<MtgoVisibleDuelViewModelBrokerAbstentionReasonV1> {
        match &self.outcome {
            CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Abstained { reason } => {
                Some(*reason)
            }
            CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(_) => None,
        }
    }

    pub fn selected_index_v1(&self) -> Option<usize> {
        match &self.outcome {
            CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Abstained { .. } => None,
            CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(selection) => {
                Some(selection.selected_index_v1())
            }
        }
    }

    pub fn selected_action_v1(&self) -> Option<&mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1> {
        match &self.outcome {
            CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Abstained { .. } => None,
            CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(selection) => {
                Some(selection.selected_action_v1())
            }
        }
    }

    pub fn model_scoring_completed_v1(&self) -> bool {
        true
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Qualification-only selected action re-observed through the exact pinned
/// producer. The private type cannot carry caller-selected logits into an
/// externally callable live dispatch route.
enum PrivateMtgoInitialDirectVisibleObservationV1 {
    Attested(OpaqueMtgoAttestedDirectVisibleSourceObservationV1),
    StableBackground(OpaqueMtgoStableBackgroundDirectVisibleSourceV1),
}

pub struct OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1 {
    _initial_observation: PrivateMtgoInitialDirectVisibleObservationV1,
    _refreshed_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    selection: CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1,
    commitments: MtgoRefreshedAttestedDirectVisibleSelectionCommitmentsV1,
}

/// Direct-source selection retained with an independent same-decision pixel
/// perception, exact competitive lifecycle, and pixel-derived before-dispatch
/// regions. Internal client objects never enter this value. It is move-only
/// and cannot expose pixels, rectangles, input, entry, or spending authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1;
/// fn cannot_act(value: OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1) {
///     let _ = value.coordinates();
///     value.dispatch();
/// }
/// ```
pub(crate) struct OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1 {
    _initial_observation: PrivateMtgoInitialDirectVisibleObservationV1,
    _refreshed_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    _corroborating_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    checked: CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1,
    commitments: MtgoAttestedDirectVisibleCompetitiveBeforeDispatchCommitmentsV1,
}

impl OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1 {
    pub(crate) fn selected_action_v1(&self) -> &mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1 {
        self.checked.selected_action_v1()
    }

    pub(crate) fn binding_commitment_sha256_v1(&self) -> &str {
        &self.commitments._binding_commitment_sha256
    }

    pub(crate) fn checked_v1(&self) -> &CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1 {
        &self.checked
    }
}

pub(crate) struct OpaqueMtgoPendingAttestedDirectVisibleDispatchV1 {
    before_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    before: CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1,
    dispatch_receipt_commitment_sha256: String,
    dispatch_submitted_at_unix_millis: u128,
}

impl OpaqueMtgoPendingAttestedDirectVisibleDispatchV1 {
    pub(crate) fn dispatch_receipt_commitment_sha256_v1(&self) -> &str {
        &self.dispatch_receipt_commitment_sha256
    }
}

impl OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.selection.selected_index_v1()
    }

    pub fn selected_action_v1(&self) -> &mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1 {
        self.selection.selected_action_v1()
    }

    pub fn commitments_v1(&self) -> MtgoRefreshedAttestedDirectVisibleSelectionCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Qualification-only bridge into a caller-supplied player-visible scorer.
/// This must remain crate-private even after a no-stakes producer commitment is
/// pinned. A future production bridge must obtain inference only from the
/// opaque loaded checkpoint and its native player-visible capability.
pub fn score_ratified_attested_direct_visible_source_observation_v1<
    S: MtgoPlayerVisibleDuelScorerV1,
>(
    observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    reviewed_qualification_commitment_sha256: &str,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1, String> {
    require_ratified_direct_visible_source_qualification_v1(
        reviewed_qualification_commitment_sha256,
    )?;
    let outcome = score_and_select_strict_visible_duel_producer_result_v1(
        &observation.exact_result_bytes.0,
        deployment_commitment_sha256,
        scorer,
    )
    .map_err(|error| format!("score attested direct visible observation: {error}"))?;
    Ok(OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1 {
        _observation: observation,
        outcome,
    })
}

/// Qualification-only bridge from one source-attested observation into the
/// unified player-visible combat scorer. The production source qualification
/// root is empty, so no live observation can currently reach a caller-supplied
/// scorer through this function.
pub fn score_ratified_attested_direct_visible_combat_source_observation_v1<
    S: MtgoPlayerVisibleCombatScorerV1,
>(
    observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    reviewed_qualification_commitment_sha256: &str,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1, String> {
    require_ratified_direct_visible_combat_source_qualification_v1(
        reviewed_qualification_commitment_sha256,
    )?;
    let outcome = score_and_prepare_strict_visible_combat_producer_result_v1(
        &observation.exact_result_bytes.0,
        deployment_commitment_sha256,
        scorer,
    )
    .map_err(|error| format!("score attested direct visible combat observation: {error}"))?;
    Ok(
        OpaqueMtgoRatifiedAttestedDirectVisibleCombatScoringOutcomeV1 {
            _observation: observation,
            outcome,
        },
    )
}

/// Dormant background-source bridge into an ordinary player-visible scorer.
/// The scorer remains caller-supplied and this function remains crate-private.
/// Production use additionally requires a future opaque loaded-checkpoint
/// scorer and a reviewed data-bearing background qualification commitment.
pub(crate) fn score_ratified_stable_background_direct_visible_source_observation_v1<
    S: MtgoPlayerVisibleDuelScorerV1,
>(
    observation: OpaqueMtgoStableBackgroundDirectVisibleSourceV1,
    reviewed_qualification_commitment_sha256: &str,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoRatifiedStableBackgroundDirectVisibleScoringOutcomeV1, String> {
    require_ratified_background_direct_visible_source_qualification_v1(
        reviewed_qualification_commitment_sha256,
    )?;
    let outcome = score_and_select_strict_visible_duel_producer_result_v1(
        &observation._exact_result_bytes.0,
        deployment_commitment_sha256,
        scorer,
    )
    .map_err(|error| format!("score stable background visible observation: {error}"))?;
    Ok(
        OpaqueMtgoRatifiedStableBackgroundDirectVisibleScoringOutcomeV1 {
            observation,
            outcome,
        },
    )
}

/// Dormant background-source bridge into the unified combat scorer. The
/// resulting plan cannot prepare a broker operation until a fresh foreground
/// source-attested result proves the exact same visible presentation bytes.
pub(crate) fn score_ratified_stable_background_direct_visible_combat_source_observation_v1<
    S: MtgoPlayerVisibleCombatScorerV1,
>(
    observation: OpaqueMtgoStableBackgroundDirectVisibleSourceV1,
    reviewed_qualification_commitment_sha256: &str,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoRatifiedStableBackgroundDirectVisibleCombatScoringOutcomeV1, String> {
    require_ratified_background_direct_visible_combat_source_qualification_v1(
        reviewed_qualification_commitment_sha256,
    )?;
    let outcome = score_and_prepare_strict_visible_combat_producer_result_v1(
        &observation._exact_result_bytes.0,
        deployment_commitment_sha256,
        scorer,
    )
    .map_err(|error| format!("score stable background visible combat observation: {error}"))?;
    Ok(
        OpaqueMtgoRatifiedStableBackgroundDirectVisibleCombatScoringOutcomeV1 {
            observation,
            outcome,
        },
    )
}

pub(crate) fn require_ratified_background_direct_visible_source_qualification_v1(
    reviewed_qualification_commitment_sha256: &str,
) -> Result<(), String> {
    let Some(ratified) = RATIFIED_BACKGROUND_DIRECT_VISIBLE_SOURCE_QUALIFICATION_COMMITMENT_V1
    else {
        return Err(
            "the production background visible-source qualification root is empty".to_owned(),
        );
    };
    if reviewed_qualification_commitment_sha256 != ratified {
        return Err(
            "the reviewed background visible-source qualification commitment is not ratified"
                .to_owned(),
        );
    }
    Ok(())
}

pub(crate) fn require_ratified_background_direct_visible_combat_source_qualification_v1(
    reviewed_qualification_commitment_sha256: &str,
) -> Result<(), String> {
    let Some(ratified) =
        RATIFIED_BACKGROUND_DIRECT_VISIBLE_COMBAT_SOURCE_QUALIFICATION_COMMITMENT_V1
    else {
        return Err(
            "the production background visible combat-source qualification root is empty"
                .to_owned(),
        );
    };
    if reviewed_qualification_commitment_sha256 != ratified {
        return Err(
            "the reviewed background visible combat-source qualification commitment is not ratified"
                .to_owned(),
        );
    }
    Ok(())
}

pub(crate) fn require_ratified_direct_visible_combat_source_qualification_v1(
    reviewed_qualification_commitment_sha256: &str,
) -> Result<(), String> {
    let Some(ratified) = RATIFIED_DIRECT_VISIBLE_COMBAT_SOURCE_QUALIFICATION_COMMITMENT_V1 else {
        return Err(
            "the production player-visible combat-source qualification root is empty".to_owned(),
        );
    };
    if reviewed_qualification_commitment_sha256 != ratified {
        return Err(
            "the reviewed player-visible combat-source qualification commitment is not ratified"
                .to_owned(),
        );
    }
    Ok(())
}

pub(crate) fn require_ratified_direct_visible_source_qualification_v1(
    reviewed_qualification_commitment_sha256: &str,
) -> Result<(), String> {
    let Some(ratified) = RATIFIED_DIRECT_VISIBLE_SOURCE_QUALIFICATION_COMMITMENT_V1 else {
        return Err(
            "the production no-stakes direct-source qualification root is empty".to_owned(),
        );
    };
    if reviewed_qualification_commitment_sha256 != ratified {
        return Err(
            "the reviewed no-stakes direct-source qualification commitment is not ratified"
                .to_owned(),
        );
    }
    Ok(())
}

/// Re-observes a ratified scored selection through the exact pinned live
/// producer. A changed visible decision, action order, runtime, profile,
/// process, window, output, or geometry rejects. The production qualification
/// root is still empty, so this path remains dormant until live review.
pub fn refresh_ratified_attested_direct_visible_selection_v1(
    scored: OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    capture_timeout_ms: u32,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1, String> {
    let OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1 {
        _observation: initial_observation,
        outcome,
    } = scored;
    let CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(selection) = outcome else {
        return Err(
            "an abstained direct-source observation has no selection to refresh".to_owned(),
        );
    };
    let initial_commitments = initial_observation.commitments_v1();
    if initial_commitments.runtime_identity_commitment_sha256
        != runtime.commitments.runtime_identity_commitment_sha256
        || initial_commitments.broker_binary_sha256 != runtime.commitments.broker_binary_sha256
        || initial_commitments.producer_binary_sha256 != runtime.commitments.producer_binary_sha256
    {
        return Err("scored direct-source observation and refreshed runtime differ".to_owned());
    }
    let before_frame = capture_admitted_mtgo_duel_visible_frame_v1(profile, capture_timeout_ms)?;
    let refreshed_observation = observe_attested_direct_visible_source_v1(
        before_frame,
        profile,
        runtime,
        capture_timeout_ms,
        broker_timeout_ms,
    )?;
    let refreshed_commitments = refreshed_observation.commitments_v1();
    validate_same_duel_observation_lineage_v1(
        &initial_observation._after_frame,
        &refreshed_observation._before_frame,
    )?;
    if refreshed_commitments.runtime_identity_commitment_sha256
        != initial_commitments.runtime_identity_commitment_sha256
        || refreshed_commitments.broker_binary_sha256 != initial_commitments.broker_binary_sha256
        || refreshed_commitments.producer_binary_sha256
            != initial_commitments.producer_binary_sha256
        || refreshed_observation
            ._after_frame
            .source_frame
            .manifest
            .captured_at_unix_millis
            <= initial_observation
                ._after_frame
                .source_frame
                .manifest
                .captured_at_unix_millis
    {
        return Err(
            "refreshed direct-source observation is not a newer identical runtime".to_owned(),
        );
    }
    let selection = refresh_direct_visible_selection_before_dispatch_v1(
        *selection,
        &refreshed_observation.exact_result_bytes.0,
    )
    .map_err(|error| format!("refresh attested direct visible selection: {error}"))?;
    let scored_refresh_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_SOURCE_SCORED_REFRESH_DOMAIN_V1,
        &[
            initial_commitments.observation_commitment_sha256.as_bytes(),
            refreshed_commitments
                .observation_commitment_sha256
                .as_bytes(),
            selection.selection_commitment_sha256_v1().as_bytes(),
            selection.refresh_commitment_sha256_v1().as_bytes(),
            b"exact_visible_result_reobserved_after_score_no_input_authority",
        ],
    );
    let commitments = MtgoRefreshedAttestedDirectVisibleSelectionCommitmentsV1 {
        runtime_identity_commitment_sha256: initial_commitments.runtime_identity_commitment_sha256,
        initial_observation_commitment_sha256: initial_commitments.observation_commitment_sha256,
        refreshed_observation_commitment_sha256: refreshed_commitments
            .observation_commitment_sha256,
        selection_commitment_sha256: selection.selection_commitment_sha256_v1().to_owned(),
        refresh_commitment_sha256: selection.refresh_commitment_sha256_v1().to_owned(),
        scored_refresh_commitment_sha256,
    };
    Ok(OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1 {
        _initial_observation: PrivateMtgoInitialDirectVisibleObservationV1::Attested(
            initial_observation,
        ),
        _refreshed_observation: refreshed_observation,
        selection,
        commitments,
    })
}

/// Converts a ratified background ordinary selection into the existing
/// foreground-attested refresh owner. The current producer result must be
/// exactly the same visible-equivalent bytes that the model scored. The later
/// pixel corroboration and dispatch gates are unchanged.
pub(crate) fn refresh_ratified_stable_background_direct_visible_selection_v1(
    scored: OpaqueMtgoRatifiedStableBackgroundDirectVisibleScoringOutcomeV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    capture_timeout_ms: u32,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1, String> {
    let OpaqueMtgoRatifiedStableBackgroundDirectVisibleScoringOutcomeV1 {
        observation: initial_observation,
        outcome,
    } = scored;
    let CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(selection) = outcome else {
        return Err(
            "an abstained background visible observation has no selection to refresh".to_owned(),
        );
    };
    let initial_commitments = initial_observation.commitments_v1();
    if initial_commitments.runtime_identity_commitment_sha256
        != runtime.commitments.runtime_identity_commitment_sha256
        || initial_commitments.broker_binary_sha256 != runtime.commitments.broker_binary_sha256
        || initial_commitments.producer_binary_sha256 != runtime.commitments.producer_binary_sha256
    {
        return Err("background selection and foreground observer runtime differ".to_owned());
    }
    let before_frame = capture_admitted_mtgo_duel_visible_frame_v1(profile, capture_timeout_ms)?;
    let refreshed_observation = observe_attested_direct_visible_source_v1(
        before_frame,
        profile,
        runtime,
        capture_timeout_ms,
        broker_timeout_ms,
    )?;
    let refreshed_commitments = refreshed_observation.commitments_v1();
    if refreshed_commitments.runtime_identity_commitment_sha256
        != initial_commitments.runtime_identity_commitment_sha256
        || refreshed_commitments.broker_binary_sha256 != initial_commitments.broker_binary_sha256
        || refreshed_commitments.producer_binary_sha256
            != initial_commitments.producer_binary_sha256
    {
        return Err("background selection and fresh foreground observation differ".to_owned());
    }
    let selection = refresh_direct_visible_selection_before_dispatch_v1(
        *selection,
        &refreshed_observation.exact_result_bytes.0,
    )
    .map_err(|error| format!("refresh background visible selection in foreground: {error}"))?;
    let scored_refresh_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_SOURCE_SCORED_REFRESH_DOMAIN_V1,
        &[
            initial_commitments.stability_commitment_sha256.as_bytes(),
            refreshed_commitments
                .observation_commitment_sha256
                .as_bytes(),
            selection.selection_commitment_sha256_v1().as_bytes(),
            selection.refresh_commitment_sha256_v1().as_bytes(),
            b"background_visible_result_exactly_reobserved_foreground_before_input",
        ],
    );
    let commitments = MtgoRefreshedAttestedDirectVisibleSelectionCommitmentsV1 {
        runtime_identity_commitment_sha256: initial_commitments.runtime_identity_commitment_sha256,
        initial_observation_commitment_sha256: initial_commitments.stability_commitment_sha256,
        refreshed_observation_commitment_sha256: refreshed_commitments
            .observation_commitment_sha256,
        selection_commitment_sha256: selection.selection_commitment_sha256_v1().to_owned(),
        refresh_commitment_sha256: selection.refresh_commitment_sha256_v1().to_owned(),
        scored_refresh_commitment_sha256,
    };
    Ok(OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1 {
        _initial_observation: PrivateMtgoInitialDirectVisibleObservationV1::StableBackground(
            initial_observation,
        ),
        _refreshed_observation: refreshed_observation,
        selection,
        commitments,
    })
}

/// Rebinds a ratified background combat plan to a fresh foreground-attested
/// producer result. Exact byte equality is required before the existing
/// source-bound combat step can be constructed.
pub(crate) fn refresh_ratified_stable_background_direct_visible_combat_step_v1(
    scored: OpaqueMtgoRatifiedStableBackgroundDirectVisibleCombatScoringOutcomeV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    capture_timeout_ms: u32,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1, String> {
    let OpaqueMtgoRatifiedStableBackgroundDirectVisibleCombatScoringOutcomeV1 {
        observation: initial_observation,
        outcome,
    } = scored;
    let prepared = match outcome {
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared) => prepared,
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { .. } => {
            return Err(
                "an abstained background combat observation cannot prepare input".to_owned(),
            );
        }
    };
    let initial_commitments = initial_observation.commitments_v1();
    if initial_commitments.runtime_identity_commitment_sha256
        != runtime.commitments.runtime_identity_commitment_sha256
        || initial_commitments.broker_binary_sha256 != runtime.commitments.broker_binary_sha256
        || initial_commitments.producer_binary_sha256 != runtime.commitments.producer_binary_sha256
    {
        return Err("background combat plan and foreground observer runtime differ".to_owned());
    }
    let before_frame = capture_admitted_mtgo_duel_visible_frame_v1(profile, capture_timeout_ms)?;
    let refreshed_observation = observe_attested_direct_visible_source_v1(
        before_frame,
        profile,
        runtime,
        capture_timeout_ms,
        broker_timeout_ms,
    )?;
    let refreshed_commitments = refreshed_observation.commitments_v1();
    if refreshed_commitments.runtime_identity_commitment_sha256
        != initial_commitments.runtime_identity_commitment_sha256
        || refreshed_commitments.broker_binary_sha256 != initial_commitments.broker_binary_sha256
        || refreshed_commitments.producer_binary_sha256
            != initial_commitments.producer_binary_sha256
        || refreshed_observation.exact_result_bytes.0 != initial_observation._exact_result_bytes.0
    {
        return Err(
            "background combat plan is not the exact fresh foreground visible result".to_owned(),
        );
    }
    let checked = prepare_player_visible_combat_execution_step_v1(
        prepared,
        &refreshed_observation.exact_result_bytes.0,
    )
    .map_err(|error| format!("prepare refreshed background combat step: {error}"))?;
    Ok(OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1 {
        source_observation: refreshed_observation,
        checked,
    })
}

/// Corroborates the refreshed direct result with the independent, reviewed
/// visible-pixel perception path on a strictly newer composed frame, then
/// constructs the match scope and before-dispatch region plan from retained
/// pixels. The complete player-visible decision must be byte-for-byte equal at
/// the semantic boundary, and every current-frame evidence region must be
/// pixel-identical across the direct and corroborating frames.
///
/// This performs capture and classification only. It sends no input and does
/// not create event-entry or spending authority.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_attested_direct_visible_competitive_before_dispatch_v1(
    refreshed: OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    perception_runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    corroborating_frame_sequence: u64,
    region_set: Option<MtgoAttestedDirectVisibleBeforeDispatchRegionSetV1>,
    mode_authorization: &MtgoAuthorizationScopeV1,
    gameplay_authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1, String> {
    if corroborating_frame_sequence < 2 {
        return Err("corroborating direct-source frame sequence must be at least two".to_owned());
    }
    if let Some(region_set) = region_set.as_ref() {
        if !region_set.candidate_set_complete {
            return Err("direct-source before-dispatch region set is incomplete".to_owned());
        }
        validate_requested_postcondition_categories_v1(&region_set.regions)?;
    }
    let OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1 {
        _initial_observation: initial_observation,
        _refreshed_observation: refreshed_observation,
        selection,
        commitments: refresh_commitments,
    } = refreshed;
    let direct_decision = match &refreshed_observation.result {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { decision } => decision.as_ref(),
        MtgoVisibleDuelViewModelBrokerResultV1::Abstained { .. } => {
            return Err("refreshed direct-source observation abstained".to_owned())
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { .. }
        | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
            ..
        }
        | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
            ..
        }
        | MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection { .. }
        | MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { .. } => {
            return Err(
                "refreshed direct-source observation is a combat-specific selection".to_owned(),
            )
        }
    };

    let corroborating_frame = capture_admitted_mtgo_duel_visible_frame_v1(profile, timeout_ms)?;
    validate_same_duel_observation_lineage_v1(
        &refreshed_observation._after_frame,
        &corroborating_frame,
    )?;
    let corroborating_capture = corroborating_frame.commitments_v1();
    let corroborating_frame_id = frame_id_from_capture_commitment_v1(
        &corroborating_capture
            .source_capture
            .capture_commitment_sha256,
        0,
    )?;
    let mut corroborating_perception = perceive_admitted_duel_frame_v1(
        corroborating_frame,
        profile,
        perception_runtime,
        MtgoDuelPerceptionFrameIdentityV1 {
            frame_id: corroborating_frame_id,
            frame_sequence: corroborating_frame_sequence,
        },
        timeout_ms,
    )?;
    let pixel_decision = corroborating_perception
        .player_visible_duel_decision_input_v1()
        .map_err(|error| format!("project corroborating player-visible decision: {error}"))?;
    if &pixel_decision != direct_decision
        || pixel_decision
            .ordered_legal_actions
            .get(selection.selected_index_v1())
            != Some(selection.selected_action_v1())
    {
        return Err(
            "direct parsing and visible-pixel perception disagree on the complete decision"
                .to_owned(),
        );
    }
    let region_set = match region_set {
        Some(region_set) => region_set,
        None => derive_direct_visible_postcondition_region_set_v1(
            &corroborating_perception,
            selection.selected_index_v1(),
            &pixel_decision,
            selection.selected_action_v1(),
        )?,
    };

    let equivalent_visible_regions_commitment_sha256 =
        exact_equivalent_visible_regions_commitment_v1(
            &refreshed_observation._after_frame,
            &corroborating_perception,
        )?;
    let before_source = &refreshed_observation._after_frame.source_frame;
    let after_source = &corroborating_perception.source_frame.source_frame;
    let before_capture = refreshed_observation._after_frame.commitments_v1();
    let after_perception_commitments = corroborating_perception.commitments_v1();
    let before_frame_id = frame_id_from_capture_commitment_v1(
        &before_capture.source_capture.capture_commitment_sha256,
        corroborating_frame_id,
    )?;
    let before_frame_sequence = corroborating_frame_sequence - 1;
    let before_client_identity =
        competitive_entry_window_continuity_commitment_for_frame_v1(before_source)?;
    let after_client_identity =
        competitive_entry_window_continuity_commitment_for_frame_v1(after_source)?;
    if before_client_identity != after_client_identity {
        return Err("direct-source corroboration changed client identity".to_owned());
    }
    let lifecycle_commitment = after_perception_commitments
        .competitive_lifecycle_snapshot_commitment_sha256
        .clone()
        .ok_or("direct-source competitive corroboration lacks visible lifecycle")?;
    let lifecycle = corroborating_perception
        .competitive_lifecycle
        .take()
        .ok_or("direct-source competitive corroboration lost visible lifecycle")?;
    let source_size = MtgoSizePxV1 {
        width: after_source.manifest.frame.canonical_width,
        height: after_source.manifest.frame.canonical_height,
    };
    let observation = refreshed_observation.commitments_v1();
    let bracket = MtgoDirectVisibleCompetitiveObservationBracketV1 {
        schema_version: MTGO_DIRECT_VISIBLE_COMPETITIVE_OBSERVATION_BRACKET_SCHEMA_V1,
        information_boundary: "seated_player_visible_ui_equivalent_only_v1".to_owned(),
        capture_role: "acting_player_duel".to_owned(),
        before_frame_id,
        before_frame_sequence,
        before_captured_at_unix_millis: before_capture.source_capture.captured_at_unix_millis,
        before_frame_sha256: before_capture.source_capture.canonical_bgra8_sha256,
        after_frame_id: corroborating_frame_id,
        after_frame_sequence: corroborating_frame_sequence,
        after_captured_at_unix_millis: corroborating_capture.source_capture.captured_at_unix_millis,
        after_frame_sha256: corroborating_capture
            .source_capture
            .canonical_bgra8_sha256
            .clone(),
        client_size_px: source_size.clone(),
        before_decision_regions_sha256: equivalent_visible_regions_commitment_sha256.clone(),
        after_decision_regions_sha256: equivalent_visible_regions_commitment_sha256.clone(),
        client_identity_commitment_sha256: after_client_identity,
        broker_binary_sha256: observation.broker_binary_sha256,
        producer_binary_sha256: observation.producer_binary_sha256,
        visible_equivalence_profile_commitment_sha256: profile
            .perception_profile_commitment_sha256()
            .to_owned(),
        producer_first_export_is_player_visible_schema: true,
        raw_source_values_emitted: false,
        internal_identifiers_emitted: false,
        bracket_complete: true,
    };
    let selected_index = selection.selected_index_v1();
    let plan = bind_refreshed_direct_visible_selection_to_competitive_match_v1(
        selection,
        bracket,
        lifecycle,
        mode_authorization,
        gameplay_authorization,
    )
    .map_err(|error| format!("bind attested direct source to competitive match: {error}"))?;
    let direct_competitive_scope_commitment_sha256 = plan
        .direct_competitive_scope_commitment_sha256_v1()
        .to_owned();
    let mut before_regions = Vec::with_capacity(region_set.regions.len());
    for spec in region_set.regions {
        let rect_client_px = direct_postcondition_region_from_perception_v1(
            &corroborating_perception,
            selected_index,
            spec.kind,
        )?;
        let before_bgra8_sha256 = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
            &after_source.canonical_bgra8,
            &source_size,
            &rect_client_px,
        )
        .map_err(|error| format!("rehash direct before-dispatch visible region: {error}"))?;
        before_regions.push(MtgoDirectVisibleGameplayBeforeRegionV1 {
            kind: spec.kind,
            rect_client_px,
            before_bgra8_sha256,
        });
    }
    let record = MtgoDirectVisibleGameplayBeforeDispatchRecordV1 {
        schema_version: MTGO_DIRECT_VISIBLE_GAMEPLAY_BEFORE_DISPATCH_SCHEMA_V1,
        direct_competitive_scope_commitment_sha256: direct_competitive_scope_commitment_sha256
            .clone(),
        source_frame_id: corroborating_frame_id,
        source_frame_sequence: corroborating_frame_sequence,
        source_captured_at_unix_millis: corroborating_capture
            .source_capture
            .captured_at_unix_millis,
        source_frame_sha256: corroborating_capture.source_capture.canonical_bgra8_sha256,
        client_size_px: source_size,
        region_set_complete: true,
        regions: before_regions,
        expected_game_log_baseline_commitment_sha256: None,
    };
    let checked = prepare_direct_visible_gameplay_before_dispatch_v1(plan, record)
        .map_err(|error| format!("prepare attested direct before-dispatch plan: {error}"))?;
    let before_dispatch_commitment_sha256 =
        checked.before_dispatch_commitment_sha256_v1().to_owned();
    let binding_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_SOURCE_COMPETITIVE_BEFORE_DISPATCH_DOMAIN_V1,
        &[
            refresh_commitments
                .scored_refresh_commitment_sha256
                .as_bytes(),
            after_perception_commitments
                .perception_result_commitment_sha256
                .as_bytes(),
            lifecycle_commitment.as_bytes(),
            equivalent_visible_regions_commitment_sha256.as_bytes(),
            direct_competitive_scope_commitment_sha256.as_bytes(),
            before_dispatch_commitment_sha256.as_bytes(),
            b"same_visible_decision_and_pixels_no_input_entry_or_spending_authority",
        ],
    );
    let commitments = MtgoAttestedDirectVisibleCompetitiveBeforeDispatchCommitmentsV1 {
        _scored_refresh_commitment_sha256: refresh_commitments.scored_refresh_commitment_sha256,
        _corroborating_perception_result_commitment_sha256: after_perception_commitments
            .perception_result_commitment_sha256,
        _corroborating_lifecycle_snapshot_commitment_sha256: lifecycle_commitment,
        _equivalent_visible_regions_commitment_sha256: equivalent_visible_regions_commitment_sha256,
        _direct_competitive_scope_commitment_sha256: direct_competitive_scope_commitment_sha256,
        _before_dispatch_commitment_sha256: before_dispatch_commitment_sha256,
        _binding_commitment_sha256: binding_commitment_sha256,
    };
    Ok(OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1 {
        _initial_observation: initial_observation,
        _refreshed_observation: refreshed_observation,
        _corroborating_perception: corroborating_perception,
        checked,
        commitments,
    })
}

fn direct_postcondition_region_from_perception_v1(
    perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    selected_index: usize,
    kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
) -> Result<MtgoRectPxV1, String> {
    let frame_id = perception.validated_decision.frame_id();
    let exact_evidence_id = match kind {
        MtgoPlayerVisibleGameplayPostconditionKindV1::SelectedControlChanged => {
            let selected_semantic = perception
                .validated_decision
                .legal_actions()
                .get(selected_index)
                .ok_or("direct selected action index exceeds corroborating legal actions")?;
            let matches = perception
                .visible_controls
                .controls
                .iter()
                .filter(|control| control.semantic == *selected_semantic && control.visibly_enabled)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(
                    "direct selected action lacks one unique visible control region".to_owned(),
                );
            }
            Some(matches[0].frame_region_evidence_id)
        }
        MtgoPlayerVisibleGameplayPostconditionKindV1::PromptChanged => {
            if !perception.visible_controls.prompt_reconciled {
                return Err("direct prompt is not visibly reconciled".to_owned());
            }
            Some(perception.visible_controls.prompt_frame_region_evidence_id)
        }
        _ => None,
    };
    if let Some(evidence_id) = exact_evidence_id {
        return current_frame_region_v1(
            &perception.decision_record.evidence,
            frame_id,
            evidence_id,
        );
    }

    let mut supported_rects = BTreeMap::<(u32, u32, u32, u32), MtgoRectPxV1>::new();
    for leaf in perception
        .decision_record
        .provenance
        .iter()
        .filter(|leaf| visible_pointer_supports_postcondition_kind_v1(&leaf.json_pointer, kind))
    {
        for candidate in &perception.decision_record.evidence {
            let MtgoEvidenceSourceV1::FrameRegion {
                frame_id: candidate_frame_id,
                rect,
                ..
            } = &candidate.source
            else {
                continue;
            };
            if *candidate_frame_id == frame_id
                && leaf.evidence_ids.iter().any(|source_id| {
                    evidence_reaches_frame_region_v1(
                        &perception.decision_record.evidence,
                        *source_id,
                        candidate.evidence_id,
                        &mut HashSet::new(),
                    )
                })
            {
                supported_rects.insert((rect.x, rect.y, rect.width, rect.height), rect.clone());
            }
        }
    }
    bounding_visible_rect_v1(supported_rects.values())
        .ok_or("direct before-dispatch category lacks current visible provenance".to_owned())
}

fn derive_direct_visible_postcondition_region_set_v1(
    perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    selected_index: usize,
    decision: &mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1,
    action: &mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1,
) -> Result<MtgoAttestedDirectVisibleBeforeDispatchRegionSetV1, String> {
    let family = player_visible_duel_action_family_v1(action);
    let recipes = direct_visible_postcondition_recipes_v1(decision, action)?;
    for recipe in &recipes {
        let mut rects = Vec::with_capacity(recipe.len());
        let mut supported = true;
        for kind in recipe {
            let Ok(rect) =
                direct_postcondition_region_from_perception_v1(perception, selected_index, *kind)
            else {
                supported = false;
                break;
            };
            let candidate = (rect.x, rect.y, rect.width, rect.height);
            if rects
                .iter()
                .any(|existing| direct_visible_rects_intersect_v1(*existing, candidate))
            {
                supported = false;
                break;
            }
            rects.push(candidate);
        }
        if supported {
            return Ok(MtgoAttestedDirectVisibleBeforeDispatchRegionSetV1 {
                candidate_set_complete: true,
                regions: recipe
                    .iter()
                    .copied()
                    .map(|kind| MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1 { kind })
                    .collect(),
            });
        }
    }
    Err(format!(
        "fresh visible perception lacks a complete nonoverlapping {family:?} postcondition recipe"
    ))
}

fn direct_visible_postcondition_recipes_v1(
    decision: &mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1,
    action: &mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1,
) -> Result<Vec<Vec<MtgoPlayerVisibleGameplayPostconditionKindV1>>, String> {
    use mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1 as A;
    use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;

    Ok(match player_visible_duel_action_family_v1(action) {
        MtgoDuelActionFamilyV1::PriorityPass => {
            vec![vec![K::PromptChanged], vec![K::PhaseBarChanged]]
        }
        MtgoDuelActionFamilyV1::PlayLand => {
            let A::PlayLand { source, .. } = action else {
                unreachable!("action family and variant disagree")
            };
            vec![vec![
                K::BattlefieldChanged,
                direct_visible_source_zone_kind_v1(decision, source)?,
            ]]
        }
        MtgoDuelActionFamilyV1::CastOrPlotSpell => {
            let (source, destination) = match action {
                A::CastSpell { source, .. } => (source, K::StackChanged),
                A::PlotSpell { source, .. } => (source, K::ExileChanged),
                _ => unreachable!("action family and variant disagree"),
            };
            let source_kind = direct_visible_source_zone_kind_v1(decision, source)?;
            if source_kind == destination {
                vec![vec![source_kind]]
            } else {
                vec![vec![source_kind, destination]]
            }
        }
        MtgoDuelActionFamilyV1::ManaAbility => vec![
            vec![K::ManaPoolChanged, K::SelectedControlChanged],
            vec![K::ManaPoolChanged, K::BattlefieldChanged],
            vec![K::ManaPoolChanged, K::PlayerCountsChanged],
        ],
        MtgoDuelActionFamilyV1::NonManaAbility => vec![
            vec![K::StackChanged],
            vec![K::ChoiceSurfaceChanged],
            vec![K::ManaPoolChanged],
            vec![K::BattlefieldChanged],
        ],
        MtgoDuelActionFamilyV1::TargetChoice
        | MtgoDuelActionFamilyV1::CostOrModeChoice
        | MtgoDuelActionFamilyV1::EffectChoice => vec![
            vec![K::PromptChanged],
            vec![K::ChoiceSurfaceChanged],
            vec![K::StackChanged],
        ],
        MtgoDuelActionFamilyV1::Discard => vec![vec![K::HandChanged]],
        MtgoDuelActionFamilyV1::CombatChoice => vec![vec![K::CombatChanged]],
        MtgoDuelActionFamilyV1::TriggerOrdering => {
            vec![vec![K::StackChanged], vec![K::ChoiceSurfaceChanged]]
        }
    })
}

fn direct_visible_source_zone_kind_v1(
    decision: &mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1,
    source: &mtgo_blackbox_v1::MtgoPlayerVisibleObjectRefV1,
) -> Result<MtgoPlayerVisibleGameplayPostconditionKindV1, String> {
    use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;

    let state = &decision.current_state;
    let mut matches = Vec::new();
    if state.own_hand.iter().any(|card| card.object_ref == *source) {
        matches.push(K::HandChanged);
    }
    if state
        .graveyards
        .iter()
        .flatten()
        .any(|card| card.object_ref == *source)
    {
        matches.push(K::GraveyardChanged);
    }
    if state
        .known_library_cards
        .iter()
        .flatten()
        .any(|card| card.card.object_ref == *source)
    {
        matches.push(K::LibraryChanged);
    }
    if state.exile.iter().any(|card| card.object_ref == *source) {
        matches.push(K::ExileChanged);
    }
    match matches.as_slice() {
        [kind] => Ok(*kind),
        [] => Err("selected land or spell source has no visible source zone".to_owned()),
        _ => Err("selected land or spell source appears in multiple visible zones".to_owned()),
    }
}

fn direct_visible_rects_intersect_v1(
    left: (u32, u32, u32, u32),
    right: (u32, u32, u32, u32),
) -> bool {
    let Some(left_right) = left.0.checked_add(left.2) else {
        return true;
    };
    let Some(left_bottom) = left.1.checked_add(left.3) else {
        return true;
    };
    let Some(right_right) = right.0.checked_add(right.2) else {
        return true;
    };
    let Some(right_bottom) = right.1.checked_add(right.3) else {
        return true;
    };
    left.0 < right_right && right.0 < left_right && left.1 < right_bottom && right.1 < left_bottom
}

fn validate_requested_postcondition_categories_v1(
    specs: &[MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1],
) -> Result<(), String> {
    let mut requested_kinds = HashSet::new();
    if specs.is_empty()
        || specs.len() > 16
        || specs.iter().any(|spec| !requested_kinds.insert(spec.kind))
    {
        return Err(
            "direct-source before-dispatch categories must be nonempty and unique".to_owned(),
        );
    }
    Ok(())
}

fn current_frame_region_v1(
    evidence: &[mtgo_blackbox_v1::MtgoVisibleEvidenceV1],
    frame_id: u64,
    evidence_id: u64,
) -> Result<MtgoRectPxV1, String> {
    let source = evidence
        .iter()
        .find(|candidate| candidate.evidence_id == evidence_id)
        .ok_or("direct before-dispatch classifier evidence is absent")?;
    let MtgoEvidenceSourceV1::FrameRegion {
        frame_id: source_frame_id,
        rect,
        ..
    } = &source.source
    else {
        return Err("direct before-dispatch classifier evidence is not a pixel region".to_owned());
    };
    if *source_frame_id != frame_id {
        return Err("direct before-dispatch classifier evidence is stale".to_owned());
    }
    Ok(rect.clone())
}

fn bounding_visible_rect_v1<'a>(
    mut rects: impl Iterator<Item = &'a MtgoRectPxV1>,
) -> Option<MtgoRectPxV1> {
    let first = rects.next()?;
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x.checked_add(first.width)?;
    let mut bottom = first.y.checked_add(first.height)?;
    for rect in rects {
        left = left.min(rect.x);
        top = top.min(rect.y);
        right = right.max(rect.x.checked_add(rect.width)?);
        bottom = bottom.max(rect.y.checked_add(rect.height)?);
    }
    Some(MtgoRectPxV1 {
        x: left,
        y: top,
        width: right.checked_sub(left)?,
        height: bottom.checked_sub(top)?,
    })
}

fn evidence_reaches_frame_region_v1(
    evidence: &[mtgo_blackbox_v1::MtgoVisibleEvidenceV1],
    current_evidence_id: u64,
    target_frame_region_evidence_id: u64,
    visited: &mut HashSet<u64>,
) -> bool {
    if !visited.insert(current_evidence_id) {
        return false;
    }
    if current_evidence_id == target_frame_region_evidence_id {
        return true;
    }
    let Some(current) = evidence
        .iter()
        .find(|candidate| candidate.evidence_id == current_evidence_id)
    else {
        return false;
    };
    match &current.source {
        MtgoEvidenceSourceV1::VisibleGameLogText {
            frame_region_evidence_id,
            ..
        }
        | MtgoEvidenceSourceV1::VisibleAccessibilityText {
            frame_region_evidence_id,
            ..
        }
        | MtgoEvidenceSourceV1::ManualVisibleAnnotation {
            frame_region_evidence_id,
            ..
        } => evidence_reaches_frame_region_v1(
            evidence,
            *frame_region_evidence_id,
            target_frame_region_evidence_id,
            visited,
        ),
        MtgoEvidenceSourceV1::DerivedPublicFact {
            parent_evidence_ids,
            ..
        } => parent_evidence_ids.iter().any(|parent| {
            let mut branch_visited = visited.clone();
            evidence_reaches_frame_region_v1(
                evidence,
                *parent,
                target_frame_region_evidence_id,
                &mut branch_visited,
            )
        }),
        MtgoEvidenceSourceV1::FrameRegion { .. } => false,
    }
}

fn visible_pointer_supports_postcondition_kind_v1(
    pointer: &str,
    kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
) -> bool {
    use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;
    match kind {
        K::SelectedControlChanged | K::PromptChanged => false,
        K::PhaseBarChanged => {
            pointer.contains("/phase")
                || pointer.contains("/active_player")
                || pointer.contains("/priority_player")
        }
        K::PlayerCountsChanged => {
            pointer.contains("/life_totals")
                || pointer.contains("/hand_counts")
                || pointer.contains("/library_counts")
        }
        K::BattlefieldChanged => pointer.contains("/battlefield"),
        K::HandChanged => {
            pointer.contains("/own_hand")
                || pointer.contains("/known_hand_cards")
                || pointer.contains("/hand_counts")
        }
        K::GraveyardChanged => pointer.contains("/graveyards"),
        K::LibraryChanged => {
            pointer.contains("/library_counts") || pointer.contains("/known_library_cards")
        }
        K::ExileChanged => pointer.contains("/exile"),
        K::ManaPoolChanged => pointer.contains("/mana_pools"),
        K::StackChanged => pointer.contains("/stack"),
        K::CombatChanged => pointer.contains("/combat"),
        K::ChoiceSurfaceChanged => pointer.starts_with("/legal_actions/"),
    }
}

fn exact_equivalent_visible_regions_commitment_v1(
    before: &OpaqueMtgoAdmittedDuelVisibleFrameV1,
    after: &OpaqueMtgoAdmittedDuelPerceptionV1,
) -> Result<String, String> {
    let after_frame_id = after.validated_decision.frame_id();
    let mut rectangles = BTreeMap::<(u32, u32, u32, u32), MtgoRectPxV1>::new();
    for evidence in &after.decision_record.evidence {
        if let MtgoEvidenceSourceV1::FrameRegion { frame_id, rect, .. } = &evidence.source {
            if *frame_id == after_frame_id {
                rectangles.insert((rect.x, rect.y, rect.width, rect.height), rect.clone());
            }
        }
    }
    if let Some(lifecycle) = after.competitive_lifecycle.as_ref() {
        for fact in lifecycle.visible_facts_v1() {
            let rect = &fact.rect_client_px;
            rectangles.insert((rect.x, rect.y, rect.width, rect.height), rect.clone());
        }
    }
    if rectangles.is_empty() {
        return Err("corroborating visible decision has no current-frame pixel regions".to_owned());
    }
    let before_source = &before.source_frame;
    let after_source = &after.source_frame.source_frame;
    let before_size = MtgoSizePxV1 {
        width: before_source.manifest.frame.canonical_width,
        height: before_source.manifest.frame.canonical_height,
    };
    let after_size = MtgoSizePxV1 {
        width: after_source.manifest.frame.canonical_width,
        height: after_source.manifest.frame.canonical_height,
    };
    if before_size != after_size {
        return Err("direct-source corroboration changed visible client size".to_owned());
    }
    let mut hasher = Sha256::new();
    hasher.update(DIRECT_VISIBLE_SOURCE_EQUIVALENT_REGIONS_DOMAIN_V1);
    hasher.update((rectangles.len() as u64).to_be_bytes());
    for rect in rectangles.values() {
        let before_sha256 = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
            &before_source.canonical_bgra8,
            &before_size,
            rect,
        )
        .map_err(|error| format!("rehash direct corroboration before region: {error}"))?;
        let after_sha256 = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
            &after_source.canonical_bgra8,
            &after_size,
            rect,
        )
        .map_err(|error| format!("rehash direct corroboration after region: {error}"))?;
        if before_sha256 != after_sha256 {
            return Err(
                "decision-relevant visible pixels changed during direct-source corroboration"
                    .to_owned(),
            );
        }
        hasher.update(rect.x.to_be_bytes());
        hasher.update(rect.y.to_be_bytes());
        hasher.update(rect.width.to_be_bytes());
        hasher.update(rect.height.to_be_bytes());
        hasher.update(before_sha256.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

impl OpaqueMtgoAttestedDirectVisibleSourceObservationV1 {
    pub fn commitments_v1(&self) -> MtgoAttestedDirectVisibleSourceObservationCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn abstention_reason_v1(&self) -> Option<MtgoVisibleDuelViewModelBrokerAbstentionReasonV1> {
        match self.result {
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained { reason } => Some(reason),
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { .. } => None,
        }
    }

    pub(crate) fn before_captured_at_unix_millis_v1(&self) -> u128 {
        self._before_frame
            .commitments_v1()
            .source_capture
            .captured_at_unix_millis
    }

    pub(crate) fn after_captured_at_unix_millis_v1(&self) -> u128 {
        self._after_frame
            .commitments_v1()
            .source_capture
            .captured_at_unix_millis
    }

    pub(crate) fn combat_scoring_required_v1(&self) -> bool {
        result_requires_combat_scoring_v1(&self.result)
    }

    pub fn producer_execution_attested_v1(&self) -> bool {
        true
    }

    pub fn safe_for_live_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

fn result_requires_combat_scoring_v1(result: &MtgoVisibleDuelViewModelBrokerResultV1) -> bool {
    matches!(
        result,
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { .. }
    )
}

/// One release-pinned direct-source execution bracketed by two live composed
/// duel captures. It can qualify producer behavior before a duel-perception
/// profile is ratified, but can never become semantic evidence, reach a model,
/// send input, enter an event, or spend account resources.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoQualifiedDirectVisibleSourceObservationV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoQualifiedDirectVisibleSourceObservationV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoQualifiedDirectVisibleSourceObservationV1;
/// fn cannot_extract_or_act(value: OpaqueMtgoQualifiedDirectVisibleSourceObservationV1) {
///     let _ = value.visible_decision();
///     let _ = value.raw_client_object();
///     value.dispatch();
/// }
/// ```
pub struct OpaqueMtgoQualifiedDirectVisibleSourceObservationV1 {
    _before_frame: OpaqueMtgoDxgiFrameCandidateV3,
    _after_frame: OpaqueMtgoDxgiFrameCandidateV3,
    result: MtgoVisibleDuelViewModelBrokerResultV1,
    commitments: MtgoQualifiedDirectVisibleSourceObservationCommitmentsV1,
}

/// Public commitments for one release-pinned producer result that was observed
/// identically across two invocations of the same MTGO process incarnation.
/// Process identifiers and executable paths stay private. No pixel capture is
/// required because the producer is limited to the reviewed rendered
/// presentation surface and the strict outward visible-equivalent schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoStableBackgroundDirectVisibleSourceCommitmentsV1 {
    pub runtime_identity_commitment_sha256: String,
    pub broker_binary_sha256: String,
    pub producer_binary_sha256: String,
    pub sanitized_result_sha256: String,
    pub stability_commitment_sha256: String,
}

/// Commitments for a newly written manual-review artifact containing one exact
/// strict visible-equivalent producer result. The artifact is review input
/// only. Every authority flag stays false, including after a reviewer edits a
/// separate copy of the template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MtgoStableBackgroundDirectVisibleReviewArtifactReceiptV1 {
    pub schema: &'static str,
    pub status: &'static str,
    pub output_directory: PathBuf,
    pub result_kind: String,
    pub runtime_identity_commitment_sha256: String,
    pub sanitized_result_sha256: String,
    pub stability_commitment_sha256: String,
    pub manifest_sha256: String,
    pub review_template_sha256: String,
    pub artifact_commitment_sha256: String,
    pub review_completed: bool,
    pub safe_for_live_semantic_evidence: bool,
    pub safe_for_model_scoring: bool,
    pub safe_for_input: bool,
    pub permits_event_entry: bool,
    pub permits_spending: bool,
}

#[derive(Serialize)]
struct PrivateMtgoStableBackgroundDirectVisibleReviewManifestV1<'a> {
    schema: &'static str,
    artifact_kind: &'static str,
    status: &'static str,
    information_boundary: &'static str,
    result_kind: &'a str,
    visible_result_file: &'static str,
    visible_result_byte_length: usize,
    visible_result_sha256: &'a str,
    runtime_identity_commitment_sha256: &'a str,
    broker_binary_sha256: &'a str,
    producer_binary_sha256: &'a str,
    stability_commitment_sha256: &'a str,
    review_template_file: &'static str,
    review_completed: bool,
    safe_for_live_semantic_evidence: bool,
    safe_for_model_scoring: bool,
    safe_for_input: bool,
    permits_event_entry: bool,
    permits_spending: bool,
}

#[derive(Serialize)]
struct PrivateMtgoStableBackgroundDirectVisibleReviewTemplateV1<'a> {
    schema: &'static str,
    artifact_status_required: &'static str,
    result_kind: &'a str,
    visible_result_sha256: &'a str,
    stability_commitment_sha256: &'a str,
    reviewer_alias: &'static str,
    every_exported_fact_visible_in_rendered_ui_or_rendered_game_log: bool,
    legal_action_set_matches_visible_controls: bool,
    ordinary_surface_complete: bool,
    combat_surface_complete: bool,
    no_hidden_zone_or_internal_identifier: bool,
    review_completed: bool,
}

/// Move-only background-safe qualification of the direct visible source.
///
/// The exact sanitized result is retained privately and has no extraction,
/// model-scoring, dispatch, event-entry, or spending conversion. This type
/// proves only that two consecutive observe-only producer invocations returned
/// the same strict visible-equivalent result from one pinned process
/// incarnation while the release artifacts remained unchanged.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoStableBackgroundDirectVisibleSourceV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoStableBackgroundDirectVisibleSourceV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoStableBackgroundDirectVisibleSourceV1;
/// fn cannot_extract_or_act(value: OpaqueMtgoStableBackgroundDirectVisibleSourceV1) {
///     let _ = value.visible_decision();
///     let _ = value.process_id();
///     value.dispatch();
/// }
/// ```
pub struct OpaqueMtgoStableBackgroundDirectVisibleSourceV1 {
    _exact_result_bytes: ZeroingVecV1,
    result: MtgoVisibleDuelViewModelBrokerResultV1,
    commitments: MtgoStableBackgroundDirectVisibleSourceCommitmentsV1,
}

impl OpaqueMtgoStableBackgroundDirectVisibleSourceV1 {
    pub fn commitments_v1(&self) -> MtgoStableBackgroundDirectVisibleSourceCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn abstention_reason_v1(&self) -> Option<MtgoVisibleDuelViewModelBrokerAbstentionReasonV1> {
        match self.result {
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained { reason } => Some(reason),
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { .. } => None,
        }
    }

    pub fn producer_execution_attested_v1(&self) -> bool {
        true
    }

    pub fn sanitized_visible_projection_stable_v1(&self) -> bool {
        true
    }

    pub fn requires_foreground_window_v1(&self) -> bool {
        false
    }

    pub fn requires_pixel_capture_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Qualification-only ordinary selection retaining the exact stable
/// background source. This remains crate-private because its scorer is
/// caller-supplied and cannot be a production policy owner.
pub(crate) struct OpaqueMtgoRatifiedStableBackgroundDirectVisibleScoringOutcomeV1 {
    observation: OpaqueMtgoStableBackgroundDirectVisibleSourceV1,
    outcome: CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1,
}

/// Qualification-only combat plan retaining the exact stable background
/// source. A fresh foreground source-attested observation is mandatory before
/// the plan can enter the existing combat dispatch preparation chain.
pub(crate) struct OpaqueMtgoRatifiedStableBackgroundDirectVisibleCombatScoringOutcomeV1 {
    observation: OpaqueMtgoStableBackgroundDirectVisibleSourceV1,
    outcome: CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1,
}

impl OpaqueMtgoQualifiedDirectVisibleSourceObservationV1 {
    pub fn commitments_v1(&self) -> MtgoQualifiedDirectVisibleSourceObservationCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn abstention_reason_v1(&self) -> Option<MtgoVisibleDuelViewModelBrokerAbstentionReasonV1> {
        match self.result {
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained { reason } => Some(reason),
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { .. }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
                ..
            }
            | MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { .. } => None,
        }
    }

    pub fn producer_execution_attested_v1(&self) -> bool {
        true
    }

    pub fn safe_for_live_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Verifies the exact release artifacts recorded by the producer v1.5 lobby
/// qualification. The hashes are a compile-time trust root rather than caller
/// declarations. This function does not inspect or control MTGO.
pub fn verify_direct_visible_source_runtime_v1(
    broker_path: &Path,
    bootstrap_path: &Path,
    producer_path: &Path,
    validator_path: &Path,
) -> Result<OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1, String> {
    let broker_path = verify_exact_artifact_v1(
        broker_path,
        "mtgo_visible_duel_live_broker_v1.exe",
        LIVE_BROKER_SHA256_V1,
        "live direct-source broker",
    )?;
    let bootstrap_path = verify_exact_artifact_v1(
        bootstrap_path,
        "mtgo_visible_duel_bootstrap_v1.dll",
        LIVE_BOOTSTRAP_SHA256_V1,
        "live direct-source bootstrap",
    )?;
    let producer_path = verify_exact_artifact_v1(
        producer_path,
        "mtgo_visible_duel_producer_v1.dll",
        LIVE_PRODUCER_SHA256_V1,
        "live direct-source producer",
    )?;
    let validator_path = verify_exact_artifact_v1(
        validator_path,
        "check_mtgo_visible_duel_producer_result_v1.exe",
        LIVE_VALIDATOR_SHA256_V1,
        "live direct-source strict validator",
    )?;
    let runtime_identity_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_SOURCE_RUNTIME_DOMAIN_V1,
        &[
            LIVE_BROKER_SHA256_V1.as_bytes(),
            LIVE_BOOTSTRAP_SHA256_V1.as_bytes(),
            LIVE_PRODUCER_SHA256_V1.as_bytes(),
            LIVE_VALIDATOR_SHA256_V1.as_bytes(),
            b"release_pinned_observe_only_live_dispatch_compile_disabled",
        ],
    );
    Ok(OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1 {
        broker_path,
        bootstrap_path,
        producer_path,
        validator_path,
        commitments: MtgoVerifiedDirectVisibleSourceRuntimeCommitmentsV1 {
            runtime_identity_commitment_sha256,
            broker_binary_sha256: LIVE_BROKER_SHA256_V1.to_owned(),
            bootstrap_binary_sha256: LIVE_BOOTSTRAP_SHA256_V1.to_owned(),
            producer_binary_sha256: LIVE_PRODUCER_SHA256_V1.to_owned(),
            strict_validator_binary_sha256: LIVE_VALIDATOR_SHA256_V1.to_owned(),
        },
    })
}

/// Verifies the separately named dispatch-capable broker and the same exact
/// bootstrap, producer, and strict validator used by the qualified observer.
/// Verification is read-only and cannot satisfy the empty production
/// ratification root.
pub fn verify_direct_visible_dispatch_runtime_v1(
    broker_path: &Path,
    bootstrap_path: &Path,
    producer_path: &Path,
    validator_path: &Path,
) -> Result<OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1, String> {
    let broker_path = verify_exact_artifact_v1(
        broker_path,
        "mtgo_visible_duel_live_dispatch_broker_v1.exe",
        LIVE_DISPATCH_BROKER_SHA256_V1,
        "live direct-visible dispatch broker",
    )?;
    let bootstrap_path = verify_exact_artifact_v1(
        bootstrap_path,
        "mtgo_visible_duel_bootstrap_v1.dll",
        LIVE_BOOTSTRAP_SHA256_V1,
        "live direct-visible dispatch bootstrap",
    )?;
    let producer_path = verify_exact_artifact_v1(
        producer_path,
        "mtgo_visible_duel_producer_v1.dll",
        LIVE_PRODUCER_SHA256_V1,
        "live direct-visible dispatch producer",
    )?;
    let validator_path = verify_exact_artifact_v1(
        validator_path,
        "check_mtgo_visible_duel_producer_result_v1.exe",
        LIVE_VALIDATOR_SHA256_V1,
        "live direct-visible dispatch validator",
    )?;
    let dispatch_runtime_identity_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_DISPATCH_RUNTIME_DOMAIN_V1,
        &[
            LIVE_DISPATCH_BROKER_SHA256_V1.as_bytes(),
            LIVE_BOOTSTRAP_SHA256_V1.as_bytes(),
            LIVE_PRODUCER_SHA256_V1.as_bytes(),
            LIVE_VALIDATOR_SHA256_V1.as_bytes(),
            b"separate_dispatch_binary_no_authority_without_compiled_ratification",
        ],
    );
    Ok(OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1 {
        broker_path,
        bootstrap_path,
        producer_path,
        validator_path,
        commitments: MtgoVerifiedDirectVisibleDispatchRuntimeCommitmentsV1 {
            dispatch_runtime_identity_commitment_sha256,
            dispatch_broker_binary_sha256: LIVE_DISPATCH_BROKER_SHA256_V1.to_owned(),
            bootstrap_binary_sha256: LIVE_BOOTSTRAP_SHA256_V1.to_owned(),
            producer_binary_sha256: LIVE_PRODUCER_SHA256_V1.to_owned(),
            strict_validator_binary_sha256: LIVE_VALIDATOR_SHA256_V1.to_owned(),
        },
    })
}

pub(crate) fn execute_attested_direct_visible_selection_v1(
    before: OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1,
    runtime: &OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1,
    reviewed_dispatch_runtime_commitment_sha256: &str,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoPendingAttestedDirectVisibleDispatchV1, String> {
    if !(100..=30_000).contains(&broker_timeout_ms) {
        return Err("direct-visible dispatch timeout is outside the supported range".to_owned());
    }
    require_ratified_direct_visible_dispatch_runtime_v1(
        reviewed_dispatch_runtime_commitment_sha256,
    )?;
    if reviewed_dispatch_runtime_commitment_sha256
        != runtime
            .commitments
            .dispatch_runtime_identity_commitment_sha256
    {
        return Err(
            "the ratified direct-visible dispatch commitment and verified runtime differ"
                .to_owned(),
        );
    }
    verify_dispatch_runtime_identity_now_v1(runtime)?;
    let OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1 {
        _initial_observation,
        _refreshed_observation,
        _corroborating_perception: before_perception,
        checked,
        commitments: _,
    } = before;
    let (
        decision_sha256,
        selected_index,
        before_dispatch_commitment_sha256,
        source_captured_at_unix_millis,
    ) = {
        let dispatch = checked.dispatch_commitments_v1();
        if dispatch.producer_binary_sha256_v1() != runtime.commitments.producer_binary_sha256 {
            return Err("direct observer and dispatch producer identities differ".to_owned());
        }
        (
            dispatch.exact_producer_result_sha256_v1().to_owned(),
            dispatch.selected_index_v1(),
            dispatch.before_dispatch_commitment_sha256_v1().to_owned(),
            dispatch.source_captured_at_unix_millis_v1(),
        )
    };
    let now = unix_millis_now_v1()?;
    require_fresh_source_v1(source_captured_at_unix_millis, now)?;
    let process_id = before_perception
        .source_frame
        .source_frame
        .manifest
        .pre
        .process_id;
    if process_id == 0 {
        return Err("direct-visible dispatch source has no MTGO process identity".to_owned());
    }
    reserve_direct_visible_input_gate_v1()?;
    let attempt_started_at = match unix_millis_now_v1() {
        Ok(value) => value,
        Err(error) => {
            release_unattempted_direct_visible_input_gate_v1()?;
            return Err(error);
        }
    };
    if let Err(error) = require_fresh_source_v1(
        checked.source_captured_at_unix_millis_v1(),
        attempt_started_at,
    ) {
        release_unattempted_direct_visible_input_gate_v1()?;
        return Err(error);
    }
    halt_before_direct_visible_input_attempt_v1()?;
    let receipt = invoke_dispatch_broker_v1(
        runtime,
        process_id,
        &decision_sha256,
        selected_index,
        Duration::from_millis(u64::from(broker_timeout_ms)),
    )?;
    verify_dispatch_runtime_identity_now_v1(runtime)?;
    const SUBMITTED_RECEIPT_V1: &[u8] =
        b"{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}";
    if receipt.0 != SUBMITTED_RECEIPT_V1 {
        return Err(
            "the sealed direct-visible producer did not submit the selected action".to_owned(),
        );
    }
    let dispatch_submitted_at_unix_millis = unix_millis_now_v1()?;
    if dispatch_submitted_at_unix_millis < attempt_started_at {
        return Err("system clock moved backwards during direct-visible dispatch".to_owned());
    }
    let dispatch_receipt_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_DISPATCH_RECEIPT_DOMAIN_V1,
        &[
            runtime
                .commitments
                .dispatch_runtime_identity_commitment_sha256
                .as_bytes(),
            before_dispatch_commitment_sha256.as_bytes(),
            decision_sha256.as_bytes(),
            selected_index.to_be_bytes().as_slice(),
            &attempt_started_at.to_be_bytes(),
            &dispatch_submitted_at_unix_millis.to_be_bytes(),
            &receipt.0,
            b"sealed_exact_visible_decision_submitted_once_pending_newer_visible_result",
        ],
    );
    set_direct_visible_input_pending_v1(&dispatch_receipt_commitment_sha256)?;
    Ok(OpaqueMtgoPendingAttestedDirectVisibleDispatchV1 {
        before_perception,
        before: checked,
        dispatch_receipt_commitment_sha256,
        dispatch_submitted_at_unix_millis,
    })
}

pub(crate) fn execute_attested_direct_visible_combat_step_v1(
    before: OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1,
    runtime: &OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1,
    reviewed_combat_dispatch_runtime_commitment_sha256: &str,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoPendingAttestedDirectVisibleCombatDispatchV1, String> {
    if !(100..=30_000).contains(&broker_timeout_ms) {
        return Err(
            "direct-visible combat dispatch timeout is outside the supported range".to_owned(),
        );
    }
    require_ratified_direct_visible_combat_dispatch_runtime_v1(
        reviewed_combat_dispatch_runtime_commitment_sha256,
    )?;
    if reviewed_combat_dispatch_runtime_commitment_sha256
        != runtime
            .commitments
            .dispatch_runtime_identity_commitment_sha256
    {
        return Err(
            "the ratified combat dispatch commitment and verified runtime differ".to_owned(),
        );
    }
    verify_dispatch_runtime_identity_now_v1(runtime)?;
    let OpaqueMtgoAttestedDirectVisibleCombatBeforeDispatchV1 {
        source_observation,
        checked,
    } = before;
    if source_observation.commitments.producer_binary_sha256
        != runtime.commitments.producer_binary_sha256
    {
        return Err("combat observer and dispatch producer identities differ".to_owned());
    }
    let source_captured_at_unix_millis = source_observation.after_captured_at_unix_millis_v1();
    let now = unix_millis_now_v1()?;
    require_fresh_source_v1(source_captured_at_unix_millis, now)?;
    let process_id = source_observation
        ._after_frame
        .source_frame
        .manifest
        .pre
        .process_id;
    if process_id == 0 {
        return Err("direct-visible combat source has no MTGO process identity".to_owned());
    }
    reserve_direct_visible_input_gate_v1()?;
    let attempt_started_at = match unix_millis_now_v1() {
        Ok(value) => value,
        Err(error) => {
            release_unattempted_direct_visible_input_gate_v1()?;
            return Err(error);
        }
    };
    if let Err(error) = require_fresh_source_v1(source_captured_at_unix_millis, attempt_started_at)
    {
        release_unattempted_direct_visible_input_gate_v1()?;
        return Err(error);
    }
    halt_before_direct_visible_input_attempt_v1()?;
    let receipt = invoke_combat_dispatch_broker_v1(
        runtime,
        process_id,
        checked.command_v1(),
        Duration::from_millis(u64::from(broker_timeout_ms)),
    )?;
    verify_dispatch_runtime_identity_now_v1(runtime)?;
    const SUBMITTED_RECEIPT_V1: &[u8] =
        b"{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}";
    if receipt.0 != SUBMITTED_RECEIPT_V1 {
        return Err("the sealed producer did not submit the combat operation".to_owned());
    }
    let dispatch_submitted_at_unix_millis = unix_millis_now_v1()?;
    if dispatch_submitted_at_unix_millis < attempt_started_at {
        return Err("system clock moved backwards during combat dispatch".to_owned());
    }
    let dispatch_receipt_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_COMBAT_DISPATCH_RECEIPT_DOMAIN_V1,
        &[
            runtime
                .commitments
                .dispatch_runtime_identity_commitment_sha256
                .as_bytes(),
            source_observation
                .commitments
                .observation_commitment_sha256
                .as_bytes(),
            checked.execution_step_commitment_sha256_v1().as_bytes(),
            &attempt_started_at.to_be_bytes(),
            &dispatch_submitted_at_unix_millis.to_be_bytes(),
            &receipt.0,
            b"one_combat_operation_submitted_pending_exact_newer_visible_transition",
        ],
    );
    set_direct_visible_input_pending_v1(&dispatch_receipt_commitment_sha256)?;
    Ok(OpaqueMtgoPendingAttestedDirectVisibleCombatDispatchV1 {
        source_observation,
        checked,
        dispatch_receipt_commitment_sha256,
        dispatch_submitted_at_unix_millis,
    })
}

pub(crate) fn confirm_attested_direct_visible_combat_dispatch_v1(
    pending: OpaqueMtgoPendingAttestedDirectVisibleCombatDispatchV1,
    fresh_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    not_before_unix_millis: u128,
) -> Result<OpaqueMtgoConfirmedAttestedDirectVisibleCombatTransitionV1, String> {
    require_matching_direct_visible_input_pending_v1(&pending.dispatch_receipt_commitment_sha256)?;
    if fresh_observation.before_captured_at_unix_millis_v1()
        <= pending
            .dispatch_submitted_at_unix_millis
            .max(not_before_unix_millis)
    {
        return Err(
            "combat confirmation observation is not strictly newer than dispatch".to_owned(),
        );
    }
    validate_same_duel_observation_lineage_v1(
        &pending.source_observation._after_frame,
        &fresh_observation._before_frame,
    )?;
    if pending
        .source_observation
        .commitments
        .runtime_identity_commitment_sha256
        != fresh_observation
            .commitments
            .runtime_identity_commitment_sha256
        || pending.source_observation.commitments.broker_binary_sha256
            != fresh_observation.commitments.broker_binary_sha256
        || pending
            .source_observation
            .commitments
            .producer_binary_sha256
            != fresh_observation.commitments.producer_binary_sha256
    {
        return Err("combat confirmation changed the visible-source runtime".to_owned());
    }
    let transition = confirm_player_visible_combat_execution_transition_v1(
        pending.checked,
        &fresh_observation.exact_result_bytes.0,
    )
    .map_err(|error| format!("confirm source-attested combat transition: {error}"))?;
    Ok(OpaqueMtgoConfirmedAttestedDirectVisibleCombatTransitionV1 {
        fresh_observation,
        transition,
        pending_receipt_commitment_sha256: pending.dispatch_receipt_commitment_sha256,
    })
}

pub(crate) fn confirm_attested_direct_visible_dispatch_v1(
    pending: OpaqueMtgoPendingAttestedDirectVisibleDispatchV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    capture_timeout_ms: u32,
    not_before_unix_millis: u128,
) -> Result<CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1, String> {
    if !(100..=10_000).contains(&capture_timeout_ms) {
        return Err(
            "direct-visible confirmation timeout is outside the supported range".to_owned(),
        );
    }
    require_matching_direct_visible_input_pending_v1(&pending.dispatch_receipt_commitment_sha256)?;
    let before_sequence = pending
        .before
        .dispatch_commitments_v1()
        .source_frame_sequence_v1();
    let after_sequence = before_sequence
        .checked_add(1)
        .ok_or("direct-visible after-frame sequence overflow")?;
    let deadline = Instant::now() + Duration::from_millis(u64::from(capture_timeout_ms));
    let after = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(100) {
            return Err(
                "direct-visible confirmation timed out before every fixed visible region changed"
                    .to_owned(),
            );
        }
        let candidate_timeout_ms = remaining.as_millis().min(1_000) as u32;
        let after_frame =
            capture_admitted_mtgo_duel_visible_frame_v1(profile, candidate_timeout_ms)?;
        validate_same_duel_observation_lineage_v1(
            &pending.before_perception.source_frame,
            &after_frame,
        )?;
        let after_commitments = after_frame.commitments_v1();
        if after_commitments.source_capture.captured_at_unix_millis
            <= pending
                .dispatch_submitted_at_unix_millis
                .max(not_before_unix_millis)
        {
            continue;
        }
        let size = MtgoSizePxV1 {
            width: after_commitments.source_capture.canonical_width,
            height: after_commitments.source_capture.canonical_height,
        };
        let mut region_hashes = Vec::new();
        for rect in pending.before.fixed_region_rects_v1() {
            region_hashes.push(
                mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
                    &after_frame.source_frame.canonical_bgra8,
                    &size,
                    &rect,
                )
                .map_err(|error| format!("rehash direct-visible after region: {error}"))?,
            );
        }
        if !pending
            .before
            .candidate_changes_every_fixed_region_v1(&region_hashes)
            .map_err(|error| format!("check direct-visible after regions: {error}"))?
        {
            continue;
        }
        let after_frame_id = frame_id_from_capture_commitment_v1(
            &after_commitments.source_capture.capture_commitment_sha256,
            pending.before_perception.validated_decision.frame_id(),
        )?;
        break pending
            .before
            .after_frame_record_v1(
                after_frame_id,
                after_sequence,
                after_commitments.source_capture.captured_at_unix_millis,
                after_commitments.source_capture.canonical_bgra8_sha256,
                region_hashes,
            )
            .map_err(|error| format!("bind direct-visible after frame: {error}"))?;
    };
    complete_direct_visible_gameplay_postcondition_v1(
        pending.before,
        &pending.dispatch_receipt_commitment_sha256,
        pending.dispatch_submitted_at_unix_millis,
        after,
        None,
    )
    .map_err(|error| format!("confirm direct-visible gameplay result: {error}"))
}

/// Invokes only the release-pinned observer against the same client process as
/// a fresh admitted acting-player duel frame, then immediately captures a
/// second frame. The broker output is bounded, parsed as one strict sanitized
/// visible result, and erased after parsing. Every artifact is rehashed after
/// invocation. The live broker receives no decision hash or selected index.
pub fn observe_attested_direct_visible_source_v1(
    before_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    capture_timeout_ms: u32,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoAttestedDirectVisibleSourceObservationV1, String> {
    if !(100..=10_000).contains(&capture_timeout_ms) || !(100..=30_000).contains(&broker_timeout_ms)
    {
        return Err(
            "direct-source capture or broker timeout is outside the supported range".to_owned(),
        );
    }
    verify_runtime_identity_now_v1(runtime)?;
    let before_commitments = before_frame.commitments_v1();
    if before_commitments.perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || before_commitments.perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("direct-source frame and admitted duel profile differ".to_owned());
    }
    require_fresh_source_v1(
        before_commitments.source_capture.captured_at_unix_millis,
        unix_millis_now_v1()?,
    )?;
    let process_id = before_frame.source_frame.manifest.pre.process_id;
    if process_id == 0 {
        return Err("direct-source frame has no MTGO process identity".to_owned());
    }

    let invocation = invoke_observe_only_broker_v1(
        runtime,
        process_id,
        Duration::from_millis(u64::from(broker_timeout_ms)),
    );
    let after_frame_result =
        capture_admitted_mtgo_duel_visible_frame_v1(profile, capture_timeout_ms);
    let runtime_recheck = verify_runtime_identity_now_v1(runtime);
    let after_frame = after_frame_result;
    let output = invocation;
    runtime_recheck?;
    let after_frame = after_frame?;
    let output = output?;
    validate_same_duel_observation_lineage_v1(&before_frame, &after_frame)?;
    let result = parse_and_validate_visible_duel_producer_result_v1(&output.0)
        .map_err(|_| "direct-source broker did not return one sanitized visible result".to_owned());
    let result = result?;
    let sanitized_result_sha256 = sha256_hex_v1(&output.0);
    let after_commitments = after_frame.commitments_v1();
    let observation_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_SOURCE_OBSERVATION_DOMAIN_V1,
        &[
            runtime
                .commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            before_commitments.frame_profile_binding_sha256.as_bytes(),
            before_commitments
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            after_commitments.frame_profile_binding_sha256.as_bytes(),
            after_commitments
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            sanitized_result_sha256.as_bytes(),
            b"attested_execution_checked_untrusted_visible_candidate_no_scoring_or_input",
        ],
    );
    Ok(OpaqueMtgoAttestedDirectVisibleSourceObservationV1 {
        _before_frame: before_frame,
        _after_frame: after_frame,
        exact_result_bytes: output,
        result,
        commitments: MtgoAttestedDirectVisibleSourceObservationCommitmentsV1 {
            runtime_identity_commitment_sha256: runtime
                .commitments
                .runtime_identity_commitment_sha256
                .clone(),
            broker_binary_sha256: runtime.commitments.broker_binary_sha256.clone(),
            producer_binary_sha256: runtime.commitments.producer_binary_sha256.clone(),
            before_frame_profile_binding_sha256: before_commitments.frame_profile_binding_sha256,
            before_capture_commitment_sha256: before_commitments
                .source_capture
                .capture_commitment_sha256,
            after_frame_profile_binding_sha256: after_commitments.frame_profile_binding_sha256,
            after_capture_commitment_sha256: after_commitments
                .source_capture
                .capture_commitment_sha256,
            sanitized_result_sha256,
            observation_commitment_sha256,
        },
    })
}

/// Performs one in-memory, no-input qualification of the release-pinned
/// direct observer in the currently foreground acting-player duel. Both DXGI
/// frames are captured internally against the pinned MTGO binary and signer,
/// so a caller cannot substitute a replayed frame or process identifier. The
/// result is permanently qualification-only even if the producer returns a
/// data-bearing sanitized visible decision.
pub fn qualify_attested_direct_visible_source_current_duel_v1(
    expected_game_format: &str,
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    capture_timeout_ms: u32,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoQualifiedDirectVisibleSourceObservationV1, String> {
    if !(100..=10_000).contains(&capture_timeout_ms) || !(100..=30_000).contains(&broker_timeout_ms)
    {
        return Err(
            "direct-source qualification timeout is outside the supported range".to_owned(),
        );
    }
    verify_runtime_identity_now_v1(runtime)?;
    let request = MtgoDxgiCaptureRequestV3 {
        expected_executable_sha256: PINNED_MTGO_EXECUTABLE_SHA256_V1.to_owned(),
        expected_signer_thumbprint: PINNED_MTGO_SIGNER_THUMBPRINT_V1.to_owned(),
        expected_signer_subject_sha256: PINNED_MTGO_SIGNER_SUBJECT_SHA256_V1.to_owned(),
        window_mode: CaptureWindowModeV2::DuelGame,
        expected_game_format: Some(expected_game_format.to_owned()),
        expected_title_contains: None,
        timeout_ms: capture_timeout_ms,
    };
    let before_frame = capture_mtgo_dxgi_frame_candidate_v3(request.clone())?;
    let before_commitments = before_frame.commitments_v3();
    require_fresh_source_v1(
        before_commitments.captured_at_unix_millis,
        unix_millis_now_v1()?,
    )?;
    let process_id = before_frame.manifest.pre.process_id;
    if process_id == 0 {
        return Err("direct-source qualification has no MTGO process identity".to_owned());
    }

    let invocation = invoke_observe_only_broker_v1(
        runtime,
        process_id,
        Duration::from_millis(u64::from(broker_timeout_ms)),
    );
    let after_frame_result = capture_mtgo_dxgi_frame_candidate_v3(request);
    let runtime_recheck = verify_runtime_identity_now_v1(runtime);
    runtime_recheck?;
    let after_frame = after_frame_result?;
    let output = invocation?;
    validate_same_unadmitted_duel_observation_lineage_v1(&before_frame, &after_frame)?;
    let result = parse_and_validate_visible_duel_producer_result_v1(&output.0).map_err(|_| {
        "direct-source broker did not return one sanitized visible result".to_owned()
    })?;
    let sanitized_result_sha256 = sha256_hex_v1(&output.0);
    let after_commitments = after_frame.commitments_v3();
    let qualification_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_SOURCE_QUALIFICATION_DOMAIN_V1,
        &[
            runtime
                .commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            before_commitments.capture_commitment_sha256.as_bytes(),
            after_commitments.capture_commitment_sha256.as_bytes(),
            sanitized_result_sha256.as_bytes(),
            b"live_no_stakes_checked_untrusted_no_scoring_no_input",
        ],
    );
    Ok(OpaqueMtgoQualifiedDirectVisibleSourceObservationV1 {
        _before_frame: before_frame,
        _after_frame: after_frame,
        result,
        commitments: MtgoQualifiedDirectVisibleSourceObservationCommitmentsV1 {
            runtime_identity_commitment_sha256: runtime
                .commitments
                .runtime_identity_commitment_sha256
                .clone(),
            broker_binary_sha256: runtime.commitments.broker_binary_sha256.clone(),
            producer_binary_sha256: runtime.commitments.producer_binary_sha256.clone(),
            before_capture_commitment_sha256: before_commitments.capture_commitment_sha256,
            after_capture_commitment_sha256: after_commitments.capture_commitment_sha256,
            sanitized_result_sha256,
            qualification_commitment_sha256,
        },
    })
}

/// Performs one in-memory, no-input qualification of the release-pinned
/// direct observer in the currently foreground spectator duel. This route is
/// permanently spectator qualification only. It cannot establish acting-
/// player knowledge, legal-action authority, model-scoring authority, or
/// input authority even if the producer returns a data-bearing result.
pub fn qualify_attested_direct_visible_source_current_spectator_v1(
    expected_game_format: &str,
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    capture_timeout_ms: u32,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoQualifiedDirectVisibleSourceObservationV1, String> {
    if !(100..=10_000).contains(&capture_timeout_ms) || !(100..=30_000).contains(&broker_timeout_ms)
    {
        return Err(
            "direct-source spectator qualification timeout is outside the supported range"
                .to_owned(),
        );
    }
    verify_runtime_identity_now_v1(runtime)?;
    let request = MtgoDxgiCaptureRequestV3 {
        expected_executable_sha256: PINNED_MTGO_EXECUTABLE_SHA256_V1.to_owned(),
        expected_signer_thumbprint: PINNED_MTGO_SIGNER_THUMBPRINT_V1.to_owned(),
        expected_signer_subject_sha256: PINNED_MTGO_SIGNER_SUBJECT_SHA256_V1.to_owned(),
        window_mode: CaptureWindowModeV2::SpectatorGame,
        expected_game_format: Some(expected_game_format.to_owned()),
        expected_title_contains: None,
        timeout_ms: capture_timeout_ms,
    };
    let before_frame = capture_mtgo_dxgi_frame_candidate_v3(request.clone())?;
    let before_commitments = before_frame.commitments_v3();
    require_fresh_source_v1(
        before_commitments.captured_at_unix_millis,
        unix_millis_now_v1()?,
    )?;
    let process_id = before_frame.manifest.pre.process_id;
    if process_id == 0 {
        return Err(
            "direct-source spectator qualification has no MTGO process identity".to_owned(),
        );
    }

    let invocation = invoke_observe_only_broker_v1(
        runtime,
        process_id,
        Duration::from_millis(u64::from(broker_timeout_ms)),
    );
    let after_frame_result = capture_mtgo_dxgi_frame_candidate_v3(request);
    let runtime_recheck = verify_runtime_identity_now_v1(runtime);
    runtime_recheck?;
    let after_frame = after_frame_result?;
    let output = invocation?;
    validate_same_unadmitted_spectator_observation_lineage_v1(&before_frame, &after_frame)?;
    let result = parse_and_validate_visible_duel_producer_result_v1(&output.0).map_err(|_| {
        "direct-source spectator broker did not return one sanitized visible result".to_owned()
    })?;
    let sanitized_result_sha256 = sha256_hex_v1(&output.0);
    let after_commitments = after_frame.commitments_v3();
    let qualification_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_SPECTATOR_SOURCE_QUALIFICATION_DOMAIN_V1,
        &[
            runtime
                .commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            before_commitments.capture_commitment_sha256.as_bytes(),
            after_commitments.capture_commitment_sha256.as_bytes(),
            sanitized_result_sha256.as_bytes(),
            b"spectator_visible_only_no_actor_knowledge_no_scoring_no_input",
        ],
    );
    Ok(OpaqueMtgoQualifiedDirectVisibleSourceObservationV1 {
        _before_frame: before_frame,
        _after_frame: after_frame,
        result,
        commitments: MtgoQualifiedDirectVisibleSourceObservationCommitmentsV1 {
            runtime_identity_commitment_sha256: runtime
                .commitments
                .runtime_identity_commitment_sha256
                .clone(),
            broker_binary_sha256: runtime.commitments.broker_binary_sha256.clone(),
            producer_binary_sha256: runtime.commitments.producer_binary_sha256.clone(),
            before_capture_commitment_sha256: before_commitments.capture_commitment_sha256,
            after_capture_commitment_sha256: after_commitments.capture_commitment_sha256,
            sanitized_result_sha256,
            qualification_commitment_sha256,
        },
    })
}

/// Runs the release-pinned observe-only producer twice against the sole MTGO
/// process without capturing pixels or requiring the client to be foreground.
///
/// This route reflects the permission boundary: transport may be direct, but
/// the outward value must still be exactly the strict rendered-UI-equivalent
/// schema. The broker independently verifies the signed client and all pinned
/// deployment files around each invocation. Rust additionally requires one
/// unchanged process incarnation, unchanged local runtime artifacts, and
/// byte-identical strictly parsed results across both invocations.
///
/// The returned value is qualification-only. It cannot reach a model or input
/// until a separate reviewed visible-equivalence ratification promotes this
/// exact transport.
pub fn qualify_stable_background_direct_visible_source_v1(
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoStableBackgroundDirectVisibleSourceV1, String> {
    if !(100..=30_000).contains(&broker_timeout_ms) {
        return Err(
            "background direct-source broker timeout is outside the supported range".to_owned(),
        );
    }
    verify_runtime_identity_now_v1(runtime)?;
    let before = sole_pinned_mtgo_process_incarnation_v1()?;
    let first = invoke_observe_only_broker_v1(
        runtime,
        before.process_id,
        Duration::from_millis(u64::from(broker_timeout_ms)),
    )?;
    verify_runtime_identity_now_v1(runtime)?;
    let between = sole_pinned_mtgo_process_incarnation_v1()?;
    require_same_private_process_incarnation_v1(&before, &between)?;
    let second = invoke_observe_only_broker_v1(
        runtime,
        between.process_id,
        Duration::from_millis(u64::from(broker_timeout_ms)),
    )?;
    verify_runtime_identity_now_v1(runtime)?;
    let after = sole_pinned_mtgo_process_incarnation_v1()?;
    require_same_private_process_incarnation_v1(&before, &after)?;

    let result = validate_stable_background_direct_visible_results_v1(&first.0, &second.0)?;
    let sanitized_result_sha256 = sha256_hex_v1(&first.0);
    let stability_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_BACKGROUND_STABILITY_DOMAIN_V1,
        &[
            runtime
                .commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            sanitized_result_sha256.as_bytes(),
            sanitized_result_sha256.as_bytes(),
            b"same_process_incarnation_verified_privately_and_discarded",
            b"two_exact_visible_equivalent_results_no_pixels_no_model_no_input",
        ],
    );
    Ok(OpaqueMtgoStableBackgroundDirectVisibleSourceV1 {
        _exact_result_bytes: first,
        result,
        commitments: MtgoStableBackgroundDirectVisibleSourceCommitmentsV1 {
            runtime_identity_commitment_sha256: runtime
                .commitments
                .runtime_identity_commitment_sha256
                .clone(),
            broker_binary_sha256: runtime.commitments.broker_binary_sha256.clone(),
            producer_binary_sha256: runtime.commitments.producer_binary_sha256.clone(),
            sanitized_result_sha256,
            stability_commitment_sha256,
        },
    })
}

/// Writes one exact data-bearing visible-equivalent producer result for
/// manual review. An abstention writes nothing. This function does not ratify
/// the result and cannot create model, semantic-evidence, input, event-entry,
/// or spending authority.
pub fn write_stable_background_direct_visible_review_artifact_v1(
    observation: OpaqueMtgoStableBackgroundDirectVisibleSourceV1,
    requested_output_directory: &Path,
) -> Result<MtgoStableBackgroundDirectVisibleReviewArtifactReceiptV1, String> {
    let exact_result_bytes = &observation._exact_result_bytes.0;
    let reparsed = parse_and_validate_visible_duel_producer_result_v1(exact_result_bytes)
        .map_err(|_| "background review result is not strictly visible-equivalent".to_owned())?;
    if reparsed != observation.result {
        return Err("background review result differs from its retained typed value".to_owned());
    }
    let result_kind = match &reparsed {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { .. } => "visible_decision",
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { .. } => {
            "visible_attacker_selection"
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
            ..
        } => "visible_single_attacker_blocker_selection",
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
            ..
        } => "visible_single_attacker_blocker_execution_state",
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection { .. } => {
            "visible_multi_attacker_blocker_selection"
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { .. } => {
            "visible_blocker_target_selection"
        }
        MtgoVisibleDuelViewModelBrokerResultV1::Abstained { .. } => {
            return Err("background review artifact requires a data-bearing duel result".to_owned())
        }
    };
    let commitments = observation.commitments;
    let visible_result_sha256 = sha256_hex_v1(exact_result_bytes);
    if visible_result_sha256 != commitments.sanitized_result_sha256 {
        return Err("background review result hash differs from its qualification".to_owned());
    }

    let output_directory =
        validate_background_direct_visible_review_output_v1(requested_output_directory)?;
    let manifest = PrivateMtgoStableBackgroundDirectVisibleReviewManifestV1 {
        schema: "mtgo-direct-visible-background-review-manifest/v1",
        artifact_kind: "strict_player_visible_duel_result_manual_review",
        status: "pending_manual_visible_equivalence_review",
        information_boundary: "rendered_mtgo_ui_or_rendered_game_log_only",
        result_kind,
        visible_result_file: "visible-result.json",
        visible_result_byte_length: exact_result_bytes.len(),
        visible_result_sha256: &visible_result_sha256,
        runtime_identity_commitment_sha256: &commitments.runtime_identity_commitment_sha256,
        broker_binary_sha256: &commitments.broker_binary_sha256,
        producer_binary_sha256: &commitments.producer_binary_sha256,
        stability_commitment_sha256: &commitments.stability_commitment_sha256,
        review_template_file: "review-template.json",
        review_completed: false,
        safe_for_live_semantic_evidence: false,
        safe_for_model_scoring: false,
        safe_for_input: false,
        permits_event_entry: false,
        permits_spending: false,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serialize background review manifest: {error}"))?;
    let review_template = PrivateMtgoStableBackgroundDirectVisibleReviewTemplateV1 {
        schema: "mtgo-direct-visible-background-review-template/v1",
        artifact_status_required: "pending_manual_visible_equivalence_review",
        result_kind,
        visible_result_sha256: &visible_result_sha256,
        stability_commitment_sha256: &commitments.stability_commitment_sha256,
        reviewer_alias: "",
        every_exported_fact_visible_in_rendered_ui_or_rendered_game_log: false,
        legal_action_set_matches_visible_controls: false,
        ordinary_surface_complete: false,
        combat_surface_complete: false,
        no_hidden_zone_or_internal_identifier: false,
        review_completed: false,
    };
    let review_template_bytes = serde_json::to_vec_pretty(&review_template)
        .map_err(|error| format!("serialize background review template: {error}"))?;
    let manifest_sha256 = sha256_hex_v1(&manifest_bytes);
    let review_template_sha256 = sha256_hex_v1(&review_template_bytes);
    let artifact_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_BACKGROUND_REVIEW_ARTIFACT_DOMAIN_V1,
        &[
            commitments.runtime_identity_commitment_sha256.as_bytes(),
            commitments.stability_commitment_sha256.as_bytes(),
            result_kind.as_bytes(),
            visible_result_sha256.as_bytes(),
            manifest_sha256.as_bytes(),
            review_template_sha256.as_bytes(),
            b"pending_review_no_model_no_input_no_entry_no_spending",
        ],
    );

    persist_background_direct_visible_review_artifact_v1(
        &output_directory,
        exact_result_bytes,
        &manifest_bytes,
        &review_template_bytes,
    )?;
    Ok(MtgoStableBackgroundDirectVisibleReviewArtifactReceiptV1 {
        schema: "mtgo-direct-visible-background-review-artifact-receipt/v1",
        status: "pending_manual_visible_equivalence_review",
        output_directory,
        result_kind: result_kind.to_owned(),
        runtime_identity_commitment_sha256: commitments.runtime_identity_commitment_sha256,
        sanitized_result_sha256: visible_result_sha256,
        stability_commitment_sha256: commitments.stability_commitment_sha256,
        manifest_sha256,
        review_template_sha256,
        artifact_commitment_sha256,
        review_completed: false,
        safe_for_live_semantic_evidence: false,
        safe_for_model_scoring: false,
        safe_for_input: false,
        permits_event_entry: false,
        permits_spending: false,
    })
}

fn validate_background_direct_visible_review_output_v1(
    requested: &Path,
) -> Result<PathBuf, String> {
    if !requested.is_absolute() {
        return Err("background review output must be an absolute directory".to_owned());
    }
    match fs::symlink_metadata(requested) {
        Ok(_) => return Err("background review output must not already exist".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("inspect background review output: {error}")),
    }
    let requested_parent = requested
        .parent()
        .ok_or("background review output has no parent")?;
    let parent_metadata = fs::symlink_metadata(requested_parent)
        .map_err(|error| format!("inspect background review output parent: {error}"))?;
    if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
        return Err("background review output parent must be a non-symlink directory".to_owned());
    }
    let parent = requested_parent
        .canonicalize()
        .map_err(|error| format!("canonicalize background review output parent: {error}"))?;
    let name = requested
        .file_name()
        .ok_or("background review output has no directory name")?;
    if name.to_string_lossy().starts_with('.')
        && name
            .to_string_lossy()
            .starts_with(DIRECT_VISIBLE_BACKGROUND_REVIEW_PARTIAL_PREFIX_V1)
    {
        return Err("background review output uses the private partial prefix".to_owned());
    }
    let output = parent.join(name);
    match fs::symlink_metadata(&output) {
        Ok(_) => return Err("background review output must resolve to a new path".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "inspect resolved background review output: {error}"
            ))
        }
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not derive repository root")?
        .canonicalize()
        .map_err(|error| format!("canonicalize repository root: {error}"))?;
    if output.starts_with(repository) {
        return Err(
            "background review artifact may not be written inside the repository".to_owned(),
        );
    }
    Ok(output)
}

fn persist_background_direct_visible_review_artifact_v1(
    output: &Path,
    visible_result_bytes: &[u8],
    manifest_bytes: &[u8],
    review_template_bytes: &[u8],
) -> Result<(), String> {
    let parent = output
        .parent()
        .ok_or("background review output has no parent")?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before epoch: {error}"))?
        .as_nanos();
    let mut partial = None;
    for attempt in 0..16_u8 {
        let candidate = parent.join(format!(
            "{DIRECT_VISIBLE_BACKGROUND_REVIEW_PARTIAL_PREFIX_V1}{}-{nonce}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                partial = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "create background review partial directory: {error}"
                ))
            }
        }
    }
    let partial =
        partial.ok_or("could not reserve a unique background review partial directory")?;
    let result = (|| {
        write_new_synced_file_v1(&partial.join("visible-result.json"), visible_result_bytes)?;
        write_new_synced_file_v1(&partial.join("manifest.json"), manifest_bytes)?;
        write_new_synced_file_v1(&partial.join("review-template.json"), review_template_bytes)?;
        fs::rename(&partial, output)
            .map_err(|error| format!("commit background review artifact directory: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        cleanup_background_direct_visible_review_partial_v1(parent, &partial);
    }
    result
}

fn write_new_synced_file_v1(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create background review artifact file: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("write background review artifact file: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync background review artifact file: {error}"))
}

fn cleanup_background_direct_visible_review_partial_v1(parent: &Path, partial: &Path) {
    let owned_name = partial
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.starts_with(DIRECT_VISIBLE_BACKGROUND_REVIEW_PARTIAL_PREFIX_V1))
        .unwrap_or(false);
    if partial.parent() == Some(parent) && owned_name {
        let _ = fs::remove_dir_all(partial);
    }
}

#[derive(Clone, PartialEq, Eq)]
struct PrivateMtgoProcessIncarnationV1 {
    process_id: u32,
    process_start_filetime_100ns: u64,
    executable_sha256: String,
}

struct PrivateProcessHandleV1(HANDLE);

impl Drop for PrivateProcessHandleV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn sole_pinned_mtgo_process_incarnation_v1() -> Result<PrivateMtgoProcessIncarnationV1, String> {
    let snapshot = PrivateProcessHandleV1(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(|error| format!("snapshot MTGO processes: {error}"))?,
    );
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe { Process32FirstW(snapshot.0, &mut entry) }
        .map_err(|error| format!("read first process entry: {error}"))?;
    let mut process_ids = Vec::new();
    loop {
        if utf16_nul_v1(&entry.szExeFile).eq_ignore_ascii_case("MTGO.exe") {
            process_ids.push(entry.th32ProcessID);
        }
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(error) => return Err(format!("read next process entry: {error}")),
        }
    }
    if process_ids.len() != 1 || process_ids[0] == 0 {
        return Err("background direct source requires exactly one MTGO process".to_owned());
    }
    inspect_pinned_mtgo_process_incarnation_v1(process_ids[0])
}

fn inspect_pinned_mtgo_process_incarnation_v1(
    process_id: u32,
) -> Result<PrivateMtgoProcessIncarnationV1, String> {
    let process = PrivateProcessHandleV1(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
            .map_err(|error| format!("open MTGO process identity: {error}"))?,
    );
    let mut path_utf16 = vec![0u16; 32_768];
    let mut path_length = path_utf16.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(path_utf16.as_mut_ptr()),
            &mut path_length,
        )
    }
    .map_err(|error| format!("read MTGO process image: {error}"))?;
    path_utf16.truncate(path_length as usize);
    let process_image = PathBuf::from(String::from_utf16(&path_utf16).map_err(|_| {
        "background direct-source MTGO process image is not valid UTF-16".to_owned()
    })?);
    if process_image
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| !value.eq_ignore_ascii_case("MTGO.exe"))
        .unwrap_or(true)
    {
        return Err("background direct-source process image is not MTGO.exe".to_owned());
    }
    let executable_sha256 = hash_bounded_file_v1(&process_image, "MTGO process image")?;
    if executable_sha256 != PINNED_MTGO_EXECUTABLE_SHA256_V1 {
        return Err(
            "background direct-source MTGO executable differs from the release pin".to_owned(),
        );
    }
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(|error| format!("read MTGO process start identity: {error}"))?;
    let process_start_filetime_100ns =
        ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
    if process_start_filetime_100ns == 0 {
        return Err("background direct-source MTGO process start identity is zero".to_owned());
    }
    Ok(PrivateMtgoProcessIncarnationV1 {
        process_id,
        process_start_filetime_100ns,
        executable_sha256,
    })
}

fn require_same_private_process_incarnation_v1(
    expected: &PrivateMtgoProcessIncarnationV1,
    observed: &PrivateMtgoProcessIncarnationV1,
) -> Result<(), String> {
    if expected != observed {
        return Err("background direct-source MTGO process incarnation changed".to_owned());
    }
    Ok(())
}

fn validate_stable_background_direct_visible_results_v1(
    first: &[u8],
    second: &[u8],
) -> Result<MtgoVisibleDuelViewModelBrokerResultV1, String> {
    let first_result = parse_and_validate_visible_duel_producer_result_v1(first)
        .map_err(|_| "first background direct-source result is not strictly visible-only")?;
    let second_result = parse_and_validate_visible_duel_producer_result_v1(second)
        .map_err(|_| "second background direct-source result is not strictly visible-only")?;
    if first != second || first_result != second_result {
        return Err(
            "background direct-source rendered presentation changed between observations"
                .to_owned(),
        );
    }
    Ok(first_result)
}

fn utf16_nul_v1(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|code_unit| *code_unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

fn trusted_system_windows_directory_v1() -> Result<OsString, String> {
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if length == 0 || length >= buffer.len() || buffer[..length].contains(&0) {
        return Err("read trusted Windows system directory".to_owned());
    }
    Ok(OsString::from_wide(&buffer[..length]))
}

fn broker_argument_path_v1(path: &Path) -> Result<PathBuf, String> {
    let encoded = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let verbatim_prefix = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    let verbatim_unc_prefix = [
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];
    let normalized = if encoded.starts_with(&verbatim_unc_prefix) {
        let mut value = vec![b'\\' as u16, b'\\' as u16];
        value.extend_from_slice(&encoded[verbatim_unc_prefix.len()..]);
        value
    } else if encoded.starts_with(&verbatim_prefix) {
        let value = encoded[verbatim_prefix.len()..].to_vec();
        let drive_letter = value.first().copied().is_some_and(|unit| {
            (b'A' as u16..=b'Z' as u16).contains(&unit)
                || (b'a' as u16..=b'z' as u16).contains(&unit)
        });
        if value.len() < 3 || !drive_letter || value[1] != b':' as u16 || value[2] != b'\\' as u16 {
            return Err("verified broker artifact has an unsupported verbatim path".to_owned());
        }
        value
    } else {
        encoded
    };
    if normalized.is_empty() || normalized.contains(&0) {
        return Err("verified broker artifact path is empty or contains NUL".to_owned());
    }
    let path = PathBuf::from(OsString::from_wide(&normalized));
    if !path.is_absolute() {
        return Err("verified broker artifact path is not absolute".to_owned());
    }
    Ok(path)
}

fn verify_exact_artifact_v1(
    path: &Path,
    expected_file_name: &str,
    expected_sha256: &str,
    label: &str,
) -> Result<PathBuf, String> {
    if !path.is_absolute() || !path.is_file() {
        return Err(format!("{label} must be an existing absolute file"));
    }
    let path = fs::canonicalize(path).map_err(|error| format!("resolve {label}: {error}"))?;
    if path.file_name().and_then(|value| value.to_str()) != Some(expected_file_name) {
        return Err(format!("{label} has the wrong file name"));
    }
    if hash_bounded_file_v1(&path, label)? != expected_sha256 {
        return Err(format!("{label} hash differs from the release pin"));
    }
    Ok(path)
}

struct ZeroingVecV1(Vec<u8>);

impl Drop for ZeroingVecV1 {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

fn verify_runtime_identity_now_v1(
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
) -> Result<(), String> {
    for (path, file_name, digest, label) in [
        (
            runtime.broker_path.as_path(),
            "mtgo_visible_duel_live_broker_v1.exe",
            LIVE_BROKER_SHA256_V1,
            "live direct-source broker",
        ),
        (
            runtime.bootstrap_path.as_path(),
            "mtgo_visible_duel_bootstrap_v1.dll",
            LIVE_BOOTSTRAP_SHA256_V1,
            "live direct-source bootstrap",
        ),
        (
            runtime.producer_path.as_path(),
            "mtgo_visible_duel_producer_v1.dll",
            LIVE_PRODUCER_SHA256_V1,
            "live direct-source producer",
        ),
        (
            runtime.validator_path.as_path(),
            "check_mtgo_visible_duel_producer_result_v1.exe",
            LIVE_VALIDATOR_SHA256_V1,
            "live direct-source strict validator",
        ),
    ] {
        let observed = verify_exact_artifact_v1(path, file_name, digest, label)?;
        if observed != path {
            return Err(format!("{label} canonical path changed"));
        }
    }
    Ok(())
}

fn require_ratified_direct_visible_dispatch_runtime_v1(
    reviewed_dispatch_runtime_commitment_sha256: &str,
) -> Result<(), String> {
    let Some(ratified) = RATIFIED_DIRECT_VISIBLE_DISPATCH_RUNTIME_COMMITMENT_V1 else {
        return Err("the production direct-visible dispatch ratification root is empty".to_owned());
    };
    if reviewed_dispatch_runtime_commitment_sha256 != ratified {
        return Err("the reviewed direct-visible dispatch runtime is not ratified".to_owned());
    }
    Ok(())
}

fn require_ratified_direct_visible_combat_dispatch_runtime_v1(
    reviewed_dispatch_runtime_commitment_sha256: &str,
) -> Result<(), String> {
    let Some(ratified) = RATIFIED_DIRECT_VISIBLE_COMBAT_DISPATCH_RUNTIME_COMMITMENT_V1 else {
        return Err(
            "the production direct-visible combat dispatch ratification root is empty".to_owned(),
        );
    };
    if reviewed_dispatch_runtime_commitment_sha256 != ratified {
        return Err(
            "the reviewed direct-visible combat dispatch runtime is not ratified".to_owned(),
        );
    }
    Ok(())
}

fn verify_dispatch_runtime_identity_now_v1(
    runtime: &OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1,
) -> Result<(), String> {
    for (path, file_name, digest, label) in [
        (
            runtime.broker_path.as_path(),
            "mtgo_visible_duel_live_dispatch_broker_v1.exe",
            LIVE_DISPATCH_BROKER_SHA256_V1,
            "live direct-visible dispatch broker",
        ),
        (
            runtime.bootstrap_path.as_path(),
            "mtgo_visible_duel_bootstrap_v1.dll",
            LIVE_BOOTSTRAP_SHA256_V1,
            "live direct-visible dispatch bootstrap",
        ),
        (
            runtime.producer_path.as_path(),
            "mtgo_visible_duel_producer_v1.dll",
            LIVE_PRODUCER_SHA256_V1,
            "live direct-visible dispatch producer",
        ),
        (
            runtime.validator_path.as_path(),
            "check_mtgo_visible_duel_producer_result_v1.exe",
            LIVE_VALIDATOR_SHA256_V1,
            "live direct-visible dispatch validator",
        ),
    ] {
        let observed = verify_exact_artifact_v1(path, file_name, digest, label)?;
        if observed != path {
            return Err(format!("{label} canonical path changed"));
        }
    }
    Ok(())
}

fn invoke_observe_only_broker_v1(
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    process_id: u32,
    timeout: Duration,
) -> Result<ZeroingVecV1, String> {
    let windows_directory = trusted_system_windows_directory_v1()?;
    let bootstrap_path = broker_argument_path_v1(&runtime.bootstrap_path)?;
    let producer_path = broker_argument_path_v1(&runtime.producer_path)?;
    let validator_path = broker_argument_path_v1(&runtime.validator_path)?;
    let mut child = Command::new(&runtime.broker_path)
        .arg("--pid")
        .arg(process_id.to_string())
        .arg("--bootstrap")
        .arg(bootstrap_path)
        .arg("--producer")
        .arg(producer_path)
        .arg("--validator")
        .arg(validator_path)
        .current_dir(
            runtime
                .broker_path
                .parent()
                .ok_or("direct-source broker has no parent directory")?,
        )
        .env_clear()
        .env("SystemRoot", &windows_directory)
        .creation_flags(CREATE_NO_WINDOW_V1)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "release-pinned direct-source broker could not start".to_owned())?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("release-pinned direct-source broker has no stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or("release-pinned direct-source broker has no stderr")?;
    let started = Instant::now();
    let (status, stdout, stderr, stdout_truncated, stderr_truncated) = thread::scope(|scope| {
        let stdout_reader =
            scope.spawn(|| read_bounded_and_drain_v1(&mut stdout, MAX_BROKER_STDOUT_BYTES_V1));
        let stderr_reader =
            scope.spawn(|| read_bounded_and_drain_v1(&mut stderr, MAX_BROKER_STDERR_BYTES_V1));
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|_| "poll release-pinned direct-source broker".to_owned())?
            {
                break status;
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err("release-pinned direct-source broker timed out".to_owned());
            }
            thread::sleep(Duration::from_millis(5));
        };
        let (stdout, stdout_truncated) = stdout_reader
            .join()
            .map_err(|_| "direct-source broker stdout reader panicked".to_owned())??;
        let (stderr, stderr_truncated) = stderr_reader
            .join()
            .map_err(|_| "direct-source broker stderr reader panicked".to_owned())??;
        Ok::<_, String>((status, stdout, stderr, stdout_truncated, stderr_truncated))
    })?;
    validate_broker_process_result_v1(status, stdout, stderr, stdout_truncated, stderr_truncated)
}

fn invoke_dispatch_broker_v1(
    runtime: &OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1,
    process_id: u32,
    decision_sha256: &str,
    selected_index: usize,
    timeout: Duration,
) -> Result<ZeroingVecV1, String> {
    if process_id == 0
        || selected_index >= 64
        || decision_sha256.len() != 64
        || !decision_sha256
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
    {
        return Err("direct-visible dispatch arguments are invalid".to_owned());
    }
    let windows_directory = trusted_system_windows_directory_v1()?;
    let bootstrap_path = broker_argument_path_v1(&runtime.bootstrap_path)?;
    let producer_path = broker_argument_path_v1(&runtime.producer_path)?;
    let validator_path = broker_argument_path_v1(&runtime.validator_path)?;
    let mut child = Command::new(&runtime.broker_path)
        .arg("--pid")
        .arg(process_id.to_string())
        .arg("--bootstrap")
        .arg(bootstrap_path)
        .arg("--producer")
        .arg(producer_path)
        .arg("--validator")
        .arg(validator_path)
        .arg("--decision-sha256")
        .arg(decision_sha256)
        .arg("--selected-index")
        .arg(selected_index.to_string())
        .current_dir(
            runtime
                .broker_path
                .parent()
                .ok_or("direct-visible dispatch broker has no parent directory")?,
        )
        .env_clear()
        .env("SystemRoot", &windows_directory)
        .creation_flags(CREATE_NO_WINDOW_V1)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "release-pinned direct-visible dispatch broker could not start".to_owned())?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("release-pinned direct-visible dispatch broker has no stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or("release-pinned direct-visible dispatch broker has no stderr")?;
    let started = Instant::now();
    let (status, stdout, stderr, stdout_truncated, stderr_truncated) = thread::scope(|scope| {
        let stdout_reader =
            scope.spawn(|| read_bounded_and_drain_v1(&mut stdout, MAX_BROKER_STDOUT_BYTES_V1));
        let stderr_reader =
            scope.spawn(|| read_bounded_and_drain_v1(&mut stderr, MAX_BROKER_STDERR_BYTES_V1));
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|_| "poll release-pinned direct-visible dispatch broker".to_owned())?
            {
                break status;
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err("release-pinned direct-visible dispatch broker timed out".to_owned());
            }
            thread::sleep(Duration::from_millis(5));
        };
        let (stdout, stdout_truncated) = stdout_reader
            .join()
            .map_err(|_| "direct-visible dispatch stdout reader panicked".to_owned())??;
        let (stderr, stderr_truncated) = stderr_reader
            .join()
            .map_err(|_| "direct-visible dispatch stderr reader panicked".to_owned())??;
        Ok::<_, String>((status, stdout, stderr, stdout_truncated, stderr_truncated))
    })?;
    validate_broker_process_result_v1(status, stdout, stderr, stdout_truncated, stderr_truncated)
}

fn invoke_combat_dispatch_broker_v1(
    runtime: &OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1,
    process_id: u32,
    command: &MtgoPlayerVisibleCombatBrokerCommandV1,
    timeout: Duration,
) -> Result<ZeroingVecV1, String> {
    if process_id == 0 {
        return Err("direct-visible combat dispatch process is invalid".to_owned());
    }
    let combat_arguments = combat_dispatch_arguments_v1(command)?;
    let windows_directory = trusted_system_windows_directory_v1()?;
    let bootstrap_path = broker_argument_path_v1(&runtime.bootstrap_path)?;
    let producer_path = broker_argument_path_v1(&runtime.producer_path)?;
    let validator_path = broker_argument_path_v1(&runtime.validator_path)?;
    let mut child = Command::new(&runtime.broker_path);
    child
        .arg("--pid")
        .arg(process_id.to_string())
        .arg("--bootstrap")
        .arg(bootstrap_path)
        .arg("--producer")
        .arg(producer_path)
        .arg("--validator")
        .arg(validator_path);
    for argument in combat_arguments {
        child.arg(argument);
    }
    child
        .current_dir(
            runtime
                .broker_path
                .parent()
                .ok_or("direct-visible combat dispatch broker has no parent directory")?,
        )
        .env_clear()
        .env("SystemRoot", &windows_directory)
        .creation_flags(CREATE_NO_WINDOW_V1)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = child
        .spawn()
        .map_err(|_| "release-pinned direct-visible combat broker could not start".to_owned())?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("release-pinned direct-visible combat broker has no stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or("release-pinned direct-visible combat broker has no stderr")?;
    let started = Instant::now();
    let (status, stdout, stderr, stdout_truncated, stderr_truncated) = thread::scope(|scope| {
        let stdout_reader =
            scope.spawn(|| read_bounded_and_drain_v1(&mut stdout, MAX_BROKER_STDOUT_BYTES_V1));
        let stderr_reader =
            scope.spawn(|| read_bounded_and_drain_v1(&mut stderr, MAX_BROKER_STDERR_BYTES_V1));
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|_| "poll release-pinned direct-visible combat broker".to_owned())?
            {
                break status;
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err("release-pinned direct-visible combat broker timed out".to_owned());
            }
            thread::sleep(Duration::from_millis(5));
        };
        let (stdout, stdout_truncated) = stdout_reader
            .join()
            .map_err(|_| "direct-visible combat dispatch stdout reader panicked".to_owned())??;
        let (stderr, stderr_truncated) = stderr_reader
            .join()
            .map_err(|_| "direct-visible combat dispatch stderr reader panicked".to_owned())??;
        Ok::<_, String>((status, stdout, stderr, stdout_truncated, stderr_truncated))
    })?;
    validate_broker_process_result_v1(status, stdout, stderr, stdout_truncated, stderr_truncated)
}

fn combat_dispatch_arguments_v1(
    command: &MtgoPlayerVisibleCombatBrokerCommandV1,
) -> Result<Vec<String>, String> {
    fn lower_sha256_v1(value: &str) -> bool {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }
    let arguments = match command {
        MtgoPlayerVisibleCombatBrokerCommandV1::AttackerPlan {
            current_selection_sha256,
            candidate_count,
            desired_mask_hex,
            plan_commitment_sha256,
        } => {
            let desired_mask = u64::from_str_radix(desired_mask_hex, 16).ok();
            if !lower_sha256_v1(current_selection_sha256)
                || *candidate_count > 64
                || desired_mask_hex.len() != 16
                || desired_mask.is_none()
                || (*candidate_count < 64
                    && (desired_mask.expect("checked hexadecimal mask") >> candidate_count) != 0)
                || !lower_sha256_v1(plan_commitment_sha256)
            {
                return Err("attacker combat broker arguments are invalid".to_owned());
            }
            vec![
                "--attacker-selection-sha256".to_owned(),
                current_selection_sha256.clone(),
                "--candidate-count".to_owned(),
                candidate_count.to_string(),
                "--desired-mask".to_owned(),
                desired_mask_hex.clone(),
                "--plan-sha256".to_owned(),
                plan_commitment_sha256.clone(),
            ]
        }
        MtgoPlayerVisibleCombatBrokerCommandV1::SingleAttackerBlockerPlan {
            current_selection_sha256,
            candidate_count,
            desired_mask_hex,
            plan_commitment_sha256,
        } => {
            let desired_mask = u64::from_str_radix(desired_mask_hex, 16).ok();
            if !lower_sha256_v1(current_selection_sha256)
                || *candidate_count > 64
                || desired_mask_hex.len() != 16
                || desired_mask.is_none()
                || (*candidate_count < 64
                    && (desired_mask.expect("checked hexadecimal mask") >> candidate_count) != 0)
                || !lower_sha256_v1(plan_commitment_sha256)
            {
                return Err("single-blocker combat broker arguments are invalid".to_owned());
            }
            vec![
                "--single-blocker-selection-sha256".to_owned(),
                current_selection_sha256.clone(),
                "--candidate-count".to_owned(),
                candidate_count.to_string(),
                "--desired-mask".to_owned(),
                desired_mask_hex.clone(),
                "--plan-sha256".to_owned(),
                plan_commitment_sha256.clone(),
            ]
        }
        MtgoPlayerVisibleCombatBrokerCommandV1::MultiAttackerBlockerStep {
            current_selection_sha256,
            selected_index,
            operation_kind,
            blocker_visible_ordinal,
            attacker_visible_ordinal,
            model_selection_commitment_sha256,
            execution_step_commitment_sha256,
        } => {
            let shape_valid = match operation_kind {
                'f' => blocker_visible_ordinal.is_none() && attacker_visible_ordinal.is_none(),
                'b' => blocker_visible_ordinal.is_some() && attacker_visible_ordinal.is_none(),
                't' => blocker_visible_ordinal.is_some() && attacker_visible_ordinal.is_some(),
                _ => false,
            };
            if !lower_sha256_v1(current_selection_sha256)
                || *selected_index >= 64
                || !shape_valid
                || !lower_sha256_v1(model_selection_commitment_sha256)
                || !lower_sha256_v1(execution_step_commitment_sha256)
            {
                return Err("multi-blocker combat broker arguments are invalid".to_owned());
            }
            vec![
                "--blocker-selection-sha256".to_owned(),
                current_selection_sha256.clone(),
                "--selected-index".to_owned(),
                selected_index.to_string(),
                "--blocker-operation".to_owned(),
                operation_kind.to_string(),
                "--blocker-ordinal".to_owned(),
                blocker_visible_ordinal.map_or_else(|| "-".to_owned(), |value| value.to_string()),
                "--attacker-ordinal".to_owned(),
                attacker_visible_ordinal.map_or_else(|| "-".to_owned(), |value| value.to_string()),
                "--model-selection-sha256".to_owned(),
                model_selection_commitment_sha256.clone(),
                "--step-sha256".to_owned(),
                execution_step_commitment_sha256.clone(),
            ]
        }
    };
    Ok(arguments)
}

fn validate_broker_process_result_v1(
    status: ExitStatus,
    output: ZeroingVecV1,
    stderr: ZeroingVecV1,
    output_truncated: bool,
    stderr_truncated: bool,
) -> Result<ZeroingVecV1, String> {
    let payload_length = output
        .0
        .strip_suffix(b"\r\n")
        .map(|value| value.len())
        .or_else(|| output.0.strip_suffix(b"\n").map(|value| value.len()));
    if !status.success()
        || output.0.is_empty()
        || output_truncated
        || stderr_truncated
        || !stderr.0.is_empty()
        || payload_length.is_none_or(|length| {
            length == 0
                || output.0[..length].contains(&b'\n')
                || output.0[..length].contains(&b'\r')
        })
    {
        return Err(match safe_broker_failure_code_v1(&stderr.0) {
            Some(code) => format!("release-pinned direct-source broker failed closed: {code}"),
            None => "release-pinned direct-source broker failed closed".to_owned(),
        });
    }
    let mut output = output;
    output
        .0
        .truncate(payload_length.expect("validated terminal line ending"));
    Ok(output)
}

fn safe_broker_failure_code_v1(stderr: &[u8]) -> Option<&'static str> {
    const PREFIX: &[u8] = b"mtgo_visible_duel_broker_v1:";
    const CODES: &[&str] = &[
        "arguments",
        "input_validation",
        "live_dispatch_not_admitted",
        "channel_name",
        "channel_create",
        "channel_map",
        "process_open",
        "target_not_synthetic_host",
        "live_identity_pre",
        "bootstrap_write",
        "bootstrap_load",
        "bootstrap_entry",
        "parameter_copy",
        "parameter_write",
        "producer_invoke",
        "output_validation",
        "live_identity_post",
    ];
    let line = stderr
        .strip_suffix(b"\r\n")
        .or_else(|| stderr.strip_suffix(b"\n"))?
        .strip_prefix(PREFIX)?;
    CODES
        .iter()
        .copied()
        .find(|candidate| line == candidate.as_bytes())
}

fn validate_same_duel_observation_lineage_v1(
    before: &OpaqueMtgoAdmittedDuelVisibleFrameV1,
    after: &OpaqueMtgoAdmittedDuelVisibleFrameV1,
) -> Result<(), String> {
    let before_commitments = before.commitments_v1();
    let after_commitments = after.commitments_v1();
    let before_manifest = &before.source_frame.manifest;
    let after_manifest = &after.source_frame.manifest;
    if before_commitments.perception_profile_commitment_sha256
        != after_commitments.perception_profile_commitment_sha256
        || before_commitments.perception_profile_admission_commitment_sha256
            != after_commitments.perception_profile_admission_commitment_sha256
        || before_commitments.source_capture.capture_commitment_sha256
            == after_commitments.source_capture.capture_commitment_sha256
        || after_commitments.source_capture.captured_at_unix_millis
            <= before_commitments.source_capture.captured_at_unix_millis
        || before_manifest.window_mode != "duel_game"
        || after_manifest.window_mode != "duel_game"
        || before_manifest.capture_role != "acting_player_duel"
        || after_manifest.capture_role != "acting_player_duel"
        || before_manifest.expected_game_format != after_manifest.expected_game_format
        || before_manifest.pre.hwnd != after_manifest.pre.hwnd
        || before_manifest.pre.process_id != after_manifest.pre.process_id
        || before_manifest.pre.process_start_filetime_100ns
            != after_manifest.pre.process_start_filetime_100ns
        || before_manifest.pre.process_image != after_manifest.pre.process_image
        || before_manifest.pre.executable_sha256 != after_manifest.pre.executable_sha256
        || before_manifest.pre.signer_thumbprint != after_manifest.pre.signer_thumbprint
        || before_manifest.pre.signer_subject_sha256 != after_manifest.pre.signer_subject_sha256
        || before_manifest.pre.title != after_manifest.pre.title
        || before_manifest.pre.dpi != after_manifest.pre.dpi
        || before_manifest.pre.client_rect_desktop_px != after_manifest.pre.client_rect_desktop_px
        || before_manifest.pre.extended_frame_rect_desktop_px
            != after_manifest.pre.extended_frame_rect_desktop_px
        || before_manifest.output != after_manifest.output
        || before_manifest.frame.canonical_width != after_manifest.frame.canonical_width
        || before_manifest.frame.canonical_height != after_manifest.frame.canonical_height
    {
        return Err(
            "direct-source observation changed the duel process, window, output, profile, or geometry"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_same_unadmitted_duel_observation_lineage_v1(
    before: &OpaqueMtgoDxgiFrameCandidateV3,
    after: &OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<(), String> {
    validate_same_unadmitted_observation_lineage_v1(
        before,
        after,
        "duel_game",
        "acting_player_duel",
    )
}

fn validate_same_unadmitted_spectator_observation_lineage_v1(
    before: &OpaqueMtgoDxgiFrameCandidateV3,
    after: &OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<(), String> {
    validate_same_unadmitted_observation_lineage_v1(before, after, "spectator_game", "spectator")
}

fn validate_same_unadmitted_observation_lineage_v1(
    before: &OpaqueMtgoDxgiFrameCandidateV3,
    after: &OpaqueMtgoDxgiFrameCandidateV3,
    expected_window_mode: &str,
    expected_capture_role: &str,
) -> Result<(), String> {
    let before_commitments = before.commitments_v3();
    let after_commitments = after.commitments_v3();
    let before_manifest = &before.manifest;
    let after_manifest = &after.manifest;
    if before_commitments.capture_commitment_sha256 == after_commitments.capture_commitment_sha256
        || after_commitments.captured_at_unix_millis <= before_commitments.captured_at_unix_millis
        || before_manifest.window_mode != expected_window_mode
        || after_manifest.window_mode != expected_window_mode
        || before_manifest.capture_role != expected_capture_role
        || after_manifest.capture_role != expected_capture_role
        || before_manifest.expected_game_format != after_manifest.expected_game_format
        || before_manifest.pre.hwnd != after_manifest.pre.hwnd
        || before_manifest.pre.process_id != after_manifest.pre.process_id
        || before_manifest.pre.process_start_filetime_100ns
            != after_manifest.pre.process_start_filetime_100ns
        || before_manifest.pre.process_image != after_manifest.pre.process_image
        || before_manifest.pre.executable_sha256 != after_manifest.pre.executable_sha256
        || before_manifest.pre.signer_thumbprint != after_manifest.pre.signer_thumbprint
        || before_manifest.pre.signer_subject_sha256 != after_manifest.pre.signer_subject_sha256
        || before_manifest.pre.title != after_manifest.pre.title
        || before_manifest.pre.dpi != after_manifest.pre.dpi
        || before_manifest.pre.client_rect_desktop_px != after_manifest.pre.client_rect_desktop_px
        || before_manifest.pre.extended_frame_rect_desktop_px
            != after_manifest.pre.extended_frame_rect_desktop_px
        || before_manifest.output != after_manifest.output
        || before_manifest.frame.canonical_width != after_manifest.frame.canonical_width
        || before_manifest.frame.canonical_height != after_manifest.frame.canonical_height
    {
        return Err(
            "direct-source qualification changed the process, window, output, mode, role, format, or geometry"
                .to_owned(),
        );
    }
    Ok(())
}

fn require_fresh_source_v1(captured_at: u128, now: u128) -> Result<(), String> {
    if captured_at > now || now - captured_at > MAX_SOURCE_AGE_MILLIS_V1 {
        return Err("direct-source before frame is stale or from the future".to_owned());
    }
    Ok(())
}

fn unix_millis_now_v1() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .map_err(|_| "system clock is before the Unix epoch".to_owned())
}

fn hash_bounded_file_v1(path: &Path, label: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("open {label}: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("inspect {label}: {error}"))?
        .len();
    if length == 0 || length > MAX_ARTIFACT_BYTES_V1 {
        return Err(format!("{label} length is outside the supported range"));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read {label}: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_bounded_and_drain_v1(
    reader: &mut impl Read,
    limit: usize,
) -> Result<(ZeroingVecV1, bool), String> {
    let mut retained = ZeroingVecV1(Vec::new());
    let mut truncated = false;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(read) => read,
            Err(_) => {
                buffer.fill(0);
                return Err("read direct-source broker pipe".to_owned());
            }
        };
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.0.len());
        let keep = remaining.min(read);
        retained
            .0
            .write_all(&buffer[..keep])
            .map_err(|_| "retain direct-source broker pipe".to_owned())?;
        truncated |= keep < read;
        buffer[..read].fill(0);
    }
    buffer.fill(0);
    Ok((retained, truncated))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TemporaryReviewParentV1(PathBuf);

    impl TemporaryReviewParentV1 {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "mtgo-direct-visible-review-test-{label}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn child(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TemporaryReviewParentV1 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn visible_decision_v1(
        action: mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1,
    ) -> mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1 {
        use mtgo_blackbox_v1::MtgoPlayerRelativeRoleV1 as R;
        mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1 {
            current_state: mtgo_blackbox_v1::MtgoPlayerVisibleDuelStateV1 {
                acting_player: R::SeatedPlayer,
                turn: 1,
                phase: mtg_kernel::rl::ZoneIndependentStepV1::Main1,
                active_player: R::SeatedPlayer,
                priority_player: R::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6]; 2],
                hand_counts: [1, 0],
                library_counts: [52, 53],
                battlefield: [Vec::new(), Vec::new()],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: mtgo_blackbox_v1::MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: false,
                    blockers_declared: false,
                    ordered_attackers: Vec::new(),
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_legal_actions: vec![action],
        }
    }

    fn stable_background_observation_v1(
        result: MtgoVisibleDuelViewModelBrokerResultV1,
    ) -> OpaqueMtgoStableBackgroundDirectVisibleSourceV1 {
        let bytes = serde_json::to_vec(&result).unwrap();
        let sanitized_result_sha256 = sha256_hex_v1(&bytes);
        OpaqueMtgoStableBackgroundDirectVisibleSourceV1 {
            _exact_result_bytes: ZeroingVecV1(bytes),
            result,
            commitments: MtgoStableBackgroundDirectVisibleSourceCommitmentsV1 {
                runtime_identity_commitment_sha256: "a".repeat(64),
                broker_binary_sha256: "b".repeat(64),
                producer_binary_sha256: "c".repeat(64),
                sanitized_result_sha256,
                stability_commitment_sha256: "d".repeat(64),
            },
        }
    }

    fn data_bearing_background_observation_v1() -> OpaqueMtgoStableBackgroundDirectVisibleSourceV1 {
        let mut decision =
            visible_decision_v1(mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1::Pass {
                actor: mtgo_blackbox_v1::MtgoPlayerRelativeRoleV1::SeatedPlayer,
            });
        decision.current_state.hand_counts = [0, 0];
        decision.current_state.library_counts = [53, 53];
        stable_background_observation_v1(MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision {
            decision: Box::new(decision),
        })
    }

    #[test]
    fn background_review_artifact_preserves_only_exact_visible_result_and_false_authority() {
        let parent = TemporaryReviewParentV1::new("valid");
        let output = parent.child("artifact");
        let observation = data_bearing_background_observation_v1();
        let expected_result = observation._exact_result_bytes.0.clone();
        let receipt =
            write_stable_background_direct_visible_review_artifact_v1(observation, &output)
                .unwrap();

        let mut files = fs::read_dir(&receipt.output_directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(
            files,
            vec![
                "manifest.json",
                "review-template.json",
                "visible-result.json"
            ]
        );
        let written_result =
            fs::read(receipt.output_directory.join("visible-result.json")).unwrap();
        assert_eq!(written_result, expected_result);
        assert_eq!(
            sha256_hex_v1(&written_result),
            receipt.sanitized_result_sha256
        );

        let manifest_bytes = fs::read(receipt.output_directory.join("manifest.json")).unwrap();
        assert_eq!(sha256_hex_v1(&manifest_bytes), receipt.manifest_sha256);
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        for field in [
            "review_completed",
            "safe_for_live_semantic_evidence",
            "safe_for_model_scoring",
            "safe_for_input",
            "permits_event_entry",
            "permits_spending",
        ] {
            assert_eq!(manifest[field], false, "manifest field {field}");
        }
        let review_bytes = fs::read(receipt.output_directory.join("review-template.json")).unwrap();
        assert_eq!(sha256_hex_v1(&review_bytes), receipt.review_template_sha256);
        let review: serde_json::Value = serde_json::from_slice(&review_bytes).unwrap();
        assert_eq!(review["reviewer_alias"], "");
        for field in [
            "every_exported_fact_visible_in_rendered_ui_or_rendered_game_log",
            "legal_action_set_matches_visible_controls",
            "ordinary_surface_complete",
            "combat_surface_complete",
            "no_hidden_zone_or_internal_identifier",
            "review_completed",
        ] {
            assert_eq!(review[field], false, "review field {field}");
        }
        assert!(!receipt.review_completed);
        assert!(!receipt.safe_for_live_semantic_evidence);
        assert!(!receipt.safe_for_model_scoring);
        assert!(!receipt.safe_for_input);
        assert!(!receipt.permits_event_entry);
        assert!(!receipt.permits_spending);
    }

    #[test]
    fn background_review_abstention_writes_nothing() {
        let parent = TemporaryReviewParentV1::new("abstention");
        let output = parent.child("artifact");
        let observation =
            stable_background_observation_v1(MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::DuelSurfaceUnavailable,
            });
        assert!(
            write_stable_background_direct_visible_review_artifact_v1(observation, &output)
                .is_err()
        );
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&parent.0).unwrap().count(), 0);
    }

    #[test]
    fn background_review_refuses_repository_and_existing_outputs() {
        let inside_repository = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "never-create-background-review-{}",
            std::process::id()
        ));
        assert!(!inside_repository.exists());
        assert!(write_stable_background_direct_visible_review_artifact_v1(
            data_bearing_background_observation_v1(),
            &inside_repository,
        )
        .is_err());
        assert!(!inside_repository.exists());

        let parent = TemporaryReviewParentV1::new("existing");
        let existing = parent.child("artifact");
        fs::create_dir(&existing).unwrap();
        assert!(write_stable_background_direct_visible_review_artifact_v1(
            data_bearing_background_observation_v1(),
            &existing,
        )
        .is_err());
        assert_eq!(fs::read_dir(&existing).unwrap().count(), 0);
    }

    #[test]
    fn source_freshness_is_bounded_and_clock_ordered() {
        require_fresh_source_v1(8_000, 10_000).unwrap();
        assert!(require_fresh_source_v1(7_999, 10_000).is_err());
        assert!(require_fresh_source_v1(10_001, 10_000).is_err());
    }

    #[test]
    fn runtime_and_observation_commitments_bind_every_part() {
        let runtime = commitment_v1(
            DIRECT_VISIBLE_SOURCE_RUNTIME_DOMAIN_V1,
            &[b"broker", b"bootstrap", b"producer", b"validator"],
        );
        let observation = commitment_v1(
            DIRECT_VISIBLE_SOURCE_OBSERVATION_DOMAIN_V1,
            &[runtime.as_bytes(), b"before", b"after", b"visible-result"],
        );
        assert_eq!(runtime.len(), 64);
        assert_eq!(observation.len(), 64);
        assert_ne!(
            observation,
            commitment_v1(
                DIRECT_VISIBLE_SOURCE_OBSERVATION_DOMAIN_V1,
                &[runtime.as_bytes(), b"before", b"changed", b"visible-result"],
            )
        );
        let qualification = commitment_v1(
            DIRECT_VISIBLE_SOURCE_QUALIFICATION_DOMAIN_V1,
            &[runtime.as_bytes(), b"before", b"after", b"visible-result"],
        );
        assert_eq!(qualification.len(), 64);
        assert_ne!(qualification, observation);
        assert_ne!(
            qualification,
            commitment_v1(
                DIRECT_VISIBLE_SOURCE_QUALIFICATION_DOMAIN_V1,
                &[runtime.as_bytes(), b"before", b"after", b"changed-result"],
            )
        );
        let scored_refresh = commitment_v1(
            DIRECT_VISIBLE_SOURCE_SCORED_REFRESH_DOMAIN_V1,
            &[b"initial", b"refreshed", b"selection", b"refresh"],
        );
        assert_eq!(scored_refresh.len(), 64);
        assert_ne!(scored_refresh, qualification);
        assert_ne!(
            scored_refresh,
            commitment_v1(
                DIRECT_VISIBLE_SOURCE_SCORED_REFRESH_DOMAIN_V1,
                &[b"initial", b"refreshed", b"selection", b"changed"],
            )
        );
        let background = commitment_v1(
            DIRECT_VISIBLE_BACKGROUND_STABILITY_DOMAIN_V1,
            &[b"runtime", b"process", b"first", b"second"],
        );
        assert_eq!(background.len(), 64);
        assert_ne!(background, qualification);
        assert_ne!(
            background,
            commitment_v1(
                DIRECT_VISIBLE_BACKGROUND_STABILITY_DOMAIN_V1,
                &[b"runtime", b"process", b"first", b"changed"],
            )
        );
    }

    #[test]
    fn background_direct_source_requires_two_exact_strict_visible_results() {
        let first = serde_json::to_vec(&MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
            reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
        })
        .unwrap();
        let parsed = validate_stable_background_direct_visible_results_v1(&first, &first).unwrap();
        assert!(matches!(
            parsed,
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete
            }
        ));

        let changed = serde_json::to_vec(&MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
            reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::DuelSurfaceUnavailable,
        })
        .unwrap();
        assert!(validate_stable_background_direct_visible_results_v1(&first, &changed).is_err());

        let mut unknown = first.clone();
        unknown.pop();
        unknown.extend_from_slice(br#",\"hidden_client_id\":7}"#);
        assert!(validate_stable_background_direct_visible_results_v1(&unknown, &unknown).is_err());
    }

    #[test]
    fn background_process_join_rejects_any_incarnation_change() {
        let original = PrivateMtgoProcessIncarnationV1 {
            process_id: 17,
            process_start_filetime_100ns: 23,
            executable_sha256: "a".repeat(64),
        };
        require_same_private_process_incarnation_v1(&original, &original).unwrap();
        let mut changed = original.clone();
        changed.process_start_filetime_100ns += 1;
        assert!(require_same_private_process_incarnation_v1(&original, &changed).is_err());
    }

    #[test]
    fn broker_arguments_use_non_verbatim_absolute_windows_paths() {
        assert_eq!(
            broker_argument_path_v1(Path::new(r"\\?\C:\mtgo\broker.exe")).unwrap(),
            PathBuf::from(r"C:\mtgo\broker.exe")
        );
        assert_eq!(
            broker_argument_path_v1(Path::new(r"\\?\UNC\server\share\broker.exe")).unwrap(),
            PathBuf::from(r"\\server\share\broker.exe")
        );
        assert!(broker_argument_path_v1(Path::new(r"\\?\Volume{abcd}\broker.exe")).is_err());
        let windows_directory = PathBuf::from(trusted_system_windows_directory_v1().unwrap());
        assert!(windows_directory.is_absolute());
        assert!(windows_directory.is_dir());
    }

    #[test]
    fn combat_routing_is_kind_only_and_excludes_ordinary_and_abstained_results() {
        let ordinary = MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision {
            decision: Box::new(visible_decision_v1(
                mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1::Pass {
                    actor: mtgo_blackbox_v1::MtgoPlayerRelativeRoleV1::SeatedPlayer,
                },
            )),
        };
        assert!(!result_requires_combat_scoring_v1(&ordinary));
        assert!(!result_requires_combat_scoring_v1(
            &MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
            }
        ));

        let mut state =
            visible_decision_v1(mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1::Pass {
                actor: mtgo_blackbox_v1::MtgoPlayerRelativeRoleV1::SeatedPlayer,
            })
            .current_state;
        state.phase = mtg_kernel::rl::ZoneIndependentStepV1::DeclareAttackers;
        let combat = MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection {
            selection: Box::new(
                mtgo_blackbox_v1::MtgoPlayerVisibleAttackerSelectionInputV1 {
                    current_state: state,
                    ordered_candidates: Vec::new(),
                    unique_visible_enabled_done_control: true,
                },
            ),
        };
        assert!(result_requires_combat_scoring_v1(&combat));
    }

    #[test]
    fn runtime_verification_rejects_unpinned_relative_artifacts() {
        let relative = Path::new("not-a-release-artifact");
        assert!(
            verify_direct_visible_source_runtime_v1(relative, relative, relative, relative,)
                .is_err()
        );
        assert!(
            verify_direct_visible_dispatch_runtime_v1(relative, relative, relative, relative,)
                .is_err()
        );
    }

    #[test]
    fn production_scoring_ratification_root_is_empty() {
        assert!(require_ratified_direct_visible_source_qualification_v1(&"a".repeat(64)).is_err());
        assert!(
            require_ratified_direct_visible_combat_source_qualification_v1(&"a".repeat(64))
                .is_err()
        );
        assert!(
            require_ratified_background_direct_visible_source_qualification_v1(&"a".repeat(64))
                .is_err()
        );
        assert!(
            require_ratified_background_direct_visible_combat_source_qualification_v1(
                &"a".repeat(64)
            )
            .is_err()
        );
    }

    #[test]
    fn fixed_broker_diagnostics_never_forward_arbitrary_stderr() {
        assert_eq!(
            safe_broker_failure_code_v1(b"mtgo_visible_duel_broker_v1:bootstrap_entry\r\n"),
            Some("bootstrap_entry")
        );
        assert_eq!(
            safe_broker_failure_code_v1(b"mtgo_visible_duel_broker_v1:producer_invoke\n"),
            Some("producer_invoke")
        );
        assert_eq!(
            safe_broker_failure_code_v1(b"mtgo_visible_duel_broker_v1:hidden_state=7\r\n"),
            None
        );
        assert_eq!(safe_broker_failure_code_v1(b"arbitrary stderr\r\n"), None);
    }

    #[test]
    fn broker_result_accepts_one_terminal_windows_or_unix_line_ending() {
        let success_status = || {
            Command::new("cmd.exe")
                .args(["/d", "/c", "exit", "0"])
                .status()
                .unwrap()
        };
        let windows = validate_broker_process_result_v1(
            success_status(),
            ZeroingVecV1(b"{}\r\n".to_vec()),
            ZeroingVecV1(Vec::new()),
            false,
            false,
        )
        .unwrap();
        assert_eq!(windows.0, b"{}");
        let unix = validate_broker_process_result_v1(
            success_status(),
            ZeroingVecV1(b"{}\n".to_vec()),
            ZeroingVecV1(Vec::new()),
            false,
            false,
        )
        .unwrap();
        assert_eq!(unix.0, b"{}");
        assert!(validate_broker_process_result_v1(
            success_status(),
            ZeroingVecV1(b"{\r}\r\n".to_vec()),
            ZeroingVecV1(Vec::new()),
            false,
            false,
        )
        .is_err());
    }

    #[test]
    fn production_dispatch_ratification_root_is_empty() {
        assert!(require_ratified_direct_visible_dispatch_runtime_v1(&"a".repeat(64)).is_err());
        assert!(
            require_ratified_direct_visible_combat_dispatch_runtime_v1(&"a".repeat(64)).is_err()
        );
    }

    #[test]
    fn combat_dispatch_receipt_is_domain_separated_from_ordinary_dispatch() {
        let parts = [b"runtime".as_slice(), b"step", b"receipt"];
        assert_ne!(
            commitment_v1(DIRECT_VISIBLE_COMBAT_DISPATCH_RECEIPT_DOMAIN_V1, &parts),
            commitment_v1(DIRECT_VISIBLE_DISPATCH_RECEIPT_DOMAIN_V1, &parts)
        );
    }

    #[test]
    fn combat_broker_arguments_match_the_three_sealed_native_routes() {
        let attacker = MtgoPlayerVisibleCombatBrokerCommandV1::AttackerPlan {
            current_selection_sha256: "a".repeat(64),
            candidate_count: 2,
            desired_mask_hex: "0000000000000003".to_owned(),
            plan_commitment_sha256: "b".repeat(64),
        };
        assert_eq!(
            combat_dispatch_arguments_v1(&attacker).unwrap(),
            vec![
                "--attacker-selection-sha256".to_owned(),
                "a".repeat(64),
                "--candidate-count".to_owned(),
                "2".to_owned(),
                "--desired-mask".to_owned(),
                "0000000000000003".to_owned(),
                "--plan-sha256".to_owned(),
                "b".repeat(64),
            ]
        );

        let single = MtgoPlayerVisibleCombatBrokerCommandV1::SingleAttackerBlockerPlan {
            current_selection_sha256: "c".repeat(64),
            candidate_count: 1,
            desired_mask_hex: "0000000000000001".to_owned(),
            plan_commitment_sha256: "d".repeat(64),
        };
        let single_args = combat_dispatch_arguments_v1(&single).unwrap();
        assert_eq!(single_args[0], "--single-blocker-selection-sha256");
        assert_eq!(single_args[5], "0000000000000001");

        let multi = MtgoPlayerVisibleCombatBrokerCommandV1::MultiAttackerBlockerStep {
            current_selection_sha256: "e".repeat(64),
            selected_index: 3,
            operation_kind: 't',
            blocker_visible_ordinal: Some(4),
            attacker_visible_ordinal: Some(7),
            model_selection_commitment_sha256: "f".repeat(64),
            execution_step_commitment_sha256: "1".repeat(64),
        };
        let multi_args = combat_dispatch_arguments_v1(&multi).unwrap();
        assert_eq!(multi_args[0], "--blocker-selection-sha256");
        assert_eq!(multi_args[5], "t");
        assert_eq!(multi_args[7], "4");
        assert_eq!(multi_args[9], "7");

        let malformed = MtgoPlayerVisibleCombatBrokerCommandV1::MultiAttackerBlockerStep {
            current_selection_sha256: "e".repeat(64),
            selected_index: 3,
            operation_kind: 'b',
            blocker_visible_ordinal: None,
            attacker_visible_ordinal: Some(7),
            model_selection_commitment_sha256: "f".repeat(64),
            execution_step_commitment_sha256: "1".repeat(64),
        };
        assert!(combat_dispatch_arguments_v1(&malformed).is_err());
    }

    #[test]
    fn dispatch_runtime_commitment_is_domain_separated_and_complete() {
        let dispatch = commitment_v1(
            DIRECT_VISIBLE_DISPATCH_RUNTIME_DOMAIN_V1,
            &[b"broker", b"bootstrap", b"producer", b"validator"],
        );
        let source = commitment_v1(
            DIRECT_VISIBLE_SOURCE_RUNTIME_DOMAIN_V1,
            &[b"broker", b"bootstrap", b"producer", b"validator"],
        );
        assert_eq!(dispatch.len(), 64);
        assert_ne!(dispatch, source);
        assert_ne!(
            dispatch,
            commitment_v1(
                DIRECT_VISIBLE_DISPATCH_RUNTIME_DOMAIN_V1,
                &[b"changed", b"bootstrap", b"producer", b"validator"],
            )
        );
    }

    #[test]
    fn visible_postcondition_categories_accept_only_corresponding_provenance_paths() {
        use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;
        assert!(visible_pointer_supports_postcondition_kind_v1(
            "/observation/projection/surface/phase",
            K::PhaseBarChanged,
        ));
        assert!(visible_pointer_supports_postcondition_kind_v1(
            "/observation/projection/surface/battlefield/0/0/card_name",
            K::BattlefieldChanged,
        ));
        assert!(visible_pointer_supports_postcondition_kind_v1(
            "/observation/own_hand/0/card_name",
            K::HandChanged,
        ));
        assert!(visible_pointer_supports_postcondition_kind_v1(
            "/legal_actions/0/action_kind",
            K::ChoiceSurfaceChanged,
        ));
        assert!(!visible_pointer_supports_postcondition_kind_v1(
            "/observation/projection/surface/life_totals/0",
            K::BattlefieldChanged,
        ));
        assert!(!visible_pointer_supports_postcondition_kind_v1(
            "/observation/projection/surface/stack/0/source",
            K::PromptChanged,
        ));
    }

    #[test]
    fn visible_postcondition_category_requests_must_be_nonempty_and_unique() {
        use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;
        assert!(validate_requested_postcondition_categories_v1(&[]).is_err());
        assert!(validate_requested_postcondition_categories_v1(&[
            MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1 {
                kind: K::BattlefieldChanged,
            },
            MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1 {
                kind: K::BattlefieldChanged,
            },
        ])
        .is_err());
        validate_requested_postcondition_categories_v1(&[
            MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1 {
                kind: K::BattlefieldChanged,
            },
            MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1 {
                kind: K::HandChanged,
            },
        ])
        .unwrap();
    }

    #[test]
    fn automatic_postcondition_recipes_bind_the_selected_visible_source_zone() {
        use mtgo_blackbox_v1::{
            MtgoPlayerRelativeRoleV1 as R, MtgoPlayerVisibleDuelActionV1 as A,
            MtgoPlayerVisibleExileCardV1, MtgoPlayerVisibleNamedCardV1,
            MtgoPlayerVisibleObjectRefV1,
        };
        use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;

        let source = MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 7 };
        let action = A::CastSpell {
            actor: R::SeatedPlayer,
            source,
        };
        let mut hand = visible_decision_v1(action.clone());
        hand.current_state
            .own_hand
            .push(MtgoPlayerVisibleNamedCardV1 {
                object_ref: source,
                card_name: "Visible spell".to_owned(),
            });
        assert_eq!(
            direct_visible_postcondition_recipes_v1(&hand, &action).unwrap(),
            vec![vec![K::HandChanged, K::StackChanged]]
        );

        let mut exile = visible_decision_v1(action.clone());
        exile
            .current_state
            .exile
            .push(MtgoPlayerVisibleExileCardV1 {
                object_ref: source,
                zone_owner: R::SeatedPlayer,
                visible_card_name: Some("Visible spell".to_owned()),
            });
        assert_eq!(
            direct_visible_postcondition_recipes_v1(&exile, &action).unwrap(),
            vec![vec![K::ExileChanged, K::StackChanged]]
        );

        let plot = A::PlotSpell {
            actor: R::SeatedPlayer,
            source,
        };
        assert_eq!(
            direct_visible_postcondition_recipes_v1(&hand, &plot).unwrap(),
            vec![vec![K::HandChanged, K::ExileChanged]]
        );
    }

    #[test]
    fn automatic_postcondition_recipes_fail_closed_on_unknown_or_ambiguous_source_zone() {
        use mtgo_blackbox_v1::{
            MtgoPlayerRelativeRoleV1 as R, MtgoPlayerVisibleDuelActionV1 as A,
            MtgoPlayerVisibleExileCardV1, MtgoPlayerVisibleNamedCardV1,
            MtgoPlayerVisibleObjectRefV1,
        };

        let source = MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 9 };
        let action = A::PlayLand {
            actor: R::SeatedPlayer,
            source,
        };
        let mut decision = visible_decision_v1(action.clone());
        assert!(direct_visible_postcondition_recipes_v1(&decision, &action).is_err());
        decision
            .current_state
            .own_hand
            .push(MtgoPlayerVisibleNamedCardV1 {
                object_ref: source,
                card_name: "Visible land".to_owned(),
            });
        decision
            .current_state
            .exile
            .push(MtgoPlayerVisibleExileCardV1 {
                object_ref: source,
                zone_owner: R::SeatedPlayer,
                visible_card_name: Some("Visible land".to_owned()),
            });
        assert!(direct_visible_postcondition_recipes_v1(&decision, &action).is_err());
    }

    #[test]
    fn automatic_postcondition_region_overlap_check_is_conservative() {
        assert!(direct_visible_rects_intersect_v1(
            (0, 0, 10, 10),
            (9, 9, 2, 2)
        ));
        assert!(!direct_visible_rects_intersect_v1(
            (0, 0, 10, 10),
            (10, 0, 2, 2)
        ));
        assert!(direct_visible_rects_intersect_v1(
            (u32::MAX, 0, 1, 1),
            (0, 0, 1, 1)
        ));
    }

    #[test]
    fn visible_region_bounding_is_checked_and_order_independent() {
        let left = MtgoRectPxV1 {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        };
        let right = MtgoRectPxV1 {
            x: 100,
            y: 5,
            width: 20,
            height: 10,
        };
        let expected = MtgoRectPxV1 {
            x: 10,
            y: 5,
            width: 110,
            height: 55,
        };
        assert_eq!(
            bounding_visible_rect_v1([&left, &right].into_iter()),
            Some(expected.clone())
        );
        assert_eq!(
            bounding_visible_rect_v1([&right, &left].into_iter()),
            Some(expected)
        );
        assert!(bounding_visible_rect_v1([].into_iter()).is_none());
        let overflow = MtgoRectPxV1 {
            x: u32::MAX,
            y: 0,
            width: 1,
            height: 1,
        };
        assert!(bounding_visible_rect_v1([&overflow].into_iter()).is_none());
    }

    #[test]
    fn evidence_reachability_is_exact_and_cycle_safe() {
        let rect = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let evidence = vec![
            mtgo_blackbox_v1::MtgoVisibleEvidenceV1 {
                evidence_id: 1,
                sequence: 1,
                source: MtgoEvidenceSourceV1::FrameRegion {
                    frame_id: 7,
                    rect,
                    content_sha256: "a".repeat(64),
                },
            },
            mtgo_blackbox_v1::MtgoVisibleEvidenceV1 {
                evidence_id: 2,
                sequence: 2,
                source: MtgoEvidenceSourceV1::DerivedPublicFact {
                    parent_evidence_ids: vec![1],
                    derivation: mtgo_blackbox_v1::MtgoPublicDerivationV1::PublicStateProjection,
                },
            },
            mtgo_blackbox_v1::MtgoVisibleEvidenceV1 {
                evidence_id: 3,
                sequence: 3,
                source: MtgoEvidenceSourceV1::DerivedPublicFact {
                    parent_evidence_ids: vec![2, 3],
                    derivation: mtgo_blackbox_v1::MtgoPublicDerivationV1::PublicStateProjection,
                },
            },
        ];
        assert!(evidence_reaches_frame_region_v1(
            &evidence,
            2,
            1,
            &mut HashSet::new(),
        ));
        assert!(evidence_reaches_frame_region_v1(
            &evidence,
            3,
            1,
            &mut HashSet::new(),
        ));
        assert!(!evidence_reaches_frame_region_v1(
            &evidence,
            3,
            99,
            &mut HashSet::new(),
        ));
    }
}
