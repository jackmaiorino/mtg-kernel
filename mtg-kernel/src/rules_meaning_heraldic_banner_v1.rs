//! Exact definition-owned Banner record, separate from frozen vector readers.
use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef, PlayerRef};
use crate::mana::ManaColor;
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/heraldic-banner/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    PermanentSpell,
    AsEntersColor,
    ChosenColorAnthem,
    ChosenColorMana,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub printed_cost: String,
    pub choices: [ManaColor; 5],
    pub choice_before_entry: bool,
    pub all_batch_choices_before_any_entry: bool,
    pub fresh_incarnation_needs_fresh_choice: bool,
    pub current_source_controller: bool,
    pub current_effective_creature_and_color: bool,
    pub power_delta: i16,
    pub toughness_delta: i16,
    pub requires_active_printed_ability: bool,
    pub mana_cost_is_tap_self: bool,
    pub mana_count: u8,
    pub resolves_without_stack: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub native_model_sampler_admitted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    WrongRoot,
    ForgedMeaning,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_heraldic_banner_v1.rs"),
        include_str!("heraldic_banner_v1.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("state.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
    ] {
        h.update([0]);
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
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
        producer_sha256: producer_sha256_v1(),
    })
}
pub fn meaning(selector: &SelectorV1, card_id: u16, root: RootV1) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let d = CARD_DEFS.get(card_id as usize).ok_or(ErrorV1::WrongRoot)?;
    let permanent = EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    };
    if !crate::heraldic_banner_v1::owns_definition(card_id)
        || d.name != "Heraldic Banner"
        || d.types != [CardType::Artifact]
        || !d.colors.is_empty()
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.cost.generic != 3
        || !d.cost.pips.is_empty()
        || d.cost.x_count != 0
        || d.mana_value != 3
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)() != Some(permanent.clone())
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.adventure.is_some()
        || d.omen.is_some()
        || d.mana_ability_def.is_some()
        || !d.mana_ability_choices.is_empty()
        || !d.additional_mana_abilities.is_empty()
        || !d.mana_ability_includes_chosen_color
        || d.conditional_tap_yield.is_some()
        || d.as_enters_choose_color_other_than.is_some()
    {
        return Err(ErrorV1::WrongRoot);
    }
    let mana = crate::heraldic_banner_v1::COLORS
        .into_iter()
        .map(|color| {
            let expected = EffectOp::Sequence(vec![
                EffectOp::TapObject {
                    object: ObjectRef::ThisSource,
                },
                EffectOp::AddMana {
                    player: PlayerRef::Controller,
                    colors: vec![color],
                },
            ]);
            if d.primary_mana_ability_choices(Some(color)) != [color]
                || d.mana_ability_program_for(color, Some(color)) != Some(expected.clone())
            {
                return Err(ErrorV1::WrongRoot);
            }
            Ok(expected)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let supertype_names = d
        .supertypes
        .iter()
        .map(|s| match s {
            crate::card_def::Supertype::Basic => "Basic",
            crate::card_def::Supertype::Snow => "Snow",
            crate::card_def::Supertype::Legendary => "Legendary",
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&(
        card_id,
        root,
        d.name,
        d.cost,
        d.types,
        d.colors,
        supertype_names,
        d.subtypes,
        d.keywords,
        d.as_enters_color_choice_v1,
        d.chosen_color_creature_boost_v1,
        permanent,
        mana,
    ))
    .map_err(|_| ErrorV1::WrongRoot)?;
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        root,
        root_sha256: digest(&bytes),
        rule: RuleV1 {
            printed_cost: "{3}".into(),
            choices: crate::heraldic_banner_v1::COLORS,
            choice_before_entry: true,
            all_batch_choices_before_any_entry: true,
            fresh_incarnation_needs_fresh_choice: true,
            current_source_controller: true,
            current_effective_creature_and_color: true,
            power_delta: 1,
            toughness_delta: 0,
            requires_active_printed_ability: true,
            mana_cost_is_tap_self: true,
            mana_count: 1,
            resolves_without_stack: true,
        },
        native_model_sampler_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if meaning(&record.selector, record.card_id, record.root)? != *record {
        return Err(ErrorV1::ForgedMeaning);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn predecessor_roots_and_foreign_selector_refuse_readonly() {
        let Some(id) = crate::card_def::card_id_by_name("Forest") else {
            panic!("registered Forest")
        };
        if let Ok(selector) = live_selector() {
            assert_eq!(
                meaning(&selector, id, RootV1::AsEntersColor),
                Err(ErrorV1::WrongRoot)
            );
            let mut wrong = selector;
            wrong.card_db_hash ^= 1;
            assert_eq!(
                meaning(&wrong, id, RootV1::AsEntersColor),
                Err(ErrorV1::WrongSelector)
            );
        } else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
        }
    }
    #[test]
    #[ignore = "requires genuine Heraldic Banner registration"]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_all_four_roots_reconstruct_and_refuse_complete_record_forgeries() {
        let selector = live_selector().unwrap();
        let id = crate::card_def::card_id_by_name("Heraldic Banner").unwrap();
        for root in [
            RootV1::PermanentSpell,
            RootV1::AsEntersColor,
            RootV1::ChosenColorAnthem,
            RootV1::ChosenColorMana,
        ] {
            let record = meaning(&selector, id, root).unwrap();
            validate(&record).unwrap();
            for mutation in 0..5 {
                let mut bad = record.clone();
                match mutation {
                    0 => bad.rule.choice_before_entry = false,
                    1 => bad.rule.choices[0] = ManaColor::C,
                    2 => bad.rule.current_source_controller = false,
                    3 => bad.rule.mana_count = 2,
                    _ => bad.root_sha256.clear(),
                }
                assert_eq!(validate(&bad), Err(ErrorV1::ForgedMeaning));
            }
            let mut json = serde_json::to_value(record).unwrap();
            json["unknown"] = serde_json::json!(true);
            assert!(serde_json::from_value::<MeaningV1>(json).is_err());
        }
    }
}
