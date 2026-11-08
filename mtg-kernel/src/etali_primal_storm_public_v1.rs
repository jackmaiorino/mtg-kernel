//! Exact FDN Etali printing and actor-visible native exile-cast meaning.
use crate::{
    card_def::{self, CardType, Keywords, Subtype, Supertype},
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    state::Zone,
};

pub const PRINTING: u16 = 481;
pub const NAME: &str = "Etali, Primal Storm";
pub const SCHEMA: &str = "mtg-kernel/etali-primal-storm-public/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    let triggers = crate::trigger::triggers_for(PRINTING);
    if card_def::card_id_by_name(NAME) != Some(PRINTING)
        || !definition(PRINTING)
        || triggers.len() != 1
        || triggers[0].condition != crate::trigger::TriggerCondition::Attacks
        || triggers[0].home_zone != Zone::Battlefield
        || triggers[0].face_index != 0
        || triggers[0].intervening_if_kicked
        || triggers[0].intervening_if_controls_another_source_card
        || (triggers[0].effect)() != EffectOp::ExileTopEachPlayerThenCastV1
    {
        return Err("Etali requires its exact pure Limited printing and attack trigger".into());
    }
    Ok(PRINTING)
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == PRINTING
        && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
            let card_def::CardDef {
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
                creature_type_entry_v1: _,
            } = d;
            d.name == NAME
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            Pip::Colored(ManaColor::R),
                            Pip::Colored(ManaColor::R),
                        ],
                        generic: 4,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [Subtype::Elder, Subtype::Dinosaur]
                && d.supertypes == [Supertype::Legendary]
                && d.power == Some(6)
                && d.toughness == Some(6)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [ManaColor::R]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::NONE
                && (d.spell_effect)()
                    == Some(EffectOp::MoveObject {
                        object: ObjectRef::ThisSource,
                        to_zone: Zone::Battlefield,
                    })
                && (d.mana_ability)().is_none()
                && d.alt_cost.is_none()
                && d.kicker_cost.is_none()
                && d.additional_cost.is_none()
                && d.flashback.is_none()
                && d.activated_abilities.is_empty()
                && d.plot_cost.is_none()
                && d.madness_cost.is_none()
                && d.mode2.is_none()
                && d.mode3.is_none()
                && d.additional_modes.is_empty()
                && d.kicked_target_spec.is_none()
                && !d.is_token
                && d.escape.is_none()
                && d.mana_ability_choices.is_empty()
                && !d.enters_battlefield_tapped
                && d.mana_ability_def.is_none()
                && d.minimum_blockers == 1
                && d.omen.is_none()
                && d.mana_value == 6
                && !d.mana_ability_includes_chosen_color
                && d.as_enters_choose_color_other_than.is_none()
                && d.additional_mana_abilities.is_empty()
                && d.object_name == d.name
                && d.enters_battlefield_tapped_unless.is_none()
                && d.attachment.is_none()
                && d.transform_face.is_none()
                && d.saga.is_none()
                && d.optional_additional_cost.is_none()
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
                && d.combat_static_rules == card_def::CombatStaticRulesDef::NONE
                && d.as_enters_color_choice_v1.is_none()
                && d.chosen_color_creature_boost_v1.is_none()
                && d.damage_replacement_v1.is_none()
                && d.token_counter_replacement_v48.is_none()
                && d.starting_life_self_bonuses_v1.is_empty()
                && d.source_type_hexproof_v1.is_none()
                && d.other_controlled_nontoken_death_v1.is_none()
                && d.creature_type_entry_v1.is_none()
        })
}



use crate::{
    effect::{standard_exile, EffectTargetSelectionPurpose, PendingEffectChoice},
    etali_primal_storm_receipts_v1 as receipts,
    ids::PlayerId,
    optional_trigger_public_v1::{self as public, IdentityV1, ReferenceV1, RootV1},
    policy_surface_v5::PolicyDecisionV5,
    rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, GameState, Target},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("etali_primal_storm_public_v1.rs"),
            include_str!("etali_primal_storm_receipts_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("state.rs"),
            include_str!("../build.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        format!("{:x}", hash.finalize())
    }).clone()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub card_type: CardType,
    pub subtypes: [Subtype; 2],
    pub supertype: Supertype,
    pub generic_mana: u8,
    pub red_mana: u8,
    pub power: i16,
    pub toughness: i16,
    pub triggers_on_own_attack: bool,
    pub exile_each_top_simultaneously: bool,
    pub cast_any_number_nonlands_during_resolution: bool,
    pub waive_printed_mana_cost: bool,
    pub retain_normal_targets_and_additional_costs: bool,
    pub free_cast_x: u16,
    pub uncast_cards_stay_exiled: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    let card_id = selected_id()?;
    Ok(MeaningV1 {
        schema: "rules-meaning/etali-primal-storm/v1".into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id,
        card_type: CardType::Creature,
        subtypes: [Subtype::Elder, Subtype::Dinosaur],
        supertype: Supertype::Legendary,
        generic_mana: 4,
        red_mana: 2,
        power: 6,
        toughness: 6,
        triggers_on_own_attack: true,
        exile_each_top_simultaneously: true,
        cast_any_number_nonlands_during_resolution: true,
        waive_printed_mana_cost: true,
        retain_normal_targets_and_additional_costs: true,
        free_cast_x: 0,
        uncast_cards_stay_exiled: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub original_exile: ReferenceV1,
    pub spell: ReferenceV1,
    pub method: crate::state::CastMethodV4,
    pub x: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackV1 {
    pub source: RootV1,
    pub stack_position: Option<u32>,
    pub batch_started: bool,
    pub original_exiles: Vec<ReferenceV1>,
    pub actual_casts: Vec<CastV1>,
    pub finished: Option<bool>,
    pub countered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ChoiceV1 {
    CastOrDecline { source: ReferenceV1, legal: Vec<ReferenceV1> },
    ChildCast { source: ReferenceV1, original_exile: ReferenceV1, placeholder: ReferenceV1 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub held: Vec<RootV1>,
    pub attacks: Vec<AttackV1>,
    pub chooser: Option<ChoiceV1>,
}

fn captured(source: AbilitySourceContractV4) -> ReferenceV1 {
    ReferenceV1 {
        object: source.source.0,
        generation: source.zone_change_count,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        identity: IdentityV1::Known { card_id: source.card_def },
    }
}

fn attack_root(state: &GameState, source: AbilitySourceContractV4) -> Result<RootV1, String> {
    let mut root = public::root(state, source, None)?;
    if source.card_def != PRINTING {
        return Err("Etali native source is not the exact attack printing".into());
    }
    root.original_source = captured(source);
    Ok(root)
}

fn current_choice(
    state: &GameState,
    viewer: PlayerId,
    origin: &PolicyDecisionV5,
    rows: &[receipts::Occurrence],
) -> Result<Option<ChoiceV1>, String> {
    if origin.actor(state) != Some(viewer) {
        return Ok(None);
    }
    if let Some(pending) = state.engine.pending_effect.as_ref()
        .filter(|pending| standard_exile::top_batch_pending(pending))
    {
        let Some(PendingEffectChoice::SelectTargets {
            player, purpose: EffectTargetSelectionPurpose::ExileBatch { .. }, legal, ..
        }) = &pending.choice
        else { return Ok(None) };
        if *player != viewer {
            return Ok(None);
        }
        let row = rows.iter().find(|row| row.id == pending.resolving_item.v4.stack_item_id)
            .ok_or("Etali current cast menu has no actual original attack")?;
        let legal = legal.iter().map(|candidate| {
            let (Target::Object(object), Some(binding)) = (candidate.target, candidate.expected_object)
            else { return Err("Etali native menu has no original exile binding".into()) };
            row.exiled.iter().find(|top| top.arrival.source == object && binding.object == object
                && binding.expected_zone == Zone::Exile
                && top.arrival.zone_change_count == binding.expected_zone_change_count)
                .map(|top| captured(top.arrival))
                .ok_or_else(|| "Etali offered card was not exiled by its original attack".into())
        }).collect::<Result<_, String>>()?;
        return Ok(Some(ChoiceV1::CastOrDecline { source: captured(row.source), legal }));
    }
    let Some(context) = state.standard_v1.as_ref().and_then(|standard| standard.resolution_play.as_ref())
        .filter(|context| context.suspended.as_ref().is_some_and(|pending|
            pending.ctx.controller == viewer && standard_exile::top_batch_pending(pending)))
    else { return Ok(None) };
    let row = rows.iter().find(|row| row.id == context.parent)
        .ok_or("Etali current child has no actual original attack")?;
    let permission = context.permission.ok_or("Etali current child permission absent")?;
    let top = row.exiled.iter().find(|top| top.arrival.source == permission.card.object
        && top.arrival.zone_change_count == permission.card.expected_zone_change_count)
        .ok_or("Etali current child lost its original public exile")?;
    let placeholder = public::reference(state, permission.card.object, None)
        .filter(|card| card.zone == Zone::Stack && card.controller == viewer.into())
        .ok_or("Etali current child lost its native stack placeholder")?;
    Ok(Some(ChoiceV1::ChildCast {
        source: captured(row.source),
        original_exile: captured(top.arrival),
        placeholder,
    }))
}

pub(crate) fn capture(
    state: &GameState,
    viewer: PlayerId,
    origin: &PolicyDecisionV5,
) -> Result<FactsV1, String> {
    let rows = receipts::occurrences(state)?;
    receipts::validate_current(state, &rows)?;
    let chooser = current_choice(state, viewer, origin, &rows)?;
    let mut held = Vec::new();
    for trigger in public::held(state) {
        if trigger.effect != EffectOp::ExileTopEachPlayerThenCastV1 {
            continue;
        }
        crate::engine::validate_pending_trigger_identity(state, trigger)?;
        let source = trigger.source_contract.ok_or("Etali held source absent")?;
        if !receipts::observed_attack(state, source) {
            return Err("Etali held trigger lacks native attack witness".into());
        }
        held.push(attack_root(state, source)?);
    }
    let attacks = rows.into_iter().map(|row| {
        let actual_casts = row.casts.iter().map(|cast| {
            let original = row.exiled.iter().find(|top| top.arrival.source == cast.original.object)
                .ok_or("Etali actual cast lost original public exile")?;
            let spell = cast.source;
            Ok(CastV1 {
                original_exile: captured(original.arrival),
                spell: ReferenceV1 {
                    object: spell.source.0, generation: spell.zone_change_count,
                    owner: spell.owner.into(), controller: spell.controller.into(), zone: Zone::Stack,
                    identity: IdentityV1::Known { card_id: spell.card_def },
                },
                method: spell.cast_method,
                x: spell.finalized_cast_binding.ok_or("Etali actual cast lacks finalized X")?.x_value,
            })
        }).collect::<Result<_, String>>()?;
        Ok(AttackV1 {
            source: attack_root(state, row.source)?, stack_position: row.stack_position,
            batch_started: row.started, original_exiles: row.exiled.iter().map(|row| captured(row.arrival)).collect(),
            actual_casts, finished: row.finished, countered: row.countered,
        })
    }).collect::<Result<_, String>>()?;
    Ok(FactsV1 {
        schema: SCHEMA.into(), viewer: viewer.into(), meaning: printed_meaning()?,
        held, attacks, chooser,
    })
}
