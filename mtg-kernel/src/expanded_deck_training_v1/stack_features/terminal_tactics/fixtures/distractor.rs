//! Immediate face-lethal controls with an unresolved creature-removal alternative.
use super::*;

fn position(actor:u8,hidden:bool,opponent_life:i32,large_cat:bool,opponent_hand:bool)->Result<(FastActorSessionV1,ObjectId),String> {
    let actor=PlayerId(actor);let opponent=PlayerId(1-actor.0);
    let mut state=GameState::new_from_libraries(&[],&[],crate::rl::card_name,77_003);
    state.step=Step::Main1;state.active_player=actor;state.priority_player=actor;
    state.players[actor.index()].life=3;state.players[opponent.index()].life=opponent_life;
    let spell=put(&mut state,actor,"Lightning Bolt",Zone::Hand);
    let cat=put(&mut state,opponent,"Sacred Cat",Zone::Battlefield);
    if opponent_hand {put(&mut state,opponent,"Island",Zone::Hand);}
    if large_cat {state.objects.get_mut(cat).counters.plus1_plus1=3;state.objects.get_mut(cat).damage=1;}
    for owner in [PlayerId::P0,PlayerId::P1] {
        for name in ["Forest","Island","Mountain","Swamp"] {put(&mut state,owner,name,Zone::Library);}
        if hidden {state.players[owner.index()].library.reverse();}
    }
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()]=1;
    engine::step(&mut state,Action::CastSpell(spell)).map_err(err)?;
    ensure(matches!(engine::advance_until_decision(&mut state),Decision::ChooseTargets{..}),"distractor fixture lacks target decision")?;
    Ok((FastActorSessionV1::from_public_terminal_fixture_v1(state),cat))
}

pub(super) fn run(command:Command)->Result<Value,String> {
    let (mut policy,_)=initialize(&command.source)?;let mut records=Vec::new();
    for actor in 0..2 {
        let (session,_)=position(actor,false,3,false,true)?;
        let FastActorResponseV1::Decision(d)=session.current_response() else {return Err("shadow exclusion root terminal".into());};
        let audit=PairedBo1PolicyInputV1::new(&session,d).diagnostic_terminal_targets_v1()?;
        ensure(audit["status"]=="opponent_hand_nonempty","shadow did not abstain with hidden opponent hand")?;
    }
    for large_cat in [false,true] {for opponent_life in [3,4] {for actor in 0..2 {
        let mut invariant=None;
        for hidden in [false,true] {
            let (session,cat)=position(actor,hidden,opponent_life,large_cat,false)?;
            let FastActorResponseV1::Decision(d)=session.current_response() else {return Err("distractor root terminal".into());};
            ensure(seat(d.acting_player)==actor,"distractor actor differs")?;
            let (visible,actions)=PairedBo1PolicyInputV1::new(&session,d).diagnostic_visible_v4()?;
            ensure(actions.len()==3 && d.legal_action_count==3,"distractor menu differs")?;
            let mut face=None;let mut fatal=None;let mut creature=None;
            for (i,a) in actions.iter().enumerate() {if let ActionSemanticV1::ChooseTarget{target,..}=a {match target {
                crate::rl::TargetRefV1::Player{player}=>if *player==d.acting_player {fatal=Some(i)} else {face=Some(i)},
                crate::rl::TargetRefV1::Object{object}=>if object.arena_id==cat.0 {creature=Some(i)},
            }}}
            let face=face.ok_or("missing opponent target")?;let fatal=fatal.ok_or("missing self target")?;let creature=creature.ok_or("missing creature target")?;
            let mut outcomes=Vec::new();for i in 0..d.legal_action_count {outcomes.push(witness(&session,d,i)?);}
            ensure(outcomes[fatal]["classification"]=="loss","self lethal control failed")?;
            ensure((outcomes[face]["classification"]=="win")== (opponent_life==3),"face lethal threshold differs")?;
            ensure(outcomes[creature]["classification"].as_str().is_some_and(|c|c.starts_with("unresolved_")),"creature alternative unexpectedly certified")?;
            if opponent_life==4 {ensure(outcomes[face]["classification"].as_str().is_some_and(|c|c.starts_with("unresolved_")),"nonlethal face unexpectedly certified")?;}
            let shadow=PairedBo1PolicyInputV1::new(&session,d).diagnostic_terminal_targets_v1()?;
            ensure(shadow["status"]=="audited" && shadow["outcomes"]==json!(outcomes),"shadow path differs from fixture witnesses")?;
            // Only score once the predeclared terminal controls are established.
            let scores=policy.score_fast_session_v1(&session)?;
            let tensor=TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v4()?.common);
            let p=probability(&scores.logits);let mut best=0;
            for i in 1..scores.logits.len() {if scores.logits[i]>scores.logits[best] {best=i;}}
            let signature=json!({"visible":visible,"actions":actions,"tensor":tensor,"logits":bits(&scores.logits),"value_bits":scores.value.to_bits(),"outcomes":outcomes});
            ensure(invariant.as_ref().is_none_or(|old|old==&signature),"distractor hidden-order invariance failed")?;invariant=Some(signature);
            records.push(json!({"actor":actor,"opponent_life":opponent_life,"large_cat":large_cat,"hidden_order_variant":hidden,
                "face_index":face,"creature_index":creature,"self_index":fatal,"argmax":best,
                "immediate_win_available":opponent_life==3,"argmax_certified_win":outcomes[best]["classification"]=="win",
                "face_softmax_probability":p[face],"probabilities":p,"signature":invariant}));
        }
    }}}
    let result=json!({"schema":"public-lethal-distractor-controls/v1","positions":16,"distinct_public_positions":8,
        "records":records,"source":command.source,"model":policy.actual_model_identity_v1(),
        "non_claim":"Constructed immediate-win diagnostic, not natural mistake prevalence or whole-game optimality; unresolved alternatives are not forced losses."});
    fs::create_dir(&command.output_directory).map_err(err)?;publish_json(&command.output_directory,"result.json",&result)?;Ok(result)
}
