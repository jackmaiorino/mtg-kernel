//! Genuine paid Harvester plus native cast-reversal coverage. The private abort
//! invocation is an existing engine primitive, not an exposed cancellation action.
use super::*;
use crate::event::CommittedEvent;
use crate::policy_observation_v6::tests::put;

fn restore(state: &GameState) -> GameState {
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state, &restored);
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}

fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..16 {
        match advance_until_decision(state) {
            Decision::CastSpellOrPass { player, .. } if player == actor => return,
            Decision::CastSpellOrPass { .. } => step(state, Action::Pass).unwrap(),
            other => panic!("genuine native cast priority: {other:?}"),
        }
        assert!(state.engine.halted.is_none());
    }
    panic!("native cast priority absent");
}

fn taps(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| state.objects.get(**id).tapped)
        .count()
}

fn paid(
    state: &mut GameState,
    actor: PlayerId,
    card: ObjectId,
    target: Option<Target>,
    mana: usize,
) {
    priority(state, actor);
    let before = taps(state, actor);
    step(state, Action::CastSpell(card)).unwrap();
    if let Some(target) = target {
        assert!(matches!(advance_until_decision(state),
            Decision::ChooseTargets { player, ref legal_targets, .. }
                if player == actor && legal_targets.contains(&target)));
        step(state, Action::ChooseTarget(target)).unwrap();
    }
    assert!(matches!(advance_until_decision(state),
        Decision::CastSpellOrPass { player, .. } if player == actor));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.objects.get(card).zone, Zone::Stack);
    assert_eq!(taps(state, actor) - before, mana);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    for _ in 0..16 {
        let decision = advance_until_decision(state);
        assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        if state.stack.is_empty() {
            return;
        }
        step(state, Action::Pass).unwrap();
    }
    panic!("genuine native paid cast did not resolve");
}

#[test]
fn genuine_paid_harvester_native_cast_abort_rebinds_original_hand_and_graveyard_with_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for origin in [Zone::Hand, Zone::Graveyard] {
            let forest = card_def::card_id_by_name("Forest").unwrap();
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_479_14,
                actor,
            );
            state.step = Step::Main1;
            for player in [actor, actor.opponent()] {
                for name in ["Swamp", "Forest", "Mountain"] {
                    for _ in 0..6 {
                        put(&mut state, player, name, Zone::Battlefield);
                    }
                }
            }
            let harvester = put(&mut state, actor, "Abyssal Harvester", Zone::Hand);
            paid(&mut state, actor, harvester, None, 3);
            assert_eq!(state.objects.get(harvester).zone, Zone::Battlefield);
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
            if origin == Zone::Graveyard {
                paid(&mut state, actor, creature, None, 1);
                let bolt = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
                paid(
                    &mut state,
                    actor.opponent(),
                    bolt,
                    Some(Target::Object(creature)),
                    1,
                );
                assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
                assert!(effect::harvester_v1::eligible(&state, creature));
                // Existing generic permission primitive, not a paid Case/Zul
                // claim. It enables the genuine public graveyard announcement.
                effect::execute(
                    &EffectOp::StandardV1(
                        crate::standard_cards_v1::StandardOpV1::GraveyardCreaturesCastableThisTurn,
                    ),
                    &effect::ExecCtx::no_targets(harvester, actor),
                    &mut state,
                );
            }
            priority(&mut state, actor);
            let before = state.clone();
            let generation = state.objects.get(creature).zone_change_count;
            let history_len = state.engine.event_history.len();
            assert!(matches!(advance_until_decision(&mut state),
                Decision::CastSpellOrPass { ref castable_spells, .. }
                    if castable_spells.contains(&creature)));
            step(&mut state, Action::CastSpell(creature)).unwrap();
            assert_eq!(state.objects.get(creature).zone, Zone::Stack);
            assert_eq!(
                state.objects.get(creature).zone_change_count,
                generation + 1
            );
            let announcement = state.engine.event_history.len() - 1;
            assert!(matches!(state.engine.event_history[announcement],
                CommittedEvent::HarvesterCastAnnouncedV1 { cast }
                    if cast.source == creature && cast.origin.origin_zone == origin
                        && cast.stack_generation == generation + 1));
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                validate_pending_cast(branch, branch.engine.pending_cast.as_ref().unwrap())
                    .unwrap();
                let pending = branch.engine.pending_cast.take().unwrap();
                assert_eq!(pending.spell, creature);
                // Exercise the actual private reversal on its unmodified native
                // pending cast. No public cancellation or failed-payment claim.
                abort_cast(branch, pending, CastMethodV4::Normal);
                assert!(branch.engine.halted.is_none(), "{:?}", branch.engine.halted);
                assert!(branch.engine.pending_cast.is_none());
                assert_eq!(branch.objects.get(creature).zone, origin);
                assert_eq!(
                    branch.objects.get(creature).zone_change_count,
                    generation + 2
                );
                assert_eq!(branch.stack, before.stack);
                assert_eq!(
                    branch.players[actor.index()].hand,
                    before.players[actor.index()].hand
                );
                assert_eq!(
                    branch.players[actor.index()].graveyard,
                    before.players[actor.index()].graveyard
                );
                assert_eq!(branch.step, before.step);
                assert_eq!(branch.active_player, before.active_player);
                for player in [actor, actor.opponent()] {
                    assert_eq!(
                        branch.players[player.index()].mana_pool,
                        before.players[player.index()].mana_pool
                    );
                    assert_eq!(taps(branch, player), taps(&before, player));
                    assert_eq!(
                        branch.players[player.index()].battlefield,
                        before.players[player.index()].battlefield
                    );
                    for object in &before.players[player.index()].battlefield {
                        assert_eq!(
                            branch.objects.get(*object).tapped,
                            before.objects.get(*object).tapped
                        );
                    }
                    assert_eq!(
                        branch.players[player.index()].spells_cast_this_turn,
                        before.players[player.index()].spells_cast_this_turn
                    );
                    assert_eq!(
                        branch.players[player.index()].life,
                        before.players[player.index()].life
                    );
                }
                assert_eq!(
                    &branch.engine.event_history[..history_len],
                    &before.engine.event_history[..]
                );
                let reversals = branch
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(at, event)| match event {
                        CommittedEvent::HarvesterCastReversedV1 {
                            announcement: original,
                            restored,
                        } if *original == announcement as u64 => Some((at, *restored)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(reversals.len(), 1);
                let (reversal, restored) = reversals[0];
                assert_eq!(reversal, announcement + 1);
                assert_eq!(restored.object, creature);
                assert_eq!(restored.expected_zone, origin);
                assert_eq!(restored.expected_zone_change_count, generation + 2);
                assert!(!branch
                    .engine
                    .event_log
                    .iter()
                    .any(|event| matches!(event, CommittedEvent::HarvesterCastReversedV1 { .. })));
                if origin == Zone::Graveyard {
                    assert!(matches!(branch.engine.event_history[reversal + 1],
                        CommittedEvent::HarvesterGraveyardEntryV1 { entry }
                            if entry.card == restored && matches!(entry.origin,
                                effect::harvester_v1::GraveyardOriginV1::AbortedCast { reversal: bound, .. }
                                    if bound == reversal as u64)));
                    assert!(effect::harvester_v1::eligible(branch, creature));
                }
                effect::harvester_v1::validate_state(branch).unwrap();
            }
            assert_eq!(state, replay);
            assert_eq!(state.state_hash(), replay.state_hash());
            let mut restored = restore(&state);
            assert_eq!(
                advance_until_decision(&mut state),
                advance_until_decision(&mut restored)
            );
            assert_eq!(state, restored);
            if origin == Zone::Graveyard {
                // The generic grant was incarnation-bound. This separate
                // permission primitive grants the returned incarnation before
                // the subsequent genuinely paid cast; it is not a claim that
                // the aborted announcement preserved the old generic grant.
                for branch in [&mut state, &mut restored] {
                    effect::execute(
                        &EffectOp::StandardV1(
                            crate::standard_cards_v1::StandardOpV1::GraveyardCreaturesCastableThisTurn,
                        ),
                        &effect::ExecCtx::no_targets(harvester, actor),
                        branch,
                    );
                }
            }
            paid(&mut state, actor, creature, None, 1);
            paid(&mut restored, actor, creature, None, 1);
            assert_eq!(state, restored);
            assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
            effect::harvester_v1::validate_state(&state).unwrap();
        }
    }
}
