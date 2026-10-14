//! Selected Temporal Manipulation meaning, separate from frozen Opaque atoms.
use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::mana::{Cost, ManaColor, Pip};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/temporal-manipulation/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    PrintedSpell,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub no_targets: bool,
    pub beneficiary_is_captured_spell_controller: bool,
    pub after_current_turn_before_next_regular_turn: bool,
    pub last_created_extra_turn_first: bool,
    pub source_departure_does_not_cancel: bool,
    pub countered_spell_schedules_nothing: bool,
    pub distinct_turn_occurrence_for_per_turn_and_later_turn_rules: bool,
    pub regular_round_and_starting_player_convention_retained: bool,
    pub actual_cleanup_then_untap_upkeep_draw: bool,
    pub first_game_turn_draw_skip_does_not_skip_extra_draw: bool,
    pub holder_next_turn_expiry_uses_actual_holder_turn: bool,
    pub owner_next_turn_impulse_expires_at_that_turn_cleanup: bool,
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
        include_str!("rules_meaning_extra_turn_v1.rs"),
        include_str!("extra_turn_v1.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("effect.rs"),
        include_str!("trigger.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_spell_copy.rs"),
        include_str!("effect/standard_copy.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("life_gain_turn_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("card_def.rs"),
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
    if def.name != "Temporal Manipulation" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let expected = crate::extra_turn_v1::recipe();
    if def.cost
        != (Cost {
            generic: 3,
            pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)],
            x_count: 0,
        })
        || def.mana_value != 5
        || def.types != [CardType::Sorcery]
        || def.colors != [ManaColor::U]
        || !def.subtypes.is_empty()
        || !def.supertypes.is_empty()
        || def.power.is_some()
        || def.toughness.is_some()
        || def.starting_loyalty.is_some()
        || def.is_land
        || def.is_token
        || def.spell_cannot_be_countered
        || def.generic_cost_reduction.is_some()
        || def.records_mana_spent
        || def.equipment.is_some()
        || def.ward_cost.is_some()
        || (def.mana_ability)().is_some()
        || !def.produces_mana.is_empty()
        || !def.mana_ability_choices.is_empty()
        || def.keywords != Keywords::NONE
        || def.target_spec != TargetSpec::None
        || !def.activated_abilities.is_empty()
        || def.alt_cost.is_some()
        || def.kicker_cost.is_some()
        || def.additional_cost.is_some()
        || def.flashback.is_some()
        || def.escape.is_some()
        || def.plot_cost.is_some()
        || def.madness_cost.is_some()
        || def.mode2.is_some()
        || def.mode3.is_some()
        || !def.additional_modes.is_empty()
        || def.kicked_target_spec.is_some()
        || (def.spell_effect)() != Some(expected.clone())
    {
        return Err(ErrorV1::WrongRoot);
    }
    let rule = RuleV1 {
        no_targets: true,
        beneficiary_is_captured_spell_controller: true,
        after_current_turn_before_next_regular_turn: true,
        last_created_extra_turn_first: true,
        source_departure_does_not_cancel: true,
        countered_spell_schedules_nothing: true,
        distinct_turn_occurrence_for_per_turn_and_later_turn_rules: true,
        regular_round_and_starting_player_convention_retained: true,
        actual_cleanup_then_untap_upkeep_draw: true,
        first_game_turn_draw_skip_does_not_skip_extra_draw: true,
        holder_next_turn_expiry_uses_actual_holder_turn: true,
        owner_next_turn_impulse_expires_at_that_turn_cleanup: true,
    };
    let bytes = serde_json::to_vec(&(card_id, root, def.cost, def.target_spec, expected, &rule))
        .expect("printed root serializes");
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
    fn selector_refuses_wrong_identity_before_card_lookup_and_other_catalogs() {
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
                meaning(&wrong, u16::MAX, RootV1::PrintedSpell),
                Err(ErrorV1::WrongSelector)
            );
        }
        assert_eq!(
            meaning(
                &selector,
                crate::card_def::card_id_by_name("Counterspell").unwrap(),
                RootV1::PrintedSpell
            ),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn future_genuine_printed_root_reconstructs_every_fact_and_refuses_coherent_record_changes() {
        let selector = live_selector().unwrap();
        let id = crate::card_def::card_id_by_name("Temporal Manipulation").unwrap();
        let record = meaning(&selector, id, RootV1::PrintedSpell).unwrap();
        validate(&record).unwrap();
        let original = serde_json::to_value(&record).unwrap();
        for key in original["rule"].as_object().unwrap().keys() {
            let mut forged = original.clone();
            forged["rule"][key] = serde_json::Value::Bool(false);
            let forged: MeaningV1 = serde_json::from_value(forged).unwrap();
            assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch), "{key}");
        }
        let mut forged = record.clone();
        forged.root_sha256.push('x');
        assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch));
        let mut forged = record.clone();
        forged.public_human_sampler_and_model_admitted = true;
        assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch));
        let mut forged = record.clone();
        forged.selector.producer_sha256.push('x');
        assert_eq!(validate(&forged), Err(ErrorV1::WrongSelector));
        let mut forged = record;
        forged.card_id = crate::card_def::card_id_by_name("Counterspell").unwrap();
        assert_eq!(validate(&forged), Err(ErrorV1::UnsupportedDefinition));
    }
}
