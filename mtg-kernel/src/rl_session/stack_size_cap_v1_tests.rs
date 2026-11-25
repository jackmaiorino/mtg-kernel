//! The optional session stack-size cap (`max_stack_objects`): absent means no
//! cap (the Zirda, the Dawnwaker + Basalt Monolith loop grows well past 600
//! stack items without halting); when configured on a session, any decision
//! that would leave the engine with more than `S` stack items halts with
//! `engine_halted:StackSizeCapExceeded:source:<id>` naming the first item
//! beyond the cap, while `S` items is still offered and snapshots/clones
//! preserve the cap.

use super::*;
use crate::card_def::{card_id_by_name, CARD_DEFS};
use crate::engine::{Action, Decision, UnsupportedMechanic};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
use crate::surface_v2::PriorityModeV1;

fn battlefield(state: &mut GameState, name: &str) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name}"));
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: PlayerId::P0,
        controller: PlayerId::P0,
        zone: Zone::Battlefield,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[0].battlefield.push(object);
    object
}

/// P0's first main phase with Zirda and Basalt Monolith after `items` untap
/// activations of the Monolith, each made from an ordinary priority decision.
fn untap_stack(items: usize) -> (GameState, ObjectId) {
    let plains = card_id_by_name("Plains").unwrap();
    let mut state = GameState::new_from_libraries(
        &[plains; 20],
        &[plains; 20],
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5354_4143_4b00,
    );
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    battlefield(&mut state, "Zirda, the Dawnwaker");
    let monolith = battlefield(&mut state, "Basalt Monolith");
    for _ in 0..items {
        state.players[0].mana_pool[ManaColor::C.pool_index()] += 1;
        match crate::engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass {
                activatable_abilities,
                ..
            } => assert!(activatable_abilities.contains(&(monolith, 0))),
            other => panic!("expected priority, got {other:?}"),
        }
        crate::engine::step(&mut state, Action::ActivateAbility(monolith, 0)).unwrap();
    }
    (state, monolith)
}

fn cap_reason(source: ObjectId) -> String {
    format!("engine_halted:StackSizeCapExceeded:source:{}", source.0)
}

fn fast_session_with_mode(
    state: GameState,
    max_stack_objects: Option<u32>,
    priority_mode: PriorityModeV1,
) -> FastActorSessionV1 {
    let mut session = FastActorSessionV1::reset_with_limits(23, 91, 32, 32);
    session.max_stack_objects = max_stack_objects;
    session.state = state;
    session.surface =
        PolicySurfaceV5::new_with_priority_mode_v1(priority_mode, SuppressionAuditMode::Off);
    session.environment_revision = 0;
    session.policy_step_count = 0;
    session.physical_decision_count = 0;
    session.current = None;
    session.flat_action_cache_spare = None;
    session.flat_action_cache_spare_v2 = None;
    session.terminal = None;
    session.flat_action_contract_mode = FlatActionContractModeV1::V2;
    session.advance_to_decision_or_terminal();
    session
}

fn episode_session_with_cap(
    state: GameState,
    max_stack_objects: Option<u32>,
    priority_mode: PriorityModeV1,
) -> RlEpisodeSessionV1 {
    let mut session = RlEpisodeSessionV1::reset_with_limits(23, 91, 32, 32);
    session.max_stack_objects = max_stack_objects;
    session.install_fixture_state_preserving_limits_for_test(state, priority_mode);
    session
}

fn monolith_action_index_fast(session: &FastActorSessionV1, monolith: ObjectId) -> u32 {
    session
        .diagnostic_current_action_semantics()
        .expect("current decision")
        .iter()
        .position(|semantic| {
            matches!(
                semantic,
                ActionSemanticV1::ActivateAbility {
                    source,
                    ability_index: 0,
                    ..
                } if source.arena_id == monolith.0
            )
        })
        .expect("Basalt Monolith untap ability offered") as u32
}

fn monolith_action_episode(decision: &RlSessionDecisionV1, monolith: ObjectId) -> (u32, String) {
    let action = decision
        .legal_actions
        .iter()
        .find(|action| {
            matches!(
                &action.semantic,
                ActionSemanticV1::ActivateAbility {
                    source,
                    ability_index: 0,
                    ..
                } if source.arena_id == monolith.0
            )
        })
        .expect("Basalt Monolith untap ability offered");
    (action.selected_index, action.stable_id.clone())
}

#[test]
fn sessions_without_max_stack_objects_never_halt_past_600_stack_items() {
    let (mut state, monolith) = untap_stack(640);
    state.players[0].mana_pool[ManaColor::C.pool_index()] += 1;
    assert_eq!(state.stack.len(), 640);
    assert_eq!(state.engine.halted, None);

    let fast = FastActorSessionV1::from_v2_fixture_state(state.clone());
    assert_eq!(fast.max_stack_objects(), None);
    assert!(matches!(
        fast.current_response(),
        FastActorResponseV1::Decision(_)
    ));
    assert_eq!(fast.game_state().stack.len(), 640);
    assert_eq!(fast.game_state().engine.halted, None);

    let mut episode = episode_session_with_cap(state, None, PriorityModeV1::EngineWindowsV1);
    assert_eq!(episode.max_stack_objects(), None);
    let RlSessionResponseV1::Decision(decision) = episode.current_response() else {
        panic!("expected uncapped decision at 640 stack items");
    };
    let (index, stable_id) = monolith_action_episode(&decision, monolith);
    let next = episode
        .step(decision.episode_id, decision.step, index, &stable_id)
        .unwrap();
    assert!(matches!(next, RlSessionResponseV1::Decision(_)));
    assert_eq!(episode.game_state().stack.len(), 641);
    assert_eq!(episode.game_state().engine.halted, None);
}

#[test]
fn flat_actor_session_enforces_max_stack_objects_16_and_128() {
    for cap in [16u32, 128u32] {
        let (mut state, monolith) = untap_stack(cap as usize);
        state.players[0].mana_pool[ManaColor::C.pool_index()] += 1;

        // Decision at exactly `cap` items is still offered; stepping to
        // `cap + 1` halts with `StackSizeCapExceeded` even under HarnessV2.
        let mut session =
            FastActorSessionV1::from_v2_fixture_state(state).with_max_stack_objects(Some(cap));
        assert_eq!(session.max_stack_objects(), Some(cap));
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("expected decision at exact cap {cap}");
        };
        assert_eq!(session.game_state().stack.len(), cap as usize);
        assert_eq!(session.stack_objects(), cap);
        let action_index = monolith_action_index_fast(&session, monolith);
        let response = session
            .step(decision.episode_id, decision.step, action_index)
            .unwrap();
        match response {
            FastActorResponseV1::Terminal(terminal) => {
                assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::Halted);
                assert_eq!(
                    terminal.terminal_classification,
                    TerminalClassificationV1::Halted
                );
                assert_eq!(terminal.terminal_reason, cap_reason(monolith));
            }
            FastActorResponseV1::Decision(_) => {
                panic!("a decision was exposed over max_stack_objects={cap}");
            }
        }
        assert_eq!(session.game_state().stack.len(), cap as usize + 1);
        assert_eq!(
            session.game_state().engine.halted,
            Some((UnsupportedMechanic::StackSizeCapExceeded, monolith))
        );
    }
}

#[test]
fn episode_session_enforces_max_stack_objects_16_and_128() {
    for cap in [16u32, 128u32] {
        let (mut state, monolith) = untap_stack(cap as usize);
        state.players[0].mana_pool[ManaColor::C.pool_index()] += 1;

        let mut session =
            episode_session_with_cap(state, Some(cap), PriorityModeV1::EngineWindowsV1);
        assert_eq!(session.max_stack_objects(), Some(cap));
        let RlSessionResponseV1::Decision(decision) = session.current_response() else {
            panic!("expected decision at exact cap {cap}");
        };
        assert_eq!(session.game_state().stack.len(), cap as usize);
        assert_eq!(session.stack_objects(), cap);
        let (index, stable_id) = monolith_action_episode(&decision, monolith);
        let response = session
            .step(decision.episode_id, decision.step, index, &stable_id)
            .unwrap();
        match response {
            RlSessionResponseV1::Terminal(terminal) => {
                assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::Halted);
                assert_eq!(
                    terminal.terminal_classification,
                    TerminalClassificationV1::Halted
                );
                assert_eq!(terminal.terminal_reason, cap_reason(monolith));
            }
            RlSessionResponseV1::Decision(_) => {
                panic!("a decision was exposed over max_stack_objects={cap}");
            }
        }
        assert_eq!(session.game_state().stack.len(), cap as usize + 1);
        assert_eq!(
            session.game_state().engine.halted,
            Some((UnsupportedMechanic::StackSizeCapExceeded, monolith))
        );
    }
}

#[test]
fn stack_size_cap_names_first_item_beyond_cap_and_preserves_prior_terminal() {
    // Two distinct Basalt Monoliths: 16 activations from A, then item 17
    // (index 16) from B, then item 18 (index 17) from A. Cap 16 must name B.
    let (mut state, monolith_a) = untap_stack(16);
    let monolith_b = battlefield(&mut state, "Basalt Monolith");
    assert_ne!(monolith_a, monolith_b);
    for source in [monolith_b, monolith_a] {
        state.players[0].mana_pool[ManaColor::C.pool_index()] += 1;
        assert!(matches!(
            crate::engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        crate::engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    }
    assert_eq!(state.stack.len(), 18);

    let fast = fast_session_with_mode(state.clone(), Some(16), PriorityModeV1::EngineWindowsV1);
    match fast.current_response() {
        FastActorResponseV1::Terminal(terminal) => {
            assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::Halted);
            assert_eq!(terminal.terminal_reason, cap_reason(monolith_b));
        }
        FastActorResponseV1::Decision(_) => panic!("expected cap halt"),
    }
    assert_eq!(
        fast.game_state().engine.halted,
        Some((UnsupportedMechanic::StackSizeCapExceeded, monolith_b))
    );

    // Finished game stands even when the stack exceeds the cap.
    let mut finished = state.clone();
    finished.players[1].life = 0;
    let fast_finished = fast_session_with_mode(finished, Some(16), PriorityModeV1::EngineWindowsV1);
    match fast_finished.current_response() {
        FastActorResponseV1::Terminal(terminal) => {
            assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::P0Win);
            assert_eq!(
                terminal.terminal_classification,
                TerminalClassificationV1::Natural
            );
        }
        FastActorResponseV1::Decision(_) => panic!("expected natural game over"),
    }
    assert_eq!(fast_finished.game_state().engine.halted, None);

    // Earlier engine halt stands even when the stack exceeds the cap.
    let mut earlier_halt = state;
    earlier_halt.engine.halted = Some((UnsupportedMechanic::InvalidEffectContinuation, monolith_a));
    let fast_earlier =
        fast_session_with_mode(earlier_halt, Some(16), PriorityModeV1::EngineWindowsV1);
    match fast_earlier.current_response() {
        FastActorResponseV1::Terminal(terminal) => {
            assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::Halted);
            assert_eq!(
                terminal.terminal_reason,
                format!(
                    "engine_halted:InvalidEffectContinuation:source:{}",
                    monolith_a.0
                )
            );
        }
        FastActorResponseV1::Decision(_) => panic!("expected earlier halt"),
    }
    assert_eq!(
        fast_earlier.game_state().engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, monolith_a))
    );
}

#[test]
fn fast_actor_snapshot_and_clone_keep_max_stack_objects() {
    let (mut state, monolith) = untap_stack(15);
    state.players[0].mana_pool[ManaColor::C.pool_index()] += 2;
    let mut session = fast_session_with_mode(state, Some(16), PriorityModeV1::EngineWindowsV1);
    let snapshot = session.snapshot_v1();
    let cloned = session.clone();

    // Advance the original session past the cap (15 -> 16 offered -> 17 halts).
    let FastActorResponseV1::Decision(d0) = session.current_response() else {
        panic!("expected decision at 15 items");
    };
    let idx0 = monolith_action_index_fast(&session, monolith);
    let FastActorResponseV1::Decision(d1) = session.step(d0.episode_id, d0.step, idx0).unwrap()
    else {
        panic!("expected decision at 16 items");
    };
    let idx1 = monolith_action_index_fast(&session, monolith);
    assert!(matches!(
        session.step(d1.episode_id, d1.step, idx1).unwrap(),
        FastActorResponseV1::Terminal(t) if t.terminal_reason == cap_reason(monolith)
    ));

    // Restoring into an uncapped session restores the snapshot's cap.
    let mut restored = FastActorSessionV1::reset_with_limits(99, 101, 32, 32);
    assert_eq!(restored.max_stack_objects(), None);
    restored.restore_v1(&snapshot);
    assert_eq!(restored.max_stack_objects(), Some(16));

    for mut branch in [restored, cloned] {
        assert_eq!(branch.max_stack_objects(), Some(16));
        let FastActorResponseV1::Decision(b0) = branch.current_response() else {
            panic!("expected decision at 15 items after restore/clone");
        };
        let b_idx0 = monolith_action_index_fast(&branch, monolith);
        let FastActorResponseV1::Decision(b1) =
            branch.step(b0.episode_id, b0.step, b_idx0).unwrap()
        else {
            panic!("expected decision at 16 items after restore/clone");
        };
        assert_eq!(branch.game_state().stack.len(), 16);
        let b_idx1 = monolith_action_index_fast(&branch, monolith);
        match branch.step(b1.episode_id, b1.step, b_idx1).unwrap() {
            FastActorResponseV1::Terminal(terminal) => {
                assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::Halted);
                assert_eq!(terminal.terminal_reason, cap_reason(monolith));
            }
            FastActorResponseV1::Decision(_) => {
                panic!("expected halt at 17 items after restore/clone");
            }
        }
    }
}

#[test]
fn episode_session_snapshot_and_clone_keep_max_stack_objects() {
    let (mut state, monolith) = untap_stack(15);
    state.players[0].mana_pool[ManaColor::C.pool_index()] += 2;
    let mut session = episode_session_with_cap(state, Some(16), PriorityModeV1::EngineWindowsV1);
    let snapshot = session.snapshot_v5();
    let cloned = session.clone();

    // Advance the original session past the cap (15 -> 16 offered -> 17 halts).
    let RlSessionResponseV1::Decision(d0) = session.current_response() else {
        panic!("expected decision at 15 items");
    };
    let (idx0, id0) = monolith_action_episode(&d0, monolith);
    let RlSessionResponseV1::Decision(d1) =
        session.step(d0.episode_id, d0.step, idx0, &id0).unwrap()
    else {
        panic!("expected decision at 16 items");
    };
    let (idx1, id1) = monolith_action_episode(&d1, monolith);
    assert!(matches!(
        session.step(d1.episode_id, d1.step, idx1, &id1).unwrap(),
        RlSessionResponseV1::Terminal(t) if t.terminal_reason == cap_reason(monolith)
    ));

    // Restoring into an uncapped session restores the snapshot's cap.
    let mut restored = RlEpisodeSessionV1::reset_with_limits(99, 101, 32, 32);
    assert_eq!(restored.max_stack_objects(), None);
    restored.restore_v5(&snapshot);
    assert_eq!(restored.max_stack_objects(), Some(16));

    for mut branch in [restored, cloned] {
        assert_eq!(branch.max_stack_objects(), Some(16));
        let RlSessionResponseV1::Decision(b0) = branch.current_response() else {
            panic!("expected decision at 15 items after restore/clone");
        };
        let (b_idx0, b_id0) = monolith_action_episode(&b0, monolith);
        let RlSessionResponseV1::Decision(b1) =
            branch.step(b0.episode_id, b0.step, b_idx0, &b_id0).unwrap()
        else {
            panic!("expected decision at 16 items after restore/clone");
        };
        assert_eq!(branch.game_state().stack.len(), 16);
        let (b_idx1, b_id1) = monolith_action_episode(&b1, monolith);
        match branch.step(b1.episode_id, b1.step, b_idx1, &b_id1).unwrap() {
            RlSessionResponseV1::Terminal(terminal) => {
                assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::Halted);
                assert_eq!(terminal.terminal_reason, cap_reason(monolith));
            }
            RlSessionResponseV1::Decision(_) => {
                panic!("expected halt at 17 items after restore/clone");
            }
        }
    }
}
