//! Public combat controls. Unresolved alternatives are never labeled losses.
use super::*;

fn position(actor:u8,hidden:bool,opponent_life:i32)->Result<(FastActorSessionV1,ObjectId),String> {
    let actor=PlayerId(actor);let opponent=PlayerId(1-actor.0);
    let mut state=GameState::new_from_libraries(&[],&[],crate::rl::card_name,77_002);
    state.step=Step::DeclareBlockers;state.active_player=actor;state.priority_player=actor;
    state.players[actor.index()].life=3;state.players[opponent.index()].life=opponent_life;
    let spell=put(&mut state,actor,"Lightning Bolt",Zone::Hand);
    let attacker=put(&mut state,actor,"Avenging Hunter",Zone::Battlefield);
    let blocker=put(&mut state,opponent,"Sacred Cat",Zone::Battlefield);
    state.objects.get_mut(attacker).tapped=true;
    state.objects.get_mut(blocker).counters.plus1_plus1=3;
    state.objects.get_mut(blocker).damage=1;
    state.engine.combat.attackers_declared=true;
    state.engine.combat.blockers_declared=true;
    state.engine.combat.attackers=vec![attacker];
    state.engine.combat.blocked_by=vec![(attacker,vec![blocker])];
    for owner in [PlayerId::P0,PlayerId::P1] {
        for name in ["Forest","Island","Mountain","Swamp"] {put(&mut state,owner,name,Zone::Library);}
        if hidden {state.players[owner.index()].library.reverse();}
    }
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()]=1;
    engine::step(&mut state,Action::CastSpell(spell)).map_err(err)?;
    ensure(matches!(engine::advance_until_decision(&mut state),Decision::ChooseTargets{..}),"combat fixture lacks target decision")?;
    Ok((FastActorSessionV1::from_public_terminal_fixture_v1(state),blocker))
}

fn combat_witness(session:&FastActorSessionV1,root:crate::rl_session::FastActorDecisionV1,index:u32)->Result<Value,String> {
    let initial=session.game_state();let mut branch=session.clone();let mut current=root;let mut action=index;let mut line=Vec::new();
    for _ in 0..24 {
        line.push(json!({"actor":current.acting_player,"step":current.step,"index":action}));
        branch.step(current.episode_id,current.step,action).map_err(err)?;
        let after=branch.game_state();
        if initial.turn!=after.turn || initial.library_knowledge!=after.library_knowledge
            || !(0..2).all(|i|initial.players[i].library==after.players[i].library && initial.players[i].draws_this_turn==after.players[i].draws_this_turn)
            || !matches!(after.step,Step::DeclareBlockers|Step::CombatDamage|Step::EndCombat|Step::Main2) {
            return Ok(json!({"classification":"unresolved_information_boundary","line":line}));
        }
        match branch.current_response() {
            FastActorResponseV1::Terminal(t)=>return Ok(json!({"classification":if t.terminal_classification!=TerminalClassificationV1::Natural {"unresolved_non_natural"}
                else if t.winner==Some(root.acting_player) {"win"} else if t.winner.is_some() {"loss"} else {"draw"},"line":line,"terminal":t})),
            FastActorResponseV1::Decision(d)=>{
                if after.step==Step::Main2 {return Ok(json!({"classification":"unresolved_combat_finished","line":line,"life":[after.players[0].life,after.players[1].life]}));}
                let (_,actions)=PairedBo1PolicyInputV1::new(&branch,d).diagnostic_visible_v4()?;
                // No chosen cooperative continuation, even for the learner.
                if actions.len()!=1 {return Ok(json!({"classification":"unresolved_next_choice","line":line,"actions":actions}));}
                current=d;action=0;
            }
        }
    }
    Ok(json!({"classification":"unresolved_depth","line":line}))
}

pub(super) fn run(command:Command)->Result<Value,String> {
    let (mut policy,_)=initialize(&command.source)?;let mut records=Vec::new();
    for opponent_life in [3,5] {for actor in 0..2 {
        let mut invariant=None;
        for hidden in [false,true] {
            let (session,blocker)=position(actor,hidden,opponent_life)?;
            let FastActorResponseV1::Decision(d)=session.current_response() else {return Err("combat root terminal".into());};
            let (visible,actions)=PairedBo1PolicyInputV1::new(&session,d).diagnostic_visible_v4()?;
            let scores=policy.score_fast_session_v1(&session)?;
            let tensor=TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v4()?.common);
            let mut outcomes=Vec::new();for i in 0..d.legal_action_count {outcomes.push(combat_witness(&session,d,i)?);}
            let mut creature=None;let mut face=None;let mut fatal=None;
            for (i,a) in actions.iter().enumerate() {if let ActionSemanticV1::ChooseTarget{target,..}=a {match target {
                crate::rl::TargetRefV1::Player{player}=>if *player==d.acting_player {fatal=Some(i)} else {face=Some(i)},
                crate::rl::TargetRefV1::Object{object}=>if object.arena_id==blocker.0 {creature=Some(i)},
            }}}
            let creature=creature.ok_or("missing blocker target")?;let face=face.ok_or("missing face target")?;let fatal=fatal.ok_or("missing self target")?;
            if outcomes[creature]["classification"]!="win" {return Err(format!("removing lifelink blocker failed to certify lethal: actor={actor} life={opponent_life} creature={creature} outcomes={outcomes:?} visible={visible:?}"));}
            ensure(outcomes[fatal]["classification"]=="loss","self lethal negative control failed")?;
            ensure((outcomes[face]["classification"]=="win")== (opponent_life==3),"face control differs from declared lethal contrast")?;
            let signature=json!({"visible":visible,"actions":actions,"tensor":tensor,"logits":bits(&scores.logits),"value_bits":scores.value.to_bits(),"outcomes":outcomes});
            ensure(invariant.as_ref().is_none_or(|v|v==&signature),"combat hidden order invariance failed")?;invariant=Some(signature);
            let p=probability(&scores.logits);let mut best=0;for i in 1..scores.logits.len(){if scores.logits[i]>scores.logits[best]{best=i;}}
            records.push(json!({"actor":actor,"opponent_life":opponent_life,"hidden_order_variant":hidden,"blocker_index":creature,"face_index":face,"argmax":best,
                "argmax_certified_win":outcomes[best]["classification"]=="win","winning_softmax_mass":p.iter().zip(&outcomes).filter(|(_,o)|o["classification"]=="win").map(|(p,_)|p).sum::<f64>(),"probabilities":p,"signature":invariant}));
        }
    }}
    let result=json!({"schema":"public-combat-terminal-controls/v1","positions":8,"distinct_public_positions":4,"records":records,"source":command.source,"model":policy.actual_model_identity_v1(),
        "non_claim":"Synthetic missed immediate-lethal controls only; unresolved alternatives are not forced losses or natural prevalence."});
    fs::create_dir(&command.output_directory).map_err(err)?;publish_json(&command.output_directory,"result.json",&result)?;Ok(result)
}
