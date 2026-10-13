//! Exact selected Fishing Pole roots; frozen vectors remain opaque.
use crate::card_def::{
    ActivationTargetFilter, CardType, CostComponent, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::Cost;
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/fishing-pole/v39";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV39 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV39 {
    PrintedArtifactSpell,
    PrintedEquip { ability_index: u16 },
    GrantedBaitActivation,
    EquippedCreatureUntaps,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV39 {
    PrintedArtifactSpell {
        printed_one_generic_colorless_equipment: bool,
        ordinary_cast_permissions_and_cost_modifiers: bool,
        ordinary_untargeted_battlefield_entry: bool,
    },
    PrintedEquip {
        printed_two_generic_sorcery_speed: bool,
        target_creature_you_control_at_announcement_and_resolution: bool,
        attach_exact_source_equipment_incarnation: bool,
        ordinary_protection_and_ability_suppression_apply: bool,
    },
    GrantedBaitActivation {
        one_generic_and_host_tap_symbol: bool,
        named_actual_attached_granting_pole_tap_cost: bool,
        host_sickness_haste_and_grant_suppression_apply: bool,
        named_pole_requires_controller_and_untapped_but_no_tap_symbol_sickness: bool,
        one_selected_equivalent_grant_family_distinguishes_multiple_poles: bool,
        not_a_target_or_any_other_artifact_tap: bool,
        captured_original_pole_receives_one_bait_at_resolution: bool,
        host_survival_attachment_and_later_control_changes_do_not_cancel: bool,
        departed_reentered_pole_does_not_receive_bait: bool,
        no_once_limit_or_arbitrary_count_cap: bool,
        actual_payment_and_removed_owner_bind_counter_change: bool,
    },
    EquippedCreatureUntaps {
        actual_tapped_to_untapped_transition_only: bool,
        natural_untap_and_effect_untap_both_trigger: bool,
        stun_skip_and_already_untapped_do_not_trigger: bool,
        pole_printed_ability_not_host_grant_is_observer: bool,
        original_attachment_incarnation_and_pole_controller_captured: bool,
        mandatory_untargeted_no_may_or_intervening_if: bool,
        remove_one_bait_from_original_pole_if_present_at_resolution: bool,
        successful_one_removal_creates_one_blue_one_one_fish_without_abilities: bool,
        zero_bait_or_departed_source_creates_no_fish: bool,
        multiple_bait_not_all_removed: bool,
        captured_controller_creates_and_owns_fish_after_control_change: bool,
        actual_counter_and_token_occurrences_bind_completion: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV39 {
    pub selector: SelectorV39,
    pub card_id: u16,
    pub root: RootV39,
    pub root_sha256: String,
    pub rule: RuleV39,
    pub model_sampler_and_catalog_admitted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV39 {
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
        include_str!("rules_meaning_fishing_pole_v39.rs"),
        include_str!("fishing_pole_v39.rs"),
        include_str!("fishing_pole_v39/lifecycle.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("trigger.rs"),
        include_str!("mana.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("garruks_uprising_v1.rs"),
        include_str!("arahbo_v1.rs"),
        include_str!("authority_of_the_consuls_v1.rs"),
        include_str!("raid_entry_v1.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector() -> Result<SelectorV39, ErrorV39> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV39::UnsupportedCatalog);
    }
    Ok(SelectorV39 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(selector: &SelectorV39, id: u16, root: RootV39) -> Result<MeaningV39, ErrorV39> {
    if *selector != live_selector()? {
        return Err(ErrorV39::WrongSelector);
    }
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(ErrorV39::UnsupportedDefinition)?;
    if !crate::fishing_pole_v39::selected_definition(id) || !d.has_full_support() {
        return Err(ErrorV39::UnsupportedDefinition);
    }
    let cost = Cost {
        generic: 1,
        pips: &[],
        x_count: 0,
    };
    let ordinary = Some(EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    });
    if d.cost != cost
        || d.object_name != d.name
        || d.mana_value != 1
        || d.types != [CardType::Artifact]
        || d.subtypes != [Subtype::Equipment]
        || !d.supertypes.is_empty()
        || !d.colors.is_empty()
        || (d.power, d.toughness) != (None, None)
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)() != ordinary
        || d.is_token
        || d.is_land
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || d.mana_ability_def.is_some()
        || !d.additional_mana_abilities.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.alt_cost.is_some()
        || d.additional_cost.is_some()
        || d.optional_additional_cost.is_some()
        || d.kicker_cost.is_some()
        || d.kicked_target_spec.is_some()
        || d.flashback.is_some()
        || d.escape.is_some()
        || d.madness_cost.is_some()
        || d.plot_cost.is_some()
        || d.bestow.is_some()
        || d.omen.is_some()
        || d.adventure.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.transform_face.is_some()
        || d.attachment.is_some()
        || d.animation.is_some()
        || d.saga.is_some()
        || d.changeling
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.enters_battlefield_tapped
        || d.enters_battlefield_tapped_unless.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || d.starting_loyalty.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.controlled_creature_grant.is_some()
        || d.ward_cost.is_some()
        || d.generic_cost_reduction.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.delve
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || d.conditional_tap_yield.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.minimum_blockers > 1
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
    {
        return Err(ErrorV39::UnsupportedDefinition);
    }
    let Some(equipment) = d.equipment else {
        return Err(ErrorV39::UnsupportedDefinition);
    };
    let expected = crate::fishing_pole_v39::profile();
    let (Some(grant), Some(expected_grant)) = (
        equipment.granted_activated_ability,
        expected.granted_activated_ability,
    ) else {
        return Err(ErrorV39::UnsupportedDefinition);
    };
    if equipment.power_delta != 0
        || equipment.toughness_delta != 0
        || equipment.pt_controller_turn_only
        || equipment.add_subtype.is_some()
        || equipment.controller_turn_keywords != Keywords::NONE
        || equipment.other_turn_keywords != Keywords::NONE
        || equipment.noncreature_spell_damage_to_each_opponent != 0
        || equipment.job_select
        || grant.cost != expected_grant.cost
        || grant.target_spec != TargetSpec::None
        || (grant.effect)() != EffectOp::FishingPoleAddBaitV39
    {
        return Err(ErrorV39::UnsupportedDefinition);
    }
    let [ability] = d.activated_abilities else {
        return Err(ErrorV39::UnsupportedDefinition);
    };
    let equip = EffectOp::AttachSourceToTarget;
    if ability.cost
        != [CostComponent::Mana(Cost {
            generic: 2,
            pips: &[],
            x_count: 0,
        })]
        || ability.target_spec != TargetSpec::ControlledCreature
        || (ability.effect)() != equip
        || !ability.sorcery_speed_only
        || ability.activation_zone != Zone::Battlefield
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.face.is_some()
        || ability.max_activations_per_turn.is_some()
    {
        return Err(ErrorV39::UnsupportedDefinition);
    }
    let [trigger] = crate::trigger::triggers_for(id) else {
        return Err(ErrorV39::UnsupportedDefinition);
    };
    if trigger.condition != crate::trigger::TriggerCondition::FishingPoleEquippedCreatureUntappedV39
        || trigger.home_zone != Zone::Battlefield
        || trigger.intervening_if_kicked
        || trigger.intervening_if_controls_another_source_card
        || trigger.face_index != 0
        || (trigger.effect)() != EffectOp::BindFishingPoleUntapV39
    {
        return Err(ErrorV39::UnsupportedDefinition);
    }
    let token = crate::fishing_pole_v39::lifecycle::fish_definition()
        .map_err(|_| ErrorV39::UnsupportedDefinition)?;
    let rule = match root {
        RootV39::PrintedArtifactSpell => RuleV39::PrintedArtifactSpell {
            printed_one_generic_colorless_equipment: true,
            ordinary_cast_permissions_and_cost_modifiers: true,
            ordinary_untargeted_battlefield_entry: true,
        },
        RootV39::PrintedEquip { ability_index: 0 } => RuleV39::PrintedEquip {
            printed_two_generic_sorcery_speed: true,
            target_creature_you_control_at_announcement_and_resolution: true,
            attach_exact_source_equipment_incarnation: true,
            ordinary_protection_and_ability_suppression_apply: true,
        },
        RootV39::GrantedBaitActivation => RuleV39::GrantedBaitActivation {
            one_generic_and_host_tap_symbol: true,
            named_actual_attached_granting_pole_tap_cost: true,
            host_sickness_haste_and_grant_suppression_apply: true,
            named_pole_requires_controller_and_untapped_but_no_tap_symbol_sickness: true,
            one_selected_equivalent_grant_family_distinguishes_multiple_poles: true,
            not_a_target_or_any_other_artifact_tap: true,
            captured_original_pole_receives_one_bait_at_resolution: true,
            host_survival_attachment_and_later_control_changes_do_not_cancel: true,
            departed_reentered_pole_does_not_receive_bait: true,
            no_once_limit_or_arbitrary_count_cap: true,
            actual_payment_and_removed_owner_bind_counter_change: true,
        },
        RootV39::EquippedCreatureUntaps => RuleV39::EquippedCreatureUntaps {
            actual_tapped_to_untapped_transition_only: true,
            natural_untap_and_effect_untap_both_trigger: true,
            stun_skip_and_already_untapped_do_not_trigger: true,
            pole_printed_ability_not_host_grant_is_observer: true,
            original_attachment_incarnation_and_pole_controller_captured: true,
            mandatory_untargeted_no_may_or_intervening_if: true,
            remove_one_bait_from_original_pole_if_present_at_resolution: true,
            successful_one_removal_creates_one_blue_one_one_fish_without_abilities: true,
            zero_bait_or_departed_source_creates_no_fish: true,
            multiple_bait_not_all_removed: true,
            captured_controller_creates_and_owns_fish_after_control_change: true,
            actual_counter_and_token_occurrences_bind_completion: true,
        },
        _ => return Err(ErrorV39::WrongRoot),
    };
    let facts = serde_json::json!({
        "card_id":id,"root":root,"cost":d.cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,"colors":d.colors,
        "keywords":d.keywords,"spell_program":(d.spell_effect)(),"equip_cost":ability.cost,"equip_target":ability.target_spec,"equip_program":(ability.effect)(),
        "grant_cost":grant.cost,"grant_target":grant.target_spec,"grant_program":(grant.effect)(),"untap_condition":trigger.condition,"untap_program":(trigger.effect)(),
        "fish_token":token,"rule":rule,
    });
    let root_sha256 =
        Sha256::digest(serde_json::to_vec(&facts).expect("typed Fishing root serializes"))
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
    Ok(MeaningV39 {
        selector: selector.clone(),
        card_id: id,
        root,
        root_sha256,
        rule,
        model_sampler_and_catalog_admitted: false,
    })
}
pub fn validate(value: &MeaningV39) -> Result<(), ErrorV39> {
    if *value != meaning(&value.selector, value.card_id, value.root)? {
        return Err(ErrorV39::IdentityMismatch);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn predecessor_and_registered_non_pole_do_not_fabricate_meaning() {
        if cfg!(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )) {
            let selector = live_selector().unwrap();
            assert_eq!(
                meaning(
                    &selector,
                    crate::card_def::card_id_by_name("Viridian Longbow").unwrap(),
                    RootV39::GrantedBaitActivation
                ),
                Err(ErrorV39::UnsupportedDefinition)
            );
        } else {
            assert_eq!(live_selector(), Err(ErrorV39::UnsupportedCatalog));
        }
    }
    #[test]
    #[ignore = "requires actual future Fishing Pole and Fish Token registration"]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_four_roots_reconstruct_complete_ordered_facts_and_refuse_every_forgery() {
        let id = crate::card_def::card_id_by_name("Fishing Pole").unwrap();
        let selector = live_selector().unwrap();
        for root in [
            RootV39::PrintedArtifactSpell,
            RootV39::PrintedEquip { ability_index: 0 },
            RootV39::GrantedBaitActivation,
            RootV39::EquippedCreatureUntaps,
        ] {
            let actual = meaning(&selector, id, root).unwrap();
            validate(&actual).unwrap();
            let mut forged = actual.clone();
            forged.root_sha256.clear();
            assert_eq!(validate(&forged), Err(ErrorV39::IdentityMismatch));
            let mut forged = actual.clone();
            forged.selector.producer_sha256.clear();
            assert_eq!(validate(&forged), Err(ErrorV39::WrongSelector));
            let mut forged = actual.clone();
            forged.model_sampler_and_catalog_admitted = true;
            assert_eq!(validate(&forged), Err(ErrorV39::IdentityMismatch));
            let value = serde_json::to_value(&actual).unwrap();
            let (variant, fields) = value["rule"].as_object().unwrap().iter().next().unwrap();
            for field in fields.as_object().unwrap().keys() {
                let mut forged = value.clone();
                forged["rule"][variant][field] = serde_json::Value::Bool(false);
                assert_eq!(
                    validate(&serde_json::from_value(forged).unwrap()),
                    Err(ErrorV39::IdentityMismatch),
                    "{field}"
                );
            }
        }
        assert_eq!(
            meaning(&selector, id, RootV39::PrintedEquip { ability_index: 1 }),
            Err(ErrorV39::WrongRoot)
        );
    }
}
