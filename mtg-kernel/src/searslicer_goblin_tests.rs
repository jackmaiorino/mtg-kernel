//! Registered operands exercise the timing/token primitives, not printed-card admission.
use crate::card_def::{self, Keywords};
use crate::effect::{self, ExecCtx};
use crate::engine::{self, Action};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Step, Zone};
use crate::trigger::{self, TriggerCondition};

#[test]
fn end_step_raid_primitive_checks_real_declarations_and_captured_controller() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for declaration in 0..3 {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            state.turn = 3;
            state.step = Step::DeclareAttackers;
            let source = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
            if declaration == 2 {
                let before = serde_json::to_vec(&state).unwrap();
                assert!(
                    engine::step(&mut state, Action::DeclareAttackers(vec![source, source]))
                        .is_err()
                );
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            } else {
                engine::step(
                    &mut state,
                    Action::DeclareAttackers(if declaration == 1 {
                        vec![source]
                    } else {
                        vec![]
                    }),
                )
                .unwrap();
            }
            let event = CommittedEvent::BeginningEndStep {
                active_player: actor,
                creature_died_this_turn: false,
            };
            let cond = TriggerCondition::BeginningControllerEndStepAfterControllerAttackedThisTurn;
            assert_eq!(
                trigger::trigger_matches(cond, &event, source, actor, &state, 0),
                declaration == 1
            );
            assert!(!trigger::trigger_matches(
                cond,
                &event,
                source,
                actor.opponent(),
                &state,
                0
            ));
            assert!(!trigger::trigger_matches(
                cond,
                &CommittedEvent::BeginningEndStep {
                    active_player: actor.opponent(),
                    creature_died_this_turn: false,
                },
                source,
                actor,
                &state,
                0
            ));
            let ctx = ExecCtx::no_targets(source, actor);
            let program = trigger::searslicer_goblin_effect();
            let count = state.players[actor.index()].battlefield.len();
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            effect::execute(&program, &ctx, &mut state);
            effect::execute(&program, &ctx, &mut replay);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert_eq!(
                state.players[actor.index()].battlefield.len(),
                count + usize::from(declaration == 1)
            );
            assert!(state.engine.halted.is_none());
            if declaration == 1 {
                let token = *state.players[actor.index()].battlefield.last().unwrap();
                let live = state.objects.get(token);
                assert_eq!(
                    live.card_def,
                    card_def::card_id_by_name("Goblin Token").unwrap()
                );
                assert_eq!((live.owner, live.controller), (actor, actor));
                assert!(live.v4.is_token && live.summoning_sick);
                assert!(!live.tapped);
                assert!(!engine::has_effective_keyword(
                    &state,
                    token,
                    Keywords::HASTE
                ));
                assert!(!state.engine.combat.attackers.contains(&token));
            }
        }
    }
}

#[test]
fn resolving_raid_token_uses_capture_after_source_departure_and_refuses_stale_turn() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.turn = 3;
        state.step = Step::DeclareAttackers;
        let source = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
        engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        assert!(state.player_attacked_this_turn_v1(actor));
        let ctx = ExecCtx::no_targets(source, actor);
        let program = trigger::searslicer_goblin_effect();
        for invalid in 0..3 {
            let mut stale = state.clone();
            match invalid {
                0 => stale.turn += 1,
                1 => stale.active_player = actor.opponent(),
                _ => stale.attack_declaration_turn_v1 = None,
            }
            let before = serde_json::to_vec(&stale).unwrap();
            effect::execute(&program, &ctx, &mut stale);
            assert_eq!(serde_json::to_vec(&stale).unwrap(), before);
        }
        effect::execute(&program, &ctx, &mut state);
        assert_eq!(state.players[actor.index()].battlefield.len(), 1);
        assert!(
            state
                .objects
                .get(state.players[actor.index()].battlefield[0])
                .v4
                .is_token
        );
    }
}
