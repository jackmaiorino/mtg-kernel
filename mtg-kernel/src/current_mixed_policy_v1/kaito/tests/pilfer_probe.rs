//! Actual paid reveal/discard prompts, including duplicate public names.
use super::*;
use crate::{
    current_actor_probe_v1::collision_counts_v2, current_tree_learner_v1::AuthenticatedInputV1,
};

#[test]
fn issue454_probe_public_families_and_mapped_apply_are_exact_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut paired = None;
        for names in [
            ["Giant Growth", "Hero's Downfall", "Hero's Downfall"],
            ["Hero's Downfall", "Giant Growth", "Hero's Downfall"],
        ] {
            let (mut state, source) = scene(actor);
            for name in names {
                put(&mut state, actor.opponent(), name, Zone::Hand);
            }
            let (mut owner, mut native, _) = full_pool_pending(actor, state, source);
            let FastActorResponseV1::Decision(header) = owner.current_response() else {
                panic!("paid choice");
            };
            let decision = owner.encode(header).unwrap();
            let capture = owner.probe_capture_v2(&decision).unwrap();
            let training = owner.training_input_v1(&decision).unwrap();
            assert_eq!(capture.input.packet(), training.packet());
            assert_eq!(capture.input.action_flags(), training.action_flags());
            let order = decision.current.name_order.as_ref().unwrap();
            assert_eq!(
                capture.actions,
                order
                    .iter()
                    .map(|&i| decision.backend.actions()[i as usize].clone())
                    .collect::<Vec<_>>()
            );
            assert!(capture.families.iter().all(Option::is_some));
            if let Some(packet) = &paired {
                assert_eq!(capture.input.packet(), packet);
            } else {
                paired = Some(capture.input.packet().clone());
            }

            let counts =
                collision_counts_v2(&capture.input, &capture.actions, &capture.families).unwrap();
            assert_eq!((counts.neutral, counts.explained), (1, 1));
            let no_proof = collision_counts_v2(
                &capture.input,
                &capture.actions,
                &vec![None; capture.actions.len()],
            )
            .unwrap();
            assert_eq!((no_proof.neutral, no_proof.explained), (1, 0));

            // Force all numeric action rows equal as a negative encoding
            // control. This counterfactual capture is never applied or trained.
            // Native family proofs must still explain only the duplicate name.
            let mut corrupt = capture.input.packet().clone();
            let first = corrupt.actions[0].clone();
            corrupt.actions.fill(first);
            let corrupt = AuthenticatedInputV1::from_current(corrupt, &capture.actions).unwrap();
            let wrong_encoding =
                collision_counts_v2(&corrupt, &capture.actions, &capture.families).unwrap();
            assert_eq!((wrong_encoding.neutral, wrong_encoding.explained), (3, 1));
            let repeated_proof = vec![capture.families[0].clone(); capture.actions.len()];
            let transplanted =
                collision_counts_v2(&corrupt, &capture.actions, &repeated_proof).unwrap();
            assert!(transplanted.explained < transplanted.neutral);
            let separate_capture = owner.probe_capture_v2(&decision).unwrap();
            for i in 0..capture.actions.len() {
                assert!(!capture.families[i].as_ref().unwrap().equivalent_actions(
                    &capture.actions[i],
                    separate_capture.families[i].as_ref().unwrap(),
                    &capture.actions[i]
                ));
            }

            let saved = owner.snapshot();
            let native_saved = native.snapshot_v1();
            for (public_index, &backend_index) in order.iter().enumerate() {
                owner.restore(&saved).unwrap();
                native.kaito_restore_v75(&native_saved).unwrap();
                assert!(owner.probe_capture_v2(&decision).is_err());
                assert!(owner
                    .probe_apply_v1(&decision, public_index as u32)
                    .is_err());
                let FastActorResponseV1::Decision(header) = owner.current_response() else {
                    panic!("restored choice");
                };
                let fresh = owner.encode(header).unwrap();
                let original = fresh
                    .backend
                    .fixture_original_index_v75(backend_index)
                    .unwrap();
                owner.probe_apply_v1(&fresh, public_index as u32).unwrap();
                native
                    .step(header.episode_id, header.step, original)
                    .unwrap();
                assert_eq!(owner.current_response(), native.current_response());
                assert_eq!(owner.fixture_state(), native.kaito_fixture_state());
                assert!(owner.probe_capture_v2(&fresh).is_err());
            }
        }
    }
}
