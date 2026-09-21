//! Small synthetic positive/negative controls, never natural-game evidence.
use super::*;
use crate::card_def::card_id_by_name;
use crate::engine::{self,Action,Decision};
use crate::ids::ObjectId;
use crate::mana::ManaColor;
use crate::state::{Counters,GameObject,GameState,ObjectStateV4,Step,Zone};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub source: ExpandedModelSourceV1,
    pub output_directory: PathBuf,
}

fn put(state:&mut GameState,owner:PlayerId,name:&str,zone:Zone)->ObjectId {
    let card_def=card_id_by_name(name).expect("declared fixture card");
    let id=state.objects.push(GameObject {card_def,name:name.into(),owner,controller:owner,zone,
        tapped:false,summoning_sick:false,damage:0,counters:Counters::default(),attachments:Vec::new(),
        v4:ObjectStateV4::from_card_def(card_def),spell_copy_origin:None,plotted_turn:None,zone_change_count:0});
    match zone {
        Zone::Hand=>state.players[owner.index()].hand.push(id),
        Zone::Battlefield=>state.players[owner.index()].battlefield.push(id),
        Zone::Library=>state.players[owner.index()].library.push(id),
        _=>unreachable!(),
    }
    id
}

fn fixture(card:&str,damage:i32,artifacts:usize,actor:u8,hidden:bool)->Result<FastActorSessionV1,String> {
    let actor=PlayerId(actor);
    let mut state=GameState::new_from_libraries(&[],&[],crate::rl::card_name,77_001);
    state.step=Step::Main1;state.active_player=actor;state.priority_player=actor;
    state.players[0].life=damage;state.players[1].life=damage;
    let spell=put(&mut state,actor,card,Zone::Hand);
    for _ in 0..artifacts {put(&mut state,actor,"Ornithopter",Zone::Battlefield);}
    for owner in [PlayerId::P0,PlayerId::P1] {
        for name in ["Forest","Island","Mountain","Swamp"] {put(&mut state,owner,name,Zone::Library);}
        if hidden {state.players[owner.index()].library.reverse();}
    }
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()]=1;
    engine::step(&mut state,Action::CastSpell(spell)).map_err(err)?;
    ensure(matches!(engine::advance_until_decision(&mut state),Decision::ChooseTargets{..}),"fixture did not reach target decision")?;
    Ok(FastActorSessionV1::from_public_terminal_fixture_v1(state))
}

pub fn run(command:Command)->Result<Value,String> {
    let (mut policy,_)=initialize(&command.source)?;
    let mut records=Vec::new();
    for (card,damage,artifacts) in [("Lightning Bolt",3,0),("Lava Dart",1,0),("Galvanic Blast",2,0),("Galvanic Blast",4,3)] {
        for actor in 0..2 {
            let mut invariant:Option<Value>=None;
            for hidden in [false,true] {
                let session=fixture(card,damage,artifacts,actor,hidden)?;
                let FastActorResponseV1::Decision(d)=session.current_response() else {return Err("fixture already terminal".into());};
                ensure(seat(d.acting_player)==actor,"fixture actor differs")?;
                let (visible,actions)=PairedBo1PolicyInputV1::new(&session,d).diagnostic_visible_v4()?;
                let scores=policy.score_fast_session_v1(&session)?;
                let tensor=TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v4()?.common);
                let mut outcomes=Vec::new();
                for index in 0..d.legal_action_count {outcomes.push(witness(&session,d,index)?);}
                let p=probability(&scores.logits);
                let mut correct=None;let mut fatal=None;
                for (i,action) in actions.iter().enumerate() {
                    if let ActionSemanticV1::ChooseTarget {target:crate::rl::TargetRefV1::Player {player},..}=action {
                        if *player==d.acting_player {fatal=Some(i)} else {correct=Some(i)}
                    }
                }
                let correct=correct.ok_or("missing opponent target")?;
                let fatal=fatal.ok_or("missing self target")?;
                ensure(outcomes[correct]["classification"]=="win","fixture opponent lethal not certified")?;
                ensure(outcomes[fatal]["classification"]=="loss","fixture self lethal not certified")?;
                let signature=json!({"visible":visible,"actions":actions,"tensor":tensor,
                    "logits":bits(&scores.logits),"value_bits":scores.value.to_bits(),"outcomes":outcomes});
                ensure(invariant.as_ref().is_none_or(|old|old==&signature),"hidden order changed actor scores or terminal witness")?;
                invariant=Some(signature);
                let mut best=0;for i in 1..scores.logits.len() {if scores.logits[i]>scores.logits[best] {best=i;}}
                records.push(json!({"card":card,"damage":damage,"artifacts":artifacts,"actor":actor,
                    "hidden_order_variant":hidden,"winning_index":correct,"losing_index":fatal,
                    "winning_softmax_probability":p[correct],"losing_softmax_probability":p[fatal],
                    "argmax":best,"argmax_wins":best==correct,"signature":invariant}));
            }
        }
    }
    let result=json!({"schema":"public-terminal-tactic-fixtures/v1","positions":16,"distinct_public_positions":8,
        "model":policy.actual_model_identity_v1(),"source":command.source,"records":records,
        "non_claim":"Synthetic elementary lethal-target controls and hidden-order invariance only; not natural prevalence or broad tactical strength."});
    fs::create_dir(&command.output_directory).map_err(err)?;publish_json(&command.output_directory,"result.json",&result)?;Ok(result)
}
