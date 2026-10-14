//! Scoped cooperation between original Raid ownership and the Crystal driver.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BeginV1 {
    pub index: u32,
    pub publication: u32,
    pub boundary: u32,
    pub turn: u32,
    pub active: PlayerId,
    pub attacked: Option<AttackDeclarationTurnV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompletionV1 {
    pub index: u32,
    pub begin: u32,
    pub boundary: u32,
    pub item: Box<StackItem>,
}
fn beginnings(state: &GameState, id: StackItemId) -> Vec<&BeginV1> {
    state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::GorehornCrystalBeginV1 { begin }
                if matches!(
            state.engine.event_history.get(begin.publication as usize),
            Some(CommittedEvent::GorehornPublishedV1 { publication })
            if publication.item.v4.stack_item_id == id) =>
            {
                Some(begin.as_ref())
            }
            _ => None,
        })
        .collect()
}
pub(crate) fn has_begin(state: &GameState, id: StackItemId) -> bool {
    beginnings(state, id).len() == 1
}
fn begin_at(state: &GameState, index: u32) -> Result<&BeginV1, String> {
    let Some(CommittedEvent::GorehornCrystalBeginV1 { begin }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("Gorehorn Crystal begin is absent".into());
    };
    let Some(CommittedEvent::GorehornPublishedV1 { publication: p }) =
        state.engine.event_history.get(begin.publication as usize)
    else {
        return Err("Gorehorn Crystal begin lost publication".into());
    };
    let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: b }) =
        state.engine.event_history.get(begin.boundary as usize)
    else {
        return Err("Gorehorn Crystal begin lost actual initial boundary".into());
    };
    if begin.index != index
        || begin.publication >= begin.boundary
        || begin.boundary >= index
        || beginnings(state, p.item.v4.stack_item_id).len() != 1
        || b.previous.is_some()
        || b.answer.is_some()
        || b.root != crate::trigger::gorehorn_raider_effect()
        || !publication_matches(state, &b.item, &p.item)
        || b.pending.as_ref().is_none_or(|pending| {
            pending.resolving_item != *b.item || pending.ctx != context(&p.item)
        })
    {
        return Err("Gorehorn Crystal begin changed original authority".into());
    }
    Ok(begin)
}
fn met(begin: &BeginV1, controller: PlayerId) -> bool {
    begin.attacked.is_some_and(|a| {
        a.turn == begin.turn && a.active_player == begin.active && a.active_player == controller
    })
}
pub(crate) fn begin_owned(
    state: &mut GameState,
    owner: &OwnerV1,
    item: &StackItem,
) -> Result<(), String> {
    if !beginnings(state, item.v4.stack_item_id).is_empty()
        || context(item) != context(&owner.item)
        || !publication_matches(state, item, &owner.item)
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Gorehorn Crystal cannot restart or borrow a foreign row".into());
    }
    let boundary = item
        .v4
        .crystal_prevention_started_v1
        .0
        .ok_or("Gorehorn Crystal lost marked start")?;
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Gorehorn Crystal history overflow")?;
    let begin = BeginV1 {
        index,
        publication: owner.publication,
        boundary,
        turn: state.turn_key_v1(),
        active: state.active_player,
        attacked: state.attack_declaration_turn_v1,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::GorehornCrystalBeginV1 {
            begin: Box::new(begin),
        });
    begin_at(state, index)?;
    Ok(())
}
pub(crate) fn prepare_owned(
    state: &mut GameState,
    owner: &OwnerV1,
    ctx: &ExecCtx,
    op: &EffectOp,
) -> Result<(), String> {
    if !matches!(
        op,
        EffectOp::DealDamage {
            target: crate::effect::TargetRef::Target(0),
            amount: 2
        }
    ) || *ctx != context(&owner.item)
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Gorehorn Crystal preparation changed actual damage leaf".into());
    }
    let rows = beginnings(state, owner.item.v4.stack_item_id);
    let [begin] = rows.as_slice() else {
        return Err("Gorehorn Crystal preparation lost unique begin".into());
    };
    begin_at(state, begin.index)?;
    if !met(begin, ctx.controller) {
        return Err("Gorehorn Crystal prepared a false Raid branch".into());
    }
    if state.engine.event_history.iter().any(|e| {
        matches!(e, CommittedEvent::GorehornPreparedV1 { prepared }
        if prepared.publication == owner.publication)
    }) {
        return Err("Gorehorn Crystal preparation replayed".into());
    }
    record_preparation(state, owner, ctx)?;
    Ok(())
}
pub(crate) fn completed_count(state: &GameState, id: StackItemId) -> usize {
    state.engine.event_history.iter().filter(|e| matches!(e,
        CommittedEvent::GorehornCrystalCompletionV1 { completion } if completion.item.v4.stack_item_id == id)).count()
}
pub(crate) fn prepared_is_owned(state: &GameState, r: &PreparedV1) -> bool {
    let Some(id) = r.ctx.stack_item_id else {
        return false;
    };
    if beginnings(state, id).len() != 1 {
        return false;
    }
    let pending = state.engine.pending_effect.as_ref();
    (pending.is_some_and(|p| {
        p.resolving_item.v4.stack_item_id == id
            && p.ctx == r.ctx
            && !p.resolving_item.v4.crystal_prevention_started_v1.is_empty()
    }) && state
        .stack
        .iter()
        .filter(|i| i.v4.stack_item_id == id)
        .count()
        == 1)
        || completed_count(state, id) == 1
}
pub(crate) fn finish_owned(
    state: &mut GameState,
    owner: &OwnerV1,
    item: &StackItem,
) -> Result<(), String> {
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        || state.engine.pending_effect.is_some()
        || completed_count(state, item.v4.stack_item_id) != 0
        || !publication_matches(
            state,
            item,
            &publication(state, item.v4.stack_item_id)?.item,
        )
        || context(item) != context(&owner.item)
    {
        return Err("Gorehorn Crystal completion lost its removed owner".into());
    }
    let rows = beginnings(state, item.v4.stack_item_id);
    let [begin] = rows.as_slice() else {
        return Err("Gorehorn Crystal completion lost unique begin".into());
    };
    let begin = begin.index;
    let boundary = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|e| match e {
            CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                if boundary.item.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some(boundary.history_index)
            }
            _ => None,
        })
        .ok_or("Gorehorn Crystal completion lost final boundary")?;
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Gorehorn Crystal history overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::GorehornCrystalCompletionV1 {
            completion: Box::new(CompletionV1 {
                index,
                begin,
                boundary,
                item: Box::new(item.clone()),
            }),
        });
    validate(state)
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|e| {
        matches!(
            e,
            CommittedEvent::GorehornCrystalBeginV1 { .. }
                | CommittedEvent::GorehornCrystalCompletionV1 { .. }
        )
    }) {
        return Err("Gorehorn Crystal private authority entered transient log".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::GorehornCrystalBeginV1 { begin } => {
                let begin = begin_at(state, begin.index)?;
                if begin.index as usize != index {
                    return Err("Gorehorn Crystal begin index changed".into());
                }
                let Some(CommittedEvent::GorehornPublishedV1 { publication: p }) =
                    state.engine.event_history.get(begin.publication as usize)
                else {
                    unreachable!()
                };
                let executions = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        CommittedEvent::CrystalExecutionV1 { execution }
                            if execution.item.v4.stack_item_id == p.item.v4.stack_item_id =>
                        {
                            Some(execution.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let first = executions
                    .first()
                    .ok_or("Gorehorn Crystal begin lost actual execution")?;
                let Some(step) = first.steps.first() else {
                    return Err("Gorehorn Crystal lost printed conditional".into());
                };
                if first.previous != begin.boundary
                    || first.history_index <= begin.index
                    || step.frame
                        != (crate::effect::EffectFrame::Program {
                            op: crate::trigger::gorehorn_raider_effect(),
                            path: vec![],
                        })
                    || step.conditional_result != Some(met(begin, p.item.controller))
                {
                    return Err(
                        "Gorehorn Crystal conditional lost captured turn/controller authority"
                            .into(),
                    );
                }
                let prepared_count = state.engine.event_history.iter().filter(|e| matches!(e,
                    CommittedEvent::GorehornPreparedV1 { prepared } if prepared.publication == begin.publication)).count();
                if prepared_count != usize::from(met(begin, p.item.controller)) {
                    return Err(
                        "Gorehorn Crystal branch changed mandatory source preparation".into(),
                    );
                }
                if completed_count(state, p.item.v4.stack_item_id) > 1 {
                    return Err("Gorehorn Crystal duplicated completion".into());
                }
            }
            CommittedEvent::GorehornCrystalCompletionV1 { completion: c } => {
                let begin = begin_at(state, c.begin)?;
                let Some(CommittedEvent::GorehornPublishedV1 { publication: p }) =
                    state.engine.event_history.get(begin.publication as usize)
                else {
                    unreachable!()
                };
                let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: b }) =
                    state.engine.event_history.get(c.boundary as usize)
                else {
                    return Err("Gorehorn Crystal completion lost real closed boundary".into());
                };
                let Some(CommittedEvent::CrystalExecutionV1 { execution: e }) = c
                    .boundary
                    .checked_sub(1)
                    .and_then(|i| state.engine.event_history.get(i as usize))
                else {
                    return Err("Gorehorn Crystal completion lost actual final execution".into());
                };
                if c.index as usize != index || c.begin >= c.boundary || c.boundary >= c.index
                    || c.item != b.item || e.item != c.item || e.after.is_some() || b.pending.is_some()
                    || e.history_index + 1 != b.history_index || b.answer.is_some()
                    || !publication_matches(state,&c.item,&p.item)
                    || state.stack.iter().any(|i|i.v4.stack_item_id == c.item.v4.stack_item_id)
                    || state.engine.event_history.iter().any(|e| matches!(e,
                        CommittedEvent::GorehornOperationV1 { operation } if operation.publication == p.index))
                { return Err("Gorehorn Crystal closed owner changed or was reintroduced".into()); }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_bridge_after_registered_mana_activation_keeps_bytes_hash_and_journal_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
            let elves = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Lightning Bolt",
                Zone::Hand,
            );
            crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Mountain",
                Zone::Battlefield,
            );
            assert!(matches!(
                crate::engine::advance_until_decision(&mut state),
                crate::engine::Decision::CastSpellOrPass { .. }
            ));
            crate::engine::step(
                &mut state,
                crate::engine::Action::ActivateManaAbility(elves),
            )
            .unwrap();
            assert!(state.objects.get(elves).tapped);
            let bytes = serde_json::to_vec(&state).unwrap();
            let hash = state.state_hash();
            validate(&state).unwrap();
            assert_eq!(completed_count(&state, StackItemId(0)), 0);
            assert!(!has_begin(&state, StackItemId(0)));
            assert_eq!(bytes, serde_json::to_vec(&state).unwrap());
            assert_eq!(hash, state.state_hash());
            let replay: GameState = serde_json::from_slice(&bytes).unwrap();
            validate(&replay).unwrap();
            assert_eq!(bytes, serde_json::to_vec(&replay).unwrap());
            assert_eq!(hash, replay.state_hash());
        }
    }
}
