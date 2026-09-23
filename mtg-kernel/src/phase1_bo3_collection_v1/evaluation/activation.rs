//! A consumed-position acceptance fixture, never a whole-match estimate.
use super::*;
const SCHEMA:&str="mtg-kernel-bo3-search-activation/v1";

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3ActivationOptionsV1 {
    pub root:Bo3ReportRootV1,
    pub actor:PlayerSeatV1,
    pub search:AgentSearchPolicyV1,
}
impl Bo3ActivationOptionsV1 {
    pub fn from_json_v1(text:&str)->Result<Self,String> {
        ensure(text.len()<=32768,"activation options exceed 32 KiB")?;
        let value=crate::rl::parse_strict_json_value(text).map_err(|e|e.to_string())?;
        serde_json::from_value(value).map_err(|e|e.to_string())
    }
}
#[derive(Debug,Serialize)]
pub struct Bo3ActivationResultV1 {
    schema:String,config:Bo3CollectionConfigV1,options:Bo3ActivationOptionsV1,
    ordinary_packages:[CompleteAgentPackageV1;2],search_package:CompleteAgentPackageV1,
    archive_sha256:String,current_runtimes:[CurrentAgentRuntimeV1;2],
    activation:ActivationSummary,
    /// Earlier ordinary games plus the target game only. No BO3 winner is inferred.
    games:Vec<EvaluationGame>,abort:Option<Abort>,
    committed_decision_records:u64,committed_decision_json_bytes:u64,
    semantic_sha256:String,timings:Vec<SearchTiming>,
}
#[derive(Debug,Default,Serialize)]
struct ActivationSummary {
    prefix_equal:bool,root_equal:bool,activated:bool,
    target_game_natural:bool,scope_complete:bool,
}
pub(super) struct ActivationSink {
    options:Bo3ActivationOptionsV1,archive:Bo3ReportArchiveV1,
    pub(super) search:AgentSearchPolicyV1,
    pub(super) search_hash:String,
    summary:ActivationSummary,
}
fn stop(message:impl Into<String>)->EvaluationStop {
    EvaluationStop{reason:IncompleteMatchReasonV1::EngineError,actor:None,step:None,
        cause:AbortCause::Activation{message:message.into()}}
}
impl ActivationSink {
    fn new(config:&Bo3CollectionConfigV1,packages:[&CompleteAgentPackageV1;2],options:&Bo3ActivationOptionsV1,
        archive:Bo3ReportArchiveV1)->Result<(Self,CompleteAgentPackageV1),String> {
        ensure(config==&archive.config,"activation configuration differs from archive")?;
        for i in 0..2 {
            ensure(matches!(packages[i].search,AgentSearchPolicyV1::Disabled),"activation requires ordinary base packages")?;
            let mut expected=archive.packages[i].clone();expected.runtime=packages[i].runtime.clone();
            ensure(expected==*packages[i],"activation package differs beyond runtime")?;
        }
        archive.trajectory.validate_v1(archive.packages.each_ref())?;
        ensure(matches!(archive.trajectory.ending,Bo3TrajectoryEndingV1::Complete{..}),"activation archive must be naturally complete")?;
        ensure(archive.trajectory.match_id==config.match_id && archive.trajectory.initial_chooser==config.initial_chooser
            && archive.trajectory.registrations_by_seat==config.registrations,"activation archive identity differs")?;
        let expected=archive.trajectory.games.iter().find(|g|g.game_index==options.root.game_index)
            .and_then(|g|g.decisions.iter().find(|r|r.decision_index==options.root.decision_index))
            .ok_or_else(||"activation root absent from archive".to_owned())?;
        ensure(expected.actor==options.actor,"activation actor differs from archived root")?;
        ensure(matches!(&expected.visible,ActorVisibleDecisionV1::Gameplay{ordered_actions,..} if ordered_actions.len()>1)
            && matches!(expected.behavior,BehaviorDistributionV1::HamiltonQ64{..}),"activation root must be multi-action Hamilton gameplay")?;
        let d=match &options.search {
            AgentSearchPolicyV1::V4InformationSetV1{descriptor}=>descriptor,
            AgentSearchPolicyV1::V4InformationSetEstimateV1{descriptor}=>&descriptor.0,
            AgentSearchPolicyV1::V4InformationSetEstimateV2{descriptor}=>&descriptor.0,
            _=>return Err("activation requires a V4 mean or E playing route".into()),
        };
        ensure(d.root_allocation==V4SearchRootAllocationV1::RoundRobin && d.interior_bonus==V4SearchInteriorBonusV1::PriorFree,
            "activation comparison requires RoundRobin/PriorFree")?;
        let mut search_package=packages[seat(options.actor)].clone();search_package.search=options.search.clone();
        search_package.validate_metadata_v1()?;
        let search_hash=search_package.package_sha256_v1()?;
        Ok((Self{options:options.clone(),archive,search:options.search.clone(),search_hash,
            summary:ActivationSummary::default()},search_package))
    }
    pub(super) fn target_game(&self)->u8 {self.options.root.game_index}
    pub(super) fn uses_search(&self,game:u8,actor:PlayerSeatV1)->bool {
        self.summary.activated && game==self.target_game() && actor==self.options.actor
    }
    fn record_equal(&self,actual:&EvaluationDecision,expected:&Bo3DecisionRecordV1)->bool {
        let EvaluationDecision::Ordinary{record}=actual else {return false;};
        let mut comparable=record.clone();
        comparable.behavior_package_sha256=self.archive.trajectory.behavior_packages_by_seat[seat(record.actor)].clone();
        comparable==*expected
    }
    pub(super) fn before_decision(&mut self,game:&EvaluationGame,index:u64,input:&PairedBo1PolicyInputV1<'_>)->Result<(),EvaluationStop> {
        if self.summary.activated {return Ok(());}
        if index<self.options.root.decision_index {return Ok(());}
        let root=&self.options.root;
        if game.game_index!=root.game_index || index!=root.decision_index {return Err(stop("activation missed pinned root"));}
        let archived=self.archive.trajectory.games.iter().find(|g|g.game_index==root.game_index).unwrap();
        let prefix:Vec<_>=archived.decisions.iter().filter(|r|r.decision_index<index).collect();
        if game.start!=archived.start || game.discarded_pending_selections!=0 || game.decisions.len()!=prefix.len()
            || !game.decisions.iter().zip(prefix).all(|(a,b)|self.record_equal(a,b)) {
            return Err(stop("activation ordinary prefix differs from archive"));
        }
        // Capture only visible identity. No ordinary action is sampled at the switch.
        let actual=input.capture_bo3_gameplay_v4(index,self.search_hash.clone(),BehaviorDistributionV1::Deterministic{selected_index:0})
            .map_err(|e|stop(e.to_string()))?;
        let expected=archived.decisions.iter().find(|r|r.decision_index==index).unwrap();
        if actual.actor!=self.options.actor || actual.visible!=expected.visible {
            return Err(stop("activation visible root or actor differs from archive"));
        }
        self.summary.prefix_equal=true;self.summary.root_equal=true;self.summary.activated=true;
        Ok(())
    }
    pub(super) fn after_game(&mut self,game:&EvaluationGame,played_ok:bool)->Result<(),EvaluationStop> {
        if !played_ok {return Ok(());}
        if game.game_index<self.target_game() {
            let expected=self.archive.trajectory.games.iter().find(|g|g.game_index==game.game_index)
                .ok_or_else(||stop("activation prior game absent from archive"))?;
            if game.start!=expected.start || game.terminal!=expected.terminal || game.discarded_pending_selections!=0
                || game.decisions.len()!=expected.decisions.len()
                || !game.decisions.iter().zip(&expected.decisions).all(|(a,b)|self.record_equal(a,b)) {
                return Err(stop("activation prior ordinary game differs from archive"));
            }
        } else if game.game_index==self.target_game() {
            if !self.summary.activated {return Err(stop("activation target game ended before pinned root"));}
            self.summary.target_game_natural=game.terminal.as_ref().is_some_and(|t|t.classification==TerminalClassificationV1::Natural);
            self.summary.scope_complete=self.summary.target_game_natural;
        } else {return Err(stop("activation escaped target game scope"));}
        Ok(())
    }
}
/// Current runtime is verified for the unchanged ordinary model; the search
/// package differs only in a strictly validated descriptor for that same model.
pub fn activate_bo3_v4(config:Bo3CollectionConfigV1,packages:[CompleteAgentPackageV1;2],options:Bo3ActivationOptionsV1,
    archive:Bo3ReportArchiveV1)->Result<Bo3ActivationResultV1,String> {
    let evaluation_options=Bo3EvaluationOptionsV1{max_retained_search_outcomes:0};
    validate_evaluation(&config,packages.each_ref(),&evaluation_options)?;
    let archive_sha256=semantic_hash(&archive)?;
    let (mut sink,search_package)=ActivationSink::new(&config,packages.each_ref(),&options,archive)?;
    let [p0,p1]=packages.each_ref().map(CompleteAgentPackageV1::load_evaluation_components_v1);
    let p0=p0?;let p1=p1?;let mut policies=[p0.gameplay,p1.gameplay];let heads=[p0.sideboard,p1.sideboard];
    let (evaluated,timings)=evaluate_loaded_scoped(&config,packages.each_ref(),&mut policies,heads.each_ref().map(Option::as_ref),
        &evaluation_options,None,Some(&mut sink))?;
    sink.summary.scope_complete &= evaluated.abort.is_none();
    let semantic_sha256=semantic_hash(&(&config,&options,&packages,&search_package,&archive_sha256,
        &sink.summary,&evaluated.games,&evaluated.abort,evaluated.committed_decision_records,evaluated.committed_decision_json_bytes))?;
    Ok(Bo3ActivationResultV1{schema:SCHEMA.into(),config,options,ordinary_packages:packages,search_package,archive_sha256,
        current_runtimes:[p0.current_runtime,p1.current_runtime],activation:sink.summary,games:evaluated.games,abort:evaluated.abort,
        committed_decision_records:evaluated.committed_decision_records,committed_decision_json_bytes:evaluated.committed_decision_json_bytes,
        semantic_sha256,timings})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase1_bo3_collection_v1::tests as fixtures;
    fn archive()->Bo3ReportArchiveV1 {
        let config=fixtures::config("activation-controls");
        let (mut policies,packages)=fixtures::fixtures_v4([PlayDrawChoiceV1::Play;2]);
        let native=collect_loaded(&config,packages.each_ref(),&mut policies,[None,None]).unwrap();
        Bo3ReportArchiveV1{config,packages,trajectory:native.trajectory}
    }
    fn options(a:&Bo3ReportArchiveV1,actor:PlayerSeatV1,estimate:bool)->Bo3ActivationOptionsV1 {
        let g=&a.trajectory.games[0];
        let row=g.decisions.iter().find(|r|r.actor==actor && matches!(&r.visible,ActorVisibleDecisionV1::Gameplay{ordered_actions,..} if ordered_actions.len()>1)).unwrap();
        let mut package=a.packages[seat(actor)].clone();super::super::tests::search_package(&mut package,1);
        if estimate {
            let AgentSearchPolicyV1::V4InformationSetV1{mut descriptor}=package.search else {unreachable!()};
            descriptor.schema=V4_INFORMATION_SET_ESTIMATE_SCHEMA_V1.into();descriptor.algorithm=V4_INFORMATION_SET_ESTIMATE_ALGORITHM_V1.into();
            package.search=AgentSearchPolicyV1::V4InformationSetEstimateV1{descriptor:V4InformationSetEstimateDescriptorV1(descriptor)};
        }
        Bo3ActivationOptionsV1{root:Bo3ReportRootV1{game_index:g.game_index,decision_index:row.decision_index},actor,search:package.search}
    }
    fn run(a:&Bo3ReportArchiveV1,sink:&mut ActivationSink)->(EvaluatedMatch,Vec<SearchTiming>) {
        let mut policies=fixtures::fixtures_v4([PlayDrawChoiceV1::Play;2]).0;
        evaluate_loaded_scoped(&a.config,a.packages.each_ref(),&mut policies,[None,None],
            &Bo3EvaluationOptionsV1{max_retained_search_outcomes:0},None,Some(sink)).unwrap()
    }
    #[test]
    fn v4_evaluation_activation_both_seats_scoped_choices_and_repeat() {
        let a=archive();
        for actor in [PlayerSeatV1::P0,PlayerSeatV1::P1] {for estimate in [false,true] {
            let o=options(&a,actor,estimate);
            let (mut sink,search_package)=ActivationSink::new(&a.config,a.packages.each_ref(),&o,a.clone()).unwrap();
            let (result,timing)=run(&a,&mut sink);
            assert!(sink.summary.prefix_equal && sink.summary.root_equal && sink.summary.activated);
            assert_eq!(result.games.len(),1);assert!(!timing.is_empty());
            let rows=&result.games[0].decisions;
            assert!(rows.iter().any(|r|!matches!(r,EvaluationDecision::Ordinary{..})),"a played search receipt is required");
            for row in rows {
                let record=row.record();
                let switched=record.decision_index>=o.root.decision_index && record.actor==actor
                    && matches!(record.visible,ActorVisibleDecisionV1::Gameplay{..});
                if switched {
                    assert_eq!(record.behavior_package_sha256,search_package.package_sha256_v1().unwrap());
                    match row {
                        EvaluationDecision::EstimateSearch{record,search}=>{
                            assert!(estimate);assert_eq!(record.behavior,BehaviorDistributionV1::Deterministic{selected_index:search.selected_by_estimate});
                        },
                        EvaluationDecision::Search{..}=>assert!(!estimate),
                        _=>panic!("switched seat played ordinary"),
                    }
                } else {assert!(matches!(row,EvaluationDecision::Ordinary{..}));}
            }
            let (mut repeat,_)=ActivationSink::new(&a.config,a.packages.each_ref(),&o,a.clone()).unwrap();
            let (again,_)=run(&a,&mut repeat);
            assert_eq!(semantic_hash(&result).unwrap(),semantic_hash(&again).unwrap());
            assert_eq!(semantic_hash(&sink.summary).unwrap(),semantic_hash(&repeat.summary).unwrap());
            if result.abort.is_none() {assert!(sink.summary.scope_complete && sink.summary.target_game_natural);}
        }}
    }
    #[test]
    fn v4_evaluation_activation_prefix_and_root_mismatch_never_search() {
        let a=archive();let o=options(&a,PlayerSeatV1::P0,true);
        for root_mismatch in [false,true] {
            let (mut sink,_)=ActivationSink::new(&a.config,a.packages.each_ref(),&o,a.clone()).unwrap();
            let g=&mut sink.archive.trajectory.games[0];
            if root_mismatch {
                let row=g.decisions.iter_mut().find(|r|r.decision_index==o.root.decision_index).unwrap();
                // Tamper the expected visible identity after admission to exercise the live boundary.
                if let ActorVisibleDecisionV1::Gameplay{ordered_actions,..}=&mut row.visible {ordered_actions.swap(0,1);}
            } else {g.decisions[0].actor=PlayerSeatV1::P1;}
            let (result,timing)=run(&a,&mut sink);
            assert!(matches!(result.abort.unwrap().cause,AbortCause::Activation{..}));
            assert!(timing.is_empty());assert!(!sink.summary.activated && !sink.summary.scope_complete);
            assert!(result.games.iter().flat_map(|g|&g.decisions).all(|r|matches!(r,EvaluationDecision::Ordinary{..})));
        }
    }
    #[test]
    fn v4_evaluation_activation_rejects_wrong_actor_and_ordinary_route() {
        let a=archive();let mut o=options(&a,PlayerSeatV1::P0,true);o.actor=PlayerSeatV1::P1;
        assert!(ActivationSink::new(&a.config,a.packages.each_ref(),&o,a.clone()).is_err());
        o.actor=PlayerSeatV1::P0;o.search=AgentSearchPolicyV1::Disabled;
        assert!(ActivationSink::new(&a.config,a.packages.each_ref(),&o,a.clone()).is_err());
        assert!(Bo3ActivationOptionsV1::from_json_v1("{\"actor\":\"p0\",\"actor\":\"p1\"}").is_err());
    }
}
