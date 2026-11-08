//! An explicit owning profile composing the represented Foundations families.
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

pub const PROFILE_V1: &str = "fdn-mixed-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/mixed-fdn-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/mixed-fdn-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/mixed-fdn-candidate-meaning/v1";
pub const PARTIAL_PRINTINGS_V1: [u16; 20] = [
    457, 458, 459, 460, 461, 462, 463, 464, 465, 466, 470, 478, 486, 487, 488, 489, 490, 491, 493,
    494,
];

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("limited_mixed_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("policy_surface_v5.rs"),
            include_str!("bin/kernel_limited_mixed_env_v1.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
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
    )) || config.expected_card_db_hash != KERNEL_CARDDB_HASH
        || config.max_physical_decisions == 0
        || config.max_policy_steps == 0
        || config.mainboards.iter().any(|deck| {
            !(40..=crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1).contains(&deck.len())
        })
    {
        return Err("invalid mixed Foundations profile reset".into());
    }
    let exact = exact_partial_printings()?;
    for &id in config.mainboards.iter().flatten() {
        let card = CARD_DEFS
            .get(usize::from(id))
            .ok_or("unknown declared card")?;
        if card.is_token
            || !(card.has_full_support() || (exact.contains(&id) && card.is_executable()))
        {
            return Err(format!(
                "card {id} is outside the mixed Foundations profile"
            ));
        }
        if !exact.contains(&id) {
            crate::rl::require_declared_cards_for_scope(
                crate::rl::CommonProjectionScopeV42::KaitoV75,
                std::slice::from_ref(&id),
            )?;
        }
    }
    Ok(())
}

pub(crate) fn exact_partial_printings() -> Result<[u16; 20], String> {
    use crate::rules_meaning_chosen_type_v66::{self as chosen, FamilyV66};
    use crate::rules_meaning_graveyard_return_v61::{self as graveyard, FamilyV61};
    // Authenticate the exact native definitions and generated tokens before
    // admitting their real IDs. No replacement card or alternate deck is used.
    let exact = [
        graveyard::printed_meaning(FamilyV61::Alesha)?.card_id,
        graveyard::printed_meaning(FamilyV61::NineLives)?.card_id,
        crate::rules_meaning_valkyries_call_v62::printed_meaning()?.card_id,
        crate::rules_meaning_artist_v64::printed_meaning()?.card_id,
        crate::rules_meaning_niv_mizzet_v63::printed_meaning()?.card_id,
        crate::rules_meaning_embercleave_v65::printed_meaning()?.card_id,
        chosen::printed_meaning(FamilyV66::BannerOfKinship)?.card_id,
        crate::rules_meaning_drakuseth_v67::printed_meaning()?.card_id,
        crate::zimone_public_v1::printed_meaning()?.card_id,
        crate::tutelage_public_v1::printed_meaning()?.card_id,
        chosen::printed_meaning(FamilyV66::SecludedCourtyard)?.card_id,
        crate::omniscience_public_v1::printed_meaning()?.card_id,
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
        return Err("mixed Foundations printing identities changed".into());
    }
    Ok(exact)
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_mixed_v1(config, false).map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_mixed_v1(config, true)
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
