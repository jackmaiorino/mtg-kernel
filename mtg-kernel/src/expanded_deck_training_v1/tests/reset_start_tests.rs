//! Reset episodes (spy-reset-pilot-v1, RUNNER.md section 6 risks 1 and 2):
//! the restored cut reproduces the original game, the recorded continuation
//! from it reaches the original terminal, and the starting history never
//! enters learner groups, rewards or normalization inputs.
//!
//! Real games with fixture nets: opt-in like the other native-game tests.

use super::*;

fn deck(id: &str) -> ExpandedDeckListV1 {
    let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: registered.registered_configuration().mainboard().to_vec(),
        sideboard: registered.registered_configuration().sideboard().to_vec(),
    }
}

fn base_episode(seed: u64) -> ExpandedEpisodeV1 {
    ExpandedEpisodeV1 {
        id: "reset-start-base".into(),
        seed,
        starting_player: 1,
        learner_seat: 0,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: [deck("Spy"), deck("Burn")],
        selected: [deck("Spy"), deck("Burn")],
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: Some(EpisodeRuntimeRulesV1::ResolutionBoundaryV1),
        reset: None,
    }
}

/// A reset of `base` at row `k` (learner, first substep), with the cut's
/// replay key computed by an independent replay.
fn reset_at(base: &ExpandedTrajectoryV1, k: usize, seed: u64) -> ExpandedEpisodeV1 {
    let mut episode = base.episode.clone();
    episode.id = "reset-start-cut".into();
    episode.seed = seed;
    episode.reset = Some(ExpandedResetStartV1 {
        guide: "test-guide".into(),
        game_seed: base.episode.seed,
        prefix: base.decisions[..k].iter().map(|r| r.selected).collect(),
        cut_physical_decision_id: base.decisions[k].physical_decision_id,
        cut_observation_sha256: String::new(),
        cut_menu_sha256: String::new(),
        cut_state_hash: String::new(),
    });
    let configs = episode.configurations().unwrap();
    let mut session = episode_session_v1(&episode, &configs).unwrap();
    for row in &base.decisions[..k] {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("terminal inside the prefix");
        };
        session.step(d.episode_id, d.step, row.selected).unwrap();
    }
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        panic!("terminal cut");
    };
    let (observation, menu) = crate::regret_census_v1::decision_replay_key(&session, d).unwrap();
    let reset = episode.reset.as_mut().unwrap();
    reset.cut_observation_sha256 = observation;
    reset.cut_menu_sha256 = menu;
    reset.cut_state_hash = format!("{:016x}", session.diagnostic_state_hash());
    episode
}

/// The learner first-substep row nearest the middle of the game.
fn middle_learner_start(t: &ExpandedTrajectoryV1) -> usize {
    let half = t.decisions.len() / 2;
    (half..t.decisions.len())
        .chain((1..half).rev())
        .find(|&i| {
            let r = &t.decisions[i];
            r.actor == t.episode.learner_seat && r.substep_index == 0
        })
        .expect("a learner decision start")
}

fn collected_base() -> (
    FrozenPlayPolicyV1,
    ExpandedSeatBehaviorV1,
    ExpandedTrajectoryV1,
) {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let learner = test_behavior(&policy, false);
    let base = collect_episode(&mut policy, &learner, None, &base_episode(2_026_101_017)).unwrap();
    assert!(base.decisions.len() > 20, "base game too short to cut");
    (policy, learner, base)
}

#[test]
#[ignore = "root-owned native qualification: real complete games"]
fn reset_restores_the_cut_and_records_only_the_suffix() {
    let (mut policy, learner, base) = collected_base();
    let k = middle_learner_start(&base);
    let episode = reset_at(&base, k, 77);
    let t = collect_episode(&mut policy, &learner, None, &episode).unwrap();
    validate_trajectory(&t).unwrap();
    // Same state, same weights: the first suffix row is the original row.
    let (first, original) = (&t.decisions[0], &base.decisions[k]);
    assert_eq!(first.step, k as u64);
    assert_eq!(first.physical_decision_id, original.physical_decision_id);
    assert_eq!(first.actor, original.actor);
    assert_eq!(first.substep_count, original.substep_count);
    assert_eq!(
        serde_json::to_value(&first.tensor).unwrap(),
        serde_json::to_value(&original.tensor).unwrap(),
        "actor-visible tensor at the cut"
    );
    assert_eq!(first.logits, original.logits);
    assert_eq!(first.value, original.value);
    assert_eq!(
        t.terminal.policy_step_count,
        k as u64 + t.decisions.len() as u64
    );
    assert!(t.decisions.iter().all(|r| r.step >= k as u64));
    // The runtime profile holds for the whole game.
    let configs = episode.configurations().unwrap();
    let mut session = episode_session_v1(&episode, &configs).unwrap();
    assert_eq!(
        session.game_state().engine.runtime_rules,
        crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1
    );
    // The original recorded continuation from the restored cut reaches the
    // original terminal, decision by decision.
    restore_reset_cut_v1(&mut session, &episode).unwrap();
    for row in &base.decisions[k..] {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("restored game ended early");
        };
        assert_eq!(d.step, row.step);
        assert_eq!(d.physical_decision_id, row.physical_decision_id);
        assert_eq!(d.legal_action_count as usize, row.logits.len());
        session.step(d.episode_id, d.step, row.selected).unwrap();
    }
    let FastActorResponseV1::Terminal(end) = session.current_response() else {
        panic!("restored game did not end where the original did");
    };
    assert_eq!(end.winner, base.terminal.winner);
    assert_eq!(end.policy_step_count, base.terminal.policy_step_count);
    assert_eq!(
        end.physical_decision_count,
        base.terminal.physical_decision_count
    );
}

#[test]
#[ignore = "root-owned native qualification: real complete games"]
fn reset_prefix_never_enters_learner_groups() {
    let (mut policy, learner, base) = collected_base();
    let k = middle_learner_start(&base);
    let episode = reset_at(&base, k, 78);
    let t = collect_episode(&mut policy, &learner, None, &episode).unwrap();
    let groups = replay_learner_groups_v1(0, &t, &policy, None).unwrap();
    let starts = t
        .decisions
        .iter()
        .filter(|r| r.actor == t.episode.learner_seat && r.substep_index == 0)
        .count();
    assert_eq!(
        groups.len(),
        starts,
        "one group per suffix learner decision"
    );
    let reward = t.terminal.terminal_reward[t.episode.learner_seat as usize] as i8;
    for (r, ordinal, rows) in &groups {
        assert_eq!(*r, reward, "only the suffix's own terminal reward");
        assert_eq!(*ordinal, 0);
        assert!(rows
            .iter()
            .all(|(row, _)| row.step >= k as u64 && row.actor == t.episode.learner_seat));
    }
    // Perturbing what the guide carried is rejected or inert: its label
    // changes no group, and a changed prefix action fails the cut's
    // full-state check instead of training from a different history.
    let mut relabelled = t.clone();
    relabelled.episode.reset.as_mut().unwrap().guide = "another-guide".into();
    let again = replay_learner_groups_v1(0, &relabelled, &policy, None).unwrap();
    assert_eq!(again.len(), groups.len());
    for (a, b) in again.iter().zip(&groups) {
        assert_eq!(a.0, b.0);
        assert_eq!(a.2.len(), b.2.len());
        for ((ra, _), (rb, _)) in a.2.iter().zip(&b.2) {
            assert_eq!(ra.logits, rb.logits);
            assert_eq!(
                serde_json::to_value(&ra.tensor).unwrap(),
                serde_json::to_value(&rb.tensor).unwrap()
            );
        }
    }
    let mut changed = episode.clone();
    let prefix = &mut changed.reset.as_mut().unwrap().prefix;
    let i = prefix.iter().position(|&a| a > 0).unwrap_or(0);
    prefix[i] = if prefix[i] > 0 { 0 } else { 1 };
    assert!(collect_episode(&mut policy, &learner, None, &changed).is_err());
}

#[test]
#[ignore = "root-owned native qualification: real complete games"]
fn reset_rejects_terminal_opponent_and_mismatched_cuts() {
    let (mut policy, learner, base) = collected_base();
    let k = middle_learner_start(&base);
    // A terminal "cut": the whole game as prefix.
    let mut terminal = reset_at(&base, k, 79);
    terminal.reset.as_mut().unwrap().prefix = base.decisions.iter().map(|r| r.selected).collect();
    let error = collect_episode(&mut policy, &learner, None, &terminal).unwrap_err();
    assert!(error.to_string().contains("terminal"), "{error}");
    // A cut at the other seat's decision.
    let opponent_row = base
        .decisions
        .iter()
        .position(|r| r.actor != base.episode.learner_seat && r.substep_index == 0)
        .unwrap();
    let mut wrong_seat = reset_at(&base, opponent_row, 80);
    wrong_seat.reset.as_mut().unwrap().cut_physical_decision_id =
        base.decisions[opponent_row].physical_decision_id;
    assert!(collect_episode(&mut policy, &learner, None, &wrong_seat).is_err());
    // A recorded observation that differs.
    let mut mismatch = reset_at(&base, k, 81);
    mismatch.reset.as_mut().unwrap().cut_observation_sha256 = "0".repeat(64);
    let error = collect_episode(&mut policy, &learner, None, &mismatch).unwrap_err();
    assert!(error.to_string().contains("differs"), "{error}");
}

/// Serialization keeps ordinary episodes byte-identical.
#[test]
fn ordinary_episode_serialization_is_unchanged() {
    let mut episode = base_episode(5);
    episode.runtime_rules = None;
    let value = serde_json::to_value(&episode).unwrap();
    assert!(value.get("runtime_rules").is_none() && value.get("reset").is_none());
    let with = serde_json::to_value(base_episode(5)).unwrap();
    assert_eq!(with["runtime_rules"], "resolution-boundary-v1");
}
