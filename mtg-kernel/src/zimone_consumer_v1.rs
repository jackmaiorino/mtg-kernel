//! Declared-deck owning consumer for Zimone's native counter choices.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
    zimone_public_v1::{self as public, AbilityKindV1, FactsV1, StageV1, PRINTING},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-zimone-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/zimone-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/zimone-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/zimone-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("zimone_consumer_v1.rs"),
            include_str!("human_bo3_v1/zimone_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_zimone_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_zimone_v1(config, false).map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_zimone_v1(config, true)
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
        || facts.visible_printings.iter().any(|card| {
            card.identity != (IdentityV1::Known { card_id: PRINTING })
                || card.zone == Zone::Library
                || (card.zone == Zone::Hand && card.owner != actor)
        })
    {
        return Err("invalid Zimone viewer facts".into());
    }
    for (seat, rows) in [PlayerSeatV1::P0, PlayerSeatV1::P1]
        .into_iter()
        .zip(&facts.battlefield)
    {
        for row in rows {
            if row.card.zone != Zone::Battlefield || row.card.controller != seat {
                return Err("invalid Zimone battlefield row".into());
            }
            crate::tinybones_public_v72::nonnegative(&row.counters)?;
        }
    }
    for row in &facts.abilities {
        if row.source.original_source.zone != Zone::Battlefield
            || row.source.original_source.controller != row.source.captured_controller
            || row.original_targets.len() > 2
            || match row.stage {
                StageV1::HeldCombat | StageV1::OrderedCombat => {
                    row.kind != AbilityKindV1::CombatPutPlusOne
                }
                StageV1::PendingActivation => row.kind != AbilityKindV1::ActivateDoubleEveryKind,
                StageV1::Published => false,
            }
        {
            return Err("invalid Zimone ability context".into());
        }
        for (index, target) in row.original_targets.iter().enumerate() {
            if target.original.zone != Zone::Battlefield
                || target.original.controller != row.source.captured_controller
                || row.original_targets[..index]
                    .iter()
                    .any(|prior| prior.original.object == target.original.object)
                || target.currently_legal && target.current_original.is_none()
                || target.current_original.as_ref().is_some_and(|current| {
                    current.object != target.original.object
                        || current.generation != target.original.generation
                        || current.owner != target.original.owner
                        || current.zone != Zone::Battlefield
                })
            {
                return Err("invalid Zimone original target relation".into());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ActivateDoubleEveryKind,
    TargetChoice {
        kind: AbilityKindV1,
        position: u32,
        selected_count: u8,
        finish: bool,
    },
}

fn same_printed_source(reference: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    reference.identity == (IdentityV1::Known { card_id: PRINTING })
        && reference.object == stable.arena_id
        && reference.generation == stable.zone_change_count
        && reference.owner == stable.owner
        && reference.controller == stable.controller
        && reference.zone == stable.zone
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else {
        return Ok(None);
    };
    let (source, actor, stage, selected_count, target) = match action {
        Action::ActivateAbility {
            source,
            ability_index: 0,
            granted_ability,
            ..
        } if source.card_db_id == PRINTING && granted_ability.is_none() => {
            return Ok(Some(ActionV1::ActivateDoubleEveryKind));
        }
        Action::ChooseTarget {
            source,
            actor,
            remaining,
            target,
        } if source.card_db_id == PRINTING => (
            source,
            *actor,
            StageV1::OrderedCombat,
            2u16.checked_sub(u16::from(*remaining))
                .ok_or("Zimone trigger target cap changed")?,
            Some(target),
        ),
        Action::FinishTargetSelection {
            source,
            actor,
            selected_count,
        } => (
            source,
            *actor,
            StageV1::OrderedCombat,
            *selected_count,
            None,
        ),
        Action::ChooseEffectTarget {
            source,
            actor,
            target,
            selected_count,
            min_targets,
            max_targets,
        } if source.card_db_id == PRINTING => {
            if (*min_targets, *max_targets) != (0, 2) {
                return Err("Zimone activation target bounds changed".into());
            }
            (
                source,
                *actor,
                StageV1::PendingActivation,
                *selected_count,
                Some(target),
            )
        }
        Action::FinishEffectSelection {
            source,
            actor,
            selected_count,
        } => (
            source,
            *actor,
            StageV1::PendingActivation,
            *selected_count,
            None,
        ),
        _ => return Ok(None),
    };
    if source.card_db_id != PRINTING {
        return Ok(None);
    }
    let mut matching = facts.abilities.iter().filter(|row| {
        row.stage == stage
            && (stage != StageV1::OrderedCombat || row.position == 0)
            && row.source.captured_controller == actor
            && same_printed_source(&row.source.original_source, source)
    });
    let row = matching
        .next()
        .ok_or("missing original Zimone target menu")?;
    if matching.next().is_some()
        || selected_count >= 2
        || usize::from(selected_count) != row.original_targets.len()
        || target.is_some_and(|target| {
            !matches!(target, TargetRefV1::Object { object }
                if object.zone == Zone::Battlefield
                    && object.controller == actor
                    && !row.original_targets.iter().any(|prior| prior.original.object == object.arena_id))
        })
    {
        return Err("Zimone choice differs from original native target prefix".into());
    }
    Ok(Some(ActionV1::TargetChoice {
        kind: row.kind,
        position: row.position,
        selected_count: selected_count as u8,
        finish: target.is_none(),
    }))
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
