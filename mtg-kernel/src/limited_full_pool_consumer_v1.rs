//! Declared-deck owner for the frozen 286-printing Foundations Limited pool.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    card_def::{card_id_by_name, CARD_DEFS, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    rl::PlayerSeatV1,
    rl_session::FastActorSessionV1,
    state::GameState,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::OnceLock};

pub mod life_triggers;

pub const PROFILE_V1: &str = "fdn-full-pool-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/full-pool-fdn-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/full-pool-fdn-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/full-pool-fdn-candidate-meaning/v1";
pub const HUMAN_DECISION_SCHEMA: &str = "mtg-kernel-human-full-pool-fdn-decision/v1";
pub const WIRE_DECISION_SCHEMA: &str = "mtg-kernel-limited-full-pool-fdn-decision/v1";

pub(crate) const FROZEN_POOL_JSON_V1: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/limited/fdn_v1/booster_pool_v1.json"
));

pub fn producer_sha256_v1() -> String {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            FROZEN_POOL_JSON_V1,
            include_str!("limited_full_pool_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("policy_surface_v5.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_full_pool_env_v1.rs"),
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

#[derive(Deserialize)]
struct FrozenPoolV1 {
    card_names: Vec<String>,
}

fn authenticate_frozen_pool() -> Result<BTreeSet<u16>, String> {
    let frozen: FrozenPoolV1 =
        serde_json::from_str(FROZEN_POOL_JSON_V1).map_err(|error| error.to_string())?;
    let early = crate::limited_mixed_consumer_v1::exact_partial_printings()?;
    let late = [
        crate::moon_public_v1::printed_meaning()?.card_id,
        crate::leyline_axe_public_v1::printed_meaning()?.card_id,
        crate::progenitus_public_v1::printed_meaning()?.card_id,
        crate::aberration_public_v1::printed_meaning()?.card_id,
        crate::lathril_public_v1::printed_meaning()?.card_id,
        crate::electroduplicate_public_v1::printed_meaning()?.card_id,
        crate::edict_public_v1::printed_meaning()?.card_id,
        crate::loot_public_v1::printed_meaning()?.card_id,
        crate::curator_public_v1::printed_meaning()?.card_id,
        crate::kykar_public_v1::printed_meaning()?.card_id,
        crate::sphinx_lore_public_v1::printed_meaning()?.card_id,
        crate::greatwurm_public_v1::printed_meaning()?.card_id,
        crate::harvester_public_v1::printed_meaning()?.card_id,
        crate::fiend_artisan_public_v1::printed_meaning()?.card_id,
        crate::etali_primal_storm_public_v1::printed_meaning()?.card_id,
        crate::thousand_year_storm_public_v1::printed_meaning()?.card_id,
    ];
    let partial: BTreeSet<_> = early.into_iter().chain(late).collect();
    if partial.len() != 36 {
        return Err("full-pool exact Partial printing roster changed".into());
    }
    let mut printings = BTreeSet::new();
    let mut legacy_full = Vec::new();
    for name in frozen.card_names {
        let id =
            card_id_by_name(&name).ok_or_else(|| format!("unregistered FDN printing {name}"))?;
        let card = &CARD_DEFS[usize::from(id)];
        if card.name != name || card.is_token || !card.is_executable() || !printings.insert(id) {
            return Err(format!("invalid frozen FDN printing {name}"));
        }
        if !partial.contains(&id) {
            if !card.has_full_support() {
                return Err(format!(
                    "frozen FDN printing {name} lacks an exact consumer"
                ));
            }
            legacy_full.push(id);
        }
    }
    if printings.len() != 286 || legacy_full.len() != 250 || !partial.is_subset(&printings) {
        return Err("frozen Foundations printing roster changed".into());
    }
    crate::rl::require_declared_cards_for_scope(
        crate::rl::CommonProjectionScopeV42::KaitoV75,
        &legacy_full,
    )?;
    Ok(printings)
}

/// The exact non-token printing set accepted by this declared-deck profile.
pub fn declared_printings_v1() -> Result<&'static BTreeSet<u16>, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("full-pool profile requires the pure Foundations fixture catalog".into());
    }
    static POOL: OnceLock<Result<BTreeSet<u16>, String>> = OnceLock::new();
    POOL.get_or_init(authenticate_frozen_pool)
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) fn preflight(config: &ResetV1) -> Result<(), String> {
    if config.expected_card_db_hash != KERNEL_CARDDB_HASH
        || config.max_physical_decisions == 0
        || config.max_policy_steps == 0
        || config.mainboards.iter().any(|deck| {
            !(40..=crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1).contains(&deck.len())
        })
    {
        return Err("invalid full-pool Foundations profile reset".into());
    }
    let printings = declared_printings_v1()?;
    if let Some(id) = config
        .mainboards
        .iter()
        .flatten()
        .find(|id| !printings.contains(*id))
    {
        return Err(format!("card {id} is outside the frozen Foundations pool"));
    }
    Ok(())
}

pub(crate) fn require_native_modes(state: &GameState) -> Result<(), String> {
    if state.curator_unordered_reveal_v1
        && state.greatwurm_commit_receipts_v1
        && state.harvester_commit_receipts_v1
        && state.fiend_artisan_receipts_v1
        && state.etali_commit_receipts_v1
        && state.thousand_year_storm_consumer_v1
    {
        Ok(())
    } else {
        Err("full-pool Foundations continuation lacks its native receipt modes".into())
    }
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_full_pool_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_full_pool_v1(config, true)
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
