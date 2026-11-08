//! Declared-deck owner for Electroduplicate copy targeting and flashback.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    electroduplicate_public_v1::{self as public, FactsV1, PRINTING},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-electroduplicate-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/electroduplicate-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/electroduplicate-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/electroduplicate-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("electroduplicate_consumer_v1.rs"),
            include_str!("human_bo3_v1/electroduplicate_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_electroduplicate_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_electroduplicate_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_electroduplicate_v1(config, true)
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
    use public::CastStageV1;
    if facts.viewer != actor || facts.schema != public::SCHEMA || facts.meaning != public::printed_meaning()? {
        return Err("invalid Electroduplicate viewer facts".into());
    }
    for cast in &facts.casts {
        if !exact_printing(&cast.spell)
            || cast.spell.zone != Zone::Stack
            || match cast.stage {
                CastStageV1::Announcing => {
                    cast.finalized_method.is_some() || cast.origin_zone.is_none()
                }
                CastStageV1::Published => {
                    cast.finalized_method.is_none()
                        || cast.origin_zone.is_none()
                        || cast.target.is_none()
                }
                CastStageV1::UncastCopy => {
                    cast.finalized_method.is_some()
                        || cast.origin_zone.is_some()
                        || cast.target.is_none()
                }
            }
        {
            return Err("invalid Electroduplicate native cast facts".into());
        }
        if let Some(target) = &cast.target {
            if target.original.zone != Zone::Battlefield
                || target.currently_legal && target.current_original.is_none()
                || target.current_original.as_ref().is_some_and(|current| {
                    current.zone != Zone::Battlefield
                        || current.owner != target.original.owner
                        || current.object != target.original.object
                        || current.generation != target.original.generation
                })
            {
                return Err("invalid Electroduplicate original target facts".into());
            }
        }
    }
    for token in &facts.copied_tokens {
        if token.card.zone != Zone::Battlefield || token.copiable_haste_and_each_end_step_sacrifice_instances == 0 {
            return Err("invalid Electroduplicate copied token facts".into());
        }
        crate::tinybones_public_v72::nonnegative(&token.counters)?;
    }
    Ok(())
}

/// Cross-check duplicated fields against the inherited public envelope. This
/// rejects inconsistent artifacts; native recapture still owns execution authority.
pub(crate) fn validate_inherited(
    facts: &FactsV1,
    value: &crate::kaito_observation_v75::KaitoObservationV75,
) -> Result<(), String> {
    use crate::rl::StackItemKindV2;
    use crate::state::CastMethodV4;
    let common = &value.common.common.projection.surface;
    let expected_casts = common.stack.iter().filter(|row|
        row.stack_item_kind == StackItemKindV2::Spell && row.source.card_db_id == PRINTING).count();
    if expected_casts != facts.casts.len() { return Err("Electroduplicate cast roster differs from common stack".into()); }
    for cast in &facts.casts {
        let row = common.stack.iter().find(|row| row.stack_index == cast.stack_position
            && row.stack_item_kind == StackItemKindV2::Spell && binds(&cast.spell, &row.source))
            .ok_or("Electroduplicate cast lost its inherited stack row")?;
        if row.controller != cast.spell.controller || row.cast_method != cast.finalized_method
            || row.is_copy != (cast.stage == public::CastStageV1::UncastCopy)
            || cast.finalized_method == Some(CastMethodV4::Flashback)
                && (cast.origin_zone != Some(Zone::Graveyard) || !row.is_flashback)
        { return Err("Electroduplicate cast route differs from common stack".into()); }
        let targets = if cast.stage == public::CastStageV1::Announcing {
            let pending = common.engine_context.pending_cast.as_ref().filter(|pending|
                pending.source.as_ref().is_some_and(|source| binds(&cast.spell, source)))
                .ok_or("Electroduplicate announcement lost its pending cast")?;
            if Some(pending.origin_zone) != cast.origin_zone || pending.controller != cast.spell.controller {
                return Err("Electroduplicate pending origin differs".into());
            }
            &pending.chosen_targets
        } else { &row.targets };
        match (cast.target.as_ref(), targets.as_slice()) {
            (None, []) => {},
            (Some(target), [TargetRefV1::Object { object }])
                if target.original.object == object.arena_id
                    && target.original.generation == object.zone_change_count
                    && target.original.owner == object.owner
                    && target.original.controller == object.controller
                    && target.original.zone == object.zone => {},
            _ => return Err("Electroduplicate original target differs from inherited stack".into()),
        }
    }
    let entries: Vec<_> = value.chandra.copied_tokens.iter().filter_map(|entry| {
        entry.source.current_original_source.as_ref().filter(|card|
            card.zone == Zone::Battlefield && matches!(card.identity, IdentityV1::Known { .. }))
            .map(|card| (card, entry.sacrifice_instances))
    }).collect();
    if entries.len() != facts.copied_tokens.len() { return Err("Electroduplicate token roster differs from V74".into()); }
    for token in &facts.copied_tokens {
        if entries.iter().filter(|(card, count)| *card == &token.card
            && *count == token.copiable_haste_and_each_end_step_sacrifice_instances).count() != 1
            || !value.tinybones.battlefield.iter().flatten().any(|current|
                current.card == token.card && current.is_token && current.form == token.form
                    && current.counters == token.counters && current.tapped == token.tapped
                    && current.summoning_sick == token.summoning_sick)
        { return Err("Electroduplicate current token differs from inherited facts".into()); }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 { ChooseCopyTarget }

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
    let Some(facts) = facts else { return Ok(None) };
    if let Action::ChooseTarget { actor, source, target, remaining } = action {
        if source.card_db_id == PRINTING {
            if *actor != facts.viewer || *remaining != 1
                || !facts.casts.iter().any(|cast| cast.stage == public::CastStageV1::Announcing
                    && cast.spell.controller == *actor && binds(&cast.spell, source))
                || !matches!(target, TargetRefV1::Object { object } if object.zone == Zone::Battlefield && object.controller == *actor)
            { return Err("Electroduplicate target lost its native source or controller".into()); }
            return Ok(Some(ActionV1::ChooseCopyTarget));
        }
    }
    Ok(None)
}

#[cfg(all(test, feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))]
mod tests;
