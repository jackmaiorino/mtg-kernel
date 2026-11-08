//! Declared-deck owning consumer for the exact Embercleave printing.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    embercleave_public_v65::{AbilityKindV65, FactsV65, StageV65},
    human_bo3_v1::HumanKaitoSessionV75,
    ids::PlayerId,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-embercleave-consumer/v1";
pub const PRINTING_V1: u16 = 462;
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/embercleave-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/embercleave-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/embercleave-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("embercleave_consumer_v1.rs"),
            include_str!("embercleave_public_v65.rs"),
            include_str!("human_bo3_v1/embercleave_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_embercleave_env_v1.rs"),
        ] {
            hash.update([0]);
            hash.update(source.as_bytes());
        }
        hash.update(crate::rules_meaning_embercleave_v65::producer_sha256());
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
        || crate::rules_meaning_embercleave_v65::printed_meaning().map(|meaning| meaning.card_id),
        "Embercleave",
    )
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_embercleave_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

/// The public name/count reducer is installed before the first typed menu.
pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_embercleave_v1(config, true)
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

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV65, String> {
    let mut facts = crate::embercleave_public_v65::capture(state, viewer)?;
    facts
        .authorized
        .retain(|card| card.zone != Zone::Hand || card.owner == facts.viewer);
    Ok(facts)
}

pub(crate) fn validate_facts(facts: &FactsV65, actor: PlayerSeatV1) -> Result<(), String> {
    crate::rules_meaning_embercleave_v65::validate(&facts.meaning)?;
    if facts.viewer != actor
        || facts.schema != crate::embercleave_public_v65::SCHEMA
        || facts.producer_sha256 != crate::embercleave_public_v65::producer_sha256()
        || facts.complete_ordinary_observation
        || facts.owning_consumer_admitted
        || facts.authorized.iter().any(|card| {
            card.identity != (IdentityV1::Known { card_id: PRINTING_V1 })
                || (card.zone == Zone::Hand && card.owner != actor)
        })
    {
        return Err("invalid Embercleave actor facts".into());
    }
    for (quote, seat) in facts
        .current_cast_reduction
        .iter()
        .zip([PlayerSeatV1::P0, PlayerSeatV1::P1])
    {
        if quote.controller != seat
            || usize::try_from(quote.count).ok() != Some(quote.current_attackers.len())
            || u32::from(quote.generic_due_now) != 4u32.saturating_sub(quote.count)
            || quote.retained_red_pips != 2
            || quote.printed_mana_value != 6
            || quote
                .current_attackers
                .iter()
                .any(|r| r.controller != seat || r.zone != Zone::Battlefield)
        {
            return Err("invalid Embercleave current cast quote".into());
        }
    }
    for ability in &facts.abilities {
        let controller = ability.source.captured_controller;
        if ability.source.original_source.zone != Zone::Battlefield
            || ability.source.original_source.controller != controller
            || ability.original_targets.len() > 1
            || ability.selection_complete != (ability.original_targets.len() == 1)
            || ability.payment_complete != (ability.stage != StageV65::PayingEquip)
            || ability
                .current_candidate_pool
                .iter()
                .any(|r| r.controller != controller || r.zone != Zone::Battlefield)
        {
            return Err("invalid Embercleave original ability context".into());
        }
    }
    Ok(())
}

pub(crate) fn is_attachment_program(effect: &crate::effect::EffectOp) -> bool {
    effect == &crate::effect::EffectOp::AttachSourceToTarget {
        object: crate::effect::ObjectRef::Target(0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    /// A current printed quote. The ordinary action still binds the actual
    /// native route, costs and executable index.
    CastQuote {
        controlled_attackers: u32,
        generic_due_now: u8,
        retained_red_pips: u8,
    },
    ActivateEquip {
        printed_generic_cost: u8,
    },
    AttachTarget {
        kind: AbilityKindV65,
        stage: StageV65,
        position: u32,
    },
}

fn matches_reference(reference: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    let printing = match reference.identity {
        IdentityV1::Known { card_id } => Some(card_id),
        IdentityV1::FaceDown { disguised } => crate::card_def::card_id_by_name(if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        }),
    };
    printing == Some(stable.card_db_id)
        && reference.object == stable.arena_id
        && reference.generation == stable.zone_change_count
        && reference.owner == stable.owner
        && reference.controller == stable.controller
        && reference.zone == stable.zone
}

pub(crate) fn action(
    facts: Option<&FactsV65>,
    action: &Action,
) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else {
        return Ok(None);
    };
    match action {
        Action::CastSpell { actor, source } if source.card_db_id == PRINTING_V1 => {
            let quote = facts
                .current_cast_reduction
                .iter()
                .find(|quote| quote.controller == *actor)
                .ok_or("missing Embercleave current cast quote")?;
            Ok(Some(ActionV1::CastQuote {
                controlled_attackers: quote.count,
                generic_due_now: quote.generic_due_now,
                retained_red_pips: quote.retained_red_pips,
            }))
        }
        Action::ActivateAbility { source, ability_index: 0, granted_ability, .. }
            if source.card_db_id == PRINTING_V1 && granted_ability.is_none() =>
        {
            Ok(Some(ActionV1::ActivateEquip { printed_generic_cost: 3 }))
        }
        Action::ChooseTarget { actor, source, target, .. } => {
            let mut matching = facts.abilities.iter().filter(|ability| {
                matches!(ability.stage, StageV65::ChoosingEntryTarget | StageV65::PayingEquip)
                    && !ability.selection_complete
                    && ability.source.captured_controller == *actor
                    && matches_reference(&ability.source.original_source, source)
            });
            let Some(ability) = matching.next() else {
                return if source.card_db_id == PRINTING_V1 {
                    Err("Embercleave target lost its original attachment choice".into())
                } else {
                    Ok(None)
                };
            };
            if matching.next().is_some()
                || !matches!(target, TargetRefV1::Object { object }
                    if ability.current_candidate_pool.iter().any(|candidate| matches_reference(candidate, object)))
            {
                return Err("Embercleave target does not bind the original attachment menu".into());
            }
            Ok(Some(ActionV1::AttachTarget {
                kind: ability.kind,
                stage: ability.stage,
                position: ability.position,
            }))
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
