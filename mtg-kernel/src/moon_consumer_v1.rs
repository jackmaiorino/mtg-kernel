//! Declared-deck owning consumer for Imprisoned in the Moon's native attachment and mana choices.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    mana::ManaColor,
    moon_public_v1::{self as public, AttachmentKindV1, CastStageV1, FactsV1, PRINTING},
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-moon-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/moon-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/moon-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/moon-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("moon_consumer_v1.rs"),
            include_str!("human_bo3_v1/moon_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_moon_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_moon_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_moon_v1(config, true)
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
        return Err("invalid Moon viewer facts".into());
    }
    for cast in &facts.casts {
        if !exact_printing(&cast.spell) || cast.spell.zone != Zone::Stack
            || match cast.stage {
                CastStageV1::Announcing => cast.finalized_method.is_some() || cast.origin_zone.is_none(),
                CastStageV1::Published => cast.finalized_method.is_none() || cast.origin_zone.is_none() || cast.target.is_none(),
                CastStageV1::UncastCopy => cast.finalized_method.is_some() || cast.origin_zone.is_some() || cast.target.is_none(),
            }
        {
            return Err("invalid Moon native cast facts".into());
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
                return Err("invalid Moon original target facts".into());
            }
        }
    }
    for host in &facts.hosts {
        let any_current = host.auras.iter().any(|aura| aura.supplies_current_override);
        if host.card.zone != Zone::Battlefield || host.auras.is_empty()
            || (host.moon_sets_land || host.moon_sets_colorless || host.moon_grants_tap_for_colorless) && !any_current
            || host.granted_mana_available_now && (!host.moon_grants_tap_for_colorless || host.tapped)
            || host.auras.iter().enumerate().any(|(index, aura)| {
                !exact_printing(&aura.card) || aura.card.zone != Zone::Battlefield
                    || aura.card.object == host.card.object
                    || host.auras[..index].iter().any(|earlier| earlier.card.object == aura.card.object)
            })
        {
            return Err("invalid Moon live attachment or layer facts".into());
        }
        crate::tinybones_public_v72::nonnegative(&host.counters)?;
    }
    if let Some(menu) = &facts.attachment_menu {
        if !exact_printing(&menu.aura) || menu.legal_hosts.is_empty()
            || menu.selected_hosts.len() >= menu.total_hosts as usize
            || menu.selected_hosts.iter().chain(&menu.legal_hosts).any(|host| host.zone != Zone::Battlefield)
            || match menu.kind {
                AttachmentKindV1::CopiedAura => menu.aura.zone != Zone::Battlefield,
                AttachmentKindV1::ReturningAura => menu.total_hosts != 1 || !menu.selected_hosts.is_empty(),
            }
        {
            return Err("invalid Moon native attachment menu".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseCastTarget,
    ChooseAttachmentHost { kind: AttachmentKindV1 },
    TapForColorless,
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
    let Some(facts) = facts else { return Ok(None) };
    match action {
        Action::ChooseTarget { actor, source, target, remaining } if source.card_db_id == PRINTING => {
            if !facts.casts.iter().any(|cast| cast.stage == CastStageV1::Announcing
                && cast.spell.controller == *actor && binds(&cast.spell, source))
                || *remaining != 1
                || !matches!(target, TargetRefV1::Object { object } if object.zone == Zone::Battlefield)
            {
                return Err("Moon target does not match the native announcing cast".into());
            }
            Ok(Some(ActionV1::ChooseCastTarget))
        }
        Action::ChooseEffectTarget { actor, target, .. } => {
            let Some(menu) = &facts.attachment_menu else { return Ok(None) };
            if menu.chooser != *actor || !matches!(target, TargetRefV1::Object { object }
                if menu.legal_hosts.iter().any(|host| binds(host, object)))
            {
                return Err("Moon host action does not match the native attachment menu".into());
            }
            Ok(Some(ActionV1::ChooseAttachmentHost { kind: menu.kind }))
        }
        Action::ActivateManaAbility { actor, source, mana_choice: Some(ManaColor::C), cost_target: None }
            if facts.hosts.iter().any(|host| host.granted_mana_available_now
                && host.card.controller == *actor && binds(&host.card, source)) =>
        {
            Ok(Some(ActionV1::TapForColorless))
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
