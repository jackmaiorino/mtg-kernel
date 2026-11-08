//! Declared-deck owner for Consuming Aberration's CDA and cast trigger.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    aberration_public_v1::{self as public, FactsV1, PRINTING},
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-aberration-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/aberration-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/aberration-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/aberration-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("aberration_consumer_v1.rs"),
            include_str!("human_bo3_v1/aberration_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_aberration_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_aberration_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_aberration_v1(config, true)
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
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("invalid Aberration viewer facts".into());
    }
    let printing = IdentityV1::Known { card_id: PRINTING };
    for row in &facts.characteristics {
        let relative_player = if matches!(row.card.zone, Zone::Battlefield | Zone::Stack) {
            row.card.controller
        } else {
            row.card.owner
        };
        if row.card.identity != printing
            || row.relative_player != relative_player
            || row.base_from_cda.is_some_and(|value| value < 0)
        {
            return Err("invalid Aberration CDA facts".into());
        }
    }
    let mut menu_seen = false;
    for trigger in &facts.triggers {
        let source = &trigger.source.original_source;
        if source.identity != printing
            || source.zone != Zone::Battlefield
            || trigger.source.captured_controller != source.controller
            || trigger.library_owner == source.controller
        {
            return Err("invalid Aberration trigger source".into());
        }
        use public::StageV1;
        let revealed = trigger.stage == StageV1::Revealed;
        if (trigger.stage == StageV1::Collected) != trigger.stack_position.is_none()
            || (revealed && trigger.revealed_prefix.len() < 2)
            || (!revealed && !trigger.revealed_prefix.is_empty())
            || trigger.order.is_some() != (revealed && trigger.library_owner == actor)
            || trigger.revealed_prefix.iter().any(|card| {
                card.zone != Zone::Library
                    || card.owner != trigger.library_owner
                    || !matches!(card.identity, IdentityV1::Known { .. })
            })
        {
            return Err("invalid Aberration revealed prefix".into());
        }
        if let Some(order) = &trigger.order {
            let mut unique = std::collections::BTreeSet::new();
            if menu_seen
                || order.chooser != actor
                || order.legal.is_empty()
                || order.selected.len() + order.legal.len() != trigger.revealed_prefix.len()
                || order.selected.iter().chain(&order.legal).any(|card| {
                    !trigger.revealed_prefix.contains(card) || !unique.insert(card.object)
                })
            {
                return Err("invalid Aberration actual graveyard order".into());
            }
            menu_seen = true;
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
    card.identity
        == (IdentityV1::Known {
            card_id: stable.card_db_id,
        })
        && card.object == stable.arena_id
        && card.generation == stable.zone_change_count
        && card.owner == stable.owner
        && card.controller == stable.controller
        && card.zone == stable.zone
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some((trigger, order)) = facts.and_then(|facts| {
        facts
            .triggers
            .iter()
            .find_map(|trigger| trigger.order.as_ref().map(|order| (trigger, order)))
    }) else {
        return Ok(None);
    };
    let Action::ChooseEffectTarget {
        actor,
        source,
        target: TargetRefV1::Object { object },
        selected_count,
        min_targets,
        max_targets,
    } = action
    else {
        return Err("Aberration graveyard order lost its native action".into());
    };
    // The shared V75 action projection restores the captured source reference
    // even when the physical object has moved to a different incarnation.
    if !binds(&trigger.source.original_source, source)
        || *actor != order.chooser
        || usize::from(*selected_count) != order.selected.len()
        || min_targets != max_targets
        || usize::from(*min_targets) != order.selected.len() + order.legal.len()
        || !order.legal.iter().any(|card| binds(card, object))
    {
        return Err("Aberration graveyard answer lost its original binding".into());
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
