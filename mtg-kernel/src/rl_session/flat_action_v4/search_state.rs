//! Additive V4 hidden-state clone. No playing route or legacy guard changes.
use super::*;
use crate::state::GameState;
mod key_step;
mod sampler;
mod effect_refs;
#[cfg(test)]
mod sampler_tests;
#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod chance_tests;
#[cfg(test)]
mod throne_tests;

#[derive(Debug,Clone,Copy,PartialEq,Eq,serde::Serialize)]
pub(crate) enum V4SearchStateErrorV1 {
    UnsupportedActionContract,
    NoLiveDecision,
    DecisionLocalLibrary,
    InvalidVisibleBinding,
    HiddenStateContract,
    SampleCandidateRebuildFailed,
    SampleBoundaryEncodingFailed,
    SampleBoundaryChanged,
    LibraryChoicePlanFailed,
    LibraryChoiceRebuildFailed,
    LibraryChoiceOriginFailed,
    StepFailed,
    HaltedSimulation,
}
use V4SearchStateErrorV1 as Error;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub(crate) enum V4SearchSampleMode { Legacy, LibraryChoiceV2, FutureChanceV3 }

#[derive(Debug,PartialEq,Eq)]
struct Boundary {
    visible:Vec<u8>,
    scoring:crate::flat_policy_v4::FlatDecisionV4,
    slice:FlatActionDecisionSliceV3,
    actions:Vec<FlatActionCoreV1>,
    refs:Vec<FlatActionRefV2>,
    objects:Vec<FlatActionObjectV2>,
}
fn boundary(s:&FastActorSessionV1,d:FastActorDecisionV1,mode:V4SearchSampleMode)->Result<Boundary,Error> {
    let (observation,semantics)=s.diagnostic_current_decision_input_v4(d).map_err(|_|Error::InvalidVisibleBinding)?;
    if mode==V4SearchSampleMode::Legacy && observation.extensions.decision_local_library.is_some() {return Err(Error::DecisionLocalLibrary);}
    // Match the scorer's frozen-source semantics, not a relabeled hidden card.
    let semantics:Vec<_>=semantics.into_iter().map(|x|frozen_pending_trigger_semantic_v4(&s.state,x)).collect();
    let visible=serde_json::to_vec(&(observation,semantics)).map_err(|_|Error::InvalidVisibleBinding)?;
    let (mut scoring_objects,mut relations,mut subtypes,mut uses,mut goads,mut dungeons,
        mut changes,mut paths,mut scoring_actions,mut action_refs)=
        (Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new());
    let scoring=s.encode_current_flat_scoring_decision_owned_v4(d,&mut crate::flat_policy_v4::FlatDecisionEncoderV4::default(),
        &mut crate::flat_policy_v2::FlatScoringOwnedBuffersV2 {
            objects:&mut scoring_objects,relations:&mut relations,object_subtypes:&mut subtypes,
            ability_uses:&mut uses,goads:&mut goads,completed_dungeons:&mut dungeons,
            effect_subtype_changes:&mut changes,context_path_elements:&mut paths,
            actions:&mut scoring_actions,action_refs:&mut action_refs,
        }).map_err(|_|Error::InvalidVisibleBinding)?;
    let count=d.legal_action_count as usize;
    let capacity=count.checked_mul(FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1).ok_or(Error::InvalidVisibleBinding)?.max(256);
    let mut actions=vec![FlatActionCoreV1::default();count];
    let mut refs=vec![FlatActionRefV2::default();capacity];
    let mut objects=vec![FlatActionObjectV2::default();capacity];
    let slice=s.encode_current_flat_action_slice_v4(d,&mut FlatActionDecisionSliceBuffersV2{actions:&mut actions,refs:&mut refs,objects:&mut objects}).map_err(|_|Error::InvalidVisibleBinding)?;
    actions.truncate(slice.active_action_count as usize);refs.truncate(slice.active_ref_count as usize);objects.truncate(slice.active_object_count as usize);
    Ok(Boundary{visible,scoring,slice,actions,refs,objects})
}

// Keep the origin-to-live-choice check separate so a stale stored origin can
// be tested against the same guard used by production redeterminization.
fn validate_library_origin_candidates_v4(state:&GameState,candidates:&[CorePolicyActionCandidateV1])->Result<(),Error> {
    let Some(crate::effect::PendingEffectChoice::SelectTargets{legal,..})=state.engine.pending_effect.as_ref().and_then(|p|p.choice.as_ref()) else {return Err(Error::LibraryChoiceOriginFailed);};
    let expected:Vec<_>=legal.iter().map(|c|c.target).collect();
    let actual:Vec<_>=candidates.iter().filter_map(|c|match &c.semantic {
        ActionSemanticV1::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}=>Some(crate::state::Target::Object(ObjectId(object.arena_id))),
        _=>None,
    }).collect();
    if actual!=expected {return Err(Error::LibraryChoiceOriginFailed);}
    Ok(())
}

impl FastActorSessionV1 {
    pub(crate) fn kernel_search_redeterminized_clone_v4(&self,seed:u64)->Result<Self,Error> {
        self.search_clone_v4_inner(seed,|_,_|{})
    }
    pub(crate) fn kernel_search_redeterminized_clone_mode_v4(&self,seed:u64,mode:V4SearchSampleMode)->Result<Self,Error> {
        self.search_clone_v4_mode_inner(seed,mode,|_,_|{})
    }
    pub(crate) fn kernel_search_redeterminized_clone_library_v2(&self,seed:u64)->Result<Self,Error> {
        self.kernel_search_redeterminized_clone_mode_v4(seed,V4SearchSampleMode::LibraryChoiceV2)
    }
    fn search_clone_v4_inner(&self,seed:u64,after_sample:impl FnOnce(&mut GameState,PlayerId))->Result<Self,Error> {
        self.search_clone_v4_mode_inner(seed,V4SearchSampleMode::Legacy,after_sample)
    }
    fn search_clone_v4_mode_inner(&self,seed:u64,mode:V4SearchSampleMode,after_sample:impl FnOnce(&mut GameState,PlayerId))->Result<Self,Error> {
        if self.flat_action_contract_mode!=FlatActionContractModeV1::V3 {return Err(Error::UnsupportedActionContract);}
        let FastActorResponseV1::Decision(d)=self.current_response() else {return Err(Error::NoLiveDecision);};
        let before=boundary(self,d,mode)?;
        let actor=self.current.as_ref().ok_or(Error::NoLiveDecision)?.actor;
        let plan=if mode==V4SearchSampleMode::FutureChanceV3 {
            crate::effect::library_choice_search_v2::plan_future_v3(&self.state,actor).map_err(|_|Error::LibraryChoicePlanFailed)?
        } else if mode!=V4SearchSampleMode::Legacy {
            crate::effect::library_choice_search_v2::plan(&self.state,actor).map_err(|_|Error::LibraryChoicePlanFailed)?
        } else {None};
        let original_candidates=&self.current.as_ref().ok_or(Error::NoLiveDecision)?.candidates;
        let mut copy=self.clone();
        if let Err(error) = sampler::redeterminize_with_library_plan(&mut copy.state,actor,seed,plan.as_ref()) {
            // Opt-in local correctness diagnostics only. Never overwrite a prior
            // root, expose this private state in a policy input, or resume play.
            if let Some(path) = std::env::var_os("MTG_V4_SEARCH_FAILURE_STATE") {
                use std::io::Write;
                if let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
                    let record = serde_json::json!({"schema":"v4-search-failure-state/v1",
                        "actor":actor,"seed":seed,"error":error,"state":self.state});
                    if let Ok(bytes) = serde_json::to_vec(&record) {
                        let _ = file.write_all(&bytes).and_then(|_|file.sync_all());
                    }
                }
            }
            return Err(error);
        }
        if mode==V4SearchSampleMode::FutureChanceV3 {
            // `seed` already derives from visible root, experiment and
            // simulation ordinal. Separate future chance from hidden-pool RNG.
            let mut hash=Sha256::new();
            hash.update(b"mtg-kernel/v4-search-future-chance/v3\0");
            hash.update(seed.to_le_bytes());
            let bytes=hash.finalize();
            let future=u64::from_le_bytes(bytes[..8].try_into().expect("eight digest bytes"));
            copy.state.resample_future_randomness_for_search_v3(future);
        }
        after_sample(&mut copy.state,actor);
        let mut current=copy.current.take().ok_or(Error::NoLiveDecision)?;
        if let Some(plan)=&plan {
            plan.rebuild(&mut copy.state).map_err(|_|Error::LibraryChoiceRebuildFailed)?;
            let pending=copy.state.engine.pending_effect.as_ref().ok_or(Error::LibraryChoiceOriginFailed)?;
            let origin=crate::engine::pending_effect_targets_decision_v2(pending).ok_or(Error::LibraryChoiceOriginFailed)?;
            current.origin_decision=PolicyDecisionV5::Surface(crate::surface_v2::SurfaceDecision::Decision(origin));
        }
        current.candidates=core_policy_action_candidates_v5(&current.origin_decision,&copy.state).map_err(|_|Error::SampleCandidateRebuildFailed)?;
        if plan.is_some() {
            validate_library_origin_candidates_v4(&copy.state,&current.candidates)?;
        }
        current.flat_action_cache=None;current.flat_action_cache_error=None;
        current.flat_action_cache_v2=None;current.flat_action_cache_error_v2=None;
        // V3 normalization owns the live candidate ordering. Its cache may
        // reject hidden triggers; V4 intentionally derives its own fresh rows.
        let cache=super::super::flat_action_v3::prepare_and_build_v3(&copy,&mut current);
        flat_install_action_cache_build_result_v2(&mut current,cache);
        if plan.is_some() && current.candidates!=*original_candidates {return Err(Error::SampleCandidateRebuildFailed);}
        copy.flat_action_cache_spare=None;copy.flat_action_cache_spare_v2=None;
        copy.current=Some(current);
        let after=boundary(&copy,d,mode).map_err(|_|Error::SampleBoundaryEncodingFailed)?;
        if before!=after {return Err(Error::SampleBoundaryChanged);}
        Ok(copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put,ready_state};
    use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
    use crate::model_guided_search_core_v1::{ModelGuidedSearchLeafEvaluatorV1,ModelGuidedSearchLeafSiteV1};
    pub(super) fn state(actor:PlayerId)->GameState {
        let mut s=ready_state();s.active_player=actor;s.priority_player=actor;
        put(&mut s,actor,"Lightning Bolt",Zone::Hand);
        s.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()]=3;
        put(&mut s,actor.opponent(),"Gut Shot",Zone::Hand);
        put(&mut s,actor.opponent(),"Lotus Petal",Zone::Hand);
        for owner in [actor,actor.opponent()] {for name in ["Forest","Mountain","Island","Swamp","Counterspell"] {put(&mut s,owner,name,Zone::Library);}}
        s
    }
    pub(super) fn tensor_and_output(s:&FastActorSessionV1)->(crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1,Vec<u32>,u32) {
        let mut p=FrozenPlayPolicyV1::training_fixture_v4();p.score_fast_session_v1(s).unwrap();
        let t=p.last_scored_training_tensor_v4().unwrap();
        let captured=crate::phase1_bo3_learning_v1::Bo3CapturedTensorBitsV1::from_tensor(&crate::native_flat_tensorizer_v3::NativeFlatDecisionTensorV3{common:t.common.clone()});
        let FastActorResponseV1::Decision(d)=s.current_response() else {panic!("fixture")};
        let e=crate::sideboard_play_policy_v1::search_leaf_v4::V4SearchLeafEvaluatorV1::new(&p).unwrap();
        let out=e.evaluate_leaf_v1(s,[0;32],d.legal_action_count,ModelGuidedSearchLeafSiteV1::RootPrior).unwrap();
        (captured,out.legal_action_weights.iter().map(|x|x.to_bits()).collect(),out.v_raw.to_bits())
    }
    fn multiset(s:&FastActorSessionV1,owner:PlayerId)->Vec<u16> {
        let p=&s.state.players[owner.index()];
        let mut values:Vec<_>=p.hand.iter().chain(&p.library).map(|id|s.state.objects.get(*id).card_def).collect();
        values.sort_unstable();values
    }
    #[test]
    fn v4_search_clone_preserves_information_both_seats_and_known_cards() {
        for actor in [PlayerId::P0,PlayerId::P1] {
            let mut s=state(actor);let opponent=actor.opponent();
            let hand=s.players[opponent.index()].hand[0];
            s.reveal_hand_card(actor,opponent,hand).unwrap();s.reveal_library_top(actor,opponent,1);
            let top=s.players[opponent.index()].library[0];
            let hand_def=s.objects.get(hand).card_def;let top_def=s.objects.get(top).card_def;
            let s=FastActorSessionV1::from_v3_fixture_state(s);let hash=s.diagnostic_state_hash();let bits=tensor_and_output(&s);
            let a=s.kernel_search_redeterminized_clone_v4(8172).unwrap();
            let repeat=s.kernel_search_redeterminized_clone_v4(8172).unwrap();
            let b=s.kernel_search_redeterminized_clone_v4(9381).unwrap();
            assert_ne!(a.diagnostic_state_hash(),hash);assert_ne!(a.diagnostic_state_hash(),b.diagnostic_state_hash());
            assert_eq!(a.diagnostic_state_hash(),repeat.diagnostic_state_hash());
            assert_eq!(serde_json::to_vec(&a.state).unwrap(),serde_json::to_vec(&repeat.state).unwrap());
            for c in [&a,&b] {
                assert_eq!(tensor_and_output(c),bits);
                assert_eq!(c.state.objects.get(hand).card_def,hand_def);assert_eq!(c.state.objects.get(top).card_def,top_def);
                for owner in [PlayerId::P0,PlayerId::P1] {assert_eq!(multiset(&s,owner),multiset(c,owner));}
            }
            assert_eq!(s.diagnostic_state_hash(),hash);
            assert!(matches!(s.kernel_search_redeterminized_clone_v1(8172),Err(crate::kernel_native_search_opponent_v1::KernelNativeSearchErrorV1::UnsupportedFlatActionContract)));
        }
    }
    #[test]
    fn v4_search_clone_rejects_library_search_and_visible_corruption() {
        let library=FastActorSessionV1::from_v3_fixture_state(crate::policy_observation_v6::tests::forest_search_state(false,"Lightning Bolt"));
        assert!(matches!(library.kernel_search_redeterminized_clone_v4(1),Err(Error::DecisionLocalLibrary)));
        let s=FastActorSessionV1::from_v3_fixture_state(state(PlayerId::P0));
        assert!(matches!(s.search_clone_v4_inner(8172,|state,actor|state.players[actor.index()].life-=1),Err(Error::SampleBoundaryChanged)));
    }
    #[test]
    fn v4_search_clone_rejects_legacy_contract_and_terminal() {
        let old=FastActorSessionV1::reset_with_limits(23,91,1000,1000);
        assert!(matches!(old.kernel_search_redeterminized_clone_v4(1),Err(Error::UnsupportedActionContract)));
        let mut state=ready_state();state.players[0].has_lost=true;
        let terminal=FastActorSessionV1::from_v3_fixture_state(state);
        assert!(matches!(terminal.kernel_search_redeterminized_clone_v4(1),Err(Error::NoLiveDecision)));
    }
    #[test]
    fn v4_search_clone_preserves_hidden_trigger_frozen_sources() {
        let (mut state,hunter,_,_)=crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        let (hidden,sources)=crate::rl_session::hidden_order_triggers_state_v1(2);
        // Use two different existing V4 hidden-source fixture forms.
        crate::rl_session::shuffle_trigger_source_into_library_v1(&mut state,hunter,PlayerId::P0);
        assert_eq!(sources.len(),2);
        for mut state in [state,hidden] {
            for owner in [PlayerId::P0,PlayerId::P1] {for name in ["Forest","Island","Mountain"] {put(&mut state,owner,name,Zone::Library);}}
            let s=FastActorSessionV1::from_v3_fixture_state(state);let bits=tensor_and_output(&s);
            let c=s.kernel_search_redeterminized_clone_v4(8172).unwrap();
            assert_ne!(s.diagnostic_state_hash(),c.diagnostic_state_hash());assert_eq!(bits,tensor_and_output(&c));
            for owner in [PlayerId::P0,PlayerId::P1] {assert_eq!(multiset(&s,owner),multiset(&c,owner));}
        }
    }
    #[test]
    fn v4_search_clone_hidden_source_step_probe() {
        let (mut state,hunter,_,_)=crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(&mut state,hunter,PlayerId::P0);
        for name in ["Forest","Island","Mountain"] {put(&mut state,PlayerId::P0,name,Zone::Library);}
        let original=FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(d)=original.current_response() else {panic!("fixture")};
        let definition=original.state.objects.get(hunter).card_def;
        let mut moved=false;
        let position=original.state.players[0].library.iter().position(|id|*id==hunter).unwrap();
        let mut direct=original.clone();
        let normal=direct.step(d.episode_id,d.step,0).unwrap();
        for seed in 1..=32 {
            let mut sample=original.kernel_search_redeterminized_clone_v4(seed).unwrap();
            assert_eq!(sample.state.objects.get(hunter).card_def,definition);
            moved|=sample.state.players[0].library.iter().position(|id|*id==hunter).unwrap()!=position;
            assert_eq!(tensor_and_output(&original),tensor_and_output(&sample));
            assert_eq!(sample.step(d.episode_id,d.step,0).unwrap(),normal);
        }
        assert!(moved,"historical identity must not pin unknown library position");
    }
}
