use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

fn declaration_state(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready_state();
    state.turn = 3;
    state.active_player = actor;
    state.priority_player = actor;
    state.step = Step::DeclareAttackers;
    let creature = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
    (state, creature)
}

#[test]
fn committed_nonempty_declarations_record_history_and_refusals_are_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (initial, creature) = declaration_state(actor);
        for attackers in [vec![creature, creature], vec![ObjectId(u32::MAX)]] {
            let mut state = initial.clone();
            let before = state.diagnostic_state_hash();
            assert!(apply_declare_attackers(&mut state, attackers).is_err());
            assert_eq!(state.diagnostic_state_hash(), before);
            assert!(!state.player_attacked_this_turn_v1(actor));
        }
        let mut wrong_step = initial.clone();
        wrong_step.step = Step::Main1;
        let before = wrong_step.diagnostic_state_hash();
        assert!(apply_declare_attackers(&mut wrong_step, vec![creature]).is_err());
        assert_eq!(wrong_step.diagnostic_state_hash(), before);
        let mut empty = initial.clone();
        apply_declare_attackers(&mut empty, vec![]).unwrap();
        assert!(empty.engine.combat.attackers_declared);
        assert!(empty.attack_declaration_turn_v1.is_none());
        let mut state = initial;
        apply_declare_attackers(&mut state, vec![creature]).unwrap();
        assert!(state.player_attacked_this_turn_v1(actor));
        assert!(!state.player_attacked_this_turn_v1(actor.opponent()));
        let before = state.diagnostic_state_hash();
        assert!(apply_declare_attackers(&mut state, vec![creature]).is_err());
        assert_eq!(state.diagnostic_state_hash(), before);
    }
}

#[test]
fn token_entering_attacking_is_not_an_attack_declaration() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = declaration_state(actor);
        apply_declare_attackers(&mut state, vec![]).unwrap();
        state.step = Step::DeclareBlockers;
        let token_def = card_def::card_id_by_name("Human Soldier Token").unwrap();
        crate::effect::execute(
            &crate::effect::EffectOp::CreateTokenTappedAndAttacking { token_def },
            &crate::effect::ExecCtx::no_targets(source, actor),
            &mut state,
        );
        assert!(state.engine.halted.is_none());
        assert_eq!(state.engine.combat.attackers.len(), 1);
        let token = state.engine.combat.attackers[0];
        assert!(state.objects.get(token).v4.is_token);
        assert!(state.objects.get(token).tapped);
        assert!(!state.player_attacked_this_turn_v1(actor));
        assert!(state.attack_declaration_turn_v1.is_none());
    }
}

#[test]
fn history_survives_controller_changes_death_and_end_of_combat() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, creature) = declaration_state(actor);
        apply_declare_attackers(&mut state, vec![creature]).unwrap();
        state.players[actor.index()]
            .battlefield
            .retain(|&id| id != creature);
        state.players[actor.opponent().index()]
            .battlefield
            .push(creature);
        state.objects.get_mut(creature).controller = actor.opponent();
        assert!(state.player_attacked_this_turn_v1(actor));
        assert!(!state.player_attacked_this_turn_v1(actor.opponent()));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Graveyard),
        );
        collect_and_queue_triggers(&mut state);
        assert!(state.engine.event_log.is_empty());
        assert!(state.player_attacked_this_turn_v1(actor));
        state.step = Step::EndCombat;
        advance_step(&mut state);
        assert_eq!(state.step, Step::Main2);
        assert!(state.engine.combat.attackers.is_empty());
        assert!(state.player_attacked_this_turn_v1(actor));
        let mut extra = state.clone();
        extra.step = Step::DeclareAttackers;
        let another = put(&mut extra, actor, "Guttersnipe", Zone::Battlefield);
        apply_declare_attackers(&mut extra, vec![another]).unwrap();
        assert!(extra.player_attacked_this_turn_v1(actor));
        assert_eq!(
            extra.attack_declaration_turn_v1,
            state.attack_declaration_turn_v1
        );
        // An empty additional combat cannot erase an earlier declaration.
        state.step = Step::DeclareAttackers;
        apply_declare_attackers(&mut state, vec![]).unwrap();
        assert!(state.player_attacked_this_turn_v1(actor));
    }
}

#[test]
fn cleanup_preserves_current_turn_and_actual_turn_transition_resets_both_seats() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, creature) = declaration_state(actor);
            state.starting_player = starting;
            apply_declare_attackers(&mut state, vec![creature]).unwrap();
            state.step = Step::Cleanup;
            run_step_entry_action(&mut state, Step::Cleanup);
            assert!(state.player_attacked_this_turn_v1(actor));
            let round = state.turn;
            advance_step(&mut state);
            assert_eq!(state.step, Step::Untap);
            assert_eq!(state.active_player, actor.opponent());
            assert_eq!(state.turn, round + u32::from(actor.opponent() == starting));
            assert!(state.attack_declaration_turn_v1.is_none());
            assert!(!state.player_attacked_this_turn_v1(actor));
            assert!(!state.player_attacked_this_turn_v1(actor.opponent()));
        }
    }
}

#[test]
fn missing_history_preserves_snapshot_bytes_and_recorded_history_replays() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, creature) = declaration_state(actor);
        let absent = serde_json::to_vec(&state).unwrap();
        assert!(serde_json::to_value(&state)
            .unwrap()
            .get("attack_declaration_turn_v1")
            .is_none());
        let old: GameState = serde_json::from_slice(&absent).unwrap();
        assert_eq!(serde_json::to_vec(&old).unwrap(), absent);
        assert_eq!(old.diagnostic_state_hash(), state.diagnostic_state_hash());
        apply_declare_attackers(&mut state, vec![creature]).unwrap();
        let mut stale = state.clone();
        stale.turn += 1;
        assert!(!stale.player_attacked_this_turn_v1(actor));
        let mut switched = state.clone();
        switched.active_player = actor.opponent();
        assert!(!switched.player_attacked_this_turn_v1(actor));
        assert!(!switched.player_attacked_this_turn_v1(actor.opponent()));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
        assert_eq!(
            restored.diagnostic_state_hash(),
            state.diagnostic_state_hash()
        );
        let mut erased = state.clone();
        erased.attack_declaration_turn_v1 = None;
        assert_ne!(
            erased.diagnostic_state_hash(),
            state.diagnostic_state_hash()
        );
        state.step = Step::EndCombat;
        restored.step = Step::EndCombat;
        advance_step(&mut state);
        advance_step(&mut restored);
        assert_eq!(restored, state);
        assert_eq!(
            restored.diagnostic_state_hash(),
            state.diagnostic_state_hash()
        );
    }
}
