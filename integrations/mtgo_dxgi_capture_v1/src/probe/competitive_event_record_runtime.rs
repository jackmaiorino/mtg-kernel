use super::{
    competitive_navigation_classifier_assets_manifest_bytes_v1,
    invoke_verified_competitive_event_record_classifier_process_v1, sha256_hex_v1,
    verify_runtime_identity_now_v1, MtgoCompetitiveNavigationFrameIdentityV1,
    OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
};
use mtgo_blackbox_v1::{
    validate_visible_competitive_event_record_v1,
    validate_visible_competitive_lifecycle_snapshot_v1, visible_frame_region_content_sha256_v1,
    AdmittedMtgoCompetitiveNavigationProfileV1, CheckedUntrustedMtgoCompetitiveEventRecordV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoCompetitiveEventCompletionV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveEventProgressV1,
    MtgoCompetitiveEventRecordVisibleFactV1, MtgoCompetitiveEventVisibleStatusV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoCompetitiveNavigationProfileScopeV1, MtgoSizePxV1,
    MtgoVisibleCompetitiveEventRecordV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

const SOURCE_BOUND_COMPETITIVE_EVENT_RECORD_DOMAIN_V1: &[u8] =
    b"mtgo-source-bound-competitive-event-record-v1";
const COMPETITIVE_EVENT_RECORD_CLASSIFIER_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-record-classifier-request-v1";
const COMPETITIVE_EVENT_RECORD_CLASSIFIER_RESULT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-record-classifier-result-v1";
const COMPETITIVE_EVENT_MONITOR_CHAIN_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-monitor-chain-v1";
const MAX_EVENT_RECORD_REQUEST_HEADER_BYTES_V1: usize = 1024 * 1024;
const MAX_EVENT_RECORD_ASSETS_MANIFEST_BYTES_V1: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoSourceBoundCompetitiveEventRecordCommitmentsV1 {
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub source_classification_result_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub event_identity_sha256: String,
    pub event_record_commitment_sha256: String,
    pub source_binding_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub status: MtgoCompetitiveEventVisibleStatusV1,
}

/// Canonical JSON header followed by the exact classifier assets and tightly
/// packed BGRA8 frame in the private event-record parser protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventRecordClassifierRequestHeaderV1 {
    pub schema_version: u32,
    pub protocol: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub canonical_stride: u32,
    pub canonical_byte_length: u64,
    pub canonical_bgra8_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub parser_scope: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub classifier_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventRecordClassifierProcessResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub record: MtgoVisibleCompetitiveEventRecordV1,
}

/// Structurally checked request bytes. This retains no pixels and does not
/// prove that a request came from the opaque capture path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoCompetitiveEventRecordClassifierRequestV1 {
    header: MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventRecordClassifierRequestV1 {
    pub fn header_v1(&self) -> &MtgoCompetitiveEventRecordClassifierRequestHeaderV1 {
        &self.header
    }

    pub fn request_commitment_sha256_v1(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifiedCompetitiveEventRecordCommitmentsV1 {
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub request_commitment_sha256: String,
    pub classifier_response_sha256: String,
    pub lifecycle_snapshot_commitment_sha256: String,
    pub event_record_commitment_sha256: String,
    pub classification_result_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub event_identity_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub status: MtgoCompetitiveEventVisibleStatusV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventMonitorCommitmentsV1 {
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub event_identity_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub current_status: MtgoCompetitiveEventVisibleStatusV1,
    pub current_event_record_commitment_sha256: String,
    pub current_classification_result_commitment_sha256: String,
    pub monitor_chain_commitment_sha256: String,
    pub observation_count: u64,
    pub last_frame_id: u64,
    pub last_frame_sequence: u64,
    pub last_captured_at_unix_millis: u128,
    pub terminal: bool,
}

/// One exact opaque navigation frame passed through the bounded event-record
/// parser and the same-frame pixel rehash. The parser is not ratified by the
/// current build, so this value remains monitoring-only and non-actionable.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveEventRecordV1;
/// fn cannot_act(value: &OpaqueMtgoClassifiedCompetitiveEventRecordV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.visible_fact_rectangles();
///     let _ = value.input_command();
///     let _ = value.enter_event();
/// }
/// ```
pub struct OpaqueMtgoClassifiedCompetitiveEventRecordV1 {
    _source_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    _lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    _record: CheckedUntrustedMtgoCompetitiveEventRecordV1,
    commitments: MtgoClassifiedCompetitiveEventRecordCommitmentsV1,
}

impl OpaqueMtgoClassifiedCompetitiveEventRecordV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitiveEventRecordCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn status_v1(&self) -> MtgoCompetitiveEventVisibleStatusV1 {
        self.commitments.status
    }

    pub fn progress_v1(&self) -> &MtgoCompetitiveEventProgressV1 {
        self._record.progress_v1()
    }

    pub fn completion_v1(&self) -> Option<MtgoCompetitiveEventCompletionV1> {
        self._record.completion_v1()
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_gameplay_v1(&self) -> bool {
        false
    }
}

/// A move-only checked-untrusted monitor for one exact League or Challenge
/// event. Every advance consumes both the prior monitor and next classified
/// record. Identity drift, stale frames, progress regression, and updates
/// after terminal completion fail closed and return neither input nor entry
/// authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventMonitorV1;
/// let _forged = OpaqueMtgoCompetitiveEventMonitorV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventMonitorV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoCompetitiveEventMonitorV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventMonitorV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveEventMonitorV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventMonitorV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<OpaqueMtgoCompetitiveEventMonitorV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventMonitorV1;
/// fn cannot_act(value: &OpaqueMtgoCompetitiveEventMonitorV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.input_command();
///     let _ = value.enter_event();
///     let _ = value.play_game();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveEventMonitorV1 {
    _current: OpaqueMtgoClassifiedCompetitiveEventRecordV1,
    commitments: MtgoCompetitiveEventMonitorCommitmentsV1,
}

impl OpaqueMtgoCompetitiveEventMonitorV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEventMonitorCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn status_v1(&self) -> MtgoCompetitiveEventVisibleStatusV1 {
        self.commitments.current_status
    }

    pub fn progress_v1(&self) -> &MtgoCompetitiveEventProgressV1 {
        self._current.progress_v1()
    }

    pub fn completion_v1(&self) -> Option<MtgoCompetitiveEventCompletionV1> {
        self._current.completion_v1()
    }

    pub fn terminal_v1(&self) -> bool {
        self.commitments.terminal
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn permits_gameplay_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn begin_checked_untrusted_competitive_event_monitor_v1(
    current: OpaqueMtgoClassifiedCompetitiveEventRecordV1,
) -> Result<OpaqueMtgoCompetitiveEventMonitorV1, String> {
    let classified = current.commitments_v1();
    let observation = event_monitor_observation_v1(&current);
    validate_event_monitor_observation_v1(&observation)?;
    let monitor_chain_commitment_sha256 = commitment_parts_v1(
        COMPETITIVE_EVENT_MONITOR_CHAIN_DOMAIN_V1,
        &[
            classified
                .classification_result_commitment_sha256
                .as_bytes(),
            classified.event_record_commitment_sha256.as_bytes(),
            &1_u64.to_be_bytes(),
            b"checked_untrusted_event_monitor_begin_no_entry_no_spending_no_gameplay_no_input",
        ],
    );
    Ok(OpaqueMtgoCompetitiveEventMonitorV1 {
        commitments: event_monitor_commitments_v1(&classified, monitor_chain_commitment_sha256, 1),
        _current: current,
    })
}

pub fn advance_checked_untrusted_competitive_event_monitor_v1(
    monitor: OpaqueMtgoCompetitiveEventMonitorV1,
    next: OpaqueMtgoClassifiedCompetitiveEventRecordV1,
) -> Result<OpaqueMtgoCompetitiveEventMonitorV1, String> {
    if monitor.commitments.terminal {
        return Err(
            "a completed competitive event monitor cannot accept another record".to_owned(),
        );
    }
    let prior_observation = event_monitor_observation_v1(&monitor._current);
    let next_observation = event_monitor_observation_v1(&next);
    validate_event_monitor_observation_v1(&next_observation)?;
    validate_event_monitor_advance_v1(&prior_observation, &next_observation)?;
    let next_classified = next.commitments_v1();
    let observation_count = monitor
        .commitments
        .observation_count
        .checked_add(1)
        .ok_or("competitive event monitor observation count overflow")?;
    let monitor_chain_commitment_sha256 = commitment_parts_v1(
        COMPETITIVE_EVENT_MONITOR_CHAIN_DOMAIN_V1,
        &[
            monitor
                .commitments
                .monitor_chain_commitment_sha256
                .as_bytes(),
            next_classified
                .classification_result_commitment_sha256
                .as_bytes(),
            next_classified.event_record_commitment_sha256.as_bytes(),
            &observation_count.to_be_bytes(),
            b"checked_untrusted_event_monitor_advance_no_entry_no_spending_no_gameplay_no_input",
        ],
    );
    Ok(OpaqueMtgoCompetitiveEventMonitorV1 {
        commitments: event_monitor_commitments_v1(
            &next_classified,
            monitor_chain_commitment_sha256,
            observation_count,
        ),
        _current: next,
    })
}

/// One checked event record whose complete visible fact set has been rehashed
/// against the exact retained classifier frame. The frame and coordinates stay
/// private. The value exposes coordinate-free progress only and grants no
/// input, event entry, spending, or gameplay authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSourceBoundCompetitiveEventRecordV1;
/// let _forged = OpaqueMtgoSourceBoundCompetitiveEventRecordV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSourceBoundCompetitiveEventRecordV1;
/// fn cannot_extract(value: &OpaqueMtgoSourceBoundCompetitiveEventRecordV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.visible_fact_rectangles();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoSourceBoundCompetitiveEventRecordV1 {
    _source_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    _record: CheckedUntrustedMtgoCompetitiveEventRecordV1,
    commitments: MtgoSourceBoundCompetitiveEventRecordCommitmentsV1,
}

impl OpaqueMtgoSourceBoundCompetitiveEventRecordV1 {
    pub fn commitments_v1(&self) -> MtgoSourceBoundCompetitiveEventRecordCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn status_v1(&self) -> MtgoCompetitiveEventVisibleStatusV1 {
        self.commitments.status
    }

    pub fn progress_v1(&self) -> &MtgoCompetitiveEventProgressV1 {
        self._record.progress_v1()
    }

    pub fn completion_v1(&self) -> Option<MtgoCompetitiveEventCompletionV1> {
        self._record.completion_v1()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn permits_gameplay_v1(&self) -> bool {
        false
    }
}

/// Consumes one exact classified main-client frame and binds a visible League
/// or Challenge record only after rehashing every declared record fact against
/// that frame's retained composed-desktop pixels.
pub fn bind_classified_navigation_frame_to_visible_event_record_v1(
    source: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    record: MtgoVisibleCompetitiveEventRecordV1,
) -> Result<OpaqueMtgoSourceBoundCompetitiveEventRecordV1, String> {
    let source_commitments = source.commitments_v1();
    let approved_account_alias_sha256 = source_commitments
        .source_frame
        .approved_account_alias_sha256
        .clone();
    let checked = validate_visible_competitive_event_record_v1(
        &source._lifecycle,
        &approved_account_alias_sha256,
        record,
    )
    .map_err(|error| format!("validate visible competitive event record: {error}"))?;

    let retained = &source._source_frame.source_frame;
    let size = MtgoSizePxV1 {
        width: retained.manifest.frame.canonical_width,
        height: retained.manifest.frame.canonical_height,
    };
    validate_event_record_visible_fact_pixels_v1(
        &retained.canonical_bgra8,
        &size,
        checked.visible_facts_v1(),
    )?;
    if retained.capture_commitment_sha256
        != source_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256
        || sha256_hex_v1(&retained.canonical_bgra8)
            != source_commitments
                .source_frame
                .source_capture
                .canonical_bgra8_sha256
    {
        return Err("event-record source capture changed before pixel binding".to_owned());
    }

    let event_record_commitment_sha256 = checked.record_commitment_sha256_v1().to_owned();
    let event_identity_sha256 = checked.event_identity_sha256_v1().to_owned();
    let source_binding_commitment_sha256 = source_bound_event_record_commitment_v1(
        &source_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        &source_commitments.source_frame.frame_profile_binding_sha256,
        &source_commitments.classification_result_commitment_sha256,
        checked.source_lifecycle_snapshot_commitment_sha256_v1(),
        checked.approved_account_alias_sha256_v1(),
        &event_identity_sha256,
        &event_record_commitment_sha256,
    );
    Ok(OpaqueMtgoSourceBoundCompetitiveEventRecordV1 {
        _source_frame: source,
        commitments: MtgoSourceBoundCompetitiveEventRecordCommitmentsV1 {
            source_capture_commitment_sha256: source_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256,
            source_frame_profile_binding_sha256: source_commitments
                .source_frame
                .frame_profile_binding_sha256,
            source_classification_result_commitment_sha256: source_commitments
                .classification_result_commitment_sha256,
            source_lifecycle_snapshot_commitment_sha256: checked
                .source_lifecycle_snapshot_commitment_sha256_v1()
                .to_owned(),
            approved_account_alias_sha256,
            event_identity_sha256,
            event_record_commitment_sha256,
            source_binding_commitment_sha256,
            event_kind: checked.event_kind_v1(),
            status: checked.status_v1(),
        },
        _record: checked,
    })
}

/// Checks the canonical event-record parser request, exact asset bytes, and
/// exact BGRA8 payload. Success remains structural and non-authorizing.
pub fn check_untrusted_competitive_event_record_classifier_request_v1(
    canonical_header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveEventRecordClassifierRequestV1, String> {
    if canonical_header_json.is_empty()
        || canonical_header_json.len() > MAX_EVENT_RECORD_REQUEST_HEADER_BYTES_V1
        || classifier_assets_manifest.is_empty()
        || classifier_assets_manifest.len() > MAX_EVENT_RECORD_ASSETS_MANIFEST_BYTES_V1
    {
        return Err(
            "event-record classifier request header or assets are outside bounds".to_owned(),
        );
    }
    let header: MtgoCompetitiveEventRecordClassifierRequestHeaderV1 =
        serde_json::from_slice(canonical_header_json)
            .map_err(|error| format!("parse event-record classifier request header: {error}"))?;
    let canonical = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize event-record classifier request header: {error}"))?;
    if canonical != canonical_header_json {
        return Err("event-record classifier request header is not canonical JSON".to_owned());
    }
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_competitive_event_record_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
        || header.parser_scope
            != "league_and_challenge_eight_slice_event_record_checked_untrusted_v1"
    {
        return Err("event-record classifier request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("event-record classifier stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("event-record classifier byte length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || usize::try_from(expected_length).ok() != Some(canonical_bgra8.len())
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(classifier_assets_manifest)
    {
        return Err("event-record classifier pixels or assets differ from the header".to_owned());
    }
    for value in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.source_frame_profile_binding_sha256.as_str(),
        header.navigation_profile_commitment_sha256.as_str(),
        header
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        header.approved_account_alias_sha256.as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.classifier_binary_sha256.as_str(),
        header.classifier_assets_manifest_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256(value) {
            return Err(
                "event-record classifier request contains an invalid commitment".to_owned(),
            );
        }
    }
    let request_commitment_sha256 = commitment_parts_v1(
        COMPETITIVE_EVENT_RECORD_CLASSIFIER_REQUEST_DOMAIN_V1,
        &[
            canonical_header_json,
            classifier_assets_manifest,
            canonical_bgra8,
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitiveEventRecordClassifierRequestV1 {
            header,
            request_commitment_sha256,
        },
    )
}

/// Runs the exact profile-pinned classifier binary in its event-record mode
/// over one opaque main-client frame. The strict response supplies both the
/// lifecycle and event record, and every declared region is rehashed against
/// the same retained pixels. The output remains checked-untrusted until a
/// real eight-slice evaluation is separately ratified.
pub fn classify_checked_untrusted_competitive_event_record_v1(
    source: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    identity: MtgoCompetitiveNavigationFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitiveEventRecordV1, String> {
    if identity.frame_id == 0 || identity.frame_sequence == 0 {
        return Err("event-record classifier frame identity must be nonzero".to_owned());
    }
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("event-record classifier timeout must be between 100 and 60000 ms".to_owned());
    }
    if profile.scope()
        != MtgoCompetitiveNavigationProfileScopeV1::LeagueAndChallengeLifecycleClassification
    {
        return Err("navigation profile does not cover the exact classifier runtime".to_owned());
    }
    let source_commitments = source.commitments_v1();
    let checked_profile = profile.checked_runtime_profile();
    let runtime_commitments = runtime.commitments_v1();
    if source_commitments.profile_commitment_sha256 != profile.profile_commitment_sha256()
        || source_commitments.profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || source_commitments.approved_account_alias_sha256
            != checked_profile.approved_account_alias_sha256()
        || runtime_commitments.navigation_profile_commitment_sha256
            != profile.profile_commitment_sha256()
        || runtime_commitments.navigation_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || runtime_commitments.approved_account_alias_sha256
            != checked_profile.approved_account_alias_sha256()
    {
        return Err("event-record source, runtime, and admitted profile differ".to_owned());
    }
    verify_runtime_identity_now_v1(runtime)?;

    let raw_source = &source.source_frame;
    let width = raw_source.manifest.frame.canonical_width;
    let height = raw_source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("event-record classifier canonical stride overflow")?;
    let byte_length = u64::try_from(raw_source.canonical_bgra8.len())
        .map_err(|_| "event-record classifier canonical byte length overflow")?;
    if raw_source.manifest.frame.canonical_stride != stride
        || raw_source.manifest.frame.canonical_byte_length != raw_source.canonical_bgra8.len()
        || raw_source.manifest.frame.canonical_bgra8_sha256
            != sha256_hex_v1(&raw_source.canonical_bgra8)
        || raw_source.capture_commitment_sha256
            != source_commitments.source_capture.capture_commitment_sha256
        || raw_source.manifest.frame.canonical_bgra8_sha256
            != source_commitments.source_capture.canonical_bgra8_sha256
    {
        return Err(
            "opaque event-record source pixels no longer match capture metadata".to_owned(),
        );
    }
    let classifier_assets_manifest =
        competitive_navigation_classifier_assets_manifest_bytes_v1(runtime);
    let header = MtgoCompetitiveEventRecordClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_event_record_v1".to_owned(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
        captured_at_unix_millis: raw_source.manifest.captured_at_unix_millis,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: byte_length,
        canonical_bgra8_sha256: raw_source.manifest.frame.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: raw_source.capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: source_commitments
            .frame_profile_binding_sha256
            .clone(),
        parser_scope: "league_and_challenge_eight_slice_event_record_checked_untrusted_v1"
            .to_owned(),
        navigation_profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
        navigation_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        approved_account_alias_sha256: checked_profile.approved_account_alias_sha256().to_owned(),
        runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256
            .clone(),
        classifier_binary_sha256: checked_profile.classifier_binary_sha256().to_owned(),
        classifier_assets_manifest_sha256: checked_profile
            .classifier_assets_manifest_sha256()
            .to_owned(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize event-record classifier request: {error}"))?;
    let checked_request = check_untrusted_competitive_event_record_classifier_request_v1(
        &header_json,
        classifier_assets_manifest,
        &raw_source.canonical_bgra8,
    )?;
    let request_commitment_sha256 = checked_request.request_commitment_sha256_v1().to_owned();
    let response_bytes = invoke_verified_competitive_event_record_classifier_process_v1(
        runtime,
        &header_json,
        classifier_assets_manifest,
        &raw_source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_runtime_identity_now_v1(runtime)?;
    let response =
        parse_event_record_classifier_response_v1(&response_bytes, &request_commitment_sha256)?;
    if response.lifecycle.frame_id != identity.frame_id
        || response.lifecycle.frame_sequence != identity.frame_sequence
    {
        return Err("event-record classifier response changed the frame identity".to_owned());
    }
    let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(response.lifecycle.clone())
        .map_err(|error| format!("validate event-record lifecycle response: {error}"))?;
    if !event_record_phase_supported_v1(lifecycle.phase())
        || lifecycle.frame_sha256_v1() != raw_source.manifest.frame.canonical_bgra8_sha256
        || lifecycle.client_bounds_v1().x != 0
        || lifecycle.client_bounds_v1().y != 0
        || lifecycle.client_bounds_v1().width != width
        || lifecycle.client_bounds_v1().height != height
    {
        return Err(
            "event-record lifecycle does not bind the exact supported source frame".to_owned(),
        );
    }
    rehash_lifecycle_visible_facts_for_event_record_v1(
        &response.lifecycle,
        &raw_source.canonical_bgra8,
        &MtgoSizePxV1 { width, height },
    )?;
    let record = validate_visible_competitive_event_record_v1(
        &lifecycle,
        checked_profile.approved_account_alias_sha256(),
        response.record,
    )
    .map_err(|error| format!("validate event-record classifier response: {error}"))?;
    validate_event_record_visible_fact_pixels_v1(
        &raw_source.canonical_bgra8,
        &MtgoSizePxV1 { width, height },
        record.visible_facts_v1(),
    )?;
    let classifier_response_sha256 = sha256_hex_v1(&response_bytes);
    let lifecycle_snapshot_commitment_sha256 = lifecycle.snapshot_commitment_sha256().to_owned();
    let event_record_commitment_sha256 = record.record_commitment_sha256_v1().to_owned();
    let classification_result_commitment_sha256 = commitment_parts_v1(
        COMPETITIVE_EVENT_RECORD_CLASSIFIER_RESULT_DOMAIN_V1,
        &[
            profile.profile_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            runtime_commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            source_commitments
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            source_commitments.frame_profile_binding_sha256.as_bytes(),
            request_commitment_sha256.as_bytes(),
            classifier_response_sha256.as_bytes(),
            lifecycle_snapshot_commitment_sha256.as_bytes(),
            event_record_commitment_sha256.as_bytes(),
            b"checked_untrusted_event_record_parser_no_ratification_no_entry_no_spending_no_gameplay_no_input",
        ],
    );
    Ok(OpaqueMtgoClassifiedCompetitiveEventRecordV1 {
        commitments: MtgoClassifiedCompetitiveEventRecordCommitmentsV1 {
            navigation_profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            navigation_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            runtime_identity_commitment_sha256: runtime_commitments
                .runtime_identity_commitment_sha256,
            source_capture_commitment_sha256: source_commitments
                .source_capture
                .capture_commitment_sha256,
            source_frame_profile_binding_sha256: source_commitments.frame_profile_binding_sha256,
            request_commitment_sha256,
            classifier_response_sha256,
            lifecycle_snapshot_commitment_sha256,
            event_record_commitment_sha256,
            classification_result_commitment_sha256,
            approved_account_alias_sha256: checked_profile
                .approved_account_alias_sha256()
                .to_owned(),
            event_identity_sha256: record.event_identity_sha256_v1().to_owned(),
            frame_id: identity.frame_id,
            frame_sequence: identity.frame_sequence,
            captured_at_unix_millis: raw_source.manifest.captured_at_unix_millis,
            event_kind: record.event_kind_v1(),
            status: record.status_v1(),
        },
        _source_frame: source,
        _lifecycle: lifecycle,
        _record: record,
    })
}

fn parse_event_record_classifier_response_v1(
    response_bytes: &[u8],
    request_commitment_sha256: &str,
) -> Result<MtgoCompetitiveEventRecordClassifierProcessResponseV1, String> {
    let response: MtgoCompetitiveEventRecordClassifierProcessResponseV1 =
        serde_json::from_slice(response_bytes).map_err(|error| {
            format!("event-record classifier response is not one strict JSON value: {error}")
        })?;
    let canonical = serde_json::to_vec(&response)
        .map_err(|error| format!("serialize event-record classifier response: {error}"))?;
    if canonical != response_bytes {
        return Err("event-record classifier response is not canonical JSON".to_owned());
    }
    if response.schema_version != 1
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("event-record classifier response does not bind the exact request".to_owned());
    }
    Ok(response)
}

fn rehash_lifecycle_visible_facts_for_event_record_v1(
    lifecycle: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), String> {
    for fact in &lifecycle.facts {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &fact.rect_client_px)
                .map_err(|error| format!("rehash event-record lifecycle fact: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "event-record lifecycle fact does not match the retained source pixels".to_owned(),
            );
        }
    }
    Ok(())
}

fn event_record_phase_supported_v1(phase: MtgoCompetitiveLifecyclePhaseV1) -> bool {
    matches!(
        phase,
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
            | MtgoCompetitiveLifecyclePhaseV1::PairingReady
            | MtgoCompetitiveLifecyclePhaseV1::MatchComplete
            | MtgoCompetitiveLifecyclePhaseV1::EventComplete
    )
}

#[derive(Clone)]
struct MtgoCompetitiveEventMonitorObservationV1 {
    navigation_profile_commitment_sha256: String,
    navigation_profile_admission_commitment_sha256: String,
    runtime_identity_commitment_sha256: String,
    approved_account_alias_sha256: String,
    event_identity_sha256: String,
    source_capture_commitment_sha256: String,
    source_frame_profile_binding_sha256: String,
    classification_result_commitment_sha256: String,
    event_record_commitment_sha256: String,
    frame_id: u64,
    frame_sequence: u64,
    captured_at_unix_millis: u128,
    event_kind: MtgoCompetitiveEventKindV1,
    status: MtgoCompetitiveEventVisibleStatusV1,
    progress: MtgoCompetitiveEventProgressV1,
    completion: Option<MtgoCompetitiveEventCompletionV1>,
}

fn event_monitor_observation_v1(
    value: &OpaqueMtgoClassifiedCompetitiveEventRecordV1,
) -> MtgoCompetitiveEventMonitorObservationV1 {
    let commitments = value.commitments_v1();
    MtgoCompetitiveEventMonitorObservationV1 {
        navigation_profile_commitment_sha256: commitments.navigation_profile_commitment_sha256,
        navigation_profile_admission_commitment_sha256: commitments
            .navigation_profile_admission_commitment_sha256,
        runtime_identity_commitment_sha256: commitments.runtime_identity_commitment_sha256,
        approved_account_alias_sha256: commitments.approved_account_alias_sha256,
        event_identity_sha256: commitments.event_identity_sha256,
        source_capture_commitment_sha256: commitments.source_capture_commitment_sha256,
        source_frame_profile_binding_sha256: commitments.source_frame_profile_binding_sha256,
        classification_result_commitment_sha256: commitments
            .classification_result_commitment_sha256,
        event_record_commitment_sha256: commitments.event_record_commitment_sha256,
        frame_id: commitments.frame_id,
        frame_sequence: commitments.frame_sequence,
        captured_at_unix_millis: commitments.captured_at_unix_millis,
        event_kind: commitments.event_kind,
        status: commitments.status,
        progress: value.progress_v1().clone(),
        completion: value.completion_v1(),
    }
}

fn event_monitor_commitments_v1(
    classified: &MtgoClassifiedCompetitiveEventRecordCommitmentsV1,
    monitor_chain_commitment_sha256: String,
    observation_count: u64,
) -> MtgoCompetitiveEventMonitorCommitmentsV1 {
    MtgoCompetitiveEventMonitorCommitmentsV1 {
        navigation_profile_commitment_sha256: classified
            .navigation_profile_commitment_sha256
            .clone(),
        navigation_profile_admission_commitment_sha256: classified
            .navigation_profile_admission_commitment_sha256
            .clone(),
        runtime_identity_commitment_sha256: classified.runtime_identity_commitment_sha256.clone(),
        approved_account_alias_sha256: classified.approved_account_alias_sha256.clone(),
        event_identity_sha256: classified.event_identity_sha256.clone(),
        event_kind: classified.event_kind,
        current_status: classified.status,
        current_event_record_commitment_sha256: classified.event_record_commitment_sha256.clone(),
        current_classification_result_commitment_sha256: classified
            .classification_result_commitment_sha256
            .clone(),
        monitor_chain_commitment_sha256,
        observation_count,
        last_frame_id: classified.frame_id,
        last_frame_sequence: classified.frame_sequence,
        last_captured_at_unix_millis: classified.captured_at_unix_millis,
        terminal: classified.status == MtgoCompetitiveEventVisibleStatusV1::EventComplete,
    }
}

fn validate_event_monitor_observation_v1(
    value: &MtgoCompetitiveEventMonitorObservationV1,
) -> Result<(), String> {
    for digest in [
        value.navigation_profile_commitment_sha256.as_str(),
        value
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        value.runtime_identity_commitment_sha256.as_str(),
        value.approved_account_alias_sha256.as_str(),
        value.event_identity_sha256.as_str(),
        value.source_capture_commitment_sha256.as_str(),
        value.source_frame_profile_binding_sha256.as_str(),
        value.classification_result_commitment_sha256.as_str(),
        value.event_record_commitment_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256(digest) {
            return Err(
                "competitive event monitor observation has an invalid commitment".to_owned(),
            );
        }
    }
    if value.frame_id == 0
        || value.frame_sequence == 0
        || value.captured_at_unix_millis == 0
        || (value.status == MtgoCompetitiveEventVisibleStatusV1::EventComplete)
            != value.completion.is_some()
    {
        return Err("competitive event monitor observation is internally inconsistent".to_owned());
    }
    match (value.event_kind, &value.progress) {
        (MtgoCompetitiveEventKindV1::League, MtgoCompetitiveEventProgressV1::League { .. })
        | (
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveEventProgressV1::Challenge { .. },
        ) => Ok(()),
        _ => Err("competitive event monitor progress uses the wrong event mode".to_owned()),
    }
}

fn validate_event_monitor_advance_v1(
    prior: &MtgoCompetitiveEventMonitorObservationV1,
    next: &MtgoCompetitiveEventMonitorObservationV1,
) -> Result<(), String> {
    if prior.navigation_profile_commitment_sha256 != next.navigation_profile_commitment_sha256
        || prior.navigation_profile_admission_commitment_sha256
            != next.navigation_profile_admission_commitment_sha256
        || prior.runtime_identity_commitment_sha256 != next.runtime_identity_commitment_sha256
        || prior.approved_account_alias_sha256 != next.approved_account_alias_sha256
        || prior.event_identity_sha256 != next.event_identity_sha256
        || prior.event_kind != next.event_kind
    {
        return Err("competitive event monitor identity changed across records".to_owned());
    }
    if next.frame_id == prior.frame_id
        || next.frame_sequence <= prior.frame_sequence
        || next.captured_at_unix_millis < prior.captured_at_unix_millis
        || next.source_capture_commitment_sha256 == prior.source_capture_commitment_sha256
        || next.source_frame_profile_binding_sha256 == prior.source_frame_profile_binding_sha256
        || next.classification_result_commitment_sha256
            == prior.classification_result_commitment_sha256
        || next.event_record_commitment_sha256 == prior.event_record_commitment_sha256
    {
        return Err(
            "competitive event monitor requires a changed strictly newer record".to_owned(),
        );
    }
    if !event_monitor_status_advance_allowed_v1(prior.status, next.status) {
        return Err("competitive event monitor status transition is not allowed".to_owned());
    }
    let completed_progress_delta =
        validate_event_monitor_progress_advance_v1(&prior.progress, &next.progress)?;
    if (completed_progress_delta > 0
        && !matches!(
            (prior.status, next.status),
            (
                MtgoCompetitiveEventVisibleStatusV1::PairingReady,
                MtgoCompetitiveEventVisibleStatusV1::BetweenMatches
                    | MtgoCompetitiveEventVisibleStatusV1::EventComplete
            )
        ))
        || (prior.status == MtgoCompetitiveEventVisibleStatusV1::PairingReady
            && next.status == MtgoCompetitiveEventVisibleStatusV1::BetweenMatches
            && completed_progress_delta != 1)
    {
        return Err(
            "competitive event record changed progress outside one visible match completion"
                .to_owned(),
        );
    }
    Ok(())
}

fn event_monitor_status_advance_allowed_v1(
    prior: MtgoCompetitiveEventVisibleStatusV1,
    next: MtgoCompetitiveEventVisibleStatusV1,
) -> bool {
    use MtgoCompetitiveEventVisibleStatusV1::*;
    matches!(
        (prior, next),
        (
            WaitingForPairing,
            WaitingForPairing | PairingReady | EventComplete
        ) | (
            PairingReady,
            PairingReady | WaitingForPairing | BetweenMatches | EventComplete
        ) | (
            BetweenMatches,
            BetweenMatches | WaitingForPairing | PairingReady | EventComplete
        )
    )
}

fn validate_event_monitor_progress_advance_v1(
    prior: &MtgoCompetitiveEventProgressV1,
    next: &MtgoCompetitiveEventProgressV1,
) -> Result<u16, String> {
    match (prior, next) {
        (
            MtgoCompetitiveEventProgressV1::League {
                match_record: prior_record,
                matches_total: prior_total,
            },
            MtgoCompetitiveEventProgressV1::League {
                match_record: next_record,
                matches_total: next_total,
            },
        ) => {
            let match_delta =
                validate_event_monitor_match_record_advance_v1(prior_record, next_record)?;
            match (prior_total, next_total) {
                (Some(prior), Some(next)) if prior != next => {
                    return Err("competitive League match total changed".to_owned());
                }
                (Some(_), None) => {
                    return Err("competitive League match total disappeared".to_owned());
                }
                _ => {}
            }
            Ok(match_delta)
        }
        (
            MtgoCompetitiveEventProgressV1::Challenge {
                match_record: prior_record,
                rounds_completed: prior_rounds_completed,
                rounds_total: prior_rounds_total,
                match_points: prior_match_points,
                standing_rank: prior_rank,
                field_size: prior_field_size,
            },
            MtgoCompetitiveEventProgressV1::Challenge {
                match_record: next_record,
                rounds_completed: next_rounds_completed,
                rounds_total: next_rounds_total,
                match_points: next_match_points,
                standing_rank: next_rank,
                field_size: next_field_size,
            },
        ) => {
            let match_delta =
                validate_event_monitor_match_record_advance_v1(prior_record, next_record)?;
            let round_delta = next_rounds_completed
                .checked_sub(*prior_rounds_completed)
                .ok_or("competitive Challenge completed-round count regressed")?;
            if next_rounds_total != prior_rounds_total
                || next_rounds_completed < prior_rounds_completed
                || round_delta > 1
                || match_delta > round_delta
                || next_match_points < prior_match_points
                || u32::from(next_match_points - prior_match_points) > u32::from(round_delta) * 3
                || matches!((prior_field_size, next_field_size), (Some(a), Some(b)) if a != b)
                || prior_rank.is_some() != prior_field_size.is_some()
                || next_rank.is_some() != next_field_size.is_some()
            {
                return Err("competitive Challenge progress regressed or changed totals".to_owned());
            }
            Ok(round_delta)
        }
        _ => Err("competitive event monitor cannot change progress schema".to_owned()),
    }
}

fn validate_event_monitor_match_record_advance_v1(
    prior: &mtgo_blackbox_v1::MtgoCompetitiveMatchRecordV1,
    next: &mtgo_blackbox_v1::MtgoCompetitiveMatchRecordV1,
) -> Result<u16, String> {
    if next.wins < prior.wins
        || next.losses < prior.losses
        || next.draws < prior.draws
        || next.matches_completed < prior.matches_completed
        || next.matches_completed - prior.matches_completed > 1
    {
        return Err("competitive event match record regressed or skipped a match".to_owned());
    }
    Ok(next.matches_completed - prior.matches_completed)
}

fn validate_event_record_visible_fact_pixels_v1(
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
    facts: &[MtgoCompetitiveEventRecordVisibleFactV1],
) -> Result<(), String> {
    let expected_len = u64::from(size.width)
        .checked_mul(u64::from(size.height))
        .and_then(|value| value.checked_mul(4))
        .ok_or("event-record source pixel length overflow")?;
    if u64::try_from(canonical_bgra8.len()).ok() != Some(expected_len) {
        return Err("event-record source pixels do not match the declared geometry".to_owned());
    }
    for fact in facts {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &fact.rect_client_px)
                .map_err(|error| format!("rehash visible event-record fact: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "visible event-record fact does not match the retained source pixels".to_owned(),
            );
        }
    }
    Ok(())
}

fn source_bound_event_record_commitment_v1(
    source_capture_commitment_sha256: &str,
    source_frame_profile_binding_sha256: &str,
    source_classification_result_commitment_sha256: &str,
    source_lifecycle_snapshot_commitment_sha256: &str,
    approved_account_alias_sha256: &str,
    event_identity_sha256: &str,
    event_record_commitment_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SOURCE_BOUND_COMPETITIVE_EVENT_RECORD_DOMAIN_V1);
    for part in [
        source_capture_commitment_sha256,
        source_frame_profile_binding_sha256,
        source_classification_result_commitment_sha256,
        source_lifecycle_snapshot_commitment_sha256,
        approved_account_alias_sha256,
        event_identity_sha256,
        event_record_commitment_sha256,
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.update(
        b"exact_same_frame_event_record_pixels_rehashed_no_input_no_entry_no_spending_no_gameplay",
    );
    format!("{:x}", hasher.finalize())
}

fn commitment_parts_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn looks_like_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        visible_frame_region_content_sha256_v1, MtgoCompetitiveEventRecordVisibleFactKindV1,
        MtgoCompetitiveMatchRecordV1, MtgoRectPxV1,
    };

    fn request_header_v1(
        pixels: &[u8],
        assets: &[u8],
    ) -> MtgoCompetitiveEventRecordClassifierRequestHeaderV1 {
        MtgoCompetitiveEventRecordClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: "mtgo_visible_competitive_event_record_v1".to_owned(),
            frame_id: 1,
            frame_sequence: 1,
            captured_at_unix_millis: 1,
            canonical_width: 2,
            canonical_height: 2,
            canonical_stride: 8,
            canonical_byte_length: 16,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: "1".repeat(64),
            source_frame_profile_binding_sha256: "2".repeat(64),
            parser_scope: "league_and_challenge_eight_slice_event_record_checked_untrusted_v1"
                .to_owned(),
            navigation_profile_commitment_sha256: "6".repeat(64),
            navigation_profile_admission_commitment_sha256: "7".repeat(64),
            approved_account_alias_sha256: "8".repeat(64),
            runtime_identity_commitment_sha256: "9".repeat(64),
            classifier_binary_sha256: "a".repeat(64),
            classifier_assets_manifest_sha256: sha256_hex_v1(assets),
        }
    }

    fn response_record_v1() -> MtgoVisibleCompetitiveEventRecordV1 {
        MtgoVisibleCompetitiveEventRecordV1 {
            schema_version: 1,
            record_id: "event-record-response-test".to_owned(),
            source_lifecycle_snapshot_commitment_sha256: "4".repeat(64),
            approved_account_alias_sha256: "8".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            lifecycle_phase: MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            frame_id: 1,
            frame_sequence: 1,
            frame_sha256: "b".repeat(64),
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            event_identity_sha256: "5".repeat(64),
            status: MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
            progress: MtgoCompetitiveEventProgressV1::League {
                match_record: MtgoCompetitiveMatchRecordV1 {
                    wins: 0,
                    losses: 0,
                    draws: 0,
                    matches_completed: 0,
                },
                matches_total: Some(5),
            },
            completion: None,
            visible_record_complete: true,
            facts: vec![
                MtgoCompetitiveEventRecordVisibleFactV1 {
                    kind: MtgoCompetitiveEventRecordVisibleFactKindV1::EventStatusVisible,
                    rect_client_px: MtgoRectPxV1 {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    content_sha256: "c".repeat(64),
                    confidence_bps: 10_000,
                },
                MtgoCompetitiveEventRecordVisibleFactV1 {
                    kind: MtgoCompetitiveEventRecordVisibleFactKindV1::EventProgressVisible,
                    rect_client_px: MtgoRectPxV1 {
                        x: 1,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    content_sha256: "d".repeat(64),
                    confidence_bps: 10_000,
                },
            ],
        }
    }

    fn response_lifecycle_v1() -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        MtgoVisibleCompetitiveLifecycleSnapshotV1 {
            schema_version: 1,
            snapshot_id: "event-record-response-lifecycle".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::League,
            phase: MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            frame_id: 1,
            frame_sequence: 1,
            frame_sha256: "b".repeat(64),
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            event_identity_sha256: Some("5".repeat(64)),
            match_identity_sha256: None,
            game_number: None,
            entry_terms: None,
            visible_state_complete: true,
            facts: vec![mtgo_blackbox_v1::MtgoLifecycleVisibleFactV1 {
                kind: mtgo_blackbox_v1::MtgoLifecycleVisibleFactKindV1::EnteredEventVisible,
                rect_client_px: MtgoRectPxV1 {
                    x: 0,
                    y: 1,
                    width: 1,
                    height: 1,
                },
                content_sha256: "e".repeat(64),
                confidence_bps: 10_000,
            }],
        }
    }

    fn monitor_observation_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        status: MtgoCompetitiveEventVisibleStatusV1,
        sequence: u64,
        progress: MtgoCompetitiveEventProgressV1,
        completion: Option<MtgoCompetitiveEventCompletionV1>,
    ) -> MtgoCompetitiveEventMonitorObservationV1 {
        MtgoCompetitiveEventMonitorObservationV1 {
            navigation_profile_commitment_sha256: "1".repeat(64),
            navigation_profile_admission_commitment_sha256: "2".repeat(64),
            runtime_identity_commitment_sha256: "3".repeat(64),
            approved_account_alias_sha256: "4".repeat(64),
            event_identity_sha256: "5".repeat(64),
            source_capture_commitment_sha256: format!("{sequence:064x}"),
            source_frame_profile_binding_sha256: format!("{:064x}", sequence + 100),
            classification_result_commitment_sha256: format!("{:064x}", sequence + 200),
            event_record_commitment_sha256: format!("{:064x}", sequence + 300),
            frame_id: sequence + 400,
            frame_sequence: sequence,
            captured_at_unix_millis: u128::from(sequence) + 1_000,
            event_kind,
            status,
            progress,
            completion,
        }
    }

    fn league_progress_v1(wins: u16, losses: u16, draws: u16) -> MtgoCompetitiveEventProgressV1 {
        MtgoCompetitiveEventProgressV1::League {
            match_record: MtgoCompetitiveMatchRecordV1 {
                wins,
                losses,
                draws,
                matches_completed: wins + losses + draws,
            },
            matches_total: Some(5),
        }
    }

    fn challenge_progress_v1(
        wins: u16,
        losses: u16,
        rounds_completed: u16,
        rounds_total: u16,
        match_points: u16,
    ) -> MtgoCompetitiveEventProgressV1 {
        MtgoCompetitiveEventProgressV1::Challenge {
            match_record: MtgoCompetitiveMatchRecordV1 {
                wins,
                losses,
                draws: 0,
                matches_completed: wins + losses,
            },
            rounds_completed,
            rounds_total,
            match_points,
            standing_rank: Some(4),
            field_size: Some(32),
        }
    }

    #[test]
    fn event_record_parser_request_is_canonical_pixel_and_asset_bound() {
        let pixels = vec![0_u8; 16];
        let assets = b"{}";
        let header = request_header_v1(&pixels, assets);
        let json = serde_json::to_vec(&header).unwrap();
        let checked =
            check_untrusted_competitive_event_record_classifier_request_v1(&json, assets, &pixels)
                .unwrap();
        assert_eq!(checked.header_v1(), &header);
        assert_eq!(checked.request_commitment_sha256_v1().len(), 64);
        assert!(!checked.safe_for_live_classification_v1());
        assert!(!checked.permits_event_entry_v1());
        assert!(!checked.permits_spending_v1());
        assert!(!checked.safe_for_input_v1());

        let mut changed_pixels = pixels.clone();
        changed_pixels[0] = 1;
        assert!(
            check_untrusted_competitive_event_record_classifier_request_v1(
                &json,
                assets,
                &changed_pixels,
            )
            .is_err()
        );
        let mut noncanonical = json.clone();
        noncanonical.push(b' ');
        assert!(
            check_untrusted_competitive_event_record_classifier_request_v1(
                &noncanonical,
                assets,
                &pixels,
            )
            .is_err()
        );
        let mut unsupported = header;
        unsupported.parser_scope = "six_slice_navigation_only".to_owned();
        assert!(
            check_untrusted_competitive_event_record_classifier_request_v1(
                &serde_json::to_vec(&unsupported).unwrap(),
                assets,
                &pixels,
            )
            .is_err()
        );
    }

    #[test]
    fn event_record_parser_response_binds_exact_request_and_canonical_json() {
        let request = "e".repeat(64);
        let response = MtgoCompetitiveEventRecordClassifierProcessResponseV1 {
            schema_version: 1,
            request_commitment_sha256: request.clone(),
            lifecycle: response_lifecycle_v1(),
            record: response_record_v1(),
        };
        let bytes = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            parse_event_record_classifier_response_v1(&bytes, &request).unwrap(),
            response
        );
        assert!(parse_event_record_classifier_response_v1(&bytes, &"f".repeat(64)).is_err());
        let mut noncanonical = bytes;
        noncanonical.push(b'\n');
        assert!(parse_event_record_classifier_response_v1(&noncanonical, &request).is_err());
    }

    #[test]
    fn event_record_fact_pixels_are_rehashed_exactly() {
        let size = MtgoSizePxV1 {
            width: 4,
            height: 2,
        };
        let mut pixels = vec![0_u8; 32];
        for (index, byte) in pixels.iter_mut().enumerate() {
            *byte = index as u8;
        }
        let rect = MtgoRectPxV1 {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        };
        let fact = MtgoCompetitiveEventRecordVisibleFactV1 {
            kind: MtgoCompetitiveEventRecordVisibleFactKindV1::EventProgressVisible,
            content_sha256: visible_frame_region_content_sha256_v1(&pixels, &size, &rect).unwrap(),
            rect_client_px: rect,
            confidence_bps: 10_000,
        };
        validate_event_record_visible_fact_pixels_v1(&pixels, &size, std::slice::from_ref(&fact))
            .unwrap();
        pixels[4] ^= 0xff;
        assert!(validate_event_record_visible_fact_pixels_v1(&pixels, &size, &[fact]).is_err());
    }

    #[test]
    fn event_record_source_binding_commitment_binds_every_lineage_digest() {
        let baseline = source_bound_event_record_commitment_v1(
            &"1".repeat(64),
            &"2".repeat(64),
            &"3".repeat(64),
            &"4".repeat(64),
            &"5".repeat(64),
            &"6".repeat(64),
            &"7".repeat(64),
        );
        let changed = source_bound_event_record_commitment_v1(
            &"1".repeat(64),
            &"2".repeat(64),
            &"3".repeat(64),
            &"4".repeat(64),
            &"5".repeat(64),
            &"6".repeat(64),
            &"8".repeat(64),
        );
        assert_eq!(baseline.len(), 64);
        assert_ne!(baseline, changed);
    }

    #[test]
    fn event_monitor_accepts_one_complete_league_cycle() {
        let waiting = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
            1,
            league_progress_v1(0, 0, 0),
            None,
        );
        let pairing = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::PairingReady,
            2,
            league_progress_v1(0, 0, 0),
            None,
        );
        let between = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
            3,
            league_progress_v1(1, 0, 0),
            None,
        );
        let waiting_again = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
            4,
            league_progress_v1(1, 0, 0),
            None,
        );
        let dropped = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::EventComplete,
            5,
            league_progress_v1(1, 0, 0),
            Some(MtgoCompetitiveEventCompletionV1::Dropped),
        );
        for observation in [&waiting, &pairing, &between, &waiting_again, &dropped] {
            validate_event_monitor_observation_v1(observation).unwrap();
        }
        validate_event_monitor_advance_v1(&waiting, &pairing).unwrap();
        validate_event_monitor_advance_v1(&pairing, &between).unwrap();
        validate_event_monitor_advance_v1(&between, &waiting_again).unwrap();
        validate_event_monitor_advance_v1(&waiting_again, &dropped).unwrap();
        assert!(!event_monitor_status_advance_allowed_v1(
            MtgoCompetitiveEventVisibleStatusV1::EventComplete,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        ));
    }

    #[test]
    fn event_monitor_rejects_identity_staleness_and_record_regression() {
        let prior = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
            10,
            league_progress_v1(1, 0, 0),
            None,
        );
        let mut wrong_event = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
            11,
            league_progress_v1(1, 0, 0),
            None,
        );
        wrong_event.event_identity_sha256 = "a".repeat(64);
        assert!(validate_event_monitor_advance_v1(&prior, &wrong_event).is_err());

        let mut stale = wrong_event.clone();
        stale.event_identity_sha256 = prior.event_identity_sha256.clone();
        stale.frame_sequence = prior.frame_sequence;
        assert!(validate_event_monitor_advance_v1(&prior, &stale).is_err());

        let regressed = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
            12,
            league_progress_v1(0, 0, 0),
            None,
        );
        assert!(validate_event_monitor_advance_v1(&prior, &regressed).is_err());

        let waiting = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
            13,
            league_progress_v1(0, 0, 0),
            None,
        );
        let invented_result = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
            14,
            league_progress_v1(1, 0, 0),
            None,
        );
        assert!(validate_event_monitor_advance_v1(&waiting, &invented_result).is_err());

        let mut missing_total = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventVisibleStatusV1::PairingReady,
            15,
            league_progress_v1(0, 0, 0),
            None,
        );
        if let MtgoCompetitiveEventProgressV1::League { matches_total, .. } =
            &mut missing_total.progress
        {
            *matches_total = None;
        }
        assert!(validate_event_monitor_advance_v1(&waiting, &missing_total).is_err());
    }

    #[test]
    fn event_monitor_rejects_challenge_total_points_and_match_skips() {
        let prior = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveEventVisibleStatusV1::PairingReady,
            20,
            challenge_progress_v1(1, 0, 1, 5, 3),
            None,
        );
        let next = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
            21,
            challenge_progress_v1(1, 1, 2, 5, 3),
            None,
        );
        validate_event_monitor_advance_v1(&prior, &next).unwrap();

        let mut wrong_total = next.clone();
        if let MtgoCompetitiveEventProgressV1::Challenge { rounds_total, .. } =
            &mut wrong_total.progress
        {
            *rounds_total = 6;
        }
        assert!(validate_event_monitor_advance_v1(&prior, &wrong_total).is_err());

        let mut fewer_points = next.clone();
        if let MtgoCompetitiveEventProgressV1::Challenge { match_points, .. } =
            &mut fewer_points.progress
        {
            *match_points = 2;
        }
        assert!(validate_event_monitor_advance_v1(&prior, &fewer_points).is_err());

        let skipped = monitor_observation_v1(
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
            22,
            challenge_progress_v1(3, 0, 3, 5, 9),
            None,
        );
        assert!(validate_event_monitor_advance_v1(&prior, &skipped).is_err());
    }
}
