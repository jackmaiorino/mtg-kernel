//! Declared-deck owning consumer for the exact Niv-Mizzet, Visionary printing.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    niv_mizzet_public_v63::{ConsumerFactsV1, DrawOutcomeV1, StageV63},
    optional_trigger_public_v1::IdentityV1,
    rl::PlayerSeatV1,
    rl_session::FastActorSessionV1,
    state::Zone,
};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-niv-consumer/v1";
pub const PRINTING_V1: u16 = 461;
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/niv-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/niv-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/niv-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("niv_consumer_v1.rs"),
            include_str!("niv_mizzet_public_v63.rs"),
            include_str!("engine/niv_mizzet_resolution_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_niv_env_v1.rs"),
        ] {
            hash.update([0]);
            hash.update(source.as_bytes());
        }
        hash.update(crate::rules_meaning_niv_mizzet_v63::producer_sha256());
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
        || crate::rules_meaning_niv_mizzet_v63::printed_meaning().map(|meaning| meaning.card_id),
        "Niv-Mizzet",
    )
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_niv_v1(config, false).map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

/// The public name/count reducer is installed before the first typed menu.
pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_niv_v1(config, true)
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

pub(crate) fn validate_facts(facts: &ConsumerFactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    let appendix = &facts.appendix;
    crate::rules_meaning_niv_mizzet_v63::validate(&appendix.meaning)?;
    if appendix.viewer != actor
        || appendix.schema != crate::niv_mizzet_public_v63::SCHEMA
        || appendix.producer_sha256 != crate::niv_mizzet_public_v63::producer_sha256()
        || appendix.complete_ordinary_observation
        || appendix.owning_consumer_admitted
        || appendix.authorized.iter().any(|card| {
            card.identity
                != (IdentityV1::Known {
                    card_id: PRINTING_V1,
                })
                || (card.zone == Zone::Hand && card.owner != actor)
        })
    {
        return Err("invalid Niv actor facts".into());
    }
    for (ordinal, draw) in appendix.damage_draws.iter().enumerate() {
        if usize::try_from(draw.occurrence).ok() != Some(ordinal)
            || draw.committed_damage <= 0
            || draw.draw_count != draw.committed_damage
            || draw.recipient == draw.observer.captured_controller
            || draw.damage_source.damage_controller != draw.observer.captured_controller
            || draw.native_position.is_some()
                != matches!(draw.stage, StageV63::Held | StageV63::Published)
        {
            return Err("invalid Niv damage and commanded draw context".into());
        }
    }
    let mut previous = None;
    for result in &facts.results {
        let row = appendix
            .damage_draws
            .get(usize::try_from(result.occurrence).map_err(|_| "Niv draw ordinal overflow")?)
            .ok_or("Niv result missing its public occurrence")?;
        if previous.is_some_and(|at| at >= result.occurrence)
            || row.stage != StageV63::NoCurrentExecutableRow
            || matches!(result.outcome, DrawOutcomeV1::Resolved { cards_drawn }
                if u32::try_from(row.draw_count).is_err() || cards_drawn > row.draw_count as u32)
        {
            return Err("invalid Niv realized outcome".into());
        }
        previous = Some(result.occurrence);
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
