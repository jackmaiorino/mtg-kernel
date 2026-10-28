//! Paid activation and operation ownership for the Partial Ravenous candidate.
//! Registration enables real printing checks; Full admission remains unqualified.
use crate::event_history_v1::EventKindV1;
use crate::{
    card_def::{
        ActivationTargetFilter, CardType, CostComponent, PermanentFilter, TargetSpec, CARD_DEFS,
    },
    effect::{EffectOp, PlayerRef},
    engine::PendingActivation,
    event::CommittedEvent,
    ids::StackItemId,
    mana::Cost,
    state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackStateV4, Zone},
};
use serde::{Deserialize, Serialize};

pub fn draw_program() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::AddSoulCounterToAbilitySourceV1,
    ])
}
pub fn loss_program() -> EffectOp {
    EffectOp::LoseLifeEachOpponentEqualSourceSoulV1
}

pub(crate) fn definition(id: u16) -> bool {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return false;
    }
    CARD_DEFS.get(usize::from(id)).is_some_and(owns_definition)
        && crate::trigger::triggers_for(id).is_empty()
}

/// Older session modes cannot represent the paid ownership or exact counters.
/// Hidden printings alone do not change their visible contract.
pub(crate) fn require_legacy_consumer(
    state: &GameState,
    actor: Option<crate::ids::PlayerId>,
) -> Result<(), String> {
    if crate::soul_counters_v1::has_history(state)
        || state
            .objects
            .iter()
            .any(|(_, object)| object.zone == Zone::Battlefield && object.counters.soul != 0)
    {
        return Err("Ravenous ownership and soul counters require V51".into());
    }
    let known = state.objects.iter().any(|(id, object)| {
        let hand_known = object.zone == Zone::Hand
            && actor.is_some_and(|actor| {
                actor == object.owner
                    || state
                        .known_hand_cards(actor, object.owner)
                        .iter()
                        .any(|entry| {
                            entry.object == id
                                && entry.zone_change_count == object.zone_change_count
                        })
            });
        let library_known = object.zone == Zone::Library
            && actor.is_some_and(|actor| {
                state
                    .known_library_cards(actor, object.owner)
                    .iter()
                    .any(|entry| {
                        entry.object == id && entry.zone_change_count == object.zone_change_count
                    })
            });
        let visible = hand_known
            || library_known
            || matches!(
                object.zone,
                Zone::Battlefield | Zone::Graveyard | Zone::Stack | Zone::Exile | Zone::Command
            );
        let definition_known = object.v4.face_down_v1.is_none()
            || actor.is_some_and(|actor| {
                object
                    .v4
                    .face_down_v1
                    .is_some_and(|face| face.lookers & (1 << actor.index()) != 0)
            });
        visible && definition_known && definition(object.card_def)
    });
    if known {
        Err("Ravenous Amulet printing requires V51".into())
    } else {
        Ok(())
    }
}

pub(crate) fn owns_definition(d: &crate::card_def::CardDef) -> bool {
    let [draw, loss] = d.activated_abilities else {
        return false;
    };
    let mana = |generic| Cost {
        pips: &[],
        generic,
        x_count: 0,
    };
    d.is_executable()
        && d.name == "Ravenous Amulet"
        && d.object_name == d.name
        && d.types == [CardType::Artifact]
        && d.colors.is_empty()
        && d.subtypes.is_empty()
        && d.supertypes.is_empty()
        && !d.is_land
        && !d.is_token
        && d.cost == mana(2)
        && d.mana_value == 2
        && d.power.is_none()
        && d.toughness.is_none()
        && d.keywords == crate::card_def::Keywords::NONE
        && d.target_spec == TargetSpec::None
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: crate::effect::ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
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
        && draw.cost
            == [
                CostComponent::Mana(mana(1)),
                CostComponent::Tap,
                CostComponent::SacrificeControlled {
                    count: 1,
                    filter: PermanentFilter::Creature,
                },
            ]
        && loss.cost
            == [
                CostComponent::Mana(mana(4)),
                CostComponent::Tap,
                CostComponent::SacrificeSelf,
            ]
        && (draw.effect)() == draw_program()
        && (loss.effect)() == loss_program()
        && draw.sorcery_speed_only
        && !loss.sorcery_speed_only
        && [draw, loss].iter().all(|a| {
            a.activation_zone == Zone::Battlefield
                && a.target_spec == TargetSpec::None
                && a.activation_target_filter == ActivationTargetFilter::TargetSpecOnly
                && a.max_activations_per_turn.is_none()
                && a.face.is_none()
        })
}

/// Never serialized or reconstructed from a pending snapshot. The payment
/// branch obtains this before moving its cost objects, then consumes it once.
pub(crate) struct PaymentOwnerV1 {
    source: AbilitySourceContractV4,
    sacrifice: AbilitySourceContractV4,
    ability_index: u8,
    original_soul_count: Option<i32>,
    history_start: usize,
}

pub(crate) fn capture_payment(
    state: &GameState,
    pending: &PendingActivation,
) -> Result<Option<PaymentOwnerV1>, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(None);
    }
    let live = state
        .objects
        .try_get(pending.source)
        .ok_or("activation source absent")?;
    let Some(d) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return Ok(None);
    };
    if d.name != "Ravenous Amulet"
        || pending.ability_index > 1
        || pending.cauldron_grant.0.is_some()
    {
        return Ok(None);
    }
    if !definition(live.card_def)
        || live.zone != Zone::Battlefield
        || live.zone_change_count != pending.source_zone_change_count
        || live.controller != pending.controller
        || live.tapped
        || live.counters.soul < 0
        || pending.target_spec != TargetSpec::None
        || !pending.targets_chosen.is_empty()
        || !pending.target_contracts.is_empty()
    {
        return Err("Amulet changed its original printed payment input".into());
    }
    let sacrifice = if pending.ability_index == 0 {
        let [chosen] = pending.object_cost_chosen.as_slice() else {
            return Err("Amulet needs one creature cost".into());
        };
        let creature = state
            .objects
            .try_get(chosen.object)
            .ok_or("Amulet creature cost absent")?;
        if chosen.expected_zone != Zone::Battlefield
            || creature.zone != Zone::Battlefield
            || creature.zone_change_count != chosen.expected_zone_change_count
            || creature.controller != pending.controller
            || !crate::engine::object_has_type(state, chosen.object, CardType::Creature)
        {
            return Err("Amulet creature payment changed incarnation/controller/type".into());
        }
        AbilitySourceContractV4::capture(state, chosen.object)
    } else {
        if !pending.object_cost_chosen.is_empty() {
            return Err("Amulet self cost gained a chosen object".into());
        }
        AbilitySourceContractV4::capture(state, pending.source)
    };
    Ok(Some(PaymentOwnerV1 {
        source: AbilitySourceContractV4::capture(state, pending.source),
        sacrifice,
        ability_index: pending.ability_index,
        original_soul_count: (pending.ability_index == 1).then_some(live.counters.soul),
        history_start: state.engine.event_history.len(),
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidV1 {
    pub index: u64,
    pub original_source: AbilitySourceContractV4,
    pub original_sacrifice: AbilitySourceContractV4,
    /// Only the self-sacrifice ability reads this value. Draw/placement uses
    /// the current resolution world, which may have changed in response.
    pub original_soul_count: Option<i32>,
    pub payment_history_start: u64,
    pub tap_history_index: u64,
    pub sacrificed_history_index: u64,
    pub sacrifice_history_index: u64,
    pub item: Box<StackItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalKindV1 {
    Resolving,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovedV1 {
    pub index: u64,
    pub paid: u64,
    pub item: Box<StackItem>,
    pub kind: RemovalKindV1,
    pub source_present: bool,
    /// Canonical whole world acquired while the exact paid row was live.
    /// Histories are linked through this removal's outer prefix, not nested.
    pub original_world: Box<GameState>,
    pub replay_borrow: Option<crate::linked_aura_operation_v1::RavenousReplayBorrowV51>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OperationV1 {
    Countered,
    Draw { draw: u64, soul: Option<(u64, u64)> },
    Loss { event: u64 },
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedV1 {
    pub index: u64,
    pub removed: u64,
    pub operation: OperationV1,
    pub output_world: Box<GameState>,
}
/// Exact live-row loan acquired before the engine removes the actual row.
/// No public/snapshot constructor can supply this one-use capability.
pub(crate) struct OwnerV1 {
    paid: u64,
    item: StackItem,
    history_index: u64,
    world: Box<GameState>,
    replay_borrow: crate::linked_aura_operation_v1::RavenousReplayBorrowV51,
}

fn operation_world(state: &GameState) -> Result<GameState, String> {
    let mut world = state.clone();
    world.engine.event_history.clear();
    world.engine.event_log.clear();
    let bytes = serde_json::to_vec(&world).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn after_removal(
    original: &GameState,
    item: &StackItem,
    kind: RemovalKindV1,
) -> Result<GameState, String> {
    if !original.engine.event_history.is_empty() || !original.engine.event_log.is_empty() {
        return Err("Amulet original operation world contains nested journals".into());
    }
    let positions = original
        .stack
        .iter()
        .enumerate()
        .filter_map(|(index, live)| {
            (live.v4.stack_item_id == item.v4.stack_item_id).then_some((index, live))
        })
        .collect::<Vec<_>>();
    let [(position, live)] = positions.as_slice() else {
        return Err("Amulet original operation world lost unique live row".into());
    };
    if *live != item
        || (kind == RemovalKindV1::Resolving
            && position.checked_add(1) != Some(original.stack.len()))
    {
        return Err("Amulet original operation changed its whole live row/top".into());
    }
    let mut world = original.clone();
    world.stack.remove(*position);
    if kind == RemovalKindV1::Resolving {
        world.engine.pending_kicked_source = None;
    }
    Ok(world)
}

fn paid_for(state: &GameState, id: StackItemId) -> Result<&PaidV1, String> {
    let mut rows = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::RavenousPaymentV1])
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::RavenousPaymentV1 { paid } if paid.item.v4.stack_item_id == id => {
                Some(paid.as_ref())
            }
            _ => None,
        });
    let paid = rows.next().ok_or("Amulet actual payment absent")?;
    if rows.next().is_some() {
        return Err("Amulet actual payment duplicated".into());
    }
    validate_paid(state, paid)?;
    Ok(paid)
}
fn tracked(state: &GameState, item: &StackItem) -> bool {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return false;
    }
    printed_item(item) || item.inline_effect == Some(draw_program()) || item.inline_effect == Some(loss_program())
        || state.engine.event_history.scan_for_v1(&[EventKindV1::RavenousPaymentV1]).iter().any(|event| matches!(event, CommittedEvent::RavenousPaymentV1 { paid } if paid.item.v4.stack_item_id == item.v4.stack_item_id))
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    if !tracked(state, item) {
        return Ok(());
    }
    let paid = paid_for(state, item.v4.stack_item_id)?;
    if paid.item.as_ref() != item || state.engine.event_history.scan_for_v1(&[EventKindV1::RavenousRemovedV1]).iter().any(|event| matches!(event, CommittedEvent::RavenousRemovedV1 { removed } if removed.paid == paid.index)) {
        return Err("Amulet current row changed or replayed completion".into());
    }
    Ok(())
}
pub(crate) fn acquire(state: &GameState, item: &StackItem) -> Result<Option<OwnerV1>, String> {
    if !tracked(state, item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    let live = state
        .stack
        .iter()
        .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
        .collect::<Vec<_>>();
    if live.as_slice() != [item] {
        return Err("Amulet removal lost exact unique live row".into());
    }
    Ok(Some(OwnerV1 {
        paid: paid_for(state, item.v4.stack_item_id)?.index,
        item: item.clone(),
        history_index: state.engine.event_history.len() as u64,
        world: Box::new(operation_world(state)?),
        replay_borrow: crate::linked_aura_operation_v1::capture_ravenous_replay_borrow_v51(state),
    }))
}
fn remove_owned(
    state: &mut GameState,
    owner: &OwnerV1,
    kind: RemovalKindV1,
) -> Result<u64, String> {
    validate_item(state, &owner.item)?;
    if state.engine.event_history.len() as u64 != owner.history_index
        || operation_world(state)? != after_removal(&owner.world, &owner.item, kind)?
    {
        return Err("Amulet removed operation changed its complete original world".into());
    }
    if state
        .stack
        .iter()
        .any(|item| item.v4.stack_item_id == owner.item.v4.stack_item_id)
        || paid_for(state, owner.item.v4.stack_item_id)?.index != owner.paid
    {
        return Err("Amulet operation lacks genuine removed-row loan".into());
    }
    let source = owner
        .item
        .v4
        .ability_source_contract
        .ok_or("Amulet original source absent")?;
    let source_present = state.objects.try_get(source.source).is_some_and(|live| {
        live.zone == source.zone && live.zone_change_count == source.zone_change_count
    });
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::RavenousRemovedV1 {
            removed: Box::new(RemovedV1 {
                index,
                paid: owner.paid,
                item: Box::new(owner.item.clone()),
                kind,
                source_present,
                original_world: owner.world.clone(),
                replay_borrow: (kind == RemovalKindV1::Resolving)
                    .then(|| owner.replay_borrow.clone()),
            }),
        });
    Ok(index)
}
fn complete(state: &mut GameState, removed: u64, operation: OperationV1) -> Result<(), String> {
    let index = state.engine.event_history.len() as u64;
    let output_world = Box::new(operation_world(state)?);
    state
        .engine
        .event_history
        .push(CommittedEvent::RavenousCompletedV1 {
            completed: Box::new(CompletedV1 {
                index,
                removed,
                operation,
                output_world,
            }),
        });
    crate::soul_counters_v1::validate(state)
}
pub(crate) fn countered(state: &mut GameState, owner: Option<OwnerV1>) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    let removed = remove_owned(state, &owner, RemovalKindV1::Countered)?;
    complete(state, removed, OperationV1::Countered)
}
pub(crate) fn resolve(
    state: &mut GameState,
    owner: OwnerV1,
    ctx: &crate::effect::ExecCtx,
) -> Result<(), String> {
    let mut expected = crate::effect::ExecCtx::no_targets(owner.item.source, owner.item.controller);
    expected.stack_item_id = Some(owner.item.v4.stack_item_id);
    expected.paid_cost_refs = owner.item.v4.paid_cost_refs.clone();
    expected.ability_source_contract = owner.item.v4.ability_source_contract;
    if ctx != &expected {
        return Err("Amulet resolving context changed its whole paid input".into());
    }
    let removed = remove_owned(state, &owner, RemovalKindV1::Resolving)?;
    let start = state.engine.event_history.len();
    crate::effect::execute(
        owner
            .item
            .inline_effect
            .as_ref()
            .ok_or("Amulet program absent")?,
        ctx,
        state,
    );
    if state.engine.halted.is_some() || state.engine.pending_effect.is_some() {
        return Err("Amulet atomic program halted or suspended".into());
    }
    let operation = if owner.item.v4.activated_ability_index == Some(0) {
        let draws = state.engine.event_history[start..]
            .iter()
            .enumerate()
            .filter_map(|(offset, event)| {
                matches!(event, CommittedEvent::Draw { player, .. } if *player == ctx.controller)
                    .then_some((start + offset) as u64)
            })
            .collect::<Vec<_>>();
        let [draw] = draws.as_slice() else {
            return Err("Amulet did not commit its one draw attempt".into());
        };
        let souls = state.engine.event_history[start..]
            .iter()
            .enumerate()
            .filter_map(|(offset, event)| match event {
                CommittedEvent::SoulCounterAddedV1 {
                    input_history_index,
                    source,
                    ..
                } if Some(*source) == ctx.ability_source_contract => {
                    Some((*input_history_index, (start + offset) as u64))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let soul = match souls.as_slice() {
            [] => None,
            [pair] => Some(*pair),
            _ => return Err("Amulet duplicated soul placement".into()),
        };
        OperationV1::Draw { draw: *draw, soul }
    } else {
        let losses = state.engine.event_history[start..].iter().enumerate().filter_map(|(offset,event)| matches!(event, CommittedEvent::LifeLoss { player, .. } if *player == ctx.controller.opponent()).then_some((start+offset) as u64)).collect::<Vec<_>>();
        let [event] = losses.as_slice() else {
            return Err("Amulet did not commit one untargeted loss".into());
        };
        OperationV1::Loss { event: *event }
    };
    complete(state, removed, operation)
}

fn validate_completion(
    state: &GameState,
    paid: &PaidV1,
    removed: &RemovedV1,
    completed: &CompletedV1,
) -> Result<(), String> {
    let history = &state.engine.event_history;
    let at = |index| usize::try_from(index).ok().and_then(|i| history.get(i));
    if removed.paid != paid.index
        || removed.item != paid.item
        || removed.index <= paid.index
        || completed.removed != removed.index
        || completed.index <= removed.index
        || !matches!(at(removed.index), Some(CommittedEvent::RavenousRemovedV1 { removed: original }) if original.as_ref() == removed)
        || !matches!(at(completed.index), Some(CommittedEvent::RavenousCompletedV1 { completed: original }) if original.as_ref() == completed)
    {
        return Err("Amulet completion lost its original paid removal".into());
    }
    let start = usize::try_from(removed.index)
        .ok()
        .and_then(|i| i.checked_add(1))
        .ok_or("Amulet removal index overflow")?;
    let end = usize::try_from(completed.index).map_err(|_| "Amulet completion index overflow")?;
    let rows = history
        .get(start..end)
        .ok_or("Amulet operation history range absent")?;
    validate_operation_world(state, paid, removed, completed, rows)?;
    let draws = rows
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            matches!(event, CommittedEvent::Draw { .. }).then_some((start + offset) as u64)
        })
        .collect::<Vec<_>>();
    let souls = rows
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| match event {
            CommittedEvent::SoulCounterAddedV1 {
                input_history_index,
                ..
            } => Some((*input_history_index, (start + offset) as u64)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let inputs = rows
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            matches!(event, CommittedEvent::SoulCounterInputV1 { .. })
                .then_some((start + offset) as u64)
        })
        .collect::<Vec<_>>();
    let losses = rows
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            matches!(event, CommittedEvent::LifeLoss { .. }).then_some((start + offset) as u64)
        })
        .collect::<Vec<_>>();
    match &completed.operation {
        OperationV1::Countered => {
            if removed.kind != RemovalKindV1::Countered || !rows.is_empty() {
                return Err("countered Amulet invented an effect".into());
            }
        }
        OperationV1::Draw { draw, soul } => {
            if removed.kind != RemovalKindV1::Resolving
                || paid.item.v4.activated_ability_index != Some(0)
                || draws.as_slice() != [*draw]
                || !losses.is_empty()
                || !matches!(at(*draw), Some(CommittedEvent::Draw { player, .. }) if *player == paid.original_source.controller)
            {
                return Err("Amulet draw changed its actual actor/attempt".into());
            }
            match soul {
                None if !removed.source_present && souls.is_empty() && inputs.is_empty() => {}
                Some((input_at, added_at))
                    if removed.source_present
                        && *draw < *input_at
                        && input_at.checked_add(1) == Some(*added_at)
                        && souls.as_slice() == [(*input_at, *added_at)]
                        && inputs.as_slice() == [*input_at] =>
                {
                    let Some(CommittedEvent::SoulCounterInputV1 { input }) = at(*input_at) else {
                        return Err("Amulet soul original world absent".into());
                    };
                    let Some(CommittedEvent::SoulCounterAddedV1 {
                        source,
                        player,
                        input_history_index,
                        ..
                    }) = at(*added_at)
                    else {
                        return Err("Amulet soul placement absent".into());
                    };
                    if Some(*source) != paid.item.v4.ability_source_contract
                        || input.source != *source
                        || input.actor != paid.item.controller
                        || *player != paid.item.controller
                        || *input_history_index != *input_at
                    {
                        return Err("Amulet soul input redirected its paid source/actor".into());
                    }
                }
                _ => {
                    return Err(
                        "Amulet soul completion changed the original source-presence input".into(),
                    )
                }
            }
        }
        OperationV1::Loss { event } => {
            if removed.kind != RemovalKindV1::Resolving
                || paid.item.v4.activated_ability_index != Some(1)
                || !draws.is_empty()
                || !souls.is_empty()
                || !inputs.is_empty()
                || losses.as_slice() != [*event]
                || !matches!(at(*event), Some(CommittedEvent::LifeLoss { player, amount }) if *player == paid.item.controller.opponent() && Some(*amount) == paid.original_soul_count)
            {
                return Err(
                    "Amulet loss changed original controller/count or gained targets".into(),
                );
            }
        }
    }
    Ok(())
}

fn validate_operation_world(
    state: &GameState,
    paid: &PaidV1,
    removed: &RemovedV1,
    completed: &CompletedV1,
    rows: &[CommittedEvent],
) -> Result<(), String> {
    if !completed.output_world.engine.event_history.is_empty()
        || !completed.output_world.engine.event_log.is_empty()
    {
        return Err("Amulet output world contains nested journals".into());
    }
    let mut replay = after_removal(&removed.original_world, &paid.item, removed.kind)?;
    let source = paid
        .item
        .v4
        .ability_source_contract
        .ok_or("Amulet original operation source absent")?;
    let present = replay.objects.try_get(source.source).is_some_and(|live| {
        live.zone == source.zone && live.zone_change_count == source.zone_change_count
    });
    if present != removed.source_present {
        return Err("Amulet operation changed its original source presence".into());
    }
    let start = usize::try_from(removed.index)
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or("Amulet original operation prefix overflow")?;
    // Reattach the actual outer prefix. Running stable validators against a
    // stripped snapshot alone would erase required prior Fake/Season custody.
    replay.engine.event_history = state
        .engine
        .event_history
        .get(..start)
        .ok_or("Amulet original operation prefix absent")?
        .to_vec()
        .into();
    if removed.kind == RemovalKindV1::Resolving {
        let borrow = removed
            .replay_borrow
            .as_ref()
            .ok_or("Amulet original replay scope absent")?;
        let mut ctx = crate::effect::ExecCtx::no_targets(paid.item.source, paid.item.controller);
        ctx.stack_item_id = Some(paid.item.v4.stack_item_id);
        ctx.paid_cost_refs = paid.item.v4.paid_cost_refs.clone();
        ctx.ability_source_contract = paid.item.v4.ability_source_contract;
        crate::linked_aura_operation_v1::with_ravenous_replay_borrow_v51(
            &mut replay,
            &paid.item,
            borrow,
            |world| {
                crate::effect::execute(
                    paid.item
                        .inline_effect
                        .as_ref()
                        .ok_or("Amulet original program absent")?,
                    &ctx,
                    world,
                );
                if world.engine.halted.is_some() || world.engine.pending_effect.is_some() {
                    return Err("Amulet original atomic operation halted or suspended".into());
                }
                Ok(())
            },
        )?;
    } else if removed.replay_borrow.is_some() {
        return Err("Countered Amulet invented a resolution scope".into());
    }
    if replay.engine.event_history.get(start..) != Some(rows)
        || operation_world(&replay)? != *completed.output_world
    {
        return Err("Amulet complete original-world operation replay differs".into());
    }
    Ok(())
}

fn printed_item(item: &StackItem) -> bool {
    item.v4
        .activated_ability_index
        .is_some_and(|index| index < 2)
        && item.v4.cauldron_grant.0.is_none()
        && item.v4.granted_by.is_none()
        && item
            .v4
            .ability_source_contract
            .is_some_and(|source| definition(source.card_def))
}

/// Historical payment structure is checked without following current source
/// control or a returned incarnation. This is not an operation-completion proof.
fn validate_paid(state: &GameState, paid: &PaidV1) -> Result<(), String> {
    let item = paid.item.as_ref();
    let source = paid.original_source;
    let sacrifice = paid.original_sacrifice;
    let [cost] = item.v4.paid_cost_refs.as_slice() else {
        return Err("Amulet receipt lost its single sacrifice".into());
    };
    let mut published_source = source;
    if sacrifice.source == source.source {
        published_source.attached_to = None;
    }
    let role = item
        .v4
        .activated_ability_index
        .ok_or("Amulet receipt lost printed index")?;
    let expected = StackItem {
        kind: StackItemKind::ActivatedAbility,
        source: source.source,
        controller: source.controller,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(if role == 0 {
            draw_program()
        } else {
            loss_program()
        }),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: item.v4.stack_item_id,
            paid_cost_refs: item.v4.paid_cost_refs.clone(),
            ability_source_contract: Some(published_source),
            target_spec: Some(TargetSpec::None),
            activated_ability_index: Some(role),
            ..Default::default()
        },
    };
    if !printed_item(item)
        || item != &expected
        || item.v4.stack_item_id == StackItemId::default()
        || source.zone != Zone::Battlefield
        || sacrifice.zone != Zone::Battlefield
        || sacrifice.controller != source.controller
        || if role == 1 {
            paid.original_soul_count.is_none_or(|count| count < 0)
        } else {
            paid.original_soul_count.is_some()
        }
        || (role == 1 && sacrifice != source)
        || (sacrifice.source == source.source && sacrifice != source)
        || cost.object != sacrifice.source
        || cost.owner != sacrifice.owner
        || sacrifice.zone_change_count.checked_add(1) != Some(cost.zone_change_count)
        || cost.zone == Zone::Battlefield
        || !(paid.payment_history_start <= paid.tap_history_index
            && paid.tap_history_index < paid.sacrificed_history_index
            && paid.sacrificed_history_index < paid.sacrifice_history_index
            && paid.sacrifice_history_index < paid.index)
    {
        return Err("Amulet payment receipt changed its printed source/root/cost".into());
    }
    let at = |index| {
        usize::try_from(index)
            .ok()
            .and_then(|i| state.engine.event_history.get(i))
    };
    if !matches!(at(paid.index), Some(CommittedEvent::RavenousPaymentV1 { paid: original }) if original.as_ref() == paid)
        || !matches!(at(paid.tap_history_index), Some(CommittedEvent::Tap { object }) if *object == source.source)
        || !matches!(at(paid.sacrificed_history_index), Some(CommittedEvent::Sacrificed { object, controller_before, .. }) if *object == sacrifice.source && *controller_before == source.controller)
        || !matches!(at(paid.sacrifice_history_index), Some(CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before }) if *object == sacrifice.source && *to == cost.zone && *controller_before == source.controller)
    {
        return Err("Amulet payment receipt lost its actual indexed cost occurrences".into());
    }
    // A self-sacrificed original generation has stable LKI even after return.
    // Other-creature payments may later gain more soul counters legitimately.
    match paid.original_soul_count {
        Some(count) if state.soul_count_for_ability_source_v1(source)? != count => {
            return Err("Amulet self-sacrifice rewrote its original soul count".into());
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    let mut ids = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::RavenousRemovedV1 { removed } => {
                if removed.index != index as u64
                    || !matches!(usize::try_from(removed.paid).ok().and_then(|at| state.engine.event_history.get(at)), Some(CommittedEvent::RavenousPaymentV1 { paid }) if paid.item == removed.item)
                {
                    return Err("Amulet removal lost its actual original payment position".into());
                }
            }
            CommittedEvent::RavenousCompletedV1 { completed } => {
                if completed.index != index as u64
                    || !matches!(
                        usize::try_from(completed.removed)
                            .ok()
                            .and_then(|at| state.engine.event_history.get(at)),
                        Some(CommittedEvent::RavenousRemovedV1 { .. })
                    )
                {
                    return Err("Amulet completion lost its original removal position".into());
                }
            }
            _ => {}
        }
        let CommittedEvent::RavenousPaymentV1 { paid } = event else {
            continue;
        };
        if paid.index != index as u64 || ids.contains(&paid.item.v4.stack_item_id) {
            return Err("Amulet payment has duplicate/reordered authority".into());
        }
        validate_paid(state, paid)?;
        ids.push(paid.item.v4.stack_item_id);
        let live = state
            .stack
            .iter()
            .filter(|item| item.v4.stack_item_id == paid.item.v4.stack_item_id)
            .collect::<Vec<_>>();
        let removals = state
            .engine
            .event_history
            .iter()
            .filter_map(|event| match event {
                CommittedEvent::RavenousRemovedV1 { removed } if removed.paid == paid.index => {
                    Some(removed.as_ref())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        match (live.as_slice(), removals.as_slice()) {
            ([item], []) if *item == paid.item.as_ref() => {}
            ([], []) if crate::engine::time_stop_v1::disposed(state, &paid.item)? => {}
            ([], [removed]) => {
                let completions = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|event| match event {
                        CommittedEvent::RavenousCompletedV1 { completed }
                            if completed.removed == removed.index =>
                        {
                            Some(completed.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [completed] = completions.as_slice() else {
                    return Err("Amulet removed row lost unique completion".into());
                };
                validate_completion(state, paid, removed, completed)?;
            }
            _ => {
                return Err("Amulet paid row was lost, duplicated or replayed after removal".into())
            }
        }
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::RavenousPaymentV1 { .. }
                | CommittedEvent::RavenousRemovedV1 { .. }
                | CommittedEvent::RavenousCompletedV1 { .. }
        )
    }) {
        return Err("private Amulet payment entered trigger log".into());
    }
    Ok(())
}

/// Called after the generic engine has successfully collected the entire cost.
/// Payment failure never calls this, and a copied/borrowed activation cannot
/// manufacture the private original source/count token.
pub(crate) fn record_paid(
    state: &mut GameState,
    id: StackItemId,
    owner: Option<PaymentOwnerV1>,
) -> Result<(), String> {
    let mut published = state.stack.iter().filter(|i| i.v4.stack_item_id == id);
    let item = published.next().ok_or("paid activation absent")?;
    if published.next().is_some() {
        return Err("paid activation has duplicate finalized identities".into());
    }
    let selected = printed_item(item);
    let Some(owner) = owner else {
        return if selected {
            Err("Amulet lost private prepayment capture".into())
        } else {
            Ok(())
        };
    };
    let mut source = owner.source;
    if state.objects.get(source.source).zone_change_count != source.zone_change_count {
        source.attached_to = None;
    }
    let program = if owner.ability_index == 0 {
        draw_program()
    } else {
        loss_program()
    };
    let [cost] = item.v4.paid_cost_refs.as_slice() else {
        return Err("Amulet lost its single paid sacrifice".into());
    };
    if !selected
        || item.kind != StackItemKind::ActivatedAbility
        || item.is_copy
        || item.source != owner.source.source
        || item.controller != owner.source.controller
        || item.v4.ability_source_contract != Some(source)
        || item.v4.activated_ability_index != Some(owner.ability_index)
        || item.inline_effect != Some(program)
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || item.v4.target_spec != Some(TargetSpec::None)
        || item.v4.granted_by.is_some()
        || item.v4.cauldron_grant.0.is_some()
        || id == StackItemId::default()
        || cost.object != owner.sacrifice.source
        || cost.owner != owner.sacrifice.owner
        || owner.sacrifice.zone_change_count.checked_add(1) != Some(cost.zone_change_count)
        || cost.zone == Zone::Battlefield
        || owner.original_soul_count.is_some_and(|count| {
            state.soul_count_for_ability_source_v1(owner.source).ok() != Some(count)
        })
    {
        return Err("Amulet finalized publication changed its private payment".into());
    }
    let mut taps = Vec::new();
    let mut markers = Vec::new();
    let mut sacrifices = Vec::new();
    for (offset, event) in state.engine.event_history[owner.history_start..]
        .iter()
        .enumerate()
    {
        match event {
            CommittedEvent::Tap { object } if *object == owner.source.source => {
                taps.push(owner.history_start + offset)
            }
            CommittedEvent::Sacrificed {
                object,
                controller_before,
                ..
            } if *object == owner.sacrifice.source
                && *controller_before == owner.source.controller =>
            {
                markers.push(owner.history_start + offset)
            }
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to,
                controller_before,
            } if *object == owner.sacrifice.source
                && *to == cost.zone
                && *controller_before == owner.source.controller =>
            {
                sacrifices.push(owner.history_start + offset)
            }
            _ => {}
        }
    }
    let ([tap], [marker], [sacrifice]) =
        (taps.as_slice(), markers.as_slice(), sacrifices.as_slice())
    else {
        return Err("Amulet payment lost unique tap/departure".into());
    };
    if tap >= marker || marker >= sacrifice {
        return Err("Amulet sacrifice preceded its tap cost".into());
    }
    let paid = PaidV1 {
        index: state.engine.event_history.len() as u64,
        original_source: owner.source,
        original_sacrifice: owner.sacrifice,
        original_soul_count: owner.original_soul_count,
        payment_history_start: owner.history_start as u64,
        tap_history_index: *tap as u64,
        sacrificed_history_index: *marker as u64,
        sacrifice_history_index: *sacrifice as u64,
        item: Box::new(item.clone()),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::RavenousPaymentV1 {
            paid: Box::new(paid),
        });
    Ok(())
}
