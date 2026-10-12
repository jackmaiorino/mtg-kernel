//! Original end-step observer identity, independent of mutable trigger rows.
use crate::card_def::{Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectCond, EffectOp};
use crate::event::CommittedEvent;
use crate::state::{AbilitySourceContractV4, GameState, Zone};

pub(crate) fn root() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::OpponentLostLifeThisTurn,
        then: Box::new(EffectOp::StromkirkVampirePermanentCounterV1),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}
pub(crate) fn target_spec() -> TargetSpec {
    TargetSpec::ControlledPermanentWithAnySubtype([Subtype::Vampire; 4])
}
pub(crate) fn owns_definition(id: u16) -> bool {
    CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.is_executable()
            && crate::trigger::triggers_for(id).iter().any(|t| {
                t.condition
                == crate::trigger::TriggerCondition::BeginningControllerEndStepIfOpponentLostLife
                && t.home_zone == Zone::Battlefield
                && t.face_index == 0
                && (t.effect)() == root()
            })
    })
}

pub(crate) fn record_begin(state: &mut GameState, beginning: usize) {
    if !crate::life_loss_turn_v1::enabled() {
        return;
    }
    let Some(CommittedEvent::BeginningEndStep { active_player, .. }) =
        state.engine.event_history.get(beginning)
    else {
        return;
    };
    let controller = *active_player;
    if !crate::life_loss_turn_v1::opponent_lost_life(state, controller).unwrap_or(false) {
        return;
    }
    let observers: Vec<_> = state.players[controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| {
            state.objects.try_get(*id).is_some_and(|o| {
                o.zone == Zone::Battlefield
                    && o.controller == controller
                    && o.v4.face_index == 0
                    && o.v4.face_down_v1.is_none()
                    && owns_definition(o.card_def)
            }) && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
        })
        .map(|id| AbilitySourceContractV4::capture(state, id))
        .collect();
    if !observers.is_empty() {
        state
            .engine
            .event_history
            .push(CommittedEvent::StromkirkEndStepObserversV1 {
                beginning_index: beginning as u64,
                turn: state.turn,
                observers,
            });
    }
}

pub(crate) fn validate_source(
    state: &GameState,
    op: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let selected = source.is_some_and(|s| owns_definition(s.card_def));
    if !selected && *op != root() {
        return Ok(());
    }
    let source = source.ok_or("Stromkirk lost its original end-step source")?;
    if !selected || *op != root() {
        return Err("Stromkirk changed its selected trigger root".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    let original = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|event| match event {
            CommittedEvent::StromkirkEndStepObserversV1 { observers, .. } => {
                observers.iter().find(|s| {
                    s.source == source.source && s.zone_change_count == source.zone_change_count
                })
            }
            _ => None,
        });
    if original != Some(&source) {
        return Err("Stromkirk changed its captured end-step controller/source".into());
    }
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::life_loss_turn_v1::validate(state)?;
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, CommittedEvent::StromkirkEndStepObserversV1 { .. }))
    {
        return Err("private Stromkirk observer journal entered the trigger log".into());
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::StromkirkEndStepObserversV1 {
            beginning_index,
            turn,
            observers,
        } = event
        else {
            continue;
        };
        let begin =
            usize::try_from(*beginning_index).map_err(|_| "Stromkirk beginning index overflow")?;
        let Some(CommittedEvent::BeginningEndStep { active_player, .. }) =
            state.engine.event_history.get(begin)
        else {
            return Err("Stromkirk original observer lost its end-step occurrence".into());
        };
        if begin >= at || observers.is_empty() {
            return Err("Stromkirk observer occurrence is empty or out of order".into());
        }
        let anchor = state.engine.event_history[..begin]
            .iter()
            .rfind(|e| matches!(e, CommittedEvent::LifeLossTurnBeganV1 { .. }));
        if anchor
            != Some(&CommittedEvent::LifeLossTurnBeganV1 {
                turn: *turn,
                active_player: *active_player,
            })
        {
            return Err("Stromkirk observer lost its original turn anchor".into());
        }
        let anchor_at = state.engine.event_history[..begin]
            .iter()
            .rposition(|e| matches!(e, CommittedEvent::LifeLossTurnBeganV1 { .. }))
            .unwrap();
        if !state.engine.event_history[anchor_at + 1..begin]
            .iter()
            .any(|e| {
                crate::life_loss_turn_v1::committed_loss_player(e) == Some(active_player.opponent())
            })
        {
            return Err(
                "Stromkirk observer lacks actual opponent life loss before end step".into(),
            );
        }
        for (i, source) in observers.iter().enumerate() {
            if source.controller != *active_player || source.zone != Zone::Battlefield
                || !owns_definition(source.card_def) || observers[..i].iter().any(|s| s.source == source.source)
                || state.engine.event_history[..at].iter().any(|e| matches!(e,
                    CommittedEvent::StromkirkEndStepObserversV1 { beginning_index: old, .. } if old == beginning_index)) {
                return Err("Stromkirk observer changed source/controller or repeated its occurrence".into());
            }
            crate::engine::validate_historical_ability_source_contract(state, *source)?;
        }
    }
    for pending in state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
    {
        if pending.granted_by.is_none()
            && (pending
                .source_contract
                .is_some_and(|s| owns_definition(s.card_def))
                || pending.effect == root())
        {
            validate_source(state, &pending.effect, pending.source_contract)?;
            if pending
                .source_contract
                .is_none_or(|s| s.controller != pending.controller || s.source != pending.source)
                || pending.target_spec != target_spec()
            {
                return Err(
                    "Stromkirk pending trigger changed controller or permanent target scope".into(),
                );
            }
        }
    }
    for item in &state.stack {
        if item.v4.granted_by.is_none()
            && item
                .v4
                .ability_source_contract
                .is_some_and(|s| owns_definition(s.card_def))
        {
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
    }
    Ok(())
}

pub(crate) fn owns_item(item: &crate::state::StackItem) -> bool {
    item.kind == crate::state::StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_none()
        && item.inline_effect.as_ref() == Some(&root())
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| owns_definition(s.card_def))
}
pub(crate) fn resolve(state: &mut GameState, ctx: &crate::effect::ExecCtx) {
    if resolve_checked(state, ctx).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
    }
}
fn resolve_checked(state: &mut GameState, ctx: &crate::effect::ExecCtx) -> Result<(), String> {
    let source = ctx
        .ability_source_contract
        .ok_or("Stromkirk counter lost source authority")?;
    validate_source(state, &root(), Some(source))?;
    if source.source != ctx.source
        || source.controller != ctx.controller
        || ctx.targets.len() != 1
        || ctx.target_contracts.len() != 1
    {
        return Err("Stromkirk counter changed original context".into());
    }
    let crate::state::Target::Object(object) = ctx.targets[0] else {
        return Err("Stromkirk target is not a permanent".into());
    };
    let contract = crate::effect::temporary_control_v1::normalize_historical_target(
        state,
        ctx.target_contracts[0],
    );
    let crate::state::StackTargetContractV4::Object {
        object: captured,
        zone: Zone::Battlefield,
        zone_change_count,
        ..
    } = contract
    else {
        return Err("Stromkirk counter lost captured permanent incarnation".into());
    };
    if captured != object || !state.engine.event_history.iter().any(|e| matches!(e,
        CommittedEvent::Targeted { target, target_zone_change_count, targeting_stack_item, targeting_controller }
            if *target == object && *target_zone_change_count == zone_change_count
                && Some(*targeting_stack_item) == ctx.stack_item_id && *targeting_controller == ctx.controller)) {
        return Err("Stromkirk counter lost actual targeted publication".into());
    }
    if !crate::engine::target_contract_matches_live(state, ctx.targets[0], contract)
        || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source,
            ctx.controller,
            target_spec(),
            &ctx.targets,
            0,
        )
    {
        return Ok(());
    }
    crate::event::add_plus_one_counters(state, object, ctx.controller, 1)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    #[test]
    fn permanent_target_and_checked_counter_primitive_include_noncreature_vampire() {
        // Typed physical subtype boundary. No invented Stromkirk producer/root.
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = actor;
            let own = put(&mut state, actor, "Forest", Zone::Battlefield);
            let foreign = put(&mut state, actor.opponent(), "Forest", Zone::Battlefield);
            for id in [own, foreign] {
                state.objects.get_mut(id).v4.effective_subtype_ids =
                    vec![Subtype::Vampire.stable_id()];
                assert!(!crate::engine::object_has_type(
                    &state,
                    id,
                    crate::card_def::CardType::Creature
                ));
            }
            let legal =
                crate::engine::legal_targets_for_controller(target_spec(), &[], actor, &state);
            assert!(legal.contains(&crate::state::Target::Object(own)));
            assert!(!legal.contains(&crate::state::Target::Object(foreign)));
            crate::event::add_plus_one_counters(&mut state, own, actor, 1).unwrap();
            assert_eq!(state.objects.get(own).counters.plus1_plus1, 1);
            let replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(
                serde_json::to_vec(&replay).unwrap(),
                serde_json::to_vec(&state).unwrap()
            );
        }
    }
}
