//! Declared-deck owner for Sphinx attack grants and native flashback cost routes.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    sphinx_lore_public_v1::{self as public, FactsV1, PRINTING},
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-sphinx-lore-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/sphinx-lore-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/sphinx-lore-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/sphinx-lore-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("sphinx_lore_consumer_v1.rs"),
            include_str!("human_bo3_v1/sphinx_lore_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_sphinx_lore_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_sphinx_lore_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_sphinx_lore_v1(config, true)
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

fn same_original(a: &ReferenceV1, b: &ReferenceV1) -> bool {
    a.object == b.object && a.generation == b.generation && a.owner == b.owner && a.zone == b.zone
}
fn original(root: &crate::optional_trigger_public_v1::RootV1) -> bool {
    root.original_source.zone == Zone::Battlefield
        && root.original_source.controller == root.captured_controller
        && root.original_source.identity == (IdentityV1::Known { card_id: PRINTING })
        && root
            .current_original_source
            .as_ref()
            .is_none_or(|r| same_original(r, &root.original_source))
}
pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("invalid Sphinx viewer or printed meaning".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for row in &facts.battlefield {
        if row.card.zone != Zone::Battlefield
            || row.card.identity != (IdentityV1::Known { card_id: PRINTING })
            || !ids.insert(row.card.object)
        {
            return Err("invalid Sphinx permanent roster".into());
        }
        crate::tinybones_public_v72::nonnegative(&row.counters)?;
    }
    let mut positions = std::collections::BTreeSet::new();
    for (index, attack) in facts.attacks.iter().enumerate() {
        if !original(&attack.source) {
            return Err("invalid Sphinx original attack source".into());
        }
        if let Some((stage, position)) = attack.pending {
            let stage = match stage {
                public::StageV1::Held => 0,
                public::StageV1::Ordered => 1,
                public::StageV1::Published => 2,
            };
            if !positions.insert((stage, position))
                || stage == 2 && attack.original_target.is_none()
            {
                return Err("invalid Sphinx pending attack position".into());
            }
        }
        if let Some(target) = &attack.original_target {
            if target.original.zone != Zone::Graveyard
                || target.original.owner != attack.source.captured_controller
                || target
                    .current_original
                    .as_ref()
                    .is_some_and(|c| !same_original(c, &target.original))
                || target.currently_legal && target.current_original.is_none()
            {
                return Err("invalid Sphinx original attack target".into());
            }
        }
        if let Some(grant) = attack.installed_grant {
            if facts
                .grants
                .get(grant as usize)
                .is_none_or(|g| g.attack as usize != index)
            {
                return Err("invalid Sphinx attack grant binding".into());
            }
        }
    }
    for (index, grant) in facts.grants.iter().enumerate() {
        let attack = facts
            .attacks
            .get(grant.attack as usize)
            .ok_or("Sphinx grant attack absent")?;
        let IdentityV1::Known { card_id } = grant.original_card.identity else {
            return Err("Sphinx grant lost original public printing".into());
        };
        let def = crate::card_def::CARD_DEFS
            .get(usize::from(card_id))
            .ok_or("Sphinx grant printing absent")?;
        if attack.installed_grant != Some(index as u32)
            || grant.holder != attack.source.captured_controller
            || attack
                .original_target
                .as_ref()
                .is_none_or(|t| t.original != grant.original_card)
            || grant.original_card.zone != Zone::Graveyard
            || grant.original_card.owner != grant.holder
            || grant.mana_cost != def.cost.into()
            || grant
                .current_original_graveyard
                .as_ref()
                .is_some_and(|c| !same_original(c, &grant.original_card))
            || grant.original_incarnation_has_active_grant
                != (grant.until_cleanup_active && grant.current_original_graveyard.is_some())
        {
            return Err("invalid Sphinx original grant".into());
        }
    }
    let mut cast_ids = std::collections::BTreeSet::new();
    for cast in &facts.casts {
        if !cast_ids.insert(cast.spell.object)
            || cast.spell.object != cast.original_graveyard.object
            || cast.original_graveyard.zone != Zone::Graveyard
            || cast.spell.owner != cast.original_graveyard.owner
            || cast.spell.generation <= cast.original_graveyard.generation
            || cast.routes.is_empty()
            || cast.routes.len() > usize::from(u16::MAX)
            || cast
                .selected
                .is_some_and(|i| usize::from(i) >= cast.routes.len())
            || cast.stage == public::CastStageV1::Announcing && cast.finalized_method.is_some()
            || cast.stage != public::CastStageV1::Announcing
                && (cast.selected.is_none() || cast.finalized_method.is_none())
        {
            return Err("invalid Sphinx current cast binding".into());
        }
        let IdentityV1::Known { card_id } = cast.spell.identity else {
            return Err("Sphinx public cast printing absent".into());
        };
        let def = crate::card_def::CARD_DEFS
            .get(usize::from(card_id))
            .ok_or("Sphinx cast printing absent")?;
        for (i, option) in cast.routes.iter().enumerate() {
            if cast.routes[..i].iter().any(|r| r.route == option.route) {
                return Err("Sphinx duplicate route".into());
            }
            let (expected_cost, expected_method) = match option.route {
                public::RouteV1::Normal => (
                    vec![public::CostV1::Mana(def.cost.into())],
                    crate::state::CastMethodV4::Normal,
                ),
                public::RouteV1::PrintedFlashback => (
                    public::costs(def.flashback.as_ref().ok_or("Sphinx printed flashback absent")?.cost)?,
                    crate::state::CastMethodV4::Flashback,
                ),
                public::RouteV1::Escape => (
                    public::costs(def.escape.as_ref().ok_or("Sphinx printed escape absent")?.cost)?,
                    crate::state::CastMethodV4::Escape,
                ),
                public::RouteV1::Granted { .. } => (
                    vec![public::CostV1::Mana(def.cost.into())],
                    crate::state::CastMethodV4::Flashback,
                ),
            };
            if option.ordered_base_cost != expected_cost
                || cast.selected == Some(i as u16)
                    && cast.finalized_method.is_some_and(|method| {
                        method != expected_method
                            && !(option.route == public::RouteV1::Normal
                                && method == crate::state::CastMethodV4::Alternative)
                    })
            {
                return Err("Sphinx route changed its printed cost or finalized method".into());
            }
            if let public::RouteV1::Granted { grant } = option.route {
                let grant = facts
                    .grants
                    .get(grant as usize)
                    .ok_or("Sphinx cast original grant absent")?;
                if grant.original_card.object != cast.spell.object
                    || grant.original_card.generation != cast.original_graveyard.generation
                    || grant.holder != cast.spell.controller
                    || grant.mana_cost != def.cost.into()
                    || option.ordered_base_cost != [public::CostV1::Mana(def.cost.into())]
                {
                    return Err("Sphinx route changed its original grant or printed cost".into());
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseAttackTarget,
    ChooseCastRoute {
        native_option_index: u16,
        route: public::RouteV1,
    },
}
fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    matches!(card.identity, IdentityV1::Known { card_id } if card_id == stable.card_db_id)
        && card.object == stable.arena_id
        && card.generation == stable.zone_change_count
        && card.owner == stable.owner
        && card.controller == stable.controller
        && card.zone == stable.zone
}
pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else {
        return Ok(None);
    };
    match action {
        Action::ChooseTarget {
            actor,
            source,
            target,
            ..
        } => {
            let matches: Vec<_> = facts
                .attacks
                .iter()
                .filter(|row| {
                    row.pending == Some((public::StageV1::Ordered, 0))
                        && binds(&row.source.original_source, source)
                })
                .collect();
            let [attack] = matches.as_slice() else {
                if source.card_db_id == PRINTING {
                    return Err("Sphinx target action lost original attack".into());
                }
                return Ok(None);
            };
            if *actor != facts.viewer
                || *actor != attack.source.captured_controller
                || attack.original_target.is_some()
                || !matches!(target, TargetRefV1::Object { object } if object.zone == Zone::Graveyard && object.owner == *actor
                    && crate::card_def::CARD_DEFS.get(usize::from(object.card_db_id)).is_some_and(|d|
                        d.types.iter().any(|t| matches!(t, crate::card_def::CardType::Instant | crate::card_def::CardType::Sorcery))))
            {
                return Err("Sphinx action changed its attack target or holder".into());
            }
            Ok(Some(ActionV1::ChooseAttackTarget))
        }
        Action::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } => {
            let matches: Vec<_> = facts
                .casts
                .iter()
                .filter(|row| {
                    row.stage == public::CastStageV1::Announcing
                        && row.selected.is_none()
                        && binds(&row.spell, source)
                })
                .collect();
            let [cast] = matches.as_slice() else {
                if facts.casts.iter().any(|row| {
                    row.stage == public::CastStageV1::Announcing && row.selected.is_none()
                }) {
                    return Err("Sphinx cast choice lost its original public source".into());
                }
                return Ok(None);
            };
            if *actor != facts.viewer
                || *actor != cast.spell.controller
                || usize::from(*option_count) != cast.routes.len()
            {
                return Err("Sphinx original cast-choice actor or count changed".into());
            }
            let option = cast
                .routes
                .get(usize::from(*option_index))
                .ok_or("Sphinx native route index changed")?;
            Ok(Some(ActionV1::ChooseCastRoute {
                native_option_index: *option_index,
                route: option.route,
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
