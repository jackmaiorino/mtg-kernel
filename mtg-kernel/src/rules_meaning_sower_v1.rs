//! Precise selected printed activation meaning. Frozen vectors remain opaque.
use crate::card_def::{ActivationTargetFilter, CostComponent, TargetSpec, CARD_DEFS};
use crate::effect::EffectOp;
use crate::mana::{Cost, ManaColor, Pip};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/sower-of-chaos/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub ability_index: u16,
    pub root_sha256: String,
    pub activation_cost: &'static str,
    pub activation_zone: &'static str,
    pub requires_tap: bool,
    pub sorcery_speed_only: bool,
    pub target_spec: TargetSpec,
    pub target_minimum: u8,
    pub target_maximum: u8,
    pub target_controller_filter: &'static str,
    pub target_power_filter: &'static str,
    pub recheck_full_target_legality: bool,
    pub exact_target_incarnation: bool,
    pub effect: &'static str,
    pub duration: &'static str,
    pub remove_by_ability_suppression: bool,
    pub follows_control_change: bool,
    pub source_survival_required_after_payment: bool,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut h = Sha256::new();
    for s in [
        SCHEMA,
        include_str!("rules_meaning_sower_v1.rs"),
        include_str!("sower_of_chaos_v1.rs"),
        include_str!("engine.rs"),
        include_str!("effect.rs"),
        include_str!("../build.rs"),
    ] {
        h.update([0]);
        h.update(s.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector_v1() -> Result<SelectorV1, ErrorV1> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV1::UnsupportedCatalog);
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    })
}
pub fn meaning_v1(
    selector: &SelectorV1,
    card_id: u16,
    ability_index: u16,
) -> Result<MeaningV1, ErrorV1> {
    if selector != &live_selector_v1()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Sower of Chaos" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let ability = def
        .activated_abilities
        .get(usize::from(ability_index))
        .ok_or(ErrorV1::WrongRoot)?;
    let expected_cost = [CostComponent::Mana(Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    })];
    let program = (ability.effect)();
    if def.activated_abilities.len() != 1
        || ability_index != 0
        || ability.cost != expected_cost
        || ability.activation_zone != crate::state::Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.target_spec != TargetSpec::Creature
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || program != (EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 })
    {
        return Err(ErrorV1::WrongRoot);
    }
    let bytes = serde_json::to_vec(&(
        card_id,
        ability_index,
        format!("{:?}", ability.cost),
        ability.activation_zone,
        ability.sorcery_speed_only,
        ability.target_spec,
        format!("{:?}", ability.activation_target_filter),
        ability.max_activations_per_turn,
        program,
    ))
    .expect("printed root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        ability_index,
        root_sha256: digest(&bytes),
        activation_cost: "{2}{R}",
        activation_zone: "battlefield",
        requires_tap: false,
        sorcery_speed_only: false,
        target_spec: TargetSpec::Creature,
        target_minimum: 1,
        target_maximum: 1,
        target_controller_filter: "any",
        target_power_filter: "none",
        recheck_full_target_legality: true,
        exact_target_incarnation: true,
        effect: "target_cannot_block",
        duration: "until_next_cleanup_including_repeated_cleanup",
        remove_by_ability_suppression: false,
        follows_control_change: true,
        source_survival_required_after_payment: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_selector_foreign_definition_and_legacy_catalog_refusal() {
        if let Ok(selector) = live_selector_v1() {
            let bolt = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
            assert_eq!(
                meaning_v1(&selector, bolt, 0),
                Err(ErrorV1::UnsupportedDefinition)
            );
            for field in 0..3 {
                let mut forged = selector.clone();
                match field {
                    0 => forged.schema.push('x'),
                    1 => forged.card_db_hash ^= 1,
                    _ => forged.producer_sha256.push('x'),
                }
                assert_eq!(meaning_v1(&forged, bolt, 0), Err(ErrorV1::WrongSelector));
            }
        } else {
            assert_eq!(live_selector_v1(), Err(ErrorV1::UnsupportedCatalog));
        }
    }
}
