use super::{
    life_trigger_tests::{choose, facts},
    *,
};
use crate::state::{StackItemKind, Step};
use trigger_programs::ProgramV1;

fn setup(actor: PlayerId) -> GameState {
    let library = [card("Plains"); 40];
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &library,
        &library,
        crate::rl::card_name,
        550_227,
        actor,
    );
    state.step = Step::Main1;
    for land in [
        "Island", "Forest", "Forest", "Swamp", "Swamp", "Plains", "Plains",
    ] {
        put(&mut state, actor, land, Zone::Battlefield);
    }
    put(&mut state, actor.opponent(), "Plains", Zone::Battlefield);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
    state
}

fn reach(state: &mut GameState, selected: impl Fn(&NativeDecision) -> bool) {
    for _ in 0..64 {
        let d = storm::next(state);
        if selected(&d) {
            return;
        }
        let action = match d {
            NativeDecision::CastSpellOrPass { .. } => NativeAction::Pass,
            NativeDecision::DeclareAttackers { .. } => NativeAction::DeclareAttackers(vec![]),
            NativeDecision::DeclareBlockers { .. } => NativeAction::DeclareBlockers(vec![]),
            other => panic!("unexpected native decision: {other:?}"),
        };
        storm::action(state, action);
    }
    panic!("requested native decision absent")
}

fn settle(owner: &mut KaitoSession, native: &mut FastActorSessionV1) {
    for _ in 0..24 {
        let state = owner.fixture_state();
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        let d = decision(owner);
        choose(owner, native, &d, |action| {
            matches!(action, Action::Pass { .. })
        });
    }
    panic!("selected trigger did not settle");
}

fn stack_program(d: &KaitoDecision, program: ProgramV1) {
    assert!(facts(d).stack.iter().any(|row| row.program == program));
}

fn token_count(s: &GameState, actor: PlayerId, name: &str) -> usize {
    s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| s.objects.get(id).card_def == card(name))
        .count()
}

#[test]
fn paid_sylvan_and_wardens_keep_native_placement_modes_and_outcomes_both_seats() {
    use ProgramV1::*;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (name, cost, marker, programs) in [
            (
                "Sylvan Scavenging",
                3,
                SylvanChooseMode,
                [SylvanCounter, SylvanRaccoon],
            ),
            (
                "Wardens of the Cycle",
                4,
                WardensChooseMode,
                [WardensGainLife, WardensDrawLoseLife],
            ),
        ] {
            let mut state = setup(actor);
            let source = storm::cast(&mut state, actor, name, &[], cost);
            storm::finish(&mut state);
            let own_creature = put(&mut state, actor, "Serra Angel", Zone::Battlefield);
            if marker == WardensChooseMode {
                let dead = put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                );
                storm::cast(&mut state, actor, "Stab", &[Target::Object(dead)], 1);
                storm::finish(&mut state);
                assert!(state.creature_died_this_turn_v1());
            }
            reach(
                &mut state,
                |d| matches!(d, NativeDecision::ChooseTriggerMode { source: id, .. } if *id == source),
            );
            let mut native = raw(&state);
            let mut owner = KaitoSession::from_session(raw(&state)).unwrap();
            let saved = owner.snapshot();
            let native_saved = native.snapshot_v1();
            for (mode, program) in programs.into_iter().enumerate() {
                let d = decision(&owner);
                assert_model(&owner, &d);
                assert!(facts(&d).pending.iter().any(|row| row.program == marker));
                assert!(owner.fixture_state().stack.is_empty());
                choose(&mut owner, &mut native, &d, |action| {
                    matches!(
                        action, Action::ChooseSpellMode { mode_index, source: card, .. }
                            if usize::from(*mode_index) == mode && card.arena_id == source.0
                    )
                });
                if program == SylvanCounter {
                    let targets = decision(&owner);
                    assert!(facts(&targets)
                        .pending
                        .iter()
                        .any(|row| row.program == program));
                    choose(&mut owner, &mut native, &targets, |action| {
                        matches!(action,
                        Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }
                            if object.arena_id == own_creature.0)
                    });
                }
                let d = decision(&owner);
                stack_program(&d, program);
                assert_model(&owner, &d);
                settle(&mut owner, &mut native);
                let live = owner.fixture_state();
                match program {
                    SylvanCounter => {
                        assert_eq!(live.objects.get(own_creature).counters.plus1_plus1, 1)
                    }
                    SylvanRaccoon => assert_eq!(token_count(live, actor, "Raccoon Token"), 1),
                    WardensGainLife => assert_eq!(live.players[actor.index()].life, 22),
                    WardensDrawLoseLife => {
                        assert_eq!(live.players[actor.index()].life, 19);
                        assert_eq!(live.players[actor.index()].hand.len(), 1);
                    }
                    _ => unreachable!(),
                }
                owner.restore(&saved).unwrap();
                native.kaito_restore_v75(&native_saved).unwrap();
                assert!(owner.apply(&d, 0).is_err());
            }
        }
    }
}

#[test]
fn paid_kiora_keeps_private_discard_and_exact_threshold_choice_both_seats() {
    use ProgramV1::*;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = setup(actor);
        for _ in 0..5 {
            put(&mut state, actor, "Island", Zone::Graveyard);
        }
        // Three cards after the ETB draw require two genuine private choices.
        // With only the two drawn cards, the native engine discards both directly.
        put(&mut state, actor, "Island", Zone::Hand);
        let kiora = storm::cast(&mut state, actor, "Kiora, the Rising Tide", &[], 3);
        let mut native = raw(&state);
        let mut owner = KaitoSession::from_session(raw(&state)).unwrap();
        let mut seen_entry = false;
        let mut discarded = 0;
        for _ in 0..24 {
            let d = decision(&owner);
            let backend = d.fixture_backend_v75();
            if facts(&d)
                .stack
                .iter()
                .any(|row| row.program == KioraDrawDiscard)
            {
                seen_entry = true;
                assert_model(&owner, &d);
            }
            let discard = |action: &Action| {
                matches!(action, Action::Discard { actor: chooser, cards }
                if *chooser == actor.into() && cards.len() == 1 && cards.iter().all(|c| c.owner == actor.into()))
            };
            if backend.actions().iter().any(discard) {
                choose(&mut owner, &mut native, &d, discard);
                discarded += 1;
                if discarded == 2 {
                    break;
                }
                continue;
            }
            choose(&mut owner, &mut native, &d, |action| {
                matches!(action, Action::Pass { .. })
            });
        }
        assert!(seen_entry);
        assert_eq!(discarded, 2);
        assert_eq!(owner.fixture_state().players[actor.index()].hand.len(), 1);
        assert_eq!(
            owner.fixture_state().players[actor.index()].graveyard.len(),
            7
        );
        let mut state = owner.fixture_state().clone();
        advance_to_next_main_for(&mut state, actor);
        reach(&mut state, |d| {
            matches!(d, NativeDecision::DeclareAttackers { eligible, .. }
            if eligible.contains(&kiora))
        });
        storm::action(&mut state, NativeAction::DeclareAttackers(vec![kiora]));
        let mut native = raw(&state);
        let mut owner = KaitoSession::from_session(raw(&state)).unwrap();
        let mut seen_threshold = false;
        for _ in 0..24 {
            let d = decision(&owner);
            if facts(&d)
                .stack
                .iter()
                .any(|row| row.program == KioraThreshold)
            {
                seen_threshold = true;
                assert_model(&owner, &d);
                assert!(owner.fixture_state().stack.iter().any(|item| {
                    item.source == kiora && item.kind == StackItemKind::TriggeredAbility
                }));
            }
            let create = |action: &Action| {
                matches!(action,
                Action::ChooseEffectOption { source, option_index: 1, option_count: 2, .. }
                    if source.arena_id == kiora.0)
            };
            if d.fixture_backend_v75().actions().iter().any(create) {
                let saved = owner.snapshot();
                let native_saved = native.snapshot_v1();
                choose(&mut owner, &mut native, &d, |action| {
                    matches!(action,
                    Action::ChooseEffectOption { source, option_index: 0, option_count: 2, .. }
                        if source.arena_id == kiora.0)
                });
                assert_eq!(
                    token_count(owner.fixture_state(), actor, "Scion of the Deep Token"),
                    0
                );
                owner.restore(&saved).unwrap();
                native.kaito_restore_v75(&native_saved).unwrap();
                let rebound = decision(&owner);
                assert_eq!(rebound.packet(), d.packet());
                choose(&mut owner, &mut native, &rebound, create);
                assert_eq!(
                    token_count(owner.fixture_state(), actor, "Scion of the Deep Token"),
                    1
                );
                assert_model(&owner, &decision(&owner));
                break;
            }
            choose(&mut owner, &mut native, &d, |action| {
                matches!(action, Action::Pass { .. })
            });
        }
        assert!(seen_threshold);
        assert_eq!(
            token_count(owner.fixture_state(), actor, "Scion of the Deep Token"),
            1
        );
    }
}
