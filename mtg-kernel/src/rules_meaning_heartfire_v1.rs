//! Scoped definition-owned Heartfire target/cost/damage meaning companion.
//! No frozen RulesVector vocabulary, weights or profile is changed here.
use crate::card_def::{self, ActivationTargetFilter, CostComponent, TargetSpec};
use crate::effect::EffectOp;
use crate::mana::{ManaColor, Pip};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/heartfire/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartfireRootMeaningV1 {
    pub schema: String,
    pub catalog_hash: u64,
    pub source_sha256: String,
    pub card_id: u16,
    pub ability_index: u32,
    pub target_count: u8,
    pub battlefield_creature_or_planeswalker: bool,
    pub target_any_controller: bool,
    pub no_player_target: bool,
    pub exact_target_incarnation_rechecked: bool,
    pub red_mana_cost: u8,
    pub sacrifice_source_as_cost: bool,
    pub signed_power_at_source_departure_after_mana_payment: bool,
    pub damage_is_nonnegative_saved_power: bool,
    pub damage_source_exact_sacrificed_incarnation: bool,
    pub damage_controller_color_lifelink_deathtouch_werewolf_captured_at_departure: bool,
    pub source_reentry_or_current_characteristic_change_cannot_change_damage: bool,
    pub damage_uses_replacement_and_protection_pipeline: bool,
    pub illegal_target_deals_no_damage: bool,
    pub requires_definition_owned_departure_capture: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningErrorV1 {
    UnsupportedRoot,
    IdentityMismatch,
}
pub fn source_sha256() -> String {
    let mut hash = Sha256::new();
    for (path, source) in [
        (
            "rules_meaning_heartfire_v1.rs",
            include_str!("rules_meaning_heartfire_v1.rs"),
        ),
        (
            "source_power_activation_v1.rs",
            include_str!("source_power_activation_v1.rs"),
        ),
        (
            "creature_stats_lki_v1.rs",
            include_str!("creature_stats_lki_v1.rs"),
        ),
        ("effect.rs", include_str!("effect.rs")),
        ("event.rs", include_str!("event.rs")),
        ("engine.rs", include_str!("engine.rs")),
        ("standard_cards_v1.rs", include_str!("standard_cards_v1.rs")),
        (
            "standard_statics_v1.rs",
            include_str!("standard_statics_v1.rs"),
        ),
        (
            "standard_legends_v1.rs",
            include_str!("standard_legends_v1.rs"),
        ),
        ("build.rs", include_str!("../build.rs")),
        ("card_def.rs", include_str!("card_def.rs")),
    ] {
        hash.update(path.as_bytes());
        hash.update([0]);
        hash.update(source.as_bytes());
        hash.update([0]);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn root_meaning(
    card_id: u16,
    ability_index: u32,
) -> Result<HeartfireRootMeaningV1, MeaningErrorV1> {
    let def = card_def::CARD_DEFS
        .get(card_id as usize)
        .ok_or(MeaningErrorV1::UnsupportedRoot)?;
    let ability = def
        .activated_abilities
        .get(ability_index as usize)
        .ok_or(MeaningErrorV1::UnsupportedRoot)?;
    if !def.is_executable()
        || def.name != "Heartfire Immolator"
        || ability.target_spec != TargetSpec::CreatureOrPlaneswalker
        || ability.activation_zone != Zone::Battlefield
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.sorcery_speed_only
        || ability.max_activations_per_turn.is_some()
        || (ability.effect)() != EffectOp::StandardV1(StandardOpV1::SourceLastPowerDamageV1)
        || !matches!(ability.cost, [CostComponent::Mana(cost), CostComponent::SacrificeSelf]
            if cost.generic == 0 && cost.x_count == 0 && cost.pips == [Pip::Colored(ManaColor::R)])
    {
        return Err(MeaningErrorV1::UnsupportedRoot);
    }
    Ok(HeartfireRootMeaningV1 {
        schema: SCHEMA.into(),
        catalog_hash: card_def::KERNEL_CARDDB_HASH,
        source_sha256: source_sha256(),
        card_id,
        ability_index,
        target_count: 1,
        battlefield_creature_or_planeswalker: true,
        target_any_controller: true,
        no_player_target: true,
        exact_target_incarnation_rechecked: true,
        red_mana_cost: 1,
        sacrifice_source_as_cost: true,
        signed_power_at_source_departure_after_mana_payment: true,
        damage_is_nonnegative_saved_power: true,
        damage_source_exact_sacrificed_incarnation: true,
        damage_controller_color_lifelink_deathtouch_werewolf_captured_at_departure: true,
        source_reentry_or_current_characteristic_change_cannot_change_damage: true,
        damage_uses_replacement_and_protection_pipeline: true,
        illegal_target_deals_no_damage: true,
        requires_definition_owned_departure_capture: true,
    })
}
pub fn validate(record: &HeartfireRootMeaningV1) -> Result<(), MeaningErrorV1> {
    if *record != root_meaning(record.card_id, record.ability_index)? {
        return Err(MeaningErrorV1::IdentityMismatch);
    }
    Ok(())
}
pub fn catalog_roots() -> Vec<HeartfireRootMeaningV1> {
    card_def::CARD_DEFS
        .iter()
        .enumerate()
        .flat_map(|(id, def)| {
            def.activated_abilities
                .iter()
                .enumerate()
                .filter_map(move |(index, _)| root_meaning(id as u16, index as u32).ok())
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_registered_definition_cannot_supply_heartfire_root() {
        let elves = card_def::card_id_by_name("Llanowar Elves").unwrap();
        assert_eq!(root_meaning(elves, 0), Err(MeaningErrorV1::UnsupportedRoot));
        assert_eq!(
            root_meaning(u16::MAX, 0),
            Err(MeaningErrorV1::UnsupportedRoot)
        );
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn future_actual_heartfire_meaning_refuses_identity_target_cost_and_lki_forgery() {
        let id = card_def::card_id_by_name("Heartfire Immolator")
            .expect("true serial registration prerequisite");
        let record = root_meaning(id, 0).unwrap();
        let decoded: HeartfireRootMeaningV1 =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        validate(&decoded).unwrap();
        for field in 0..8 {
            let mut forged = record.clone();
            match field {
                0 => forged.schema.push('x'),
                1 => forged.catalog_hash ^= 1,
                2 => forged.source_sha256.push('x'),
                3 => forged.target_count = 0,
                4 => forged.sacrifice_source_as_cost = false,
                5 => forged.damage_is_nonnegative_saved_power = false,
                6 => forged
                    .damage_controller_color_lifelink_deathtouch_werewolf_captured_at_departure =
                    false,
                _ => forged.ability_index = 1,
            }
            assert!(validate(&forged).is_err());
        }
    }
}
