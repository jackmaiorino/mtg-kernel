//! Native Partial494 Kaito and exact Partial495 Ninja dependency.
//! All source fixtures remain uncompiled/unrun; no complete consumer admission.
use super::*;
use crate::engine::{EffectDuration, UntilEndOfTurnEffect};
use crate::{card_def, event::CommittedEvent as E, ids::StackItemId, state::StackItemKind};
use serde::{Deserialize, Serialize};
// This source is intentionally independent of Kaito, Bane of Nightmares.
pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 494
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
            d.name == "Kaito, Cunning Infiltrator"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                        ],
                        generic: 1,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Planeswalker]
                && d.subtypes == [card_def::Subtype::Kaito]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power.is_none()
                && d.toughness.is_none()
                && d.starting_loyalty == Some(3)
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::U]
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
                && abilities_exact(d.activated_abilities)
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
                && d.mana_value == 3
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
fn abilities_exact(abilities: &[card_def::ActivatedAbilityDef]) -> bool {
    abilities.len() == 3
        && abilities.iter().enumerate().all(|(i, a)| {
            a.cost == [card_def::CostComponent::Loyalty([1, -2, -9][i])]
                && a.target_spec == target(i)
                && (a.effect)() == program(i)
                && a.activation_zone == Zone::Battlefield
                && a.sorcery_speed_only
                && a.activation_target_filter == card_def::ActivationTargetFilter::TargetSpecOnly
                && a.max_activations_per_turn.is_none()
                && a.face.is_none()
        })
}

fn leaf(kind: u8, origin: Option<u64>) -> EffectOp {
    EffectOp::KaitoCunningV75 { kind, origin }
}
pub(crate) fn plus_one() -> EffectOp {
    EffectOp::Sequence(vec![
        leaf(0, None),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        leaf(5, None),
    ])
}
pub(crate) fn ninja() -> EffectOp {
    leaf(1, None)
}
pub(crate) fn emblem() -> EffectOp {
    leaf(2, None)
}
fn program(i: usize) -> EffectOp {
    match i {
        0 => plus_one(),
        1 => ninja(),
        2 => emblem(),
        _ => EffectOp::Sequence(vec![]),
    }
}
fn target(i: usize) -> card_def::TargetSpec {
    if i == 0 {
        card_def::TargetSpec::StandardV1(
            crate::standard_cards_v1::StandardTargetV1::UpToOneControlledCreatureV1,
        )
    } else {
        card_def::TargetSpec::None
    }
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    matches!(op, EffectOp::KaitoCunningV75 { .. }) || *op == plus_one()
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentV75 {
    index: u64,
    turn: u32,
    source: AbilitySourceContractV4,
    pending: Box<crate::engine::PendingActivation>,
    before: u32,
    after: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CombatV75 {
    source: AbilitySourceContractV4,
    creature: AbilitySourceContractV4,
    player: PlayerId,
    amount: i32,
    damage: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV75 {
    Payment {
        input: Box<PaymentV75>,
    },
    Published {
        input: Box<PaymentV75>,
        item: Box<StackItem>,
    },
    Combat {
        capture: Box<CombatV75>,
    },
    Bound {
        capture: u64,
        original: Box<CombatV75>,
    },
    Emblem {
        publication: u64,
        item: Box<StackItem>,
        controller: PlayerId,
    },
    Cast {
        emblem: u64,
        cast: u64,
        spell: Box<StackItem>,
    },
    Collected {
        origin: u64,
    },
    TriggerPublished {
        origin: u64,
        item: Box<StackItem>,
    },
    Evasion {
        publication: u64,
        target: Option<EffectObjectBinding>,
        timestamp: Option<u64>,
    },
    DiscardInput {
        publication: u64,
        pending: Box<crate::engine::PendingDiscard>,
        chosen: Vec<EffectObjectBinding>,
        history_start: u64,
    },
    DiscardFinished {
        input: u64,
        history_end: u64,
    },
    TokenInput {
        publication: u64,
        item: Box<StackItem>,
    },
    TokenFinished {
        input: u64,
        history_end: u64,
    },
    Finished {
        item: Box<StackItem>,
    },
    NotResolved {
        item: Box<StackItem>,
        countered: bool,
    },
}
fn rows(state: &GameState) -> impl DoubleEndedIterator<Item = (usize, &JournalV75)> {
    state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::KaitoCunningV75])
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            E::KaitoCunningV75 { journal } => Some((i, journal.as_ref())),
            _ => None,
        })
}
fn append(state: &mut GameState, journal: JournalV75) {
    state.engine.event_history.push(E::KaitoCunningV75 {
        journal: Box::new(journal),
    });
}
fn at(state: &GameState, i: u64) -> Result<&JournalV75, String> {
    match usize::try_from(i)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    {
        Some(E::KaitoCunningV75 { journal }) => Ok(journal),
        _ => Err("Kaito original journal absent".into()),
    }
}
fn published(state: &GameState, item: &StackItem) -> Result<u64, String> {
    let found = rows(state)
        .filter_map(|(i, j)| match j {
            JournalV75::Published { item: old, .. }
            | JournalV75::TriggerPublished { item: old, .. }
                if old.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some((i, old.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if found.len() != 1 || found[0].1 != item {
        return Err("Kaito exact original publication changed".into());
    }
    Ok(found[0].0 as u64)
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    item.inline_effect.as_ref().is_some_and(recipe)||rows(state).any(|(_,j)|matches!(j,JournalV75::Published{item:old,..}|JournalV75::TriggerPublished{item:old,..}if old.v4.stack_item_id==item.v4.stack_item_id))
}
pub(crate) fn context_selected(state: &GameState, ctx: &ExecCtx) -> bool {
    waiting(state,ctx)||rows(state).any(|(_,j)|matches!(j,JournalV75::Published{item,..}|JournalV75::TriggerPublished{item,..} if Some(item.v4.stack_item_id)==ctx.stack_item_id || ctx.ability_source_contract.is_some()&&ctx.ability_source_contract==item.v4.ability_source_contract))
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|item| {
        selected(state, item)
            && (item.source == ctx.source
                || Some(item.v4.stack_item_id) == ctx.stack_item_id
                || item.v4.ability_source_contract.is_some()
                    && item.v4.ability_source_contract == ctx.ability_source_contract)
    })
}
pub(crate) fn validate_pending(state: &GameState) -> Result<(), String> {
    let Some(p) = state
        .engine
        .pending_effect
        .as_ref()
        .filter(|p| selected(state, &p.resolving_item) || context_selected(state, &p.ctx))
    else {
        return Ok(());
    };
    published(state, &p.resolving_item)?;
    let mut ctx = super::crystal_prevention_v1::context(&p.resolving_item);
    ctx.niv_damage_origin_v1 = event::niv_damage_origin_for_saved_item_v1(state, &p.resolving_item);
    let root = p
        .resolving_item
        .inline_effect
        .as_ref()
        .ok_or("Kaito root erased")?;
    if p.ctx!=ctx||p.frames.iter().any(|f|matches!(f,EffectFrame::Program{op,path}if effect_op_at_structural_path(root,path)!=Some(op))){return Err("Kaito full continuation context/root changed".into())}
    if *root != plus_one() {
        return Err("atomic Kaito effect retained a continuation".into());
    }
    let publication = published(state, &p.resolving_item)?;
    let evasions = rows(state)
        .filter(|(_, j)| matches!(j,JournalV75::Evasion{publication:i,..}if *i==publication))
        .collect::<Vec<_>>();
    let initial = [EffectFrame::Program {
        op: plus_one(),
        path: vec![],
    }];
    if p.frames == initial
        && p.choice.is_none()
        && p.answered_choice_guard.is_none()
        && evasions.is_empty()
        && state.engine.pending_discard.is_none()
    {
        return Ok(());
    }
    if evasions.len() != 1 || p.choice.is_some() || p.answered_choice_guard.is_some() {
        return Err("Kaito draw/discard lost its actual evasion stage".into());
    }
    original_draw(state, evasions[0].0, ctx.controller)?;
    let finished = EffectFrame::Program {
        op: leaf(5, None),
        path: vec![3],
    };
    if let Some(discard)=state.engine.pending_discard.as_ref(){
        if discard.player!=ctx.controller||discard.count!=1||p.frames!=[finished,EffectFrame::Program{op:EffectOp::DiscardCards{player:PlayerRef::Controller,count:1},path:vec![2]}]{return Err("Kaito issued discard changed its exact remaining program".into())}
    }else if p.frames!=[finished]||!rows(state).any(|(_,j)|matches!(j,JournalV75::DiscardFinished{input,..}if matches!(at(state,*input),Ok(JournalV75::DiscardInput{publication:i,..})if *i==publication))){return Err("Kaito answered discard erased its actual native input/payment".into())}
    Ok(())
}
fn original_draw(state: &GameState, start: usize, player: PlayerId) -> Result<(), String> {
    if state
        .engine
        .event_history
        .iter()
        .skip(start + 1)
        .filter(|e| matches!(e,E::Draw{player:p,..}if *p==player))
        .count()
        != 1
    {
        return Err("Kaito original draw instruction absent or repeated".into());
    }
    Ok(())
}
pub(crate) fn discard_selected(state: &GameState) -> bool {
    state
        .engine
        .pending_effect
        .as_ref()
        .is_some_and(|p| selected(state, &p.resolving_item))
}
pub(crate) fn record_discard_input(
    state: &mut GameState,
    pending: &crate::engine::PendingDiscard,
    chosen: &[ObjectId],
) -> Result<u64, String> {
    validate_pending(state)?;
    let p = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or("Kaito discard lost effect")?;
    let publication = published(state, &p.resolving_item)?;
    let chosen = chosen
        .iter()
        .map(|id| {
            let o = state.objects.get(*id);
            EffectObjectBinding {
                object: *id,
                expected_zone: o.zone,
                expected_zone_change_count: o.zone_change_count,
            }
        })
        .collect::<Vec<_>>();
    let input = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV75::DiscardInput {
            publication,
            pending: Box::new(pending.clone()),
            chosen,
            history_start: input,
        },
    );
    Ok(input)
}
fn validate_discard(state: &GameState, input: u64, end: u64) -> Result<(), String> {
    let JournalV75::DiscardInput {
        publication,
        pending,
        chosen,
        history_start,
    } = at(state, input)?
    else {
        return Err("Kaito original discard input absent".into());
    };
    let JournalV75::Published { item, .. } = at(state, *publication)? else {
        return Err("Kaito discard publication absent".into());
    };
    let crate::engine::DiscardResume::FinishEffectContinuation {
        stack_item_id,
        path,
        original_hand,
    } = &pending.resume
    else {
        return Err("Kaito discard resume kind changed".into());
    };
    if *history_start != input
        || end <= input
        || item.inline_effect != Some(plus_one())
        || pending.player != item.controller
        || pending.count != 1
        || chosen.len() != 1
        || !original_hand.contains(&chosen[0])
        || *stack_item_id != item.v4.stack_item_id
        || *path != vec![2]
        || chosen[0].expected_zone != Zone::Hand
    {
        return Err("Kaito original discard hand/choice/continuation changed".into());
    }
    let entries = state
        .engine
        .event_history
        .get(input as usize + 1..end as usize)
        .ok_or("Kaito discard interval changed")?;
    if entries.iter().filter(|e|matches!(e,E::ZoneChange{object,from:Zone::Hand,controller_before,..}if *object==chosen[0].object&&*controller_before==pending.player)).count()!=1{return Err("Kaito actual selected discard departure absent or repeated".into())}
    Ok(())
}
pub(crate) fn record_discard_finished(state: &mut GameState, input: u64) -> Result<(), String> {
    let history_end = state.engine.event_history.len() as u64;
    validate_discard(state, input, history_end)?;
    append(state, JournalV75::DiscardFinished { input, history_end });
    Ok(())
}
pub(crate) struct ResumeOwnerV75 {
    native: NativeExecutionOwnerV1,
}
pub(crate) fn acquire_resume(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<ResumeOwnerV75>, String> {
    let Some(p) = state
        .engine
        .pending_effect
        .as_ref()
        .filter(|p| selected(state, &p.resolving_item))
    else {
        return Ok(None);
    };
    super::validate_pending_effect_choice(state)?;
    if p.resolving_item != *item || p.choice.is_some() || item.inline_effect != Some(plus_one()) {
        return Err("Kaito ready continuation changed".into());
    }
    let i = crate::engine::resolution_cast_v1::resolving_index(state, item.v4.stack_item_id)
        .ok_or("Kaito ready row absent")?;
    let mut native = NativeExecutionOwnerV1::from_validated_pending(state)?;
    native.saved_pending = Some(Box::new(p.clone()));
    if native.remaining_stack.remove(i) != *item
        || native
            .remaining_stack
            .iter()
            .any(|r| r.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Kaito ready row duplicated".into());
    }
    Ok(Some(ResumeOwnerV75 { native }))
}
pub(crate) fn resume(
    state: &mut GameState,
    owner: ResumeOwnerV75,
) -> Result<ResumableProgress, String> {
    owner.native.validate_supplied_pending(state)?;
    super::drive_with_native_row_v1(state, &owner.native)
}
pub(crate) fn record_input(
    state: &mut GameState,
    p: &crate::engine::PendingActivation,
) -> Result<Option<u64>, String> {
    let source = AbilitySourceContractV4::capture(state, p.source);
    if !definition(source.card_def)
        || p.cauldron_grant.0.is_some()
        || usize::from(p.ability_index) >= 3
    {
        return Ok(None);
    }
    let before = crate::planeswalker_v1::loyalty(state, p.source).ok_or("Kaito loyalty absent")?;
    let after = crate::planeswalker_v1::quote_loyalty_change_v48(
        state,
        p.source,
        [1, -2, -9][usize::from(p.ability_index)],
        crate::doubling_season_v48::CounterOriginV48::Cost,
    )?;
    let index = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV75::Payment {
            input: Box::new(PaymentV75 {
                index,
                turn: state.turn_key_v1(),
                source,
                pending: Box::new(p.clone()),
                before,
                after,
            }),
        },
    );
    Ok(Some(index))
}

pub(crate) fn record_paid(
    state: &mut GameState,
    id: StackItemId,
    input: Option<u64>,
) -> Result<(), String> {
    let Some(input) = input else { return Ok(()) };
    let JournalV75::Payment { input } = at(state, input)? else {
        return Err("Kaito paid input kind changed".into());
    };
    let input = input.clone();
    let item = state
        .stack
        .iter()
        .find(|r| r.v4.stack_item_id == id)
        .ok_or("Kaito paid row absent")?
        .clone();
    if item.source != input.source.source
        || item.controller != input.source.controller
        || item.v4.ability_source_contract != Some(input.source)
        || crate::planeswalker_v1::loyalty(state, item.source) != Some(input.after)
    {
        return Err("Kaito paid row did not follow actual loyalty cost".into());
    }
    append(
        state,
        JournalV75::Published {
            input,
            item: Box::new(item),
        },
    );
    Ok(())
}

fn validate_payment(state: &GameState, p: &PaymentV75) -> Result<(), String> {
    if !definition(p.source.card_def)
        || p.source.zone != Zone::Battlefield
        || p.pending.source != p.source.source
        || p.pending.controller != p.source.controller
        || p.pending.source_zone_change_count != p.source.zone_change_count
        || p.pending.cauldron_grant.0.is_some()
        || p.pending.ability_index >= 3
        || p.pending.target_spec != target(p.pending.ability_index as usize)
        || p.pending.targets_chosen.len()
            > usize::from(crate::engine::target_count(p.pending.target_spec))
        || p.pending.target_contracts.len() != p.pending.targets_chosen.len()
        || p.pending.loyalty_x.is_some()
        || p.pending.mana_x_v1.is_some()
        || !p.pending.object_cost_chosen.is_empty()
        || p.pending.crew_finished
        || p.pending
            .cost_discard_paid
            .as_ref()
            .is_none_or(|v| !v.is_empty())
    {
        return Err("Kaito original paid source/choice changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, p.source)?;
    let delta = [1, -2, -9][p.pending.ability_index as usize];
    if delta < 0 && p.before.checked_sub((-delta) as u32) != Some(p.after)
        || delta > 0 && p.after <= p.before
    {
        return Err("Kaito original loyalty subtraction changed".into());
    }
    Ok(())
}

fn validate_published(state: &GameState, p: &PaymentV75, item: &StackItem) -> Result<(), String> {
    validate_payment(state, p)?;
    if !matches!(at(state,p.index)?,JournalV75::Payment{input} if input.as_ref()==p)
        || item.kind != StackItemKind::ActivatedAbility
        || item.source != p.source.source
        || item.controller != p.source.controller
        || item.v4.ability_source_contract != Some(p.source)
        || item.inline_effect != Some(program(p.pending.ability_index as usize))
        || item.targets != p.pending.targets_chosen
        || item.v4.target_contracts != p.pending.target_contracts
        || item.v4.target_spec != Some(p.pending.target_spec)
        || item.v4.activated_ability_index != Some(p.pending.ability_index)
        || item.v4.source_contract.is_some()
        || item.v4.granted_by.is_some()
        || item.is_copy
        || item.is_flashback
        || item.kicked
        || item.madness_offer
        || item.mode_chosen != 0
        || !item.discarded.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
    {
        return Err("Kaito publication lost exact original payment/program/targets".into());
    }
    Ok(())
}

pub(crate) fn token_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 495
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
            d.name == "Ninja Token"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[],
                        generic: 0,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Ninja]
                && d.supertypes.is_empty()
                && d.power == Some(2)
                && d.toughness == Some(1)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::U]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::NONE
                && (d.spell_effect)().is_none()
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
                && d.is_token
                && d.escape.is_none()
                && d.mana_ability_choices.is_empty()
                && !d.enters_battlefield_tapped
                && d.mana_ability_def.is_none()
                && d.minimum_blockers == 1
                && d.omen.is_none()
                && d.mana_value == 0
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

pub(crate) struct WaveV75 {
    members: Vec<(
        AbilitySourceContractV4,
        AbilitySourceContractV4,
        PlayerId,
        i32,
    )>,
}
pub(crate) fn capture_wave(state: &GameState, events: &[event::ProposedEvent]) -> WaveV75 {
    let mut members = vec![];
    if !events.iter().any(|e|matches!(e,event::ProposedEvent::Damage(d)if d.is_combat&&d.amount>0&&matches!(d.target,Target::Player(_)))) {return WaveV75{members};}
    let observers = state
        .objects
        .iter()
        .filter(|(id, o)| {
            o.zone == Zone::Battlefield
                && o.v4.face_down_v1.is_none()
                && o.v4.face_index == 0
                && definition(o.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
        })
        .map(|(id, _)| AbilitySourceContractV4::capture(state, id))
        .collect::<Vec<_>>();
    for e in events {
        let event::ProposedEvent::Damage(d) = e else {
            continue;
        };
        let Target::Player(player) = d.target else {
            continue;
        };
        if !d.is_combat
            || d.amount <= 0
            || state
                .objects
                .try_get(d.source)
                .is_none_or(|o| o.zone != Zone::Battlefield)
            || !crate::engine::object_has_type(state, d.source, CardType::Creature)
        {
            continue;
        }
        let creature = AbilitySourceContractV4::capture(state, d.source);
        for observer in observers
            .iter()
            .filter(|o| o.controller == creature.controller)
        {
            members.push((*observer, creature, player, d.amount));
        }
    }
    WaveV75 { members }
}
impl WaveV75 {
    pub(crate) fn finish(self, state: &mut GameState, start: usize) -> Result<(), String> {
        // Each observer sees each final source-to-player Damage occurrence once.
        let mut consumed = std::collections::BTreeSet::new();
        for (source, creature, player, amount) in self.members {
            let damage = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .skip(start)
                .find_map(|(i, e)| {
                    (*e == E::Damage {
                        source: creature.source,
                        target: Target::Player(player),
                        amount,
                    } && !consumed.contains(&(source.source, i)))
                    .then_some(i)
                })
                .ok_or("Kaito final combat damage absent")?;
            consumed.insert((source.source, damage));
            let journal = JournalV75::Combat {
                capture: Box::new(CombatV75 {
                    source,
                    creature,
                    player,
                    amount,
                    damage: damage as u64,
                }),
            };
            let event = E::KaitoCunningV75 {
                journal: Box::new(journal),
            };
            state.engine.event_history.push(event.clone());
            state.engine.event_log.push(event);
        }
        Ok(())
    }
}
// A raw is_combat proposal cannot bind a trigger. Only the genuine engine
// combat-wave finish calls this function, before trigger collection/priority.
pub(crate) fn bind_wave(state: &mut GameState, start: usize) -> Result<(), String> {
    let events = state
        .engine
        .event_log
        .get(start..)
        .ok_or("Kaito combat wave start")?;
    let mut bound = vec![];
    for e in events {
        let E::KaitoCunningV75 { journal } = e else {
            continue;
        };
        let JournalV75::Combat { capture } = journal.as_ref() else {
            continue;
        };
        if !events.contains(
            state
                .engine
                .event_history
                .get(capture.damage as usize)
                .ok_or("Kaito original damage absent")?,
        ) {
            return Err("Kaito damage outside actual native wave".into());
        }
        let indices = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, old)| (old == e).then_some(i as u64))
            .collect::<Vec<_>>();
        let [index] = indices.as_slice() else {
            return Err("Kaito combat capture duplicated or erased".into());
        };
        if bound.iter().any(|(i, _)| i == index)
            || rows(state).any(|(_, j)| matches!(j,JournalV75::Bound{capture,..}if capture==index))
        {
            return Err("Kaito combat capture rebound".into());
        }
        bound.push((*index, capture.clone()));
    }
    for (capture, original) in bound {
        append(state, JournalV75::Bound { capture, original });
    }
    Ok(())
}
fn combat(state: &GameState, index: u64) -> Result<&CombatV75, String> {
    let JournalV75::Combat { capture } = at(state, index)? else {
        return Err("Kaito combat origin kind changed".into());
    };
    if capture.damage>=index||capture.amount<=0||capture.source.zone!=Zone::Battlefield||capture.creature.zone!=Zone::Battlefield||!definition(capture.source.card_def)||capture.source.controller!=capture.creature.controller||state.engine.event_history.get(capture.damage as usize)!=Some(&E::Damage{source:capture.creature.source,target:Target::Player(capture.player),amount:capture.amount})||rows(state).filter(|(_,j)|matches!(j,JournalV75::Bound{capture:i,original}if *i==index&&original.as_ref()==capture.as_ref())).count()!=1{return Err("Kaito combat original event/source/bound roster changed".into())}
    crate::engine::validate_historical_ability_source_contract(state, capture.source)?;
    crate::engine::validate_historical_ability_source_contract(state, capture.creature)?;
    Ok(capture)
}
pub(crate) fn record_cast(state: &mut GameState, cast: usize) -> Result<(), String> {
    let Some(E::SpellCast { spell, controller }) = state.engine.event_history.get(cast).cloned()
    else {
        return Err("Kaito cast occurrence absent".into());
    };
    let emblems = rows(state)
        .filter_map(|(i, j)| matches!(j, JournalV75::Emblem { .. }).then_some(i as u64))
        .collect::<Vec<_>>();
    if emblems.is_empty() {
        return Ok(());
    }
    let items = state
        .stack
        .iter()
        .filter(|r| {
            r.source == spell
                && r.controller == controller
                && r.kind == StackItemKind::Spell
                && !r.is_copy
        })
        .cloned()
        .collect::<Vec<_>>();
    let [item] = items.as_slice() else {
        return Err("Kaito actual cast row absent or ambiguous".into());
    };
    crate::engine::validated_stack_item_target_spec(item, state)?;
    for emblem in emblems {
        let e = E::KaitoCunningV75 {
            journal: Box::new(JournalV75::Cast {
                emblem,
                cast: cast as u64,
                spell: Box::new(item.clone()),
            }),
        };
        state.engine.event_history.push(e.clone());
        state.engine.event_log.push(e);
    }
    Ok(())
}
fn origin(state: &GameState, index: u64) -> Result<(u8, AbilitySourceContractV4), String> {
    match at(state, index)? {
        JournalV75::Combat { .. } => Ok((3, combat(state, index)?.source)),
        JournalV75::Cast {
            emblem,
            cast,
            spell,
        } => {
            let JournalV75::Emblem {
                item, controller, ..
            } = at(state, *emblem)?
            else {
                return Err("Kaito emblem origin absent".into());
            };
            if *emblem >= *cast
                || *cast >= index
                || spell.kind != StackItemKind::Spell
                || spell.is_copy
                || state.engine.event_history.get(*cast as usize)
                    != Some(&E::SpellCast {
                        spell: spell.source,
                        controller: spell.controller,
                    })
            {
                return Err("Kaito emblem cast occurrence changed".into());
            }
            let mut source = item
                .v4
                .ability_source_contract
                .ok_or("Kaito emblem source absent")?;
            source.controller = *controller;
            Ok((4, source))
        }
        _ => Err("Kaito trigger origin kind changed".into()),
    }
}
fn trigger(state: &GameState, index: u64) -> Result<crate::trigger::PendingTrigger, String> {
    let (kind, source) = origin(state, index)?;
    Ok(crate::trigger::PendingTrigger {
        controller: source.controller,
        source: source.source,
        effect: leaf(kind, Some(index)),
        is_madness_offer: false,
        kicked: false,
        target_spec: card_def::TargetSpec::None,
        targets: vec![],
        target_contracts: vec![],
        placement_ordered: false,
        source_contract: Some(source),
        granted_by: None,
        optional_additional_cost_paid: None,
        paid_cost_refs: vec![],
    })
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[E],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    let mut pending = vec![];
    let mut collected = vec![];
    for e in events {
        if !matches!(e,E::KaitoCunningV75{journal}if matches!(journal.as_ref(),JournalV75::Combat{..}|JournalV75::Cast{..}))
        {
            continue;
        }
        let indices = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, old)| (old == e).then_some(i as u64))
            .collect::<Vec<_>>();
        let [index] = indices.as_slice() else {
            return Err("Kaito current trigger event erased/duplicated".into());
        };
        if collected.contains(index)
            || rows(state).any(|(_, j)| matches!(j,JournalV75::Collected{origin}if origin==index))
        {
            return Err("Kaito trigger recollected".into());
        }
        pending.push(trigger(state, *index)?);
        collected.push(*index);
    }
    for origin in collected {
        append(state, JournalV75::Collected { origin });
    }
    Ok(pending)
}
pub(crate) fn validate_root(
    state: &GameState,
    op: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    if let EffectOp::KaitoCunningV75 {
        kind,
        origin: Some(index),
    } = op
    {
        let (k, s) = origin(state, *index)?;
        if k != *kind
            || Some(s) != source
            || rows(state)
                .filter(|(_, j)| matches!(j,JournalV75::Collected{origin}if origin==index))
                .count()
                != 1
        {
            return Err("Kaito trigger source/program consumption changed".into());
        }
    } else if let EffectOp::KaitoCunningV75 { kind: 3 | 4, .. } = op {
        return Err("Kaito trigger lacks original occurrence".into());
    }
    Ok(())
}
pub(crate) fn validate_trigger(state: &GameState, item: &StackItem) -> Result<(), String> {
    let Some(
        op @ EffectOp::KaitoCunningV75 {
            kind: 3 | 4,
            origin: Some(index),
        },
    ) = item.inline_effect.as_ref()
    else {
        return Err("Kaito trigger root erased".into());
    };
    let expected = trigger(state, *index)?;
    validate_root(state, op, item.v4.ability_source_contract)?;
    if item.kind != StackItemKind::TriggeredAbility
        || item.source != expected.source
        || item.controller != expected.controller
        || item.v4.ability_source_contract != expected.source_contract
        || item.v4.granted_by.is_some()
        || item.v4.source_contract.is_some()
        || item.v4.activated_ability_index.is_some()
        || item.v4.target_spec.is_some()
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || item.is_copy
        || item.is_flashback
        || item.kicked
        || item.madness_offer
        || item.mode_chosen != 0
        || item.v4.x_value != 0
        || !item.discarded.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
    {
        return Err("Kaito trigger original row changed".into());
    }
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(EffectOp::KaitoCunningV75 {
        origin: Some(index),
        ..
    }) = item.inline_effect
    else {
        return Ok(());
    };
    validate_trigger(state, item)?;
    if rows(state)
        .any(|(_, j)| matches!(j,JournalV75::TriggerPublished{origin,..}if *origin==index))
    {
        return Err("Kaito trigger published twice".into());
    }
    append(
        state,
        JournalV75::TriggerPublished {
            origin: index,
            item: Box::new(item.clone()),
        },
    );
    Ok(())
}

pub(crate) struct TokenPermitV75<'a, 'b> {
    proof: &'a NativeExecutionProofV1<'b>,
    token: u16,
}
impl TokenPermitV75<'_, '_> {
    pub(crate) fn events(&self) -> Vec<event::ProposedEvent> {
        vec![event::ProposedEvent::create_token(
            self.token,
            self.proof.ctx.controller,
        )]
    }
    pub(crate) fn allows(&self, state: &GameState, events: &[event::ProposedEvent]) -> bool {
        token_definition(self.token)
            && events == self.events()
            && matches!(
                self.proof.owner.item.inline_effect,
                Some(EffectOp::KaitoCunningV75 { kind: 1 | 4, .. })
            )
            && published(state, &self.proof.owner.item).is_ok()
            && self.proof.ctx == &self.proof.owner.ctx
            && self
                .proof
                .owner
                .validate_entry(
                    state,
                    &self.proof.owner.item,
                    self.proof.ctx,
                    &self.proof.owner.root,
                )
                .is_ok()
    }
}
fn finished(state: &GameState, item: &StackItem) -> bool {
    rows(state).any(|(_,j)|matches!(j,JournalV75::Finished{item:r}|JournalV75::NotResolved{item:r,..}if r.v4.stack_item_id==item.v4.stack_item_id))
}
pub(super) fn execute(
    op: &EffectOp,
    ctx: &ExecCtx,
    state: &mut GameState,
    proof: &NativeExecutionProofV1<'_>,
) -> Result<(), String> {
    if proof.ctx != ctx || proof.owner.ctx != *ctx {
        return Err("Kaito lexical context changed".into());
    }
    proof
        .owner
        .validate_entry(state, &proof.owner.item, ctx, &proof.owner.root)?;
    let publication = published(state, &proof.owner.item)?;
    if finished(state, &proof.owner.item) {
        return Err("Kaito publication already closed".into());
    }
    let EffectOp::KaitoCunningV75 {
        kind,
        origin: index,
    } = op
    else {
        return Err("Kaito leaf absent".into());
    };
    let expected = if *kind == 0 || *kind == 5 {
        plus_one()
    } else {
        op.clone()
    };
    if *proof.owner.root != expected {
        return Err("Kaito leaf is not original printed program".into());
    }
    match *kind {
        0 => {
            if rows(state)
                .any(|(_, j)| matches!(j,JournalV75::Evasion{publication:p,..}if *p==publication))
            {
                return Err("Kaito evasion instruction repeated".into());
            }
            let target = match ctx.targets.as_slice() {
                [] => None,
                [Target::Object(id)]
                    if ctx.target_incarnation_matches(0, state)
                        && crate::engine::object_has_type(state, *id, CardType::Creature)
                        && state.objects.get(*id).controller == ctx.controller =>
                {
                    let o = state.objects.get(*id);
                    Some(EffectObjectBinding {
                        object: *id,
                        expected_zone: o.zone,
                        expected_zone_change_count: o.zone_change_count,
                    })
                }
                _ => return Err("Kaito target no longer legal".into()),
            };
            let timestamp = target.map(|b| {
                let timestamp = crate::engine::next_timestamp(state);
                state.engine.until_end_of_turn.push(
                    UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 {
                        object_id: b.object,
                        object_zone_change_count: b.expected_zone_change_count,
                        timestamp,
                        duration: EffectDuration::EndOfTurn,
                    },
                );
                timestamp
            });
            append(
                state,
                JournalV75::Evasion {
                    publication,
                    target,
                    timestamp,
                },
            );
            return Ok(());
        }
        1 | 4 => {
            if *kind == 4 {
                validate_root(state, op, ctx.ability_source_contract)?;
            }
            let token = card_def::card_id_by_name("Ninja Token")
                .filter(|id| token_definition(*id))
                .ok_or("Kaito exact Ninja dependency absent")?;
            let input = state.engine.event_history.len() as u64;
            append(
                state,
                JournalV75::TokenInput {
                    publication,
                    item: Box::new(proof.owner.item.clone()),
                },
            );
            event::commit_kaito_ninja_v75(state, TokenPermitV75 { proof, token })?;
            let history_end = state.engine.event_history.len() as u64;
            append(state, JournalV75::TokenFinished { input, history_end });
        }
        2 => {
            if index.is_some() || proof.owner.item.inline_effect != Some(emblem()) {
                return Err("Kaito emblem recipe changed".into());
            }
            append(
                state,
                JournalV75::Emblem {
                    publication,
                    item: Box::new(proof.owner.item.clone()),
                    controller: ctx.controller,
                },
            );
        }
        3 => {
            validate_root(state, op, ctx.ability_source_contract)?;
            let s = combat(state, index.ok_or("Kaito combat origin absent")?)?.source;
            if state.objects.try_get(s.source).is_some_and(|o| {
                o.zone == Zone::Battlefield && o.zone_change_count == s.zone_change_count
            }) {
                let mut staged = state.clone();
                crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut staged, s.source, 1)?;
                *state = staged;
            }
        }
        5 => {
            if !rows(state)
                .any(|(_, j)| matches!(j,JournalV75::Evasion{publication:p,..}if *p==publication))
            {
                return Err("Kaito draw/discard lacks evasion stage".into());
            }
            if !rows(state).any(|(_,j)|matches!(j,JournalV75::DiscardFinished{input,..}if matches!(at(state,*input),Ok(JournalV75::DiscardInput{publication:i,..})if *i==publication))) {
                let (start,_)=rows(state).find(|(_,j)|matches!(j,JournalV75::Evasion{publication:p,..}if *p==publication)).ok_or("Kaito evasion absent")?;
                original_draw(state,start,ctx.controller)?;
                if !state.players[ctx.controller.index()].hand.is_empty()||!state.engine.event_history.iter().skip(start+1).any(|e|matches!(e,E::Draw{player,object:None}if *player==ctx.controller)){return Err("Kaito actual discard was skipped".into())}
            }
        }
        _ => return Err("Kaito unknown leaf".into()),
    }
    append(
        state,
        JournalV75::Finished {
            item: Box::new(proof.owner.item.clone()),
        },
    );
    Ok(())
}
pub(crate) fn record_not_resolved(
    state: &mut GameState,
    item: &StackItem,
    countered: bool,
) -> Result<(), String> {
    if !selected(state, item) {
        return Ok(());
    }
    published(state, item)?;
    if finished(state, item) {
        return Err("Kaito publication already closed".into());
    }
    append(
        state,
        JournalV75::NotResolved {
            item: Box::new(item.clone()),
            countered,
        },
    );
    Ok(())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|e|matches!(e,E::KaitoCunningV75{journal}if !matches!(journal.as_ref(),JournalV75::Combat{..}|JournalV75::Cast{..}))){return Err("Kaito private journal entered event dispatch".into())}
    let mut log_indices = std::collections::BTreeSet::new();
    for e in &state.engine.event_log {
        let E::KaitoCunningV75 { journal } = e else {
            continue;
        };
        let indices = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, old)| (old == e).then_some(i as u64))
            .collect::<Vec<_>>();
        let [index] = indices.as_slice() else {
            return Err("Kaito current event has no unique original capture".into());
        };
        if !log_indices.insert(*index) {
            return Err("Kaito current event duplicated".into());
        }
        if rows(state).any(|(_, j)| matches!(j,JournalV75::Collected{origin}if origin==index)) {
            return Err("Kaito consumed capture returned to current event dispatch".into());
        }
        match journal.as_ref() {
            JournalV75::Combat { .. } => {
                combat(state, *index)?;
            }
            JournalV75::Cast { .. } => {
                origin(state, *index)?;
            }
            _ => return Err("Kaito private journal dispatched".into()),
        }
    }
    let mut paid = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    let mut collected = std::collections::BTreeSet::new();
    let mut closed = std::collections::BTreeSet::new();
    let mut emblems = std::collections::BTreeSet::new();
    let mut token_inputs = std::collections::BTreeSet::new();
    let mut token_finished = std::collections::BTreeSet::new();
    let mut discard_inputs = std::collections::BTreeSet::new();
    let mut discard_finished = std::collections::BTreeSet::new();
    for (i, j) in rows(state) {
        match j {
            JournalV75::Payment { input } => {
                if input.index != i as u64 {
                    return Err("Kaito payment input moved".into());
                }
                validate_payment(state, input)?;
            }
            JournalV75::Published { input, item } => {
                if input.index >= i as u64
                    || !paid.insert(input.index)
                    || !ids.insert(item.v4.stack_item_id)
                {
                    return Err("Kaito payment/publication reused".into());
                }
                validate_published(state, input, item)?;
            }
            JournalV75::Combat { capture } => {
                if capture.damage >= i as u64
                    || capture.amount <= 0
                    || capture.source.controller != capture.creature.controller
                    || !definition(capture.source.card_def)
                {
                    return Err("Kaito combat input malformed".into());
                }
                let bound = rows(state)
                    .filter(|(_, j)| matches!(j,JournalV75::Bound{capture:k,..}if *k==i as u64))
                    .count();
                if bound > 0 {
                    combat(state, i as u64)?;
                }
            }
            JournalV75::Bound { capture, original } => {
                if *capture >= i as u64
                    || !matches!(at(state,*capture)?,JournalV75::Combat{capture:c}if c==original)
                {
                    return Err("Kaito actual wave original changed".into());
                }
                combat(state, *capture)?;
            }
            JournalV75::Cast { .. } => {
                origin(state, i as u64)?;
            }
            JournalV75::Collected { origin: index } => {
                if *index >= i as u64 || !collected.insert(*index) {
                    return Err("Kaito trigger recollected".into());
                }
                origin(state, *index)?;
            }
            JournalV75::TriggerPublished {
                origin: index,
                item,
            } => {
                if *index >= i as u64 || !ids.insert(item.v4.stack_item_id) {
                    return Err("Kaito trigger publication reused".into());
                }
                validate_trigger(state, item)?;
            }
            JournalV75::Emblem {
                publication,
                item,
                controller,
            } => {
                if *publication >= i as u64
                    || published(state, item)? != *publication
                    || item.inline_effect != Some(emblem())
                    || *controller != item.controller
                    || !emblems.insert(*publication)
                {
                    return Err("Kaito emblem original controller/publication changed".into());
                }
            }
            JournalV75::Evasion {
                publication,
                target,
                timestamp,
            } => {
                let JournalV75::Published { item, .. } = at(state, *publication)? else {
                    return Err("Kaito evasion publication absent".into());
                };
                if target.is_some() != !item.targets.is_empty()
                    || timestamp.is_some_and(|t| t >= state.engine.next_effect_timestamp)
                {
                    return Err(
                        "Kaito evasion omitted its original selected target/timestamp".into(),
                    );
                }
                if *publication>=i as u64||item.inline_effect!=Some(plus_one())||target.is_some()!=timestamp.is_some()||target.as_ref().is_some_and(|b|!item.v4.target_contracts.iter().any(|c|matches!(c,crate::state::StackTargetContractV4::Object{object,zone,zone_change_count,..}if *object==b.object&&*zone==b.expected_zone&&*zone_change_count==b.expected_zone_change_count))){return Err("Kaito evasion original target changed".into())}
            }
            JournalV75::TokenInput { publication, item } => {
                if *publication >= i as u64
                    || published(state, item)? != *publication
                    || !matches!(
                        item.inline_effect,
                        Some(EffectOp::KaitoCunningV75 { kind: 1 | 4, .. })
                    )
                    || !token_inputs.insert(i as u64)
                {
                    return Err("Kaito Ninja input changed".into());
                }
            }
            JournalV75::TokenFinished { input, history_end } => {
                if *input >= *history_end
                    || *history_end != i as u64
                    || !matches!(at(state, *input)?, JournalV75::TokenInput { .. })
                    || !token_finished.insert(*input)
                {
                    return Err("Kaito Ninja completion changed".into());
                }
                let JournalV75::TokenInput { item, .. } = at(state, *input)? else {
                    return Err("Kaito token input kind changed".into());
                };
                let created = state.engine.event_history[*input as usize + 1..i]
                    .iter()
                    .filter_map(|e| match e {
                        E::CreateToken {
                            token_def,
                            controller,
                            ..
                        } => Some((*token_def, *controller)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if created.is_empty()
                    || created
                        .iter()
                        .any(|(id, p)| !token_definition(*id) || *p != item.controller)
                {
                    return Err("Kaito token completion lost its exact native Ninja batch".into());
                }
            }
            JournalV75::DiscardInput {
                publication,
                pending,
                chosen,
                history_start,
            } => {
                if *publication >= i as u64
                    || *history_start != i as u64
                    || pending.count != 1
                    || chosen.len() != 1
                    || !discard_inputs.insert(i as u64)
                {
                    return Err("Kaito original discard input changed".into());
                }
            }
            JournalV75::DiscardFinished { input, history_end } => {
                if *history_end != i as u64 || !discard_finished.insert(*input) {
                    return Err("Kaito discard completion duplicated or moved".into());
                }
                validate_discard(state, *input, *history_end)?;
            }
            JournalV75::Finished { item } | JournalV75::NotResolved { item, .. } => {
                if published(state, item)? >= i as u64
                    || !closed.insert(item.v4.stack_item_id)
                    || crate::engine::time_stop_v1::disposed(state, item)?
                    || state
                        .stack
                        .iter()
                        .any(|r| r.v4.stack_item_id == item.v4.stack_item_id)
                    || state
                        .engine
                        .pending_effect
                        .as_ref()
                        .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
                {
                    return Err("Kaito terminal outcome overlaps live/disposed publication".into());
                }
            }
        }
    }
    if token_inputs != token_finished {
        return Err("Kaito Ninja input not completed".into());
    }
    if discard_inputs != discard_finished {
        return Err("Kaito discard input lacks exact completed departure".into());
    }
    for (publication, j) in rows(state) {
        if let JournalV75::Published { item, .. } | JournalV75::TriggerPublished { item, .. } = j {
            let publication = publication as u64;
            let evasion = rows(state)
                .filter(
                    |(_, j)| matches!(j,JournalV75::Evasion{publication:p,..}if *p==publication),
                )
                .count();
            let emblem = rows(state)
                .filter(|(_, j)| matches!(j,JournalV75::Emblem{publication:p,..}if *p==publication))
                .count();
            let tokens = rows(state)
                .filter(
                    |(_, j)| matches!(j,JournalV75::TokenInput{publication:p,..}if *p==publication),
                )
                .count();
            let done=rows(state).any(|(_,j)|matches!(j,JournalV75::Finished{item:r}if r.v4.stack_item_id==item.v4.stack_item_id));
            let refused=rows(state).any(|(_,j)|matches!(j,JournalV75::NotResolved{item:r,..}if r.v4.stack_item_id==item.v4.stack_item_id))||crate::engine::time_stop_v1::disposed(state,item)?;
            if evasion > 1
                || emblem > 1
                || tokens > 1
                || refused && (evasion + emblem + tokens != 0)
            {
                return Err("Kaito refused/disposed occurrence also claimed an effect".into());
            }
            if done {
                let expected = match item.inline_effect.as_ref() {
                    Some(EffectOp::KaitoCunningV75 { kind: 1 | 4, .. }) => (0, 0, 1),
                    Some(EffectOp::KaitoCunningV75 { kind: 2, .. }) => (0, 1, 0),
                    Some(EffectOp::KaitoCunningV75 { kind: 3, .. }) => (0, 0, 0),
                    Some(op) if *op == plus_one() => (1, 0, 0),
                    _ => return Err("Kaito completed root changed".into()),
                };
                if (evasion, emblem, tokens) != expected {
                    return Err(
                        "Kaito completed operation lost its exact instruction receipts".into(),
                    );
                }
            }
            for current in state
                .stack
                .iter()
                .filter(|r| r.v4.stack_item_id == item.v4.stack_item_id)
            {
                if current != item.as_ref() {
                    return Err("Kaito current row changed".into());
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn require_legacy(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    validate_state(state)?;
    let visible=state.objects.iter().any(|(id,o)|crate::optional_trigger_public_v1::reference(state,id,viewer).is_some_and(|r|matches!(r.identity,crate::optional_trigger_public_v1::IdentityV1::Known{card_id}if definition(card_id)||token_definition(card_id)))||viewer.is_some_and(|v|o.v4.face_down_v1.is_some_and(|f|f.lookers&(1<<v.index())!=0)&&(definition(o.card_def)||token_definition(o.card_def))));
    if visible || rows(state).next().is_some() {
        Err("Kaito and its emblem require a complete V75 consumer".into())
    } else {
        Ok(())
    }
}

pub(crate) fn validate_publication(state: &GameState, item: &StackItem) -> Result<(), String> {
    published(state, item).map(|_| ())
}

/// Read-only projection after unchanged native journal and continuation validation.
/// Maps private journal links to role-specific public ordinals, never raw offsets.
pub(crate) fn public_facts_v75(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::kaito_public_v75::FactsV75, String> {
    use crate::{kaito_public_v75::*, optional_trigger_public_v1 as public};
    use std::collections::BTreeMap;
    validate_state(state)?;
    validate_pending(state)?;
    crate::effect::validate_pending_effect_choice(state)?;
    for item in &state.stack {
        crate::engine::validated_stack_item_target_spec(item, state)?;
    }
    fn ord(n: usize) -> Result<u32, String> {
        u32::try_from(n).map_err(|_| "V75 occurrence exceeds u32".into())
    }
    fn incarnation(c: AbilitySourceContractV4) -> IncarnationV75 {
        IncarnationV75 {
            object: c.source.0,
            generation: c.zone_change_count,
            owner: c.owner.into(),
            controller: c.controller.into(),
            zone: c.zone,
        }
    }
    fn original_current(
        state: &GameState,
        c: &IncarnationV75,
        v: PlayerId,
    ) -> Option<public::ReferenceV1> {
        state
            .objects
            .try_get(ObjectId(c.object))
            .filter(|o| o.zone == c.zone && o.zone_change_count == c.generation)
            .and_then(|_| public::reference(state, ObjectId(c.object), Some(v)))
    }
    fn root(
        state: &GameState,
        c: AbilitySourceContractV4,
        v: PlayerId,
    ) -> Result<public::RootV1, String> {
        let mut r = public::root(state, c, Some(v))?;
        r.original_source.identity = public::IdentityV1::Known {
            card_id: c.card_def,
        };
        Ok(r)
    }
    fn stage(state: &GameState, item: &StackItem) -> Result<StageV75, String> {
        if crate::engine::time_stop_v1::disposed(state, item)? {
            return Ok(StageV75::Disposed);
        }
        for (_, j) in rows(state) {
            match j {
                JournalV75::Finished { item: r } if r.v4.stack_item_id == item.v4.stack_item_id => {
                    return Ok(StageV75::Resolved)
                }
                JournalV75::NotResolved { item: r, countered }
                    if r.v4.stack_item_id == item.v4.stack_item_id =>
                {
                    return Ok(if *countered {
                        StageV75::Countered
                    } else {
                        StageV75::Fizzled
                    })
                }
                _ => {}
            }
        }
        if state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
        {
            return Ok(if state.engine.pending_discard.is_some() {
                StageV75::WaitingDiscard
            } else {
                StageV75::Resolving
            });
        }
        if state.stack.iter().any(|r| r == item) {
            return Ok(StageV75::Published);
        }
        Err("V75 original publication lacks authentic live or terminal role".into())
    }
    let mut f = FactsV75 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        active_player: state.active_player.into(),
        turn_key: state.turn_key_v1(),
        meaning: crate::rules_meaning_kaito_v75::printed_meaning()?,
        authorized_printings: vec![],
        battlefield: [vec![], vec![]],
        activations: vec![],
        combats: vec![],
        emblems: vec![],
        casts: vec![],
        triggers: vec![],
        ninja_batches: vec![],
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    };
    // Printing discovery stays a separate stable arena scan. Complete current
    // battlefield rows follow the native per-seat membership order.
    for (id, _) in state.objects.iter() {
        if let Some(r) = public::reference(state, id, Some(viewer)) {
            if matches!(r.identity, public::IdentityV1::Known { card_id: 494 | 495 }) {
                f.authorized_printings.push(r);
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[seat.index()].battlefield {
            let o = state
                .objects
                .try_get(id)
                .ok_or("V75 battlefield member absent")?;
            if o.zone != Zone::Battlefield || o.controller != seat || !seen.insert(id.0) {
                return Err("V75 battlefield membership changed".into());
            }
            f.battlefield[seat.index()].push(CurrentV75 {
                card: public::reference(state, id, Some(viewer))
                    .ok_or("V75 battlefield reference absent")?,
                is_token: o.v4.is_token,
                tapped: o.tapped,
                summoning_sick: o.summoning_sick,
                marked_damage: o.damage,
                counters: crate::tinybones_public_v72::counters(state, id)?,
                form: public::checked_form(state, id)?,
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, o)| o.zone == Zone::Battlefield && !seen.contains(&id.0))
    {
        return Err("V75 battlefield roster incomplete".into());
    }
    // Native held rows can wait behind legend SBAs or a resolution parent.
    // Reuse the established complete traversal instead of one storage vector.
    let held = public::held(state);
    let mut held_origins = std::collections::BTreeSet::new();
    for row in &held {
        if !recipe(&row.effect) {
            continue;
        }
        let EffectOp::KaitoCunningV75 {
            kind: 3 | 4,
            origin: Some(index),
        } = &row.effect
        else {
            return Err("V75 held Kaito root is not an actual captured trigger".into());
        };
        let mut expected = trigger(state, *index)?;
        expected.placement_ordered = row.placement_ordered;
        if **row != expected
            || !held_origins.insert(*index)
            || !rows(state).any(|(_, j)| matches!(j,JournalV75::Collected{origin}if origin==index))
            || rows(state)
                .any(|(_, j)| matches!(j,JournalV75::TriggerPublished{origin,..}if origin==index))
        {
            return Err("V75 held trigger changed, duplicated or also published".into());
        }
    }
    let mut publications = BTreeMap::new();
    let mut combats = BTreeMap::new();
    let mut emblems = BTreeMap::new();
    let mut casts = BTreeMap::new();
    let mut actual_casts = BTreeMap::new();
    for (at, j) in rows(state) {
        let JournalV75::Published { input, item } = j else {
            continue;
        };
        let occurrence = ord(f.activations.len())?;
        publications.insert(at as u64, PublicationV75::Activation(occurrence));
        let mut targets = vec![];
        for (i, c) in item.v4.target_contracts.iter().enumerate() {
            let crate::state::StackTargetContractV4::Object {
                object,
                owner,
                controller,
                zone,
                zone_change_count,
                ..
            } = *c
            else {
                return Err("V75 loyalty target not object".into());
            };
            let original = IncarnationV75 {
                object: object.0,
                generation: zone_change_count,
                owner: owner.into(),
                controller: controller.into(),
                zone,
            };
            targets.push(TargetV75 {
                current_original: original_current(state, &original, viewer),
                legal_now: crate::engine::target_contract_matches_live(state, c.target(), *c)
                    && crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        input.source,
                        item.controller,
                        input.pending.target_spec,
                        &item.targets,
                        i,
                    ),
                original,
            });
        }
        let mut evasion = None;
        let mut evasion_at = None;
        for (e, j) in rows(state) {
            if let JournalV75::Evasion {
                publication,
                target,
                timestamp,
            } = j
            {
                if *publication == at as u64 {
                    let original = if let Some(b) = target {
                        let t = targets
                            .first()
                            .ok_or("V75 native evasion original target absent")?;
                        if t.original.object != b.object.0
                            || t.original.generation != b.expected_zone_change_count
                        {
                            return Err("V75 evasion original incarnation changed".into());
                        }
                        Some(t.original.clone())
                    } else {
                        None
                    };
                    let active_on_original=target.zip(*timestamp).is_some_and(|(b,stamp)|state.objects.try_get(b.object).is_some_and(|o|o.zone==b.expected_zone&&o.zone_change_count==b.expected_zone_change_count)&&state.engine.until_end_of_turn.iter().any(|e|matches!(e,UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1{object_id,object_zone_change_count,timestamp,..}if *object_id==b.object&&*object_zone_change_count==b.expected_zone_change_count&&*timestamp==stamp)));
                    evasion = Some(EvasionV75 {
                        target: original,
                        active_on_original,
                    });
                    evasion_at = Some(e);
                }
            }
        }
        let mut discard = None;
        let mut end = state.engine.event_history.len();
        for (e, j) in rows(state) {
            match j {
                JournalV75::DiscardInput {
                    publication,
                    pending,
                    chosen,
                    ..
                } if *publication == at as u64 => {
                    end = end.min(e);
                    let private_chosen = if viewer == item.controller {
                        Some(
                            chosen
                                .iter()
                                .map(|b| IncarnationV75 {
                                    object: b.object.0,
                                    generation: b.expected_zone_change_count,
                                    owner: pending.player.into(),
                                    controller: pending.player.into(),
                                    zone: b.expected_zone,
                                })
                                .collect(),
                        )
                    } else {
                        None
                    };
                    discard = Some(DiscardV75 {
                        count: pending.count,
                        answered: true,
                        private_choices: None,
                        private_chosen,
                    });
                }
                JournalV75::Finished { item: r } | JournalV75::NotResolved { item: r, .. }
                    if r.v4.stack_item_id == item.v4.stack_item_id =>
                {
                    end = end.min(e);
                }
                _ => {}
            }
        }
        let pending = state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id);
        if pending {
            if let Some(d) = &state.engine.pending_discard {
                crate::engine::validate_pending_discard_binding(state, d).map_err(|(_, e)| e)?;
                let private_choices = if viewer == d.player {
                    Some(
                        state.players[d.player.index()]
                            .hand
                            .iter()
                            .map(|id| {
                                public::reference(state, *id, Some(viewer))
                                    .ok_or("V75 own discard choice not visible")
                            })
                            .collect::<Result<_, _>>()?,
                    )
                } else {
                    None
                };
                discard = Some(DiscardV75 {
                    count: d.count,
                    answered: false,
                    private_choices,
                    private_chosen: None,
                });
            }
        }
        let draw = if let Some(start) = evasion_at {
            let draws = state.engine.event_history[start + 1..end]
                .iter()
                .filter_map(|e| match e {
                    E::Draw { player, object } if *player == item.controller => {
                        Some(object.is_some())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            match draws.as_slice() {
                [] => DrawV75::NotReached,
                [true] => DrawV75::Drawn,
                [false] => DrawV75::EmptyLibrary,
                _ => return Err("V75 original draw interval ambiguous".into()),
            }
        } else {
            DrawV75::NotReached
        };
        f.activations.push(ActivationV75 {
            occurrence,
            source: root(state, input.source, viewer)?,
            original_turn: input.turn,
            ability_index: input.pending.ability_index,
            loyalty_before: input.before,
            loyalty_after: input.after,
            targets,
            stage: stage(state, item)?,
            draw,
            discard,
            evasion,
        });
    }
    for (at, j) in rows(state) {
        if let JournalV75::Combat { capture } = j {
            combat(state, at as u64)?;
            let occurrence = ord(f.combats.len())?;
            combats.insert(at as u64, occurrence);
            let original_creature = incarnation(capture.creature);
            f.combats.push(CombatV75 {
                occurrence,
                observer: root(state, capture.source, viewer)?,
                current_original_creature: original_current(state, &original_creature, viewer),
                original_creature,
                damaged_player: capture.player.into(),
                final_damage: capture.amount,
            });
        }
    }
    for (at, j) in rows(state) {
        if let JournalV75::Emblem {
            publication,
            controller,
            ..
        } = j
        {
            let Some(PublicationV75::Activation(activation)) =
                publications.get(publication).copied()
            else {
                return Err("V75 emblem original activation absent".into());
            };
            let occurrence = ord(f.emblems.len())?;
            emblems.insert(at as u64, occurrence);
            f.emblems.push(EmblemV75 {
                occurrence,
                activation,
                controller: (*controller).into(),
            });
        }
    }
    for (at, j) in rows(state) {
        if let JournalV75::Cast {
            emblem,
            cast,
            spell,
        } = j
        {
            origin(state, at as u64)?;
            let source = spell
                .v4
                .source_contract
                .ok_or("V75 original cast source absent")?;
            let original_spell = IncarnationV75 {
                object: source.source.0,
                generation: source.zone_change_count,
                owner: source.owner.into(),
                controller: source.controller.into(),
                zone: source.zone,
            };
            let occurrence = ord(f.casts.len())?;
            casts.insert(at as u64, occurrence);
            let next = ord(actual_casts.len())?;
            let actual_cast = *actual_casts.entry(*cast).or_insert(next);
            f.casts.push(CastV75 {
                occurrence,
                emblem: *emblems.get(emblem).ok_or("V75 cast emblem absent")?,
                actual_cast,
                current_original_spell: original_current(state, &original_spell, viewer),
                original_spell,
                x_value: spell.x_value,
                kicked: spell.kicked,
                cast_method: source.cast_method,
            });
        }
    }
    for (at, j) in rows(state) {
        let (origin_kind, source) = match j {
            JournalV75::Combat { capture } => (
                OriginV75::Combat(
                    *combats
                        .get(&(at as u64))
                        .ok_or("V75 combat ordinal absent")?,
                ),
                capture.source,
            ),
            JournalV75::Cast { .. } => (
                OriginV75::EmblemCast(*casts.get(&(at as u64)).ok_or("V75 cast ordinal absent")?),
                origin(state, at as u64)?.1,
            ),
            _ => continue,
        };
        let publication = rows(state).find_map(|(p, j)| match j {
            JournalV75::TriggerPublished { origin, item } if *origin == at as u64 => {
                Some((p, item))
            }
            _ => None,
        });
        let (stage, position) = if let Some((_, item)) = publication {
            let stage = stage(state, item)?;
            let pos = if stage == StageV75::Published {
                Some(ord(state
                    .stack
                    .iter()
                    .position(|r| r == item.as_ref())
                    .ok_or("V75 live trigger position absent")?)?)
            } else {
                None
            };
            (stage, pos)
        } else {
            let expected = trigger(state, at as u64)?;
            if let Some(pos) = held.iter().copied().position(|t| {
                let mut expected = expected.clone();
                expected.placement_ordered = t.placement_ordered;
                t == &expected
            }) {
                (StageV75::Held, Some(ord(pos)?))
            } else if rows(state)
                .any(|(_, j)| matches!(j,JournalV75::Collected{origin}if *origin==at as u64))
            {
                return Err("V75 collected trigger has no authentic held/publication role".into());
            } else {
                (StageV75::Captured, None)
            }
        };
        let occurrence = ord(f.triggers.len())?;
        if let Some((p, _)) = publication {
            publications.insert(p as u64, PublicationV75::Trigger(occurrence));
        }
        f.triggers.push(TriggerV75 {
            occurrence,
            origin: origin_kind,
            source: root(state, source, viewer)?,
            stage,
            native_position: position,
        });
    }
    for (_, j) in rows(state) {
        let JournalV75::TokenFinished { input, history_end } = j else {
            continue;
        };
        let JournalV75::TokenInput { publication, item } = at(state, *input)? else {
            return Err("V75 Ninja input absent".into());
        };
        let mut created = vec![];
        for e in &state.engine.event_history[*input as usize + 1..*history_end as usize] {
            if let E::CreateToken {
                object,
                token_def,
                controller,
                ..
            } = e
            {
                let original = public::ReferenceV1 {
                    object: object.0,
                    generation: 0,
                    owner: (*controller).into(),
                    controller: (*controller).into(),
                    zone: Zone::Battlefield,
                    identity: public::IdentityV1::Known {
                        card_id: *token_def,
                    },
                };
                let c = IncarnationV75 {
                    object: object.0,
                    generation: 0,
                    owner: (*controller).into(),
                    controller: (*controller).into(),
                    zone: Zone::Battlefield,
                };
                created.push(NinjaV75 {
                    original,
                    current_original: original_current(state, &c, viewer),
                });
            }
        }
        f.ninja_batches.push(NinjaBatchV75 {
            occurrence: ord(f.ninja_batches.len())?,
            publication: *publications
                .get(publication)
                .ok_or("V75 Ninja publication ordinal absent")?,
            controller: item.controller.into(),
            created,
        });
    }
    crate::kaito_public_v75::validate(&f)?;
    Ok(f)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "kaito_cunning_v75_tests.rs"]
pub(crate) mod tests;

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "kaito_cunning_public_v75_tests.rs"]
mod public_tests;
