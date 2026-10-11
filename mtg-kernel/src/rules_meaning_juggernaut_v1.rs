//! Precise selected printed statics, separate from frozen vector/model meanings.
use crate::card_def::{CardType, CombatStaticRulesDef, Keywords, Subtype, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::Cost;
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/juggernaut/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    MustAttackEachCombat,
    CannotBeBlockedByWalls,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub current_battlefield_front_face_creature_printed_abilities_required: bool,
    pub uses_current_controller_not_owner_or_original_caster: bool,
    pub must_attack_each_combat_if_able: bool,
    pub sickness_tapped_defender_and_other_attack_restrictions_apply: bool,
    pub imposes_no_goad_defender_requirement: bool,
    pub cannot_be_blocked_by_effective_wall_creatures: bool,
    pub changeling_and_subtype_replacement_are_effective: bool,
    pub ability_suppression_removes_both_rules: bool,
    pub attack_and_block_validation_and_candidates_agree: bool,
    pub no_target_or_payment_or_saved_duration: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub model_and_sampler_admitted: bool,
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
        include_str!("rules_meaning_juggernaut_v1.rs"),
        include_str!("combat_static_rules_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
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
    if def.name != "Juggernaut" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let descriptor = CombatStaticRulesDef {
        must_attack_each_combat: true,
        cant_be_blocked_by_subtype: Some(Subtype::Wall),
    };
    let program = Some(EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    });
    if def.cost
        != (Cost {
            generic: 4,
            pips: &[],
            x_count: 0,
        })
        || def.mana_value != 4
        || def.types != [CardType::Artifact, CardType::Creature]
        || def.subtypes != [Subtype::Juggernaut]
        || !def.colors.is_empty()
        || (def.power, def.toughness) != (Some(5), Some(3))
        || def.keywords != Keywords::NONE
        || def.combat_static_rules != descriptor
        || (def.spell_effect)() != program
        || !def.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
        || def.is_token
        || def.is_land
        || (def.mana_ability)().is_some()
        || def.mana_ability_def.is_some()
        || !def.additional_mana_abilities.is_empty()
        || def.adventure.is_some()
        || def.flashback.is_some()
        || def.target_spec != crate::card_def::TargetSpec::None
    {
        return Err(ErrorV1::WrongRoot);
    }
    let rule = RuleV1 {
        current_battlefield_front_face_creature_printed_abilities_required: true,
        uses_current_controller_not_owner_or_original_caster: true,
        must_attack_each_combat_if_able: root == RootV1::MustAttackEachCombat,
        sickness_tapped_defender_and_other_attack_restrictions_apply: true,
        imposes_no_goad_defender_requirement: true,
        cannot_be_blocked_by_effective_wall_creatures: root == RootV1::CannotBeBlockedByWalls,
        changeling_and_subtype_replacement_are_effective: true,
        ability_suppression_removes_both_rules: true,
        attack_and_block_validation_and_candidates_agree: true,
        no_target_or_payment_or_saved_duration: true,
    };
    let bytes =
        serde_json::to_vec(&(SCHEMA, card_id, root, &rule)).expect("printed facts serialize");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        root,
        root_sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        rule,
        model_and_sampler_admitted: false,
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
    fn frozen_catalog_and_forged_selector_refuse_before_definition_lookup() {
        let Ok(selector) = live_selector() else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        for field in 0..3 {
            let mut forged = selector.clone();
            match field {
                0 => forged.schema.push('x'),
                1 => forged.card_db_hash ^= 1,
                _ => forged.producer_sha256.push('x'),
            }
            assert_eq!(
                meaning(&forged, u16::MAX, RootV1::MustAttackEachCombat),
                Err(ErrorV1::WrongSelector)
            );
        }
        assert_eq!(
            meaning(
                &selector,
                crate::card_def::card_id_by_name("Lightning Bolt").unwrap(),
                RootV1::CannotBeBlockedByWalls
            ),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
}
