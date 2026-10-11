//! Selected, definition-owned Self-Reflection meaning. Frozen vectors stay opaque.
use crate::card_def::{self, CardType, CostComponent, TargetSpec, CARD_DEFS};
use crate::effect::EffectOp;
use crate::mana::{Cost, ManaColor, Pip};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/self-reflection/v1";

/// These are printed casting roots, not new effect or action ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastRootV1 {
    Normal,
    Flashback,
}
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
    MissingAnonymousBlueprint,
    IdentityMismatch,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub cast_root: CastRootV1,
    pub root_sha256: String,
    pub mana_cost: String,
    pub cast_from_own_graveyard: bool,
    pub flashback_exiles_when_leaving_stack: bool,
    pub sorcery_timing: bool,
    pub target_count: u8,
    pub target_creature_controlled_by_spell_controller: bool,
    pub target_owner_unrestricted: bool,
    pub token_and_legendary_targets_allowed: bool,
    pub exact_target_incarnation_and_full_legality_rechecked: bool,
    pub current_copiable_definition_and_face_at_resolution: bool,
    pub historical_definition_change_keeps_physical_incarnation_authority: bool,
    pub token_owner_and_controller_are_spell_controller: bool,
    pub ordinary_battlefield_entry_and_legend_rule: bool,
    pub counters_damage_tap_attachments_and_temporary_grants_not_copied: bool,
    pub no_added_haste_or_delayed_sacrifice: bool,
    pub face_down_copy_is_face_up_nameless_colorless_creature_two_two: bool,
    pub face_down_copy_has_no_hidden_identity_or_printed_abilities: bool,
    pub face_down_copy_has_no_mana_cost_subtypes_supertypes_or_face_up_permission: bool,
    pub anonymous_blueprint_card_id: u16,
    pub illegal_target_creates_no_token: bool,
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for (path, source) in [
        (
            "rules_meaning_self_reflection_v1.rs",
            include_str!("rules_meaning_self_reflection_v1.rs"),
        ),
        (
            "self_reflection_v1.rs",
            include_str!("self_reflection_v1.rs"),
        ),
        ("standard_cards_v1.rs", include_str!("standard_cards_v1.rs")),
        (
            "standard_legends_v1.rs",
            include_str!("standard_legends_v1.rs"),
        ),
        ("engine.rs", include_str!("engine.rs")),
        ("effect.rs", include_str!("effect.rs")),
        ("event.rs", include_str!("event.rs")),
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
    cast_root: CastRootV1,
) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Self-Reflection" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let normal_cost = Cost {
        generic: 4,
        pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)],
        x_count: 0,
    };
    let flashback_cost = Cost {
        generic: 3,
        pips: &[Pip::Colored(ManaColor::U)],
        x_count: 0,
    };
    let program = (def.spell_effect)();
    if def.types != [CardType::Sorcery]
        || def.cost != normal_cost
        || def.target_spec != TargetSpec::ControlledCreature
        || program != Some(EffectOp::CreateControlledCreatureTargetCopyV1)
        || !matches!(def.flashback.as_ref().map(|f| f.cost), Some([CostComponent::Mana(cost)]) if *cost == flashback_cost)
        || def.additional_cost.is_some()
        || def.alt_cost.is_some()
        || def.kicker_cost.is_some()
        || def.mode2.is_some()
        || def.mode3.is_some()
        || !def.additional_modes.is_empty()
    {
        return Err(ErrorV1::WrongRoot);
    }
    let blueprint = card_def::card_id_by_name(crate::self_reflection_v1::ANONYMOUS_BLUEPRINT)
        .filter(|id| crate::self_reflection_v1::anonymous_blueprint_is_exact(*id))
        .ok_or(ErrorV1::MissingAnonymousBlueprint)?;
    let selected_cost = match cast_root {
        CastRootV1::Normal => normal_cost,
        CastRootV1::Flashback => flashback_cost,
    };
    let root_bytes = serde_json::to_vec(&(
        card_id,
        cast_root,
        selected_cost,
        def.target_spec,
        program,
        blueprint,
    ))
    .expect("typed printed root serializes");
    let flashback = cast_root == CastRootV1::Flashback;
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        cast_root,
        root_sha256: Sha256::digest(root_bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        mana_cost: if flashback { "{3}{U}" } else { "{4}{U}{U}" }.into(),
        cast_from_own_graveyard: flashback,
        flashback_exiles_when_leaving_stack: flashback,
        sorcery_timing: true,
        target_count: 1,
        target_creature_controlled_by_spell_controller: true,
        target_owner_unrestricted: true,
        token_and_legendary_targets_allowed: true,
        exact_target_incarnation_and_full_legality_rechecked: true,
        current_copiable_definition_and_face_at_resolution: true,
        historical_definition_change_keeps_physical_incarnation_authority: true,
        token_owner_and_controller_are_spell_controller: true,
        ordinary_battlefield_entry_and_legend_rule: true,
        counters_damage_tap_attachments_and_temporary_grants_not_copied: true,
        no_added_haste_or_delayed_sacrifice: true,
        face_down_copy_is_face_up_nameless_colorless_creature_two_two: true,
        face_down_copy_has_no_hidden_identity_or_printed_abilities: true,
        face_down_copy_has_no_mana_cost_subtypes_supertypes_or_face_up_permission: true,
        anonymous_blueprint_card_id: blueprint,
        illegal_target_creates_no_token: true,
    })
}
/// Reconstruct the complete record, including its printed root and producer.
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id, record.cast_root)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_catalog_refuses_and_selected_identity_precedes_definition_lookup() {
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
                meaning(&stale, u16::MAX, CastRootV1::Normal),
                Err(ErrorV1::WrongSelector)
            );
        }
        assert_eq!(
            meaning(&selector, u16::MAX, CastRootV1::Normal),
            Err(ErrorV1::UnsupportedDefinition)
        );
        let bolt = card_def::card_id_by_name("Lightning Bolt").unwrap();
        assert_eq!(
            meaning(&selector, bolt, CastRootV1::Normal),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
    #[test]
    fn actual_printed_normal_and_flashback_roots_have_distinct_binding_and_complete_copy_facts() {
        let Ok(selector) = live_selector() else {
            return;
        };
        let id = card_def::card_id_by_name("Self-Reflection").unwrap();
        let normal = meaning(&selector, id, CastRootV1::Normal).unwrap();
        let flashback = meaning(&selector, id, CastRootV1::Flashback).unwrap();
        assert_eq!(normal.mana_cost, "{4}{U}{U}");
        assert_eq!(flashback.mana_cost, "{3}{U}");
        assert!(!normal.cast_from_own_graveyard && !normal.flashback_exiles_when_leaving_stack);
        assert!(flashback.cast_from_own_graveyard && flashback.flashback_exiles_when_leaving_stack);
        assert_ne!(normal.root_sha256, flashback.root_sha256);
        assert_eq!(
            normal.anonymous_blueprint_card_id,
            flashback.anonymous_blueprint_card_id
        );
        for record in [normal, flashback] {
            assert!(record.target_owner_unrestricted && record.token_and_legendary_targets_allowed);
            assert!(record.current_copiable_definition_and_face_at_resolution);
            assert!(record.historical_definition_change_keeps_physical_incarnation_authority);
            assert!(record.face_down_copy_has_no_hidden_identity_or_printed_abilities);
            assert!(record.counters_damage_tap_attachments_and_temporary_grants_not_copied);
            validate(&record).unwrap();
            let bytes = serde_json::to_vec(&record).unwrap();
            let decoded: MeaningV1 = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(decoded, record);
        }
    }
    #[test]
    fn forged_copy_facts_cast_root_and_blueprint_are_rejected_by_complete_reconstruction() {
        let Ok(selector) = live_selector() else {
            return;
        };
        let id = card_def::card_id_by_name("Self-Reflection").unwrap();
        let good = meaning(&selector, id, CastRootV1::Normal).unwrap();
        for field in 0..6 {
            let mut forged = good.clone();
            match field {
                0 => forged.cast_root = CastRootV1::Flashback,
                1 => {
                    forged.anonymous_blueprint_card_id =
                        card_def::card_id_by_name("Llanowar Elves").unwrap()
                }
                2 => forged.face_down_copy_has_no_hidden_identity_or_printed_abilities = false,
                3 => forged.current_copiable_definition_and_face_at_resolution = false,
                4 => forged.target_count = 2,
                _ => forged.root_sha256.push('x'),
            }
            assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch));
        }
        let mut unknown = serde_json::to_value(good).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("hidden_underlying_definition".into(), 42.into());
        assert!(serde_json::from_value::<MeaningV1>(unknown).is_err());
    }
}
