//! Standalone ordered Drake Hatcher meaning. No model or transport admission.
use crate::card_def::{
    ActivationTargetFilter, CardCapability, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/drake-hatcher/v53";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV53 {
    Prowess,
    CombatIncubation,
    RemoveIncubationForDrake,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorV53 {
    SourceControllerAtEvent,
    OriginalActivatingController,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventV53 {
    ControllerCastsNoncreatureSpell,
    SourceDealsFinalPositiveCombatDamageToPlayer,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceV53 {
    OriginalBattlefieldIncarnation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterV53 {
    Incubation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmountV53 {
    Fixed(i32),
    FinalPositiveCommittedPlayerCombatDamage,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenBlueprintV53 {
    pub card_id: u16,
    pub power: i32,
    pub toughness: i32,
    pub colors: Vec<ManaColor>,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub keyword_bits: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CostV53 {
    RemoveCounters {
        source: SourceV53,
        kind: CounterV53,
        count: u16,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV53 {
    BoostUntilEndOfTurn {
        source: SourceV53,
        power: i32,
        toughness: i32,
    },
    PlaceCounters {
        source: SourceV53,
        kind: CounterV53,
        amount: AmountV53,
        replacements_from_resolution_world: bool,
    },
    CreateToken {
        controller: ActorV53,
        count: u32,
        token: TokenBlueprintV53,
        independent_of_source_survival: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ProgramV53 {
    Triggered {
        event: EventV53,
        controller: ActorV53,
        printed_ability_required_at_event: bool,
        untargeted: bool,
        original_source_incarnation_required_for_application: bool,
        ordered_instructions: Vec<InstructionV53>,
    },
    Activated {
        activation_zone: Zone,
        source: SourceV53,
        controller: ActorV53,
        any_priority_timing: bool,
        untargeted: bool,
        ordered_costs: Vec<CostV53>,
        costs_paid_before_stack: bool,
        countering_refunds_costs: bool,
        ordered_instructions: Vec<InstructionV53>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV53 {
    pub generic_mana: u8,
    pub colored_pips: Vec<ManaColor>,
    pub colors: Vec<ManaColor>,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub power: i32,
    pub toughness: i32,
    pub keyword_bits: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV53 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_id: u16,
    pub capability: CardCapability,
    pub root: RootV53,
    /// Integrity only. The typed ordered program is the meaning.
    pub root_sha256: String,
    pub printed: PrintedV53,
    pub program: ProgramV53,
    pub full_engine_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
    pub fair_search_admitted: bool,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("rules_meaning_drake_hatcher_v53.rs"),
            include_str!("drake_hatcher_v1.rs"),
            include_str!("drake_hatcher_v1/custody.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("state.rs"),
            include_str!("trigger.rs"),
            include_str!("card_def.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("../build.rs"),
            include_str!("../../data/limited/fdn_v1/cards_v1.json"),
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
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("effect/inspiration_resolution_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("mana.rs"),
            include_str!("legend_rule_v1.rs"),
            include_str!("rl.rs"),
        ] {
            h.update((source.len() as u64).to_le_bytes());
            h.update(source.as_bytes());
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
/// Exact immutable printing/root validation, independent of a later board.
fn printing(card_id: u16) -> Result<u16, String> {
    if !crate::drake_hatcher_v1::custody::definition(card_id) {
        return Err("V53 requires the actual pure Limited Drake Hatcher printing".into());
    }
    let d = &CARD_DEFS[usize::from(card_id)];
    let triggers = crate::trigger::triggers_for(card_id);
    let ability = &d.activated_abilities[0];
    if d.name != "Drake Hatcher"
        || d.object_name != d.name
        || d.is_token
        || d.is_land
        || d.cost
            != (Cost {
                generic: 1,
                pips: &[Pip::Colored(ManaColor::U)],
                x_count: 0,
            })
        || d.mana_value != 2
        || d.colors != [ManaColor::U]
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Human, Subtype::Wizard]
        || !d.supertypes.is_empty()
        || (d.power, d.toughness) != (Some(1), Some(3))
        || d.keywords != Keywords::VIGILANCE
        || d.target_spec != TargetSpec::None
        || d.flashback.is_some()
        || d.kicker_cost.is_some()
        || d.additional_cost.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.equipment.is_some()
        || d.attachment.is_some()
        || d.transform_face.is_some()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || triggers.len() != 2
        || triggers[0].condition != crate::trigger::TriggerCondition::CastNoncreatureSpell
        || triggers[1].condition != crate::trigger::TriggerCondition::DealsCombatDamageToPlayer
        || triggers.iter().any(|t| {
            t.home_zone != Zone::Battlefield
                || t.face_index != 0
                || t.intervening_if_kicked
                || t.intervening_if_controls_another_source_card
        })
        || (triggers[0].effect)()
            != (EffectOp::BindTemporaryBoostToTriggerSource {
                power: 1,
                toughness: 1,
            })
        || (triggers[1].effect)() != crate::drake_hatcher_v1::combat_binder()
        || crate::trigger::trigger_target_spec(card_id) != TargetSpec::None
        || ability.cost
            != [CostComponent::RemoveSourceCountersV1 {
                kind: crate::drake_hatcher_v1::SourceCounterKindV1::Incubation,
                count: 3,
            }]
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.target_spec != TargetSpec::None
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || (ability.effect)() != EffectOp::DrakeCreateTokenFromPaidV1
    {
        return Err("V53 genuine Drake printing or ordered roots changed".into());
    }
    let token_id =
        crate::card_def::card_id_by_name("Drake Token").ok_or("V53 genuine Drake token absent")?;
    let token = &CARD_DEFS[usize::from(token_id)];
    if !token.is_executable()
        || !token.is_token
        || token.is_land
        || token.mana_value != 0
        || token.cost
            != (Cost {
                generic: 0,
                pips: &[],
                x_count: 0,
            })
        || !token.supertypes.is_empty()
        || token.attachment.is_some()
        || token.name != "Drake Token"
        || token.object_name != token.name
        || token.colors != [ManaColor::U]
        || token.types != [CardType::Creature]
        || token.subtypes != [Subtype::Drake]
        || (token.power, token.toughness) != (Some(2), Some(2))
        || token.keywords != Keywords::FLYING
        || !token.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(token_id).is_empty()
    {
        return Err("V53 genuine Drake token blueprint changed".into());
    }
    Ok(token_id)
}
pub fn printed_meaning(card_id: u16, root: RootV53) -> Result<MeaningV53, String> {
    let token_id = printing(card_id)?;
    let original = SourceV53::OriginalBattlefieldIncarnation;
    let program = match root {
        RootV53::Prowess => ProgramV53::Triggered {
            event: EventV53::ControllerCastsNoncreatureSpell,
            controller: ActorV53::SourceControllerAtEvent,
            printed_ability_required_at_event: true,
            untargeted: true,
            original_source_incarnation_required_for_application: true,
            ordered_instructions: vec![InstructionV53::BoostUntilEndOfTurn {
                source: original,
                power: 1,
                toughness: 1,
            }],
        },
        RootV53::CombatIncubation => ProgramV53::Triggered {
            event: EventV53::SourceDealsFinalPositiveCombatDamageToPlayer,
            controller: ActorV53::SourceControllerAtEvent,
            printed_ability_required_at_event: true,
            untargeted: true,
            original_source_incarnation_required_for_application: true,
            ordered_instructions: vec![InstructionV53::PlaceCounters {
                source: original,
                kind: CounterV53::Incubation,
                amount: AmountV53::FinalPositiveCommittedPlayerCombatDamage,
                replacements_from_resolution_world: true,
            }],
        },
        RootV53::RemoveIncubationForDrake => ProgramV53::Activated {
            activation_zone: Zone::Battlefield,
            source: original,
            controller: ActorV53::OriginalActivatingController,
            any_priority_timing: true,
            untargeted: true,
            ordered_costs: vec![CostV53::RemoveCounters {
                source: original,
                kind: CounterV53::Incubation,
                count: 3,
            }],
            costs_paid_before_stack: true,
            countering_refunds_costs: false,
            ordered_instructions: vec![InstructionV53::CreateToken {
                controller: ActorV53::OriginalActivatingController,
                count: 1,
                token: TokenBlueprintV53 {
                    card_id: token_id,
                    power: 2,
                    toughness: 2,
                    colors: vec![ManaColor::U],
                    types: vec![CardType::Creature],
                    subtypes: vec![Subtype::Drake],
                    keyword_bits: Keywords::FLYING.0,
                },
                independent_of_source_survival: true,
            }],
        },
    };
    let d = &CARD_DEFS[usize::from(card_id)];
    let printed = PrintedV53 {
        generic_mana: 1,
        colored_pips: vec![ManaColor::U],
        colors: vec![ManaColor::U],
        types: vec![CardType::Creature],
        subtypes: vec![Subtype::Human, Subtype::Wizard],
        power: 1,
        toughness: 3,
        keyword_bits: Keywords::VIGILANCE.0,
    };
    let root_sha256 = digest(
        &serde_json::to_vec(&(SCHEMA, card_id, root, &printed, &program))
            .map_err(|e| e.to_string())?,
    );
    Ok(MeaningV53 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_id,
        capability: d.capability,
        root,
        root_sha256,
        printed,
        program,
        full_engine_admitted: d.capability == CardCapability::Full,
        model_admitted: false,
        sampler_admitted: false,
        fair_search_admitted: false,
    })
}
pub fn validate(value: &MeaningV53) -> Result<(), String> {
    if *value == printed_meaning(value.card_id, value.root)? {
        Ok(())
    } else {
        Err("V53 typed ordered Drake meaning differs".into())
    }
}
