//! Prospective finite Foundations consumer profile for Alesha, Nine-Lives and
//! Valkyrie's Call. Uses the complete typed V75 owner, Human and numeric paths.
//! This adds no Full capability or playing-strength qualification. It is a
//! trusted two-seat host interface, not a remote-player authorization service.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    card_def::{CARD_DEFS, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    rl::PlayerSeatV1,
    rl_session::FastActorSessionV1,
};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-return-consumer/v1";
pub const PARTIAL_PRINTINGS_V1: [u16; 3] = [457, 458, 459];

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("limited_return_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("rl_session/kaito_v75.rs"),
            include_str!("rl.rs"),
            include_str!("bin/kernel_limited_return_env_v1.rs"),
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
        return Err("return profile requires the Foundations catalog".into());
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
        return Err("invalid return profile reset".into());
    }
    // These are exhaustive comparators over actual CardDefs, programs and
    // triggers. Numeric expansion later validates the same typed meanings.
    use crate::rules_meaning_graveyard_return_v61::{self as graveyard, FamilyV61};
    let exact = [
        graveyard::printed_meaning(FamilyV61::Alesha)?.card_id,
        graveyard::printed_meaning(FamilyV61::NineLives)?.card_id,
        crate::rules_meaning_valkyries_call_v62::printed_meaning()?.card_id,
    ];
    if exact != PARTIAL_PRINTINGS_V1 {
        return Err("return profile printing identity changed".into());
    }
    for deck in &config.mainboards {
        for &id in deck {
            let def = CARD_DEFS
                .get(usize::from(id))
                .ok_or("unknown declared card")?;
            if def.is_token
                || !(def.has_full_support() || (exact.contains(&id) && def.is_executable()))
            {
                return Err(format!("card {id} is outside the return consumer profile"));
            }
        }
        crate::rl::require_declared_cards_for_scope(
            crate::rl::CommonProjectionScopeV42::KaitoV75,
            deck,
        )?;
    }
    Ok(())
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_return_v1(config).map_err(|error| format!("{error:?}"))
}

/// Genuine native owner is selected before its first surfaced decision.
/// Captures, score/apply, sampling and restore use the existing sealed V75 API.
pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

/// Same exact finite owner with the reveal reducer's public hand minima for
/// its authenticated learner input. The ledger starts before the first menu.
pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_return_reveal_facts_v3(config)
        .map_err(|error| format!("{error:?}"))?;
    KaitoSession::from_session(owner).map_err(|error| error.to_string())
}

/// A fixed-seat Human owner over the same exact profile and native decisions.
pub fn reset_human_v1(
    config: &ResetV1,
    human_seat: PlayerSeatV1,
) -> Result<HumanKaitoSessionV75, String> {
    HumanKaitoSessionV75::from_session(native(config)?, human_seat)
        .map_err(|error| error.to_string())
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
