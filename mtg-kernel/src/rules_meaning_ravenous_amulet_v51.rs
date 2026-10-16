//! Ordered printed semantics, without private payment or history operands.
use crate::card_def::{CardCapability, CARD_DEFS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "rules_meaning_ravenous_amulet/v51";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV51 {
    DrawAndSoul,
    SoulLoss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostV51 {
    GenericMana(u8),
    TapSource,
    SacrificeControlledCreature,
    SacrificeSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionV51 {
    DrawOneForAbilityController,
    PlaceOneSoulCounterOnOriginalBattlefieldSource,
    EachOpponentLosesLifeEqualOriginalSourceSoulCount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV51 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
    pub card_id: u16,
    pub capability: CardCapability,
    pub root: RootV51,
    pub root_sha256: String,
    pub ordered_costs: Vec<CostV51>,
    pub ordered_instructions: Vec<InstructionV51>,
    pub sorcery_speed_only: bool,
    pub untargeted: bool,
    pub ability_controller_is_original_controller: bool,
    pub loss_uses_original_generation_lki: bool,
    pub placement_uses_resolution_world: bool,
    pub model_admitted: bool,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("rules_meaning_ravenous_amulet_v51.rs"),
            include_str!("ravenous_amulet_v1.rs"),
            include_str!("soul_counters_v1.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("state.rs"),
            include_str!("engine.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("card_def.rs"),
            include_str!("trigger.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_statics_v1.rs"),
            include_str!("../build.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("planeswalker_v1.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
    .clone()
}

pub fn printed_meaning(card_id: u16, root: RootV51) -> Result<MeaningV51, String> {
    if !crate::ravenous_amulet_v1::definition(card_id) {
        return Err("Ravenous exact executable printing absent or changed".into());
    }
    let draw = root == RootV51::DrawAndSoul;
    let ordered_costs = if draw {
        vec![
            CostV51::GenericMana(1),
            CostV51::TapSource,
            CostV51::SacrificeControlledCreature,
        ]
    } else {
        vec![
            CostV51::GenericMana(4),
            CostV51::TapSource,
            CostV51::SacrificeSource,
        ]
    };
    let ordered_instructions = if draw {
        vec![
            InstructionV51::DrawOneForAbilityController,
            InstructionV51::PlaceOneSoulCounterOnOriginalBattlefieldSource,
        ]
    } else {
        vec![InstructionV51::EachOpponentLosesLifeEqualOriginalSourceSoulCount]
    };
    let definition = &CARD_DEFS[usize::from(card_id)];
    let program = (definition.activated_abilities[usize::from(!draw)].effect)();
    let root_sha256 = digest(
        &serde_json::to_vec(&(
            SCHEMA,
            card_id,
            root,
            &ordered_costs,
            &ordered_instructions,
            program,
        ))
        .map_err(|error| error.to_string())?,
    );
    Ok(MeaningV51 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
        card_id,
        capability: definition.capability,
        root,
        root_sha256,
        ordered_costs,
        ordered_instructions,
        sorcery_speed_only: draw,
        untargeted: true,
        ability_controller_is_original_controller: true,
        loss_uses_original_generation_lki: !draw,
        placement_uses_resolution_world: draw,
        model_admitted: false,
    })
}

pub fn validate(value: &MeaningV51) -> Result<(), String> {
    if *value == printed_meaning(value.card_id, value.root)? {
        Ok(())
    } else {
        Err("Ravenous ordered printing descriptor differs".into())
    }
}
