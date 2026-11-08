//! Declared-deck owning consumer for the exact Perforating Artist printing.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    artist_public_v64::{FactsV64, PaymentKindV64, PaymentStatusV64, StageV64},
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    ids::PlayerId,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{ConsumerV1, IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-artist-consumer/v1";
pub const PRINTING_V1: u16 = 460;
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/artist-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/artist-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/artist-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("artist_consumer_v1.rs"),
            include_str!("artist_public_v64.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_artist_env_v1.rs"),
        ] {
            hash.update([0]);
            hash.update(source.as_bytes());
        }
        hash.update(crate::rules_meaning_artist_v64::producer_sha256());
        hash.update(crate::limited_kaito_session_v16::producer_sha256_v16());
        hash.update(crate::current_mixed_policy_v1::feature_identity());
        format!("{:x}", hash.finalize())
    })
    .clone()
}

pub(crate) fn preflight(config: &ResetV1) -> Result<(), String> {
    crate::limited_consumer_wire_v1::preflight_single_appendix(
        config,
        PRINTING_V1,
        || crate::rules_meaning_artist_v64::printed_meaning().map(|meaning| meaning.card_id),
        "Artist",
    )
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_artist_v1(config, false).map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

/// The public name/count reducer is installed before the first typed menu.
pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_artist_v1(config, true)
        .map_err(|error| format!("{error:?}"))?;
    KaitoSession::from_session(owner).map_err(|error| error.to_string())
}

pub fn reset_human_v1(
    config: &ResetV1,
    seat: PlayerSeatV1,
) -> Result<HumanKaitoSessionV75, String> {
    HumanKaitoSessionV75::from_session(native(config)?, seat).map_err(|error| error.to_string())
}

pub struct JsonlServerV1(ProfileWireV1);

impl Default for JsonlServerV1 {
    fn default() -> Self {
        Self(ProfileWireV1::new(PROFILE_V1, producer_sha256_v1, native))
    }
}

impl JsonlServerV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_line(&mut self, line: &str) -> String {
        self.0.handle_line(line)
    }

    pub fn handle_request(&mut self, request: RequestV1) -> ReplyV1 {
        self.0.handle_request(request)
    }
}

pub(crate) fn capture(state: &GameState, actor: PlayerId) -> Result<FactsV64, String> {
    capture_scoped(state, actor, ConsumerV1::Legacy)
}

pub(crate) fn capture_scoped(
    state: &GameState,
    actor: PlayerId,
    consumer: ConsumerV1,
) -> Result<FactsV64, String> {
    let mut facts = crate::artist_public_v64::capture_scoped(state, actor, consumer)?;
    // An opponent's revealed Artist belongs in name/count hand knowledge.
    // Its physical slot is irrelevant to all Artist choices.
    facts
        .authorized_printings
        .retain(|reference| reference.zone != Zone::Hand || reference.owner == facts.viewer);
    Ok(facts)
}

pub(crate) fn validate_facts(facts: &FactsV64, actor: PlayerSeatV1) -> Result<(), String> {
    crate::rules_meaning_artist_v64::validate(&facts.meaning)?;
    if facts.viewer != actor
        || facts.schema != crate::artist_public_v64::SCHEMA
        || facts.producer_sha256 != crate::artist_public_v64::producer_sha256()
        || facts.complete_ordinary_observation
        || facts.owning_consumer_admitted
        || facts.authorized_printings.iter().any(|reference| {
            reference.identity
                != (IdentityV1::Known {
                    card_id: PRINTING_V1,
                })
                || (reference.zone == Zone::Hand && reference.owner != actor)
        })
    {
        return Err("invalid Artist actor facts".into());
    }
    for trigger in &facts.triggers {
        if trigger.source.captured_controller == trigger.affected_opponent
            || !trigger.captured_controller_attacked_this_turn
        {
            return Err("invalid Artist Raid context".into());
        }
        let Some(payment) = &trigger.payment else {
            continue;
        };
        let visible = payment.kind != PaymentKindV64::DiscardCard || payment.payer == actor;
        let valid_reference = |reference: &ReferenceV1| match payment.kind {
            PaymentKindV64::SacrificeControlledNonlandPermanent => {
                reference.zone == Zone::Battlefield && reference.controller == payment.payer
            }
            PaymentKindV64::DiscardCard => {
                reference.zone == Zone::Hand && reference.owner == payment.payer
            }
        };
        if trigger.stage != StageV64::Resolving
            || payment.payer != trigger.affected_opponent
            || payment.is_target
            || !payment.may_decline
            || payment.candidates.is_some() != visible
            || payment.selected.is_some() != visible
            || payment
                .candidates
                .iter()
                .flatten()
                .any(|r| !valid_reference(r))
            || payment.selected.as_ref().is_some_and(|selected| {
                selected.len() > 1
                    || selected.iter().any(|r| {
                        !payment
                            .candidates
                            .as_ref()
                            .is_some_and(|candidates| candidates.contains(r))
                    })
            })
        {
            return Err("invalid Artist payment context".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnswerV1 {
    SelectPayment,
    ConfirmPayment,
    DeclinePayment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentActionV1 {
    pub kind: PaymentKindV64,
    pub answer: AnswerV1,
}

fn matches_reference(visible: &ReferenceV1, action: &CardStableRefV1) -> bool {
    visible.object == action.arena_id
        && visible.generation == action.zone_change_count
        && visible.zone == action.zone
        && visible.owner == action.owner
        && visible.controller == action.controller
        && match visible.identity {
            IdentityV1::Known { card_id } => card_id == action.card_db_id,
            IdentityV1::FaceDown { disguised } => {
                crate::card_def::card_id_by_name(if disguised {
                    "Face-down creature"
                } else {
                    "Face-down card"
                }) == Some(action.card_db_id)
            }
        }
}

pub(crate) fn payment_action(
    facts: Option<&FactsV64>,
    action: &Action,
) -> Result<Option<PaymentActionV1>, String> {
    let Some(facts) = facts else { return Ok(None) };
    let Some((trigger, payment)) = facts.triggers.iter().find_map(|trigger| {
        trigger
            .payment
            .as_ref()
            .filter(|p| p.status == PaymentStatusV64::Waiting && p.payer == facts.viewer)
            .map(|p| (trigger, p))
    }) else {
        return Ok(None);
    };
    let (source, selected_count, target) = match action {
        Action::ChooseEffectTarget {
            source,
            selected_count,
            target: TargetRefV1::Object { object },
            ..
        } => (source, *selected_count, Some(object)),
        Action::FinishEffectSelection {
            source,
            selected_count,
            ..
        } => (source, *selected_count, None),
        _ => return Err("Artist payment lost its genuine native choice".into()),
    };
    let selected = payment
        .selected
        .as_ref()
        .ok_or("Artist payer lacks selected rows")?;
    if !matches_reference(&trigger.source.original_source, source)
        || usize::from(selected_count) != selected.len()
    {
        return Err("Artist payment changed its original source or answer prefix".into());
    }
    let answer = match target {
        Some(target) => {
            if !payment
                .candidates
                .as_ref()
                .is_some_and(|rows| rows.iter().any(|row| matches_reference(row, target)))
            {
                return Err("Artist payment candidate outside its original roster".into());
            }
            AnswerV1::SelectPayment
        }
        None if selected.is_empty() => AnswerV1::DeclinePayment,
        None => AnswerV1::ConfirmPayment,
    };
    Ok(Some(PaymentActionV1 {
        kind: payment.kind,
        answer,
    }))
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
