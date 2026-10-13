//! Exact selected printed roots, separate from frozen vector/model admission.
use crate::card_def::{
    ActivationTargetFilter, CardType, CostComponent, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/mild-mannered-librarian/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RootV1 {
    PrintedCreatureSpell,
    PrintedActivation { ability_index: u16 },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV1 {
    PrintedCreatureSpell {
        printed_green_cost_one: bool,
        ordinary_cast_permissions_and_modifiers_apply: bool,
        untargeted_ordinary_battlefield_entry: bool,
        green_human_one_one_no_keywords: bool,
    },
    PrintedActivation {
        printed_generic_three_green_one_cost: bool,
        instant_speed_without_tap_or_sacrifice: bool,
        no_targets_and_no_resolution_payment: bool,
        activate_once_on_original_permanent_not_per_turn_or_player: bool,
        full_payment_consumes_once_before_priority: bool,
        unaffordable_request_does_not_consume_once: bool,
        control_changes_cleanup_and_ability_suppression_keep_once: bool,
        real_zone_change_creates_fresh_allowance: bool,
        replace_creature_types_with_werewolf_preserving_other_subtypes: bool,
        permanent_type_effect_uses_actual_layer_timestamp: bool,
        put_two_plus_one_counters_then_independent_draw: bool,
        exact_original_source_incarnation_gets_types_and_counters: bool,
        current_controller_or_definition_change_does_not_cancel_paid_effect: bool,
        departed_or_reentered_source_gets_neither_upgrade_nor_counters: bool,
        draw_uses_original_captured_ability_controller: bool,
        empty_library_is_actual_draw_attempt: bool,
        countered_ability_keeps_once_and_has_no_effects: bool,
        actual_resolve_or_counter_operation_is_required_for_completion: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub public_human_sampler_and_model_admitted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    IdentityMismatch,
}
pub fn producer_sha256() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_librarian_v1.rs"),
        include_str!("mild_mannered_librarian_v1.rs"),
        include_str!("librarian_activation_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("engine.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("trigger.rs"),
        include_str!("card_def.rs"),
        include_str!("mana.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector() -> Result<SelectorV1, ErrorV1> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV1::UnsupportedCatalog);
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(selector: &SelectorV1, card_id: u16, root: RootV1) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Mild-Mannered Librarian" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let cast_cost = Cost {
        generic: 0,
        pips: &[Pip::Colored(ManaColor::G)],
        x_count: 0,
    };
    let activation_cost = Cost {
        generic: 3,
        pips: &[Pip::Colored(ManaColor::G)],
        x_count: 0,
    };
    let expected = crate::mild_mannered_librarian_v1::program_v1();
    let [ability] = def.activated_abilities else {
        return Err(ErrorV1::WrongRoot);
    };
    if def.object_name != def.name
        || def.cost != cast_cost
        || def.mana_value != 1
        || def.colors != [ManaColor::G]
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Human]
        || !def.supertypes.is_empty()
        || (def.power, def.toughness) != (Some(1), Some(1))
        || def.keywords != Keywords::NONE
        || def.target_spec != TargetSpec::None
        || (def.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || ability.cost != [CostComponent::Mana(activation_cost)]
        || ability.target_spec != TargetSpec::None
        || (ability.effect)() != expected
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || !crate::trigger::triggers_for(card_id).is_empty()
        || def.generic_cost_reduction.is_some()
        || def.ward_cost.is_some()
        || def.spell_cannot_be_countered
        || def.records_mana_spent
        || def.equipment.is_some()
        || def.starting_loyalty.is_some()
        || def.enters_with_plus_one_counters.is_some()
        || def.controlled_counter_keyword.is_some()
        || def.is_land
        || !def.produces_mana.is_empty()
        || (def.mana_ability)().is_some()
        || def.alt_cost.is_some()
        || def.kicker_cost.is_some()
        || def.additional_cost.is_some()
        || def.flashback.is_some()
        || def.plot_cost.is_some()
        || def.madness_cost.is_some()
        || def.mode2.is_some()
        || def.mode3.is_some()
        || !def.additional_modes.is_empty()
        || def.kicked_target_spec.is_some()
        || def.is_token
        || def.escape.is_some()
        || !def.mana_ability_choices.is_empty()
        || def.enters_battlefield_tapped
        || def.mana_ability_def.is_some()
        || def.minimum_blockers != 1
        || def.omen.is_some()
        || def.mana_ability_includes_chosen_color
        || def.as_enters_choose_color_other_than.is_some()
        || !def.additional_mana_abilities.is_empty()
        || def.enters_battlefield_tapped_unless.is_some()
        || def.attachment.is_some()
        || def.transform_face.is_some()
        || def.saga.is_some()
        || def.optional_additional_cost.is_some()
        || def.changeling
        || def.bestow.is_some()
        || def.delve
        || def.adventure.is_some()
        || def.cant_be_blocked_by_monarchs_creatures
        || def.conditional_tap_yield.is_some()
        || def.enters_tapped_unless_controller.is_some()
        || !def.additional_mana_ability_conditions.is_empty()
        || !def.restricted_mana_abilities.is_empty()
        || def.animation.is_some()
        || !def.activated_ability_generic_reductions.is_empty()
    {
        return Err(ErrorV1::WrongRoot);
    }
    let rule = match root {
        RootV1::PrintedCreatureSpell => RuleV1::PrintedCreatureSpell {
            printed_green_cost_one: true,
            ordinary_cast_permissions_and_modifiers_apply: true,
            untargeted_ordinary_battlefield_entry: true,
            green_human_one_one_no_keywords: true,
        },
        RootV1::PrintedActivation { ability_index: 0 } => RuleV1::PrintedActivation {
            printed_generic_three_green_one_cost: true,
            instant_speed_without_tap_or_sacrifice: true,
            no_targets_and_no_resolution_payment: true,
            activate_once_on_original_permanent_not_per_turn_or_player: true,
            full_payment_consumes_once_before_priority: true,
            unaffordable_request_does_not_consume_once: true,
            control_changes_cleanup_and_ability_suppression_keep_once: true,
            real_zone_change_creates_fresh_allowance: true,
            replace_creature_types_with_werewolf_preserving_other_subtypes: true,
            permanent_type_effect_uses_actual_layer_timestamp: true,
            put_two_plus_one_counters_then_independent_draw: true,
            exact_original_source_incarnation_gets_types_and_counters: true,
            current_controller_or_definition_change_does_not_cancel_paid_effect: true,
            departed_or_reentered_source_gets_neither_upgrade_nor_counters: true,
            draw_uses_original_captured_ability_controller: true,
            empty_library_is_actual_draw_attempt: true,
            countered_ability_keeps_once_and_has_no_effects: true,
            actual_resolve_or_counter_operation_is_required_for_completion: true,
        },
        _ => return Err(ErrorV1::WrongRoot),
    };
    let bytes = serde_json::to_vec(&(card_id, root, cast_cost, activation_cost, &expected, &rule))
        .expect("selected root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        root,
        root_sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        rule,
        public_human_sampler_and_model_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id, record.root)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selectors_and_other_registered_definitions_refuse_without_fabricated_roots() {
        let Ok(selector) = live_selector() else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        for field in 0..3 {
            let mut wrong = selector.clone();
            match field {
                0 => wrong.schema.push('x'),
                1 => wrong.card_db_hash ^= 1,
                _ => wrong.producer_sha256.push('x'),
            }
            assert_eq!(
                meaning(&wrong, u16::MAX, RootV1::PrintedCreatureSpell),
                Err(ErrorV1::WrongSelector)
            );
        }
        assert_eq!(
            meaning(
                &selector,
                crate::card_def::card_id_by_name("Llanowar Elves").unwrap(),
                RootV1::PrintedCreatureSpell
            ),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn genuine_future_selected_roots_and_every_rule_field_are_reconstructed() {
        let selector = live_selector().unwrap();
        let card = crate::card_def::card_id_by_name("Mild-Mannered Librarian")
            .expect("genuine future registration");
        for root in [
            RootV1::PrintedCreatureSpell,
            RootV1::PrintedActivation { ability_index: 0 },
        ] {
            let record = meaning(&selector, card, root).unwrap();
            validate(&record).unwrap();
            let value = serde_json::to_value(&record).unwrap();
            let rule = value["rule"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .as_object()
                .unwrap();
            for field in rule.keys() {
                let mut forged = value.clone();
                forged["rule"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                    .next()
                    .unwrap()[field] = serde_json::Value::Bool(false);
                let forged: MeaningV1 = serde_json::from_value(forged).unwrap();
                assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch));
            }
            let mut wrong = record.clone();
            wrong.public_human_sampler_and_model_admitted = true;
            assert_eq!(validate(&wrong), Err(ErrorV1::IdentityMismatch));
        }
        assert_eq!(
            meaning(
                &selector,
                card,
                RootV1::PrintedActivation { ability_index: 1 }
            ),
            Err(ErrorV1::WrongRoot)
        );
    }
}
