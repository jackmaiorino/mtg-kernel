//! Precise mandatory additional-cost semantics, separate from frozen models.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "rules_meaning_eaten_alive/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub catalog_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    NormalSpell,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub printed_cost: String,
    pub sorcery: bool,
    pub additional_cost_mandatory: bool,
    pub additional_branches_ordered: Vec<String>,
    pub sacrifice_count: u8,
    pub sacrifice_current_controlled_creature: bool,
    pub sacrifice_is_target: bool,
    pub sacrifice_requires_untapped: bool,
    pub sacrifice_requires_haste: bool,
    pub additional_mana_cost: String,
    pub unreduced_mana_branch_total: String,
    pub combined_payment_modifiers: bool,
    pub target_count: u8,
    pub creature_or_planeswalker: bool,
    pub captured_target_incarnation: bool,
    pub exile: bool,
    pub cost_paid_before_responses: bool,
    pub all_illegal_targets_fizzle_without_refund: bool,
    pub copies_pay_additional_cost: bool,
    pub frozen_model_admission: bool,
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
        include_str!("rules_meaning_eaten_alive_v1.rs"),
        include_str!("eaten_alive_v1.rs"),
        include_str!("engine/mandatory_cast_cost_v1.rs"),
        include_str!("engine.rs"),
        include_str!("spell_costs_v1.rs"),
        include_str!("mana.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
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
pub fn printed_meaning(card_id: u16) -> Result<MeaningV1, String> {
    crate::eaten_alive_v1::exact_recipe(card_id)?;
    let definition = &crate::card_def::CARD_DEFS[usize::from(card_id)];
    let root_sha256 = digest(
        &serde_json::to_vec(&(
            SCHEMA,
            card_id,
            definition.cost.generic,
            format!("{:?}", definition.cost.pips),
            definition.cost.x_count,
            definition.mana_value,
            format!("{:?}", definition.types),
            format!("{:?}", definition.colors),
            format!("{:?}", definition.additional_cost),
            (definition.spell_effect)(),
            definition.target_spec,
        ))
        .map_err(|e| e.to_string())?,
    );
    Ok(MeaningV1 {
        selector: selector(),
        card_id,
        root: RootV1::NormalSpell,
        root_sha256,
        printed_cost: "{B}".into(),
        sorcery: true,
        additional_cost_mandatory: true,
        additional_branches_ordered: vec!["SacrificeCreature".into(), "PayMana".into()],
        sacrifice_count: 1,
        sacrifice_current_controlled_creature: true,
        sacrifice_is_target: false,
        sacrifice_requires_untapped: false,
        sacrifice_requires_haste: false,
        additional_mana_cost: "{3}{B}".into(),
        unreduced_mana_branch_total: "{3}{B}{B}".into(),
        combined_payment_modifiers: true,
        target_count: 1,
        creature_or_planeswalker: true,
        captured_target_incarnation: true,
        exile: true,
        cost_paid_before_responses: true,
        all_illegal_targets_fizzle_without_refund: true,
        copies_pay_additional_cost: false,
        frozen_model_admission: false,
    })
}
pub fn validate(value: &MeaningV1) -> Result<(), String> {
    if *value == printed_meaning(value.card_id)? {
        Ok(())
    } else {
        Err("Eaten Alive complete rooted meaning differs".into())
    }
}
