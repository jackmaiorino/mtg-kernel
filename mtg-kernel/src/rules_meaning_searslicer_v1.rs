//! Precise, definition-owned Searslicer trigger companion. No vector/model admission.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::trigger::TriggerCondition;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/searslicer-goblin/v1";
pub const SCOPE: &str = "pauper+fdn/limited-without-standard";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectorV1 {
    pub schema: String,
    pub scope: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    WrongToken,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub ability_index: u16,
    pub root_sha256: String,
    pub event: &'static str,
    pub predicate: &'static str,
    pub capture_controller: bool,
    pub recheck_captured_controller_on_resolution: bool,
    pub declaration_round_and_active_seat_bound: bool,
    pub surviving_attacker_required: bool,
    pub untargeted: bool,
    pub optional: bool,
    pub token_def: u16,
    pub token_count: u8,
    pub token_controller: &'static str,
    pub token_power: i32,
    pub token_toughness: i32,
    pub token_color: &'static str,
    pub token_subtype: &'static str,
    pub token_tapped: bool,
    pub token_attacking: bool,
    pub token_has_haste: bool,
    pub token_summoning_sick: bool,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_searslicer_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("state.rs"),
        include_str!("engine.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
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
        scope: SCOPE.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    })
}
/// Caller programs, flat atom ordinals and card labels cannot select authority.
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
    if def.name != "Searslicer Goblin" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let abilities = crate::trigger::triggers_for(card_id);
    let ability = abilities
        .get(usize::from(ability_index))
        .ok_or(ErrorV1::WrongRoot)?;
    let program = (ability.effect)();
    if abilities.len() != 1
        || ability_index != 0
        || ability.condition
            != TriggerCondition::BeginningControllerEndStepAfterControllerAttackedThisTurn
        || ability.home_zone != crate::state::Zone::Battlefield
        || ability.face_index != 0
        || ability.intervening_if_kicked
        || ability.intervening_if_controls_another_source_card
        || program != crate::trigger::searslicer_goblin_effect()
        || crate::trigger::target_spec_for_trigger(card_id, &program) != Some(TargetSpec::None)
    {
        return Err(ErrorV1::WrongRoot);
    }
    let token_def = crate::card_def::card_id_by_name("Goblin Token").ok_or(ErrorV1::WrongToken)?;
    let token = &CARD_DEFS[usize::from(token_def)];
    if !token.is_token
        || !token.is_executable()
        || token.types != [CardType::Creature]
        || token.subtypes != [Subtype::Goblin]
        || token.colors != [crate::mana::ManaColor::R]
        || token.power != Some(1)
        || token.toughness != Some(1)
        || token.keywords != Keywords::NONE
        || !token.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(token_def).is_empty()
    {
        return Err(ErrorV1::WrongToken);
    }
    let root = serde_json::to_vec(&(
        card_id,
        ability_index,
        format!("{:?}", ability.condition),
        ability.home_zone,
        ability.face_index,
        ability.intervening_if_kicked,
        ability.intervening_if_controls_another_source_card,
        program,
    ))
    .expect("definition-owned trigger serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        ability_index,
        root_sha256: digest(&root),
        event: "beginning_of_captured_controller_end_step",
        predicate: "captured_controller_successfully_declared_one_or_more_attackers_this_turn",
        capture_controller: true,
        recheck_captured_controller_on_resolution: true,
        declaration_round_and_active_seat_bound: true,
        surviving_attacker_required: false,
        untargeted: true,
        optional: false,
        token_def,
        token_count: 1,
        token_controller: "captured_controller",
        token_power: 1,
        token_toughness: 1,
        token_color: "R",
        token_subtype: "Goblin",
        token_tapped: false,
        token_attacking: false,
        token_has_haste: false,
        token_summoning_sick: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_selector_and_registered_foreign_definition_refuse() {
        if let Ok(selector) = live_selector_v1() {
            let foreign = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
            assert_eq!(
                meaning_v1(&selector, foreign, 0),
                Err(ErrorV1::UnsupportedDefinition)
            );
            for field in 0..4 {
                let mut forged = selector.clone();
                match field {
                    0 => forged.schema.push('x'),
                    1 => forged.scope.push('x'),
                    2 => forged.card_db_hash ^= 1,
                    _ => forged.producer_sha256.push('x'),
                }
                assert_eq!(meaning_v1(&forged, foreign, 0), Err(ErrorV1::WrongSelector));
            }
        } else {
            assert_eq!(live_selector_v1(), Err(ErrorV1::UnsupportedCatalog));
        }
    }
}
