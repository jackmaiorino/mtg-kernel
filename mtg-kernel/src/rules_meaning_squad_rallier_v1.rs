//! Selected Squad Rallier private look/pick/reveal/random-bottom meaning.
use crate::card_def::{self, ActivationTargetFilter, CostComponent, TargetSpec, CARD_DEFS};
use crate::effect::squad_rallier_root_is_exact;
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/squad-rallier/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    IdentityMismatch,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub ability_index: u16,
    pub root_sha256: String,
    pub mana_cost: String,
    pub activation_zone: Zone,
    pub instant_speed_without_tap_or_sickness_restriction: bool,
    pub repeatable: bool,
    pub target_count: u8,
    pub captured_controller_library_after_source_departure_reentry_or_control_change: bool,
    pub private_look_maximum: u8,
    pub short_library_looks_at_every_available_card: bool,
    pub select_minimum: u8,
    pub select_maximum: u8,
    pub selected_card_is_front_face_creature: bool,
    pub selected_card_effective_power_including_supported_cda_maximum: i32,
    pub selected_card_mana_value_unrestricted: bool,
    pub selection_is_nontargeting_and_ignores_hexproof_protection: bool,
    pub selected_card_revealed_to_both_then_put_in_controllers_hand: bool,
    pub rest_goes_to_bottom_in_random_order: bool,
    pub unseen_library_suffix_keeps_order: bool,
    pub empty_or_no_eligible_card_does_not_require_impossible_choice: bool,
    pub paid_exact_source_and_private_continuation_authority_required: bool,
    pub continuation_authority_is_never_public: bool,
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for (path, source) in [
        (
            "rules_meaning_squad_rallier_v1.rs",
            include_str!("rules_meaning_squad_rallier_v1.rs"),
        ),
        (
            "effect/squad_rallier_resolution_v1.rs",
            include_str!("effect/squad_rallier_resolution_v1.rs"),
        ),
        ("effect.rs", include_str!("effect.rs")),
        ("engine.rs", include_str!("engine.rs")),
        (
            "creature_stats_lki_v1.rs",
            include_str!("creature_stats_lki_v1.rs"),
        ),
        ("state.rs", include_str!("state.rs")),
        ("card_def.rs", include_str!("card_def.rs")),
        ("build.rs", include_str!("../build.rs")),
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
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(
    selector: &SelectorV1,
    card_id: u16,
    ability_index: u16,
) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Squad Rallier" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let ability = def
        .activated_abilities
        .get(usize::from(ability_index))
        .ok_or(ErrorV1::WrongRoot)?;
    let program = (ability.effect)();
    if ability_index != 0
        || def.activated_abilities.len() != 1
        || !matches!(ability.cost, [CostComponent::Mana(cost)] if cost.generic == 2 && cost.x_count == 0 && cost.pips == [Pip::Colored(ManaColor::W)])
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.target_spec != TargetSpec::None
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || !squad_rallier_root_is_exact(&program)
    {
        return Err(ErrorV1::WrongRoot);
    }
    let root_bytes = serde_json::to_vec(&(
        card_id,
        ability_index,
        program,
        ability.activation_zone,
        ability.target_spec,
    ))
    .expect("typed printed root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        ability_index,
        root_sha256: Sha256::digest(root_bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        mana_cost: "{2}{W}".into(),
        activation_zone: Zone::Battlefield,
        instant_speed_without_tap_or_sickness_restriction: true,
        repeatable: true,
        target_count: 0,
        captured_controller_library_after_source_departure_reentry_or_control_change: true,
        private_look_maximum: 4,
        short_library_looks_at_every_available_card: true,
        select_minimum: 0,
        select_maximum: 1,
        selected_card_is_front_face_creature: true,
        selected_card_effective_power_including_supported_cda_maximum: 2,
        selected_card_mana_value_unrestricted: true,
        selection_is_nontargeting_and_ignores_hexproof_protection: true,
        selected_card_revealed_to_both_then_put_in_controllers_hand: true,
        rest_goes_to_bottom_in_random_order: true,
        unseen_library_suffix_keeps_order: true,
        empty_or_no_eligible_card_does_not_require_impossible_choice: true,
        paid_exact_source_and_private_continuation_authority_required: true,
        continuation_authority_is_never_public: true,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id, record.ability_index)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_and_selector_refusal_precede_definition_and_activation_selection() {
        let Ok(selector) = live_selector() else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        for field in 0..3 {
            let mut stale = selector.clone();
            match field {
                0 => stale.schema.push('x'),
                1 => stale.card_db_hash ^= 1,
                _ => stale.producer_sha256.push('x'),
            }
            assert_eq!(
                meaning(&stale, u16::MAX, u16::MAX),
                Err(ErrorV1::WrongSelector)
            );
        }
        let bolt = card_def::card_id_by_name("Lightning Bolt").unwrap();
        assert_eq!(
            meaning(&selector, bolt, 0),
            Err(ErrorV1::UnsupportedDefinition)
        );
        let squad = card_def::card_id_by_name("Squad Rallier").unwrap();
        assert_eq!(meaning(&selector, squad, 1), Err(ErrorV1::WrongRoot));
    }
    #[test]
    fn actual_paid_root_encodes_private_power_selection_and_public_reveal_without_receipt_contents()
    {
        let Ok(selector) = live_selector() else {
            return;
        };
        let squad = card_def::card_id_by_name("Squad Rallier").unwrap();
        let record = meaning(&selector, squad, 0).unwrap();
        assert_eq!(record.mana_cost, "{2}{W}");
        assert_eq!(
            (
                record.target_count,
                record.private_look_maximum,
                record.select_minimum,
                record.select_maximum
            ),
            (0, 4, 0, 1)
        );
        assert_eq!(
            record.selected_card_effective_power_including_supported_cda_maximum,
            2
        );
        assert!(
            record.selected_card_mana_value_unrestricted
                && record.selection_is_nontargeting_and_ignores_hexproof_protection
        );
        assert!(
            record.rest_goes_to_bottom_in_random_order && record.unseen_library_suffix_keeps_order
        );
        assert!(
            record.captured_controller_library_after_source_departure_reentry_or_control_change
        );
        let bytes = serde_json::to_vec(&record).unwrap();
        let decoded: MeaningV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, record);
        validate(&decoded).unwrap();
        for field in 0..5 {
            let mut forged = record.clone();
            match field {
                0 => forged.selected_card_effective_power_including_supported_cda_maximum = 3,
                1 => forged.select_minimum = 1,
                2 => forged.rest_goes_to_bottom_in_random_order = false,
                3 => forged.continuation_authority_is_never_public = false,
                _ => forged.root_sha256.push('x'),
            }
            assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch));
        }
        let mut unknown = serde_json::to_value(record).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("looked_card_definition".into(), 42.into());
        assert!(serde_json::from_value::<MeaningV1>(unknown).is_err());
    }
}
