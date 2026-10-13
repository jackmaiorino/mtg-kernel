//! Printed rules and the actual prepared producer, separate from frozen models.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "rules_meaning_fiendish_panda/v1";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    CreatureSpell,
    ControllerLifeGain,
    SourceDeath,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub catalog_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub printed_cost: String,
    pub printed_power: i32,
    pub printed_toughness: i32,
    pub ordered_printed_roots: Vec<RootV1>,
    pub positive_controller_gain_event: bool,
    pub one_counter_per_event: bool,
    pub counter_count_multiplied_by_life_amount: bool,
    pub counter_exact_original_battlefield_incarnation: bool,
    pub counter_requires_source_still_creature: bool,
    pub actual_source_creature_death: bool,
    pub capture_before_simultaneous_departures: bool,
    pub last_battlefield_power_signed: bool,
    pub mandatory_single_target: bool,
    pub target_own_graveyard_non_token_creature_card: bool,
    pub target_excludes_bear_and_changeling: bool,
    pub target_excludes_physical_source_id: bool,
    pub target_mana_value_at_most_captured_power: bool,
    pub negative_power_has_no_nonnegative_mana_value: bool,
    pub target_incarnation_rechecked: bool,
    pub return_to_battlefield: bool,
    pub instruction_forces_tapped: bool,
    pub complete_publication_custody: bool,
    pub safe_consumer_admitted: bool,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_fiendish_panda_v1.rs"),
        include_str!("fiendish_panda_v1.rs"),
        include_str!("creature_death_capture_v1.rs"),
        include_str!("life_gain_turn_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("effect/crystal_prevention_v1.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update((source.len() as u64).to_le_bytes());
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn selector() -> SelectorV1 {
    SelectorV1 {
        schema: SCHEMA.into(),
        catalog_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    }
}
pub fn printed_meaning(card_id: u16, root: RootV1) -> Result<MeaningV1, String> {
    if !crate::fiendish_panda_v1::owns_definition(card_id) {
        return Err("Fiendish Panda exact printing absent or changed".into());
    }
    let definition = &crate::card_def::CARD_DEFS[usize::from(card_id)];
    let program = match root {
        RootV1::CreatureSpell => (definition.spell_effect)(),
        RootV1::ControllerLifeGain => Some(crate::fiendish_panda_v1::life_recipe()),
        RootV1::SourceDeath => Some(crate::fiendish_panda_v1::death_recipe()),
    };
    let root_sha256 = digest(
        &serde_json::to_vec(&(
            SCHEMA,
            card_id,
            root,
            definition.cost.generic,
            format!("{:?}", definition.cost.pips),
            definition.cost.x_count,
            definition.mana_value,
            format!("{:?}", definition.types),
            format!("{:?}", definition.colors),
            format!("{:?}", definition.subtypes),
            definition.power,
            definition.toughness,
            program,
        ))
        .map_err(|e| e.to_string())?,
    );
    let life = root == RootV1::ControllerLifeGain;
    let death = root == RootV1::SourceDeath;
    Ok(MeaningV1 {
        selector: selector(),
        card_id,
        root,
        root_sha256,
        printed_cost: "{2}{W}{B}".into(),
        printed_power: 3,
        printed_toughness: 2,
        ordered_printed_roots: vec![RootV1::ControllerLifeGain, RootV1::SourceDeath],
        positive_controller_gain_event: life,
        one_counter_per_event: life,
        counter_count_multiplied_by_life_amount: false,
        counter_exact_original_battlefield_incarnation: life,
        counter_requires_source_still_creature: false,
        actual_source_creature_death: death,
        capture_before_simultaneous_departures: death,
        last_battlefield_power_signed: death,
        mandatory_single_target: death,
        target_own_graveyard_non_token_creature_card: death,
        target_excludes_bear_and_changeling: death,
        target_excludes_physical_source_id: death,
        target_mana_value_at_most_captured_power: death,
        negative_power_has_no_nonnegative_mana_value: death,
        target_incarnation_rechecked: death,
        return_to_battlefield: death,
        instruction_forces_tapped: false,
        complete_publication_custody: false,
        safe_consumer_admitted: false,
    })
}
pub fn validate(value: &MeaningV1) -> Result<(), String> {
    if *value == printed_meaning(value.card_id, value.root)? {
        Ok(())
    } else {
        Err("Fiendish Panda full rooted meaning differs".into())
    }
}
