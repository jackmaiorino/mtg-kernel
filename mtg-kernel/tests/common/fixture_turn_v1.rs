// Traverse genuine turn boundaries without rewriting retained occurrence anchors.
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::PlayerId;
use mtg_kernel::state::{GameState, Step};

pub fn reach(state: &mut GameState, active: PlayerId, step: Step) {
    for _ in 0..256 {
        let decision = engine::advance_until_decision(state);
        assert!(
            !matches!(
                decision,
                Decision::Halted { .. } | Decision::GameOver { .. }
            ),
            "{decision:?}"
        );
        if state.active_player == active && state.step == step {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::Discard { count, choices, .. } => {
                Action::Discard(choices.into_iter().take(count as usize).collect())
            }
            other => panic!("unexpected decision while traversing a fixture turn: {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("fixture did not reach actual {active:?} {step:?}");
}

/// Corrupt turn/active fields may test stale authority, but cannot be used as a
/// legitimate new turn. Keep original journals and prove action refusal.
pub fn stale_turn_refuses_readonly(state: &mut GameState) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, Action::Pass).is_err());
    assert_eq!(before, serde_json::to_vec(state).unwrap());
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::Halted { .. }
    ));
}
