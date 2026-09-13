//! Private bridge from the controlled London opening to a fresh fast session.
use super::*;
use crate::human_opening_v1::HumanOpeningV1;

pub(crate) fn build_opening_state_v1(
    mainboards: &[Vec<u16>; 2],
    pair_environment_seed: u64,
    starting_player: PlayerId,
) -> Result<(SessionDeckHashesV1, crate::state::GameState), String> {
    build_session_deck_pair_state_from_explicit_decks(
        mainboards,
        pair_environment_seed,
        Some(starting_player),
    )
    .map_err(|error| error.to_string())
}

impl FastActorSessionV1 {
    pub(crate) fn from_human_opening_v1(opening: HumanOpeningV1) -> Result<Self, String> {
        let parts = opening.into_ready_parts()?;
        let mut session = Self {
            deck_ids: parts.deck_ids,
            deck_hashes: parts.deck_hashes,
            episode_id: parts.episode_id,
            max_physical_decisions: parts.max_physical_decisions,
            max_policy_steps: parts.max_policy_steps,
            state: parts.state,
            surface: PolicySurfaceV5::new_for_session(),
            environment_revision: 0,
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            flat_action_contract_mode: FlatActionContractModeV1::V3,
            flat_action_cache_spare: None,
            flat_action_cache_spare_v2: None,
            terminal: None,
        };
        session.advance_to_decision_or_terminal();
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;

    #[test]
    fn human_opening_keep_seven_changes_only_legacy_pregame_draw_counters() {
        let mainboards = [
            vec![card_id_by_name("Mountain").unwrap(); 60],
            vec![card_id_by_name("Island").unwrap(); 60],
        ];
        for starting in [PlayerId::P0, PlayerId::P1] {
            let decks = ["Human".into(), "Model".into()];
            let mut legacy=FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
                1,99,256,8192,decks.clone(),mainboards.clone(),starting).unwrap();
            let mut opening = HumanOpeningV1::new(
                1,
                99,
                256,
                8192,
                decks,
                mainboards.clone(),
                starting,
                PlayerId::P0,
            )
            .unwrap();
            opening.keep().unwrap();
            let fresh = opening.into_session().unwrap();
            for seat in [PlayerId::P0, PlayerId::P1] {
                assert_eq!(legacy.state.players[seat.index()].draws_this_turn, 7);
                assert_eq!(fresh.state.players[seat.index()].draws_this_turn, 0);
                // Existing fixed reset behavior remains unchanged. The native
                // human path deliberately removes its pregame draw feature.
                legacy.state.players[seat.index()].draws_this_turn = 0;
            }
            assert_eq!(
                serde_json::to_vec(&fresh.state).unwrap(),
                serde_json::to_vec(&legacy.state).unwrap()
            );
            assert_eq!(fresh.current_response(), legacy.current_response());
            let FastActorResponseV1::Decision(decision) = fresh.current_response() else {
                panic!("opening must reach a playable decision");
            };
            let mut projector = crate::human_bo3_v1::HumanDecisionProjectorV1::new(decision.acting_player);
            let visible = projector.project_current(&fresh, decision).unwrap();
            assert!(!visible.actions.is_empty());
            assert_eq!(fresh.state.starting_player, starting);
            assert_eq!(fresh.state.players[starting.index()].hand.len(), 7);
            assert_eq!(
                fresh.state.players[starting.opponent().index()].hand.len(),
                7
            );
        }
    }

    #[test]
    fn human_opening_mulligan_retains_first_draw_and_turn_counters() {
        let mainboards = [
            vec![card_id_by_name("Mountain").unwrap(); 60],
            vec![card_id_by_name("Island").unwrap(); 60],
        ];
        for starting in [PlayerId::P0, PlayerId::P1] {
            for human in [PlayerId::P0, PlayerId::P1] {
                let mut opening = HumanOpeningV1::new(
                    1,
                    100,
                    256,
                    8192,
                    ["Human".into(), "Model".into()],
                    mainboards.clone(),
                    starting,
                    human,
                )
                .unwrap();
                opening.mulligan().unwrap();
                opening.keep().unwrap();
                opening.bottom(&[0]).unwrap();
                let mut session = opening.into_session().unwrap();
                assert!(session.state.engine.event_log.is_empty());
                assert_eq!(session.state.players[0].draws_this_turn, 0);
                assert_eq!(session.state.players[1].draws_this_turn, 0);
                for target in [starting, starting.opponent()] {
                    let mut reached = false;
                    for _ in 0..128 {
                        if session.state.active_player == target
                            && session.state.step == crate::state::Step::Main1
                        {
                            reached = true;
                            break;
                        }
                        let FastActorResponseV1::Decision(decision) = session.current_response()
                        else {
                            panic!("unexpected terminal before first main");
                        };
                        let selected = session
                            .current
                            .as_ref()
                            .unwrap()
                            .candidates
                            .iter()
                            .position(|candidate| {
                                matches!(&candidate.semantic, ActionSemanticV1::Pass { .. })
                            })
                            .expect("all-land fixture offers pass");
                        session
                            .step(decision.episode_id, decision.step, selected as u32)
                            .unwrap();
                    }
                    assert!(reached, "first main must be reachable with pass actions");
                    let kept = if target == human { 6 } else { 7 };
                    assert_eq!(
                        session.state.players[target.index()].hand.len(),
                        kept + usize::from(target != starting)
                    );
                    assert_eq!(
                        session.state.players[target.index()].draws_this_turn,
                        u32::from(target != starting)
                    );
                    assert_eq!(
                        session.state.players[target.opponent().index()].draws_this_turn,
                        0
                    );
                }
            }
        }
    }
}
