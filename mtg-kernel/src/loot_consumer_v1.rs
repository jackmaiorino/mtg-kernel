//! Declared-deck owner for Loot land allowance and private top-six choices.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    loot_public_v1::{self as public, FactsV1, PRINTING},
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-loot-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/loot-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/loot-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/loot-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("loot_consumer_v1.rs"),
            include_str!("human_bo3_v1/loot_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_loot_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_loot_v1(config, false).map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_loot_v1(config, true)
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
        return Err("invalid Loot actor facts".into());
    }
    let printing = IdentityV1::Known { card_id: PRINTING };
    let mut seen = std::collections::BTreeSet::new();
    for permanent in &facts.battlefield {
        if permanent.card.identity != printing
            || permanent.card.zone != Zone::Battlefield
            || !seen.insert(permanent.card.object)
        {
            return Err("invalid Loot public permanent".into());
        }
        crate::tinybones_public_v72::nonnegative(&permanent.counters)?;
    }
    for (index, lands) in facts.lands.iter().enumerate() {
        let player = [PlayerSeatV1::P0, PlayerSeatV1::P1][index];
        let active = facts
            .battlefield
            .iter()
            .filter(|card| card.card.controller == player && card.form.printed_abilities_active)
            .count();
        if lands.player != player
            || usize::try_from(lands.total_plays_allowed).ok() != active.checked_add(1)
            || lands.remaining_plays
                != lands
                    .total_plays_allowed
                    .saturating_sub(u32::from(lands.plays_used))
        {
            return Err("invalid Loot current land allowance".into());
        }
    }
    let mut stack_positions = std::collections::BTreeSet::new();
    let mut private_choices = 0;
    for ability in &facts.abilities {
        let root = &ability.source;
        if root.original_source.identity != printing
            || root.original_source.zone != Zone::Battlefield
            || root.captured_controller != root.original_source.controller
            || root
                .current_original_source
                .as_ref()
                .is_some_and(|current| {
                    current.object != root.original_source.object
                        || current.generation != root.original_source.generation
                })
            || (ability.stage == public::StageV1::Published)
                != ability.native_stack_position.is_some()
            || ability
                .native_stack_position
                .is_some_and(|position| !stack_positions.insert(position))
        {
            return Err("invalid Loot original ability source or stage".into());
        }
        if let Some(choice) = &ability.choice {
            private_choices += 1;
            let mut looked = std::collections::BTreeSet::new();
            let mut chosen = std::collections::BTreeSet::new();
            if private_choices > 1
                || ability.stage != public::StageV1::Resolving
                || choice.chooser != actor
                || choice.chooser != root.captured_controller
                || choice.looked_in_original_order.len() > 6
                || choice.selected.len() > 1
                || choice.maximum_mana_value
                    != facts.lands[match actor {
                        PlayerSeatV1::P0 => 0,
                        PlayerSeatV1::P1 => 1,
                    }]
                    .controlled_lands
                || choice.looked_in_original_order.iter().any(|card| {
                    card.zone != Zone::Library
                        || card.owner != actor
                        || !matches!(card.identity, IdentityV1::Known { .. })
                        || !looked.insert(card.object)
                })
                || choice.selected.iter().chain(&choice.legal).any(|card| {
                    !choice.looked_in_original_order.contains(card) || !chosen.insert(card.object)
                })
            {
                return Err("invalid Loot chooser-private look".into());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseCreature,
    ConfirmCreature,
    DeclineCreature,
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
    let Some(facts) = facts else { return Ok(None) };
    let (actor, source, selected_count) = match action {
        Action::ChooseEffectTarget {
            actor,
            source,
            selected_count,
            ..
        }
        | Action::FinishEffectSelection {
            actor,
            source,
            selected_count,
        } => (actor, source, selected_count),
        _ => return Ok(None),
    };
    if source.card_db_id != PRINTING {
        return Ok(None);
    }
    let ability = facts
        .abilities
        .iter()
        .find(|ability| ability.choice.is_some() && binds(&ability.source.original_source, source))
        .ok_or("Loot private action lost its native ability")?;
    let choice = ability.choice.as_ref().unwrap();
    if *actor != facts.viewer
        || *actor != choice.chooser
        || usize::from(*selected_count) != choice.selected.len()
    {
        return Err("Loot action changed original chooser or selected count".into());
    }
    match action {
        Action::ChooseEffectTarget {
            target: TargetRefV1::Object { object },
            min_targets: 0,
            max_targets: 1,
            ..
        } if choice.legal.iter().any(|card| binds(card, object)) => {
            Ok(Some(ActionV1::ChooseCreature))
        }
        Action::FinishEffectSelection { .. } if choice.selected.is_empty() => {
            Ok(Some(ActionV1::DeclineCreature))
        }
        Action::FinishEffectSelection { .. } if choice.selected.len() == 1 => {
            Ok(Some(ActionV1::ConfirmCreature))
        }
        _ => Err("Loot action changed original optional creature menu".into()),
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
