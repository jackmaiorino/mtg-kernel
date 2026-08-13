use super::{
    capture_admitted_mtgo_duel_visible_frame_v1, capture_mtgo_dxgi_frame_candidate_v3,
    sha256_hex_v1, CaptureWindowModeV2, MtgoDxgiCaptureRequestV3,
    OpaqueMtgoAdmittedDuelVisibleFrameV1, OpaqueMtgoDxgiFrameCandidateV3,
};
use mtgo_blackbox_v1::{
    parse_and_validate_visible_duel_producer_result_v1,
    refresh_direct_visible_selection_before_dispatch_v1,
    score_and_select_strict_visible_duel_producer_result_v1, AdmittedMtgoDuelPerceptionProfileV1,
    CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1,
    CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1, MtgoPlayerVisibleDuelScorerV1,
    MtgoVisibleDuelViewModelBrokerAbstentionReasonV1, MtgoVisibleDuelViewModelBrokerResultV1,
};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const LIVE_BROKER_SHA256_V1: &str =
    "6b008edc7c261a3022729f0d7f8ea68eaa727ef86b9b90b2328830c4e121afd9";
const LIVE_BOOTSTRAP_SHA256_V1: &str =
    "1d764382d56fe27aa845acf10b92ee8b9effd79d161baeaace1294a2d01c8c9b";
const LIVE_PRODUCER_SHA256_V1: &str =
    "a99751da026d9e9e0b023c090cb24e06b8399a52bb745f9bea1b1f9be22e53e9";
const LIVE_VALIDATOR_SHA256_V1: &str =
    "e95e60bdf3ff6b4e2347609e79b6b9950152912d92cb6105ccef9dc95085fd16";
const DIRECT_VISIBLE_SOURCE_RUNTIME_DOMAIN_V1: &[u8] = b"mtgo-direct-visible-source-runtime-v1";
const DIRECT_VISIBLE_SOURCE_OBSERVATION_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-source-observation-v1";
const DIRECT_VISIBLE_SOURCE_QUALIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-source-no-stakes-qualification-v1";
const DIRECT_VISIBLE_SOURCE_SCORED_REFRESH_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-source-scored-refresh-v1";
const RATIFIED_DIRECT_VISIBLE_SOURCE_QUALIFICATION_COMMITMENT_V1: Option<&str> = None;
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

/// A player-visible-only scoring outcome that retains the exact admitted live
/// observation that produced it. The outcome is move-only and has no generic
/// checked-untrusted selection extractor.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1;
/// fn cannot_extract_or_act(value: OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1) {
///     let _ = value.generic_selection();
///     value.dispatch();
/// }
/// ```
pub struct OpaqueMtgoRatifiedAttestedDirectVisibleScoringOutcomeV1 {
    _observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    outcome: CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1,
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

/// One selected player-visible action re-observed after scoring through the
/// exact release-pinned producer. It retains both attested observations and
/// the generic refreshed selection privately. There is no generic selection,
/// dispatch, process, event-entry, or spending extractor.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1;
/// fn cannot_extract_or_act(value: OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1) {
///     let _ = value.generic_selection();
///     value.dispatch();
/// }
/// ```
pub struct OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1 {
    _initial_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    _refreshed_observation: OpaqueMtgoAttestedDirectVisibleSourceObservationV1,
    selection: CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1,
    commitments: MtgoRefreshedAttestedDirectVisibleSelectionCommitmentsV1,
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

/// Consumes one profile-admitted, execution-attested observation into the
/// player-visible-only scorer only after the exact no-stakes producer
/// qualification commitment has been pinned in this build. The production
/// ratification root is currently empty, so the scorer cannot yet be called.
/// The qualification-only observation type has no corresponding scoring path.
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

fn require_ratified_direct_visible_source_qualification_v1(
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
        _initial_observation: initial_observation,
        _refreshed_observation: refreshed_observation,
        selection,
        commitments,
    })
}

impl OpaqueMtgoAttestedDirectVisibleSourceObservationV1 {
    pub fn commitments_v1(&self) -> MtgoAttestedDirectVisibleSourceObservationCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn abstention_reason_v1(&self) -> Option<MtgoVisibleDuelViewModelBrokerAbstentionReasonV1> {
        match self.result {
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained { reason } => Some(reason),
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { .. } => None,
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

impl OpaqueMtgoQualifiedDirectVisibleSourceObservationV1 {
    pub fn commitments_v1(&self) -> MtgoQualifiedDirectVisibleSourceObservationCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn abstention_reason_v1(&self) -> Option<MtgoVisibleDuelViewModelBrokerAbstentionReasonV1> {
        match self.result {
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained { reason } => Some(reason),
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { .. } => None,
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

fn invoke_observe_only_broker_v1(
    runtime: &OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1,
    process_id: u32,
    timeout: Duration,
) -> Result<ZeroingVecV1, String> {
    let mut child = Command::new(&runtime.broker_path)
        .arg("--pid")
        .arg(process_id.to_string())
        .arg("--bootstrap")
        .arg(&runtime.bootstrap_path)
        .arg("--producer")
        .arg(&runtime.producer_path)
        .arg("--validator")
        .arg(&runtime.validator_path)
        .current_dir(
            runtime
                .broker_path
                .parent()
                .ok_or("direct-source broker has no parent directory")?,
        )
        .env_clear()
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
    let (status, stdout, stdout_truncated, stderr_truncated) = thread::scope(|scope| {
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
        drop(stderr);
        Ok::<_, String>((status, stdout, stdout_truncated, stderr_truncated))
    })?;
    validate_broker_process_result_v1(status, stdout, stdout_truncated, stderr_truncated)
}

fn validate_broker_process_result_v1(
    status: ExitStatus,
    output: ZeroingVecV1,
    output_truncated: bool,
    stderr_truncated: bool,
) -> Result<ZeroingVecV1, String> {
    if !status.success()
        || output.0.is_empty()
        || output_truncated
        || stderr_truncated
        || output.0.last() != Some(&b'\n')
        || output.0[..output.0.len().saturating_sub(1)].contains(&b'\n')
        || output.0.contains(&b'\r')
    {
        return Err("release-pinned direct-source broker failed closed".to_owned());
    }
    let mut output = output;
    output.0.pop();
    Ok(output)
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
    let before_commitments = before.commitments_v3();
    let after_commitments = after.commitments_v3();
    let before_manifest = &before.manifest;
    let after_manifest = &after.manifest;
    if before_commitments.capture_commitment_sha256 == after_commitments.capture_commitment_sha256
        || after_commitments.captured_at_unix_millis <= before_commitments.captured_at_unix_millis
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
            "direct-source qualification changed the duel process, window, output, format, or geometry"
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
    }

    #[test]
    fn runtime_verification_rejects_unpinned_relative_artifacts() {
        let relative = Path::new("not-a-release-artifact");
        assert!(
            verify_direct_visible_source_runtime_v1(relative, relative, relative, relative,)
                .is_err()
        );
    }

    #[test]
    fn production_scoring_ratification_root_is_empty() {
        assert!(require_ratified_direct_visible_source_qualification_v1(&"a".repeat(64)).is_err());
    }
}
