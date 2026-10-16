//! Standalone exact ordered Midnight Snack meaning. No executable public payload.
use crate::{
    card_def::{
        ActivationTargetFilter, CardCapability, CardDef, CardType, CostComponent, Keywords,
        Subtype, TargetSpec, CARD_DEFS,
    },
    effect::{EffectOp, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "rules-meaning/midnight-snack/v58";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV58 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManaV58 {
    pub generic: u8,
    pub colored_pips: Vec<ManaColor>,
    pub x_multiplicity: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV58 {
    pub mana: ManaV58,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub colors: Vec<ManaColor>,
    pub legendary: bool,
    pub power: Option<i16>,
    pub toughness: Option<i16>,
    pub keyword_bits: u32,
    pub is_token: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostV58 {
    PayNativeMana,
    TapOriginalSource,
    SacrificeOriginalSource,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimingV58 {
    CasterMainPhaseEmptyStackPriorityFromHand,
    PriorityWithNativeCostRestrictions,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetV58 {
    None,
    ExactlyOriginalControllerOpponent,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionV58 {
    MoveOriginalSpellFromStackToBattlefieldUnderCasterControl,
    TargetLegalityBeforeReadingAmount,
    SumActualPositiveLifeGainEventsForOriginalActivationControllerInActualResolutionTurn,
    LifeLossesAndOpponentsGainsDoNotSubtractOrAdd,
    ResponsesBeforeResolutionCanChangeTotal,
    OriginalTargetLosesExactlyThatFullWidthAmount,
    GainThreeLifeForOriginalController,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationV58 {
    pub zone: Zone,
    pub timing: TimingV58,
    pub targets: TargetV58,
    pub mana: ManaV58,
    pub ordered_costs: Vec<CostV58>,
    pub ordered_resolution: Vec<ResolutionV58>,
    pub native_candidate_and_payment_required: bool,
    pub source_must_be_controlled_by_actor: bool,
    pub complete_costs_before_stack_publication: bool,
    pub countering_does_not_refund_paid_costs: bool,
    pub source_departure_or_new_incarnation_does_not_change_original_controller: bool,
    pub ordinary_zone_replacements_apply: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaidCheckV58 {
    AtCollection,
    AtResolutionBeforeTokenCreation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV58 {
    OrdinaryEnchantmentSpell {
        operation: OperationV58,
        countered_physical_spell_goes_to_owners_graveyard: bool,
    },
    Raid {
        printed_index: u8,
        home: Zone,
        original_controller_own_end_step: bool,
        successful_nonempty_attack_declaration_in_current_turn: bool,
        checks: Vec<RaidCheckV58>,
        captured_source_incarnation_and_controller: bool,
        source_survival_not_required_after_collection: bool,
        mandatory_food_count: u16,
        food_controller_is_original_trigger_controller: bool,
        token_replacements_apply_once: bool,
    },
    PrintedActivation {
        printed_index: u8,
        operation: OperationV58,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodV58 {
    pub card_id: u16,
    pub printed: PrintedV58,
    pub activation: OperationV58,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV58 {
    pub selector: SelectorV58,
    pub card_id: u16,
    pub capability: CardCapability,
    pub printed: PrintedV58,
    pub ordered_rules: Vec<RuleV58>,
    pub food: FoodV58,
    pub root_sha256: String,
    pub model_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for s in [
            SCHEMA,
            include_str!("rules_meaning_midnight_snack_v58.rs"),
            include_str!("midnight_snack_public_v58.rs"),
            include_str!("midnight_snack_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("card_def.rs"),
            include_str!("engine.rs"),
            include_str!("effect.rs"),
            include_str!("event.rs"),
            include_str!("state.rs"),
            include_str!("trigger.rs"),
            include_str!("../build.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
            include_str!("soul_shackled_zombie_v1.rs"),
            include_str!("progenitus_v1.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("effect/inspiration_resolution_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("snack_wide_life_v59.rs"),
            include_str!("snack_wide_life_v59/custody.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("mana.rs"),
            include_str!("legend_rule_v1.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("rl.rs"),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub fn live_selector() -> Result<SelectorV58, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Snack V58 requires pure Limited catalog".into());
    }
    Ok(SelectorV58 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
fn closed_fields(d: &CardDef) {
    let CardDef {
        name: _,
        capability: _,
        cost: _,
        generic_cost_reduction: _,
        ward_cost: _,
        spell_cannot_be_countered: _,
        records_mana_spent: _,
        equipment: _,
        types: _,
        subtypes: _,
        supertypes: _,
        power: _,
        toughness: _,
        starting_loyalty: _,
        enters_with_plus_one_counters: _,
        controlled_counter_keyword: _,
        game_outcome_protection_v1: _,
        controlled_creature_grant: _,
        is_land: _,
        produces_mana: _,
        colors: _,
        target_spec: _,
        keywords: _,
        spell_effect: _,
        mana_ability: _,
        alt_cost: _,
        kicker_cost: _,
        additional_cost: _,
        flashback: _,
        activated_abilities: _,
        plot_cost: _,
        madness_cost: _,
        mode2: _,
        mode3: _,
        additional_modes: _,
        kicked_target_spec: _,
        is_token: _,
        escape: _,
        mana_ability_choices: _,
        enters_battlefield_tapped: _,
        mana_ability_def: _,
        minimum_blockers: _,
        omen: _,
        mana_value: _,
        mana_ability_includes_chosen_color: _,
        as_enters_choose_color_other_than: _,
        additional_mana_abilities: _,
        object_name: _,
        enters_battlefield_tapped_unless: _,
        attachment: _,
        transform_face: _,
        saga: _,
        optional_additional_cost: _,
        changeling: _,
        bestow: _,
        delve: _,
        adventure: _,
        cant_be_blocked_by_monarchs_creatures: _,
        conditional_tap_yield: _,
        enters_tapped_unless_controller: _,
        additional_mana_ability_conditions: _,
        restricted_mana_abilities: _,
        animation: _,
        activated_ability_generic_reductions: _,
        combat_static_rules: _,
        as_enters_color_choice_v1: _,
        chosen_color_creature_boost_v1: _,
        damage_replacement_v1: _,
        token_counter_replacement_v48: _,
        starting_life_self_bonuses_v1: _,
        source_type_hexproof_v1: _,
        other_controlled_nontoken_death_v1: _,
        creature_type_entry_v1: _,
    } = d;
}
pub(crate) fn selected_definition(id: u16, d: &CardDef) -> bool {
    closed_fields(d);
    crate::midnight_snack_v1::exact_definition(d)
        && crate::midnight_snack_v1::definition(id)
        && CARD_DEFS
            .get(usize::from(id))
            .is_some_and(|actual| actual.name == d.name)
}
fn food_activation(d: &CardDef) -> bool {
    let [a] = d.activated_abilities else {
        return false;
    };
    a.cost
        == [
            CostComponent::Mana(Cost {
                generic: 2,
                pips: &[],
                x_count: 0,
            }),
            CostComponent::Tap,
            CostComponent::SacrificeSelf,
        ]
        && a.activation_zone == Zone::Battlefield
        && !a.sorcery_speed_only
        && a.target_spec == TargetSpec::None
        && a.activation_target_filter == ActivationTargetFilter::TargetSpecOnly
        && a.face.is_none()
        && a.max_activations_per_turn.is_none()
        && (a.effect)()
            == EffectOp::GainLife {
                player: PlayerRef::Controller,
                amount: 3,
            }
}
fn food_definition(d: &CardDef) -> bool {
    closed_fields(d);
    d.name == "Food Token"
        && d.capability == CardCapability::Full
        && d.types == [CardType::Artifact]
        && d.subtypes == [Subtype::Food]
        && d.supertypes.is_empty()
        && d.cost.generic == 0
        && d.cost.x_count == 0
        && d.cost.pips.is_empty()
        && d.colors.is_empty()
        && d.mana_value == 0
        && d.power.is_none()
        && d.toughness.is_none()
        && !d.is_land
        && d.is_token
        && d.keywords == Keywords::NONE
        && d.target_spec == TargetSpec::None
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.flashback.is_none()
        && d.ward_cost.is_none()
        && d.starting_loyalty.is_none()
        && d.object_name == "Food Token"
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.generic_cost_reduction.is_none()
        && d.equipment.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.controlled_creature_grant.is_none()
        && (d.mana_ability)().is_none()
        && d.produces_mana.is_empty()
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.escape.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.kicked_target_spec.is_none()
        && d.mana_ability_choices.is_empty()
        && !d.enters_battlefield_tapped
        && d.enters_battlefield_tapped_unless.is_none()
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && d.bestow.is_none()
        && d.adventure.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.additional_mana_ability_conditions.is_empty()
        && d.attachment.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && d.optional_additional_cost.is_none()
        && !d.changeling
        && !d.delve
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.restricted_mana_abilities.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.creature_type_entry_v1.is_none()
        && d.other_controlled_nontoken_death_v1.is_none()
        && food_activation(d)
        && d.token_counter_replacement_v48.is_none()
        && (d.spell_effect)().is_none()
        && crate::trigger::triggers_for(crate::card_def::card_id_by_name("Food Token").unwrap())
            .is_empty()
}
fn mana(cost: Cost) -> ManaV58 {
    ManaV58 {
        generic: cost.generic,
        colored_pips: cost
            .pips
            .iter()
            .map(|p| match p {
                Pip::Colored(c) => *c,
                _ => unreachable!("exact supported mana recipe checked"),
            })
            .collect(),
        x_multiplicity: cost.x_count,
    }
}
fn printed(d: &CardDef) -> PrintedV58 {
    PrintedV58 {
        mana: mana(d.cost),
        mana_value: d.mana_value,
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        colors: d.colors.to_vec(),
        legendary: false,
        power: d.power,
        toughness: d.toughness,
        keyword_bits: d.keywords.0,
        is_token: d.is_token,
    }
}
pub fn meaning(selector: &SelectorV58, id: u16) -> Result<MeaningV58, String> {
    if *selector != live_selector()? {
        return Err("Snack selector changed".into());
    }
    let d = CARD_DEFS
        .get(usize::from(id))
        .filter(|d| selected_definition(id, d))
        .ok_or("Snack exact printing changed")?;
    let food_id = crate::card_def::card_id_by_name("Food Token").ok_or("Food absent")?;
    let food = CARD_DEFS
        .get(usize::from(food_id))
        .filter(|d| food_definition(d))
        .ok_or("Food exact dependency changed")?;
    let operation = |zone, timing, targets, mana, ordered_costs, ordered_resolution| OperationV58 {
        zone,
        timing,
        targets,
        mana,
        ordered_costs,
        ordered_resolution,
        native_candidate_and_payment_required: true,
        source_must_be_controlled_by_actor: true,
        complete_costs_before_stack_publication: true,
        countering_does_not_refund_paid_costs: true,
        source_departure_or_new_incarnation_does_not_change_original_controller: true,
        ordinary_zone_replacements_apply: true,
    };
    let [ability] = d.activated_abilities else {
        unreachable!()
    };
    let CostComponent::Mana(activation_mana) = ability.cost[0] else {
        unreachable!()
    };
    let ordered_rules=vec![
        RuleV58::OrdinaryEnchantmentSpell {operation:operation(Zone::Hand,TimingV58::CasterMainPhaseEmptyStackPriorityFromHand,TargetV58::None,mana(d.cost),vec![CostV58::PayNativeMana],vec![ResolutionV58::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl]),countered_physical_spell_goes_to_owners_graveyard:true},
        RuleV58::Raid { printed_index:0,home:Zone::Battlefield,original_controller_own_end_step:true,successful_nonempty_attack_declaration_in_current_turn:true,checks:vec![RaidCheckV58::AtCollection,RaidCheckV58::AtResolutionBeforeTokenCreation],captured_source_incarnation_and_controller:true,source_survival_not_required_after_collection:true,mandatory_food_count:1,food_controller_is_original_trigger_controller:true,token_replacements_apply_once:true },
        RuleV58::PrintedActivation {printed_index:0,operation:operation(Zone::Battlefield,TimingV58::PriorityWithNativeCostRestrictions,TargetV58::ExactlyOriginalControllerOpponent,mana(activation_mana),vec![CostV58::PayNativeMana,CostV58::SacrificeOriginalSource],vec![ResolutionV58::TargetLegalityBeforeReadingAmount,ResolutionV58::SumActualPositiveLifeGainEventsForOriginalActivationControllerInActualResolutionTurn,ResolutionV58::LifeLossesAndOpponentsGainsDoNotSubtractOrAdd,ResolutionV58::ResponsesBeforeResolutionCanChangeTotal,ResolutionV58::OriginalTargetLosesExactlyThatFullWidthAmount])},
    ];
    let food = FoodV58 {
        card_id: food_id,
        printed: printed(food),
        activation: operation(
            Zone::Battlefield,
            TimingV58::PriorityWithNativeCostRestrictions,
            TargetV58::None,
            ManaV58 {
                generic: 2,
                colored_pips: vec![],
                x_multiplicity: 0,
            },
            vec![
                CostV58::PayNativeMana,
                CostV58::TapOriginalSource,
                CostV58::SacrificeOriginalSource,
            ],
            vec![ResolutionV58::GainThreeLifeForOriginalController],
        ),
    };
    let printed = printed(d);
    let bytes = serde_json::to_vec(&(selector, id, d.capability, &printed, &ordered_rules, &food))
        .map_err(|e| e.to_string())?;
    let root_sha256 = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(MeaningV58 {
        selector: selector.clone(),
        card_id: id,
        capability: d.capability,
        printed,
        ordered_rules,
        food,
        root_sha256,
        model_admitted: false,
    })
}
pub fn printed_meaning(id: u16) -> Result<MeaningV58, String> {
    meaning(&live_selector()?, id)
}
pub fn validate(value: &MeaningV58) -> Result<(), String> {
    if *value == meaning(&value.selector, value.card_id)? {
        Ok(())
    } else {
        Err("Snack ordered meaning changed".into())
    }
}

#[cfg(test)]
mod selector_tests {
    use super::*;
    #[test]
    fn default_standard_and_foreign_catalog_operands_refuse() {
        match live_selector() {
            Err(_) => assert!(!cfg!(all(
                feature = "limited-fdn-fixtures",
                not(feature = "standard-magezero-fixtures")
            ))),
            Ok(selector) => {
                let forest = crate::card_def::card_id_by_name("Forest").unwrap();
                assert!(meaning(&selector, forest).is_err());
                let mut wrong = selector;
                wrong.card_db_hash ^= 1;
                assert!(meaning(&wrong, forest).is_err());
            }
        }
    }
}
