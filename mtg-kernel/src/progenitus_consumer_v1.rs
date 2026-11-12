//! Declared-deck owner for Progenitus protection and replacement choices.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    progenitus_public_v1::{self as public, FactsV1, PRINTING},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-progenitus-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/progenitus-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/progenitus-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/progenitus-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("progenitus_consumer_v1.rs"),
            include_str!("human_bo3_v1/progenitus_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_progenitus_env_v1.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        hash.update(public::producer_sha256());
        hash.update(crate::limited_kaito_session_v16::producer_sha256_v16());
        hash.update(crate::current_mixed_policy_v1::feature_identity());
        format!("{:x}", hash.finalize())
    })
    .clone()
}

pub(crate) fn preflight(config: &ResetV1) -> Result<(), String> {
    crate::limited_consumer_wire_v1::preflight_single_appendix(
        config,
        PRINTING,
        public::selected_id,
        public::NAME,
    )
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_progenitus_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_progenitus_v1(config, true)
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

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("invalid Progenitus viewer facts".into());
    }
    let printing = IdentityV1::Known { card_id: PRINTING };
    for permanent in &facts.battlefield {
        if permanent.card.identity != printing
            || permanent.card.zone != Zone::Battlefield
            || permanent.protection_from_everything_active
                != (permanent.form.keyword_bits
                    & crate::card_def::Keywords::PROTECTION_FROM_EVERYTHING.0
                    != 0)
        {
            return Err("invalid Progenitus effective protection".into());
        }
    }
    for spell in &facts.spells {
        if spell.card.identity != printing
            || spell.card.zone != Zone::Stack
            || spell.uncast_copy != spell.origin_zone.is_none()
            || (!spell.announcing && !spell.uncast_copy) != spell.finalized_method.is_some()
            || spell.announcing && spell.uncast_copy
        {
            return Err("invalid Progenitus original cast".into());
        }
    }
    if let Some(order) = &facts.graveyard_order {
        if order.chooser != actor
            || order.legal.is_empty()
            || order.selected.len() + order.legal.len() < 2
            || order
                .selected
                .iter()
                .chain(&order.legal)
                .any(|card| card.owner != actor)
        {
            return Err("invalid Progenitus graveyard order".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseGraveyardOrder { selected_count: u16 },
}

fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    let card_id = match card.identity {
        IdentityV1::Known { card_id } => Some(card_id),
        IdentityV1::FaceDown { disguised } => crate::card_def::card_id_by_name(if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        }),
    };
    card_id == Some(stable.card_db_id)
        && card.object == stable.arena_id
        && card.generation == stable.zone_change_count
        && card.owner == stable.owner
        && card.controller == stable.controller
        && card.zone == stable.zone
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(order) = facts.and_then(|facts| facts.graveyard_order.as_ref()) else {
        return Ok(None);
    };
    let Action::ChooseEffectTarget {
        actor,
        target: TargetRefV1::Object { object },
        selected_count,
        min_targets,
        max_targets,
        ..
    } = action
    else {
        return Err("Progenitus graveyard order lost its native action".into());
    };
    if *actor != order.chooser
        || usize::from(*selected_count) != order.selected.len()
        || min_targets != max_targets
        || usize::from(*min_targets) != order.selected.len() + order.legal.len()
        || !order.legal.iter().any(|card| binds(card, object))
    {
        return Err("Progenitus graveyard answer lost its original incarnation".into());
    }
    Ok(Some(ActionV1::ChooseGraveyardOrder {
        selected_count: *selected_count,
    }))
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
