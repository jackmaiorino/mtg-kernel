//! Standalone immutable ordered Phoenix rules. No backend or model admission.
use crate::{
    card_def::{CardCapability, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS},
    effect::{EffectCond, EffectOp, ObjectRef, PlayerRef},
    mana::{ManaColor, Pip},
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "rules_meaning_flamewake_phoenix/v56";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepresentationV56 {
    ExactPhoenixV56,
    FrozenRulesVectorV1,
    FrozenModelFeaturesV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV56 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV56 {
    pub generic_mana: u8,
    pub colored_pips: Vec<ManaColor>,
    pub mana_value: u16,
    pub colors: Vec<ManaColor>,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub power: i16,
    pub toughness: i16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryV56 {
    UntappedByDefaultSubjectToEntryReplacements,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeywordV56 {
    Flying,
    Haste,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorV56 {
    OriginalGraveyardOwnerAndController,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventV56 {
    BeginningOfCombatOnYourTurn,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckV56 {
    TriggerCollection,
    ResolutionBeforeOptionalPayment,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FerociousV56 {
    pub controller: ActorV56,
    pub minimum_effective_creature_power: i32,
    pub checks: Vec<CheckV56>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReturnStepV56 {
    OfferOptionalRedMana,
    OnAcceptancePayRedMana,
    AfterPaymentCheckOriginalGraveyardIncarnation,
    IfStillPresentReturnToBattlefieldUnderOriginalOwner,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV56 {
    CreatureSpellEntersBattlefield,
    Keyword(KeywordV56),
    AttacksEachCombatIfAble,
    GraveyardReturn {
        home: Zone,
        event: EventV56,
        actor: ActorV56,
        intervening_if: FerociousV56,
        optional_colored_cost: Vec<ManaColor>,
        optional_generic_cost: u8,
        untargeted: bool,
        active_printed_ability_required_at_collection: bool,
        recheck_ferocious_after_payment_offer: bool,
        payment_allowed_after_original_source_departure: bool,
        return_failure_refunds_payment: bool,
        ordered_steps: Vec<ReturnStepV56>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV56 {
    pub selector: SelectorV56,
    pub card_id: u16,
    pub capability: CardCapability,
    pub printed: PrintedV56,
    pub battlefield_entry: EntryV56,
    pub ordered_rules: Vec<RuleV56>,
    pub full_engine_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
    pub fair_search_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("rules_meaning_phoenix_v56.rs"),
            include_str!("phoenix_public_v56.rs"),
            include_str!("card_def.rs"),
            include_str!("trigger.rs"),
            include_str!("effect.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("state.rs"),
            include_str!("mana.rs"),
            include_str!("self_reflection_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("combat_static_rules_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("../build.rs"),
            include_str!("../../data/limited/fdn_v1/cards_v1.json"),
            include_str!("bloodthirsty_conqueror_v49/custody.rs"),
        ] {
            h.update((source.len() as u64).to_le_bytes());
            h.update(source.as_bytes());
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub fn live_selector() -> Result<SelectorV56, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Phoenix V56 requires the genuine pure Limited catalog".into());
    }
    Ok(SelectorV56 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
// Executable matching stays private; no executable operands enter the DTO.
pub(crate) fn return_program() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::ControlsCreaturePowerAtLeast(4),
        then: Box::new(EffectOp::MayPayManaThen {
            player: PlayerRef::Controller,
            colored: vec![ManaColor::R],
            generic: 0,
            then: Box::new(EffectOp::Conditional {
                cond: EffectCond::SourceStillInTriggerZone,
                then: Box::new(EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                }),
                else_: Box::new(EffectOp::Sequence(vec![])),
            }),
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}
fn selected_trigger(card_id: u16) -> bool {
    let t = crate::trigger::triggers_for(card_id);
    t.len() == 1
        && t[0].home_zone == Zone::Graveyard
        && t[0].condition
            == crate::trigger::TriggerCondition::BeginningControllerCombatWithCreaturePowerAtLeast(
                4,
            )
        && t[0].face_index == 0
        && !t[0].intervening_if_kicked
        && !t[0].intervening_if_controls_another_source_card
        && (t[0].effect)() == return_program()
        && crate::trigger::trigger_target_spec(card_id) == TargetSpec::None
}
pub(crate) fn selected_definition(d: &CardDef, card_id: u16) -> bool {
    // Exhaustive coverage requires a deliberate decision for every new field.
    let CardDef {
        name: _,
        capability: _,
        cost: _,
        generic_cost_reduction: _,
        ward_cost: _,
        spell_cannot_be_countered: _,
        records_mana_spent: _,
        equipment: _,
        types: _,
        subtypes: _,
        supertypes: _,
        power: _,
        toughness: _,
        starting_loyalty: _,
        enters_with_plus_one_counters: _,
        controlled_counter_keyword: _,
        game_outcome_protection_v1: _,
        controlled_creature_grant: _,
        is_land: _,
        produces_mana: _,
        colors: _,
        target_spec: _,
        keywords: _,
        spell_effect: _,
        mana_ability: _,
        alt_cost: _,
        kicker_cost: _,
        additional_cost: _,
        flashback: _,
        activated_abilities: _,
        plot_cost: _,
        madness_cost: _,
        mode2: _,
        mode3: _,
        additional_modes: _,
        kicked_target_spec: _,
        is_token: _,
        escape: _,
        mana_ability_choices: _,
        enters_battlefield_tapped: _,
        mana_ability_def: _,
        minimum_blockers: _,
        omen: _,
        mana_value: _,
        mana_ability_includes_chosen_color: _,
        as_enters_choose_color_other_than: _,
        additional_mana_abilities: _,
        object_name: _,
        enters_battlefield_tapped_unless: _,
        attachment: _,
        transform_face: _,
        saga: _,
        optional_additional_cost: _,
        changeling: _,
        bestow: _,
        delve: _,
        adventure: _,
        cant_be_blocked_by_monarchs_creatures: _,
        conditional_tap_yield: _,
        enters_tapped_unless_controller: _,
        additional_mana_ability_conditions: _,
        restricted_mana_abilities: _,
        animation: _,
        activated_ability_generic_reductions: _,
        combat_static_rules: _,
        as_enters_color_choice_v1: _,
        chosen_color_creature_boost_v1: _,
        damage_replacement_v1: _,
        token_counter_replacement_v48: _,
        starting_life_self_bonuses_v1: _,
        source_type_hexproof_v1: _,
        other_controlled_nontoken_death_v1: _,
    } = d;
    d.name == "Flamewake Phoenix"
        && d.object_name == d.name
        && matches!(d.capability, CardCapability::Partial | CardCapability::Full)
        && d.is_executable()
        && d.cost.generic == 1
        && d.cost.x_count == 0
        && d.cost.pips == [Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)]
        && d.mana_value == 3
        && d.colors == [ManaColor::R]
        && d.types == [CardType::Creature]
        && d.subtypes == [Subtype::Phoenix]
        && d.supertypes.is_empty()
        && d.power == Some(2)
        && d.toughness == Some(2)
        && !d.is_land
        && !d.is_token
        && d.target_spec == TargetSpec::None
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        && selected_trigger(card_id)
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
        && d.combat_static_rules
            == crate::card_def::CombatStaticRulesDef {
                must_attack_each_combat: true,
                cant_be_blocked_by_subtype: None,
            }
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.token_counter_replacement_v48.is_none()
        && d.keywords == Keywords(Keywords::FLYING.0 | Keywords::HASTE.0)
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.other_controlled_nontoken_death_v1.is_none()
}

pub fn meaning(selector: &SelectorV56, card_id: u16) -> Result<MeaningV56, String> {
    if selector != &live_selector()? {
        return Err("Phoenix V56 selector differs".into());
    }
    let d = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or("Phoenix definition absent")?;
    if !selected_definition(d, card_id) {
        return Err("Phoenix exact printing/ordered recipe differs".into());
    }
    Ok(MeaningV56 {
        selector: selector.clone(),
        card_id,
        capability: d.capability,
        printed: PrintedV56 {
            generic_mana: d.cost.generic,
            colored_pips: vec![ManaColor::R, ManaColor::R],
            mana_value: d.mana_value,
            colors: d.colors.to_vec(),
            types: d.types.to_vec(),
            subtypes: d.subtypes.to_vec(),
            power: 2,
            toughness: 2,
        },
        battlefield_entry: EntryV56::UntappedByDefaultSubjectToEntryReplacements,
        ordered_rules: vec![
            RuleV56::CreatureSpellEntersBattlefield,
            RuleV56::Keyword(KeywordV56::Flying),
            RuleV56::Keyword(KeywordV56::Haste),
            RuleV56::AttacksEachCombatIfAble,
            RuleV56::GraveyardReturn {
                home: Zone::Graveyard,
                event: EventV56::BeginningOfCombatOnYourTurn,
                actor: ActorV56::OriginalGraveyardOwnerAndController,
                intervening_if: FerociousV56 {
                    controller: ActorV56::OriginalGraveyardOwnerAndController,
                    minimum_effective_creature_power: 4,
                    checks: vec![
                        CheckV56::TriggerCollection,
                        CheckV56::ResolutionBeforeOptionalPayment,
                    ],
                },
                optional_colored_cost: vec![ManaColor::R],
                optional_generic_cost: 0,
                untargeted: true,
                active_printed_ability_required_at_collection: true,
                recheck_ferocious_after_payment_offer: false,
                payment_allowed_after_original_source_departure: true,
                return_failure_refunds_payment: false,
                ordered_steps: vec![
                    ReturnStepV56::OfferOptionalRedMana,
                    ReturnStepV56::OnAcceptancePayRedMana,
                    ReturnStepV56::AfterPaymentCheckOriginalGraveyardIncarnation,
                    ReturnStepV56::IfStillPresentReturnToBattlefieldUnderOriginalOwner,
                ],
            },
        ],
        full_engine_admitted: d.capability == CardCapability::Full,
        model_admitted: false,
        sampler_admitted: false,
        fair_search_admitted: false,
    })
}
pub fn printed_meaning(card_id: u16) -> Result<MeaningV56, String> {
    meaning(&live_selector()?, card_id)
}
pub fn meaning_for_representation(
    representation: RepresentationV56,
    selector: &SelectorV56,
    card_id: u16,
) -> Result<MeaningV56, String> {
    if representation != RepresentationV56::ExactPhoenixV56 {
        return Err("Phoenix requires its precise V56 representation".into());
    }
    meaning(selector, card_id)
}
pub fn validate(value: &MeaningV56) -> Result<(), String> {
    if *value == meaning(&value.selector, value.card_id)? {
        Ok(())
    } else {
        Err("Phoenix immutable ordered meaning differs".into())
    }
}
#[cfg(test)]
#[path = "rules_meaning_phoenix_v56_tests.rs"]
mod tests;
