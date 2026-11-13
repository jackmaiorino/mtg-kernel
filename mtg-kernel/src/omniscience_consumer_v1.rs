//! Declared-deck owning consumer for Omniscience's native hand-cast routes.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    omniscience_public_v1::{self as public, FactsV1, RouteV1, StageV1, PRINTING},
    optional_trigger_public_v1::{IdentityV1, ReferenceV1, RootV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-omniscience-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/omniscience-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/omniscience-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/omniscience-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("omniscience_consumer_v1.rs"),
            include_str!("human_bo3_v1/omniscience_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_omniscience_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_omniscience_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_omniscience_v1(config, true)
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

fn permission_source(root: &RootV1, controller: PlayerSeatV1) -> bool {
    root.captured_controller == controller
        && root.original_source.zone == Zone::Battlefield
        && root.original_source.controller == controller
        && matches!(
            root.original_source.identity,
            IdentityV1::Known { card_id: PRINTING } | IdentityV1::FaceDown { .. }
        )
        && root.current_original_source.as_ref().is_none_or(|current| {
            current.zone == Zone::Battlefield
                && current.object == root.original_source.object
                && current.generation == root.original_source.generation
                && current.owner == root.original_source.owner
        })
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("invalid Omniscience viewer facts".into());
    }
    for (seat, source) in [PlayerSeatV1::P0, PlayerSeatV1::P1]
        .into_iter()
        .zip(&facts.current_permissions)
    {
        if source.as_ref().is_some_and(|source| {
            source.zone != Zone::Battlefield
                || source.controller != seat
                || source.identity != (IdentityV1::Known { card_id: PRINTING })
        }) {
            return Err("invalid Omniscience current permission source".into());
        }
    }
    for (index, offer) in facts.hand_offers.iter().enumerate() {
        if offer.card.zone != Zone::Hand
            || offer.card.owner != actor
            || offer.card.controller != actor
            || !permission_source(&offer.permission, actor)
            || !facts
                .current_permissions
                .iter()
                .flatten()
                .any(|source| source == &offer.permission.original_source)
            || facts.hand_offers[..index]
                .iter()
                .any(|prior| prior.card.object == offer.card.object)
        {
            return Err("invalid Omniscience actor hand offer".into());
        }
    }
    for cast in &facts.casts {
        if cast.origin_zone != Zone::Hand
            || cast.spell.zone != Zone::Stack
            || cast.spell.owner != cast.spell.controller
            || !permission_source(&cast.permission, cast.spell.controller)
            || cast.original_permission_active && cast.permission.current_original_source.is_none()
            || cast.route == Some(RouteV1::ReplaceBaseMana) && cast.x != Some(0)
            || cast.route != Some(RouteV1::ReplaceBaseMana) && !cast.ordinary_payment_was_available
            || (cast.stage == StageV1::Resolving) != cast.stack_position.is_none()
            || match cast.stage {
                StageV1::Announcing => {
                    cast.spell.controller != actor || cast.finalized_method.is_some()
                }
                StageV1::Published | StageV1::Resolving => {
                    cast.route.is_none()
                        || cast.x.is_none()
                        || cast.finalized_method.is_none()
                        || cast.payment_terms_now.is_some()
                }
            }
        {
            return Err("invalid Omniscience cast-route facts".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    AnnounceHandSpell { ordinary_payment_available: bool },
    ChooseRoute { route: RouteV1 },
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
    match action {
        Action::CastSpell { actor, source } if *actor == facts.viewer => Ok(facts
            .hand_offers
            .iter()
            .find(|offer| binds(&offer.card, source))
            .map(|offer| ActionV1::AnnounceHandSpell {
                ordinary_payment_available: offer.ordinary_payment_available,
            })),
        Action::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } => {
            let Some(cast) = facts
                .casts
                .iter()
                .find(|cast| cast.stage == StageV1::Announcing && cast.route.is_none())
            else {
                return Ok(None);
            };
            if *actor != facts.viewer || !binds(&cast.spell, source) || *option_count != 2 {
                return Err("Omniscience route action does not match the native menu".into());
            }
            let route = match option_index {
                0 => RouteV1::OrdinaryPayment,
                1 => RouteV1::ReplaceBaseMana,
                _ => return Err("Omniscience native route ordinal changed".into()),
            };
            Ok(Some(ActionV1::ChooseRoute { route }))
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
