//! Declared-deck owner and wire for Abyssal Harvester's native tap and Nightmare copy.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    harvester_public_v1::{self as public, FactsV1, OutcomeV1, PRINTING},
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const PROFILE_V1: &str = "fdn-abyssal-harvester-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/harvester-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/harvester-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/harvester-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("harvester_consumer_v1.rs"),
            include_str!("human_bo3_v1/harvester_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_harvester_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_harvester_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_harvester_v1(config, true)
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

fn known(card: &ReferenceV1, zone: Zone) -> Option<u16> {
    if card.zone != zone {
        return None;
    }
    match card.identity {
        IdentityV1::Known { card_id } => Some(card_id),
        IdentityV1::FaceDown { .. } => None,
    }
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("Harvester facts lost exact viewer or printing".into());
    }
    let mut ids = BTreeSet::new();
    if facts
        .eligible_graveyard_cards
        .iter()
        .any(|row| known(row, Zone::Graveyard).is_none() || !ids.insert(row.object))
    {
        return Err("Harvester this-turn eligibility lost an original public creature card".into());
    }
    if let Some(choice) = &facts.chooser {
        let mut seen = BTreeSet::new();
        if known(&choice.source, Zone::Battlefield) != Some(PRINTING)
            || choice.source.controller != actor
            || choice.legal.is_empty()
            || choice.legal.iter().any(|card| {
                !facts.eligible_graveyard_cards.contains(card) || !seen.insert(card.object)
            })
        {
            return Err("Harvester chooser lost its exact original graveyard menu".into());
        }
    }
    if facts
        .sources
        .iter()
        .any(|row| known(&row.source, Zone::Battlefield) != Some(PRINTING))
        || facts
            .nightmare_copies
            .iter()
            .any(|row| row.card.zone != Zone::Battlefield || !row.copiable_nightmare)
    {
        return Err("Harvester battlefield source or copiable Nightmare changed".into());
    }
    for row in &facts.activations {
        let copied = known(&row.original_target, Zone::Graveyard)
            .ok_or("Harvester original target lost its public identity")?;
        let controller = row.source.captured_controller;
        if known(&row.source.original_source, Zone::Battlefield) != Some(PRINTING)
            || row.source.original_source.controller != controller
            || row.stack_position.is_some() != row.target_currently_legal.is_some()
            || row.stack_position.is_some() == row.outcome.is_some()
            || row.outcome != Some(OutcomeV1::Resolved)
                && (row.target_movement.is_some()
                    || !row.created.is_empty()
                    || !row.other_nightmare_movements.is_empty())
            || row.target_movement.as_ref().is_some_and(|movement| {
                movement.original.zone != Zone::Graveyard
                    || movement.original.object != row.original_target.object
                    || movement.original.generation != row.original_target.generation
                    || movement.original.owner != row.original_target.owner
                    || movement.identity != row.original_target.identity
            })
        {
            return Err(
                "Harvester activation lost its original target, controller or terminal".into(),
            );
        }
        let mut seen = BTreeSet::new();
        if row.created.iter().any(|token| {
            token.original.owner != controller
                || token.original.controller != controller
                || token.original.zone != Zone::Battlefield
                || token.copied_printing != copied
                || !token.added_copiable_nightmare
                || !seen.insert(token.original.object)
        }) || row.other_nightmare_movements.iter().any(|movement| {
            movement.original.zone != Zone::Battlefield
                || movement.original.controller != controller
                || !seen.insert(movement.original.object)
        }) {
            return Err("Harvester actual births and other Nightmare movements changed".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    AnnounceTapAbility,
    ChooseOriginalGraveyardTarget,
}

fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    known(card, stable.zone) == Some(stable.card_db_id)
        && card.object == stable.arena_id
        && card.generation == stable.zone_change_count
        && card.owner == stable.owner
        && card.controller == stable.controller
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else { return Ok(None) };
    if let Some(choice) = &facts.chooser {
        if let Action::ChooseTarget {
            actor,
            source,
            remaining: 1,
            target: TargetRefV1::Object { object },
        } = action
        {
            if *actor == facts.viewer
                && binds(&choice.source, source)
                && choice.legal.iter().any(|card| binds(card, object))
            {
                return Ok(Some(ActionV1::ChooseOriginalGraveyardTarget));
            }
        }
        return Err(
            "Harvester action changed its original chooser, source or graveyard target".into(),
        );
    }
    if let Action::ActivateAbility {
        actor,
        source,
        ability_index: 0,
        granted_ability,
    } = action
    {
        if source.card_db_id == PRINTING && granted_ability.is_none() {
            if *actor != facts.viewer
                || source.controller != *actor
                || !facts.sources.iter().any(|row| binds(&row.source, source))
            {
                return Err("Harvester activation lost original visible controller".into());
            }
            return Ok(Some(ActionV1::AnnounceTapAbility));
        }
    }
    Ok(None)
}

pub(crate) fn validate_candidate_order<'a>(
    facts: &FactsV1,
    actions: impl ExactSizeIterator<Item = &'a Action>,
) -> Result<(), String> {
    if let Some(choice) = &facts.chooser {
        if actions.len() != choice.legal.len()
            || !actions.zip(&choice.legal).all(|(action, card)| {
                matches!(action, Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }
                if binds(card, object))
            })
        {
            return Err("Harvester original graveyard choice order changed".into());
        }
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
