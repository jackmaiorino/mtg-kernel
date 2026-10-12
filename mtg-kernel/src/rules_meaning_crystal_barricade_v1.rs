//! Selected printed static rules. This does not admit a public/model consumer.
use crate::card_def::{CardType, Keywords, Subtype, CARD_DEFS};
use crate::mana::{Cost, ManaColor, Pip};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/crystal-barricade/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
/// Printed static rules, never action, effect or activated-ability ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaticRootV1 {
    ControllerHexproof,
    OtherControlledCreatureNoncombatPrevention,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV1 {
    ControllerHexproof {
        opponents_cannot_target_controller_with_spells_or_abilities: bool,
        controller_can_target_themself: bool,
        does_not_grant_object_hexproof: bool,
        target_legality_rechecked_at_resolution: bool,
    },
    OtherControlledCreatureNoncombatPrevention {
        prevent_all_positive_noncombat_damage: bool,
        recipient_is_current_controlled_battlefield_effective_creature: bool,
        exclude_this_source_permanent: bool,
        another_source_can_protect_this_source: bool,
        combat_and_player_damage_unaffected: bool,
        cannot_be_prevented_overrides_this_rule: bool,
        fully_prevented_damage_is_not_committed: bool,
        fully_prevented_damage_causes_no_damage_triggers_or_lifelink: bool,
        affected_player_mixed_replacement_ordering_supported: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub static_root: StaticRootV1,
    pub root_sha256: String,
    pub source_requires_current_front_face_battlefield_and_active_printed_abilities: bool,
    pub current_controller_is_beneficiary: bool,
    pub physical_owner_original_cast_controller_and_tapped_state_do_not_limit_rule: bool,
    pub departure_or_ability_suppression_ends_rule_immediately: bool,
    pub control_change_updates_beneficiary_immediately: bool,
    pub no_installed_effect_or_resolution_receipt: bool,
    pub rule: RuleV1,
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
    let mut h = Sha256::new();
    for (path, source) in [
        (
            "meaning",
            include_str!("rules_meaning_crystal_barricade_v1.rs"),
        ),
        ("static", include_str!("crystal_barricade_v1.rs")),
        (
            "characteristics",
            include_str!("continuous_characteristics_v1.rs"),
        ),
        ("engine", include_str!("engine.rs")),
        ("event", include_str!("event.rs")),
        ("card_def", include_str!("card_def.rs")),
        ("build", include_str!("../build.rs")),
    ] {
        h.update(path.as_bytes());
        h.update([0]);
        h.update(source.as_bytes());
        h.update([0]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
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
pub fn meaning(
    selector: &SelectorV1,
    card_id: u16,
    static_root: StaticRootV1,
) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Crystal Barricade" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    if def.cost
        != (Cost {
            generic: 1,
            pips: &[Pip::Colored(ManaColor::W)],
            x_count: 0,
        })
        || def.types != [CardType::Artifact, CardType::Creature]
        || def.colors != [ManaColor::W]
        || def.subtypes != [Subtype::Wall]
        || (def.power, def.toughness) != (Some(0), Some(4))
        || def.keywords != Keywords::DEFENDER
        || (def.spell_effect)().is_some()
        || !def.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
    {
        return Err(ErrorV1::WrongRoot);
    }
    let rule = match static_root {
        StaticRootV1::ControllerHexproof => RuleV1::ControllerHexproof {
            opponents_cannot_target_controller_with_spells_or_abilities: true,
            controller_can_target_themself: true,
            does_not_grant_object_hexproof: true,
            target_legality_rechecked_at_resolution: true,
        },
        StaticRootV1::OtherControlledCreatureNoncombatPrevention => {
            RuleV1::OtherControlledCreatureNoncombatPrevention {
                prevent_all_positive_noncombat_damage: true,
                recipient_is_current_controlled_battlefield_effective_creature: true,
                exclude_this_source_permanent: true,
                another_source_can_protect_this_source: true,
                combat_and_player_damage_unaffected: true,
                cannot_be_prevented_overrides_this_rule: true,
                fully_prevented_damage_is_not_committed: true,
                fully_prevented_damage_causes_no_damage_triggers_or_lifelink: true,
                affected_player_mixed_replacement_ordering_supported: false,
            }
        }
    };
    let bytes = serde_json::to_vec(&(
        card_id,
        static_root,
        def.cost,
        def.types,
        def.subtypes,
        def.power,
        def.toughness,
        &rule,
    ))
    .expect("typed static root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        static_root,
        root_sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        source_requires_current_front_face_battlefield_and_active_printed_abilities: true,
        current_controller_is_beneficiary: true,
        physical_owner_original_cast_controller_and_tapped_state_do_not_limit_rule: true,
        departure_or_ability_suppression_ends_rule_immediately: true,
        control_change_updates_beneficiary_immediately: true,
        no_installed_effect_or_resolution_receipt: true,
        rule,
    })
}
/// Reconstruct every semantic fact. Artifact validation does not attest gameplay.
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id, record.static_root)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_catalog_refusal_and_selected_identity_precede_definition_lookup() {
        let Ok(selector) = live_selector() else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        for field in 0..3 {
            let mut forged = selector.clone();
            match field {
                0 => forged.schema.push('x'),
                1 => forged.card_db_hash ^= 1,
                _ => forged.producer_sha256.push('x'),
            }
            assert_eq!(
                meaning(&forged, u16::MAX, StaticRootV1::ControllerHexproof),
                Err(ErrorV1::WrongSelector)
            );
        }
        assert_eq!(
            meaning(
                &selector,
                crate::card_def::card_id_by_name("Lightning Bolt").unwrap(),
                StaticRootV1::ControllerHexproof
            ),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
}
