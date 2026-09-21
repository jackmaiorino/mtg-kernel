//! Exact natural-trajectory reconstruction and narrow public terminal witnesses.
use super::*;
use crate::rl::ActionSemanticV1;
pub mod fixtures;

/// Shadow-only census. This does not select an action or expose a live session.
/// Restrict witnesses to visible burn targets and a publicly empty opposing hand.
pub(crate) fn audit_live_burn_targets_v1(session:&FastActorSessionV1,root:crate::rl_session::FastActorDecisionV1)->Result<Value,String> {
    ensure(session.current_response()==FastActorResponseV1::Decision(root),"shadow root binding differs")?;
    if !(2..=32).contains(&root.legal_action_count) {return Ok(json!({"status":"menu_outside_bounds"}));}
    let (visible,actions)=PairedBo1PolicyInputV1::new(session,root).diagnostic_visible_v4()?;
    let admitted=actions.iter().all(|a| match a {
        ActionSemanticV1::ChooseTarget{source,remaining,..}=>*remaining==1
            && matches!(crate::rl::card_name(source.card_db_id).as_str(),"Lightning Bolt"|"Lava Dart"|"Galvanic Blast"|"Fireblast"),
        _=>false,
    });
    if !admitted {return Ok(json!({"status":"not_supported_burn_target"}));}
    let actor=seat(root.acting_player) as usize;
    if !session.game_state().players[1-actor].hand.is_empty() {return Ok(json!({"status":"opponent_hand_nonempty"}));}
    let mut outcomes=Vec::new();
    for i in 0..root.legal_action_count {outcomes.push(witness(session,root,i)?);}
    ensure(session.current_response()==FastActorResponseV1::Decision(root),"shadow witness mutated root")?;
    Ok(json!({"status":"audited","visible":visible,"actions":actions,"outcomes":outcomes}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub source: ExpandedModelSourceV1,
    pub trajectories: Vec<PinnedFileV1>,
    pub output_directory: PathBuf,
}

fn probability(logits: &[f32]) -> Vec<f64> {
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    let p: Vec<_> = logits.iter().map(|v| (*v as f64 - maximum).exp()).collect();
    let sum: f64 = p.iter().sum();
    p.into_iter().map(|v| v / sum).collect()
}

fn unchanged_information(before: &crate::state::GameState, after: &crate::state::GameState) -> bool {
    before.turn == after.turn && before.step == after.step
        && before.library_knowledge == after.library_knowledge
        && (0..2).all(|i| before.players[i].library == after.players[i].library
            && before.players[i].draws_this_turn == after.players[i].draws_this_turn)
}

fn witness(session: &FastActorSessionV1, root: crate::rl_session::FastActorDecisionV1,
    index: u32) -> Result<Value, String> {
    let initial = session.game_state();
    let mut branch = session.clone();
    let mut current = root;
    let mut action = index;
    let mut line = Vec::new();
    for _ in 0..16 {
        line.push(json!({"step":current.step,"actor":current.acting_player,"index":action}));
        branch.step(current.episode_id,current.step,action).map_err(err)?;
        if !unchanged_information(initial, branch.game_state()) {
            return Ok(json!({"classification":"unresolved_information_boundary","line":line}));
        }
        match branch.current_response() {
            FastActorResponseV1::Terminal(t) => {
                if t.terminal_classification != TerminalClassificationV1::Natural {
                    return Ok(json!({"classification":"unresolved_non_natural","line":line}));
                }
                return Ok(json!({"classification":if t.winner==Some(root.acting_player) {"win"}
                    else if t.winner.is_some() {"loss"} else {"draw"},"line":line,"terminal":t}));
            }
            FastActorResponseV1::Decision(d) => {
                let (_, actions) = PairedBo1PolicyInputV1::new(&branch,d).diagnostic_visible_v4()?;
                let pass = actions.iter().position(|a| matches!(a,ActionSemanticV1::Pass{..}));
                if actions.len()==1 {
                    current=d;action=0;continue;
                }
                if pass.is_none() || d.acting_player != root.acting_player {
                    return Ok(json!({"classification":"unresolved_next_choice","line":line,
                        "next_actor":d.acting_player,"next_actions":actions}));
                }
                current=d;action=pass.unwrap() as u32;
            }
        }
    }
    Ok(json!({"classification":"unresolved_depth","line":line}))
}

pub fn run(command: Command) -> Result<Value, String> {
    ensure((1..=10).contains(&command.trajectories.len()),"tactic archive count outside bounds")?;
    let (mut policy,_) = initialize(&command.source)?;
    let identity=policy.actual_model_identity_v1();
    let mut roots=Vec::new();let mut archives=Vec::new();let mut total=0;let mut branches=0;
    for pin in &command.trajectories {
        let t: Trajectory=serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        ensure(t.schema=="mtg-kernel-public-stack-trajectory/v1","wrong tactic trajectory schema")?;
        let episode=&t.episode;let configs=episode.configurations()?;
        let mut session=FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            1,episode.seed,episode.max_physical_decisions,episode.max_policy_steps,
            episode.selected.each_ref().map(|d|d.label.clone()),configs.each_ref().map(|c|c.mainboard().to_vec()),
            PlayerId(episode.starting_player)).map_err(err)?;
        policy.reset_sampling_v1(paired_policy_seeds_v1(episode.seed));
        for row in &t.decisions {
            let FastActorResponseV1::Decision(d)=session.current_response() else {return Err("archive continues beyond replay terminal".into());};
            ensure(d.step==row.step && d.physical_decision_id==row.physical_decision_id
                && d.substep_index==row.substep_index && d.substep_count==row.substep_count
                && seat(d.acting_player)==row.actor && d.legal_action_count as usize==row.logits.len(),
                "tactic replay decision identity differs")?;
            let scores=policy.score_fast_session_v1(&session)?;
            let encoded=TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v4()?.common);
            ensure(serde_json::to_vec(&encoded).map_err(err)?==serde_json::to_vec(&row.tensor).map_err(err)?,
                "tactic engine replay tensor differs")?;
            total+=1;
            if row.actor==episode.learner_seat && (2..=32).contains(&d.legal_action_count)
                && session.game_state().players[1-row.actor as usize].hand.is_empty() {
                let (visible,actions)=PairedBo1PolicyInputV1::new(&session,d).diagnostic_visible_v4()?;
                let admitted=actions.iter().all(|a| match a {
                    ActionSemanticV1::ChooseTarget {source,remaining,..} => *remaining==1
                        && matches!(crate::rl::card_name(source.card_db_id).as_str(),"Lightning Bolt"|"Lava Dart"|"Galvanic Blast"|"Fireblast"),
                    _=>false,
                });
                if admitted {
                    let mut outcomes=Vec::new();
                    for index in 0..d.legal_action_count {
                        branches+=1;ensure(branches<=5000,"tactic branch cap exceeded")?;
                        outcomes.push(witness(&session,d,index)?);
                    }
                    let p=probability(&scores.logits);
                    let mass=|label:&str| -> f64 {p.iter().zip(&outcomes).filter(|(_,o)|o["classification"]==label).map(|(v,_)|v).sum()};
                    let mut best=0;for i in 1..scores.logits.len() {if scores.logits[i]>scores.logits[best] {best=i;}}
                    roots.push(json!({"trajectory":pin,"episode":episode.id,
                        "decision":{"step":d.step,"physical_decision_id":d.physical_decision_id,
                            "substep_index":d.substep_index,"actor":d.acting_player},"visible":visible,
                        "actions":actions,"logits":bits(&scores.logits),"probabilities":p,"argmax":best,
                        "winning_mass":mass("win"),"losing_mass":mass("loss"),"outcomes":outcomes}));
                }
            }
            session.step(d.episode_id,d.step,row.selected).map_err(err)?;
        }
        let FastActorResponseV1::Terminal(terminal)=session.current_response() else {return Err("archive ends before replay terminal".into());};
        ensure(terminal==t.terminal,"tactic replay terminal differs")?;
        archives.push(json!({"trajectory":pin,"exact_decisions":t.decisions.len(),"terminal_exact":true}));
    }
    let result=json!({"schema":"public-terminal-tactic-audit/v1","source":command.source,"model":identity,
        "archives":archives,"exact_decisions":total,"branches":branches,"roots":roots,
        "non_claim":"Narrow diagnostic public terminal lines, not a hidden-state search policy, whole-game optimality or strength benchmark."});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory,"result.json",&result)?;
    Ok(result)
}
