//! Offline, report-only execution of typed public winning proofs.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize)]
pub(super) struct Binding {
    actor: crate::rl::PlayerSeatV1,
    visible: Value,
    actions: Vec<ActionSemanticV1>,
}
impl Binding {
    pub(super) fn capture(s:&FastActorSessionV1,d:crate::rl_session::FastActorDecisionV1)->Result<Self,String> {
        let (visible,actions)=PairedBo1PolicyInputV1::new(s,d).diagnostic_visible_v4()?;
        Ok(Self{actor:d.acting_player,visible:serde_json::to_value(visible).map_err(err)?,actions})
    }
    fn check(&self,s:&FastActorSessionV1,d:crate::rl_session::FastActorDecisionV1)->Result<(),String> {
        let actual=Self::capture(s,d)?;
        ensure(serde_json::to_vec(self).map_err(err)?==serde_json::to_vec(&actual).map_err(err)?,"certificate semantic/visible binding differs")
    }
}
pub(super) trait Proof: Sized {
    fn lower(&self)->i8;
    fn reason(&self)->&str;
    fn branches(&self)->&[(u32,Self)];
    fn binding(&self)->Option<&Binding>;
}
#[derive(Debug,Serialize)]
struct Strategy { binding:Option<Binding>, branches:Vec<(u32,Strategy)> }
fn extract<T:Proof>(tree:&T)->Result<Strategy,String> {
    ensure(tree.lower()==1,"cannot execute uncertified tree")?;
    if tree.reason()=="natural_terminal" {
        ensure(tree.branches().is_empty(),"terminal proof has children")?;
        return Ok(Strategy{binding:None,branches:Vec::new()});
    }
    let binding=tree.binding().ok_or("certificate binding absent")?.clone();
    ensure(tree.branches().len()==binding.actions.len(),"certificate does not cover legal menu")?;
    for (i,(index,_)) in tree.branches().iter().enumerate() {ensure(*index==i as u32,"noncanonical proof menu")?;}
    let selected:Vec<_>=match tree.reason() {
        "own_choice"=>vec![tree.branches().iter().find(|(_,child)|child.lower()==1).ok_or("own winning branch missing")?],
        "opponent_choice"=>tree.branches().iter().collect(),
        _=>return Err("non-executable proof reason".into()),
    };
    let branches=selected.into_iter().map(|(i,t)|Ok((*i,extract(t)?))).collect::<Result<_,String>>()?;
    Ok(Strategy{binding:Some(binding),branches})
}
fn leaves(s:&Strategy)->u64 {if s.binding.is_none(){1}else{s.branches.iter().map(|(_,s)|leaves(s)).sum()}}
#[derive(Default,Serialize)]
struct Counts { natural_wins:u64, own_nodes:u64, opponent_nodes:u64, transitions:u64, minimum_headroom:Option<[u64;2]> }
fn execute(s:&FastActorSessionV1,initial:&crate::state::GameState,actor:crate::rl::PlayerSeatV1,
    strategy:&Strategy,counts:&mut Counts)->Result<(),String> {
    let state=s.game_state();
    ensure(unchanged_information(initial,state)
        && state.players[1-seat(actor) as usize].hand.is_empty()
        && initial.legacy_rng()==state.legacy_rng()
        && initial.environment_randomization_v2()==state.environment_randomization_v2(),"certificate execution crossed information/randomness boundary")?;
    let headroom=s.diagnostic_remaining_headroom_v1();
    counts.minimum_headroom=Some(match counts.minimum_headroom {None=>headroom,Some(old)=>[old[0].min(headroom[0]),old[1].min(headroom[1])]});
    match s.current_response() {
        FastActorResponseV1::Terminal(t)=>{
            ensure(strategy.binding.is_none() && strategy.branches.is_empty(),"strategy ended at unexpected terminal")?;
            ensure(t.terminal_classification==TerminalClassificationV1::Natural && t.winner==Some(actor),"certificate did not execute to natural actor win")?;
            counts.natural_wins+=1;
        }
        FastActorResponseV1::Decision(d)=>{
            let binding=strategy.binding.as_ref().ok_or("strategy terminal before engine terminal")?;
            binding.check(s,d)?;
            if d.acting_player==actor {ensure(strategy.branches.len()==1,"own strategy is not single action")?;counts.own_nodes+=1;}
            else {
                ensure(strategy.branches.len()==binding.actions.len(),"opponent coverage incomplete")?;
                for (i,(index,_)) in strategy.branches.iter().enumerate(){ensure(*index==i as u32,"opponent response omitted")?;}
                counts.opponent_nodes+=1;
            }
            for (index,next) in &strategy.branches {
                ensure((*index as usize)<binding.actions.len(),"strategy action outside menu")?;
                let mut child=s.clone();child.step(d.episode_id,d.step,*index).map_err(err)?;
                counts.transitions+=1;execute(&child,initial,actor,next,counts)?;
            }
        }
    }
    Ok(())
}
pub(super) fn report<T:Proof>(s:&FastActorSessionV1,root:crate::rl_session::FastActorDecisionV1,
    trees:&[(u32,T)])->Result<Value,String> {
    let before=s.diagnostic_state_hash();let headroom=s.diagnostic_remaining_headroom_v1();
    let binding=Binding::capture(s,root)?;let mut reports=Vec::new();
    for (index,tree) in trees {
        if tree.lower()!=1 {continue;}
        let strategy=Strategy{binding:Some(binding.clone()),branches:vec![(*index,extract(tree)?)]};
        let expected=leaves(&strategy);let bytes=serde_json::to_vec(&strategy).map_err(err)?;
        let mut counts=Counts::default();execute(s,s.game_state(),root.acting_player,&strategy,&mut counts)?;
        ensure(counts.natural_wins==expected,"executed proof leaf count differs")?;
        reports.push(json!({"index":index,"expected_winning_leaves":expected,"counts":counts,
            "strategy_sha256":format!("{:x}",Sha256::digest(&bytes))}));
    }
    ensure(before==s.diagnostic_state_hash() && headroom==s.diagnostic_remaining_headroom_v1(),"executor mutated original session")?;
    Ok(json!({"schema":"offline-public-certificate-execution/v1","winning_actions":reports,
        "root_headroom":headroom,"original_unchanged":true,
        "scope":"Engine-relative diagnostic execution only. No policy action returned or playing eligibility."}))
}
#[cfg(test)]
pub(super) fn mutated_menu_rejected(s:&FastActorSessionV1,root:crate::rl_session::FastActorDecisionV1)->bool {
    let mut binding=Binding::capture(s,root).unwrap();binding.actions.pop();binding.check(s,root).is_err()
}

/// Three separate invisible perturbations must preserve the entire audit bytes.
/// The result is a report, never a reusable strategy or a live recommendation.
pub(crate) fn invariance_report(s:&FastActorSessionV1,root:crate::rl_session::FastActorDecisionV1,
    audit:fn(&FastActorSessionV1,crate::rl_session::FastActorDecisionV1)->Result<Value,String>)->Result<Value,String> {
    let baseline=audit(s,root)?;let bytes=serde_json::to_vec(&baseline).map_err(err)?;
    let binding=Binding::capture(s,root)?;let visible=serde_json::to_vec(&binding).map_err(err)?;
    let before=s.diagnostic_state_hash();let mut variants=Vec::new();
    for (name,library,rng) in [("own_library",Some(seat(root.acting_player) as usize),false),
        ("opponent_library",Some(1-seat(root.acting_player) as usize),false),("randomness",None,true)] {
        let changed=s.diagnostic_certificate_perturbed_clone_v1(library,rng)?;
        ensure(visible==serde_json::to_vec(&Binding::capture(&changed,root)?).map_err(err)?,"certificate perturbation was actor-visible")?;
        ensure(bytes==serde_json::to_vec(&audit(&changed,root)?).map_err(err)?,"certificate hidden-state invariance failed")?;
        variants.push(name);
    }
    ensure(before==s.diagnostic_state_hash(),"invariance diagnostic mutated source")?;
    Ok(json!({"audit":baseline,"byte_identical_variants":variants,"original_unchanged":true}))
}
