//! Precise selected Zul Ashur roots, separate from fixed vectors and models.
use crate::card_def::{
    ActivationTargetFilter, CardType, CostComponent, Keywords, Subtype, Supertype, TargetSpec,
    WardCostDef, CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/zul-ashur/v1";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    PrintedCreature,
    WardPayTwoLife,
    PrintedActivation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub printed_cost: String,
    pub ward_life: u8,
    pub activation_cost: String,
    pub mandatory_target_count: u8,
    pub target_is_own_graveyard_zombie_creature: bool,
    pub original_controller_is_permission_holder: bool,
    pub exact_target_incarnation: bool,
    pub source_departure_does_not_cancel_permission: bool,
    pub ordinary_timing_and_all_costs: bool,
    pub this_turn_cleanup_expiry: bool,
    pub repeated_cleanup_expiry: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub model_sampler_admitted: bool,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    digest(
        [
            SCHEMA,
            include_str!("rules_meaning_zul_ashur_v1.rs"),
            include_str!("zul_ashur_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("card_def.rs"),
            include_str!("../build.rs"),
        ]
        .join("\0")
        .as_bytes(),
    )
}
pub fn live_selector() -> Result<SelectorV1, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Zul meaning needs Limited catalog".into());
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::CARD_DB_HASH,
        producer_sha256: producer_sha256_v1(),
    })
}
pub fn meaning(selector: &SelectorV1, id: u16, root: RootV1) -> Result<MeaningV1, String> {
    if *selector != live_selector()? {
        return Err("Zul selector changed".into());
    }
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or("Zul definition absent")?;
    let [ability] = d.activated_abilities else {
        return Err("Zul activation shape changed".into());
    };
    if !crate::zul_ashur_v1::owns_definition(id)
        || d.object_name != d.name
        || d.cost.generic != 1
        || d.cost.pips != [Pip::Colored(ManaColor::B)]
        || d.cost.x_count != 0
        || d.mana_value != 2
        || d.types != [CardType::Creature]
        || d.colors != [ManaColor::B]
        || d.supertypes != [Supertype::Legendary]
        || d.subtypes != [Subtype::Zombie, Subtype::Warlock]
        || (d.power, d.toughness) != (Some(2), Some(2))
        || d.keywords != Keywords::NONE
        || d.ward_cost != Some(WardCostDef::PayLife(2))
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || ability.cost != [CostComponent::Tap]
        || (ability.effect)() != crate::zul_ashur_v1::recipe()
        || ability.target_spec != TargetSpec::CreatureCardInOwnGraveyard
        || ability.activation_target_filter
            != ActivationTargetFilter::CreatureSubtype(Subtype::Zombie)
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.face.is_some()
        || ability.max_activations_per_turn.is_some()
        || d.is_land
        || d.is_token
        || !d.produces_mana.is_empty()
        || d.alt_cost.is_some()
        || d.kicker_cost.is_some()
        || d.flashback.is_some()
        || d.escape.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.adventure.is_some()
        || d.omen.is_some()
        || !crate::trigger::triggers_for(id).is_empty()
    {
        return Err("Zul printed definition/root changed".into());
    }
    let rule = RuleV1 {
        printed_cost: "{1}{B}".into(),
        ward_life: 2,
        activation_cost: "{T}".into(),
        mandatory_target_count: 1,
        target_is_own_graveyard_zombie_creature: true,
        original_controller_is_permission_holder: true,
        exact_target_incarnation: true,
        source_departure_does_not_cancel_permission: true,
        ordinary_timing_and_all_costs: true,
        this_turn_cleanup_expiry: true,
        repeated_cleanup_expiry: true,
    };
    let bytes = serde_json::to_vec(&(
        id,
        root,
        d.cost,
        d.colors,
        d.types,
        d.subtypes,
        d.supertypes
            .iter()
            .map(|s| match s {
                Supertype::Basic => "Basic",
                Supertype::Snow => "Snow",
                Supertype::Legendary => "Legendary",
            })
            .collect::<Vec<_>>(),
        d.power,
        d.toughness,
        d.keywords,
        2_u8,
        d.target_spec,
        (d.spell_effect)(),
        ability.target_spec,
        (ability.effect)(),
        &rule,
    ))
    .map_err(|e| e.to_string())?;
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id: id,
        root,
        root_sha256: digest(&bytes),
        rule,
        model_sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV1) -> Result<(), String> {
    if *value != meaning(&value.selector, value.card_id, value.root)? {
        return Err("Zul meaning forged".into());
    }
    Ok(())
}
