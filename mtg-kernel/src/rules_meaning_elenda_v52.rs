//! Ordered immutable rules of the genuine Partial Elenda printing.
//! No runtime object, source snapshot, historical input or model admission.
use crate::card_def::{
    CardCapability, CardDef, CardType, Keywords, SourceTypeHexproofDefV1,
    StartingLifeSelfBonusDefV1, Subtype, Supertype, TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "rules_meaning_elenda/v52";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV52 {
    CreatureSpellAndPrintedStatics,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepresentationV52 {
    ExactElendaV52,
    FrozenRulesVectorV1,
    FrozenModelFeaturesV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV52 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    UnsupportedRepresentation,
    ForgedMeaning,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV52 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
/// Public rule vocabulary. The frozen engine Supertype has no serde contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupertypeV52 {
    Basic,
    Snow,
    Legendary,
}
impl From<Supertype> for SupertypeV52 {
    fn from(value: Supertype) -> Self {
        match value {
            Supertype::Basic => Self::Basic,
            Supertype::Snow => Self::Snow,
            Supertype::Legendary => Self::Legendary,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedCreatureV52 {
    pub generic_mana: u8,
    pub colored_pips: Vec<ManaColor>,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<SupertypeV52>,
    pub colors: Vec<ManaColor>,
    pub power: i16,
    pub toughness: i16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HomeV52 {
    BattlefieldPrintedFrontFace,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvaluationV52 {
    CurrentControllerAndWorldOnEveryQuery,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombinationV52 {
    AddEverySatisfiedSelfBonus,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeywordV52 {
    Lifelink,
    Menace,
}
/// Relation of the targeting spell or ability controller to this creature's
/// current controller. The physical source card's owner is irrelevant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargeterV52 {
    OpponentOfCurrentController,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceTypeBasisV52 {
    /// Effective types of the original live source, the actual selected
    /// Adventure/Omen/Bestow spell form, a copy's retained selected form even
    /// after its parent leaves, or original characteristics after departure.
    /// This rule does not export or authenticate any individual source history.
    EffectiveSelectedFormOrOriginalSourceCharacteristics,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetCheckV52 {
    TargetSelection,
    ResolutionRevalidation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum LifeConditionV52 {
    CurrentControllerLifeAtLeastTheirStartingLifePlus { amount: u16 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SelfGrantV52 {
    ModifyPowerToughness { power: i16, toughness: i16 },
    GainKeyword(KeywordV52),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV52 {
    Keyword(KeywordV52),
    HexproofFromSourceType {
        targeter: TargeterV52,
        required_source_type: CardType,
        source_type_basis: SourceTypeBasisV52,
        ordered_checks: Vec<TargetCheckV52>,
    },
    ConditionalSelfBonus {
        condition: LifeConditionV52,
        ordered_grants: Vec<SelfGrantV52>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV52 {
    pub selector: SelectorV52,
    pub card_id: u16,
    pub capability: CardCapability,
    pub root: RootV52,
    pub root_sha256: String,
    pub printed: PrintedCreatureV52,
    pub home: HomeV52,
    pub active_printed_ability_required: bool,
    pub evaluation: EvaluationV52,
    pub supported_starting_life: i32,
    pub bonus_combination: CombinationV52,
    pub ordered_rules: Vec<RuleV52>,
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
            include_str!("rules_meaning_elenda_v52.rs"),
            include_str!("elenda_public_v52.rs"),
            include_str!("rl.rs"),
            include_str!("starting_life_statics_v1.rs"),
            include_str!("source_type_hexproof_v1.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("fixed_source_damage_v3.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("gorehorn_damage_v1.rs"),
            include_str!("faebloom_trick_v43.rs"),
            include_str!("standard_spell_copy.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("state.rs"),
            include_str!("card_def.rs"),
            include_str!("trigger.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("standard_statics_v1.rs"),
            include_str!("../build.rs"),
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

pub fn live_selector() -> Result<SelectorV52, ErrorV52> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV52::UnsupportedCatalog);
    }
    Ok(SelectorV52 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}

// Exact ordered definition descriptors, independent of any runtime life total.
fn ordered_statics(
    bonuses: &[StartingLifeSelfBonusDefV1],
    source_rule: Option<SourceTypeHexproofDefV1>,
    keywords: Keywords,
) -> Result<Vec<RuleV52>, ErrorV52> {
    let expected = [
        StartingLifeSelfBonusDefV1 {
            minimum_above_start: 1,
            power: 1,
            toughness: 1,
            keywords: Keywords::MENACE,
        },
        StartingLifeSelfBonusDefV1 {
            minimum_above_start: 10,
            power: 5,
            toughness: 5,
            keywords: Keywords::NONE,
        },
    ];
    if bonuses != expected
        || source_rule != Some(SourceTypeHexproofDefV1::Instants)
        || keywords != Keywords::LIFELINK
    {
        return Err(ErrorV52::UnsupportedDefinition);
    }
    let mut rules = vec![
        RuleV52::Keyword(KeywordV52::Lifelink),
        RuleV52::HexproofFromSourceType {
            targeter: TargeterV52::OpponentOfCurrentController,
            required_source_type: CardType::Instant,
            source_type_basis:
                SourceTypeBasisV52::EffectiveSelectedFormOrOriginalSourceCharacteristics,
            ordered_checks: vec![
                TargetCheckV52::TargetSelection,
                TargetCheckV52::ResolutionRevalidation,
            ],
        },
    ];
    for bonus in bonuses {
        let mut ordered_grants = vec![SelfGrantV52::ModifyPowerToughness {
            power: bonus.power,
            toughness: bonus.toughness,
        }];
        if bonus.keywords.has(Keywords::MENACE) {
            ordered_grants.push(SelfGrantV52::GainKeyword(KeywordV52::Menace));
        }
        rules.push(RuleV52::ConditionalSelfBonus {
            condition: LifeConditionV52::CurrentControllerLifeAtLeastTheirStartingLifePlus {
                amount: bonus.minimum_above_start,
            },
            ordered_grants,
        });
    }
    Ok(rules)
}

fn selected_definition(d: &CardDef, card_id: u16) -> bool {
    d.name == "Elenda, Saint of Dusk"
        && d.object_name == d.name
        && matches!(d.capability, CardCapability::Partial | CardCapability::Full)
        && d.is_executable()
        && d.cost.generic == 2
        && d.cost.x_count == 0
        && d.cost.pips == [Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::B)]
        && d.mana_value == 4
        && d.colors == [ManaColor::W, ManaColor::B]
        && d.types == [CardType::Creature]
        && d.subtypes == [Subtype::Vampire, Subtype::Knight]
        && d.supertypes == [Supertype::Legendary]
        && d.power == Some(4)
        && d.toughness == Some(4)
        && !d.is_land
        && !d.is_token
        && d.target_spec == TargetSpec::None
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        && crate::trigger::triggers_for(card_id).is_empty()
        && d.activated_abilities.is_empty()
        && d.ward_cost.is_none()
        && d.alt_cost.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.flashback.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.optional_additional_cost.is_none()
        && d.equipment.is_none()
        && d.attachment.is_none()
        && d.produces_mana.is_empty()
        && d.mana_ability_choices.is_empty()
        && (d.mana_ability)().is_none()
        && d.starting_loyalty.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_creature_grant.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.generic_cost_reduction.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.kicked_target_spec.is_none()
        && d.escape.is_none()
        && !d.enters_battlefield_tapped
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.enters_battlefield_tapped_unless.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && !d.changeling
        && d.bestow.is_none()
        && !d.delve
        && d.adventure.is_none()
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.additional_mana_ability_conditions.is_empty()
        && d.restricted_mana_abilities.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.token_counter_replacement_v48.is_none()
}

pub fn meaning(
    selector: &SelectorV52,
    card_id: u16,
    root: RootV52,
) -> Result<MeaningV52, ErrorV52> {
    if selector != &live_selector()? {
        return Err(ErrorV52::WrongSelector);
    }
    let definition = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV52::UnsupportedDefinition)?;
    if !selected_definition(definition, card_id) {
        return Err(ErrorV52::UnsupportedDefinition);
    }
    let ordered_rules = ordered_statics(
        definition.starting_life_self_bonuses_v1,
        definition.source_type_hexproof_v1,
        definition.keywords,
    )?;
    let printed = PrintedCreatureV52 {
        generic_mana: definition.cost.generic,
        colored_pips: vec![ManaColor::W, ManaColor::B],
        types: definition.types.to_vec(),
        subtypes: definition.subtypes.to_vec(),
        supertypes: definition
            .supertypes
            .iter()
            .copied()
            .map(SupertypeV52::from)
            .collect(),
        colors: definition.colors.to_vec(),
        power: definition.power.unwrap(),
        toughness: definition.toughness.unwrap(),
    };
    let home = HomeV52::BattlefieldPrintedFrontFace;
    let evaluation = EvaluationV52::CurrentControllerAndWorldOnEveryQuery;
    let bonus_combination = CombinationV52::AddEverySatisfiedSelfBonus;
    let supported_starting_life = crate::state::STARTING_LIFE;
    let root_sha256 = digest(
        &serde_json::to_vec(&(
            SCHEMA,
            card_id,
            root,
            definition.capability,
            &printed,
            home,
            true,
            evaluation,
            supported_starting_life,
            bonus_combination,
            &ordered_rules,
            (definition.spell_effect)(),
        ))
        .map_err(|_| ErrorV52::UnsupportedDefinition)?,
    );
    Ok(MeaningV52 {
        selector: selector.clone(),
        card_id,
        capability: definition.capability,
        root,
        root_sha256,
        printed,
        home,
        active_printed_ability_required: true,
        evaluation,
        supported_starting_life,
        bonus_combination,
        ordered_rules,
        model_admitted: false,
    })
}

pub fn printed_meaning(card_id: u16, root: RootV52) -> Result<MeaningV52, ErrorV52> {
    meaning(&live_selector()?, card_id, root)
}

pub fn meaning_for_representation(
    representation: RepresentationV52,
    selector: &SelectorV52,
    card_id: u16,
    root: RootV52,
) -> Result<MeaningV52, ErrorV52> {
    if representation != RepresentationV52::ExactElendaV52 {
        return Err(ErrorV52::UnsupportedRepresentation);
    }
    meaning(selector, card_id, root)
}

/// Authenticate the entire immutable record against the current exact printing.
pub fn validate(value: &MeaningV52) -> Result<(), ErrorV52> {
    if *value == meaning(&value.selector, value.card_id, value.root)? {
        Ok(())
    } else {
        Err(ErrorV52::ForgedMeaning)
    }
}

#[cfg(test)]
#[path = "rules_meaning_elenda_v52_tests.rs"]
mod tests;
