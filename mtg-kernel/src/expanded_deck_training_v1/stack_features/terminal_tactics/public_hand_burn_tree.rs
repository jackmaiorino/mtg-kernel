//! Separate public Bolt plus hand Galvanic continuation diagnostic. No teaching labels.
use super::*;

const DEPTH: u32 = 32;
const NODES: u32 = 8192;

#[derive(Debug)]
struct Tree {
    lower: i8,
    upper: i8,
    reason: &'static str,
    branches: Vec<(u32, Tree)>,
}
// Lossless compact wire form keeps every branch under the recorder byte cap.
// [lower, upper, reason_code, [[action_index, child], ...]]
impl Serialize for Tree {
    fn serialize<S: serde::Serializer>(&self, serializer:S)->Result<S::Ok,S::Error> {
        use serde::ser::SerializeTuple;
        let code=match self.reason {
            "information_boundary"=>0u8,"non_natural"=>1,"natural_terminal"=>2,
            "depth_limit"=>3,"menu_limit"=>4,"unsupported_action"=>5,"node_limit"=>6,
            "own_choice"=>7,"opponent_choice"=>8,"test"=>9,_=>return Err(serde::ser::Error::custom("unknown burn tree reason")),
        };
        let mut tuple=serializer.serialize_tuple(4)?;
        tuple.serialize_element(&self.lower)?;tuple.serialize_element(&self.upper)?;
        tuple.serialize_element(&code)?;tuple.serialize_element(&self.branches)?;tuple.end()
    }
}
fn unknown(reason: &'static str) -> Tree {
    Tree { lower: -1, upper: 1, reason, branches: Vec::new() }
}
fn aggregate(branches: Vec<(u32, Tree)>, own: bool) -> Tree {
    let lower = if own { branches.iter().map(|x| x.1.lower).max() } else { branches.iter().map(|x| x.1.lower).min() }.unwrap();
    let upper = if own { branches.iter().map(|x| x.1.upper).max() } else { branches.iter().map(|x| x.1.upper).min() }.unwrap();
    Tree { lower, upper, reason: if own { "own_choice" } else { "opponent_choice" }, branches }
}
fn explore(s: &FastActorSessionV1, initial: &crate::state::GameState,
    actor: crate::rl::PlayerSeatV1, depth: u32, remaining: &mut u32) -> Result<Tree, String> {
    if !unchanged_information(initial, s.game_state())
        || !s.game_state().players[1-seat(actor) as usize].hand.is_empty() {
        return Ok(unknown("information_boundary"));
    }
    let d = match s.current_response() {
        FastActorResponseV1::Terminal(t) => {
            if t.terminal_classification != TerminalClassificationV1::Natural { return Ok(unknown("non_natural")); }
            let value = if t.winner == Some(actor) { 1 } else if t.winner.is_some() { -1 } else { 0 };
            return Ok(Tree { lower: value, upper: value, reason: "natural_terminal", branches: Vec::new() });
        }
        FastActorResponseV1::Decision(d) => d,
    };
    if depth == 0 { return Ok(unknown("depth_limit")); }
    let (_, actions) = PairedBo1PolicyInputV1::new(s,d).diagnostic_visible_v4()?;
    ensure(!actions.is_empty() && actions.len()==d.legal_action_count as usize,"burn tree menu differs")?;
    if actions.len()>32 { return Ok(unknown("menu_limit")); }
    let mut branches=Vec::new();
    for (i,a) in actions.iter().enumerate() {
        let supported = match a {
            ActionSemanticV1::Pass{..} => true,
            ActionSemanticV1::ActivateManaAbility{source,..} => matches!(crate::rl::card_name(source.card_db_id).as_str(), "Island"|"Mountain"|"Great Furnace"),
            ActionSemanticV1::CastSpell{actor:who,source} => *who==actor && source.zone==crate::state::Zone::Hand
                && crate::rl::card_name(source.card_db_id)=="Galvanic Blast",
            ActionSemanticV1::ChooseTarget{actor:who,source,remaining,..} => *who==actor && *remaining==1
                && crate::rl::card_name(source.card_db_id)=="Galvanic Blast",
            _ => false,
        };
        let child = if !supported { unknown("unsupported_action") }
            else if *remaining==0 { unknown("node_limit") }
            else {
                *remaining-=1;let mut child=s.clone();
                child.step(d.episode_id,d.step,i as u32).map_err(err)?;
                explore(&child,initial,actor,depth-1,remaining)?
            };
        branches.push((i as u32,child));
    }
    Ok(aggregate(branches,d.acting_player==actor))
}
pub(crate) fn audit(s:&FastActorSessionV1,root:crate::rl_session::FastActorDecisionV1)->Result<Value,String> {
    ensure(s.current_response()==FastActorResponseV1::Decision(root),"burn tree root binding differs")?;
    if !(2..=32).contains(&root.legal_action_count) {return Ok(json!({"status":"menu_outside_bounds"}));}
    let (visible,actions)=PairedBo1PolicyInputV1::new(s,root).diagnostic_visible_v4()?;
    if !actions.iter().all(|a|matches!(a,ActionSemanticV1::ChooseTarget{source,remaining,..}
        if *remaining==1 && crate::rl::card_name(source.card_db_id)=="Lightning Bolt")) {
        return Ok(json!({"status":"not_single_bolt_target"}));
    }
    let initial=s.game_state();
    if !initial.players[1-seat(root.acting_player) as usize].hand.is_empty() {return Ok(json!({"status":"opponent_hand_nonempty"}));}
    let mut outcomes=Vec::new();
    for i in 0..root.legal_action_count {
        let mut child=s.clone();child.step(root.episode_id,root.step,i).map_err(err)?;
        let mut remaining=NODES-1;
        let tree=explore(&child,initial,root.acting_player,DEPTH-1,&mut remaining)?;
        outcomes.push(json!({"index":i,"transitions":NODES-remaining,"tree":tree}));
    }
    ensure(s.current_response()==FastActorResponseV1::Decision(root),"burn tree mutated root")?;
    Ok(json!({"status":"audited","schema":"public-bolt-hand-galvanic-response-tree/v1","visible":visible,"actions":actions,
        "depth":DEPTH,"nodes_per_action":NODES,"outcomes":outcomes,
        "tree_encoding":"[lower,upper,reason_code,[[action_index,child],...]]",
        "reason_codes":["information_boundary","non_natural","natural_terminal","depth_limit","menu_limit","unsupported_action","node_limit","own_choice","opponent_choice","test"],
        "non_claim":"Shadow-only bound under public responses; unsupported and budget-limited branches stay unknown. No certified training dataset or playing-strength claim."}))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{self,Action,Decision};
    use crate::ids::PlayerId;
    use crate::mana::ManaColor;
    use crate::state::{GameState,Step,Zone};
    use super::super::public_combat::tests::put;
    #[test]
    fn hand_burn_tree_needs_hand_spell_and_payment() {
        for actor in 0..2 {
            for (hand,mana,expected) in [(true,2,1),(false,2,-1),(true,1,-1)] {
                let own=PlayerId(actor); let other=PlayerId(1-actor);
                let mut state=GameState::new_from_libraries(&[],&[],crate::rl::card_name,77102);
                state.step=Step::Main1;state.active_player=own;state.priority_player=own;
                state.players[other.index()].life=5;
                let bolt=put(&mut state,own,"Lightning Bolt",Zone::Hand);
                if hand {put(&mut state,own,"Galvanic Blast",Zone::Hand);}
                for player in [own,other] {
                    for name in ["Island","Forest","Mountain","Swamp"] {put(&mut state,player,name,Zone::Library);}
                }
                state.players[own.index()].mana_pool[ManaColor::R.pool_index()]=mana;
                engine::step(&mut state,Action::CastSpell(bolt)).unwrap();
                assert!(matches!(engine::advance_until_decision(&mut state),Decision::ChooseTargets{..}));
                let session=FastActorSessionV1::from_public_terminal_fixture_v1(state);
                let FastActorResponseV1::Decision(root)=session.current_response() else {panic!("root missing")};
                let (_,actions)=PairedBo1PolicyInputV1::new(&session,root).diagnostic_visible_v4().unwrap();
                let face=actions.iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..} if *player!=root.acting_player)).unwrap();
                let result=audit(&session,root).unwrap();
                assert_eq!(result["outcomes"][face]["tree"][0],expected,"hand={hand} mana={mana}");
                assert_eq!(session.current_response(),FastActorResponseV1::Decision(root));
            }
        }
    }
}
