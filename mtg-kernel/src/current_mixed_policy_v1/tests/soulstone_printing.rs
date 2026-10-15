#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
use serde_json::json;

#[test]
fn exact_soulstone_land_play_mana_and_persistent_animation_recipe() {
    let id = crate::card_def::card_id_by_name("Soulstone Sanctuary").unwrap();
    let d = &CARD_DEFS[usize::from(id)];
    validate_shape(id, d).unwrap();

    let baseline = baseline(id, d).unwrap();
    assert_eq!(baseline["primitive_printing"]["types"], json!(["Land"]));
    assert_eq!(
        baseline["primitive_printing"]["ordered_instructions"],
        json!([{"operation":"PlayLand"}])
    );
    assert_eq!(
        baseline["primitive_printing"]["mana_ability"],
        json!({"cost":"TapSelf","recipient":"Controller","colors":["C"],"amount":1})
    );

    let descriptor = printing(id, d).unwrap().unwrap();
    let printed = &descriptor["registered_permanent_printing"];
    assert_eq!(printed["characteristics"]["types"], json!(["Land"]));
    assert_eq!(printed["characteristics"]["colors"], json!([]));
    assert_eq!(printed["characteristics"]["printed_mana_abilities"], 1);
    assert_eq!(printed["characteristics"]["printed_activated_abilities"], 1);
    assert_eq!(printed["ordinary_land_play"]["origin"], json!("Hand"));
    assert_eq!(
        printed["ordinary_land_play"]["is_a_spell_or_stack_object"],
        false
    );
    assert_eq!(
        printed["ordinary_land_play"]["land_playing_player_becomes_controller"],
        true
    );
    assert_eq!(printed["printed_mana_abilities"][0]["colors"], json!(["C"]));
    assert_eq!(
        printed["printed_activated_abilities"][0]["ordered_costs"][0]["PrintedMana"],
        json!({"generic":4,"pips":[],"x_count":0})
    );
    assert_eq!(
        printed["printed_activated_abilities"][0]["requires_source_untapped"],
        false
    );
    assert_eq!(printed["animation"]["base_power"], 3);
    assert_eq!(printed["animation"]["base_toughness"], 3);
    assert_eq!(printed["animation"]["artifact"], false);
    assert_eq!(printed["animation"]["adds_creature_type"], true);
    assert_eq!(printed["animation"]["vigilance"], true);
    assert_eq!(printed["animation"]["preserves_printed_land_type"], true);
    assert_eq!(
        printed["animation"]["duration"],
        json!("WhileOriginalSourceRemainsOnBattlefield")
    );
    assert_eq!(
        printed["animation"]["creature_subtypes"],
        serde_json::to_value(crate::card_def::Subtype::CREATURE_TYPES).unwrap()
    );

    assert!(crate::current_mixed_policy_v1::roots::baseline(id).is_ok());
    assert_eq!(
        crate::current_mixed_policy_v1::roots::registered_permanent_printing(id).unwrap(),
        Some(descriptor)
    );
    let other = crate::card_def::card_id_by_name("Forest").unwrap();
    assert!(matches!(
        printing(other, &CARD_DEFS[usize::from(other)]),
        Err(Error::UnsupportedPrintedRoot(actual)) if actual == other
    ));
}
