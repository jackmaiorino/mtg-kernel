//! Frozen three-arm retained-imitation continuation with native throughput guard.
use super::*;
use crate::native_policy_train_step_v1::retention_v1::{RetentionGroupV1, RetentionRowV1};

pub(super) const RETAINED_SCHEMA: &str = "terminal-retained-campaign/v1";
pub(super) const EQUAL_SCHEMA: &str = "terminal-equal-budget-continuation/v1";
pub(super) const EQUAL_LOSS: &str = "terminal-correct-ce-retention-fixed128/v1";

pub(super) const RETAINED_LOSS: &str = "terminal-ce-plus-parent-forward-kl-three-arm/v1";


#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Train {
    #[serde(default="dense_checkpoint_interval")]
    checkpoint_interval: usize,
    #[serde(default)]
    replay_reference: Option<PinnedFileV1>,
    arm: String,
    labels: PinnedFileV1,
    design: PinnedFileV1,
    qualification: Option<PinnedFileV1>,
    source: ExpandedModelSourceV1,
    dataset: PinnedFileV1,
    trajectory: Option<PinnedFileV1>,
    retention_dataset: Option<PinnedFileV1>,
    output_directory: PathBuf,
    workers: usize,
    end_update: usize,
    resume: Option<PinnedFileV1>,
    #[serde(default)]
    predecessor: Option<PinnedFileV1>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Saved {
    pub(super) arm: String,
    pub(super) labels: PinnedFileV1,
    pub(super) design: PinnedFileV1,
    pub(super) checkpoint: Checkpoint,
    pub(super) trajectory: Option<PinnedFileV1>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub(super) retention_dataset: Option<PinnedFileV1>,
    pub(super) selected_rows: Vec<Vec<usize>>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub(super) predecessor: Option<PinnedFileV1>,
}

fn dense_checkpoint_interval()->usize {1}

fn same<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(serde_json::to_value(a).map_err(err)? == serde_json::to_value(b).map_err(err)?)
}


fn canonical(value:&Value,actor:u8)->Value {
    match value {
        Value::Array(a)=>Value::Array(a.iter().map(|v|canonical(v,actor)).collect()),
        Value::Object(m)=>Value::Object(m.iter().map(|(k,v)|(k.clone(),canonical(v,actor))).collect()),
        Value::String(s) if s=="p0"||s=="p1"=>json!(if *s==format!("p{actor}") {"self"} else {"opponent"}),
        _=>value.clone(),
    }
}

pub(super) fn semantic_target(actions:&[Value],actor:u8,winner:usize)->Result<usize,String> {
    ensure(actions.len()>1 && winner<actions.len(),"invalid semantic menu")?;
    let keys=actions.iter().map(|v|serde_json::to_string(&canonical(v,actor)).map_err(err)).collect::<Result<Vec<_>,_>>()?;
    ensure(keys.iter().collect::<BTreeSet<_>>().len()==keys.len(),"ambiguous semantic menu")?;
    let mut order:Vec<_>=(0..keys.len()).collect();order.sort_by_key(|&i|&keys[i]);
    let index=order.iter().position(|&i|i==winner).ok_or("missing semantic winner")?;
    Ok(order[(index+1)%order.len()])
}

fn qualify(c:&Train,diagnostic:bool,budget:bool)->Result<(),String> {
    read_pinned_bytes(&c.design)?;
    if c.end_update<=if budget {34} else {2} {return Ok(());}
    let q:Value=serde_json::from_slice(&read_pinned_bytes(c.qualification.as_ref().ok_or("qualified retention throughput receipt required")?)?).map_err(err)?;
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(err)?.as_secs_f64();
    let at=q["checked_unix"].as_f64().ok_or("missing retention qualification time")?;
    let exe=std::env::current_exe().map_err(err)?;
    let schema=if budget && !diagnostic {"terminal-equal-budget-compute/v1"} else if budget {"semantic-budget-diagnostic-compute/v1"} else if diagnostic {"semantic-retention-diagnostic-compute/v1"} else {"retained-teacher-compute/v1"};
    ensure(q["schema"]==schema && q["complete"]==true && q["replay_verified"]==true && now>=at && now-at<86400.0
        && q["binary_sha256"]==sha(&fs::read(exe).map_err(err)?) && q["dataset_sha256"]==c.dataset.sha256
        && q["retention_sha256"]==c.retention_dataset.as_ref().unwrap().sha256 && q["labels_sha256"]==c.labels.sha256
        && q["checkpoint_interval"].as_u64().unwrap_or(1)==c.checkpoint_interval as u64
        && q["design_sha256"]==c.design.sha256 && q["source"]==serde_json::to_value(&c.source).map_err(err)?,"retention throughput receipt incompatible")?;
    let machine=std::env::var("COMPUTERNAME").map_err(err)?;
    ensure(q["allowed"].as_array().ok_or("missing retained allocation")?.iter().any(|v|v["machine"]==machine && v["workers"]==c.workers && v["arm"]==c.arm
        && (!budget || v["predecessor_sha256"]==c.predecessor.as_ref().unwrap().sha256)),"retained placement differs from qualified allocation")
}

pub(super) fn run(c: Train) -> Result<Value, String> {
    run_inner(c,false,false,false)
}

pub(super) fn diagnostic(c: Train) -> Result<Value, String> {
    run_inner(c,true,false,false)
}

pub(super) fn budget_diagnostic(c: Train) -> Result<Value, String> {
    run_inner(c,true,true,false)
}

pub(super) fn equal_budget(c: Train) -> Result<Value,String> {
    run_inner(c,false,true,false)
}

pub(super) fn publication_replay(c: Train) -> Result<Value,String> {
    run_inner(c,true,true,true)
}

fn run_inner(c: Train,diagnostic:bool,budget:bool,publication_replay:bool) -> Result<Value, String> {
    ensure(matches!(c.checkpoint_interval,1|16) && (budget||c.checkpoint_interval==1)
        && c.replay_reference.is_some()==publication_replay,"checkpoint publication contract differs")?;
    ensure(!publication_replay || (c.resume.is_some() && c.end_update==49),"publication replay requires fixed47/48-to49 state")?;
    let reference:Option<Saved>=c.replay_reference.as_ref().map(|p|serde_json::from_slice(&read_pinned_bytes(p)?).map_err(err)).transpose()?;
    let valid_arm=if budget && !diagnostic {matches!(c.arm.as_str(),"unretained"|"retained")} else if diagnostic {matches!(c.arm.as_str(),"semantic_unretained"|"semantic_retained")} else {matches!(c.arm.as_str(),"unretained"|"retained"|"semantic")};
    let valid_end=if publication_replay {c.end_update==49} else if budget {matches!(c.end_update,33|34|128)} else {(1..=2).contains(&c.end_update)||c.end_update==32};
    ensure(matches!(c.workers,1|4) && valid_arm && valid_end && c.predecessor.is_some()==budget,"retained campaign bounds differ")?;
    ensure(budget || !diagnostic || c.arm!="semantic_retained" || c.end_update<=2,"reuse the completed retained semantic baseline")?;
    ensure(c.trajectory.is_none() && c.retention_dataset.is_some(),"retained campaign requires pinned full retention data")?;
    let beta=if matches!(c.arm.as_str(),"unretained"|"semantic_unretained") {0.0} else {0.5};
    let control=diagnostic || c.arm=="semantic";
    let schema=if budget && !diagnostic {EQUAL_SCHEMA} else if budget {"terminal-semantic-budget-diagnostic/v1"} else if diagnostic {"terminal-semantic-retention-diagnostic/v1"} else {RETAINED_SCHEMA};
    let loss=if budget && !diagnostic {EQUAL_LOSS} else if budget {"terminal-semantic-ce-fixed128-budget/v1"} else if diagnostic {"terminal-semantic-ce-retention-ablation/v1"} else {RETAINED_LOSS};
    if publication_replay {read_pinned_bytes(&c.design)?;} else {qualify(&c,diagnostic,budget)?;}
    let labels:Value=serde_json::from_slice(&read_pinned_bytes(&c.labels)?).map_err(err)?;
    ensure(labels["schema"]=="terminal-semantic-teacher-labels/v1" && labels["training"]==serde_json::to_value(&c.dataset).map_err(err)? && labels["parent_source"]==serde_json::to_value(&c.source).map_err(err)?,"retained labels identity differs")?;
    let labels=labels["records"].as_array().ok_or("missing semantic labels")?;
    ensure(labels.len()==32,"retained labels cardinality differs")?;
    let mut label_ids=BTreeSet::new();
    ensure(labels.iter().all(|r|r["id"].as_str().is_some_and(|id|label_ids.insert(id))),"duplicate semantic labels")?;
    let data: Value = serde_json::from_slice(&read_pinned_bytes(&c.dataset)?).map_err(err)?;
    ensure(data["schema"] == "public-terminal-teacher-data/v1" && data["source"] == serde_json::to_value(&c.source).map_err(err)?, "retained teacher identity differs")?;
    let rows = data["records"].as_array().ok_or("missing retained teacher rows")?;
    ensure(rows.len() == 32, "retained engineering requires32 training rows")?;
    let (parent, original) = initialize(&c.source)?;
    ensure(data["model"] == serde_json::to_value(parent.actual_model_identity_v1()).map_err(err)? && parent.feature_identity_v1().generation == FreshLineageGenerationV1::V4, "retained teacher model differs")?;
    let initial_adam = original.adam_step_v1();
    let original_hash = hex(&original.state_sha256_v1().map_err(err)?);
    let mut teaching = Vec::new(); let mut targets = Vec::new(); let mut teacher_tensors = BTreeSet::new(); let mut ids = BTreeSet::new();
    for row in rows {
        ensure(row["spec"]["split"] == "train" && row["spec"]["no_win_control"] == false && ids.insert(row["id"].as_str().ok_or("missing teacher id")?), "retained engineering rejects heldout/duplicate/targetless teacher")?;
        let winners = row["data"]["winning_indices"].as_array().ok_or("missing winners")?;
        ensure(winners.len() == 1, "retained teacher needs one winner")?;
        let winner = winners[0].as_u64().ok_or("invalid winner")? as usize;
        let outcomes = row["data"]["outcomes"].as_array().ok_or("missing outcomes")?;
        ensure(winner < outcomes.len() && outcomes[winner]["classification"] == "win" && outcomes[winner]["terminal"]["terminal_classification"] == "natural" && outcomes[winner]["terminal"]["winner"] == row["data"]["visible"]["acting_player"], "retained target lacks natural winning witness")?;
        let t: TensorBitsV1 = serde_json::from_value(row["data"]["tensor"].clone()).map_err(err)?;
        teacher_tensors.insert(sha(&serde_json::to_vec(&t).map_err(err)?));
        let t = NativeFlatDecisionTensorV4 { common: t.tensor() };
        ensure(parent.score_training_tensor_v4(&t)?.logits.len() == outcomes.len(), "retained teacher menu differs")?;
        let label=labels.iter().find(|l|l["id"]==row["id"]).ok_or("missing semantic label")?;
        let actor=row["spec"]["actor"].as_u64().ok_or("missing teacher actor")?;
        ensure(actor<2,"invalid teacher actor")?;
        let actions=row["data"]["actions"].as_array().ok_or("missing teacher actions")?;
        let semantic=semantic_target(actions,actor as u8,winner)?;
        ensure(label["winner"]==winner && label["control"]==semantic,"semantic label mapping differs")?;
        teaching.push(t); targets.push(if control {semantic} else {winner});
    }
    let loaded=super::retention_data::load(&c.source,c.retention_dataset.as_ref().unwrap(),&c.dataset,&parent)?;
    let (retained,selected)=(loaded.groups,loaded.selected_rows);
    let parent_score_replay_rows=retained.iter().map(Vec::len).sum::<usize>();
    let mut state = original.clone(); let mut completed = 0;
    let mut predecessor_fit=Vec::new();
    if let Some(pin) = c.resume.as_ref().or(c.predecessor.as_ref()) {
        let saved: Saved = serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        let s = &saved.checkpoint;
        let root=budget && c.resume.is_none();
        let identity=if root {
            read_pinned_bytes(&saved.design)?;
            let old=if !diagnostic {(c.arm.as_str(),RETAINED_SCHEMA,RETAINED_LOSS)} else if beta==0.0 {("semantic_unretained","terminal-semantic-retention-diagnostic/v1","terminal-semantic-ce-retention-ablation/v1")}
                else {("semantic",RETAINED_SCHEMA,RETAINED_LOSS)};
            saved.arm==old.0 && s.schema==old.1 && s.loss_identity==old.2 && saved.predecessor.is_none() && s.completed_updates==32
        } else {
            saved.arm==c.arm && same(&saved.design,&c.design)? && s.schema==schema && s.loss_identity==loss
                && same(&saved.predecessor,&c.predecessor)? && s.completed_updates>if budget {32} else {0}
                && s.completed_updates<if budget {128} else {32}
        };
        ensure(identity && same(&saved.labels,&c.labels)? && same(&s.source, &c.source)? && same(&s.dataset, &c.dataset)? && same(&saved.trajectory, &c.trajectory)? && same(&saved.retention_dataset,&c.retention_dataset)? && saved.selected_rows == selected && s.target_permuted==control && s.backend == "cpu" && s.device_ordinal == 0 && s.initial_adam == initial_adam && s.adam_step == initial_adam + s.completed_updates as u64, "retained resume contract differs")?;
        let template = state.snapshot_v1().map_err(err)?;
        let snapshot = NativePolicyValueTrainSnapshotV1 { parameters: restore_parameters(&s.parameters, &template.parameters)?, first_moments: restore_parameters(&s.first_moments, &template.first_moments)?, second_moments: restore_parameters(&s.second_moments, &template.second_moments)?, adam_step: s.adam_step, scorer_bias_anchor_bits: s.scorer_bias_anchor_bits };
        ensure(hex(&snapshot.state_sha256_v1().map_err(err)?) == s.state_sha256, "retained resume state hash differs")?;
        state = NativePolicyValueTrainStateV1::from_snapshot_v1(state.model_v1().clone(), &snapshot).map_err(err)?;
        completed = s.completed_updates;
        ensure(!publication_replay || matches!(completed,47|48),"publication replay rejects an unbounded prefix")?;
    }
    ensure(completed < c.end_update, "retained endpoint already reached")?;
    let mut policy = parent; policy.replace_training_parameters_v3(&state.snapshot_v1().map_err(err)?.parameters)?;
    let start_update=completed;let start_state=hex(&state.state_sha256_v1().map_err(err)?);
    if budget && c.resume.is_none() {
        for (i,t) in teaching.iter().enumerate() {
            let s=policy.score_training_tensor_v4(t)?;
            predecessor_fit.push(json!({"id":rows[i]["id"],"logits_bits":bits(&s.logits),"value_bits":s.value.to_bits()}));
        }
    }
    fs::create_dir(&c.output_directory).map_err(err)?;
    let started = std::time::Instant::now(); let mut updates = Vec::new();let mut timings=Vec::new();let mut stages=Vec::new();let mut published=Vec::new();
    while completed < c.end_update {
        let tick=std::time::Instant::now();
        let scores = teaching.iter().map(|t|policy.score_training_tensor_v4(t)).collect::<Result<Vec<_>,_>>()?;
        let logits: Vec<_> = scores.iter().map(|s| bits(&s.logits)).collect();
        let steps: Vec<Vec<_>> = teaching.iter().enumerate().map(|(i,t)| vec![NativePolicySubstepV1 { forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_generic_v1(&t.common, FreshLineageGenerationV1::V4))), selected_action_index: targets[i], expected_raw_action_logit_bits: &logits[i], expected_value_bits: scores[i].value.to_bits() }]).collect();
        let groups: Vec<_> = steps.iter().map(|substeps|NativePolicyPhysicalDecisionV1 { substeps, terminal_return:if control {0} else {1}, baseline_bits:0 }).collect();
        let current = if beta==0.0 {Vec::new()} else {retained.iter().map(|group| group.iter().map(|(t,_)|policy.score_training_tensor_v4(t)).collect::<Result<Vec<_>,_>>()).collect::<Result<Vec<_>,_>>()?};
        let replay: Vec<Vec<_>> = current.iter().map(|group|group.iter().map(|s|bits(&s.logits)).collect()).collect();
        let retention_rows: Vec<Vec<_>> = retained.iter().take(current.len()).enumerate().map(|(g,group)|group.iter().enumerate().map(|(r,(t,p))|RetentionRowV1 { encoded:encoded_decision_view_generic_v1(&t.common,FreshLineageGenerationV1::V4), expected_current_logits:&replay[g][r], expected_current_value:current[g][r].value.to_bits(), parent_logits:p }).collect()).collect();
        let retention_groups: Vec<_> = retention_rows.iter().map(|rows|RetentionGroupV1 { rows }).collect();
        let prepared=tick.elapsed().as_secs_f64();let update_tick=std::time::Instant::now();
        let result = state.train_step_retained_imitation_v4(&groups, &retention_groups, beta, 0.0001, c.workers)?;
        let direct_ce = scores.iter().zip(&targets).map(|(s,&i)|cross_entropy(&s.logits,i)).sum::<f64>()/32.0;
        ensure((result.teacher_loss as f64-direct_ce).abs() < 1e-4+1e-5*direct_ce.abs(), "retained direct teacher loss differs")?;
        if completed == 0 {
            ensure(result.retention_kl == 0.0 && result.weighted_retention_loss == 0.0, "identical parent must start at zero KL")?;
            let mut legacy = original.clone(); legacy.train_step_terminal_winner_imitation_v4(&groups,0.0001,c.workers).map_err(err)?;
            ensure(legacy.snapshot_v1().map_err(err)? == state.snapshot_v1().map_err(err)?, "initial zero-KL update differs from teacher")?;
        }
        let updated=update_tick.elapsed().as_secs_f64();let publication_tick=std::time::Instant::now();
        completed += 1; let snapshot = state.snapshot_v1().map_err(err)?;
        ensure(snapshot.adam_step == initial_adam + completed as u64, "retained Adam age differs")?;
        policy.replace_training_parameters_v3(&snapshot.parameters)?;
        let state_hash=hex(&snapshot.state_sha256_v1().map_err(err)?);
        let publish=completed%c.checkpoint_interval==0 || completed==c.end_update;
        if publish {
        let saved = Saved { arm:c.arm.clone(),labels:c.labels.clone(),design:c.design.clone(),trajectory:c.trajectory.clone(), retention_dataset:c.retention_dataset.clone(), selected_rows:selected.clone(), predecessor:c.predecessor.clone(), checkpoint: Checkpoint { schema:schema.into(), loss_identity:loss.into(), source:c.source.clone(), dataset:c.dataset.clone(), target_permuted:control, backend:"cpu".into(), device_ordinal:0, completed_updates:completed, initial_adam, adam_step:snapshot.adam_step, scorer_bias_anchor_bits:snapshot.scorer_bias_anchor_bits, state_sha256:state_hash.clone(), parameters:snapshot.parameters.iter().map(ParameterBitsV1::from_native).collect(), first_moments:snapshot.first_moments.iter().map(ParameterBitsV1::from_native).collect(), second_moments:snapshot.second_moments.iter().map(ParameterBitsV1::from_native).collect() }};
        if publication_replay && completed==c.end_update {
            ensure(same(&saved,reference.as_ref().unwrap())?,"publication replay differs from frozen reference")?;
        }
        publish_json(&c.output_directory,&format!("checkpoint-{completed:03}.json"),&saved)?;
        published.push(completed);
        }
        stages.push(json!({"update":completed,"forward_preparation_seconds":prepared,"update_and_checks_seconds":updated,
            "snapshot_and_publication_seconds":publication_tick.elapsed().as_secs_f64(),"published":publish}));
        timings.push(tick.elapsed().as_secs_f64());
        updates.push(json!({"update":completed,"teacher_loss":result.teacher_loss,"retention_kl":result.retention_kl,"weighted_retention_loss":result.weighted_retention_loss,"loss":result.loss,"state_sha256":state_hash}));
    }
    ensure(original_hash == hex(&original.state_sha256_v1().map_err(err)?), "retained original mutated")?;
    let mut result = json!({"schema":schema,"complete":true,"initial_state":original_hash,"final_state":hex(&state.state_sha256_v1().map_err(err)?),"initial_adam":initial_adam,"final_adam":state.adam_step_v1(),"completed_updates":completed,"updates":updates,"selected_rows":selected,"arm":c.arm,"beta":beta,"teacher_positions":32,"retention_physical_groups":retained.len(),"retention_rows":retained.iter().map(Vec::len).sum::<usize>(),"evaluation_positions_read":0,"non_claim":"Frozen terminal imitation continuation; completed training is not playing-strength evidence."});
    if diagnostic {
        let mut fit=Vec::new();let mut sampler=WideCategoricalScratchV1::default();
        for (i,t) in teaching.iter().enumerate() {
            let s=policy.score_training_tensor_v4(t)?;
            let mut best=0;for j in 1..s.logits.len(){if s.logits[j]>s.logits[best]{best=j;}}
            let masses=sampler.apportion(&s.logits).map_err(err)?;
            ensure(masses.iter().sum::<u128>()==(1u128<<64),"diagnostic probability mass differs")?;
            fit.push(json!({"id":rows[i]["id"],"spec":rows[i]["spec"],"target":targets[i],"argmax":best,"correct":best==targets[i],
                "cross_entropy":cross_entropy(&s.logits,targets[i]),"logits_bits":bits(&s.logits),"value_bits":s.value.to_bits(),
                "probabilities":masses.iter().map(|&m|m as f64/18446744073709551616.0).collect::<Vec<_>>()}));
        }
        result["training_fit"]=json!(fit);
        result["non_claim"]=json!("Training-only semantic-control optimization diagnostic; no validation, candidate promotion or playing-strength claim.");
    }
    if budget {
        result["start_update"]=json!(start_update);result["start_state"]=json!(start_state);
        result["predecessor"]=json!(c.predecessor);result["predecessor_fit"]=json!(predecessor_fit);
    }
    if let Some(pin)=&c.retention_dataset {
        result["retention_dataset"]=json!(pin);result["parent_score_replay_rows"]=json!(parent_score_replay_rows);
    }
    if c.checkpoint_interval!=1 {
        result["checkpoint_interval"]=json!(c.checkpoint_interval);result["checkpoint_updates"]=json!(published);
    }
    publish_json(&c.output_directory,"result.json",&result)?;
    publish_json(&c.output_directory,"timing.json",&json!({"seconds":started.elapsed().as_secs_f64(),"workers":c.workers,"updates":timings,"stages":stages}))?;
    Ok(result)
}
