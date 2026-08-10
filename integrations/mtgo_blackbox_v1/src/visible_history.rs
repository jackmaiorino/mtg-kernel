use crate::{
    CheckedUntrustedMtgoGameplayCalibrationV1, CheckedUntrustedMtgoPregameCalibrationV1,
    CheckedUntrustedMtgoVisibleObjectActionCalibrationV1, MtgoContractErrorV1,
    MtgoPregameActionSemanticV1, MtgoSizePxV1, MtgoVisibleObjectCalibrationActionV1,
};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{ActionSemanticV1, PlayerSeatV1};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_VISIBLE_HISTORY_SCHEMA_V1: u32 = 1;

const VISIBLE_HISTORY_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-visible-history-v1";
const MAX_HISTORY_STEPS_V1: usize = 1_024;

pub enum MtgoCheckedUntrustedCalibrationTransitionRefV1<'a> {
    Pregame(&'a CheckedUntrustedMtgoPregameCalibrationV1),
    Gameplay(&'a CheckedUntrustedMtgoGameplayCalibrationV1),
    VisibleObject(&'a CheckedUntrustedMtgoVisibleObjectActionCalibrationV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "continuity_kind", rename_all = "snake_case")]
pub enum MtgoVisibleHistoryLinkV1 {
    StartOfCapturedHistory,
    ExactFrameMatch,
    VisibleGap { reason_codes: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event_kind", rename_all = "snake_case")]
enum MtgoVisibleHistoryEventV1 {
    KeepOpeningHand,
    Mulligan {
        next_hand_size: u8,
    },
    Pass {
        actor: PlayerSeatV1,
    },
    PlayLand {
        actor: PlayerSeatV1,
        source_adapter_object_id: String,
        visible_card_name: String,
    },
    ActivateManaAbility {
        actor: PlayerSeatV1,
        source_adapter_object_id: String,
        visible_card_name: String,
        mana_choice: Option<ManaColor>,
        visible_mana_added: ManaColor,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MtgoVisibleHistorySourceKindV1 {
    PregameCalibration,
    GameplayCalibration,
    VisibleObjectCalibration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MtgoVisibleHistoryStepV1 {
    sequence: u64,
    source_kind: MtgoVisibleHistorySourceKindV1,
    transition_commitment_sha256: String,
    before_frame_sha256: String,
    after_frame_sha256: String,
    event: MtgoVisibleHistoryEventV1,
    link: MtgoVisibleHistoryLinkV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MtgoVisibleHistoryRecordV1 {
    schema_version: u32,
    history_id: String,
    client_size_px: MtgoSizePxV1,
    steps: Vec<MtgoVisibleHistoryStepV1>,
    begins_at_opening_hand_decision: bool,
    exact_pixel_chain_from_opening_hand: bool,
    safe_for_kernel_context: bool,
    safe_for_policy_scoring: bool,
    safe_for_input: bool,
}

/// Structurally checked, caller-ordered history made only from already checked
/// calibration traces.
///
/// Exact frame linkage proves only byte continuity between two captured traces.
/// Gap-separated source order is a caller declaration, not a timestamp proof.
/// The ledger also cannot prove that no unobserved transition happened while the
/// pixels stayed unchanged. This wrapper therefore cannot construct an engine
/// context, ObservationV5, policy score, or input command.
///
/// ```compile_fail
/// use mtg_kernel::rl::EngineContextV2;
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoVisibleHistoryV1;
/// fn cannot_extract_engine_context(
///     history: &CheckedUntrustedMtgoVisibleHistoryV1,
/// ) -> &EngineContextV2 {
///     history.engine_context()
/// }
/// ```
pub struct CheckedUntrustedMtgoVisibleHistoryV1 {
    record: MtgoVisibleHistoryRecordV1,
    gap_count: usize,
    trailing_exact_step_count: usize,
    history_commitment_sha256: String,
}

impl CheckedUntrustedMtgoVisibleHistoryV1 {
    pub fn step_count(&self) -> usize {
        self.record.steps.len()
    }

    pub fn gap_count(&self) -> usize {
        self.gap_count
    }

    pub fn trailing_exact_step_count(&self) -> usize {
        self.trailing_exact_step_count
    }

    pub fn begins_at_opening_hand_decision(&self) -> bool {
        self.record.begins_at_opening_hand_decision
    }

    pub fn exact_pixel_chain_from_opening_hand(&self) -> bool {
        self.record.exact_pixel_chain_from_opening_hand
    }

    pub fn safe_for_kernel_context(&self) -> bool {
        self.record.safe_for_kernel_context
    }

    pub fn history_commitment_sha256(&self) -> &str {
        &self.history_commitment_sha256
    }
}

pub fn build_checked_untrusted_visible_history_v1(
    history_id: &str,
    sources: &[MtgoCheckedUntrustedCalibrationTransitionRefV1<'_>],
    links: &[MtgoVisibleHistoryLinkV1],
) -> Result<CheckedUntrustedMtgoVisibleHistoryV1, MtgoContractErrorV1> {
    validate_safe_identifier_v1(history_id, "visible_history_id_invalid")?;
    if sources.is_empty() || sources.len() > MAX_HISTORY_STEPS_V1 {
        return Err(MtgoContractErrorV1::new(
            "visible_history_source_count_invalid",
            sources.len().to_string(),
        ));
    }
    if sources.len() != links.len() {
        return Err(MtgoContractErrorV1::new(
            "visible_history_link_count_mismatch",
            format!("sources={},links={}", sources.len(), links.len()),
        ));
    }

    let client_size_px = sources[0].client_size_px().clone();
    let mut steps = Vec::with_capacity(sources.len());
    let mut commitments = HashSet::new();
    let mut gap_count = 0;
    for (index, (source, link)) in sources.iter().zip(links).enumerate() {
        if source.client_size_px() != &client_size_px {
            return Err(MtgoContractErrorV1::new(
                "visible_history_client_size_changed",
                format!("index={index}"),
            ));
        }
        let canonical = source.canonical_transition_v1()?;
        if !commitments.insert(canonical.transition_commitment_sha256) {
            return Err(MtgoContractErrorV1::new(
                "visible_history_duplicate_transition",
                canonical.transition_commitment_sha256,
            ));
        }
        validate_link_v1(index, link, steps.last(), &canonical)?;
        if matches!(link, MtgoVisibleHistoryLinkV1::VisibleGap { .. }) {
            gap_count += 1;
        }
        steps.push(MtgoVisibleHistoryStepV1 {
            sequence: u64::try_from(index + 1).map_err(|_| {
                MtgoContractErrorV1::new(
                    "visible_history_sequence_overflow",
                    (index + 1).to_string(),
                )
            })?,
            source_kind: canonical.source_kind,
            transition_commitment_sha256: canonical.transition_commitment_sha256.to_owned(),
            before_frame_sha256: canonical.before_frame_sha256.to_owned(),
            after_frame_sha256: canonical.after_frame_sha256.to_owned(),
            event: canonical.event,
            link: link.clone(),
        });
    }

    let begins_at_opening_hand_decision = matches!(
        steps.first().map(|step| &step.event),
        Some(
            MtgoVisibleHistoryEventV1::KeepOpeningHand | MtgoVisibleHistoryEventV1::Mulligan { .. }
        )
    );
    let exact_pixel_chain_from_opening_hand = begins_at_opening_hand_decision && gap_count == 0;
    let trailing_exact_step_count = trailing_exact_step_count_v1(&steps);
    let record = MtgoVisibleHistoryRecordV1 {
        schema_version: MTGO_VISIBLE_HISTORY_SCHEMA_V1,
        history_id: history_id.to_owned(),
        client_size_px,
        steps,
        begins_at_opening_hand_decision,
        exact_pixel_chain_from_opening_hand,
        safe_for_kernel_context: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
    };
    let encoded = serde_json::to_vec(&record).map_err(|error| {
        MtgoContractErrorV1::new("visible_history_serialization_failed", error.to_string())
    })?;
    let mut hasher = Sha256::new();
    hasher.update(VISIBLE_HISTORY_COMMITMENT_DOMAIN_V1);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(&encoded);
    let history_commitment_sha256 = format!("{:x}", hasher.finalize());

    Ok(CheckedUntrustedMtgoVisibleHistoryV1 {
        record,
        gap_count,
        trailing_exact_step_count,
        history_commitment_sha256,
    })
}

struct CanonicalTransitionRefV1<'a> {
    source_kind: MtgoVisibleHistorySourceKindV1,
    transition_commitment_sha256: &'a str,
    before_frame_sha256: &'a str,
    after_frame_sha256: &'a str,
    event: MtgoVisibleHistoryEventV1,
}

impl<'a> MtgoCheckedUntrustedCalibrationTransitionRefV1<'a> {
    fn client_size_px(&self) -> &MtgoSizePxV1 {
        match self {
            Self::Pregame(source) => source.client_size_px(),
            Self::Gameplay(source) => source.client_size_px(),
            Self::VisibleObject(source) => source.client_size_px(),
        }
    }

    fn canonical_transition_v1(&self) -> Result<CanonicalTransitionRefV1<'a>, MtgoContractErrorV1> {
        match self {
            Self::Pregame(source) => {
                let event = match source.action() {
                    MtgoPregameActionSemanticV1::KeepOpeningHand => {
                        MtgoVisibleHistoryEventV1::KeepOpeningHand
                    }
                    MtgoPregameActionSemanticV1::Mulligan { next_hand_size } => {
                        MtgoVisibleHistoryEventV1::Mulligan {
                            next_hand_size: *next_hand_size,
                        }
                    }
                };
                Ok(CanonicalTransitionRefV1 {
                    source_kind: MtgoVisibleHistorySourceKindV1::PregameCalibration,
                    transition_commitment_sha256: source.transition_commitment_sha256(),
                    before_frame_sha256: source.before_frame_sha256(),
                    after_frame_sha256: source.after_frame_sha256(),
                    event,
                })
            }
            Self::Gameplay(source) => {
                let ActionSemanticV1::Pass { actor } = source.action() else {
                    return Err(MtgoContractErrorV1::new(
                        "visible_history_gameplay_action_unsupported",
                        "checked gameplay source was not Pass",
                    ));
                };
                Ok(CanonicalTransitionRefV1 {
                    source_kind: MtgoVisibleHistorySourceKindV1::GameplayCalibration,
                    transition_commitment_sha256: source.transition_commitment_sha256(),
                    before_frame_sha256: source.before_frame_sha256(),
                    after_frame_sha256: source.after_frame_sha256(),
                    event: MtgoVisibleHistoryEventV1::Pass { actor: *actor },
                })
            }
            Self::VisibleObject(source) => {
                let event = match source.action() {
                    MtgoVisibleObjectCalibrationActionV1::PlayLand {
                        actor,
                        source_adapter_object_id,
                        visible_card_name,
                    } => MtgoVisibleHistoryEventV1::PlayLand {
                        actor: *actor,
                        source_adapter_object_id: source_adapter_object_id.clone(),
                        visible_card_name: visible_card_name.clone(),
                    },
                    MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility {
                        actor,
                        source_adapter_object_id,
                        visible_card_name,
                        mana_choice,
                        visible_mana_added,
                    } => MtgoVisibleHistoryEventV1::ActivateManaAbility {
                        actor: *actor,
                        source_adapter_object_id: source_adapter_object_id.clone(),
                        visible_card_name: visible_card_name.clone(),
                        mana_choice: *mana_choice,
                        visible_mana_added: *visible_mana_added,
                    },
                };
                Ok(CanonicalTransitionRefV1 {
                    source_kind: MtgoVisibleHistorySourceKindV1::VisibleObjectCalibration,
                    transition_commitment_sha256: source.transition_commitment_sha256(),
                    before_frame_sha256: source.before_frame_sha256(),
                    after_frame_sha256: source.after_frame_sha256(),
                    event,
                })
            }
        }
    }
}

fn validate_link_v1(
    index: usize,
    link: &MtgoVisibleHistoryLinkV1,
    previous: Option<&MtgoVisibleHistoryStepV1>,
    current: &CanonicalTransitionRefV1<'_>,
) -> Result<(), MtgoContractErrorV1> {
    match (index, link, previous) {
        (0, MtgoVisibleHistoryLinkV1::StartOfCapturedHistory, None) => Ok(()),
        (0, _, _) => Err(MtgoContractErrorV1::new(
            "visible_history_first_link_invalid",
            "the first source must start the captured history",
        )),
        (_, MtgoVisibleHistoryLinkV1::StartOfCapturedHistory, _) => Err(MtgoContractErrorV1::new(
            "visible_history_noninitial_start",
            index.to_string(),
        )),
        (_, MtgoVisibleHistoryLinkV1::ExactFrameMatch, Some(previous)) => {
            if previous.after_frame_sha256 != current.before_frame_sha256 {
                return Err(MtgoContractErrorV1::new(
                    "visible_history_exact_link_mismatch",
                    format!("index={index}"),
                ));
            }
            Ok(())
        }
        (_, MtgoVisibleHistoryLinkV1::VisibleGap { reason_codes }, Some(previous)) => {
            if previous.after_frame_sha256 == current.before_frame_sha256 {
                return Err(MtgoContractErrorV1::new(
                    "visible_history_gap_over_exact_frame",
                    format!("index={index}"),
                ));
            }
            validate_reason_codes_v1(reason_codes, index)
        }
        (_, _, None) => Err(MtgoContractErrorV1::new(
            "visible_history_internal_previous_missing",
            index.to_string(),
        )),
    }
}

fn trailing_exact_step_count_v1(steps: &[MtgoVisibleHistoryStepV1]) -> usize {
    let mut count = usize::from(!steps.is_empty());
    for step in steps.iter().skip(1).rev() {
        if !matches!(step.link, MtgoVisibleHistoryLinkV1::ExactFrameMatch) {
            break;
        }
        count += 1;
    }
    count
}

fn validate_reason_codes_v1(reasons: &[String], index: usize) -> Result<(), MtgoContractErrorV1> {
    if reasons.is_empty() || reasons.len() > 16 {
        return Err(MtgoContractErrorV1::new(
            "visible_history_gap_reason_count_invalid",
            format!("index={index},count={}", reasons.len()),
        ));
    }
    let mut unique = HashSet::new();
    for reason in reasons {
        validate_safe_identifier_v1(reason, "visible_history_gap_reason_invalid")?;
        if !unique.insert(reason) {
            return Err(MtgoContractErrorV1::new(
                "visible_history_duplicate_gap_reason",
                reason,
            ));
        }
    }
    Ok(())
}

fn validate_safe_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}
