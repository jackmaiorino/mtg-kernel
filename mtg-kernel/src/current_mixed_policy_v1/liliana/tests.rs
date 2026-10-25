//! Actual paid native bindings and numeric model source parity. Rust unrun.
use super::*;
use crate::{
    effect::liliana_dreadhorde_v1::tests::owner_v73_tests::owning_ready, ids::PlayerId,
    liliana_public_v73::ActivationStageV73, rl::ActionSemanticV1 as A,
};
#[test]
fn split_numeric_inputs_retain_every_original_observation_and_candidate_field_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _) = owning_ready(p, false, "Island");
        let own =
            LilianaSession::from_session(FastActorSessionV1::from_liliana_v73_fixture_state(s))
                .unwrap();
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            panic!()
        };
        let d = own.encode(h).unwrap();
        let original = d.backend.candidates_v73();
        let (mut state, actions) = packet_inputs(original, d.backend.actions()).unwrap();
        assert_eq!(
            packet_from_current(&d.backend).unwrap(),
            normalize::encode(Source::LilianaV73, state.clone(), actions.clone()).unwrap()
        );
        let fields = state.as_object_mut().unwrap();
        assert_eq!(
            fields.remove("candidate_count").unwrap(),
            original.candidates.len()
        );
        assert!(!fields.contains_key("candidates"));
        fields.insert("candidates".into(), Value::Array(actions));
        // Reconstruct the complete original typed envelope, including every
        // action annotation, in native candidate order. No field may vanish
        // when moving the per-action portion out of the state tree.
        assert_eq!(state, serde_json::to_value(original).unwrap());
        assert!(NativeModel::deterministic_fixture()
            .unwrap()
            .forward(d.packet())
            .unwrap()
            .iter()
            .all(|value| value.is_finite()));
    }
}
#[test]
fn streamed_projection_commitments_preserve_original_exact_json_and_nested_hashes_both_seats() {
    use sha2::{Digest, Sha256};
    fn original_hash(value: &impl serde::Serialize) -> String {
        Sha256::digest(serde_json::to_vec(value).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    }
    // Exercise both the buffered blocks and the final partial write, including
    // JSON escaping and multibyte text. The original full observations below
    // continue to check all actual nested fields in both seats.
    for size in [0, 8191, 8192, 8193, 16384] {
        let value = serde_json::json!({"prefix":"\"\\\nλ","payload":"x".repeat(size)});
        assert_eq!(
            crate::liliana_observation_v73::streamed_commitment(&value).unwrap(),
            original_hash(&value)
        );
    }
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _) = owning_ready(p, false, "Island");
        let own =
            LilianaSession::from_session(FastActorSessionV1::from_liliana_v73_fixture_state(s))
                .unwrap();
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            panic!()
        };
        let d = own.encode(h).unwrap();
        let mut candidates = d.backend.candidates_v73().clone();
        // Fresh construction may reuse its own just-computed commitment.
        // Supplied outer and nested hashes remain independent obligations.
        let authentic = candidates.observation.clone();
        crate::liliana_observation_v73::validate_artifact(&authentic).unwrap();
        for layer in 0..3 {
            let mut forged = authentic.clone();
            let hash = match layer {
                0 => &mut forged.projection_sha256,
                1 => &mut forged.common.projection_sha256,
                _ => &mut forged.common.common.projection_sha256,
            };
            let replacement = if hash.starts_with('0') { "1" } else { "0" };
            hash.replace_range(..1, replacement);
            if layer == 2 {
                forged.common.projection_sha256 =
                    crate::elenda_observation_v52::commitment(&forged.common).unwrap();
            }
            if layer != 0 {
                forged.projection_sha256 =
                    crate::liliana_observation_v73::commitment(&forged).unwrap();
            }
            assert!(crate::liliana_observation_v73::validate_artifact(&forged).is_err());
        }
        candidates.projection_sha256.clear();
        assert_eq!(
            crate::liliana_candidates_v73::commitment(&candidates).unwrap(),
            original_hash(&candidates)
        );
        let mut observation = candidates.observation.clone();
        observation.projection_sha256.clear();
        assert_eq!(
            crate::liliana_observation_v73::commitment(&observation).unwrap(),
            original_hash(&observation)
        );
        let prior = original_hash(&observation);
        observation.common.projection_sha256.push('0');
        assert_ne!(
            crate::liliana_observation_v73::commitment(&observation).unwrap(),
            prior
        );
        assert_eq!(
            crate::liliana_observation_v73::commitment(&observation).unwrap(),
            original_hash(&observation)
        );
        let prior = original_hash(&candidates);
        candidates.observation.projection_sha256.push('0');
        assert_ne!(
            crate::liliana_candidates_v73::commitment(&candidates).unwrap(),
            prior
        );
        assert_eq!(
            crate::liliana_candidates_v73::commitment(&candidates).unwrap(),
            original_hash(&candidates)
        );
    }
}
fn act(own: &mut LilianaSession, native: &mut FastActorSessionV1, choose: impl Fn(&A) -> bool) {
    let FastActorResponseV1::Decision(h) = own.current_response() else {
        panic!("numeric V73 ended")
    };
    eprintln!("Liliana numeric action step={}: encoding", h.step);
    let d = own.encode(h).unwrap();
    eprintln!("Liliana numeric action step={}: encoded", h.step);
    act_encoded(own, native, d, choose);
}
fn act_encoded(
    own: &mut LilianaSession,
    native: &mut FastActorSessionV1,
    d: LilianaDecision,
    choose: impl Fn(&A) -> bool,
) {
    let h = d.backend.expected();
    let i = d
        .backend
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("missing numeric V73 action: {:?}", d.backend.actions()));
    let original = d.backend.fixture_original_index_v73(i as u32).unwrap();
    eprintln!("Liliana numeric action step={}: model forward", h.step);
    let scores = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(d.packet())
        .unwrap();
    eprintln!("Liliana numeric action step={}: model completed", h.step);
    assert_eq!(scores.len(), d.backend.actions().len());
    assert!(scores.iter().all(|v| v.is_finite()));
    assert_eq!(d.packet().source(), Source::LilianaV73);
    let applied = own.apply(&d, i as u32).unwrap();
    eprintln!(
        "Liliana numeric action step={}: owning apply completed",
        h.step
    );
    assert_eq!(
        applied,
        native.step(h.episode_id, h.step, original).unwrap()
    );
    eprintln!(
        "Liliana numeric action step={}: native apply completed",
        h.step
    );
    assert_eq!(own.fixture_state(), native.liliana_fixture_state());
    assert_eq!(
        own.fixture_counts(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}
fn complete(own: &mut LilianaSession, native: &mut FastActorSessionV1) {
    for _ in 0..512 {
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            panic!(
                "numeric V73 ended before native completion: {:?}",
                own.current_response()
            )
        };
        let d = own.encode(h).unwrap();
        if d.backend
            .observation()
            .liliana
            .activations
            .last()
            .is_some_and(|a| a.stage == ActivationStageV73::NativeCompleted)
        {
            return;
        }
        let finish = d
            .backend
            .actions()
            .iter()
            .any(|a| matches!(a, A::FinishEffectSelection { .. }));
        // Score and apply this exact issued decision. Re-encoding the same
        // unchanged native root would retain two full numeric packets here.
        act_encoded(own, native, d, |a| {
            if finish {
                matches!(a, A::FinishEffectSelection { .. })
            } else {
                matches!(
                    a,
                    A::Pass { .. }
                        | A::OrderTriggers { .. }
                        | A::ChooseTriggerOrderNext { .. }
                        | A::ChooseEffectTarget { .. }
                        | A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            }
        });
    }
    panic!("numeric native root incomplete")
}
#[test]
fn paid_numeric_all_three_original_loyalty_roots_and_native_choice_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            eprintln!("Liliana numeric root seat={p:?} ability={index}: setup");
            let (s, id) = owning_ready(p, index == 2, "Island");
            eprintln!("Liliana numeric root seat={p:?} ability={index}: ready");
            let mut native = FastActorSessionV1::from_liliana_v73_fixture_state(s.clone());
            eprintln!("Liliana numeric root seat={p:?} ability={index}: native session ready");
            let own_native = FastActorSessionV1::from_liliana_v73_fixture_state(s);
            eprintln!("Liliana numeric root seat={p:?} ability={index}: owning native ready");
            let mut own = LilianaSession::from_session(own_native).unwrap();
            eprintln!("Liliana numeric root seat={p:?} ability={index}: owning session ready");
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index),
            );
            eprintln!("Liliana numeric root seat={p:?} ability={index}: activated");
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let old = own.encode(h).unwrap();
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            complete(&mut own, &mut native);
            eprintln!("Liliana numeric root seat={p:?} ability={index}: completed");
            let expected = own.fixture_state().clone();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let completed = own.encode(h).unwrap();
            let facts = &completed.backend.observation().liliana;
            assert_eq!(facts.activations[0].ability_index, index);
            if index == 0 {
                assert_eq!(facts.activations[0].created_zombies.len(), 1);
                assert!(printing(492).unwrap().is_some());
            } else {
                assert!(facts.resolutions[0].finished);
                assert_eq!(facts.resolutions[0].ultimate, index == 2);
            }
            own.restore(&saved).unwrap();
            native.liliana_restore_v73(&raw).unwrap();
            eprintln!("Liliana numeric root seat={p:?} ability={index}: restored");
            let before = own.fixture_state().clone();
            assert!(own.apply(&old, 0).is_err());
            assert_eq!(own.fixture_state(), &before);
            complete(&mut own, &mut native);
            eprintln!("Liliana numeric root seat={p:?} ability={index}: replay completed");
            assert_eq!(own.fixture_state(), &expected);
        }
    }
}
#[test]
fn paid_numeric_original_sourcegone_and_same_incarnation_concealment_bind_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s,_,generation)=crate::effect::liliana_dreadhorde_v1::tests::owner_v73_tests::published_source_boundary(p,concealed);
            let mut native = FastActorSessionV1::from_liliana_v73_fixture_state(s.clone());
            let mut own =
                LilianaSession::from_session(FastActorSessionV1::from_liliana_v73_fixture_state(s))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            assert_eq!(
                d.backend.observation().liliana.activations[0]
                    .source
                    .original_source
                    .generation,
                generation
            );
            complete(&mut own, &mut native);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            assert_eq!(
                d.backend.observation().liliana.activations[0].created_zombies[0].controller,
                p.into()
            );
        }
    }
}
#[test]
fn paid_numeric_foreign_forged_packet_and_action_refusals_leave_native_unchanged_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _) = owning_ready(p, false, "Island");
        let mut a = LilianaSession::from_session(
            FastActorSessionV1::from_liliana_v73_fixture_state(s.clone()),
        )
        .unwrap();
        let mut b =
            LilianaSession::from_session(FastActorSessionV1::from_liliana_v73_fixture_state(s))
                .unwrap();
        let FastActorResponseV1::Decision(h) = a.current_response() else {
            panic!()
        };
        let d = a.encode(h).unwrap();
        let before = a.fixture_state().clone();
        let counts = a.fixture_counts();
        assert!(b.apply(&d, 0).is_err());
        assert!(a.restore(&b.snapshot()).is_err());
        let mut erased = d.clone();
        erased.packet.actions.clear();
        assert!(a.apply(&erased, 0).is_err());
        let mut changed = d.clone();
        changed.packet.state = crate::current_mixed_policy_v1::NumericTree::encode(
            &serde_json::json!({"replacement":"forged"}),
        )
        .unwrap();
        assert!(a.apply(&changed, 0).is_err());
        let mut forged = d.backend.candidates_v73().clone();
        forged.observation.liliana.activations.clear();
        forged.observation.liliana.authorized_printings.clear();
        assert!(packet_inputs(&forged, d.backend.actions()).is_err());
        assert!(a.apply(&d, u32::MAX).is_err());
        assert_eq!(a.fixture_state(), &before);
        assert_eq!(a.fixture_counts(), counts);
    }
}
#[test]
fn independently_paid_numeric_hidden_world_equality_and_full_native_integer_distinction_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut packets = vec![];
        for hidden in ["Island", "Imprisoned in the Moon"] {
            let (s, _) = owning_ready(p, false, hidden);
            let own =
                LilianaSession::from_session(FastActorSessionV1::from_liliana_v73_fixture_state(s))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            packets.push(own.encode(h).unwrap().packet().clone());
        }
        assert_eq!(packets[0], packets[1]);
        let (s, id) = owning_ready(p, false, "Island");
        let mut wide = vec![];
        for count in [i32::MAX - 1, i32::MAX] {
            let mut world = s.clone();
            let c = &mut world.objects.get_mut(id).counters;
            // Explicit native storage boundary on a genuinely six-mana paid printing.
            c.stash = count;
            c.fellowship = i32::MAX;
            c.charge = i32::MAX;
            let own = LilianaSession::from_session(
                FastActorSessionV1::from_liliana_v73_fixture_state(world),
            )
            .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            let current = d.backend.observation().liliana.battlefield[p.index()]
                .iter()
                .find(|c| c.card.object == id.0)
                .unwrap();
            assert_eq!(current.counters.stash, count);
            assert!(NativeModel::deterministic_fixture()
                .unwrap()
                .forward(d.packet())
                .unwrap()
                .iter()
                .all(|v| v.is_finite()));
            wide.push(d.packet().clone());
        }
        assert_ne!(wide[0], wide[1]);
    }
}
