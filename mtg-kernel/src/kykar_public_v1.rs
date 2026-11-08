//! Exact Kykar trigger modes, original targets and one-shot return schedules.
use crate::{
    card_def::{self, CardType, Subtype},
    effect::{self, kykar_v1 as native, EffectOp}, engine,
    event::CommittedEvent as E, ids::PlayerId,
    optional_trigger_public_v1 as public, rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, GameState, StackTargetContractV4, Target, Zone},
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = 474;
pub const NAME: &str = "Kykar, Zephyr Awakener";
pub const SCHEMA: &str = "mtg-kernel/kykar-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/kykar/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    let triggers = crate::trigger::triggers_for(PRINTING);
    let [trigger] = triggers else { return Err("Kykar trigger roster changed".into()); };
    if card_def::card_id_by_name(NAME) != Some(PRINTING) || !native::definition(PRINTING)
        || trigger.condition != crate::trigger::TriggerCondition::CastNoncreatureSpell
        || trigger.home_zone != Zone::Battlefield || trigger.face_index != 0
        || trigger.intervening_if_kicked || trigger.intervening_if_controls_another_source_card
        || (trigger.effect)() != native::marker()
        || crate::trigger::target_spec_for_trigger(PRINTING, &(trigger.effect)()) != Some(card_def::TargetSpec::None) {
        return Err("complete Kykar definition or trigger changed".into());
    }
    token_id()?;
    Ok(PRINTING)
}
pub(crate) fn token_id() -> Result<u16, String> {
    card_def::card_id_by_name("Kykar Spirit Token").filter(|id| native::token_definition(*id))
        .ok_or_else(|| "complete Kykar Spirit definition changed".into())
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [SCHEMA, MEANING_SCHEMA, include_str!("kykar_public_v1.rs"),
            include_str!("effect/kykar_v1.rs"), include_str!("trigger.rs"),
            include_str!("effect.rs"), include_str!("engine.rs"), include_str!("event.rs"),
            include_str!("../build.rs")] {
            hash.update((source.len() as u64).to_le_bytes()); hash.update(source.as_bytes());
        }
        format!("{:x}", hash.finalize())
    }).clone()
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String, pub producer_sha256: String, pub card_db_hash: u64, pub card_id: u16,
    pub mana_cost: crate::omniscience_public_v1::ManaCostV1, pub mana_value: u16,
    pub legendary: bool, pub card_type: CardType, pub subtypes: Vec<Subtype>,
    pub power: i16, pub toughness: i16, pub flying: bool,
    pub triggers_on_controllers_noncreature_cast: bool, pub choose_exactly_one_mode: bool,
    pub exile_mode_index: u8, pub exile_another_controlled_creature: bool,
    pub return_at_next_end_step_under_physical_owners_control: bool,
    pub return_requires_original_exile_incarnation_and_a_card: bool,
    pub spirit_mode_index: u8, pub spirit_card_id: u16, pub spirit_count: u8,
    pub spirit_power: i16, pub spirit_toughness: i16, pub spirit_white: bool, pub spirit_flying: bool,
}
pub fn printed_meaning() -> Result<MeaningV1, String> {
    let id = selected_id()?;
    Ok(MeaningV1 { schema: MEANING_SCHEMA.into(), producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH, card_id: id,
        mana_cost: card_def::CARD_DEFS[usize::from(id)].cost.into(), mana_value: 4,
        legendary: true, card_type: CardType::Creature, subtypes: vec![Subtype::Bird, Subtype::Wizard],
        power: 3, toughness: 4, flying: true, triggers_on_controllers_noncreature_cast: true,
        choose_exactly_one_mode: true, exile_mode_index: 0, exile_another_controlled_creature: true,
        return_at_next_end_step_under_physical_owners_control: true,
        return_requires_original_exile_incarnation_and_a_card: true,
        spirit_mode_index: 1, spirit_card_id: token_id()?, spirit_count: 1,
        spirit_power: 1, spirit_toughness: 1, spirit_white: true, spirit_flying: true })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV1 {
    pub card: public::ReferenceV1, pub form: public::FormV1, pub counters: CountersV72,
    pub is_token: bool, pub tapped: bool, pub summoning_sick: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModeV1 { Unchosen, Exile, Spirit, Return { schedule: u32 } }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV1 { Held, Ordered, Published }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV1 {
    pub original: IncarnationV72, pub current_original: Option<public::ReferenceV1>, pub currently_legal: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityV1 {
    pub source: public::RootV1, pub mode: ModeV1, pub stage: StageV1,
    pub position: u32, pub target: Option<TargetV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleV1 {
    pub source: public::RootV1, pub original_exile: IncarnationV72, pub physical_owner: PlayerSeatV1,
    pub current_original_exile: Option<public::ReferenceV1>, pub collected_at_first_end_step: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String, pub viewer: PlayerSeatV1, pub meaning: MeaningV1,
    pub battlefield: Vec<PermanentV1>, pub abilities: Vec<AbilityV1>, pub schedules: Vec<ScheduleV1>,
}
fn width(value: usize) -> Result<u32, String> { u32::try_from(value).map_err(|_| "Kykar public count exceeds u32".into()) }
fn mode(op: &EffectOp, schedules: &std::collections::BTreeMap<u64, u32>) -> Result<ModeV1, String> {
    Ok(match op {
        EffectOp::KykarChooseModeV1 => ModeV1::Unchosen,
        EffectOp::KykarExileV1 => ModeV1::Exile,
        EffectOp::KykarSpiritV1 => ModeV1::Spirit,
        EffectOp::KykarReturnV1 { schedule_history_index } => ModeV1::Return {
            schedule: *schedules.get(schedule_history_index).ok_or("Kykar return schedule absent")? },
        _ => return Err("Kykar original program changed".into()),
    })
}
#[allow(clippy::too_many_arguments)]
fn ability(state: &GameState, viewer: PlayerId, source: AbilitySourceContractV4,
    mode: ModeV1, stage: StageV1, position: usize, targets: &[Target], contracts: &[StackTargetContractV4]) -> Result<AbilityV1, String> {
    if targets.len() != contracts.len() || targets.len() > 1
        || mode != ModeV1::Exile && !targets.is_empty()
        || stage == StageV1::Published && mode == ModeV1::Exile && targets.len() != 1 {
        return Err("Kykar original target count changed".into());
    }
    let target = if let (Some(target), Some(contract)) = (targets.first(), contracts.first()) {
        let StackTargetContractV4::Object { object, owner, controller, zone, zone_change_count, .. } = *contract
            else { return Err("Kykar exile target is not a permanent".into()); };
        if *target != Target::Object(object) || object == source.source || zone != Zone::Battlefield
            || controller != source.controller || !crate::state::stack_target_contract_is_structurally_valid(
                state, card_def::TargetSpec::AnotherControlledCreature, 0, *target, *contract) {
            return Err("Kykar original target contract changed".into());
        }
        let current_original = state.objects.try_get(object).filter(|o| o.zone == zone && o.zone_change_count == zone_change_count)
            .and_then(|_| public::reference(state, object, Some(viewer)));
        Some(TargetV1 { original: IncarnationV72 { object: object.0, generation: zone_change_count, owner: owner.into(), controller: controller.into(), zone },
            currently_legal: current_original.is_some() && engine::effect_target_is_legal_from_ability_source(
                state, source, source.controller, card_def::TargetSpec::AnotherControlledCreature, targets, 0), current_original })
    } else { None };
    Ok(AbilityV1 { source: public::root(state, source, Some(viewer))?, mode, stage, position: width(position)?, target })
}
pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    effect::validate_pending_effect_choice(state)?;
    let mut battlefield = vec![];
    for (id, live) in state.objects.iter() {
        if live.zone == Zone::Battlefield && public::visible_known(state, id, Some(viewer), |id| id == PRINTING || native::token_definition(id)) {
            battlefield.push(PermanentV1 { card: public::reference(state, id, Some(viewer)).ok_or("Kykar public permanent absent")?,
                form: public::checked_form(state, id)?, counters: crate::tinybones_public_v72::counters(state, id)?,
                is_token: live.v4.is_token, tapped: live.tapped, summoning_sick: live.summoning_sick });
        }
    }
    let mut schedules = vec![];
    let mut positions = std::collections::BTreeMap::new();
    for (index, entry) in state.engine.event_history.scan_for_v1(&[
        crate::event_history_v1::EventKindV1::KykarExiledV1,
        crate::event_history_v1::EventKindV1::KykarDelayedCollectedV1]).iter().enumerate() {
        if !matches!(entry, E::KykarExiledV1 { .. }) { continue; }
        let index = u64::try_from(index).map_err(|_| "Kykar history width")?;
        let (source, exiled, owner) = native::validate_schedule(state, index)?;
        let collected = state.engine.event_history.iter().filter(|entry|
            matches!(entry, E::KykarDelayedCollectedV1 { schedule_history_index, .. } if *schedule_history_index == index)).count();
        if collected > 1 { return Err("Kykar delayed collection repeated".into()); }
        if collected == 1 { native::validate_root(state, &EffectOp::KykarReturnV1 { schedule_history_index: index }, Some(source))?; }
        positions.insert(index, width(schedules.len())?);
        let current_original_exile = state.objects.try_get(exiled.object)
            .filter(|live| live.zone == Zone::Exile && live.zone_change_count == exiled.zone_change_count)
            .and_then(|_| public::reference(state, exiled.object, Some(viewer)));
        schedules.push(ScheduleV1 { source: public::root(state, source, Some(viewer))?,
            original_exile: IncarnationV72 { object: exiled.object.0, generation: exiled.zone_change_count,
                owner: owner.into(), controller: owner.into(), zone: Zone::Exile },
            physical_owner: owner.into(), current_original_exile, collected_at_first_end_step: collected == 1 });
    }
    for entry in state.engine.event_history.scan_for_v1(&[crate::event_history_v1::EventKindV1::KykarDelayedCollectedV1]) {
        if let E::KykarDelayedCollectedV1 { schedule_history_index, .. } = entry {
            if !positions.contains_key(schedule_history_index) { return Err("Kykar collected an absent original schedule".into()); }
        }
    }
    let mut abilities = vec![];
    for (position, pending) in public::held(state).into_iter().enumerate() {
        let Some(source) = pending.source_contract.filter(|source| native::definition(source.card_def)) else { continue; };
        if !native::recipe(&pending.effect) { continue; } // Native ward has its own authenticated meaning.
        engine::validate_pending_trigger_identity(state, pending)?;
        if pending.placement_ordered { engine::validate_pending_trigger(state, pending)?; }
        native::validate_root(state, &pending.effect, Some(source))?;
        abilities.push(ability(state, viewer, source, mode(&pending.effect, &positions)?,
            if pending.placement_ordered { StageV1::Ordered } else { StageV1::Held }, position, &pending.targets, &pending.target_contracts)?);
    }
    for (position, item) in state.stack.iter().enumerate() {
        if !native::item_selected(state, item) { continue; }
        engine::validated_stack_item_target_spec(item, state)?;
        let op = item.inline_effect.as_ref().ok_or("Kykar published program absent")?;
        let source = item.v4.ability_source_contract.ok_or("Kykar original source absent")?;
        native::validate_root(state, op, Some(source))?;
        abilities.push(ability(state, viewer, source, mode(op, &positions)?, StageV1::Published, position, &item.targets, &item.v4.target_contracts)?);
    }
    // The native modes execute atomically. The public schema never invents a
    // completed return from a later same-valued card or serializes private history indices.
    Ok(FactsV1 { schema: SCHEMA.into(), viewer: viewer.into(), meaning, battlefield, abilities, schedules })
}
