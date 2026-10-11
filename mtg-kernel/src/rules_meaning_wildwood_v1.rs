//! Selected printed meaning only. No historical event projection or model admission.
use crate::card_def::{CardCapability, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::EffectOp;
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use crate::trigger::TriggerCondition;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/wildwood-scourge/v1";
pub const SCOPE: &str = "pauper+fdn/limited-without-standard";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub scope: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    WrongMeaning,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryV1 {
    pub spell_root_sha256: String,
    pub printed_cost: String,
    /// Current finalized-stack representation, not an unbounded printed-X claim.
    pub represented_x_minimum: u16,
    pub represented_x_maximum: u16,
    pub x_origin: String,
    pub counter_kind: String,
    pub counter_count_equals_captured_x: bool,
    pub counters_committed_during_entry_before_sba: bool,
    pub entry_is_a_replacement_not_a_trigger: bool,
    pub zero_base_toughness_uses_ordinary_sba: bool,
    pub untargeted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterTriggerV1 {
    pub ability_index: u16,
    pub trigger_root_sha256: String,
    pub event: String,
    pub minimum_committed_count: u8,
    pub recipient_zone: Zone,
    pub exact_recipient_event_incarnation: bool,
    pub recipient_effective_type: CardType,
    pub excluded_effective_subtype: Subtype,
    pub recipient_is_another_object: bool,
    pub recipient_controller_at_event_equals_source_controller: bool,
    pub counter_placing_player_is_irrelevant: bool,
    pub triggers_per_qualifying_recipient_event: u8,
    pub capture_time: String,
    pub source_home_zone: Zone,
    pub source_face_index: u8,
    pub active_printed_ability_required_at_event: bool,
    pub capture_original_source_controller_and_incarnation: bool,
    pub recipient_condition_rechecked_at_resolution: bool,
    pub source_current_controller_rechecked_at_resolution: bool,
    pub source_ability_loss_cancels_captured_trigger: bool,
    pub source_effect: String,
    pub source_counter_count: u8,
    pub source_reentry_receives_old_counter: bool,
    pub untargeted: bool,
    pub optional: bool,
    pub placement: String,
    pub later_batch_collection_duplicates_capture: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub entry: EntryV1,
    pub counter_trigger: CounterTriggerV1,
    /// A static rule descriptor cannot authenticate old event-time characteristics.
    pub historical_event_capture_transport_supplied: bool,
    pub complete_apnap_capture_merge_supplied: bool,
    pub unbounded_x_successor_supplied: bool,
    pub frozen_vector_model_admission_supplied: bool,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_wildwood_v1.rs"),
        include_str!("wildwood_scourge_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("trigger.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn live_selector_v1() -> Result<SelectorV1, ErrorV1> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV1::UnsupportedCatalog);
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        scope: SCOPE.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    })
}
/// Select both actual printed roots by the catalog definition. A caller effect,
/// atom ordinal, name-only row or prepared recipe cannot supply authority.
pub fn meaning_v1(selector: &SelectorV1, card_id: u16) -> Result<MeaningV1, ErrorV1> {
    if selector != &live_selector_v1()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Wildwood Scourge" || def.capability != CardCapability::Full {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let expected_cost = Cost {
        generic: 0,
        pips: &[Pip::Colored(ManaColor::G)],
        x_count: 1,
    };
    let spell_program = (def.spell_effect)();
    let abilities = crate::trigger::triggers_for(card_id);
    let [ability] = abilities else {
        return Err(ErrorV1::WrongRoot);
    };
    let trigger_program = (ability.effect)();
    if def.cost != expected_cost
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Hydra]
        || !def.supertypes.is_empty()
        || def.colors != [ManaColor::G]
        || def.power != Some(0)
        || def.toughness != Some(0)
        || def.keywords != Keywords::NONE
        || def.is_land
        || def.is_token
        || def.target_spec != TargetSpec::None
        || def.alt_cost.is_some()
        || def.kicker_cost.is_some()
        || def.enters_with_plus_one_counters.is_some()
        || !def.activated_abilities.is_empty()
        || (def.mana_ability)().is_some()
        || spell_program != Some(EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters)
        || ability.condition
            != TriggerCondition::AnotherControlledNonHydraCreatureGetsPlusOneCounters
        || ability.home_zone != Zone::Battlefield
        || ability.face_index != 0
        || ability.intervening_if_kicked
        || ability.intervening_if_controls_another_source_card
        || trigger_program != EffectOp::BindPlusOnePlusOneCounterToTriggerSource
        || crate::trigger::target_spec_for_trigger(card_id, &trigger_program)
            != Some(TargetSpec::None)
    {
        return Err(ErrorV1::WrongRoot);
    }
    let spell_root = serde_json::to_vec(&(
        card_id,
        def.cost,
        def.types,
        def.subtypes,
        def.colors,
        def.power,
        def.toughness,
        def.keywords.0,
        def.target_spec,
        spell_program,
    ))
    .expect("selected printed entry root serializes");
    let trigger_root = serde_json::to_vec(&(
        card_id,
        0u16,
        format!("{:?}", ability.condition),
        ability.home_zone,
        ability.face_index,
        ability.intervening_if_kicked,
        ability.intervening_if_controls_another_source_card,
        trigger_program,
    ))
    .expect("selected printed counter root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        entry: EntryV1 {
            spell_root_sha256: digest(&spell_root),
            printed_cost: "{X}{G}".into(),
            represented_x_minimum: 0,
            represented_x_maximum: u16::from(u8::MAX),
            x_origin: "finalized_spell_context_v4_x_value_not_current_mana".into(),
            counter_kind: "plus_one_plus_one".into(),
            counter_count_equals_captured_x: true,
            counters_committed_during_entry_before_sba: true,
            entry_is_a_replacement_not_a_trigger: true,
            zero_base_toughness_uses_ordinary_sba: true,
            untargeted: true,
        },
        counter_trigger: CounterTriggerV1 {
            ability_index: 0,
            trigger_root_sha256: digest(&trigger_root),
            event: "committed_positive_PlusOneCountersAdded".into(),
            minimum_committed_count: 1,
            recipient_zone: Zone::Battlefield,
            exact_recipient_event_incarnation: true,
            recipient_effective_type: CardType::Creature,
            excluded_effective_subtype: Subtype::Hydra,
            recipient_is_another_object: true,
            recipient_controller_at_event_equals_source_controller: true,
            counter_placing_player_is_irrelevant: true,
            triggers_per_qualifying_recipient_event: 1,
            capture_time:
                "immediately_after_positive_counter_commit_before_next_instruction_or_sba".into(),
            source_home_zone: Zone::Battlefield,
            source_face_index: 0,
            active_printed_ability_required_at_event: true,
            capture_original_source_controller_and_incarnation: true,
            recipient_condition_rechecked_at_resolution: false,
            source_current_controller_rechecked_at_resolution: false,
            source_ability_loss_cancels_captured_trigger: false,
            source_effect:
                "one_plus_one_counter_on_bound_BF_owner_ZCC_source_if_same_incarnation_remains"
                    .into(),
            source_counter_count: 1,
            source_reentry_receives_old_counter: false,
            untargeted: true,
            optional: false,
            placement: "ordinary_checkpoint_APNAP_then_controller_orders_simultaneous_triggers"
                .into(),
            later_batch_collection_duplicates_capture: false,
        },
        historical_event_capture_transport_supplied: false,
        complete_apnap_capture_merge_supplied: true,
        unbounded_x_successor_supplied: false,
        frozen_vector_model_admission_supplied: false,
    })
}
/// Complete reconstruction, including every nested semantic and scope field.
/// This read-only validation authenticates a meaning artifact, not a game action.
pub fn validate_meaning_v1(meaning: &MeaningV1) -> Result<(), ErrorV1> {
    let expected = meaning_v1(&meaning.selector, meaning.card_id)?;
    if *meaning != expected {
        return Err(ErrorV1::WrongMeaning);
    }
    Ok(())
}
#[cfg(test)]
#[path = "rules_meaning_wildwood_v1_tests.rs"]
mod tests;
