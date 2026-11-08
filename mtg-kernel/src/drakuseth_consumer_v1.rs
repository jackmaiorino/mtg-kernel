//! Declared-deck owning consumer for the exact Drakuseth, Maw of Flames printing.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    card_def::{CARD_DEFS, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    ids::PlayerId,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{ConsumerV1, IdentityV1},
    rl::PlayerSeatV1,
    rl_session::FastActorSessionV1,
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-drakuseth-consumer/v1";
pub const PRINTING_V1: u16 = 464;
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/drakuseth-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/drakuseth-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/drakuseth-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("drakuseth_consumer_v1.rs"),
            include_str!("drakuseth_consumer_v1/damage.rs"),
            include_str!("human_bo3_v1/drakuseth_consumer_v1.rs"),
            include_str!("fdn_family_source_v76.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_drakuseth_env_v1.rs"),
        ] {
            hash.update([0]);
            hash.update(source.as_bytes());
        }
        hash.update(crate::rules_meaning_drakuseth_v67::producer_sha256());
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
        return Err("Drakuseth profile requires the Foundations catalog".into());
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
        return Err("invalid Drakuseth profile reset".into());
    }
    if crate::rules_meaning_drakuseth_v67::printed_meaning()?.card_id != PRINTING_V1 {
        return Err("Drakuseth profile printing identity changed".into());
    }
    for deck in &config.mainboards {
        for &id in deck {
            let def = CARD_DEFS
                .get(usize::from(id))
                .ok_or("unknown declared card")?;
            if def.is_token
                || !(def.has_full_support() || (id == PRINTING_V1 && def.is_executable()))
            {
                return Err(format!(
                    "card {id} is outside the Drakuseth consumer profile"
                ));
            }
            if id != PRINTING_V1 {
                crate::rl::require_declared_cards_for_scope(
                    crate::rl::CommonProjectionScopeV42::KaitoV75,
                    std::slice::from_ref(&id),
                )?;
            }
        }
    }
    Ok(())
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_drakuseth_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

/// The public name/count reducer is installed before the first typed menu.
pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_drakuseth_v1(config, true)
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

mod damage;
pub use damage::{DamageChoiceV1, DamageOutcomeV1, DamageRowV1};
pub(crate) fn owns_damage_menu(
    state: &GameState,
    surface: &crate::policy_surface_v5::PolicySurfaceV5,
) -> bool {
    surface
        .limited_appendix_v1()
        .includes(crate::policy_surface_v5::LimitedAppendixV1::Drakuseth)
        && state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| crate::drakuseth_damage_source_v76::selected(&p.resolving_item))
        && crate::effect::crystal_prevention_v1::is_choice(state)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub meaning: crate::rules_meaning_drakuseth_v67::MeaningV67,
    pub source: crate::fdn_family_source_v76::FactsV76,
    pub pending_damage: Option<DamageChoiceV1>,
}

pub(crate) fn capture_scoped(
    state: &GameState,
    actor: PlayerId,
    consumer: ConsumerV1,
) -> Result<FactsV1, String> {
    let mut source = crate::fdn_family_source_v76::capture_scoped(state, actor, consumer)?;
    for crate::fdn_family_source_v76::LaterFamilyV76::Drakuseth(family) in &mut source.families {
        // Public name/count knowledge is not a physical opponent-hand handle.
        family
            .authorized_printings
            .retain(|r| r.zone != Zone::Hand || r.owner == source.viewer);
    }
    let pending_damage = damage::capture(state, actor, &source)?;
    Ok(FactsV1 {
        meaning: crate::rules_meaning_drakuseth_v67::printed_meaning()?,
        source,
        pending_damage,
    })
}
pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    crate::rules_meaning_drakuseth_v67::validate(&facts.meaning)?;
    let s = &facts.source;
    if s.viewer != actor
        || s.schema != crate::fdn_family_source_v76::SCHEMA
        || s.producer_sha256 != crate::fdn_family_source_v76::producer_sha256()
        || s.complete_ordinary_observation
        || s.owning_consumer_admitted
        || s.families.len() != 1
    {
        return Err("invalid Drakuseth consumer source".into());
    }
    let crate::fdn_family_source_v76::LaterFamilyV76::Drakuseth(f) = &s.families[0];
    if f.card_id != PRINTING_V1
        || f.catalog_capability != CARD_DEFS[usize::from(PRINTING_V1)].capability
        || f.authorized_printings.iter().any(|r| {
            r.identity
                != (IdentityV1::Known {
                    card_id: PRINTING_V1,
                })
                || (r.zone == Zone::Hand && r.owner != actor)
        })
    {
        return Err("invalid Drakuseth authorized printing".into());
    }
    if let Some(menu) = &facts.pending_damage {
        let matching = f
            .triggers
            .iter()
            .filter(|r| {
                r.stage == crate::fdn_family_source_v76::StageV76::Resolving
                    && r.publication == Some(menu.publication)
            })
            .collect::<Vec<_>>();
        let [trigger] = matching.as_slice() else {
            return Err("damage menu lost original resolving publication".into());
        };
        let n = trigger.original_target_slots.len();
        if menu.rows.is_empty()
            || menu.rows.len() > n
            || menu.active_original_slot as usize >= n
            || menu.remaining_choice_slots.first() != Some(&menu.active_original_slot)
            || menu
                .rows
                .windows(2)
                .any(|r| r[0].original_slot >= r[1].original_slot)
            || menu
                .rows
                .iter()
                .any(|r| r.original_slot as usize >= n || r.proposed_amount < 0)
            || menu
                .remaining_choice_slots
                .iter()
                .any(|&slot| slot as usize >= n)
            || menu.options.is_empty()
            || menu
                .options
                .iter()
                .enumerate()
                .any(|(i, o)| usize::from(o.menu_index) != i)
        {
            return Err("invalid Drakuseth damage continuation shape".into());
        }
    }
    // Full current recapture remains the action authority. This artifact-only
    // check does not authenticate an untrusted native world or restore token.
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TargetActionV1 {
    Choose {
        original_slot: u32,
        printed_damage: i32,
    },
    Finish {
        chosen_slots: u32,
    },
}
pub(crate) fn target_action(
    facts: Option<&FactsV1>,
    action: &crate::rl::ActionSemanticV1,
) -> Result<Option<TargetActionV1>, String> {
    use crate::rl::ActionSemanticV1 as A;
    let Some(facts) = facts else { return Ok(None) };
    let (source, finish) = match action {
        A::ChooseTarget { source, .. } if source.card_db_id == PRINTING_V1 => (source, false),
        A::FinishTargetSelection { source, .. } if source.card_db_id == PRINTING_V1 => {
            (source, true)
        }
        _ => return Ok(None),
    };
    let crate::fdn_family_source_v76::LaterFamilyV76::Drakuseth(f) = &facts.source.families[0];
    let rows = f
        .triggers
        .iter()
        .filter(|t| {
            t.stage == crate::fdn_family_source_v76::StageV76::ChoosingTargets
                && t.source.original_source.object == source.arena_id
                && t.source.original_source.generation == source.zone_change_count
                && t.source.original_source.owner == source.owner
                && t.source.captured_controller == source.controller
        })
        .collect::<Vec<_>>();
    let [row] = rows.as_slice() else {
        return Err("Drakuseth target answer lacks one genuine placement".into());
    };
    let slot =
        u32::try_from(row.original_target_slots.len()).map_err(|_| "target slot overflow")?;
    if slot >= 3 || finish && slot == 0 {
        return Err("Drakuseth target cardinality changed".into());
    }
    if let A::FinishTargetSelection { selected_count, .. } = action {
        if u32::from(*selected_count) != slot {
            return Err("Drakuseth target prefix changed".into());
        }
    }
    Ok(Some(if finish {
        TargetActionV1::Finish { chosen_slots: slot }
    } else {
        TargetActionV1::Choose {
            original_slot: slot,
            printed_damage: if slot == 0 { 4 } else { 3 },
        }
    }))
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
