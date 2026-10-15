use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::{GameState, Zone};
pub(crate) fn ordinary(actor: PlayerId) -> GameState {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    state
}
pub(crate) fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(expected) = response else {
        panic!("{response:?}")
    };
    expected
}
#[test]
fn actual_absence_mixed_input_owning_revision_and_whole_observation_binding_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = FastActorSessionV1::from_season_v48_fixture_state(ordinary(actor));
        let expected = expected(&session);
        let (observation, actions, binding) = session
            .season_current_decision_input_v48(expected, actor.into())
            .unwrap();
        assert!(observation.season.active_sources.is_empty());
        assert!(observation.season.creations.is_empty());
        assert!(actions.len() > 1);
        assert!(session
            .season_current_decision_input_v48(expected, actor.opponent().into())
            .is_err());
        let snapshot = session.snapshot_v1();
        for _ in 0..3 {
            session.season_restore_v48(&snapshot).unwrap();
            let before = session.state.clone();
            assert!(session
                .season_validate_bound_decision_v48(expected, actor.into(), &binding)
                .is_err());
            assert_eq!(session.state, before);
            let live = self::expected(&session);
            let (_, _, current) = session
                .season_current_decision_input_v48(live, actor.into())
                .unwrap();
            session
                .season_validate_bound_decision_v48(live, actor.into(), &current)
                .unwrap();
        }
        let live = self::expected(&session);
        let (_, _, mut forged) = session
            .season_current_decision_input_v48(live, actor.into())
            .unwrap();
        forged.observation.season.card_db_hash ^= 1;
        forged.observation.projection_sha256 =
            crate::season_observation_v48::commitment(&forged.observation).unwrap();
        let before = session.state.clone();
        assert!(session
            .season_validate_bound_decision_v48(live, actor.into(), &forged)
            .is_err());
        assert_eq!(session.state, before);
        let mut corrupted = snapshot.clone();
        corrupted.0.current.as_mut().unwrap().environment_revision += 1;
        assert!(session.season_restore_v48(&corrupted).is_err());
        assert_eq!(session.state, before);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_source_old_mode_advance_refuses_readonly_and_v48_publishes_distinct_facts() {
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordinary(actor);
        let source = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        let mut session = FastActorSessionV1::from_season_v48_fixture_state(state.clone());
        let expected = self::expected(&session);
        let (observation, _, _) = session
            .season_current_decision_input_v48(expected, actor.into())
            .unwrap();
        assert_eq!(observation.season.active_sources.len(), 1);
        assert_eq!(observation.season.meanings.len(), 3);
        let mut old = session.clone();
        old.flat_action_contract_mode = FlatActionContractModeV1::PandaV47;
        let before = old.state.clone();
        old.advance_to_decision_or_terminal();
        assert!(old.terminal.is_some());
        assert_eq!(old.state, before);
        for _ in 0..3 {
            session.season_restore_v48(&session.snapshot_v1()).unwrap();
        }
        assert_eq!(session.state, state);
    }
}
