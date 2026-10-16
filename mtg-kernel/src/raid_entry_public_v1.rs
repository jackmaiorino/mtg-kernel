//! Explicit public companion, retaining genuine predecessor actor-private projection.
use crate::ids::PlayerId;
use crate::policy_observation_v9::ObservationV9;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::RlContractError;
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/raid-entry-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeepOneContextV1 {
    pub looked_count: u8,
    pub graveyard_min: u16,
    pub graveyard_max: u16,
    pub retained_max: u8,
    pub graveyard_ordered: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RaidEntryObservationV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub base: ObservationV9,
    pub keep_one: Option<KeepOneContextV1>,
    pub gorehorn_sources: Vec<crate::gorehorn_damage_v1::GorehornSourceV1>,
    pub roots: Vec<crate::rules_meaning_raid_entry_v1::MeaningV1>,
    pub projection_sha256: String,
}
pub fn producer_sha256_v1() -> String {
    let mut h = Sha256::new();
    for s in [
        SCHEMA,
        include_str!("raid_entry_public_v1.rs"),
        include_str!("raid_entry_v1.rs"),
        include_str!("raid_entry_custody_v1.rs"),
        include_str!("gorehorn_damage_v1.rs"),
        include_str!("gorehorn_crystal_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
        include_str!("rules_meaning_raid_entry_v1.rs"),
        include_str!("effect/private_keep_one_v1.rs"),
        include_str!("effect/surveil_many.rs"),
        include_str!("effect/squad_rallier_resolution_v1.rs"),
        include_str!("engine.rs"),
        include_str!("effect.rs"),
        include_str!("trigger.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("planeswalker_v1.rs"),
        include_str!("policy_observation_v9.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("rl_session.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("rl_session/human_v8.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/raid_entry_v21.rs"),
        include_str!("human_bo3_v1/raid_entry_v21.rs"),
        include_str!("bloodthirsty_conqueror_v49/custody.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("counter_target_effects_v1.rs"),
        include_str!("zimone_paradox_sculptor_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("engine/omniscience_cast_v1.rs"),
        include_str!("resolution_cast_v1.rs"),
        include_str!("effect/harvester_v1.rs"),
        include_str!("leyline_axe_v1.rs"),
        include_str!("london_mulligan_v1.rs"),
        include_str!("life_loss_turn_v1.rs"),
        include_str!("soul_shackled_zombie_v1.rs"),
        include_str!("progenitus_v1.rs"),
        include_str!("controlled_creature_grants_v1.rs"),
        include_str!("effect/nonland_copy_v1.rs"),
        include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
        include_str!("effect/inspiration_resolution_v1.rs"),
        include_str!("effect/standard_exile.rs"),
        include_str!("snack_wide_life_v59.rs"),
        include_str!("snack_wide_life_v59/custody.rs"),
        include_str!("bloodthirsty_conqueror_v49.rs"),
        include_str!("fiendish_panda_v1.rs"),
        include_str!("fiendish_panda_v1/custody.rs"),
        include_str!("life_gain_turn_v1.rs"),
        include_str!("midnight_snack_v1.rs"),
        include_str!("mana.rs"),
        include_str!("legend_rule_v1.rs"),
        include_str!("doubling_season_v48.rs"),
        include_str!("standard_cast_convoke.rs"),
        include_str!("engine/mandatory_cast_cost_v1.rs"),
        include_str!("rules_meaning_next_five_v59.rs"),
        include_str!("effect/thousand_year_storm_v1.rs"),
        include_str!("effect/standard_copy.rs"),
        include_str!("standard_spell_copy.rs"),
        include_str!("event_history_v1.rs"),
    ] {
        h.update([0]);
        h.update(s.as_bytes());
    }
    h.update(b"legacy-graveyard-return-guard/v61\0");
    h.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn roots() -> Result<Vec<crate::rules_meaning_raid_entry_v1::MeaningV1>, String> {
    let mut roots = Vec::new();
    if let Ok(selector) = crate::rules_meaning_raid_entry_v1::selector_v1() {
        for name in ["Gorehorn Raider", "Gutless Plunderer"] {
            if let Some(id) = crate::card_def::card_id_by_name(name) {
                roots.push(crate::rules_meaning_raid_entry_v1::meaning_v1(
                    &selector, id, 0,
                )?);
            }
        }
    }
    Ok(roots)
}
fn commitment(row: &RaidEntryObservationV1) -> Result<String, String> {
    let mut data = serde_json::to_value(row).map_err(|e| e.to_string())?;
    data["projection_sha256"] = "".into();
    Ok(
        Sha256::digest(serde_json::to_vec(&data).map_err(|e| e.to_string())?)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}
impl RaidEntryObservationV1 {
    pub fn validate_artifact(&self) -> Result<(), String> {
        if self.schema != SCHEMA
            || self.producer_sha256 != producer_sha256_v1()
            || self.base.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.roots != roots()?
            || self.projection_sha256 != commitment(self)?
        {
            return Err("Raid entry public artifact identity mismatch".into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: u32,
    count: u32,
) -> Result<RaidEntryObservationV1, RlContractError> {
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    let base = crate::policy_observation_v9::observe_policy_v9(
        state, surface, actor, step, physical, substep, count,
    )?;
    let keep_one = if state
        .engine
        .pending_effect
        .as_ref()
        .is_some_and(|pending| pending.ctx.controller == actor)
    {
        crate::effect::selection_shape(state)
            .map_err(RlContractError)?
            .map(|(minimum, maximum)| KeepOneContextV1 {
                looked_count: maximum as u8,
                graveyard_min: minimum,
                graveyard_max: maximum,
                retained_max: 1,
                graveyard_ordered: true,
            })
    } else {
        None
    };
    let mut row = RaidEntryObservationV1 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256_v1(),
        base,
        keep_one,
        gorehorn_sources: crate::gorehorn_damage_v1::public_sources(state)
            .map_err(RlContractError)?,
        roots: roots().map_err(RlContractError)?,
        projection_sha256: String::new(),
    };
    row.projection_sha256 = commitment(&row).map_err(RlContractError)?;
    Ok(row)
}
