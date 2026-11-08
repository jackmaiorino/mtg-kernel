//! Declared-deck owning consumer for Leyline Axe's native opening and equip choices.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    leyline_axe_public_v1::{self as public, FactsV1, PRINTING},
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-leyline-axe-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/leyline-axe-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/leyline-axe-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/leyline-axe-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("leyline_axe_consumer_v1.rs"),
            include_str!("human_bo3_v1/leyline_axe_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_leyline_axe_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_leyline_axe_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_leyline_axe_v1(config, true)
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

fn exact_printing(card: &ReferenceV1) -> bool {
    card.identity == (IdentityV1::Known { card_id: PRINTING })
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor || facts.schema != public::SCHEMA || facts.meaning != public::printed_meaning()? {
        return Err("invalid Leyline Axe actor facts".into());
    }
    if facts.opening_choice.as_ref().is_some_and(|card|
        !exact_printing(card) || card.zone != Zone::Hand || card.owner != actor)
    {
        return Err("invalid Leyline Axe private opening source".into());
    }
    for entry in &facts.opening_entries {
        if entry.original.zone != Zone::Battlefield || entry.original.owner != entry.original.controller
            || entry.current_original.as_ref().is_some_and(|card| {
                !exact_printing(card) || card.zone != Zone::Battlefield
                    || card.object != entry.original.object
                    || card.generation != entry.original.generation
                    || card.owner != entry.original.owner
            })
        {
            return Err("invalid Leyline Axe public opening entry".into());
        }
    }
    for cast in &facts.casts {
        if !exact_printing(&cast.card) || cast.card.zone != Zone::Stack
            || cast.uncast_copy != cast.origin_zone.is_none()
            || (!cast.announcing && !cast.uncast_copy) != cast.finalized_method.is_some()
            || cast.uncast_copy && cast.announcing
        {
            return Err("invalid Leyline Axe original cast".into());
        }
    }
    for ability in &facts.equip {
        let source = &ability.source.original_source;
        if !exact_printing(source) || source.zone != Zone::Battlefield
            || source.controller != ability.source.captured_controller
            || ability.stack_position.is_some() && ability.target.is_none()
        {
            return Err("invalid Leyline Axe original equip source".into());
        }
        if let Some(target) = &ability.target {
            if target.original.zone != Zone::Battlefield
                || target.original.controller != ability.source.captured_controller
                || target.currently_legal && target.current_original.is_none()
                || target.current_original.as_ref().is_some_and(|card| {
                    card.zone != Zone::Battlefield || card.object != target.original.object
                        || card.generation != target.original.generation
                        || card.owner != target.original.owner
                })
            {
                return Err("invalid Leyline Axe original equip target".into());
            }
        }
    }
    for equipment in &facts.battlefield {
        if !exact_printing(&equipment.card) || equipment.card.zone != Zone::Battlefield {
            return Err("invalid Leyline Axe battlefield source".into());
        }
        if let Some(host) = &equipment.host {
            if host.card.zone != Zone::Battlefield || host.card.object == equipment.card.object {
                return Err("invalid Leyline Axe live host".into());
            }
            crate::tinybones_public_v72::nonnegative(&host.counters)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseOpeningEntry { accept: bool },
    ActivateEquip { generic_cost: u8 },
    ChooseEquipTarget,
}

fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    exact_printing(card) && stable.card_db_id == PRINTING
        && card.object == stable.arena_id && card.generation == stable.zone_change_count
        && card.owner == stable.owner && card.controller == stable.controller && card.zone == stable.zone
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else { return Ok(None) };
    match action {
        Action::ChooseEffectOption { actor, source, option_index, option_count }
            if source.card_db_id == PRINTING =>
        {
            if *actor != facts.viewer || *option_count != 2 || *option_index > 1
                || !facts.opening_choice.as_ref().is_some_and(|card| binds(card, source))
            {
                return Err("Leyline Axe answer lost its private opening source".into());
            }
            Ok(Some(ActionV1::ChooseOpeningEntry { accept: *option_index == 1 }))
        }
        Action::ActivateAbility { actor, source, ability_index: 0, granted_ability }
            if source.card_db_id == PRINTING && granted_ability.is_none() =>
        {
            if !facts.battlefield.iter().any(|equipment|
                equipment.card.controller == *actor && binds(&equipment.card, source))
            {
                return Err("Leyline Axe equip action lost its battlefield source".into());
            }
            Ok(Some(ActionV1::ActivateEquip { generic_cost: 3 }))
        }
        Action::ChooseTarget { actor, source, target, remaining } if source.card_db_id == PRINTING => {
            if *remaining != 1
                || !facts.equip.iter().any(|ability| ability.stack_position.is_none() && ability.target.is_none()
                    && ability.source.captured_controller == *actor && binds(&ability.source.original_source, source))
                || !matches!(target, TargetRefV1::Object { object } if object.zone == Zone::Battlefield && object.controller == *actor)
            {
                return Err("Leyline Axe target lost its native equip action".into());
            }
            Ok(Some(ActionV1::ChooseEquipTarget))
        }
        _ => Ok(None),
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
