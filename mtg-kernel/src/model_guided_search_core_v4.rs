//! Additive, bounded V4 information-set search. No package or playing caller.
use crate::ids::PlayerId;
use crate::kernel_native_search_opponent_v1::{SearchActionStatV1,integer_ucb_bonus_v1,natural_terminal_value_v1,player_id_v1,select_final_root_action_v1};
use crate::model_guided_search_core_v1::{ModelGuidedSearchLeafEvaluatorV1 as Evaluator,ModelGuidedSearchLeafForwardV1 as Forward,ModelGuidedSearchLeafSiteV1 as Site};
use crate::model_guided_search_prior_quantization_v1::{quantize_prior_v1,prior_expansion_order_v1,puct_bonus_v1};
use crate::model_guided_search_value_quantization_v1::{quantize_value_v1,ModelGuidedSearchValueHeadDomainV1};
use crate::rl_session::{FastActorSessionV1,FastActorResponseV1,FastActorDecisionV1,V4SearchStateErrorV1};
use crate::rl::TerminalClassificationV1;
use serde::Serialize;
use sha2::{Digest,Sha256};
mod estimate;
pub(crate) use estimate::{BackupMode,Estimator};
use estimate::Sample;

#[derive(Debug,Clone,Copy)]
pub(crate) struct Limits {pub simulations:u32,pub transitions:u32,pub depth:u16,pub seed:u64}
#[derive(Debug,Clone,PartialEq,Eq)]
pub(crate) enum Error {NoDecision,InvalidBudget,InsufficientHeadroom,State{stage:StateStage,source:V4SearchStateErrorV1},Evaluator(String),NonNaturalTerminal,CorruptTree}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub(crate) enum StateStage {Key,Redeterminize,Token,Consume}
type Result<T> = std::result::Result<T,Error>;
#[derive(Debug,Default,Clone,PartialEq,Serialize)]
pub(crate) struct Census {
    pub natural:u32,pub natural_wins:u32,pub natural_losses:u32,pub natural_draws:u32,pub expanded:u32,pub depth:u32,pub budget:u32,pub coverage:u32,
    pub clip_low:u64,pub clip_high:u64,pub raw_min:Option<f32>,pub raw_max:Option<f32>,pub forwards:u64,
}
#[derive(Debug,Default,Clone,PartialEq,Serialize)]
pub(crate) struct RootWork {
    pub natural_wins:u32,pub natural_losses:u32,pub natural_draws:u32,
    pub expanded:u32,pub depth:u32,pub budget:u32,pub coverage:u32,
    pub transitions:u32,pub forwards:u64,
}
#[derive(Debug,Clone,PartialEq,Serialize)]
pub(crate) struct NodeReport {
    pub key:[u8;32],pub actor:u8,pub visits:u32,pub priors:Vec<u32>,
    pub edge_visits:Vec<u32>,pub edge_value_sums:Vec<i64>,pub children:Vec<Vec<usize>>,
}
#[derive(Debug,Clone,Copy,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum RootAllocation {Puct,RoundRobin}
#[derive(Debug,Clone,Copy,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum InteriorBonus {PriorWeighted,PriorFree}
#[derive(Debug,Clone,PartialEq,Serialize)]
pub(crate) struct TraceChoice {pub node:usize,pub action:usize,pub mode:&'static str,pub scores:Vec<Option<i64>>}
#[derive(Debug,Clone,PartialEq,Serialize)]
pub(crate) struct TraceSimulation {pub ordinal:u32,pub choices:Vec<TraceChoice>,pub value:i32}
#[derive(Debug,Clone,PartialEq,Serialize)]
pub(crate) struct Outcome {
    #[serde(skip_serializing_if="Option::is_none")]
    pub estimator:Option<Estimator>,
    pub allocation:RootAllocation,pub interior_bonus:InteriorBonus,pub trace:Vec<TraceSimulation>,pub selected:u32,pub selected_by_mean:u32,pub tree:Vec<NodeReport>,pub simulations:u32,pub transitions:u32,pub nodes:usize,
    pub headroom:[u64;2],pub root_visits:Vec<u32>,pub root_value_sums:Vec<i64>,pub census:Census,pub root_priors:Vec<u32>,pub root_work:Vec<RootWork>,
}
struct Node {key:[u8;32],actor:PlayerId,visits:u32,prior:Vec<u32>,actions:Vec<SearchActionStatV1>,witness:Option<[u8;32]>}
type Witness<'a> = Option<&'a mut dyn FnMut(&FastActorSessionV1,u32)->std::result::Result<[u8;32],String>>;

fn live(s:&FastActorSessionV1)->Result<FastActorDecisionV1> {
    match s.current_response(){FastActorResponseV1::Decision(d)=>Ok(d),_=>Err(Error::NoDecision)}
}
fn key(s:&FastActorSessionV1,depth:u16)->Result<[u8;32]> {
    s.kernel_search_visible_key_v4(u32::from(depth)).map_err(|source|Error::State{stage:StateStage::Key,source})
}
fn seed(root:[u8;32],experiment:u64,ordinal:u32)->u64 {
    let mut h=Sha256::new();h.update(b"mtg-kernel/v4-info-set-simulation/v1\0");h.update(root);
    h.update(experiment.to_le_bytes());h.update(ordinal.to_le_bytes());
    u64::from_le_bytes(h.finalize()[..8].try_into().unwrap())
}
fn forward<E:Evaluator>(e:&E,s:&FastActorSessionV1,k:[u8;32],d:FastActorDecisionV1,site:Site,c:&mut Census)->Result<Forward> {
    let f=e.evaluate_leaf_v1(s,k,d.legal_action_count,site).map_err(|e|Error::Evaluator(format!("{e:?}")))?;
    if f.legal_action_weights.len()!=d.legal_action_count as usize || !f.v_raw.is_finite() || f.legal_action_weights.iter().any(|x|!x.is_finite() || !(0.0..=1.0).contains(x)) {return Err(Error::Evaluator("invalid shape or value".into()));}
    c.forwards+=1;c.clip_low+=u64::from(f.v_raw < -1.0);c.clip_high+=u64::from(f.v_raw > 1.0);
    c.raw_min=Some(c.raw_min.map_or(f.v_raw,|v|v.min(f.v_raw)));c.raw_max=Some(c.raw_max.map_or(f.v_raw,|v|v.max(f.v_raw)));
    Ok(f)
}
fn value(f:&Forward,actor:PlayerId,root:PlayerId)->Result<i32> {
    quantize_value_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh,f.v_raw.clamp(-1.0,1.0),actor==root)
        .map_err(|e|Error::Evaluator(format!("{e:?}")))
}
fn node(k:[u8;32],actor:PlayerId,f:&Forward,witness:Option<[u8;32]>)->Result<Node> {
    let prior=quantize_prior_v1(&f.legal_action_weights).map_err(|e|Error::Evaluator(format!("{e:?}")))?;
    if prior.is_empty(){return Err(Error::CorruptTree);}
    let actions=prior.iter().map(|_|SearchActionStatV1{visits:0,value_sum:0,child_nodes:Vec::new()}).collect();
    Ok(Node{key:k,actor,visits:0,prior,actions,witness})
}
fn score(n:&Node,root:PlayerId,i:usize,policy:InteriorBonus)->Result<i64> {score_estimated(n,root,i,policy,None)}
fn score_estimated(n:&Node,root:PlayerId,i:usize,policy:InteriorBonus,estimate:Option<i64>)->Result<i64> {
    let a=&n.actions[i];
    let raw=integer_ucb_bonus_v1(n.visits,a.visits);
    let bonus=i64::try_from(match policy {InteriorBonus::PriorWeighted=>puct_bonus_v1(raw,n.prior[i]).map_err(|_|Error::CorruptTree)?,InteriorBonus::PriorFree=>raw}).map_err(|_|Error::CorruptTree)?;
    let base=estimate.map_or(a.mean(),|v|(a.mean()+v)/2);
    Ok(if n.actor==root{base+bonus}else{base-bonus})
}
fn choose(n:&Node,root:PlayerId,policy:InteriorBonus)->Result<usize> {choose_estimated(n,root,policy,None)}
fn choose_estimated(n:&Node,root:PlayerId,policy:InteriorBonus,estimates:Option<&estimate::NodeEstimate>)->Result<usize> {
    for i in prior_expansion_order_v1(&n.prior) {if n.actions[i].visits==0{return Ok(i);}}
    let maximizing=n.actor==root;let mut best=None;
    for (i,_) in n.actions.iter().enumerate() {
        let score=score_estimated(n,root,i,policy,estimates.and_then(|n|n.edges[i].value))?;
        if best.is_none_or(|(_,old)|if maximizing{score>old}else{score<old}){best=Some((i,score));}
    }
    best.map(|(i,_)|i).ok_or(Error::CorruptTree)
}
fn witness(w:&mut Witness<'_>,s:&FastActorSessionV1,n:u32)->Result<Option<[u8;32]>> {
    w.as_mut().map(|f|f(s,n).map_err(Error::Evaluator)).transpose()
}

pub(crate) fn search<E:Evaluator>(session:&FastActorSessionV1,limits:Limits,evaluator:&E)->Result<Outcome> {
    search_inner(session,limits,evaluator,None)
}
/// Optional acceptance witness verifies equal keys imply equal fresh tensors.
pub(crate) fn search_inner<E:Evaluator>(session:&FastActorSessionV1,l:Limits,e:&E,w:Witness<'_>)->Result<Outcome> {
    search_with_allocation(session,l,e,RootAllocation::Puct,w)
}
pub(crate) fn search_with_allocation<E:Evaluator>(session:&FastActorSessionV1,l:Limits,e:&E,allocation:RootAllocation,w:Witness<'_>)->Result<Outcome> {
    search_with_policies(session,l,e,allocation,InteriorBonus::PriorWeighted,w)
}
pub(crate) fn search_with_policies<E:Evaluator>(session:&FastActorSessionV1,l:Limits,e:&E,allocation:RootAllocation,interior_bonus:InteriorBonus,w:Witness<'_>)->Result<Outcome> {
    search_with_backup(session,l,e,allocation,interior_bonus,BackupMode::Off,w)
}
pub(crate) fn search_with_backup<E:Evaluator>(session:&FastActorSessionV1,l:Limits,e:&E,allocation:RootAllocation,interior_bonus:InteriorBonus,backup:BackupMode,mut w:Witness<'_>)->Result<Outcome> {
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|x|Error::Evaluator(format!("{x:?}")))?;
    let d=live(session)?;let count=d.legal_action_count;
    if count==0||l.depth==0||l.simulations<count||l.transitions<count{return Err(Error::InvalidBudget);}
    let headroom=session.diagnostic_remaining_headroom_v1();
    if headroom.iter().any(|x|*x<u64::from(l.depth)){return Err(Error::InsufficientHeadroom);}
    let root=player_id_v1(d.acting_player);let root_key=key(session,l.depth)?;
    let mut census=Census::default();let f=forward(e,session,root_key,d,Site::RootPrior,&mut census)?;
    let mut tree=vec![node(root_key,root,&f,witness(&mut w,session,count)?)?];
    let mut estimator=if backup==BackupMode::Off{None}else{let mut x=Estimator::new(backup);x.push(l.depth,i64::from(value(&f,root,root)?),count as usize);Some(x)};
    let coverage_order=prior_expansion_order_v1(&tree[0].prior);
    let mut root_work=vec![RootWork::default();count as usize];
    let mut trace=Vec::new();
    let mut simulations=0;let mut transitions=0;
    while simulations<l.simulations && transitions<l.transitions {
        let mut sample=session.kernel_search_redeterminized_clone_v4(seed(root_key,l.seed,simulations))
            .map_err(|source|Error::State{stage:StateStage::Redeterminize,source})?;
        let census_before=census.clone();let transitions_before=transitions;
        let coverage=simulations<count;let mut remaining=l.depth;let mut at=0usize;
        let mut choices=Vec::new();let mut path=vec![0usize];let mut edges=Vec::new();let result;
        loop {
            let current=live(&sample)?;let k=key(&sample,remaining)?;
            if tree[at].key!=k||tree[at].actor!=player_id_v1(current.acting_player)
                ||tree[at].actions.len()!=current.legal_action_count as usize
                ||tree[at].witness!=witness(&mut w,&sample,current.legal_action_count)? {return Err(Error::CorruptTree);}
            let round_robin=at==0 && allocation==RootAllocation::RoundRobin;
            let bonus_policy=if at==0{InteriorBonus::PriorWeighted}else{interior_bonus};
            let estimates=estimator.as_ref().filter(|x|x.mode==BackupMode::Blend).map(|x|&x.nodes[at]);
            let action=if coverage {coverage_order[simulations as usize]}else if round_robin {coverage_order[(simulations%count) as usize]}else{choose_estimated(&tree[at],root,bonus_policy,estimates)?};
            let mode=if coverage{"coverage"}else if round_robin{"round_robin"}else if tree[at].actions[action].visits==0{"unvisited"}else if bonus_policy==InteriorBonus::PriorFree{"prior_free"}else{"puct"};
            let scores=tree[at].actions.iter().enumerate().map(|(i,a)|if a.visits==0{Ok(None)}else{score_estimated(&tree[at],root,i,bonus_policy,estimates.and_then(|n|n.edges[i].value)).map(Some)}).collect::<Result<Vec<_>>>()?;
            choices.push(TraceChoice{node:at,action,mode,scores});
            let token=sample.kernel_search_action_token_v4(current).map_err(|source|Error::State{stage:StateStage::Token,source})?;
            let response=sample.kernel_search_consume_v4(current,token,action as u32).map_err(|source|Error::State{stage:StateStage::Consume,source})?;
            transitions+=1;remaining-=1;edges.push((at,action));
            let next=match response {
                FastActorResponseV1::Terminal(t)=>{
                    if t.terminal_classification!=TerminalClassificationV1::Natural{return Err(Error::NonNaturalTerminal);}
                    census.natural+=1;result=natural_terminal_value_v1(t.terminal_outcome,root).map_err(|_|Error::NonNaturalTerminal)?;
                    if let Some(x)=&mut estimator{x.record(at,action,Sample::Terminal(i64::from(result)))?;}
                    if result>0 {census.natural_wins+=1;} else if result<0 {census.natural_losses+=1;} else {census.natural_draws+=1;}
                    break;
                },
                FastActorResponseV1::Decision(next)=>next,
            };
            let next_key=key(&sample,remaining)?;let actor=player_id_v1(next.acting_player);
            if remaining==0||transitions==l.transitions||coverage {
                if remaining==0 {census.depth+=1;} else if transitions==l.transitions {census.budget+=1;} else {census.coverage+=1;}
                let f=forward(e,&sample,next_key,next,Site::RevisitedDepthCapLeaf,&mut census)?;
                result=value(&f,actor,root)?;
                if let Some(x)=&mut estimator{x.record(at,action,Sample::Cutoff(i64::from(result)))?;}
                break;
            }
            if let Some(existing)=tree.iter().position(|n|n.key==next_key) {
                if let Some(x)=&mut estimator {
                    if x.nodes[existing].remaining!=remaining{return Err(Error::CorruptTree);}
                    x.record(at,action,Sample::Child(existing))?;
                }
                if !tree[at].actions[action].child_nodes.contains(&existing){tree[at].actions[action].child_nodes.push(existing);}
                at=existing;path.push(at);
            } else {
                let f=forward(e,&sample,next_key,next,Site::NewlyExpandedNode,&mut census)?;
                let created=tree.len();tree.push(node(next_key,actor,&f,witness(&mut w,&sample,next.legal_action_count)?)?);
                if let Some(x)=&mut estimator{x.push(remaining,i64::from(value(&f,actor,root)?),next.legal_action_count as usize);x.record(at,action,Sample::Child(created))?;}
                tree[at].actions[action].child_nodes.push(created);path.push(created);
                census.expanded+=1;result=value(&f,actor,root)?;break;
            }
        }
        let rw=&mut root_work[edges[0].1];
        rw.natural_wins+=census.natural_wins-census_before.natural_wins;
        rw.natural_losses+=census.natural_losses-census_before.natural_losses;
        rw.natural_draws+=census.natural_draws-census_before.natural_draws;
        rw.expanded+=census.expanded-census_before.expanded;rw.depth+=census.depth-census_before.depth;
        rw.budget+=census.budget-census_before.budget;rw.coverage+=census.coverage-census_before.coverage;
        rw.transitions+=transitions-transitions_before;rw.forwards+=census.forwards-census_before.forwards;
        for i in path {tree[i].visits+=1;}
        for (i,a) in edges {tree[i].actions[a].visits+=1;tree[i].actions[a].value_sum+=i64::from(result);}
        if let Some(x)=&mut estimator{x.recompute(&tree,root)?;}
        trace.push(TraceSimulation{ordinal:simulations,choices,value:result});
        simulations+=1;
    }
    if tree[0].actions.iter().any(|a|a.visits==0)
        ||tree[0].visits!=simulations||tree[0].actions.iter().map(|a|a.visits).sum::<u32>()!=simulations
        ||census.natural+census.expanded+census.depth+census.budget+census.coverage!=simulations{return Err(Error::CorruptTree);}
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|x|Error::Evaluator(format!("{x:?}")))?;
    let selected_by_mean=tree[0].actions.iter().enumerate().fold(0,|best,(i,a)|if a.mean()>tree[0].actions[best].mean(){i}else{best}) as u32;
    let report=tree.iter().map(|n|NodeReport{key:n.key,actor:n.actor.index() as u8,visits:n.visits,priors:n.prior.clone(),
        edge_visits:n.actions.iter().map(|a|a.visits).collect(),edge_value_sums:n.actions.iter().map(|a|a.value_sum).collect(),
        children:n.actions.iter().map(|a|a.child_nodes.clone()).collect()}).collect();
    Ok(Outcome{estimator,allocation,interior_bonus,trace,selected:select_final_root_action_v1(&tree[0].actions).map_err(|_|Error::CorruptTree)?,
        selected_by_mean,tree:report,simulations,transitions,nodes:tree.len(),headroom,
        root_visits:tree[0].actions.iter().map(|a|a.visits).collect(),root_value_sums:tree[0].actions.iter().map(|a|a.value_sum).collect(),census,root_priors:tree[0].prior.clone(),root_work})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sideboard_play_policy_v1::{FrozenPlayPolicyV1,search_leaf_v4::V4SearchLeafEvaluatorV1};
    fn session(actor:PlayerId)->FastActorSessionV1 {
        use crate::policy_observation_v6::tests::{put,ready_state};
        use crate::state::Zone;
        let mut s=ready_state();s.active_player=actor;s.priority_player=actor;
        put(&mut s,actor,"Lightning Bolt",Zone::Hand);
        s.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()]=3;
        for owner in [actor,actor.opponent()] {
            for name in ["Forest","Mountain","Island"] {put(&mut s,owner,name,Zone::Library);}
        }
        FastActorSessionV1::from_v3_fixture_state(s)
    }
    #[test]
    fn v4_search_core_repeat_both_seats_tensor_witness_and_accounting() {
        for actor in [PlayerId::P0,PlayerId::P1] {
            let s=session(actor);let before=s.diagnostic_state_hash();
            let policy=FrozenPlayPolicyV1::training_fixture_v4();let e=V4SearchLeafEvaluatorV1::new(&policy).unwrap();
            let limits=Limits{simulations:32,transitions:128,depth:4,seed:29};
            let mut witness=|s:&FastActorSessionV1,n|e.tensor_digest(s,n);
            let a=search_inner(&s,limits,&e,Some(&mut witness)).unwrap();
            let b=search_inner(&s,limits,&e,Some(&mut witness)).unwrap();
            assert_eq!(a,b);assert_eq!(a,search(&s,limits,&e).unwrap());
            assert_eq!(s.diagnostic_state_hash(),before);
            assert_eq!(a.simulations,32);assert!(a.transitions<=128);
            assert!(a.root_visits.iter().all(|x|*x>0));assert!(a.selected<(a.root_visits.len() as u32));
            assert!(a.nodes>1);assert!(a.census.expanded>0);
            assert_eq!(a.root_priors.iter().sum::<u32>(),1_000_000);
            assert_eq!(a.root_work.iter().map(|x|x.transitions).sum::<u32>(),a.transitions);
            assert_eq!(a.root_work.iter().map(|x|x.forwards).sum::<u64>()+1,a.census.forwards);
            for (rw,visits) in a.root_work.iter().zip(&a.root_visits) {
                assert_eq!(rw.natural_wins+rw.natural_losses+rw.natural_draws+rw.expanded+rw.depth+rw.budget+rw.coverage,*visits);
            }

        }
    }
    #[test]
    fn v4_search_core_budget_admission_and_cutoff_census() {
        let s=session(PlayerId::P0);let n=live(&s).unwrap().legal_action_count;
        let p=FrozenPlayPolicyV1::training_fixture_v4();let e=V4SearchLeafEvaluatorV1::new(&p).unwrap();
        for limits in [Limits{simulations:n-1,transitions:n,depth:3,seed:0},Limits{simulations:n,transitions:n-1,depth:3,seed:0},Limits{simulations:n,transitions:n,depth:0,seed:0}] {
            assert_eq!(search(&s,limits,&e),Err(Error::InvalidBudget));
        }
        assert_eq!(search(&s,Limits{simulations:n,transitions:n,depth:u16::MAX,seed:0},&e),Err(Error::InsufficientHeadroom));
        let a=search(&s,Limits{simulations:n,transitions:n,depth:1,seed:0},&e).unwrap();
        assert_eq!(a.census.depth+a.census.natural,n);assert_eq!(a.census.coverage+a.census.budget,0);
        let b=search(&s,Limits{simulations:n,transitions:n,depth:3,seed:0},&e).unwrap();
        assert_eq!(b.census.budget,1);assert_eq!(b.census.coverage+b.census.natural,n-1);
    }
    #[test]
    fn v4_search_core_clipping_perspective_and_seed_domain() {
        crate::deterministic_math_v1::ensure_thread_mxcsr_normalized_v1().unwrap();
        for (raw,expected) in [(-12.0,-9000),(-1.0,-9000),(0.0,0),(1.0,9000),(12.0,9000)] {
            let f=Forward{legal_action_weights:vec![1.0],v_raw:raw};
            assert_eq!(value(&f,PlayerId::P0,PlayerId::P0).unwrap(),expected);
            assert_eq!(value(&f,PlayerId::P1,PlayerId::P0).unwrap(),-expected);
        }
        assert_ne!(seed([0;32],0,0),seed([0;32],0,1));assert_ne!(seed([0;32],0,0),seed([0;32],1,0));
        assert_ne!(seed([0;32],0,0),seed([1;32],0,0));
    }
    #[test]
    fn v4_search_core_tensor_witness_mismatch_aborts() {
        let s=session(PlayerId::P0);let p=FrozenPlayPolicyV1::training_fixture_v4();let e=V4SearchLeafEvaluatorV1::new(&p).unwrap();
        let mut calls=0;let mut w=|_:&FastActorSessionV1,_|{calls+=1;Ok([calls;32])};
        assert_eq!(search_inner(&s,Limits{simulations:32,transitions:128,depth:4,seed:0},&e,Some(&mut w)),Err(Error::CorruptTree));
    }
    #[test]
    fn v4_search_core_truncation_and_sampler_rejection_preserve_original() {
        let mut s=session(PlayerId::P0);s.fixture_policy_headroom_v4(1);
        let p=FrozenPlayPolicyV1::training_fixture_v4();let e=V4SearchLeafEvaluatorV1::new(&p).unwrap();
        let n=live(&s).unwrap().legal_action_count;let before=s.diagnostic_state_hash();
        assert!(s.diagnostic_remaining_headroom_v1().iter().all(|x|*x>=1));
        assert_eq!(search(&s,Limits{simulations:n,transitions:n,depth:1,seed:0},&e),Err(Error::NonNaturalTerminal));
        assert_eq!(before,s.diagnostic_state_hash());
        use crate::policy_observation_v6::tests::{put,ready_state};
        let mut state=ready_state();
        put(&mut state,PlayerId::P0,"Lightning Bolt",crate::state::Zone::Hand);
        state.players[0].mana_pool[crate::mana::ManaColor::R.pool_index()]=3;
        let id=put(&mut state,PlayerId::P0,"Forest",crate::state::Zone::Library);
        state.engine.initiative_source=Some(crate::state::AbilitySourceContractV4::capture(&state,id));
        let s=FastActorSessionV1::from_v3_fixture_state(state);let before=s.diagnostic_state_hash();
        assert!(matches!(search(&s,Limits{simulations:32,transitions:128,depth:4,seed:0},&e),Err(Error::State{stage:StateStage::Redeterminize,source:V4SearchStateErrorV1::HiddenStateContract})));
        assert_eq!(before,s.diagnostic_state_hash());
    }
    #[test]
    fn v4_search_core_natural_terminal_sign_both_seats() {
        use crate::policy_observation_v6::tests::{put,ready_state};
        use crate::state::Zone;
        for actor in [PlayerId::P0,PlayerId::P1] {
            let mut state=ready_state();state.active_player=actor;state.priority_player=actor;
            for owner in [actor,actor.opponent()] {
                state.players[owner.index()].life=3;
                put(&mut state,owner,"Lightning Bolt",Zone::Hand);
                state.players[owner.index()].mana_pool[crate::mana::ManaColor::R.pool_index()]=3;
                for name in ["Forest","Mountain","Island"] {put(&mut state,owner,name,Zone::Library);}
            }
            let s=FastActorSessionV1::from_v3_fixture_state(state);
            let p=FrozenPlayPolicyV1::training_fixture_v4();let e=V4SearchLeafEvaluatorV1::new(&p).unwrap();
            let o=search(&s,Limits{simulations:128,transitions:1024,depth:8,seed:29},&e).unwrap();
            assert!(o.census.natural_wins>0 && o.census.natural_losses>0,"{o:?}");
            assert_eq!(o.census.natural,o.census.natural_wins+o.census.natural_losses+o.census.natural_draws);
            let limits=Limits{simulations:128,transitions:1024,depth:8,seed:29};
            let off=search_with_backup(&s,limits,&e,RootAllocation::RoundRobin,InteriorBonus::PriorFree,BackupMode::Off,None).unwrap();
            let mut report=search_with_backup(&s,limits,&e,RootAllocation::RoundRobin,InteriorBonus::PriorFree,BackupMode::Report,None).unwrap();
            let estimate=report.estimator.take().unwrap();assert_eq!(report,off);
            assert!(!serde_json::to_value(&off).unwrap().as_object().unwrap().contains_key("estimator"));
            let blend=search_with_backup(&s,limits,&e,RootAllocation::RoundRobin,InteriorBonus::PriorFree,BackupMode::Blend,None).unwrap();
            for (label,out,est) in [("report",&report,&estimate),("blend",&blend,blend.estimator.as_ref().unwrap())] {
                let multi=out.tree.iter().flat_map(|n|&n.children).filter(|x|x.len()>1).count();
                println!("hidden-world actor={actor:?} mode={label} multi_successor_edges={multi} root_estimates={:?} root_means={:?}",est.nodes[0].edges.iter().map(|e|e.value).collect::<Vec<_>>(),out.root_value_sums.iter().zip(&out.root_visits).map(|(v,n)|v/i64::from(*n)).collect::<Vec<_>>());
                assert_eq!(est.recompute_passes,out.simulations);
            }
        }
    }

    #[test]
    fn v4_search_prior_free_bonus_changes_both_seat_selection_without_prior_order_change() {
        for actor in [PlayerId::P0,PlayerId::P1] {
            for root in [actor,actor.opponent()] {
                let sign=if root==actor{1}else{-1};
                let n=Node{key:[0;32],actor,visits:7,prior:vec![75,999925],witness:None,
                    actions:vec![SearchActionStatV1{visits:1,value_sum:sign * -3318,child_nodes:vec![]},SearchActionStatV1{visits:1,value_sum:sign * -3768,child_nodes:vec![]}]};
                assert_eq!(choose(&n,root,InteriorBonus::PriorWeighted).unwrap(),1);
                assert_eq!(choose(&n,root,InteriorBonus::PriorFree).unwrap(),0);
                assert_eq!(score(&n,root,0,InteriorBonus::PriorFree).unwrap(),sign * -490);
                assert_eq!(score(&n,root,1,InteriorBonus::PriorFree).unwrap(),sign * -940);
                let mut unvisited=n;for a in &mut unvisited.actions {a.visits=0;a.value_sum=0;}
                assert_eq!(choose(&unvisited,root,InteriorBonus::PriorFree).unwrap(),1);
            }
        }
    }

    #[test]
    fn v4_search_core_round_robin_only_forces_root_and_traces_choices() {
        for actor in [PlayerId::P0,PlayerId::P1] {
          for interior_bonus in [InteriorBonus::PriorWeighted,InteriorBonus::PriorFree] {
            let s=session(actor);let n=live(&s).unwrap().legal_action_count;
            let original=s.diagnostic_state_hash();
            let p=FrozenPlayPolicyV1::training_fixture_v4();let e=V4SearchLeafEvaluatorV1::new(&p).unwrap();
            let l=Limits{simulations:16*n,transitions:128*n,depth:8,seed:29};
            let a=search_with_policies(&s,l,&e,RootAllocation::RoundRobin,interior_bonus,None).unwrap();
            assert!(a.root_visits.iter().all(|x|*x==16));assert_eq!(a.selected,a.selected_by_mean);
            assert_eq!(a.trace.len(),a.simulations as usize);
            assert_eq!(a.trace.iter().map(|x|x.choices.len() as u32).sum::<u32>(),a.transitions);
            for simulation in &a.trace {for choice in &simulation.choices {
                assert!(choice.node<a.nodes);assert!(choice.action<a.tree[choice.node].priors.len());
                assert!(choice.node==0 || (choice.mode!="round_robin" && choice.mode!="coverage"));
                if choice.mode=="puct" || choice.mode=="prior_free" {
                    assert_eq!(choice.mode,if interior_bonus==InteriorBonus::PriorFree{"prior_free"}else{"puct"});
                    let selected=choice.scores[choice.action].unwrap();let maximizing=a.tree[choice.node].actor==actor.index() as u8;
                    assert!(choice.scores.iter().all(|v|if maximizing{selected>=v.unwrap()}else{selected<=v.unwrap()}));
                }
            }}
            assert_eq!(a,search_with_policies(&s,l,&e,RootAllocation::RoundRobin,interior_bonus,None).unwrap());
            assert_eq!(original,s.diagnostic_state_hash());
          }
        }
    }

}
