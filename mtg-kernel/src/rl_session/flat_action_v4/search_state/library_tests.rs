use super::*;
use crate::engine::{self,Action,Decision};
use crate::policy_observation_v6::tests::{put,ready_state};
use crate::effect::{PendingEffectChoice,EffectTargetSelectionPurpose};

pub(super) fn fixture(actor:PlayerId, form:u8, names:&[&str], reverse:bool, known:bool, partial:bool)->GameState {
    let mut s=ready_state();s.active_player=actor;s.priority_player=actor;
    let source=put(&mut s,actor,match form {0=>"Generous Ent",1=>"Squadron Hawk",_=>"Twisted Landscape"},if form==2{Zone::Battlefield}else{Zone::Hand});
    for name in names {put(&mut s,actor,name,Zone::Library);}
    for name in ["Forest","Island","Mountain"] {put(&mut s,actor.opponent(),name,Zone::Library);}
    if reverse {s.players[actor.index()].library.reverse();}
    if known {s.reveal_library_top(actor,actor,1);}
    s.players[actor.index()].mana_pool[crate::mana::ManaColor::G.pool_index()]=4;
    s.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()]=4;
    engine::step(&mut s,if form==1{Action::CastSpell(source)}else{Action::ActivateAbility(source,0)}).unwrap();
    for _ in 0..24 {
        match engine::advance_until_decision(&mut s) {
            Decision::CastSpellOrPass{..}=>engine::step(&mut s,Action::Pass).unwrap(),
            Decision::ChooseEffectTargets{legal_targets,..}=>{
                if partial {
                    engine::step(&mut s,Action::ChooseEffectTarget(legal_targets[0])).unwrap();
                    assert!(matches!(engine::advance_until_decision(&mut s),Decision::ChooseEffectTargets{selected_count:1,..}));
                }
                return s;
            },
            other=>panic!("form={form} unexpected {other:?}"),
        }
    }
    panic!("search never suspended")
}
fn decision(s:&FastActorSessionV1)->FastActorDecisionV1 {
    let FastActorResponseV1::Decision(d)=s.current_response() else {panic!("missing decision")}; d
}

#[test]
fn v4_library_v2_forms_seats_empty_partial_and_boundary() {
    for actor in [PlayerId::P0,PlayerId::P1] {for form in 0..3 {
        let cases:Vec<Vec<&str>>=if form==1 {
            vec![vec!["Forest","Island"],vec!["Squadron Hawk"],vec!["Squadron Hawk";3],vec!["Squadron Hawk","Forest","Squadron Hawk","Island"]]
        } else {
            vec![vec!["Lightning Bolt","Counterspell"],vec!["Forest"],vec!["Forest";3],vec!["Forest","Snow-Covered Forest","Forest","Lightning Bolt"]]
        };
        for names in cases {for known in [false,true] {
            let s=FastActorSessionV1::from_v3_fixture_state(fixture(actor,form,&names,false,known,false));
            let original=serde_json::to_vec(&s.state).unwrap();let d=decision(&s);
            assert!(matches!(s.kernel_search_redeterminized_clone_v4(81),Err(Error::DecisionLocalLibrary)));
            let sample=s.kernel_search_redeterminized_clone_library_v2(81).unwrap();
            let repeat=s.kernel_search_redeterminized_clone_library_v2(81).unwrap();
            assert_eq!(serde_json::to_vec(&sample.state).unwrap(),serde_json::to_vec(&repeat.state).unwrap());
            assert_eq!(boundary(&s,d,V4SearchSampleMode::LibraryChoiceV2).unwrap(),boundary(&sample,d,V4SearchSampleMode::LibraryChoiceV2).unwrap());
            assert_eq!(s.kernel_search_visible_key_v4(8),sample.kernel_search_visible_key_v4(8));
            assert_eq!(s.kernel_search_action_token_v4(d),sample.kernel_search_action_token_v4(d));
            assert_eq!(super::tests::tensor_and_output(&s),super::tests::tensor_and_output(&sample));
            if known {assert_eq!(s.state.players[actor.index()].library[0],sample.state.players[actor.index()].library[0]);}
            for index in 0..d.legal_action_count {
                let mut direct=sample.clone();let mut through=sample.clone();
                let selected=match &sample.current.as_ref().unwrap().candidates[index as usize].semantic {
                    ActionSemanticV1::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}=>Some(ObjectId(object.arena_id)),
                    _=>None,
                };
                let token=through.kernel_search_action_token_v4(d).unwrap();
                let expected=direct.step(d.episode_id,d.step,index).unwrap();
                assert_eq!(through.kernel_search_consume_v4(d,token,index).unwrap(),expected);
                assert_eq!(direct.state,through.state);
                // Completion including the selected physical card and shuffle
                // must remain valid. A many-form may need an explicit Finish.
                if through.state.engine.pending_effect.as_ref().and_then(|p|p.choice.as_ref()).is_some_and(|c|matches!(c,PendingEffectChoice::SelectTargets{..})) {
                    let nd=decision(&through);
                    let finish=through.current.as_ref().unwrap().candidates.iter().position(|c|matches!(c.semantic,ActionSemanticV1::FinishEffectSelection{..})).unwrap();
                    through.step(nd.episode_id,nd.step,finish as u32).unwrap();
                }
                assert!(through.state.engine.halted.is_none());
                if let Some(id)=selected {
                    assert_eq!(through.state.objects.get(id).zone,if form==2{Zone::Battlefield}else{Zone::Hand});
                    if form==2 {assert!(through.state.objects.get(id).tapped);}
                }
                if let Some(top)=through.state.players[actor.index()].library.first().copied() {
                    assert_eq!(through.state.draw_card(actor),Some(top));
                    assert_eq!(through.state.objects.get(top).zone,Zone::Hand);
                }
            }
            if !known {
                let reversed=FastActorSessionV1::from_v3_fixture_state(fixture(actor,form,&names,true,false,false));
                let r=reversed.kernel_search_redeterminized_clone_library_v2(81).unwrap();
                assert_eq!(sample.state.players[actor.index()].library,r.state.players[actor.index()].library);
            }
            assert_eq!(original,serde_json::to_vec(&s.state).unwrap());
        }}
        if form==1 {
            let s=FastActorSessionV1::from_v3_fixture_state(fixture(actor,form,&["Squadron Hawk","Squadron Hawk","Squadron Hawk","Forest"],false,true,true));
            let sample=s.kernel_search_redeterminized_clone_library_v2(81).unwrap();
            assert_eq!(s.current.as_ref().unwrap().candidates,sample.current.as_ref().unwrap().candidates);
            assert_eq!(boundary(&s,decision(&s),V4SearchSampleMode::LibraryChoiceV2).unwrap(),boundary(&sample,decision(&sample),V4SearchSampleMode::LibraryChoiceV2).unwrap());
        }
    }}
}

#[test]
fn v4_library_v2_nonlibrary_is_exact_legacy_and_hidden_diversity() {
    for actor in [PlayerId::P0,PlayerId::P1] {
        let s=FastActorSessionV1::from_v3_fixture_state(super::tests::state(actor));
        let a=s.kernel_search_redeterminized_clone_v4(81).unwrap();let b=s.kernel_search_redeterminized_clone_library_v2(81).unwrap();
        assert_eq!(a.state,b.state);assert_eq!(a.current.as_ref().unwrap().candidates,b.current.as_ref().unwrap().candidates);
        let s=FastActorSessionV1::from_v3_fixture_state(fixture(actor,0,&["Forest","Forest","Snow-Covered Forest","Lightning Bolt"],false,false,false));
        let libraries:std::collections::BTreeSet<_>=(1..=8).map(|seed|s.kernel_search_redeterminized_clone_library_v2(seed).unwrap().state.players[actor.index()].library.clone()).collect();
        assert!(libraries.len()>1);
    }
}

#[test]
fn v4_library_v2_plan_rejects_changed_contract_and_unrelated_references() {
    let state=fixture(PlayerId::P0,0,&["Forest","Forest","Island"],false,false,false);
    let plan=crate::effect::library_choice_search_v2::plan(&state,PlayerId::P0).unwrap().unwrap();
    for fault in 0..6 {
        let mut s=state.clone();
        let c=s.engine.pending_effect.as_mut().unwrap().choice.as_mut().unwrap();
        let PendingEffectChoice::SelectTargets{player,path,selected,legal,purpose,..}=c else{unreachable!()};
        match fault {
            0=>*player=PlayerId::P1,
            1=>path.push(9),
            2=>selected.push(legal[0].clone()),
            3=>legal[0].expected_object.as_mut().unwrap().expected_zone_change_count+=1,
            4=>{let EffectTargetSelectionPurpose::SearchLibraryToHand{filter,..}=purpose else{unreachable!()};*filter=crate::effect::LibraryCardFilter::BasicLand;},
            _=>legal[0].target=crate::state::Target::Object(s.players[0].library[2]),
        }
        let before=s.clone();assert!(plan.rebuild(&mut s).is_err());assert_eq!(s,before);
    }
    let mut s=state.clone();let id=s.players[0].library[0];
    s.engine.initiative_source=Some(crate::state::AbilitySourceContractV4::capture(&s,id));
    let session=FastActorSessionV1::from_v3_fixture_state(s);
    let before=session.state.clone();assert!(session.kernel_search_redeterminized_clone_library_v2(81).is_err());assert_eq!(session.state,before);
    let mut s=state.clone();
    let pending=s.engine.pending_effect.as_mut().unwrap();
    let Some(PendingEffectChoice::SelectTargets{purpose:EffectTargetSelectionPurpose::SearchLibraryToHand{player,filter,filter_fingerprint,original_library,canonical_path},..})=&pending.choice else{unreachable!()};
    pending.frames.push(crate::effect::EffectFrame::SearchLibraryToHand{player:*player,filter:*filter,filter_fingerprint:*filter_fingerprint,original_library:original_library.clone(),selected:None,path:canonical_path.clone(),canonical_path:canonical_path.clone()});
    let session=FastActorSessionV1::from_v3_fixture_state(s);
    let before=session.state.clone();assert!(session.kernel_search_redeterminized_clone_library_v2(81).is_err());assert_eq!(session.state,before);
}

#[test]
#[cfg(feature="experimental-burn-net8-packed-cuda-v1")]
fn v4_library_v2_bound_report_all_forms_and_legacy_nonlibrary_parity() {
    use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
    use crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1;
    use crate::model_guided_search_core_v4::{Limits,Error as SearchError,StateStage};
    let limits=Limits{simulations:8,transitions:24,depth:3,seed:29};
    for actor in [PlayerId::P0,PlayerId::P1] {
        let mut policy=FrozenPlayPolicyV1::training_fixture_v4();
        for form in 0..3 {
            let names=if form==1 {vec!["Squadron Hawk","Squadron Hawk","Forest"]}else{vec!["Forest","Forest","Island"]};
            let s=FastActorSessionV1::from_v3_fixture_state(fixture(actor,form,&names,false,false,false));
            policy.score_fast_session_v1(&s).unwrap();let d=decision(&s);let before=s.state.clone();
            let input=PairedBo1PolicyInputV1::new(&s,d);
            assert!(matches!(input.report_search_v4(&policy,limits),Err(SearchError::State{stage:StateStage::Redeterminize,source:Error::DecisionLocalLibrary})));
            let outcome=input.report_search_library_v2(&policy,limits).unwrap();
            assert!(outcome.estimator.is_some());assert_eq!(outcome.simulations,8);assert_eq!(before,s.state);
            let mut stale=d;stale.step+=1;
            assert_eq!(PairedBo1PolicyInputV1::new(&s,stale).report_search_library_v2(&policy,limits),Err(SearchError::InvalidAdapterBinding));
        }
        let s=FastActorSessionV1::from_v3_fixture_state(super::tests::state(actor));
        policy.score_fast_session_v1(&s).unwrap();let input=PairedBo1PolicyInputV1::new(&s,decision(&s));
        assert_eq!(input.report_search_v4(&policy,limits).unwrap(),input.report_search_library_v2(&policy,limits).unwrap());
    }
}

#[test]
fn v4_library_v2_documents_retained_known_card_shuffle_foresight() {
    let state=fixture(PlayerId::P0,0,&["Forest","Forest","Island","Mountain"],false,true,false);
    let known=state.players[0].library[0];
    let s=FastActorSessionV1::from_v3_fixture_state(state);let mut real=s.state.clone();
    real.shuffle_library(PlayerId::P0).unwrap();
    let actual=real.players[0].library.iter().position(|x|*x==known);
    for seed in 1..=8 {
        let mut sample=s.kernel_search_redeterminized_clone_library_v2(seed).unwrap();
        sample.state.shuffle_library(PlayerId::P0).unwrap();
        assert_eq!(actual,sample.state.players[0].library.iter().position(|x|*x==known));
        assert!(sample.state.known_library_cards(PlayerId::P0,PlayerId::P0).is_empty());
    }
}

#[test]
fn v4_library_stale_stored_origin_fails_live_choice_guard() {
    for actor in [PlayerId::P0,PlayerId::P1] {for form in 0..3 {
        let names=if form==1 {vec!["Squadron Hawk","Squadron Hawk","Squadron Hawk","Forest"]}else{vec!["Forest","Snow-Covered Forest","Forest","Lightning Bolt"]};
        let session=FastActorSessionV1::from_v3_fixture_state(fixture(actor,form,&names,false,false,false));
        let stored_origin=session.current.as_ref().unwrap().origin_decision.clone();
        let before=session.state.clone();
        let mut rejected=false;
        for seed in 1..=32 {
            let mut sampled=session.state.clone();
            let plan=crate::effect::library_choice_search_v2::plan(&sampled,actor).unwrap().unwrap();
            sampler::redeterminize_with_library_plan(&mut sampled,actor,seed,Some(&plan)).unwrap();
            plan.rebuild(&mut sampled).unwrap();
            let pending=sampled.engine.pending_effect.as_ref().unwrap();
            let live=crate::engine::pending_effect_targets_decision_v2(pending).unwrap();
            let live_origin=PolicyDecisionV5::Surface(crate::surface_v2::SurfaceDecision::Decision(live));
            let fresh=core_policy_action_candidates_v5(&live_origin,&sampled).unwrap();
            assert_eq!(validate_library_origin_candidates_v4(&sampled,&fresh),Ok(()));
            // Simulate the C2 regression: rebuilding rows from the old stored
            // decision instead of regenerating it from the sampled live choice.
            let stale=core_policy_action_candidates_v5(&stored_origin,&sampled).unwrap();
            if stale!=fresh {
                assert_eq!(validate_library_origin_candidates_v4(&sampled,&stale),Err(Error::LibraryChoiceOriginFailed));
                rejected=true;
                break;
            }
        }
        assert!(rejected,"fixture must actually expose a stale origin, actor={actor:?} form={form}");
        assert_eq!(session.state,before);
    }}
}
