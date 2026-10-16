//! Ordered printing descriptors. These contain no executable death-history
//! index. Engine capability is reported exactly; model admission stays false.
use crate::card_def::CardCapability;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "rules_meaning_fake_your_own_death/v50";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV50 {
    InstantSpell,
    GainedDiesAbility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeneficiaryV50 {
    OriginalCardOwner,
    CreatureControllerAtDeath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV50 {
    BoostTargetUntilEndOfTurn {
        power: i32,
        toughness: i32,
    },
    GrantDiesAbilityUntilEndOfTurn {
        instructions: Vec<InstructionV50>,
    },
    ReturnOriginalCard {
        tapped: bool,
        controller: BeneficiaryV50,
    },
    CreateTreasure {
        controller: BeneficiaryV50,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV50 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV50 {
    pub selector: SelectorV50,
    pub card_id: u16,
    pub capability: CardCapability,
    pub root: RootV50,
    pub root_sha256: String,
    pub printed_cost: String,
    pub mandatory_single_creature_target: bool,
    pub ordered_instructions: Vec<InstructionV50>,
    pub grant_bound_to_original_battlefield_incarnation: bool,
    pub distinct_grants_create_distinct_death_triggers: bool,
    pub return_bound_to_original_graveyard_incarnation: bool,
    pub token_never_returns: bool,
    pub treasure_independent_of_return_success: bool,
    pub returning_aura_attachment_is_untargeted: bool,
    pub returning_aura_host_chosen_by_card_owner: bool,
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
            include_str!("rules_meaning_fake_your_own_death_v50.rs"),
            include_str!("fake_your_own_death_v50.rs"),
            include_str!("fake_your_own_death_v50/grant_primitive.rs"),
            include_str!("fake_your_own_death_v50/spell_custody.rs"),
            include_str!("fake_your_own_death_v50/death_custody.rs"),
            include_str!("fake_your_own_death_v50/return_aura.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("effect.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("state.rs"),
            include_str!("card_def.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("trigger.rs"),
            include_str!("creature_death_capture_v1.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("linked_aura_return_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("standard_statics_v1.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("../build.rs"),
            include_str!("bloodthirsty_conqueror_v49/custody.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("soul_shackled_zombie_v1.rs"),
            include_str!("progenitus_v1.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("effect/inspiration_resolution_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("snack_wide_life_v59.rs"),
            include_str!("snack_wide_life_v59/custody.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("midnight_snack_v1.rs"),
            include_str!("mana.rs"),
            include_str!("legend_rule_v1.rs"),
            include_str!("rl.rs"),
            include_str!("standard_cast_convoke.rs"),
            include_str!("engine/mandatory_cast_cost_v1.rs"),
            include_str!("rules_meaning_next_five_v59.rs"),
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

pub fn selector() -> SelectorV50 {
    SelectorV50 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    }
}

fn death_instructions() -> Vec<InstructionV50> {
    vec![
        InstructionV50::ReturnOriginalCard {
            tapped: true,
            controller: BeneficiaryV50::OriginalCardOwner,
        },
        InstructionV50::CreateTreasure {
            controller: BeneficiaryV50::CreatureControllerAtDeath,
        },
    ]
}

pub fn printed_meaning(card_id: u16, root: RootV50) -> Result<MeaningV50, String> {
    if !crate::fake_your_own_death_v50::spell_custody::selected_definition(card_id) {
        return Err("Fake Your Own Death exact executable printing absent or changed".into());
    }
    let definition = &crate::card_def::CARD_DEFS[usize::from(card_id)];
    let spell = root == RootV50::InstantSpell;
    let ordered_instructions = if spell {
        vec![
            InstructionV50::BoostTargetUntilEndOfTurn {
                power: 2,
                toughness: 0,
            },
            InstructionV50::GrantDiesAbilityUntilEndOfTurn {
                instructions: death_instructions(),
            },
        ]
    } else {
        death_instructions()
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
            (definition.spell_effect)(),
            &ordered_instructions,
        ))
        .map_err(|error| error.to_string())?,
    );
    Ok(MeaningV50 {
        selector: selector(),
        card_id,
        capability: definition.capability,
        root,
        root_sha256,
        printed_cost: "{1}{B}".into(),
        mandatory_single_creature_target: spell,
        ordered_instructions,
        grant_bound_to_original_battlefield_incarnation: true,
        distinct_grants_create_distinct_death_triggers: true,
        return_bound_to_original_graveyard_incarnation: true,
        token_never_returns: true,
        treasure_independent_of_return_success: true,
        returning_aura_attachment_is_untargeted: true,
        returning_aura_host_chosen_by_card_owner: true,
        model_admitted: false,
    })
}

pub fn validate(value: &MeaningV50) -> Result<(), String> {
    if *value == printed_meaning(value.card_id, value.root)? {
        Ok(())
    } else {
        Err("Fake Your Own Death rooted ordered descriptor differs".into())
    }
}
