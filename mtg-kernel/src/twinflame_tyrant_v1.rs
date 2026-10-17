//! Definition-owned live damage replacement. No saved state on absent paths.
use crate::card_def::{DamageReplacementDefV1, CARD_DEFS};
use crate::event::{DamageProposed, ProposedEvent, ReplacementEffectKind};
use crate::event_history_v1::EventKindV1;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Target, Zone};

/// Complete printing/root predicate; no name-only executor classification.
pub fn owns_definition(id: u16) -> bool {
    use crate::card_def::{CardCapability, CardType, Keywords, Subtype, TargetSpec};
    use crate::effect::{EffectOp, ObjectRef};
    use crate::mana::{ManaColor, Pip};
    let Some(def) = CARD_DEFS.get(usize::from(id)) else {
        return false;
    };
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && def.name == "Twinflame Tyrant"
        && def.capability == CardCapability::Full
        && def.cost.generic == 3
        && def.cost.x_count == 0
        && def.cost.pips == [Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)]
        && def.types == [CardType::Creature]
        && def.subtypes == [Subtype::Dragon]
        && def.supertypes.is_empty()
        && def.colors == [ManaColor::R]
        && def.mana_value == 5
        && !def.is_land
        && !def.is_token
        && def.alt_cost.is_none()
        && def.kicker_cost.is_none()
        && def.additional_cost.is_none()
        && def.flashback.is_none()
        && def.ward_cost.is_none()
        && def.power == Some(3)
        && def.toughness == Some(5)
        && def.keywords == Keywords::FLYING
        && def.target_spec == TargetSpec::None
        && def.damage_replacement_v1
            == Some(DamageReplacementDefV1::DoubleControlledSourceToOpponent)
        && (def.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        && def.activated_abilities.is_empty()
        && crate::trigger::triggers_for(id).is_empty()
}

pub(crate) fn controller(state: &GameState, id: ObjectId) -> Option<PlayerId> {
    let object = state.objects.try_get(id)?;
    let def = CARD_DEFS.get(usize::from(object.card_def))?;
    if !owns_definition(object.card_def) {
        return None;
    }
    (object.zone == Zone::Battlefield
        && object.v4.face_index == 0
        && def.damage_replacement_v1
            == Some(DamageReplacementDefV1::DoubleControlledSourceToOpponent)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, id))
    .then_some(object.controller)
}

pub(crate) fn has_active_source(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(id, _)| controller(state, id).is_some())
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    has_active_source(state)
        || state.stack.iter().any(|item| {
            item.v4.source_contract.is_some_and(|source| {
                CARD_DEFS
                    .get(usize::from(source.card_def))
                    .is_some_and(|def| def.damage_replacement_v1.is_some())
            }) || state.objects.try_get(item.source).is_some_and(|object| {
                CARD_DEFS
                    .get(usize::from(object.card_def))
                    .is_some_and(|def| def.damage_replacement_v1.is_some())
            })
        })
        || state
            .engine
            .event_history
            .scan_for_v1(&[
                EventKindV1::TwinflameCombatInputV38,
                EventKindV1::TwinflameCombatOperationV38,
                EventKindV1::TwinflameCombatPublishedV38,
                EventKindV1::TwinflameDamagePreparationV38,
                EventKindV1::TwinflameDynamicDamageSampleV38,
                EventKindV1::TwinflameFieryCaptureV38,
                EventKindV1::TwinflameFieryFinishedV38,
                EventKindV1::TwinflameReplacementInputV38,
                EventKindV1::TwinflameReplacementOperationV38,
            ])
            .iter()
            .any(|e| {
                matches!(
                    e,
                    crate::event::CommittedEvent::TwinflameCombatPublishedV38 { .. }
                        | crate::event::CommittedEvent::TwinflameCombatInputV38 { .. }
                        | crate::event::CommittedEvent::TwinflameCombatOperationV38 { .. }
                        | crate::event::CommittedEvent::TwinflameReplacementInputV38 { .. }
                        | crate::event::CommittedEvent::TwinflameReplacementOperationV38 { .. }
                        | crate::event::CommittedEvent::TwinflameDamagePreparationV38 { .. }
                        | crate::event::CommittedEvent::TwinflameDynamicDamageSampleV38 { .. }
                        | crate::event::CommittedEvent::TwinflameFieryCaptureV38 { .. }
                        | crate::event::CommittedEvent::TwinflameFieryFinishedV38 { .. }
                )
            })
}

pub(crate) fn sources(state: &GameState, damage: &DamageProposed) -> Result<Vec<ObjectId>, String> {
    if damage.amount <= 0 {
        return Ok(Vec::new());
    }
    let source_controller = match damage.historical_source {
        Some(id) => {
            crate::source_power_activation_v1::damage_source(state, damage.source, id)?
                .source
                .controller
        }
        None => {
            let Some(source) = state.objects.try_get(damage.source) else {
                return Ok(Vec::new());
            };
            source.controller
        }
    };
    let damaged_controller = match damage.target {
        Target::Player(player) => player,
        Target::Object(id) => {
            let Some(target) = state
                .objects
                .try_get(id)
                .filter(|o| o.zone == Zone::Battlefield)
            else {
                return Ok(Vec::new());
            };
            target.controller
        }
        Target::StackItem(_) => return Ok(Vec::new()),
    };
    if damaged_controller != source_controller.opponent() {
        return Ok(Vec::new());
    }
    Ok(state
        .objects
        .iter()
        .filter_map(|(id, _)| (controller(state, id) == Some(source_controller)).then_some(id))
        .collect())
}

/// Temporary checkpoint: refuse noncommuting prevention rather than guessing
/// the affected player's order. A later owned ordering adapter completes this.
pub(crate) fn validate_commuting_damage(
    state: &GameState,
    damage: &DamageProposed,
) -> Result<Vec<ObjectId>, String> {
    let sources = sources(state, damage)?;
    if sources.is_empty() {
        return Ok(sources);
    }
    let cannot_prevent = state.engine.until_end_of_turn.iter().any(|e| {
        matches!(
            e,
            crate::engine::UntilEndOfTurnEffect::DamageCannotBePrevented { .. }
        )
    });
    if !cannot_prevent
        && state.engine.active_replacements.iter().any(|r| {
            matches!(r.kind, ReplacementEffectKind::PreventNextDamage { .. })
                && !damage.touched_by.contains(&r.id)
                && crate::event::replacement_applies(
                    r,
                    &ProposedEvent::Damage(damage.clone()),
                    state,
                )
        })
    {
        return Err(
            "Twinflame multiplier requires affected-controller replacement ordering".into(),
        );
    }
    let mut amount = damage.amount;
    for _ in &sources {
        amount = amount
            .checked_mul(2)
            .ok_or("Twinflame doubled damage exceeds the current i32 damage representation")?;
    }
    Ok(sources)
}

pub(crate) fn replace(state: &GameState, damage: &mut DamageProposed) -> Result<(), String> {
    let mut amount = damage.amount;
    for _ in validate_commuting_damage(state, damage)? {
        amount = amount
            .checked_mul(2)
            .ok_or("Twinflame doubled damage exceeds the current i32 damage representation")?;
    }
    damage.amount = amount;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedMeaningV1 {
    pub schema: String,
    pub card_id: u16,
    pub card_db_hash: u64,
    pub producer_sha256: String,
    pub printing_root_sha256: String,
    pub current_source_controller: bool,
    pub damage_source_captured_controller_when_historical: bool,
    pub opponent_player_or_opponent_controlled_permanent: bool,
    pub combat_and_noncombat: bool,
    pub each_live_source_once_per_event: bool,
    pub affected_controller_orders_noncommuting_effects: bool,
}
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
/// Exact printed rule description; this is not executor/model admission.
pub fn printed_meaning(id: u16) -> Result<PrintedMeaningV1, String> {
    if !owns_definition(id) {
        return Err("wrong Twinflame printing/root".into());
    }
    let def = &CARD_DEFS[usize::from(id)];
    let root = serde_json::to_vec(&(
        def.name,
        def.cost,
        def.types,
        def.subtypes,
        def.colors,
        def.power,
        def.toughness,
        def.keywords,
        def.target_spec,
        (def.spell_effect)(),
        def.damage_replacement_v1,
    ))
    .map_err(|e| e.to_string())?;
    let producer = [
        include_str!("twinflame_tyrant_v1.rs"),
        include_str!("twinflame_tyrant_v1/ordering.rs"),
        include_str!("twinflame_tyrant_v1/sampling.rs"),
        include_str!("twinflame_tyrant_v1/combat.rs"),
        include_str!("twinflame_tyrant_v1/fiery.rs"),
        include_str!("fiery_annihilation_v1.rs"),
        include_str!("rules_meaning_fiery_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("event.rs"),
        include_str!("source_power_activation_v1.rs"),
        include_str!("gorehorn_damage_v1.rs"),
        include_str!("equipment_damage_source_v4.rs"),
        include_str!("fixed_source_damage_v3.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("../build.rs"),
        include_str!("bloodthirsty_conqueror_v49/custody.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("counter_target_effects_v1.rs"),
        include_str!("zimone_paradox_sculptor_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("planeswalker_v1.rs"),
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
        include_str!("rl.rs"),
        include_str!("standard_cast_convoke.rs"),
        include_str!("engine/mandatory_cast_cost_v1.rs"),
        include_str!("rules_meaning_next_five_v59.rs"),
        include_str!("effect/thousand_year_storm_v1.rs"),
        include_str!("effect/standard_copy.rs"),
        include_str!("standard_spell_copy.rs"),
        include_str!("event_history_v1.rs"),
        crate::engine::time_stop_v1::source_sha256_v1(),
        crate::effect::genesis_wave_v1::source_sha256_v1(),
    ]
    .join("\0");
    Ok(PrintedMeaningV1 {
        schema: "mtg-kernel/twinflame-printed-meaning/v1".into(),
        card_id: id,
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: digest(producer.as_bytes()),
        printing_root_sha256: digest(&root),
        current_source_controller: true,
        damage_source_captured_controller_when_historical: true,
        opponent_player_or_opponent_controlled_permanent: true,
        combat_and_noncombat: true,
        each_live_source_once_per_event: true,
        affected_controller_orders_noncommuting_effects: true,
    })
}
impl PrintedMeaningV1 {
    pub fn validate(&self) -> Result<(), String> {
        if *self == printed_meaning(self.card_id)? {
            Ok(())
        } else {
            Err("changed Twinflame printed facts".into())
        }
    }
}

pub mod combat;
pub mod fiery;
pub mod ordering;
pub mod sampling;

#[cfg(test)]
mod tests;
