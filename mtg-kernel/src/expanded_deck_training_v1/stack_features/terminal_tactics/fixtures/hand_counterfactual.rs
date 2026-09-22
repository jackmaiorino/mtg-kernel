//! Engine-only hand-dependent positive control. Never training data.
use super::*;

fn position(actor: u8, bolt: bool, reverse: bool) -> FastActorSessionV1 {
    let own = PlayerId(actor);
    let other = PlayerId(1 - actor);
    let mut state = GameState::new_from_libraries(&[], &[], crate::rl::card_name, 77_005);
    state.step = Step::DeclareBlockers;
    state.active_player = other;
    state.priority_player = own;
    state.players[own.index()].life = 3;
    state.players[other.index()].life = 6;
    let spell = put(&mut state, own, "Lightning Bolt", Zone::Hand);
    put(&mut state, own, if bolt { "Lightning Bolt" } else { "Mountain" }, Zone::Hand);
    let attacker = put(&mut state, other, "Myr Enforcer", Zone::Battlefield);
    state.objects.get_mut(attacker).tapped = true;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    state.engine.combat.attackers = vec![attacker];
    for owner in [own, other] {
        for name in ["Forest", "Island", "Mountain", "Swamp"] {
            put(&mut state, owner, name, Zone::Library);
        }
        if reverse { state.players[owner.index()].library.reverse(); }
    }
    state.players[own.index()].mana_pool[ManaColor::R.pool_index()] = 2;
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { .. }));
    FastActorSessionV1::from_public_terminal_fixture_v1(state)
}

fn actions(s: &FastActorSessionV1) -> (crate::rl_session::FastActorDecisionV1, Vec<ActionSemanticV1>) {
    let FastActorResponseV1::Decision(d) = s.current_response() else { panic!("expected decision") };
    let (_, a) = PairedBo1PolicyInputV1::new(s, d).diagnostic_visible_v4().unwrap();
    (d, a)
}

#[test]
fn hand_counterfactual_same_target_menu_changes_face_terminal_outcome() {
    for actor in 0..2 {
        for reverse in [false, true] {
            let land = position(actor, false, reverse);
            let bolt = position(actor, true, reverse);
            assert_eq!(actions(&land).1, actions(&bolt).1, "root action menu must be identical");
            for (has_bolt, mut session) in [(false, land), (true, bolt)] {
                let initial = session.game_state().clone();
                let mut terminal = None;
                let mut casts = 0;
                for _ in 0..32 {
                    if let FastActorResponseV1::Terminal(t) = session.current_response() {
                        terminal = Some(t); break;
                    }
                    let (d, menu) = actions(&session);
                    let selected = if let Some(i) = menu.iter().position(|a| matches!(a,
                        ActionSemanticV1::ChooseTarget { target: crate::rl::TargetRefV1::Player { player }, .. }
                        if *player != d.acting_player)) {
                        assert_eq!(seat(d.acting_player), actor);
                        i
                    } else if let Some(i) = menu.iter().position(|a| matches!(a,
                        ActionSemanticV1::CastSpell { source, .. }
                        if crate::rl::card_name(source.card_db_id) == "Lightning Bolt")) {
                        assert!(has_bolt); assert_eq!(seat(d.acting_player), actor);
                        casts += 1; i
                    } else {
                        // Opponent has no discretionary response. For the land
                        // variant every post-target continuation is forced.
                        assert_eq!(menu.len(), 1, "unproved continuation: {menu:?}");
                        assert!(matches!(menu[0], ActionSemanticV1::Pass { .. }));
                        0
                    };
                    session.step(d.episode_id, d.step, selected as u32).unwrap();
                    let after = session.game_state();
                    // Combat must advance from DeclareBlockers to damage. The
                    // single-spell witness helper also freezes the phase, so it
                    // is deliberately not used for this combat continuation.
                    assert_eq!(initial.turn, after.turn, "crossed turn boundary");
                    assert_eq!(initial.library_knowledge, after.library_knowledge);
                    for player in 0..2 {
                        assert_eq!(initial.players[player].library, after.players[player].library);
                        assert_eq!(initial.players[player].draws_this_turn, after.players[player].draws_this_turn);
                    }
                    assert!(session.game_state().players[(1 - actor) as usize].hand.is_empty());
                }
                let t = terminal.expect("bounded line must naturally terminate");
                assert_eq!(t.terminal_classification, TerminalClassificationV1::Natural);
                assert_eq!(t.winner.map(seat), Some(if has_bolt { actor } else { 1 - actor }));
                assert_eq!(casts, usize::from(has_bolt));
            }
        }
    }
}
