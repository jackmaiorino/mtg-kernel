//! CPU fixtures for the opt-in frozen-current opponent. No device is created.
use super::*;

fn policy(enabled: bool) -> PublicInputPlayPolicyV1 {
    let mut projections = ProjectionSnapshot::zero();
    projections.object[0] = 0.125f32.to_bits();
    projections.state[0] = (-0.25f32).to_bits();
    PublicInputPlayPolicyV1::new(
        FrozenPlayPolicyV1::training_fixture_v4(),
        weights(&projections).unwrap(),
    )
    .unwrap()
    .with_inputs_enabled(enabled)
}

#[test]
fn current_declaration_is_explicit_and_exclusive() {
    let kind = ExpandedOpponentKindV1::Current {};
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        json!({"kind":"current"})
    );
    assert!(serde_json::from_value::<ExpandedOpponentKindV1>(
        json!({"kind":"current", "checkpoint":null})
    )
    .is_err());
    let mut e = episode(0, Some(kind));
    let bytes = serde_json::to_vec(&e).unwrap();
    let back: ExpandedEpisodeV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&back).unwrap(), bytes);
    let mut unknown = serde_json::to_value(&e).unwrap();
    unknown["opponent_kind"]["checkpoint"] = Value::Null;
    assert!(serde_json::from_value::<ExpandedEpisodeV1>(unknown).is_err());
    assert!(e.configurations_for_public_collector_v1().is_ok());
    assert!(e.configurations().is_err());
    e.opponent = episode(0, None).opponent;
    assert!(e.configurations_for_public_collector_v1().is_err());
    e.opponent = None;
    e.opponent_search = Some(pin("not-loaded.json", &"ab".repeat(32)));
    assert!(e.configurations_for_public_collector_v1().is_err());
}

#[test]
fn current_frozen_weights_actor_views_sampler_and_learner_isolation() {
    for enabled in [false, true] {
        for learner_seat in [0, 1] {
            let frozen = policy(enabled);
            let mut e = episode(learner_seat, Some(ExpandedOpponentKindV1::Current {}));
            e.runtime_rules = Some(EpisodeRuntimeRulesV1::ResolutionBoundaryV1);
            let mut t = super::super::super::collect(
                &mut frozen.fork_for_collection().unwrap(),
                &e,
                "config",
                "state",
                enabled,
                None,
            )
            .unwrap();
            assert_eq!(t.schema, CURRENT_TRAJECTORY_SCHEMA);
            assert!(!admits_public_trajectory(&t));
            t.validate_records().unwrap();
            let mut actors = [
                frozen.fork_for_collection().unwrap(),
                frozen.fork_for_collection().unwrap(),
            ];
            for actor in &mut actors {
                actor
                    .reset_for_game_v1(paired_policy_seeds_v1(e.seed))
                    .unwrap();
            }
            let mut session =
                episode_session_v1(&e, &e.configurations_for_public_collector_v1().unwrap())
                    .unwrap();
            // Independently replay each physical actor's own visible input.
            // Compare tensors/public rows as well as scores/actions: sharing a
            // learner view or the other physical seat's RNG cannot pass.
            for (row, auxiliary) in t.decisions.iter().zip(&t.auxiliary) {
                let FastActorResponseV1::Decision(d) = session.current_response() else {
                    panic!("early terminal")
                };
                let actor = &mut actors[seat(d.acting_player) as usize];
                let (selected, scores) = actor
                    .select_with_scores(&PairedBo1PolicyInputV1::new(&session, d))
                    .unwrap();
                let (tensor, public) = actor.captured().unwrap();
                assert_eq!(selected, row.selected);
                assert_eq!(bits(&scores.logits), row.logits);
                assert_eq!(scores.value.to_bits(), row.value);
                assert_eq!(
                    serde_json::to_vec(&TensorBitsV1::from_tensor(&tensor.common)).unwrap(),
                    serde_json::to_vec(&row.tensor).unwrap()
                );
                assert_eq!(
                    serde_json::to_vec(public).unwrap(),
                    serde_json::to_vec(auxiliary.as_ref().unwrap()).unwrap()
                );
                session.step(d.episode_id, d.step, selected).unwrap();
            }
            let groups = |t: &Trajectory| {
                super::super::super::learner_groups(t)
                    .unwrap()
                    .iter()
                    .map(|g| {
                        g.iter()
                            .map(|(row, _, auxiliary)| {
                                assert_eq!(row.actor, learner_seat);
                                serde_json::to_vec(&(row, auxiliary)).unwrap()
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            let original = groups(&t);
            assert!(!original.is_empty());
            let mut opponent_rows = 0;
            for (row, auxiliary) in t.decisions.iter_mut().zip(&mut t.auxiliary) {
                if row.actor != learner_seat {
                    opponent_rows += 1;
                    row.value = f32::NAN.to_bits();
                    row.logits.clear();
                    *auxiliary = None;
                }
            }
            assert!(opponent_rows > 0);
            assert_eq!(
                groups(&t),
                original,
                "opponent values/rows must not enter GAE or normalization inputs"
            );
            t.optimizer_state_sha256 = "different-update".into();
            assert!(t
                .validate_records()
                .unwrap_err()
                .contains("frozen learner state"));
        }
    }
}

#[test]
fn current_serial_parallel_primary_bytes_match() {
    let frozen = policy(true);
    let episodes: Vec<_> = [0, 1]
        .into_iter()
        .map(|seat| episode(seat, Some(ExpandedOpponentKindV1::Current {})))
        .collect();
    let mut serial_policy = frozen.fork_for_collection().unwrap();
    let serial: Vec<_> = episodes
        .iter()
        .map(|episode| {
            super::super::super::collect(&mut serial_policy, episode, "config", "state", true, None)
                .unwrap()
        })
        .collect();
    let parallel = super::super::super::collect_parallel(
        &frozen, &episodes, "config", "state", true, None, 2, &mut 0.0,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_vec(&serial).unwrap(),
        serde_json::to_vec(&parallel).unwrap()
    );
}
