//! Actual future printed definition, never a stand-in for registration.
use super::*;
use crate::engine::{self, Action, Decision};
use crate::ids::PlayerId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::put;

pub(crate) fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
    state.step = crate::state::Step::Main1;
    // Real printed draw attempts need real cards. ready_state intentionally
    // starts with empty libraries; it never deals an opening hand.
    for owner in [actor, actor.opponent()] {
        for _ in 0..16 {
            put(&mut state, owner, "Forest", Zone::Library);
        }
    }
    state
}
pub(crate) fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
pub(crate) fn settle(state: &mut GameState) {
    for _ in 0..80 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            Decision::ChooseLegendPermanent { candidates, .. } => {
                engine::step(state, Action::ChooseLegendPermanent(candidates[0])).unwrap();
            }
            other => panic!("unexpected real entry stage {other:?}"),
        }
    }
    panic!("real token resolution did not settle");
}
pub(crate) fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Arahbo, the First Fang", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass {player,castable_spells,..}
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    source
}
fn token_count(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| {
            let live = state.objects.get(**id);
            live.card_def == crate::card_def::card_id_by_name("Cat Token").unwrap()
                && live.v4.is_token
        })
        .count()
}
pub(crate) fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn readonly(state: &GameState) {
    for _ in 0..2 {
        let mut bad = restore(state);
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
    }
}
pub(crate) fn published(state: &mut GameState) -> StackItem {
    for _ in 0..20 {
        if let Some(item) = state.stack.iter().find(|item| {
            matches!(
                item.inline_effect,
                Some(EffectOp::ArahboTokenFromCaptureV1 { .. })
            )
        }) {
            return item.clone();
        }
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { .. } => {}
            other => panic!("unexpected actual publication {other:?}"),
        }
    }
    panic!("no genuine published Arahbo row");
}

#[test]
fn real_cast_cat_token_anthem_controller_and_noncat_conditions_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor);
        assert_eq!(token_count(&state, actor), 1);
        assert_eq!(engine::effective_power(&state, source), 2);
        let cat = *state.players[actor.index()]
            .battlefield
            .iter()
            .find(|id| state.objects.get(**id).v4.is_token)
            .unwrap();
        assert_eq!(engine::effective_power(&state, cat), 2);
        assert_eq!(engine::effective_toughness(&state, cat), 2);
        let hunter = put(&mut state, actor, "Helpful Hunter", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(hunter, Zone::Battlefield),
        );
        settle(&mut state);
        assert_eq!(token_count(&state, actor), 2);
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(elf, Zone::Battlefield),
        );
        settle(&mut state);
        assert_eq!(token_count(&state, actor), 2);
        assert_eq!(engine::effective_power(&state, elf), 1);
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        assert_eq!(engine::effective_power(&state, cat), 1);
        let opposing = put(&mut state, actor.opponent(), "Savannah Lions", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(opposing, Zone::Battlefield),
        );
        settle(&mut state);
        assert_eq!(token_count(&state, actor.opponent()), 1);
    }
}

#[test]
fn real_token_copy_self_entry_is_included_other_token_entry_is_excluded_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor);
        let copy = crate::standard_cards_v1::create_token_copy(&mut state, source, actor).unwrap();
        let captures = state
            .engine
            .event_history
            .iter()
            .filter_map(|event| match event {
                CommittedEvent::ArahboTokenCaptureV1 { capture } => {
                    let CommittedEvent::ArahboEntryObservationV1 {
                        entry, was_token, ..
                    } = &state.engine.event_history[capture.observation_history_index as usize]
                    else {
                        panic!()
                    };
                    (entry.object == copy).then_some((
                        capture.source.source,
                        capture.root,
                        *was_token,
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(captures, vec![(copy, RootV1::OwnEntry, true)]);
        let pending = crate::trigger::collect_and_process(&mut state);
        state.engine.pending_triggers.extend(pending);
        assert!(state.pending_legend_rule_v1.is_some());
        settle(&mut state);
        assert_eq!(token_count(&state, actor), 2);
        validate_state(&restore(&state)).unwrap();
    }
}

#[test]
fn real_complete_batch_source_and_nontoken_cat_permanent_scope_before_sba() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reversed in [false, true] {
            let mut state = ready(actor);
            let source = put(&mut state, actor, "Arahbo, the First Fang", Zone::Hand);
            let cat = put(&mut state, actor, "Savannah Lions", Zone::Hand);
            let mut entries = [source, cat];
            if reversed {
                entries.reverse();
            }
            crate::event::propose_and_commit_batch(
                &mut state,
                entries
                    .iter()
                    .map(|id| ProposedEvent::zone_change(*id, Zone::Battlefield))
                    .collect(),
            );
            assert_eq!(engine::effective_power(&state, cat), 3);
            settle(&mut state);
            assert_eq!(token_count(&state, actor), 2);
        }
    }
}

#[test]
fn real_original_controller_departure_reentry_response_and_restore_do_not_change_beneficiary() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor);
        let cat = put(&mut state, actor, "Savannah Lions", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(cat, Zone::Battlefield),
        );
        let item = published(&mut state);
        assert!(state.engine.event_history.iter().any(|event|matches!(event,CommittedEvent::ArahboTokenPublishedV1 {item:original,..} if **original==item)));
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Hand),
        );
        crate::event::propose_and_commit(&mut state, ProposedEvent::zone_change(cat, Zone::Hand));
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(token_count(&state, actor), 2);
        assert_eq!(token_count(&state, actor.opponent()), 0);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn witness_suppression_removes_current_anthem_and_new_observers_not_old_captures() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor);
        let cat = put(&mut state, actor, "Savannah Lions", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(cat, Zone::Battlefield),
        );
        published(&mut state);
        let aura = put(
            &mut state,
            actor.opponent(),
            "Witness Protection",
            Zone::Battlefield,
        );
        let aura_generation = state.objects.get(aura).zone_change_count;
        let source_generation = state.objects.get(source).zone_change_count;
        state
            .attach_object_exact(aura, aura_generation, source, source_generation)
            .unwrap();
        assert!(!active(&state, source));
        assert_eq!(engine::effective_power(&state, cat), 2);
        let other = put(&mut state, actor, "Savannah Lions", Zone::Hand);
        let before = state
            .engine
            .event_history
            .iter()
            .filter(|e| matches!(e, CommittedEvent::ArahboTokenCaptureV1 { .. }))
            .count();
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(other, Zone::Battlefield),
        );
        assert_eq!(
            state
                .engine
                .event_history
                .iter()
                .filter(|e| matches!(e, CommittedEvent::ArahboTokenCaptureV1 { .. }))
                .count(),
            before
        );
        settle(&mut state);
        assert_eq!(token_count(&state, actor), 2);
        validate_state(&restore(&state)).unwrap();
    }
}

#[test]
fn genuine_uncollected_held_published_closed_owners_refuse_erasure_and_replay() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        cast(&mut state, actor);
        let cat = put(&mut state, actor, "Savannah Lions", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(cat, Zone::Battlefield),
        );
        let mut bad = state.clone();
        bad.engine
            .event_log
            .retain(|e| !matches!(e, CommittedEvent::ArahboTokenCaptureV1 { .. }));
        readonly(&bad);
        let ready = crate::trigger::collect_and_process(&mut state);
        state.engine.pending_triggers.extend(ready);
        let mut bad = state.clone();
        bad.engine.pending_triggers.clear();
        readonly(&bad);
        let item = published(&mut state);
        let index = match item.inline_effect {
            Some(EffectOp::ArahboTokenFromCaptureV1 {
                capture_history_index,
            }) => capture_history_index,
            _ => panic!(),
        };
        let mut bad = state.clone();
        bad.stack
            .retain(|row| row.v4.stack_item_id != item.v4.stack_item_id);
        readonly(&bad);
        let removed = bad.engine.event_history.len() as u64;
        bad.engine
            .event_history
            .push(CommittedEvent::ArahboTokenRemovedV1 {
                capture_history_index: index,
                item: Box::new(item.clone()),
                kind: RemovalKindV1::Countered,
            });
        bad.engine
            .event_history
            .push(CommittedEvent::ArahboTokenCompletedV1 {
                capture_history_index: index,
                removal_history_index: removed,
                token_history_index: None,
            });
        readonly(&bad);
        for field in 0..3 {
            let mut bad = state.clone();
            let row = bad
                .stack
                .iter_mut()
                .find(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                .unwrap();
            match field {
                0 => {
                    row.controller = actor.opponent();
                    row.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
                }
                1 => {
                    row.v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                _ => row.inline_effect = Some(EffectOp::Sequence(vec![])),
            }
            readonly(&bad);
        }
        let mut coherent = state.clone();
        if let CommittedEvent::ArahboTokenCaptureV1 { capture } =
            &mut coherent.engine.event_history[index as usize]
        {
            capture.source.controller = actor.opponent();
        }
        for row in &mut coherent.stack {
            if row.v4.stack_item_id == item.v4.stack_item_id {
                row.controller = actor.opponent();
                row.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
            }
        }
        for event in &mut coherent.engine.event_history {
            if let CommittedEvent::ArahboTokenPublishedV1 {
                capture_history_index,
                item: row,
            } = event
            {
                if *capture_history_index == index {
                    row.controller = actor.opponent();
                    row.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
                }
            }
        }
        // The independent original entrant/observer observation is retained.
        readonly(&coherent);
        let mut countered = state.clone();
        engine::counter_stack_item_by_id(&mut countered, item.v4.stack_item_id).unwrap();
        validate_state(&restore(&countered)).unwrap();
        let mut bad = countered.clone();
        bad.engine
            .event_history
            .retain(|event| !matches!(event, CommittedEvent::ArahboCounterInputV1 { .. }));
        readonly(&bad);
        settle(&mut state);
        let mut bad = state.clone();
        bad.stack.push(item);
        readonly(&bad);
    }
}

#[test]
fn genuine_garruk_and_arahbo_rows_publish_independently_of_authority_leaf() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let uprising = put(&mut state, actor, "Garruk's Uprising", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(uprising)).unwrap();
        settle(&mut state);
        cast(&mut state, actor);
        let hand = state.players[actor.index()].hand.len();
        let giant = put(&mut state, actor, "Webweaver Changeling", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(giant, Zone::Battlefield),
        );
        let ready = crate::trigger::collect_and_process(&mut state);
        state.engine.pending_triggers.extend(ready);
        match next(&mut state) {
            Decision::OrderTriggers { pending, .. } => engine::step(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            )
            .unwrap(),
            other => panic!("two genuine families {other:?}"),
        }
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(state.stack.iter().any(|row| matches!(
            row.inline_effect,
            Some(EffectOp::GarrukDrawFromCaptureV1 { .. })
        )));
        assert!(state.stack.iter().any(|row| matches!(
            row.inline_effect,
            Some(EffectOp::ArahboTokenFromCaptureV1 { .. })
        )));
        settle(&mut state);
        assert_eq!(state.players[actor.index()].hand.len(), hand + 1);
    }
}
