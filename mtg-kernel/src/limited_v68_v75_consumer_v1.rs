//! Prospective V68 through V75 Foundations consumer profile for declared decks.
//! The closed list below identifies the eight exact Partial printings it admits.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    card_def::{CARD_DEFS, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    rl::{CommonProjectionScopeV42, PlayerSeatV1},
    rl_session::FastActorSessionV1,
};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-v68-v75-consumer/v1";
pub const PARTIAL_PRINTINGS_V1: [u16; 8] = [486, 487, 488, 489, 490, 491, 493, 494];

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("limited_v68_v75_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("bin/kernel_limited_v68_v75_env_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
        ] {
            hash.update([0]);
            hash.update(source.as_bytes());
        }
        hash.update(crate::limited_kaito_session_v16::producer_sha256_v16());
        hash.update(crate::current_mixed_policy_v1::feature_identity());
        format!("{:x}", hash.finalize())
    })
    .clone()
}

pub(crate) fn preflight(config: &ResetV1) -> Result<(), String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("V68-V75 consumer profile requires the Foundations catalog".into());
    }
    if config.expected_card_db_hash != KERNEL_CARDDB_HASH {
        return Err("catalog mismatch".into());
    }
    if config.max_physical_decisions == 0
        || config.max_policy_steps == 0
        || config.mainboards.iter().any(|deck| {
            !(40..=crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1).contains(&deck.len())
        })
    {
        return Err("invalid V68-V75 consumer profile reset".into());
    }

    // These meanings authenticate native definitions and programs, including
    // Liliana's Zombie and Kaito's Ninja. Neither token can appear in a deck.
    let exact = [
        crate::rules_meaning_time_stop_v68::printed_meaning()?.card_id,
        crate::rules_meaning_genesis_wave_v69::printed_meaning()?.card_id,
        crate::rules_meaning_vivien_v70::printed_meaning()?.card_id,
        crate::rules_meaning_muldrotha_v71::printed_meaning()?.card_id,
        crate::rules_meaning_tinybones_v72::printed_meaning()?.card_id,
        crate::rules_meaning_liliana_v73::printed_meaning()?.card_id,
        crate::rules_meaning_chandra_v74::printed_meaning()?.card_id,
        crate::rules_meaning_kaito_v75::printed_meaning()?.card_id,
    ];
    if exact != PARTIAL_PRINTINGS_V1 {
        return Err("V68-V75 consumer profile printing identity changed".into());
    }
    for deck in &config.mainboards {
        for &id in deck {
            let def = CARD_DEFS
                .get(usize::from(id))
                .ok_or("unknown declared card")?;
            if def.is_token
                || !(def.has_full_support() || (exact.contains(&id) && def.is_executable()))
            {
                return Err(format!("card {id} is outside the V68-V75 consumer profile"));
            }
        }
        crate::rl::require_declared_cards_for_scope(CommonProjectionScopeV42::KaitoV75, deck)?;
    }
    Ok(())
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_v68_v75_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

/// Reset a sealed V75 numeric owner with the existing current-slot sampler.
pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

/// Enable sorted public name/count minima before the first typed learner menu.
pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_v68_v75_v1(config, true)
        .map_err(|error| format!("{error:?}"))?;
    KaitoSession::from_session(owner).map_err(|error| error.to_string())
}

/// Reset a fixed-seat Human owner over the same typed native decisions.
pub fn reset_human_v1(
    config: &ResetV1,
    human_seat: PlayerSeatV1,
) -> Result<HumanKaitoSessionV75, String> {
    HumanKaitoSessionV75::from_session(native(config)?, human_seat)
        .map_err(|error| error.to_string())
}

/// Trusted two-seat host transport. The fixed profile discriminant selects
/// this reset gate, and the V16 payload projects only the current actor.
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
