//! Selected printed attack meaning, independent of frozen vector/model meaning.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, PlayerRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use crate::trigger::TriggerCondition;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/frenzied-goblin/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
/// This is a printed triggered root, not an activated-ability/action ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    SourceAttacksMayPayRed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub trigger_index: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub source_zone: Zone,
    pub source_face_index: u8,
    pub source_requires_active_printed_abilities_at_declaration: bool,
    pub captured_source_contract_and_declaring_controller_required: bool,
    pub target_spec: TargetSpec,
    pub target_minimum: u8,
    pub target_maximum: u8,
    pub target_any_controller_and_any_power: bool,
    pub target_mandatory_at_placement_before_payment: bool,
    pub recheck_exact_target_incarnation_and_full_legality_at_resolution: bool,
    pub illegal_sole_target_fizzles_without_payment_choice: bool,
    pub payment_optional_at_resolution: bool,
    pub payment_colored: Vec<ManaColor>,
    pub payment_generic: u16,
    pub payer_is_captured_trigger_controller: bool,
    pub source_departure_reentry_or_later_control_change_does_not_change_payer: bool,
    pub source_survival_required_after_capture: bool,
    pub ability_suppression_after_capture_cancels_trigger: bool,
    pub only_successful_payment_installs_target_cannot_block_rule: bool,
    pub restriction_is_rule_change_not_removable_keyword: bool,
    pub restriction_expires_at_next_cleanup_including_repeated_cleanup: bool,
    pub recipient_control_or_ability_changes_end_restriction: bool,
    pub recipient_departure_reentry_does_not_transfer_restriction: bool,
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
    let mut h = Sha256::new();
    h.update(SCHEMA.as_bytes());
    for (path, source) in [
        (
            "meaning",
            include_str!("rules_meaning_frenzied_goblin_v1.rs"),
        ),
        ("attack_authority", include_str!("frenzied_goblin_v1.rs")),
        ("restriction", include_str!("sower_of_chaos_v1.rs")),
        ("trigger", include_str!("trigger.rs")),
        ("effect", include_str!("effect.rs")),
        ("engine", include_str!("engine.rs")),
        ("event", include_str!("event.rs")),
        ("state", include_str!("state.rs")),
        ("card_def", include_str!("card_def.rs")),
        ("build", include_str!("../build.rs")),
    ] {
        h.update([0]);
        h.update(path.as_bytes());
        h.update([0]);
        h.update(source.as_bytes());
    }
    hex_digest(h.finalize())
}
fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
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
pub fn meaning(
    selector: &SelectorV1,
    card_id: u16,
    trigger_index: u16,
) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Frenzied Goblin" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let triggers = crate::trigger::triggers_for(card_id);
    let trigger = triggers
        .get(usize::from(trigger_index))
        .ok_or(ErrorV1::WrongRoot)?;
    let program = (trigger.effect)();
    let expected = EffectOp::MayPayManaThen {
        player: PlayerRef::Controller,
        colored: vec![ManaColor::R],
        generic: 0,
        then: Box::new(EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 }),
    };
    if def.cost
        != (Cost {
            generic: 0,
            pips: &[Pip::Colored(ManaColor::R)],
            x_count: 0,
        })
        || def.types != [CardType::Creature]
        || def.colors != [ManaColor::R]
        || def.subtypes != [Subtype::Goblin, Subtype::Berserker]
        || (def.mana_value, def.power, def.toughness) != (1, Some(1), Some(1))
        || def.keywords != Keywords::NONE
        || !def.activated_abilities.is_empty()
        || triggers.len() != 1
        || trigger_index != 0
        || trigger.condition != TriggerCondition::Attacks
        || trigger.home_zone != Zone::Battlefield
        || trigger.face_index != 0
        || trigger.intervening_if_kicked
        || trigger.intervening_if_controls_another_source_card
        || program != expected
        || crate::trigger::target_spec_for_trigger(card_id, &program) != Some(TargetSpec::Creature)
    {
        return Err(ErrorV1::WrongRoot);
    }
    let root_bytes = serde_json::to_vec(&(
        card_id,
        trigger_index,
        RootV1::SourceAttacksMayPayRed,
        def.cost,
        def.types,
        def.colors,
        def.subtypes,
        def.power,
        def.toughness,
        trigger.home_zone,
        trigger.face_index,
        trigger.intervening_if_kicked,
        trigger.intervening_if_controls_another_source_card,
        TargetSpec::Creature,
        program,
    ))
    .expect("printed trigger root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        trigger_index,
        root: RootV1::SourceAttacksMayPayRed,
        root_sha256: hex_digest(Sha256::digest(root_bytes)),
        source_zone: Zone::Battlefield,
        source_face_index: 0,
        source_requires_active_printed_abilities_at_declaration: true,
        captured_source_contract_and_declaring_controller_required: true,
        target_spec: TargetSpec::Creature,
        target_minimum: 1,
        target_maximum: 1,
        target_any_controller_and_any_power: true,
        target_mandatory_at_placement_before_payment: true,
        recheck_exact_target_incarnation_and_full_legality_at_resolution: true,
        illegal_sole_target_fizzles_without_payment_choice: true,
        payment_optional_at_resolution: true,
        payment_colored: vec![ManaColor::R],
        payment_generic: 0,
        payer_is_captured_trigger_controller: true,
        source_departure_reentry_or_later_control_change_does_not_change_payer: true,
        source_survival_required_after_capture: false,
        ability_suppression_after_capture_cancels_trigger: false,
        only_successful_payment_installs_target_cannot_block_rule: true,
        restriction_is_rule_change_not_removable_keyword: true,
        restriction_expires_at_next_cleanup_including_repeated_cleanup: true,
        recipient_control_or_ability_changes_end_restriction: false,
        recipient_departure_reentry_does_not_transfer_restriction: true,
    })
}
/// Reconstruct all fields from the live root. This grants no action authority.
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id, record.trigger_index)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_and_wrong_definition_refuse_before_root_lookup() {
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
            assert_eq!(meaning(&forged, u16::MAX, 0), Err(ErrorV1::WrongSelector));
        }
        let elf = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        assert_eq!(
            meaning(&selector, elf, 0),
            Err(ErrorV1::UnsupportedDefinition)
        );
        let mut unknown = serde_json::to_value(selector).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("authority".into(), true.into());
        assert!(serde_json::from_value::<SelectorV1>(unknown).is_err());
    }
}
