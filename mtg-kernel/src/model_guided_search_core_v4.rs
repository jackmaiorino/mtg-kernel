//! Additive, bounded V4 information-set search. No package or playing caller.
use crate::ids::PlayerId;
use crate::kernel_native_search_opponent_v1::{SearchActionStatV1,integer_ucb_bonus_v1,natural_terminal_value_v1,player_id_v1,select_final_root_action_v1};
use crate::model_guided_search_core_v1::{ModelGuidedSearchLeafEvaluatorV1 as Evaluator,ModelGuidedSearchLeafForwardV1 as Forward,ModelGuidedSearchLeafSiteV1 as Site};
use crate::model_guided_search_prior_quantization_v1::{quantize_prior_v1,prior_expansion_order_v1,puct_bonus_v1};
use crate::model_guided_search_value_quantization_v1::{quantize_value_v1,ModelGuidedSearchValueHeadDomainV1};
use crate::rl_session::{FastActorSessionV1,FastActorResponseV1,FastActorDecisionV1};
use crate::rl::TerminalClassificationV1;
use serde::Serialize;
use sha2::{Digest,Sha256};

#[derive(Debug,Clone,Copy)]
pub(crate) struct Limits {pub simulations:u32,pub transitions:u32,pub depth:u16,pub seed:u64}
#[derive(Debug,Clone,PartialEq,Eq)]
pub(crate) enum Error {NoDecision,InvalidBudget,InsufficientHeadroom,State(String),Evaluator(String),NonNaturalTerminal,CorruptTree}
type Result<T> = std::result::Result<T,Error>;
#[derive(Debug,Default,Clone,PartialEq,Serialize)]
pub(crate) struct Census {
    pub natural:u32,pub expanded:u32,pub depth:u32,pub budget:u32,pub coverage:u32,
    pub clip_low:u64,pub clip_high:u64,pub raw_min:Option<f32>,pub raw_max:Option<f32>,pub forwards:u64,
}
#[derive(Debug,Clone,PartialEq,Serialize)]
pub(crate) struct Outcome {
    pub selected:u32,pub simulations:u32,pub transitions:u32,pub nodes:usize,
    pub headroom:[u64;2],pub root_visits:Vec<u32>,pub root_value_sums:Vec<i64>,pub census:Census,
}
struct Node {key:[u8;32],actor:PlayerId,visits:u32,prior:Vec<u32>,actions:Vec<SearchActionStatV1>,witness:Option<[u8;32]>}
type Witness<'a> = Option<&'a mut dyn FnMut(&FastActorSessionV1,u32)->std::result::Result<[u8;32],String>>;

fn live(s:&FastActorSessionV1)->Result<FastActorDecisionV1> {
    match s.current_response(){FastActorResponseV1::Decision(d)=>Ok(d),_=>Err(Error::NoDecision)}
}
fn key(s:&FastActorSessionV1,depth:u16)->Result<[u8;32]> {
    s.kernel_search_visible_key_v4(u32::from(depth)).map_err(|e|Error::State(format!("{e:?}")))
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
fn choose(n:&Node,root:PlayerId)->Result<usize> {
    for i in prior_expansion_order_v1(&n.prior) {if n.actions[i].visits==0{return Ok(i);}}
    let maximizing=n.actor==root;let mut best=None;
    for (i,a) in n.actions.iter().enumerate() {
        let bonus=i64::try_from(puct_bonus_v1(integer_ucb_bonus_v1(n.visits,a.visits),n.prior[i]).map_err(|_|Error::CorruptTree)?).map_err(|_|Error::CorruptTree)?;
        let score=if maximizing{a.mean()+bonus}else{a.mean()-bonus};
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
pub(crate) fn search_inner<E:Evaluator>(session:&FastActorSessionV1,l:Limits,e:&E,mut w:Witness<'_>)->Result<Outcome> {
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|x|Error::Evaluator(format!("{x:?}")))?;
    let d=live(session)?;let count=d.legal_action_count;
    if count==0||l.depth==0||l.simulations<count||l.transitions<count{return Err(Error::InvalidBudget);}
    let headroom=session.diagnostic_remaining_headroom_v1();
    if headroom.iter().any(|x|*x<u64::from(l.depth)){return Err(Error::InsufficientHeadroom);}
    let root=player_id_v1(d.acting_player);let root_key=key(session,l.depth)?;
    let mut census=Census::default();let f=forward(e,session,root_key,d,Site::RootPrior,&mut census)?;
    let mut tree=vec![node(root_key,root,&f,witness(&mut w,session,count)?)?];
    let coverage_order=prior_expansion_order_v1(&tree[0].prior);
    let mut simulations=0;let mut transitions=0;
    while simulations<l.simulations && transitions<l.transitions {
        let mut sample=session.kernel_search_redeterminized_clone_v4(seed(root_key,l.seed,simulations))
            .map_err(|x|Error::State(format!("{x:?}")))?;
        let coverage=simulations<count;let mut remaining=l.depth;let mut at=0usize;
        let mut path=vec![0usize];let mut edges=Vec::new();let result;
        loop {
            let current=live(&sample)?;let k=key(&sample,remaining)?;
            if tree[at].key!=k||tree[at].actor!=player_id_v1(current.acting_player)
                ||tree[at].actions.len()!=current.legal_action_count as usize
                ||tree[at].witness!=witness(&mut w,&sample,current.legal_action_count)? {return Err(Error::CorruptTree);}
            let action=if coverage {coverage_order[simulations as usize]}else{choose(&tree[at],root)?};
            let token=sample.kernel_search_action_token_v4(current).map_err(|x|Error::State(format!("{x:?}")))?;
            let response=sample.kernel_search_consume_v4(current,token,action as u32).map_err(|x|Error::State(format!("{x:?}")))?;
            transitions+=1;remaining-=1;edges.push((at,action));
            let next=match response {
                FastActorResponseV1::Terminal(t)=>{
                    if t.terminal_classification!=TerminalClassificationV1::Natural{return Err(Error::NonNaturalTerminal);}
                    census.natural+=1;result=natural_terminal_value_v1(t.terminal_outcome,root).map_err(|_|Error::NonNaturalTerminal)?;break;
                },
                FastActorResponseV1::Decision(next)=>next,
            };
            let next_key=key(&sample,remaining)?;let actor=player_id_v1(next.acting_player);
            if remaining==0||transitions==l.transitions||coverage {
                if remaining==0 {census.depth+=1;} else if transitions==l.transitions {census.budget+=1;} else {census.coverage+=1;}
                let f=forward(e,&sample,next_key,next,Site::RevisitedDepthCapLeaf,&mut census)?;
                result=value(&f,actor,root)?;break;
            }
            if let Some(existing)=tree.iter().position(|n|n.key==next_key) {
                if !tree[at].actions[action].child_nodes.contains(&existing){tree[at].actions[action].child_nodes.push(existing);}
                at=existing;path.push(at);
            } else {
                let f=forward(e,&sample,next_key,next,Site::NewlyExpandedNode,&mut census)?;
                let created=tree.len();tree.push(node(next_key,actor,&f,witness(&mut w,&sample,next.legal_action_count)?)?);
                tree[at].actions[action].child_nodes.push(created);path.push(created);
                census.expanded+=1;result=value(&f,actor,root)?;break;
            }
        }
        for i in path {tree[i].visits+=1;}
        for (i,a) in edges {tree[i].actions[a].visits+=1;tree[i].actions[a].value_sum+=i64::from(result);}
        simulations+=1;
    }
    if tree[0].actions.iter().any(|a|a.visits==0)
        ||tree[0].visits!=simulations||tree[0].actions.iter().map(|a|a.visits).sum::<u32>()!=simulations
        ||census.natural+census.expanded+census.depth+census.budget+census.coverage!=simulations{return Err(Error::CorruptTree);}
    Ok(Outcome{selected:select_final_root_action_v1(&tree[0].actions).map_err(|_|Error::CorruptTree)?,
        simulations,transitions,nodes:tree.len(),headroom,
        root_visits:tree[0].actions.iter().map(|a|a.visits).collect(),root_value_sums:tree[0].actions.iter().map(|a|a.value_sum).collect(),census})
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
}
