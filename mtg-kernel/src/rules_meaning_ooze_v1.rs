//! Scoped target/effect companion for definition-owned Scavenging Ooze roots.
//! This is neither a RulesVector vocabulary nor a model admission.
use crate::card_def::{self, ActivationTargetFilter, CostComponent, TargetSpec};
use crate::effect::EffectOp;
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/scavenging-ooze/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MandatoryGraveyardTargetV1 {
    pub count: u8,
    pub either_players_graveyard: bool,
    pub card_types_unrestricted: bool,
    pub excludes_tokens_and_spell_copies: bool,
    pub exact_selected_incarnation: bool,
    pub legality_rechecked_before_any_instruction: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreMoveCreatureConditionalV1 {
    pub remember_creature_type_before_move: bool,
    pub propose_exile_through_replacement_pipeline: bool,
    pub conditional_requires_successful_exile: bool,
    pub counter_amount: u8,
    pub counter_source_exact_battlefield_incarnation: bool,
    pub counter_does_not_require_current_creature_type: bool,
    pub counter_does_not_require_current_control_or_definition: bool,
    pub life_gain_amount: u8,
    pub life_gain_recipient_captured_ability_controller: bool,
    pub life_gain_independent_of_source_presence: bool,
    pub illegal_target_aborts_all_instructions: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OozeRootMeaningV1 {
    pub schema: String,
    pub catalog_hash: u64,
    pub source_sha256: String,
    pub card_id: u16,
    pub ability_index: u32,
    pub target: MandatoryGraveyardTargetV1,
    pub effect: PreMoveCreatureConditionalV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningErrorV1 {
    UnsupportedRoot,
    IdentityMismatch,
}

pub fn source_sha256() -> String {
    let mut digest = Sha256::new();
    for (path, source) in [
        (
            "rules_meaning_ooze_v1.rs",
            include_str!("rules_meaning_ooze_v1.rs"),
        ),
        (
            "effect/scavenging_ooze_v1.rs",
            include_str!("effect/scavenging_ooze_v1.rs"),
        ),
        ("build.rs", include_str!("../build.rs")),
        ("card_def.rs", include_str!("card_def.rs")),
        ("engine.rs", include_str!("engine.rs")),
        ("state.rs", include_str!("state.rs")),
    ] {
        digest.update(path.as_bytes());
        digest.update([0]);
        digest.update(source.as_bytes());
        digest.update([0]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
/// Authenticates the actual selected definition root, never a caller's opcode.
pub fn root_meaning(card_id: u16, ability_index: u32) -> Result<OozeRootMeaningV1, MeaningErrorV1> {
    let def = card_def::CARD_DEFS
        .get(card_id as usize)
        .ok_or(MeaningErrorV1::UnsupportedRoot)?;
    let ability = def
        .activated_abilities
        .get(ability_index as usize)
        .ok_or(MeaningErrorV1::UnsupportedRoot)?;
    if !def.is_executable()
        || def.name != "Scavenging Ooze"
        || ability.target_spec != TargetSpec::CardInGraveyards
        || ability.activation_zone != Zone::Battlefield
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.sorcery_speed_only
        || ability.max_activations_per_turn.is_some()
        || (ability.effect)() != EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature
        || !matches!(ability.cost, [CostComponent::Mana(cost)]
            if cost.generic == 0 && cost.x_count == 0 && cost.pips == [Pip::Colored(ManaColor::G)])
    {
        return Err(MeaningErrorV1::UnsupportedRoot);
    }
    Ok(OozeRootMeaningV1 {
        schema: SCHEMA.into(),
        catalog_hash: card_def::KERNEL_CARDDB_HASH,
        source_sha256: source_sha256(),
        card_id,
        ability_index,
        target: MandatoryGraveyardTargetV1 {
            count: 1,
            either_players_graveyard: true,
            card_types_unrestricted: true,
            excludes_tokens_and_spell_copies: true,
            exact_selected_incarnation: true,
            legality_rechecked_before_any_instruction: true,
        },
        effect: PreMoveCreatureConditionalV1 {
            remember_creature_type_before_move: true,
            propose_exile_through_replacement_pipeline: true,
            conditional_requires_successful_exile: false,
            counter_amount: 1,
            counter_source_exact_battlefield_incarnation: true,
            counter_does_not_require_current_creature_type: true,
            counter_does_not_require_current_control_or_definition: true,
            life_gain_amount: 1,
            life_gain_recipient_captured_ability_controller: true,
            life_gain_independent_of_source_presence: true,
            illegal_target_aborts_all_instructions: true,
        },
    })
}
pub fn validate(record: &OozeRootMeaningV1) -> Result<(), MeaningErrorV1> {
    if *record != root_meaning(record.card_id, record.ability_index)? {
        return Err(MeaningErrorV1::IdentityMismatch);
    }
    Ok(())
}
/// Catalog program metadata is public and does not inspect hidden inventories.
pub fn catalog_roots() -> Vec<OozeRootMeaningV1> {
    card_def::CARD_DEFS
        .iter()
        .enumerate()
        .flat_map(|(id, def)| {
            def.activated_abilities
                .iter()
                .enumerate()
                .filter_map(move |(index, _)| root_meaning(id as u16, index as u32).ok())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_unrelated_root_cannot_supply_ooze_authority() {
        let elves = card_def::card_id_by_name("Llanowar Elves").unwrap();
        assert_eq!(root_meaning(elves, 0), Err(MeaningErrorV1::UnsupportedRoot));
        assert_eq!(
            root_meaning(u16::MAX, 0),
            Err(MeaningErrorV1::UnsupportedRoot)
        );
        assert_eq!(source_sha256().len(), 64);
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn future_registered_ooze_root_authenticates_all_meaning_and_identity_fields() {
        let id = card_def::card_id_by_name("Scavenging Ooze")
            .expect("future serial registration prerequisite");
        let record = root_meaning(id, 0).unwrap();
        assert_eq!(record.target.count, 1);
        assert!(!record.effect.conditional_requires_successful_exile);
        assert!(record.effect.life_gain_independent_of_source_presence);
        let decoded: OozeRootMeaningV1 =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        validate(&decoded).unwrap();
        for field in 0..7 {
            let mut forged = record.clone();
            match field {
                0 => forged.schema.push('x'),
                1 => forged.catalog_hash ^= 1,
                2 => forged.source_sha256.push('x'),
                3 => forged.target.count = 0,
                4 => forged.effect.conditional_requires_successful_exile = true,
                5 => forged.effect.life_gain_independent_of_source_presence = false,
                _ => forged.ability_index = 1,
            }
            assert!(validate(&forged).is_err());
        }
        let before = crate::rules_vector_v1::extractor_sha256();
        root_meaning(id, 0).unwrap();
        assert_eq!(before, crate::rules_vector_v1::extractor_sha256());
    }
}
