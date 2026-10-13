//! Exact definition-owned creation plus reflexive targeting, outside frozen meanings.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::mana::ManaColor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/faebloom/v43";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV43 {
    SpellCreation,
    ReflexiveTap,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV43 {
    pub schema: String,
    pub catalog_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV43 {
    pub selector: SelectorV43,
    pub card_id: u16,
    pub root: RootV43,
    pub root_sha256: String,
    pub printed_mana_cost: String,
    pub instant: bool,
    pub spell_target_count: u8,
    pub token_card_id: u16,
    pub token_count: u8,
    pub token_power: i16,
    pub token_toughness: i16,
    pub token_blue: bool,
    pub token_faerie: bool,
    pub token_flying: bool,
    pub genuine_creation_required: bool,
    pub separate_reflexive_stack_item: bool,
    pub mandatory_reflexive: bool,
    pub target_count: u8,
    pub target_opposing_creature: bool,
    pub captured_spell_controller: bool,
    pub original_blue_spell_source: bool,
    pub copied_spell_distinct_occurrence: bool,
    pub source_survival_required: bool,
    pub current_legality_and_original_target_incarnation_rechecked: bool,
    pub no_legal_target_drops_reflexive_only: bool,
    pub tap_already_tapped_is_legal: bool,
    pub apnap_and_responses: bool,
    pub mandatory_original_owner_and_completion: bool,
    pub registration_or_model_admission: bool,
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_faebloom_v43.rs"),
        include_str!("faebloom_trick_v43.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("mana.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("controlled_creature_grants_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("../build.rs"),
    ] {
        h.update([0]);
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector() -> Result<SelectorV43, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Faebloom meaning requires pure Limited catalog".into());
    }
    Ok(SelectorV43 {
        schema: SCHEMA.into(),
        catalog_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(selector: &SelectorV43, card_id: u16, root: RootV43) -> Result<MeaningV43, String> {
    if *selector != live_selector()? {
        return Err("Faebloom source/schema/catalog selector differs".into());
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or("unknown Faebloom definition")?;
    if !crate::faebloom_trick_v43::selected_definition(card_id)
        || def.mana_value != 3
        || def.keywords != Keywords::NONE
        || !def.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
        || def.flashback.is_some()
        || def.kicker_cost.is_some()
        || def.additional_cost.is_some()
        || !def.additional_modes.is_empty()
        || def.equipment.is_some()
        || !def.produces_mana.is_empty()
        || !def.mana_ability_choices.is_empty()
        || (def.mana_ability)().is_some()
        || def.starting_loyalty.is_some()
        || def.enters_with_plus_one_counters.is_some()
        || def.controlled_creature_grant.is_some()
        || def.controlled_counter_keyword.is_some()
        || def.game_outcome_protection_v1.is_some()
        || def.generic_cost_reduction.is_some()
        || def.spell_cannot_be_countered
        || def.records_mana_spent
    {
        return Err("Faebloom printing/root/metadata differs".into());
    }
    let token_id =
        crate::card_def::card_id_by_name("Faerie Token").ok_or("missing actual Faerie token")?;
    let token = &CARD_DEFS[usize::from(token_id)];
    if !token.is_executable()
        || !token.is_token
        || token.types != [CardType::Creature]
        || token.subtypes != [Subtype::Faerie]
        || token.colors != [ManaColor::U]
        || token.power != Some(1)
        || token.toughness != Some(1)
        || token.keywords != Keywords::FLYING
        || !token.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(token_id).is_empty()
        || token.ward_cost.is_some()
        || token.is_land
        || !token.supertypes.is_empty()
    {
        return Err("Faebloom actual token blueprint differs".into());
    }
    let root_sha256 = hash(
        &serde_json::to_vec(&(
            card_id,
            root,
            def.name,
            def.cost,
            def.types,
            def.colors,
            crate::faebloom_trick_v43::recipe(),
            TargetSpec::OpponentControlledCreature,
            token_id,
            token.name,
            token.types,
            token.subtypes,
            token.colors,
            token.power,
            token.toughness,
            token.keywords,
        ))
        .map_err(|e| e.to_string())?,
    );
    Ok(MeaningV43 {
        selector: selector.clone(),
        card_id,
        root,
        root_sha256,
        printed_mana_cost: "{2}{U}".into(),
        instant: true,
        spell_target_count: 0,
        token_card_id: token_id,
        token_count: 2,
        token_power: 1,
        token_toughness: 1,
        token_blue: true,
        token_faerie: true,
        token_flying: true,
        genuine_creation_required: true,
        separate_reflexive_stack_item: true,
        mandatory_reflexive: true,
        target_count: 1,
        target_opposing_creature: true,
        captured_spell_controller: true,
        original_blue_spell_source: true,
        copied_spell_distinct_occurrence: true,
        source_survival_required: false,
        current_legality_and_original_target_incarnation_rechecked: true,
        no_legal_target_drops_reflexive_only: true,
        tap_already_tapped_is_legal: true,
        apnap_and_responses: true,
        mandatory_original_owner_and_completion: true,
        registration_or_model_admission: false,
    })
}
pub fn validate(selector: &SelectorV43, artifact: &MeaningV43) -> Result<(), String> {
    if *artifact != meaning(selector, artifact.card_id, artifact.root)? {
        return Err("forged Faebloom precise meaning".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_catalog_and_genuine_definition_required_before_lookup() {
        let foreign = crate::card_def::card_id_by_name("Forest").unwrap();
        if let Ok(selector) = live_selector() {
            for root in [RootV43::SpellCreation, RootV43::ReflexiveTap] {
                assert!(meaning(&selector, foreign, root).is_err());
                let mut wrong = selector.clone();
                wrong.catalog_hash ^= 1;
                assert!(meaning(&wrong, foreign, root).is_err());
                let mut wrong = selector.clone();
                wrong.producer_sha256.clear();
                assert!(meaning(&wrong, foreign, root).is_err());
            }
        } else {
            assert!(!cfg!(all(
                feature = "limited-fdn-fixtures",
                not(feature = "standard-magezero-fixtures")
            )))
        }
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn real_printing_two_roots_and_complete_fact_or_unknown_field_forgery_refuse() {
        let selector = live_selector().unwrap();
        let card = crate::card_def::card_id_by_name("Faebloom Trick").unwrap();
        for root in [RootV43::SpellCreation, RootV43::ReflexiveTap] {
            let expected = meaning(&selector, card, root).unwrap();
            validate(&selector, &expected).unwrap();
            let value = serde_json::to_value(&expected).unwrap();
            for (name, item) in value.as_object().unwrap() {
                if item.is_boolean() {
                    let mut forged = value.clone();
                    forged[name] = serde_json::Value::Bool(!item.as_bool().unwrap());
                    let changed: MeaningV43 = serde_json::from_value(forged).unwrap();
                    assert!(validate(&selector, &changed).is_err(), "{name}");
                }
            }
            let mut unknown = value;
            unknown["unrepresented"] = serde_json::Value::Bool(true);
            assert!(serde_json::from_value::<MeaningV43>(unknown).is_err());
            let mut changed = expected.clone();
            changed.token_count = 1;
            assert!(validate(&selector, &changed).is_err());
            let mut changed = expected.clone();
            changed.target_count = 0;
            assert!(validate(&selector, &changed).is_err());
            let mut changed = expected;
            changed.root_sha256.clear();
            assert!(validate(&selector, &changed).is_err());
        }
    }
}
