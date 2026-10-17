//! Unregistered native prerequisite for frozen FDN81, Chandra, Flameshaper.
//! Pinned Mage a5c90fe180021e70e2a644ade00eeab07f857a40,
//! Mage.Sets/src/mage/cards/c/ChandraFlameshaper.java. No consumer admission.
use super::*;
use crate::{card_def, event::CommittedEvent as E, ids::StackItemId};
use serde::{Deserialize, Serialize};

pub(crate) fn program(index: usize) -> EffectOp {
    EffectOp::ChandraFlameshaperV74 {
        ability_index: index as u8,
    }
}
pub(crate) fn plus_two() -> EffectOp {
    program(0)
}
pub(crate) fn plus_one() -> EffectOp {
    program(1)
}
pub(crate) fn minus_four() -> EffectOp {
    program(2)
}
pub(crate) fn target(index: usize) -> card_def::TargetSpec {
    match index {
        1 => card_def::TargetSpec::ControlledCreature,
        2 => card_def::TargetSpec::StandardV1(
            crate::standard_cards_v1::StandardTargetV1::UpToEightCreaturesOrPlaneswalkersV74,
        ),
        _ => card_def::TargetSpec::None,
    }
}
fn abilities_exact(abilities: &[card_def::ActivatedAbilityDef]) -> bool {
    abilities.len() == 3
        && abilities.iter().enumerate().all(|(i, a)| {
            a.cost == [card_def::CostComponent::Loyalty([2, 1, -4][i])]
                && a.target_spec == target(i)
                && (a.effect)() == [plus_two(), plus_one(), minus_four()][i]
                && a.activation_zone == Zone::Battlefield
                && a.sorcery_speed_only
                && a.activation_target_filter == card_def::ActivationTargetFilter::TargetSpecOnly
                && a.max_activations_per_turn.is_none()
                && a.face.is_none()
        })
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    matches!(op, EffectOp::ChandraFlameshaperV74 { .. })
}

/// One-unit picks are the actual pre-payment allocation, not additional
/// targets. Every distinct target receives at least one of the eight damage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributionV74 {
    pub allocations: Vec<u8>,
    pub finished: bool,
}
pub(crate) fn seed(state: &GameState, source: ObjectId, index: u8) -> Option<DistributionV74> {
    (definition(state.objects.get(source).card_def) && index == 2).then_some(DistributionV74 {
        allocations: vec![],
        finished: false,
    })
}
pub(crate) fn validate_distribution(
    state: &GameState,
    p: &crate::engine::PendingActivation,
) -> Result<(), String> {
    let selected = state
        .objects
        .try_get(p.source)
        .is_some_and(|o| definition(o.card_def))
        && p.ability_index == 2
        && p.cauldron_grant.0.is_none();
    let Some(d) = &p.chandra_damage_v74 else {
        return if selected {
            Err("Chandra allocation erased".into())
        } else {
            Ok(())
        };
    };
    if !selected
        || d.allocations.len() > 8
        || p.target_spec != target(2)
        || d.allocations.len() != p.targets_chosen.len()
        || d.allocations.contains(&0)
        || d.allocations.iter().map(|n| u16::from(*n)).sum::<u16>() > 8
        || d.finished
            && !d.allocations.is_empty()
            && d.allocations.iter().map(|n| u16::from(*n)).sum::<u16>() != 8
        || !d.finished && d.allocations.iter().map(|n| u16::from(*n)).sum::<u16>() == 8
    {
        return Err("Chandra original positive eight-damage allocation changed".into());
    }
    Ok(())
}
pub(crate) fn allocation_complete(p: &crate::engine::PendingActivation) -> bool {
    p.chandra_damage_v74.as_ref().is_none_or(|d| d.finished)
}
pub(crate) fn allocation_decision(
    state: &GameState,
    p: &crate::engine::PendingActivation,
) -> Option<crate::engine::Decision> {
    let d = p.chandra_damage_v74.as_ref().filter(|d| !d.finished)?;
    Some(crate::engine::Decision::ChooseEffectTargets {
        player: p.controller,
        source: p.source,
        selected_count: p.targets_chosen.len() as u16,
        min_targets: 0,
        max_targets: 8,
        // A repeated pick adds one to the ORIGINAL contract's allocation.
        legal_targets: crate::engine::legal_targets_for_controller(
            target(2),
            &[],
            p.controller,
            state,
        )
        .into_iter()
        .filter(|t| {
            crate::engine::effect_target_is_legal_from_ability_source(
                state,
                AbilitySourceContractV4::capture(state, p.source),
                p.controller,
                target(2),
                &[*t],
                0,
            )
        })
        .collect(),
        can_finish: d.allocations.is_empty(),
    })
}
pub(crate) fn answer_allocation(
    state: &mut GameState,
    action: &crate::engine::Action,
) -> Result<(), String> {
    let p = state
        .engine
        .pending_activation
        .as_ref()
        .ok_or("Chandra activation absent")?
        .clone();
    crate::engine::validate_pending_activation(state, &p)?;
    let Some(crate::engine::Decision::ChooseEffectTargets {
        legal_targets,
        can_finish,
        ..
    }) = allocation_decision(state, &p)
    else {
        return Err("Chandra allocation already finished".into());
    };
    match action {
        crate::engine::Action::ChooseEffectTarget(t) if legal_targets.contains(t) => {
            let contract = StackTargetContractV4::capture(state, *t);
            let p = state.engine.pending_activation.as_mut().unwrap();
            let d = p.chandra_damage_v74.as_mut().unwrap();
            if let Some(i) = p.targets_chosen.iter().position(|old| old == t) {
                d.allocations[i] += 1;
            } else {
                p.targets_chosen.push(*t);
                p.target_contracts.push(contract);
                d.allocations.push(1);
            }
            d.finished = d.allocations.iter().copied().sum::<u8>() == 8;
        }
        crate::engine::Action::FinishEffectSelection if can_finish => {
            state
                .engine
                .pending_activation
                .as_mut()
                .unwrap()
                .chandra_damage_v74
                .as_mut()
                .unwrap()
                .finished = true
        }
        _ => return Err("Chandra requires an actual damage-unit pick or zero targets".into()),
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentV74 {
    index: u64,
    source: AbilitySourceContractV4,
    pending: Box<crate::engine::PendingActivation>,
    before: u32,
    after: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV74 {
    DamageDeparture {
        facts: crate::event::HistoricalDamageSourceV1,
        departure: u64,
    },
    Payment {
        input: Box<PaymentV74>,
    },
    Published {
        input: Box<PaymentV74>,
        item: Box<StackItem>,
    },
    ExileInput {
        publication: u64,
        prefix: Vec<EffectObjectBinding>,
    },
    Exiled {
        input: u64,
        cards: Vec<EffectObjectBinding>,
        public: Vec<crate::rl::CardStableRefV1>,
        moves: Vec<u64>,
    },
    Granted {
        capture: u64,
        chosen: EffectObjectBinding,
        permission: crate::engine::PlayPermission,
    },
    Expired {
        grants: Vec<u64>,
        turn: u32,
    },
    Completed {
        publication: u64,
    },
    NotResolved {
        publication: u64,
        countered: bool,
    },
}
fn rows(state: &GameState) -> impl Iterator<Item = (usize, &JournalV74)> {
    state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::ChandraFlameshaperV74])
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            E::ChandraFlameshaperV74 { journal } => Some((i, journal.as_ref())),
            _ => None,
        })
}
fn append(state: &mut GameState, journal: JournalV74) {
    state.engine.event_history.push(E::ChandraFlameshaperV74 {
        journal: Box::new(journal),
    });
}
fn at(state: &GameState, index: u64) -> Result<&JournalV74, String> {
    match usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    {
        Some(E::ChandraFlameshaperV74 { journal }) => Ok(journal),
        _ => Err("Chandra original journal absent".into()),
    }
}
pub(crate) fn record_input(
    state: &mut GameState,
    p: &crate::engine::PendingActivation,
) -> Result<Option<u64>, String> {
    let source = AbilitySourceContractV4::capture(state, p.source);
    if !definition(source.card_def) || p.cauldron_grant.0.is_some() {
        return Ok(None);
    }
    validate_distribution(state, p)?;
    if p.ability_index >= 3 || !allocation_complete(p) {
        return Err("Chandra activation is incomplete".into());
    }
    let before =
        crate::planeswalker_v1::loyalty(state, p.source).ok_or("Chandra loyalty absent")?;
    let after = crate::planeswalker_v1::quote_loyalty_change_v48(
        state,
        p.source,
        [2, 1, -4][usize::from(p.ability_index)],
        crate::doubling_season_v48::CounterOriginV48::Cost,
    )?;
    let index = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV74::Payment {
            input: Box::new(PaymentV74 {
                index,
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
    index: Option<u64>,
) -> Result<(), String> {
    let Some(index) = index else { return Ok(()) };
    let JournalV74::Payment { input } = at(state, index)? else {
        return Err("Chandra payment input kind changed".into());
    };
    let input = input.clone();
    let item = state
        .stack
        .iter()
        .find(|r| r.v4.stack_item_id == id)
        .ok_or("Chandra paid row absent")?
        .clone();
    if crate::planeswalker_v1::loyalty(state, item.source) != Some(input.after) {
        return Err("Chandra actual loyalty debit changed".into());
    }
    validate_published(state, &input, &item)?;
    append(
        state,
        JournalV74::Published {
            input,
            item: Box::new(item),
        },
    );
    Ok(())
}
fn publication(state: &GameState, item: &StackItem) -> Result<(u64, PaymentV74), String> {
    let matches = rows(state)
        .filter_map(|(i, j)| match j {
            JournalV74::Published { input, item: old }
                if old.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some((i, input, old))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(i, input, old)] = matches.as_slice() else {
        return Err("Chandra unique original publication absent".into());
    };
    if !same_published_row(state, old, item) {
        return Err("Chandra whole native row changed".into());
    }
    Ok((*i as u64, input.as_ref().clone()))
}
fn same_published_row(state: &GameState, original: &StackItem, item: &StackItem) -> bool {
    if original == item {
        return true;
    }
    let mut unstamped = item.clone();
    unstamped.v4.crystal_prevention_started_v1 = Default::default();
    original == &unstamped
        && state.engine.event_history.iter().any(|e| {
            matches!(e,
        E::CrystalPreventionBoundaryV1 { boundary } if boundary.item.as_ref() == item
            && boundary.root == minus_four())
        })
}
/// Only the original paid allocation supplies batch amounts. This returns no
/// legality permission; the actual batch still checks each original contract.
pub(super) fn damage_allocations(state: &GameState, ctx: &ExecCtx) -> Result<Vec<u8>, String> {
    let original = rows(state)
        .filter_map(|(_, j)| match j {
            JournalV74::Published { input, item }
                if Some(item.v4.stack_item_id) == ctx.stack_item_id =>
            {
                Some((input, item))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(input, item)] = original.as_slice() else {
        return Err("Chandra damage lost unique paid row".into());
    };
    validate_published(state, input, item)?;
    if item.inline_effect != Some(minus_four()) || original_context(item, state) != *ctx {
        return Err("Chandra damage changed original whole context".into());
    }
    Ok(input
        .pending
        .chandra_damage_v74
        .as_ref()
        .ok_or("Chandra paid division absent")?
        .allocations
        .clone())
}
pub(super) fn damage_target_legal(state: &GameState, ctx: &ExecCtx, slot: usize) -> bool {
    ctx.target_incarnation_matches(slot, state)
        && ctx.ability_source_contract.is_some_and(|source| {
            crate::engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                ctx.controller,
                target(2),
                &ctx.targets,
                slot,
            )
        })
}
pub(super) fn crystal_root(state: &GameState, item: &StackItem) -> Result<EffectOp, String> {
    let (_, input) = publication(state, item)?;
    if input.pending.ability_index != 2 {
        return Err("Chandra Crystal route is damage only".into());
    }
    Ok(minus_four())
}
pub(super) fn finish_crystal(
    state: &mut GameState,
    owner: &super::crystal_prevention_v1::OwnerV1,
    item: &StackItem,
) -> Result<(), String> {
    if !selected(state, item) {
        return Ok(());
    }
    if !owner.completion_matches(item) {
        return Err("Chandra Crystal owner changed item".into());
    }
    let (publication, input) = publication(state, item)?;
    if input.pending.ability_index != 2
        || state.engine.pending_effect.is_some()
        || !state.engine.event_history.iter().any(|e| {
            matches!(e,E::CrystalExecutionV1{execution}
            if execution.item.as_ref()==item && execution.after.is_none())
        })
    {
        return Err("Chandra Crystal completion lost actual complete execution".into());
    }
    append(state, JournalV74::Completed { publication });
    Ok(())
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    item.inline_effect.as_ref().is_some_and(recipe) || rows(state).any(|(_,j)|matches!(j,JournalV74::Published{item:old,..}if old.v4.stack_item_id==item.v4.stack_item_id))
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
pub(crate) fn context_selected(state: &GameState, ctx: &ExecCtx) -> bool {
    if has_pending_exile(state) {
        return true;
    }
    waiting(state,ctx)||rows(state).any(|(_,j)|matches!(j,JournalV74::Published{item,..}if Some(item.v4.stack_item_id)==ctx.stack_item_id || ctx.ability_source_contract.is_some()&&ctx.ability_source_contract==item.v4.ability_source_contract))
}
fn validate_published(state: &GameState, p: &PaymentV74, item: &StackItem) -> Result<(), String> {
    let n = usize::from(p.pending.ability_index);
    if n >= 3
        || !definition(p.source.card_def)
        || p.source.zone != Zone::Battlefield
        || p.pending.source != p.source.source
        || p.pending.source_zone_change_count != p.source.zone_change_count
        || p.pending.controller != p.source.controller
        || p.pending.target_spec != target(n)
        || p.pending.cauldron_grant.0.is_some()
        || p.pending.loyalty_x.is_some()
        || p.pending.mana_x_v1.is_some()
        || !p.pending.object_cost_chosen.is_empty()
        || p.pending.crew_finished
        || p.pending.cost_discard_paid.as_deref() != Some(&[])
        || p.pending.targets_chosen.len() != p.pending.target_contracts.len()
        || p.pending
            .targets_chosen
            .iter()
            .filter_map(|t| {
                if let Target::Object(id) = t {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != p.pending.targets_chosen.len()
        || (n != 2 && p.pending.targets_chosen.len() != usize::from(n == 1))
        || (n == 2
            && p.pending.chandra_damage_v74.as_ref().is_none_or(|d| {
                !d.finished
                    || d.allocations.len() > 8
                    || d.allocations.len() != p.pending.targets_chosen.len()
                    || d.allocations.contains(&0)
                    || !d.allocations.is_empty()
                        && d.allocations.iter().map(|n| u16::from(*n)).sum::<u16>() != 8
            }))
        || n != 2 && p.pending.chandra_damage_v74.is_some()
        || !matches!(at(state,p.index)?,JournalV74::Payment{input}if input.as_ref()==p)
        || item.kind != StackItemKind::ActivatedAbility
        || item.source != p.source.source
        || item.controller != p.source.controller
        || item.v4.ability_source_contract != Some(p.source)
        || item.inline_effect != Some(program(n))
        || item.targets != p.pending.targets_chosen
        || item.v4.target_contracts != p.pending.target_contracts
        || item.v4.target_spec != Some(target(n))
        || item.v4.activated_ability_index != Some(n as u8)
        || item.v4.source_contract.is_some()
        || item.v4.granted_by.is_some()
        || item.is_copy
        || item.is_flashback
        || item.kicked
        || item.madness_offer
        || item.mode_chosen != 0
        || !item.discarded.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
        || (n == 2 && p.before.checked_sub(4) != Some(p.after))
        || (n != 2 && p.after <= p.before)
    {
        return Err("Chandra original paid source/program/target/allocation changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, p.source)?;
    Ok(())
}

fn capture(state: &GameState, index: u64) -> Result<(u64, &[EffectObjectBinding]), String> {
    let JournalV74::Exiled {
        input,
        cards,
        public,
        moves,
    } = at(state, index)?
    else {
        return Err("Chandra exile capture kind changed".into());
    };
    let JournalV74::ExileInput {
        publication,
        prefix,
    } = at(state, *input)?
    else {
        return Err("Chandra original exile input absent".into());
    };
    let JournalV74::Published { item, .. } = at(state, *publication)? else {
        return Err("Chandra exile publication absent".into());
    };
    if item.inline_effect != Some(plus_two())
        || *publication >= *input
        || *input >= index
        || prefix.len() > 3
        || cards.len() != moves.len()
        || public.len() != cards.len()
        || cards.len() != prefix.len()
    {
        return Err("Chandra original exile batch changed".into());
    }
    for (i, ((old, card), event)) in prefix.iter().zip(cards).zip(moves).enumerate() {
        if public[i].owner != item.controller
            || public[i].controller != item.controller
            || card_def::CARD_DEFS
                .get(public[i].card_db_id as usize)
                .is_none()
            || public[i].arena_id != card.object.0
            || public[i].zone != Zone::Exile
            || public[i].zone_change_count != card.expected_zone_change_count
            || old.expected_zone != Zone::Library
            || card.object != old.object
            || card.expected_zone != Zone::Exile
            || old.expected_zone_change_count.checked_add(1)
                != Some(card.expected_zone_change_count)
            || *event <= *input
            || *event >= index
            || i > 0 && moves[i - 1] >= *event
            || !matches!(state.engine.event_history.get(*event as usize),Some(E::ZoneChange{object,from:Zone::Library,to:Zone::Exile,controller_before})if *object==old.object&&*controller_before==item.controller)
        {
            return Err("Chandra original exile departure or generation changed".into());
        }
    }
    Ok((*publication, cards))
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    validate_state_with_removed(state, None)
}
fn validate_state_with_removed(
    state: &GameState,
    removed: Option<&StackItem>,
) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, E::ChandraFlameshaperV74 { .. }))
    {
        return Err("Chandra private journal entered dispatch".into());
    }
    let mut inputs = std::collections::BTreeSet::new();
    let mut publications = std::collections::BTreeSet::new();
    let mut completions = std::collections::BTreeSet::new();
    let mut not_resolved = std::collections::BTreeSet::new();
    let mut grants = std::collections::BTreeSet::new();
    let mut expired = std::collections::BTreeSet::new();
    for (i, j) in rows(state) {
        match j {
            JournalV74::DamageDeparture { facts, departure } => {
                validate_departure(state, *facts, *departure, i as u64)?;
            }
            JournalV74::Payment { input } => {
                if input.index != i as u64 {
                    return Err("Chandra payment input moved".into());
                }
            }
            JournalV74::Published { input, item } => {
                if input.index >= i as u64
                    || !inputs.insert(input.index)
                    || !publications.insert(item.v4.stack_item_id)
                {
                    return Err("Chandra payment or publication reused".into());
                }
                validate_published(state, input, item)?;
                for row in state
                    .stack
                    .iter()
                    .filter(|r| r.v4.stack_item_id == item.v4.stack_item_id)
                {
                    if !same_published_row(state, item, row) {
                        return Err("Chandra current row changed".into());
                    }
                }
            }
            JournalV74::ExileInput {
                publication,
                prefix,
            } => {
                if *publication >= i as u64
                    || prefix.len() > 3
                    || prefix
                        .iter()
                        .map(|b| b.object)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != prefix.len()
                {
                    return Err("Chandra original exile input changed".into());
                }
                if !matches!(at(state,*publication)?,JournalV74::Published{item,..}if item.inline_effect==Some(plus_two()))
                {
                    return Err("Chandra exile input changed producer".into());
                }
            }
            JournalV74::Exiled { .. } => {
                capture(state, i as u64)?;
            }
            JournalV74::Granted {
                capture: cap,
                chosen,
                permission,
            } => {
                let (_, cards) = capture(state, *cap)?;
                let JournalV74::Exiled { input, .. } = at(state, *cap)? else {
                    unreachable!()
                };
                let JournalV74::ExileInput { publication, .. } = at(state, *input)? else {
                    unreachable!()
                };
                let JournalV74::Published { item, .. } = at(state, *publication)? else {
                    unreachable!()
                };
                let JournalV74::Exiled { public, .. } = at(state, *cap)? else {
                    unreachable!()
                };
                let original = public
                    .iter()
                    .find(|r| r.arena_id == chosen.object.0)
                    .ok_or("Chandra chosen public printing absent")?;
                let d = card_def::CARD_DEFS
                    .get(original.card_db_id as usize)
                    .ok_or("Chandra chosen printing absent")?;
                if permission.play_or_cast
                    != if d.is_playable_land() {
                        crate::engine::PlayOrCast::Play
                    } else {
                        crate::engine::PlayOrCast::Cast
                    }
                    || *cap >= i as u64
                    || !cards.contains(chosen)
                    || !grants.insert(*cap)
                    || permission.object != chosen.object
                    || permission.zone_change_generation != chosen.expected_zone_change_count
                    || permission.holder != item.controller
                    || permission.without_mana_cost.0
                    || permission.expiry != crate::engine::PlayPermissionExpiry::EndOfTurn
                {
                    return Err("Chandra chosen exact this-turn permission changed".into());
                }
            }
            JournalV74::Expired { grants, turn } => {
                if grants.is_empty() || *turn > state.turn_key_v1() {
                    return Err("Chandra cleanup authority changed".into());
                }
                for grant in grants {
                    if *grant >= i as u64
                        || !expired.insert(*grant)
                        || !matches!(at(state, *grant)?, JournalV74::Granted { .. })
                    {
                        return Err("Chandra grant cleanup changed or repeated".into());
                    }
                }
            }
            JournalV74::NotResolved { publication, .. } => {
                if *publication >= i as u64
                    || !not_resolved.insert(*publication)
                    || !matches!(at(state, *publication)?, JournalV74::Published { .. })
                {
                    return Err("Chandra nonresolution changed its unique publication".into());
                }
            }
            JournalV74::Completed { publication } => {
                let JournalV74::Published { item, .. } = at(state, *publication)? else {
                    return Err("Chandra completion lost publication".into());
                };
                if item.inline_effect == Some(plus_two()) {
                    let captures = rows(state)
                        .filter_map(|(at, j)| match j {
                            JournalV74::Exiled { .. }
                                if capture(state, at as u64)
                                    .is_ok_and(|(p, _)| p == *publication) =>
                            {
                                Some(at as u64)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    let [cap] = captures.as_slice() else {
                        return Err("Chandra completion lost original exile batch".into());
                    };
                    let (_, cards) = capture(state, *cap)?;
                    let granted = rows(state)
                        .filter(|(_, j)| matches!(j,JournalV74::Granted{capture,..}if capture==cap))
                        .count();
                    if granted != usize::from(!cards.is_empty()) {
                        return Err(
                            "Chandra completion lost its one required chosen permission".into()
                        );
                    }
                }
                if *publication >= i as u64
                    || !completions.insert(*publication)
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
                    return Err("Chandra completed/disposed/live outcomes overlap".into());
                }
            }
        }
    }
    for (i, j) in rows(state) {
        let JournalV74::Published { item, .. } = j else {
            continue;
        };
        let live = state
            .stack
            .iter()
            .filter(|r| r.v4.stack_item_id == item.v4.stack_item_id)
            .count();
        let removed = usize::from(removed.is_some_and(|r| same_published_row(state, item, r)));
        let completed = usize::from(completions.contains(&(i as u64)));
        let not_resolved = usize::from(not_resolved.contains(&(i as u64)));
        let disposed = usize::from(crate::engine::time_stop_v1::disposed(state, item)?);
        if live + removed + completed + not_resolved + disposed != 1 {
            return Err(
                "Chandra live/resolved/nonresolved/disposed ownership overlaps or was erased"
                    .into(),
            );
        }
    }
    for (i, j) in rows(state) {
        if let JournalV74::Granted { permission, .. } = j {
            let count = state
                .engine
                .exile_play_permissions
                .iter()
                .filter(|p| {
                    p.object == permission.object
                        && p.zone_change_generation == permission.zone_change_generation
                        && p.holder == permission.holder
                })
                .collect::<Vec<_>>();
            if expired.contains(&(i as u64)) {
                if count.contains(&permission) {
                    return Err("Chandra expired permission remains live".into());
                }
            } else if count != [permission] {
                return Err("Chandra live permission erased or altered".into());
            }
        }
    }
    Ok(())
}
pub(crate) fn cleanup(state: &mut GameState) -> Result<(), String> {
    let grants = rows(state)
        .filter_map(|(i, j)| match j {
            JournalV74::Granted { permission, .. }
                if state.engine.exile_play_permissions.contains(permission) =>
            {
                Some(i as u64)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if !grants.is_empty() {
        append(
            state,
            JournalV74::Expired {
                grants,
                turn: state.turn_key_v1(),
            },
        );
    }
    Ok(())
}
fn original_context(item: &StackItem, state: &GameState) -> ExecCtx {
    let mut ctx = super::crystal_prevention_v1::context(item);
    ctx.niv_damage_origin_v1 = event::niv_damage_origin_for_saved_item_v1(state, item);
    ctx
}
pub(crate) fn validate_pending(state: &GameState) -> Result<(), String> {
    for (input, j) in rows(state) {
        if let JournalV74::ExileInput { publication, .. } = j {
            let captures = rows(state)
                .filter_map(|(i, j)| match j {
                    JournalV74::Exiled {
                        input: original, ..
                    } if *original == input as u64 => Some(i as u64),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [cap] = captures.as_slice() else {
                return Err("Chandra required original exile capture erased or duplicated".into());
            };
            let completed = rows(state).any(
                |(_, j)| matches!(j,JournalV74::Completed{publication:old}if old==publication),
            );
            if !completed {
                let pending = state
                    .engine
                    .pending_effect
                    .as_ref()
                    .ok_or("Chandra required exile selection erased")?;
                if self::publication(state, &pending.resolving_item)?.0 != *publication {
                    return Err("Chandra selection substituted another native occurrence".into());
                }
                let (found, _) = capture(state, *cap)?;
                if found != *publication {
                    return Err("Chandra capture original publication changed".into());
                }
                validate_continuation(state, pending)?;
            }
        }
    }

    if let Some(p) = state
        .engine
        .pending_effect
        .as_ref()
        .filter(|p| selected(state, &p.resolving_item))
    {
        validate_continuation(state, p)?;
    }
    Ok(())
}
fn validate_continuation(state: &GameState, p: &EffectContinuation) -> Result<(), String> {
    let (pub_at, _) = publication(state, &p.resolving_item)?;
    if p.ctx != original_context(&p.resolving_item, state) {
        return Err("Chandra full pending context changed".into());
    }
    if !p.resolving_item.v4.crystal_prevention_started_v1.is_empty() {
        if p.resolving_item.inline_effect != Some(minus_four()) {
            return Err("Chandra nondamage root borrowed Crystal continuation".into());
        }
        return super::crystal_prevention_v1::validate(state);
    }
    match (&p.choice, p.frames.as_slice(), &p.answered_choice_guard) {
        (None, [EffectFrame::Program { op, path }], None)
            if *op
                == program(
                    p.resolving_item
                        .v4
                        .activated_ability_index
                        .ok_or("Chandra ability absent")? as usize,
                )
                && path.is_empty() =>
        {
            Ok(())
        }
        (
            Some(PendingEffectChoice::SelectTargets {
                player,
                path,
                selected,
                legal,
                min_targets: 1,
                max_targets: 1,
                ordered: false,
                purpose: EffectTargetSelectionPurpose::ChandraExileV74 { capture: cap },
            }),
            [],
            None,
        ) => {
            let (original, cards) = capture(state, *cap)?;
            let mut candidates = cards.iter().copied().map(candidate).collect::<Vec<_>>();
            if original != pub_at
                || *player != p.ctx.controller
                || !path.is_empty()
                || cards.len() < 2
                || selected.len() > 1
            {
                return Err("Chandra public exile chooser changed".into());
            }
            for chosen in selected {
                let i = candidates
                    .iter()
                    .position(|c| c == chosen)
                    .ok_or("Chandra chosen card outside original batch")?;
                candidates.remove(i);
            }
            if candidates != *legal {
                return Err("Chandra original public exile candidates changed".into());
            }
            for c in cards {
                validate_effect_object_binding(state, *c)?;
            }
            Ok(())
        }
        (
            None,
            [f @ EffectFrame::ChandraExileGrantV74 {
                capture: cap,
                chosen,
            }],
            Some(EffectAnsweredChoiceGuard::StandardSelection { frame }),
        ) if frame.as_ref() == f => {
            let (original, cards) = capture(state, *cap)?;
            if original != pub_at || !cards.contains(chosen) {
                return Err("Chandra exile answer changed".into());
            }
            for c in cards {
                validate_effect_object_binding(state, *c)?;
            }
            Ok(())
        }
        _ => Err("Chandra retained continuation lost its required native stage".into()),
    }
}
/// Called solely by the existing genuine native removal paths after exact
/// stack/source/target validation; this does not grant public execution.
pub(crate) fn record_not_resolved(
    state: &mut GameState,
    item: &StackItem,
    countered: bool,
) -> Result<(), String> {
    if !selected(state, item) {
        return Ok(());
    }
    let (publication, _) = publication(state, item)?;
    if state
        .stack
        .iter()
        .any(|r| r.v4.stack_item_id == item.v4.stack_item_id)
        || state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Chandra nonresolution still has a live continuation".into());
    }
    append(
        state,
        JournalV74::NotResolved {
            publication,
            countered,
        },
    );
    Ok(())
}
fn candidate(b: EffectObjectBinding) -> EffectTargetCandidate {
    EffectTargetCandidate {
        target: Target::Object(b.object),
        expected_object: Some(b),
    }
}
pub(super) fn answer(
    p: &mut EffectContinuation,
    capture: u64,
    objects: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<(), String> {
    if !path.is_empty() || objects.len() != 1 || !p.frames.is_empty() {
        return Err("Chandra exile answer cardinality changed".into());
    }
    let frame = EffectFrame::ChandraExileGrantV74 {
        capture,
        chosen: objects[0],
    };
    p.answered_choice_guard = Some(EffectAnsweredChoiceGuard::StandardSelection {
        frame: Box::new(frame.clone()),
    });
    p.frames.push(frame);
    Ok(())
}
pub(super) fn apply_grant(
    state: &mut GameState,
    p: &EffectContinuation,
    cap: u64,
    chosen: EffectObjectBinding,
) -> Result<(), String> {
    let (publication, cards) = capture(state, cap)?;
    if publication != self::publication(state, &p.resolving_item)?.0
        || !cards.contains(&chosen)
        || rows(state).any(|(_, j)| matches!(j,JournalV74::Granted{capture,..}if *capture==cap))
    {
        return Err("Chandra capture reused or answer changed".into());
    }
    for b in cards {
        validate_effect_object_binding(state, *b)?;
    }
    let d = &card_def::CARD_DEFS[state.objects.get(chosen.object).card_def as usize];
    {
        let permission = crate::engine::PlayPermission {
            without_mana_cost: Default::default(),
            object: chosen.object,
            holder: p.ctx.controller,
            zone_change_generation: chosen.expected_zone_change_count,
            play_or_cast: if d.is_playable_land() {
                crate::engine::PlayOrCast::Play
            } else {
                crate::engine::PlayOrCast::Cast
            },
            expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
        };
        state.engine.exile_play_permissions.push(permission);
        append(
            state,
            JournalV74::Granted {
                capture: cap,
                chosen,
                permission,
            },
        );
    }
    append(state, JournalV74::Completed { publication });
    Ok(())
}
pub(super) fn begin(
    state: &mut GameState,
    p: &mut EffectContinuation,
    index: u8,
    owner: &NativeExecutionOwnerV1,
) -> Result<bool, String> {
    owner.validate_entry(state, &p.resolving_item, &p.ctx, &program(index as usize))?;
    validate_state_with_removed(state, Some(&owner.item))?;
    let (publication, payment) = self::publication(state, &p.resolving_item)?;
    let payment = payment.clone();
    if rows(state)
        .any(|(_, j)| matches!(j,JournalV74::Completed{publication:old}if *old==publication))
    {
        return Err("Chandra paid occurrence already consumed".into());
    }
    match index {
        0 => {
            // The existing native pool is u8 per color. Refuse overflow before
            // any resolving effect mutation instead of dropping red mana.
            state.players[p.ctx.controller.index()].mana_pool
                [crate::mana::ManaColor::R.pool_index()]
            .checked_add(3)
            .ok_or("Chandra red mana exceeds native pool width")?;
            let mut staged = state.clone();
            event::propose_and_commit(
                &mut staged,
                event::ProposedEvent::mana_add(
                    p.ctx.controller,
                    vec![crate::mana::ManaColor::R; 3],
                ),
            );
            if staged.engine.halted.is_some() {
                return Err("Chandra mana event halted".into());
            }
            let prefix = bind_library_top(&staged, p.ctx.controller, 3);
            let input = staged.engine.event_history.len() as u64;
            append(
                &mut staged,
                JournalV74::ExileInput {
                    publication,
                    prefix: prefix.clone(),
                },
            );
            let start = staged.engine.event_history.len();
            commit_zone_change_batch(&mut staged, &prefix, Zone::Exile, false)?;
            let mut cards = Vec::new();
            let mut moves = Vec::new();
            for old in &prefix {
                let o = staged.objects.get(old.object);
                if o.zone != Zone::Exile
                    || old.expected_zone_change_count.checked_add(1) != Some(o.zone_change_count)
                {
                    return Err("Chandra exile replacement outside supported batch".into());
                }
                cards.push(EffectObjectBinding {
                    object: old.object,
                    expected_zone: Zone::Exile,
                    expected_zone_change_count: o.zone_change_count,
                });
                moves.push(staged.engine.event_history.iter().enumerate().skip(start).find_map(|(i,e)|matches!(e,E::ZoneChange{object,from:Zone::Library,to:Zone::Exile,..}if *object==old.object).then_some(i as u64)).ok_or("Chandra exile departure absent")?);
            }
            let cap = staged.engine.event_history.len() as u64;
            let public = cards
                .iter()
                .map(|b| crate::rl::card_ref(&staged, b.object).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            append(
                &mut staged,
                JournalV74::Exiled {
                    input,
                    cards: cards.clone(),
                    public,
                    moves,
                },
            );
            if cards.len() > 1 {
                p.choice = Some(PendingEffectChoice::SelectTargets {
                    player: p.ctx.controller,
                    path: vec![],
                    selected: vec![],
                    legal: cards.into_iter().map(candidate).collect(),
                    min_targets: 1,
                    max_targets: 1,
                    ordered: false,
                    purpose: EffectTargetSelectionPurpose::ChandraExileV74 { capture: cap },
                });
                *state = staged;
                return Ok(true);
            }
            if let Some(chosen) = cards.first() {
                apply_grant(&mut staged, p, cap, *chosen)?
            } else {
                append(&mut staged, JournalV74::Completed { publication });
            }
            *state = staged;
        }
        1 => {
            if let Some(proposal) = copy_proposal(state, &p.ctx)? {
                let proof = NativeExecutionProofV1 { owner, ctx: &p.ctx };
                let permit = CopyPermitV74 {
                    proof: &proof,
                    proposal,
                };
                event::commit_chandra_copy_v74(state, permit)?;
            }
            append(state, JournalV74::Completed { publication });
        }
        2 => {
            let allocations = &payment
                .pending
                .chandra_damage_v74
                .as_ref()
                .ok_or("Chandra paid allocation erased")?
                .allocations;
            let mut events = Vec::new();
            for (i, amount) in allocations.iter().copied().enumerate() {
                if p.ctx.target_incarnation_matches(i, state)
                    && crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        payment.source,
                        p.ctx.controller,
                        target(2),
                        &p.ctx.targets,
                        i,
                    )
                {
                    let Target::Object(object) = p.ctx.targets[i] else {
                        return Err("Chandra damage target became nonpermanent".into());
                    };
                    let mut proposal = event::damage_from_exec_ctx_v1(
                        &p.ctx,
                        Target::Object(object),
                        i32::from(amount),
                    );
                    if let event::ProposedEvent::Damage(d) = &mut proposal {
                        d.historical_source = p.ctx.stack_item_id;
                    }
                    events.push(proposal);
                }
            }
            event::propose_and_commit_batch(state, events);
            if state.engine.halted.is_some() {
                return Err("Chandra damage batch halted".into());
            }
            append(state, JournalV74::Completed { publication });
        }
        _ => return Err("Chandra unknown printed ability".into()),
    }
    Ok(false)
}
fn copy_proposal(state: &GameState, ctx: &ExecCtx) -> Result<Option<event::ProposedEvent>, String> {
    let source = ctx
        .ability_source_contract
        .ok_or("Chandra original source contract erased")?;
    if !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source,
            ctx.controller,
            target(1),
            &ctx.targets,
            0,
        )
    {
        return Ok(None);
    }
    let Some(Target::Object(object)) = ctx.targets.first().copied() else {
        return Err("Chandra copy target absent".into());
    };
    let live = state.objects.get(object);
    let (definition, face, count) = if live.v4.face_down_v1.is_some() {
        (
            card_def::card_id_by_name(crate::self_reflection_v1::ANONYMOUS_BLUEPRINT)
                .filter(|id| crate::self_reflection_v1::anonymous_blueprint_is_exact(*id))
                .ok_or("anonymous copy blueprint absent")?,
            0,
            0,
        )
    } else {
        (
            live.card_def,
            live.v4.face_index,
            event::copiable_token_abilities_v1(state, object)?,
        )
    };
    let mut event = event::ProposedEvent::create_token(definition, ctx.controller);
    if let event::ProposedEvent::CreateToken(t) = &mut event {
        t.entry_face_index = (face != 0).then_some(face);
        t.initialization_v48 = Some(crate::doubling_season_v48::TokenEntryV48 {
            copied: true,
            copied_end_step_sacrifices_v1: count
                .checked_add(1)
                .ok_or("Chandra copied ability count overflow")?,
            copied_nightmare_v1: harvester_v1::copiable_nightmare(state, object)?,
            ..Default::default()
        });
    }
    crate::doubling_season_v48::preflight_entry_roster(state, std::slice::from_ref(&event))?;
    crate::doubling_season_v48::preflight_token_initialization(
        state,
        std::slice::from_ref(&event),
    )?;
    Ok(Some(event))
}
pub(crate) struct CopyPermitV74<'a, 'b> {
    proof: &'a NativeExecutionProofV1<'b>,
    proposal: event::ProposedEvent,
}
impl CopyPermitV74<'_, '_> {
    pub(crate) fn events(&self) -> Vec<event::ProposedEvent> {
        vec![self.proposal.clone()]
    }
    pub(crate) fn allows(&self, state: &GameState, events: &[event::ProposedEvent]) -> bool {
        let owner = self.proof.owner;
        owner.ctx == *self.proof.ctx
            && owner.root.as_ref() == &plus_one()
            && state.stack == owner.remaining_stack
            && publication(state, &owner.item).is_ok()
            && copy_proposal(state, self.proof.ctx).ok().flatten().as_ref() == Some(&self.proposal)
            && events == self.events()
    }
}
pub(crate) fn require_legacy(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    validate_state(state)?;
    if rows(state).next().is_some() || state.objects.iter().any(|(id,o)| {
        crate::optional_trigger_public_v1::reference(state,id,viewer).is_some_and(|r|matches!(r.identity,crate::optional_trigger_public_v1::IdentityV1::Known{card_id}if definition(card_id)))
        || viewer.is_some_and(|v|o.v4.face_down_v1.is_some_and(|f|f.lookers&(1<<v.index())!=0)&&definition(o.card_def))
    }) {
        Err("Chandra requires its complete V74 consumer".into())
    } else {
        Ok(())
    }
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 493
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
            d.name == "Chandra, Flameshaper"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::R),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::R),
                        ],
                        generic: 5,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Planeswalker]
                && d.subtypes == [card_def::Subtype::Chandra]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power.is_none()
                && d.toughness.is_none()
                && d.starting_loyalty == Some(6)
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::R]
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
                && d.mana_value == 7
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

pub(crate) fn has_damage_publication(state: &GameState, id: StackItemId) -> bool {
    rows(state).any(|(_,j)|matches!(j,JournalV74::Published{item,..} if item.v4.stack_item_id==id && item.inline_effect==Some(minus_four())))
}
pub(crate) fn departure_input(
    state: &GameState,
    event: &event::ProposedEvent,
) -> Option<event::HistoricalDamageSourceV1> {
    let event::ProposedEvent::ZoneChange(change) = event else {
        return None;
    };
    let o = state.objects.try_get(change.object)?;
    if o.zone != Zone::Battlefield || change.to_zone == Zone::Battlefield {
        return None;
    }
    rows(state).any(|(_,j)|matches!(j,JournalV74::Published{item,..}if item.inline_effect==Some(minus_four()) && item.source==change.object && item.v4.ability_source_contract.is_some_and(|c|c.zone_change_count==o.zone_change_count)))
        .then(||crate::source_power_activation_v1::capture_damage_source(state,change.object))
}
fn validate_departure(
    state: &GameState,
    facts: event::HistoricalDamageSourceV1,
    departure: u64,
    marker: u64,
) -> Result<(), String> {
    if departure >= marker
        || !matches!(state.engine.event_history.get(departure as usize),Some(E::ZoneChange{object,from:Zone::Battlefield,to,controller_before})if *object==facts.source.source&&*to!=Zone::Battlefield&&*controller_before==facts.source.controller)
    {
        return Err("Chandra original damage-source departure changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, facts.source)?;
    if !rows(state).any(|(_,j)|matches!(j,JournalV74::Published{item,..}if item.inline_effect==Some(minus_four())&&item.source==facts.source.source&&item.v4.ability_source_contract.is_some_and(|c|c.zone_change_count==facts.source.zone_change_count&&c.owner==facts.source.owner))){return Err("Chandra departure lost its original damage occurrence".into())}
    Ok(())
}
pub(crate) fn record_departure(
    state: &mut GameState,
    input: Option<event::HistoricalDamageSourceV1>,
    departure: usize,
) -> Result<(), String> {
    if let Some(facts) = input {
        validate_departure(
            state,
            facts,
            departure as u64,
            state.engine.event_history.len() as u64,
        )?;
        append(
            state,
            JournalV74::DamageDeparture {
                facts,
                departure: departure as u64,
            },
        );
    }
    Ok(())
}
pub(crate) fn damage_source(
    state: &GameState,
    object: ObjectId,
    id: StackItemId,
) -> Result<event::HistoricalDamageSourceV1, String> {
    let originals = rows(state)
        .filter_map(|(_, j)| match j {
            JournalV74::Published { item, .. }
                if item.v4.stack_item_id == id
                    && item.source == object
                    && item.inline_effect == Some(minus_four()) =>
            {
                Some(item.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [item] = originals.as_slice() else {
        return Err("Chandra unique damage publication absent".into());
    };
    let original = item
        .v4
        .ability_source_contract
        .ok_or("Chandra damage source absent")?;
    if state.objects.try_get(object).is_some_and(|o| {
        o.zone == Zone::Battlefield && o.zone_change_count == original.zone_change_count
    }) {
        return Ok(crate::source_power_activation_v1::capture_damage_source(
            state, object,
        ));
    }
    let departures = rows(state)
        .filter_map(|(i, j)| match j {
            JournalV74::DamageDeparture { facts, departure }
                if facts.source.source == object
                    && facts.source.zone_change_count == original.zone_change_count =>
            {
                Some((i, *facts, *departure))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(marker, facts, departure)] = departures.as_slice() else {
        return Err("Chandra exact last damage-source incarnation absent or duplicated".into());
    };
    validate_departure(state, *facts, *departure, *marker as u64)?;
    Ok(*facts)
}

fn has_pending_exile(state: &GameState) -> bool {
    rows(state).any(|(_,j)|matches!(j,JournalV74::ExileInput{publication,..}if !rows(state).any(|(_,other)|matches!(other,JournalV74::Completed{publication:done}if done==publication))))
}
