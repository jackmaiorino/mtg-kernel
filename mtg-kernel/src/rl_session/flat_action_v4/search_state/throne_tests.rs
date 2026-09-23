use super::*;
use crate::effect::{PendingEffectChoice, EffectTargetSelectionPurpose};

fn root() -> GameState {
    let record: serde_json::Value = serde_json::from_str(include_str!("fixtures/throne_natural_root.json")).unwrap();
    serde_json::from_value(record["state"].clone()).unwrap()
}
fn decision(s: &FastActorSessionV1) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(d) = s.current_response() else { panic!("missing Throne decision") }; d
}
fn sample(s: &FastActorSessionV1, seed: u64) -> Result<FastActorSessionV1, Error> {
    s.kernel_search_redeterminized_clone_mode_v4(seed, V4SearchSampleMode::FutureChanceV3)
}

#[test]
fn v4_throne_v3_natural_root_preserves_reveal_menu_and_all_physical_choices() {
    let s = FastActorSessionV1::from_v3_fixture_state(root());
    let before = s.state.clone();
    let d = decision(&s);
    assert_eq!(d.legal_action_count, 5);
    assert!(matches!(s.kernel_search_redeterminized_clone_library_v2(81), Err(Error::HiddenStateContract)));
    let mut suffixes = std::collections::BTreeSet::new();
    for seed in 1..=16 {
        let copy = sample(&s, seed).unwrap();
        assert_eq!(copy.state, sample(&s, seed).unwrap().state);
        assert_eq!(&copy.state.players[1].library[..10], &s.state.players[1].library[..10]);
        assert_eq!(copy.state.library_knowledge, s.state.library_knowledge);
        assert_eq!(boundary(&s,d,V4SearchSampleMode::FutureChanceV3).unwrap(), boundary(&copy,d,V4SearchSampleMode::FutureChanceV3).unwrap());
        assert_eq!(s.kernel_search_visible_key_v4(8),copy.kernel_search_visible_key_v4(8));
        assert_eq!(s.kernel_search_action_token_v4(d),copy.kernel_search_action_token_v4(d));
        suffixes.insert(copy.state.players[1].library[10..].to_vec());
        for index in 0..d.legal_action_count {
            let ActionSemanticV1::ChooseEffectTarget { target: crate::rl::TargetRefV1::Object { object }, .. } = &copy.current.as_ref().unwrap().candidates[index as usize].semantic else { panic!("target") };
            let id = ObjectId(object.arena_id);
            let mut via = copy.clone(); let mut direct = copy.clone();
            let token = via.kernel_search_action_token_v4(d).unwrap();
            assert_eq!(via.kernel_search_consume_v4(d,token,index).unwrap(),direct.step(d.episode_id,d.step,index).unwrap());
            assert_eq!(via.state,direct.state);
            assert!(via.state.engine.halted.is_none());
            assert_eq!(via.state.objects.get(id).zone,Zone::Battlefield);
            assert_eq!(via.state.objects.get(id).counters.plus1_plus1,3);
            assert!(via.state.known_library_cards(PlayerId::P0,PlayerId::P1).is_empty());
            assert!(via.state.known_library_cards(PlayerId::P1,PlayerId::P1).is_empty());
        }
    }
    assert!(suffixes.len()>1);
    assert_eq!(before,s.state);
}

#[test]
fn v4_throne_v3_hidden_suffix_permutation_is_not_a_selection_signal() {
    let original = root(); let mut permuted = original.clone();
    permuted.players[1].library[10..].reverse();
    let current: Vec<_> = permuted.players[1].library.iter().map(|id| crate::effect::EffectObjectBinding {
        object:*id, expected_zone:Zone::Library, expected_zone_change_count:permuted.objects.get(*id).zone_change_count,
    }).collect();
    let Some(PendingEffectChoice::SelectTargets { purpose: EffectTargetSelectionPurpose::UndercityThroneCreature { original_library, .. }, .. }) = permuted.engine.pending_effect.as_mut().unwrap().choice.as_mut() else { panic!() };
    *original_library = current;
    let a = FastActorSessionV1::from_v3_fixture_state(original);
    let b = FastActorSessionV1::from_v3_fixture_state(permuted);
    assert_eq!(a.kernel_search_visible_key_v4(8),b.kernel_search_visible_key_v4(8));
    assert_eq!(sample(&a,81).unwrap().state,sample(&b,81).unwrap().state);
    assert_eq!(super::tests::tensor_and_output(&a),super::tests::tensor_and_output(&b));
}

#[test]
fn v4_throne_v3_rejects_changed_reveal_choice_and_unrelated_reference() {
    let state = root();
    let plan = crate::effect::library_choice_search_v2::plan_future_v3(&state,PlayerId::P1).unwrap().unwrap();
    assert!(crate::effect::library_choice_search_v2::plan(&state,PlayerId::P1).unwrap().is_none());
    for fault in 0..5 {
        let mut altered = state.clone();
        match fault {
            0 => altered.library_knowledge[0][1].clear(),
            1 => altered.players[1].library.swap(0,10),
            2 => {
                let Some(PendingEffectChoice::SelectTargets { legal, .. })=altered.engine.pending_effect.as_mut().unwrap().choice.as_mut() else { panic!() };
                legal.swap(0,1);
            },
            3 => {
                let id=altered.players[1].library[10];
                altered.engine.initiative_source=Some(crate::state::AbilitySourceContractV4::capture(&altered,id));
            },
            _ => {
                let id=altered.players[1].library[10];
                altered.objects.get_mut(id).v4.entered_battlefield_turn=Some(altered.turn);
            },
        }
        let before=altered.clone();
        if fault==1 || fault==2 {assert!(plan.rebuild(&mut altered).is_err());assert_eq!(before,altered);}
        let session=FastActorSessionV1::from_v3_fixture_state(before.clone());
        // Fixture construction itself validates corrupt continuations and may
        // mark them halted. The sampler must not mutate its input session.
        let session_before=session.state.clone();
        assert!(sample(&session,81).is_err(),"fault {fault}");
        assert!(session_before==session.state,"sampler mutated fault {fault}");
    }
}
