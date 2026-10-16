//! Paid Partial-source fixtures for Niv-Mizzet, Visionary.

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use crate::{
        effect::EffectOp,
        engine::{self, Action, Decision},
        event::CommittedEvent,
        ids::{ObjectId, PlayerId},
        mana::ManaColor,
        policy_observation_v6::tests::{put, ready_state_for},
        state::{GameState, Target, Zone},
    };

    fn finish(state: &mut GameState) {
        for _ in 0..96 {
            match engine::advance_until_decision(state) {
                Decision::OrderTriggers { pending, .. } => engine::step(
                    state,
                    Action::OrderTriggers((0..pending.len()).rev().collect()),
                )
                .unwrap(),
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("Niv fixture unexpected decision while finishing: {other:?}"),
            }
        }
        panic!("Niv fixture stack did not finish");
    }

    fn cast_niv(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId) {
        let responder = actor.opponent();
        let mut state = ready_state_for(actor);
        for player in [actor, responder] {
            for _ in 0..20 {
                put(&mut state, player, "Island", Zone::Library);
            }
        }
        for _ in 0..8 {
            put(&mut state, actor, "Island", Zone::Hand);
        }
        // These are ordinary battlefield setup permanents. The card under test,
        // the damage spell, and the control response below use real cast routes.
        let archer = put(&mut state, actor, "Firebrand Archer", Zone::Battlefield);
        let fae = put(
            &mut state,
            responder,
            "High Fae Trickster",
            Zone::Battlefield,
        );
        let niv = put(&mut state, actor, "Niv-Mizzet, Visionary", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(niv)).unwrap();
        finish(&mut state);
        assert_eq!(state.objects.get(niv).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(niv).controller, actor);
        assert_eq!(state.objects.get(archer).controller, actor);
        assert_eq!(state.objects.get(fae).controller, responder);
        (state, niv, archer, fae)
    }

    fn top_niv_damage_amount(state: &GameState) -> Option<i32> {
        let item = state.stack.last()?;
        let Some(EffectOp::NivMizzetVisionaryDrawV1 {
            damage_history_index,
        }) = item.inline_effect.as_ref()
        else {
            return None;
        };
        match state
            .engine
            .event_history
            .get(usize::try_from(*damage_history_index).ok()?)?
        {
            CommittedEvent::Damage {
                target: Target::Player(_),
                amount,
                ..
            } => Some(*amount),
            _ => None,
        }
    }

    fn cast_bolt(actor: PlayerId, state: &mut GameState) {
        let bolt = put(state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(state, Action::CastSpell(bolt)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::ChooseTargets { spell, .. } if spell == bolt
        ));
        engine::step(
            state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
    }

    fn advance_to_archer_damage_trigger(actor: PlayerId, archer: ObjectId, state: &mut GameState) {
        for _ in 0..96 {
            match engine::advance_until_decision(state) {
                Decision::OrderTriggers { pending, .. } => {
                    let archer_index = pending
                        .iter()
                        .position(|trigger| trigger.source == archer)
                        .expect("actual Firebrand Archer trigger in ordered batch");
                    let mut order: Vec<_> = (0..pending.len())
                        .filter(|index| *index != archer_index)
                        .collect();
                    order.push(archer_index);
                    engine::step(state, Action::OrderTriggers(order)).unwrap();
                }
                Decision::CastSpellOrPass { player, .. }
                    if player == actor && top_niv_damage_amount(state) == Some(1) =>
                {
                    return;
                }
                Decision::CastSpellOrPass { .. } => {
                    engine::step(state, Action::Pass).unwrap();
                }
                other => panic!(
                    "Niv fixture unexpected decision before Archer damage trigger: {other:?}"
                ),
            }
        }
        panic!("paid Firebrand Archer damage did not produce Niv's one-card draw trigger");
    }

    fn cast_flash_employment_and_finish(actor: PlayerId, target: ObjectId, state: &mut GameState) {
        let responder = actor.opponent();
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::CastSpellOrPass { player, .. } if player == actor
        ));
        engine::step(state, Action::Pass).unwrap();
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::CastSpellOrPass { player, .. } if player == responder
        ));
        let employment = put(state, responder, "Involuntary Employment", Zone::Hand);
        state.players[responder.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        state.players[responder.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(state, Action::CastSpell(employment)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::ChooseTargets { spell, .. } if spell == employment
        ));
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
        finish(state);
        assert_eq!(state.objects.get(target).controller, responder);
    }

    fn reach_active_player_cleanup(
        actor: PlayerId,
        state: &mut GameState,
    ) -> Option<(u32, Vec<ObjectId>)> {
        for _ in 0..160 {
            let decision = engine::advance_until_decision(state);
            if state.active_player != actor {
                return None;
            }
            match decision {
                Decision::Discard {
                    player,
                    count,
                    choices,
                } if state.step == crate::state::Step::Cleanup => {
                    assert_eq!(player, actor);
                    return Some((count, choices));
                }
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                Decision::DeclareAttackers { .. } => {
                    engine::step(state, Action::DeclareAttackers(Vec::new())).unwrap()
                }
                Decision::DeclareBlockers { .. } => {
                    engine::step(state, Action::DeclareBlockers(Vec::new())).unwrap()
                }
                Decision::OrderTriggers { pending, .. } => engine::step(
                    state,
                    Action::OrderTriggers((0..pending.len()).rev().collect()),
                )
                .unwrap(),
                other => panic!("Niv fixture unexpected decision before cleanup: {other:?}"),
            }
        }
        panic!("fixture did not reach the active player's cleanup");
    }

    fn run_control_change_case(actor: PlayerId, steal_observer: bool) {
        let responder = actor.opponent();
        let (mut state, niv, archer, _fae) = cast_niv(actor);
        cast_bolt(actor, &mut state);
        advance_to_archer_damage_trigger(actor, archer, &mut state);
        assert_eq!(top_niv_damage_amount(&state), Some(1));
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            restored, state,
            "receipt/stack survives native snapshot round trip"
        );
        state = restored;
        let actor_library_before = state.players[actor.index()].library.len();
        let target = if steal_observer { niv } else { archer };
        cast_flash_employment_and_finish(actor, target, &mut state);
        if steal_observer {
            assert_eq!(state.objects.get(niv).controller, responder);
            assert_eq!(state.objects.get(archer).controller, actor);
            assert!(!crate::event::niv_mizzet_visionary_removes_hand_limit_v1(
                &state, actor
            ));
            assert!(crate::event::niv_mizzet_visionary_removes_hand_limit_v1(
                &state, responder
            ));
        } else {
            assert_eq!(state.objects.get(archer).controller, responder);
            assert_eq!(state.objects.get(niv).controller, actor);
            assert!(crate::event::niv_mizzet_visionary_removes_hand_limit_v1(
                &state, actor
            ));
            assert!(!crate::event::niv_mizzet_visionary_removes_hand_limit_v1(
                &state, responder
            ));
        }
        // Archer's already-queued one-damage trigger survives either control
        // change. Bolt's later damage triggers only if the Niv remains with
        // its original controller when that separate damage event occurs.
        finish(&mut state);
        let expected_draws = if steal_observer { 1 } else { 4 };
        assert_eq!(
            actor_library_before - state.players[actor.index()].library.len(),
            expected_draws,
            "draw amount and trigger controller follow each damage event's real timing"
        );
        assert_eq!(
            state.players[responder.index()].life,
            16,
            "one Firebrand Archer and one Lightning Bolt dealt their actual damage"
        );
        let cleanup_discard = reach_active_player_cleanup(actor, &mut state);
        if steal_observer {
            let (count, choices) = cleanup_discard
                .expect("Niv control returns during cleanup, after its controller's discard check");
            assert_eq!(count, (8 + expected_draws - 7) as u32);
            assert_eq!(state.objects.get(niv).controller, actor);
            assert_eq!(state.players[actor.index()].hand.len(), 8 + expected_draws);
            let discard_count = usize::try_from(count).unwrap();
            assert!(choices.len() >= discard_count);
            engine::step(
                &mut state,
                Action::Discard(choices[..discard_count].to_vec()),
            )
            .unwrap();
            assert_eq!(state.players[actor.index()].hand.len(), 7);
        } else {
            assert!(
                cleanup_discard.is_none(),
                "Niv remains controlled at cleanup"
            );
            assert_eq!(state.players[actor.index()].hand.len(), 8 + expected_draws);
        }
    }

    #[test]
    fn paid_noncombat_damage_draws_follow_original_controller_after_source_or_observer_control_change_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            run_control_change_case(actor, false);
            run_control_change_case(actor, true);
        }
    }
}
