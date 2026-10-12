//! Exact printed Raid entry roots. This companion does not admit a vector/model.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/raid-entry/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV1 {
    DamageAnyTarget {
        amount: u8,
        players: bool,
        creatures: bool,
        planeswalkers: bool,
    },
    PrivateLook {
        count: u8,
        retained_min: u8,
        retained_max: u8,
        remainder_graveyard_ordered: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub ability_index: u16,
    pub root_sha256: String,
    pub genuine_entry: bool,
    pub original_controller: bool,
    pub successful_declaration_this_turn: bool,
    pub resolution_recheck: bool,
    pub surviving_source_required: bool,
    pub instruction: InstructionV1,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut h = Sha256::new();
    for s in [
        SCHEMA,
        include_str!("rules_meaning_raid_entry_v1.rs"),
        include_str!("raid_entry_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("effect/private_keep_one_v1.rs"),
        include_str!("effect/surveil_many.rs"),
        include_str!("effect/squad_rallier_resolution_v1.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("../build.rs"),
    ] {
        h.update([0]);
        h.update(s.as_bytes());
    }
    h.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn selector_v1() -> Result<SelectorV1, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Raid entry meaning requires the Limited catalog".into());
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
) -> Result<MeaningV1, String> {
    if *selector != selector_v1()? {
        return Err("wrong Raid entry meaning selector".into());
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or("unknown definition")?;
    let (generic, color, power, subtypes, keywords, program, spec, instruction) = match def.name {
        "Gorehorn Raider" => (
            4,
            ManaColor::R,
            4,
            vec![Subtype::Minotaur, Subtype::Pirate],
            Keywords::NONE,
            crate::trigger::gorehorn_raider_effect(),
            TargetSpec::AnyTarget,
            InstructionV1::DamageAnyTarget {
                amount: 2,
                players: true,
                creatures: true,
                planeswalkers: true,
            },
        ),
        "Gutless Plunderer" => (
            2,
            ManaColor::B,
            2,
            vec![Subtype::Skeleton, Subtype::Pirate],
            Keywords::DEATHTOUCH,
            crate::trigger::gutless_plunderer_effect(),
            TargetSpec::None,
            InstructionV1::PrivateLook {
                count: 3,
                retained_min: 0,
                retained_max: 1,
                remainder_graveyard_ordered: true,
            },
        ),
        _ => return Err("unrelated printed definition".into()),
    };
    let abilities = crate::trigger::triggers_for(card_id);
    let ability = abilities
        .get(usize::from(ability_index))
        .ok_or("unknown trigger root")?;
    if !def.is_executable()
        || def.types != [CardType::Creature]
        || def.subtypes != subtypes
        || def.cost.generic != generic
        || def.cost.pips != [Pip::Colored(color)]
        || def.colors != [color]
        || def.power != Some(power)
        || def.toughness != Some(power)
        || def.keywords != keywords
        || abilities.len() != 1
        || ability_index != 0
        || ability.condition != crate::trigger::TriggerCondition::EtbAfterControllerAttackedThisTurn
        || ability.home_zone != Zone::Battlefield
        || ability.face_index != 0
        || ability.intervening_if_kicked
        || ability.intervening_if_controls_another_source_card
        || (ability.effect)() != program
        || crate::trigger::target_spec_for_trigger(card_id, &program) != Some(spec)
    {
        return Err("printed Raid metadata/root mismatch".into());
    }
    let root = serde_json::to_vec(&(card_id, ability_index, program)).map_err(|e| e.to_string())?;
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        ability_index,
        root_sha256: digest(&root),
        genuine_entry: true,
        original_controller: true,
        successful_declaration_this_turn: true,
        resolution_recheck: true,
        surviving_source_required: false,
        instruction,
    })
}
pub fn validate_v1(record: &MeaningV1) -> Result<(), String> {
    if *record != meaning_v1(&record.selector, record.card_id, record.ability_index)? {
        return Err("Raid entry meaning was rewritten".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_and_foreign_root_refuse() {
        if let Ok(selector) = selector_v1() {
            let id = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
            assert!(meaning_v1(&selector, id, 0).is_err());
            let mut bad = selector;
            bad.card_db_hash ^= 1;
            assert!(meaning_v1(&bad, id, 0).is_err());
        } else {
            if cfg!(all(
                feature = "limited-fdn-fixtures",
                not(feature = "standard-magezero-fixtures")
            )) {
                panic!("selected printed Raid catalog must provide its genuine selector");
            }
        }
    }
}
