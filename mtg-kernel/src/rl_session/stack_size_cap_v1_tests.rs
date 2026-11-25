//! The stack-size cap (`engine::STACK_SIZE_CAP_V1`) reaches every session
//! that drives the engine: the flat actor session the trainer collects
//! through and the episode session behind the JSONL server. The position is
//! the census loop: Zirda, the Dawnwaker makes Basalt Monolith's untap
//! ability cost {1}, and the untap activations are stacked past the cap.

use super::*;
use crate::card_def::{card_id_by_name, CARD_DEFS};
use crate::engine::{Action, Decision, UnsupportedMechanic, STACK_SIZE_CAP_V1};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

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
/// activations of the Monolith, each made from an ordinary priority decision
/// (the session's first advance puts the last one on the stack if the
/// activation step has not).
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

fn cap_reason(monolith: ObjectId) -> String {
    format!("engine_halted:StackSizeCapExceeded:source:{}", monolith.0)
}

#[test]
fn flat_actor_session_halts_over_the_stack_size_cap() {
    let (state, monolith) = untap_stack(STACK_SIZE_CAP_V1 + 1);
    let session = FastActorSessionV1::from_v2_fixture_state(state);
    match session.current_response() {
        FastActorResponseV1::Terminal(terminal) => {
            assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::Halted);
            assert_eq!(
                terminal.terminal_classification,
                TerminalClassificationV1::Halted
            );
            assert_eq!(terminal.terminal_reason, cap_reason(monolith));
        }
        FastActorResponseV1::Decision(_) => panic!("a decision over the stack-size cap"),
    }
    assert_eq!(
        session.game_state().engine.halted,
        Some((UnsupportedMechanic::StackSizeCapExceeded, monolith))
    );
}

#[test]
fn flat_actor_session_exposes_a_decision_at_the_stack_size_cap() {
    let (state, _) = untap_stack(STACK_SIZE_CAP_V1);
    let session = FastActorSessionV1::from_v2_fixture_state(state);
    assert!(matches!(
        session.current_response(),
        FastActorResponseV1::Decision(_)
    ));
}

#[test]
fn episode_session_halts_over_the_stack_size_cap() {
    let (state, monolith) = untap_stack(STACK_SIZE_CAP_V1 + 1);
    let mut session = RlEpisodeSessionV1::reset_with_limits(23, 91, 8, 8);
    session.state = state;
    session.surface = PolicySurfaceV5::new();
    session.environment_revision = 0;
    session.policy_step_count = 0;
    session.physical_decision_count = 0;
    session.current = None;
    session.terminal = None;
    session.advance_to_decision_or_terminal();
    match session.current_response() {
        RlSessionResponseV1::Terminal(terminal) => {
            assert_eq!(terminal.terminal_outcome, TerminalOutcomeV1::Halted);
            assert_eq!(terminal.terminal_reason, cap_reason(monolith));
        }
        RlSessionResponseV1::Decision(_) => panic!("a decision over the stack-size cap"),
    }
}
