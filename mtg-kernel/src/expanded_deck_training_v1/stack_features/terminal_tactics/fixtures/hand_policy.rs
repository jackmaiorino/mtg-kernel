//! Bounded hand sensitivity control, no training data or action selection.
use super::*;

fn position(actor:u8,bolt:bool,reverse:bool)->Result<(FastActorSessionV1,ObjectId),String> {
    let own=PlayerId(actor);let other=PlayerId(1-actor);
    let mut state=GameState::new_from_libraries(&[],&[],crate::rl::card_name,77_006);
    state.step=Step::DeclareBlockers;state.active_player=other;state.priority_player=own;
    state.players[own.index()].life=3;state.players[other.index()].life=6;
    let spell=put(&mut state,own,"Lightning Bolt",Zone::Hand);
    put(&mut state,own,if bolt {"Lightning Bolt"} else {"Mountain"},Zone::Hand);
    let attacker=put(&mut state,other,"Myr Enforcer",Zone::Battlefield);
    state.objects.get_mut(attacker).tapped=true;
    state.objects.get_mut(attacker).damage=1;
    state.engine.combat.attackers_declared=true;state.engine.combat.blockers_declared=true;
    state.engine.combat.attackers=vec![attacker];
    for owner in [own,other] {
        for name in ["Forest","Island","Mountain","Swamp"] {put(&mut state,owner,name,Zone::Library);}
        if reverse {state.players[owner.index()].library.reverse();}
    }
    state.players[own.index()].mana_pool[ManaColor::R.pool_index()]=2;
    engine::step(&mut state,Action::CastSpell(spell)).map_err(err)?;
    ensure(matches!(engine::advance_until_decision(&mut state),Decision::ChooseTargets{..}),"missing hand-control target root")?;
    Ok((FastActorSessionV1::from_public_terminal_fixture_v1(state),attacker))
}

fn line(session:&FastActorSessionV1,index:u32)->Result<Value,String> {
    let initial=session.game_state();let mut branch=session.clone();let mut path=Vec::new();
    let FastActorResponseV1::Decision(root)=branch.current_response() else {return Err("missing root".into());};
    for step in 0..32 {
        let after=branch.game_state();
        ensure(initial.turn==after.turn && initial.library_knowledge==after.library_knowledge
            && (0..2).all(|i|initial.players[i].library==after.players[i].library
                && initial.players[i].draws_this_turn==after.players[i].draws_this_turn),"hand witness crossed information boundary")?;
        ensure(after.players[1-seat(root.acting_player) as usize].hand.is_empty(),"opponent gained hidden response")?;
        let d=match branch.current_response() {
            FastActorResponseV1::Terminal(t)=>{
                ensure(t.terminal_classification==TerminalClassificationV1::Natural,"nonnatural terminal")?;
                return Ok(json!({"outcome":if t.winner==Some(root.acting_player){"win"}else{"loss"},"terminal":t,"path":path}));
            },
            FastActorResponseV1::Decision(d)=>d,
        };
        if after.step==Step::Main2 {return Ok(json!({"outcome":"combat_survived","life":[after.players[0].life,after.players[1].life],"path":path}));}
        let (_,actions)=PairedBo1PolicyInputV1::new(&branch,d).diagnostic_visible_v4()?;
        let chosen=if step==0 {index as usize}
        else if let Some(i)=actions.iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..} if *player!=root.acting_player)) {
            ensure(d.acting_player==root.acting_player,"opponent target choice")?;i
        } else if let Some(i)=actions.iter().position(|a|matches!(a,ActionSemanticV1::CastSpell{source,..} if crate::rl::card_name(source.card_db_id)=="Lightning Bolt")) {
            ensure(d.acting_player==root.acting_player,"opponent cast choice")?;i
        } else {ensure(actions.len()==1 && matches!(actions[0],ActionSemanticV1::Pass{..}),"unproved hand continuation")?;0};
        path.push(json!({"step":d.step,"actor":d.acting_player,"action":actions[chosen]}));
        branch.step(d.episode_id,d.step,chosen as u32).map_err(err)?;
    }
    Err("hand witness depth exceeded".into())
}

pub(super) fn run(command:Command)->Result<Value,String> {
    let (mut policy,_)=initialize(&command.source)?;let mut records=Vec::new();
    for actor in 0..2 {for bolt in [false,true] {
        let mut invariant=None;
        for reverse in [false,true] {
            let (s,attacker)=position(actor,bolt,reverse)?;
            let FastActorResponseV1::Decision(d)=s.current_response() else{return Err("terminal root".into());};
            let (visible,actions)=PairedBo1PolicyInputV1::new(&s,d).diagnostic_visible_v4()?;
            let face=actions.iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..} if *player!=d.acting_player)).ok_or("missing face")?;
            let removal=actions.iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..} if object.arena_id==attacker.0)).ok_or("missing removal")?;
            let face_line=line(&s,face as u32)?;let removal_line=line(&s,removal as u32)?;
            ensure(face_line["outcome"]==if bolt{"win"}else{"loss"},"face witness differed")?;
            ensure(removal_line["outcome"]=="combat_survived","removal survival differed")?;
            let scores=policy.score_fast_session_v1(&s)?;
            let tensor=TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v4()?.common);
            let signature=json!({"visible":visible,"actions":actions,"tensor":tensor,"logits":bits(&scores.logits),"value_bits":scores.value.to_bits(),"face_line":face_line,"removal_line":removal_line});
            ensure(invariant.as_ref().is_none_or(|prior|prior==&signature),"hidden-order hand diagnostic differs")?;invariant=Some(signature);
            let p=probability(&scores.logits);
            records.push(json!({"actor":actor,"has_second_bolt":bolt,"reverse":reverse,"face_index":face,"removal_index":removal,"probabilities":p,"value":scores.value,"signature":invariant}));
        }
    }}
    for actor in 0..2 {
        let pair:Vec<_>=records.iter().filter(|r|r["actor"]==actor && r["reverse"]==false).collect();
        ensure(pair.len()==2 && pair[0]["signature"]["actions"]==pair[1]["signature"]["actions"],"counterfactual root menus differ")?;
    }
    let result=json!({"schema":"hand-policy-control/v1","positions":8,"distinct_actor_relative_hands":2,"source":command.source,"model":policy.actual_model_identity_v1(),"records":records,"non_claim":"Synthetic diagnostic only. Removal survives this combat, not a certified match win; no natural prevalence, unique optimal action, training replication, or promotion claim."});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory,"result.json",&result)?;Ok(result)
}
