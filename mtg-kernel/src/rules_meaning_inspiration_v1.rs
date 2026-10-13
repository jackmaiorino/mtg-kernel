//! Exact printed Inspiration roots, separate from the frozen coarse vocabulary.
use crate::card_def::{self, CardType, CostComponent, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectOp, ObjectSelectionActionV1, ObjectSelectionFilterV1, ObjectSelectionRuleV1, PlayerRef,
};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/inspiration-from-beyond/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastRootV1 {
    Normal,
    Flashback,
}

/// The printed minimum is one; the resolving choice is zero only when no card exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AvailabilityBoundsV1 {
    OneWhenAvailableElseZero,
}

/// This opt-in concerns exact meaning, not admission of an observation or action format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningRepresentationV1 {
    ExactInspirationV1,
    FrozenRulesVectorV1,
    FrozenModelFeaturesV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    UnsupportedRepresentation,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    ForgedMeaning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub printed_cast_cost: String,
    pub cast_from_own_graveyard: bool,
    pub flashback_exiles_when_leaving_stack: bool,
    pub ordinary_sorcery_timing_subject_to_cast_permission_effects: bool,
    pub mill_player: PlayerRef,
    pub mill_count: u8,
    pub mill_up_to_available_library_cards: bool,
    pub mill_order_precedes_return_choice: bool,
    pub milled_and_preexisting_graveyard_cards_are_available: bool,
    pub selection_rule: ObjectSelectionRuleV1,
    pub availability_bounds: AvailabilityBoundsV1,
    pub resolving_choice_is_not_a_target: bool,
    pub candidate_physical_owner_is_captured_spell_controller: bool,
    pub candidate_current_zone_and_exact_incarnation_are_authenticated: bool,
    pub spell_remains_on_stack_during_return_choice: bool,
    pub empty_library_does_not_skip_existing_graveyard_choice: bool,
    pub no_priority_between_mill_and_return_choice: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub cast_root: CastRootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub native_model_sampler_admitted: bool,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for (path, source) in [
        (
            "rules_meaning_inspiration_v1.rs",
            include_str!("rules_meaning_inspiration_v1.rs"),
        ),
        ("build.rs", include_str!("../build.rs")),
        ("card_def.rs", include_str!("card_def.rs")),
        ("trigger.rs", include_str!("trigger.rs")),
        ("effect.rs", include_str!("effect.rs")),
        (
            "effect/standard_selection_v1.rs",
            include_str!("effect/standard_selection_v1.rs"),
        ),
        (
            "effect/inspiration_resolution_v1.rs",
            include_str!("effect/inspiration_resolution_v1.rs"),
        ),
        (
            "effect/vampire_gourmand_resolution_v1.rs",
            include_str!("effect/vampire_gourmand_resolution_v1.rs"),
        ),
        ("engine.rs", include_str!("engine.rs")),
        ("event.rs", include_str!("event.rs")),
        ("state.rs", include_str!("state.rs")),
        ("spell_costs_v1.rs", include_str!("spell_costs_v1.rs")),
        (
            "continuous_characteristics_v1.rs",
            include_str!("continuous_characteristics_v1.rs"),
        ),
        ("standard_cards_v1.rs", include_str!("standard_cards_v1.rs")),
        (
            "standard_statics_v1.rs",
            include_str!("standard_statics_v1.rs"),
        ),
    ] {
        hash.update(path.as_bytes());
        hash.update([0]);
        hash.update(source.as_bytes());
        hash.update([0]);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    })
}

fn selection_rule() -> ObjectSelectionRuleV1 {
    ObjectSelectionRuleV1 {
        player: PlayerRef::Controller,
        zone: Zone::Graveyard,
        any_player: false,
        filter: ObjectSelectionFilterV1::InstantOrSorcery,
        min: 1,
        max: 1,
        action: ObjectSelectionActionV1::MoveTo(Zone::Hand),
    }
}

fn program() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MillCards {
            player: PlayerRef::Controller,
            count: 3,
        },
        EffectOp::SelectObjectsV1 {
            rule: selection_rule(),
        },
    ])
}

pub fn meaning(
    selector: &SelectorV1,
    card_id: u16,
    cast_root: CastRootV1,
) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Inspiration from Beyond" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let normal_cost = Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::U)],
        x_count: 0,
    };
    let flashback_cost = Cost {
        generic: 5,
        pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)],
        x_count: 0,
    };
    let actual_program = (def.spell_effect)();
    if def.types != [CardType::Sorcery]
        || def.colors != [ManaColor::U]
        || !def.subtypes.is_empty()
        || !def.supertypes.is_empty()
        || def.cost != normal_cost
        || def.mana_value != 3
        || def.power.is_some()
        || def.toughness.is_some()
        || def.keywords != Keywords::NONE
        || def.is_token
        || def.target_spec != TargetSpec::None
        || actual_program != Some(program())
        || !matches!(def.flashback.as_ref().map(|f| f.cost), Some([CostComponent::Mana(cost)]) if *cost == flashback_cost)
        || def.additional_cost.is_some()
        || def.alt_cost.is_some()
        || def.kicker_cost.is_some()
        || def.mode2.is_some()
        || def.mode3.is_some()
        || !def.additional_modes.is_empty()
        || def.adventure.is_some()
        || def.omen.is_some()
        || !def.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
    {
        return Err(ErrorV1::WrongRoot);
    }
    let flashback = cast_root == CastRootV1::Flashback;
    let selected_cost = if flashback {
        flashback_cost
    } else {
        normal_cost
    };
    let bytes = serde_json::to_vec(&serde_json::json!({
        "card_id": card_id,
        "cast_root": cast_root,
        "name": def.name,
        "printed_cost": def.cost,
        "selected_cost": selected_cost,
        "types": def.types,
        "colors": def.colors,
        "subtypes": def.subtypes,
        "mana_value": def.mana_value,
        "keywords": def.keywords,
        "target_spec": def.target_spec,
        "program": actual_program,
    }))
    .map_err(|_| ErrorV1::WrongRoot)?;
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        cast_root,
        root_sha256: digest(&bytes),
        rule: RuleV1 {
            printed_cast_cost: if flashback { "{5}{U}{U}" } else { "{2}{U}" }.into(),
            cast_from_own_graveyard: flashback,
            flashback_exiles_when_leaving_stack: flashback,
            ordinary_sorcery_timing_subject_to_cast_permission_effects: true,
            mill_player: PlayerRef::Controller,
            mill_count: 3,
            mill_up_to_available_library_cards: true,
            mill_order_precedes_return_choice: true,
            milled_and_preexisting_graveyard_cards_are_available: true,
            selection_rule: selection_rule(),
            availability_bounds: AvailabilityBoundsV1::OneWhenAvailableElseZero,
            resolving_choice_is_not_a_target: true,
            candidate_physical_owner_is_captured_spell_controller: true,
            candidate_current_zone_and_exact_incarnation_are_authenticated: true,
            spell_remains_on_stack_during_return_choice: true,
            empty_library_does_not_skip_existing_graveyard_choice: true,
            no_priority_between_mill_and_return_choice: true,
        },
        native_model_sampler_admitted: false,
    })
}

pub fn meaning_for_representation(
    representation: MeaningRepresentationV1,
    selector: &SelectorV1,
    card_id: u16,
    cast_root: CastRootV1,
) -> Result<MeaningV1, ErrorV1> {
    if representation != MeaningRepresentationV1::ExactInspirationV1 {
        return Err(ErrorV1::UnsupportedRepresentation);
    }
    meaning(selector, card_id, cast_root)
}

/// Authenticate every field by reconstruction from the selected actual definition.
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if meaning(&record.selector, record.card_id, record.cast_root)? != *record {
        return Err(ErrorV1::ForgedMeaning);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
