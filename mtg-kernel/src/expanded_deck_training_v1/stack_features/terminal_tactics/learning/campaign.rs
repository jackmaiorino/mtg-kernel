//! Guarded bounded imitation continuation and final-only reserved evaluation.
use super::*;
mod training_diagnostic;
mod natural_audit;
mod retained_engineering;
mod retention_data;
mod retained_campaign;
mod retained_evaluation;

#[derive(Deserialize)]
#[serde(tag="mode",rename_all="snake_case",deny_unknown_fields)]
pub enum Command { Train(Train), Evaluate(Evaluate), TrainingDiagnostic(Evaluate), NaturalAudit(natural_audit::Command), RetainedEngineering(retained_engineering::Command), RetentionDataCheck(retention_data::Command), RetainedTrain(retained_campaign::Train), RetainedEvaluate(retained_evaluation::Fixtures), RetainedNatural(retained_evaluation::Natural), SemanticDiagnostic(retained_campaign::Train) }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Train {
    source:ExpandedModelSourceV1,dataset:PinnedFileV1,output_directory:PathBuf,
    target_permuted:bool,workers:usize,backend:String,device_ordinal:usize,
    end_update:usize,resume:Option<PinnedFileV1>,qualification:Option<PinnedFileV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluate {
    source:ExpandedModelSourceV1,dataset:PinnedFileV1,checkpoints:Vec<PinnedFileV1>,output_directory:PathBuf,
}

#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema:String,loss_identity:String,source:ExpandedModelSourceV1,dataset:PinnedFileV1,
    target_permuted:bool,backend:String,device_ordinal:usize,completed_updates:usize,initial_adam:u64,
    adam_step:u64,scorer_bias_anchor_bits:u32,state_sha256:String,
    parameters:Vec<ParameterBitsV1>,first_moments:Vec<ParameterBitsV1>,second_moments:Vec<ParameterBitsV1>,
}

const SCHEMA:&str="public-terminal-teacher-continuation/v1";
const LOSS:&str="certified-terminal-winner-cross-entropy/v1";

fn restore(pin:&PinnedFileV1,source:&ExpandedModelSourceV1,policy:&mut FrozenPlayPolicyV1,state:&mut NativePolicyValueTrainStateV1)->Result<Checkpoint,String> {
    let saved:Checkpoint=serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
    ensure(saved.schema==SCHEMA && saved.loss_identity==LOSS && serde_json::to_value(&saved.source).map_err(err)?==serde_json::to_value(source).map_err(err)?,"teacher checkpoint identity differs")?;
    ensure(saved.completed_updates<=32 && saved.adam_step==saved.initial_adam+saved.completed_updates as u64,"teacher checkpoint age differs")?;
    let template=state.snapshot_v1().map_err(err)?;
    let snapshot=NativePolicyValueTrainSnapshotV1{
        parameters:restore_parameters(&saved.parameters,&template.parameters)?,
        first_moments:restore_parameters(&saved.first_moments,&template.first_moments)?,
        second_moments:restore_parameters(&saved.second_moments,&template.second_moments)?,
        adam_step:saved.adam_step,scorer_bias_anchor_bits:saved.scorer_bias_anchor_bits};
    ensure(hex(&snapshot.state_sha256_v1().map_err(err)?)==saved.state_sha256,"teacher checkpoint state hash differs")?;
    *state=NativePolicyValueTrainStateV1::from_snapshot_v1(state.model_v1().clone(),&snapshot).map_err(err)?;
    policy.replace_training_parameters_v3(&snapshot.parameters)?;Ok(saved)
}

fn qualify(c:&Train)->Result<(),String> {
    if c.end_update<=4 {return Ok(());}
    ensure(c.end_update==32,"formal teacher endpoint must be32")?;
    let q:Value=serde_json::from_slice(&read_pinned_bytes(c.qualification.as_ref().ok_or("qualified throughput receipt required")?)?).map_err(err)?;
    let binary=std::env::current_exe().map_err(err)?;
    ensure(q["schema"]=="terminal-teacher-compute/v1" && q["complete"]==true && q["replay_verified"]==true
        && q["binary_sha256"]==sha(&fs::read(binary).map_err(err)?)
        && q["dataset_sha256"]==c.dataset.sha256 && q["source_sha256"]==sha(&serde_json::to_vec(&c.source).map_err(err)?),"teacher throughput receipt incompatible")?;
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(err)?.as_secs_f64();
    let checked=q["checked_unix"].as_f64().ok_or("missing compute timestamp")?;
    ensure(now>=checked && now-checked<86400.0,"teacher compute receipt stale")?;
    let machine=std::env::var("COMPUTERNAME").map_err(err)?;
    ensure(q["allowed"].as_array().ok_or("missing qualified allocation")?.iter().any(|a|
        a["machine"]==machine && a["backend"]==c.backend && a["workers"]==c.workers && a["device_ordinal"]==c.device_ordinal),"teacher placement differs from qualified allocation")?;
    Ok(())
}

fn train(c:Train)->Result<Value,String> {
    ensure(matches!(c.workers,1|4) && matches!(c.backend.as_str(),"cpu"|"cuda") && c.device_ordinal<2 && (1..=32).contains(&c.end_update),"teacher execution bounds differ")?;
    qualify(&c)?;
    let data:Value=serde_json::from_slice(&read_pinned_bytes(&c.dataset)?).map_err(err)?;
    ensure(data["schema"]=="public-terminal-teacher-data/v1" && data["source"]==serde_json::to_value(&c.source).map_err(err)?,"teacher dataset identity differs")?;
    let rows=data["records"].as_array().ok_or("teacher records missing")?;
    ensure(rows.len()==32,"teacher requires32 training cases")?;
    let (mut policy,mut state)=initialize(&c.source)?;
    ensure(data["model"]==serde_json::to_value(policy.actual_model_identity_v1()).map_err(err)?,"teacher parent model differs")?;
    let initial_adam=state.adam_step_v1();let mut completed=0;
    if let Some(pin)=&c.resume {
        let saved=restore(pin,&c.source,&mut policy,&mut state)?;
        ensure(saved.dataset.sha256==c.dataset.sha256 && saved.target_permuted==c.target_permuted
            && saved.backend==c.backend && saved.device_ordinal==c.device_ordinal && saved.initial_adam==initial_adam,"teacher resume contract differs")?;
        completed=saved.completed_updates;
    }
    ensure(completed<c.end_update,"teacher endpoint already reached")?;
    let mut tensors=Vec::new();let mut targets=Vec::new();let mut ids=BTreeSet::new();
    for row in rows {
        ensure(row["spec"]["split"]=="train" && row["spec"]["no_win_control"]==false && ids.insert(row["id"].as_str().ok_or("teacher id missing")?),"heldout, targetless or duplicate teacher row")?;
        let winners=row["data"]["winning_indices"].as_array().ok_or("teacher winners missing")?;
        ensure(winners.len()==1,"teacher requires one certified winner")?;
        let winner=winners[0].as_u64().ok_or("invalid teacher winner")? as usize;
        let outcomes=row["data"]["outcomes"].as_array().ok_or("teacher outcomes missing")?;
        ensure(winner<outcomes.len() && outcomes[winner]["classification"]=="win"
            && outcomes[winner]["terminal"]["terminal_classification"]=="natural"
            && outcomes[winner]["terminal"]["winner"]==row["data"]["visible"]["acting_player"],"teacher target is not a natural win")?;
        let tensor:TensorBitsV1=serde_json::from_value(row["data"]["tensor"].clone()).map_err(err)?;
        let tensor=NativeFlatDecisionTensorV4{common:tensor.tensor()};
        let width=policy.score_training_tensor_v4(&tensor)?.logits.len();ensure(width==outcomes.len(),"teacher menu differs")?;
        targets.push(if c.target_permuted {(winner+1)%width} else {winner});tensors.push(tensor);
    }
    fs::create_dir(&c.output_directory).map_err(err)?;
    let started=std::time::Instant::now();let mut timings=Vec::new();let mut losses=Vec::new();
    while completed<c.end_update {
        let tick=std::time::Instant::now();
        let scores:Vec<_>=tensors.iter().map(|t|policy.score_training_tensor_v4(t)).collect::<Result<_,_>>()?;
        let bits:Vec<_>=scores.iter().map(|s|super::bits(&s.logits)).collect();
        let steps:Vec<Vec<_>>=tensors.iter().enumerate().map(|(i,t)|vec![NativePolicySubstepV1{
            forward:NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_generic_v1(&t.common,FreshLineageGenerationV1::V4))),
            selected_action_index:targets[i],expected_raw_action_logit_bits:&bits[i],expected_value_bits:scores[i].value.to_bits()}]).collect();
        let groups:Vec<_>=steps.iter().map(|substeps|NativePolicyPhysicalDecisionV1{substeps,terminal_return:if c.target_permuted {0} else {1},baseline_bits:0}).collect();
        let update=if c.backend=="cpu" {state.train_step_terminal_winner_imitation_v4(&groups,0.0001,c.workers).map_err(err)?}
            else {crate::experimental_burn_net8_packed_v1::bridge::train_step_cuda_terminal_imitation_v4(&mut state,&groups,0.0001,c.device_ordinal).map_err(err)?};
        let expected=scores.iter().zip(&targets).map(|(s,&i)|cross_entropy(&s.logits,i)).sum::<f64>()/32.0;
        ensure((update.loss as f64-expected).abs()<=1e-4+1e-5*expected.abs(),"teacher objective differs from direct cross entropy")?;
        let snapshot=state.snapshot_v1().map_err(err)?;policy.replace_training_parameters_v3(&snapshot.parameters)?;
        completed+=1;ensure(snapshot.adam_step==initial_adam+completed as u64,"teacher Adam continuation differs")?;
        let saved=Checkpoint{schema:SCHEMA.into(),loss_identity:LOSS.into(),source:c.source.clone(),dataset:c.dataset.clone(),
            target_permuted:c.target_permuted,backend:c.backend.clone(),device_ordinal:c.device_ordinal,completed_updates:completed,initial_adam,
            adam_step:snapshot.adam_step,scorer_bias_anchor_bits:snapshot.scorer_bias_anchor_bits,state_sha256:hex(&snapshot.state_sha256_v1().map_err(err)?),
            parameters:snapshot.parameters.iter().map(ParameterBitsV1::from_native).collect(),first_moments:snapshot.first_moments.iter().map(ParameterBitsV1::from_native).collect(),
            second_moments:snapshot.second_moments.iter().map(ParameterBitsV1::from_native).collect()};
        publish_json(&c.output_directory,&format!("checkpoint-{completed:03}.json"),&saved)?;
        losses.push(update.loss);timings.push(tick.elapsed().as_secs_f64());
    }
    let result=json!({"schema":"terminal-teacher-training/v1","complete":true,"completed_updates":completed,"adam_step":state.adam_step_v1(),
        "state_sha256":hex(&state.state_sha256_v1().map_err(err)?),"model":policy.actual_model_identity_v1(),"losses":losses,
        "backend":c.backend,"workers":c.workers,"target_permuted":c.target_permuted,"evaluation_positions_read":0});
    publish_json(&c.output_directory,"result.json",&result)?;
    publish_json(&c.output_directory,"timing.json",&json!({"seconds":started.elapsed().as_secs_f64(),"updates":timings}))?;Ok(result)
}

fn evaluate(c:Evaluate)->Result<Value,String> {
    ensure(c.checkpoints.len()==2,"final evaluation needs both endpoints")?;
    let data:Value=serde_json::from_slice(&read_pinned_bytes(&c.dataset)?).map_err(err)?;
    ensure(data["schema"]=="public-terminal-teacher-data/v1" && data["source"]==serde_json::to_value(&c.source).map_err(err)?,"evaluation dataset identity differs")?;
    let rows=data["records"].as_array().ok_or("evaluation rows missing")?;ensure(rows.len()==24,"evaluation needs24 positions")?;
    let mut models=Vec::new();let mut arms=BTreeSet::new();
    for checkpoint in std::iter::once(None).chain(c.checkpoints.iter().map(Some)) {
        let (mut policy,mut state)=initialize(&c.source)?;
        let label=if let Some(pin)=checkpoint {
            let saved=restore(pin,&c.source,&mut policy,&mut state)?;
            ensure(saved.completed_updates==32 && arms.insert(saved.target_permuted),"incomplete or duplicate teacher endpoint")?;
            if saved.target_permuted {"permuted"} else {"teacher"}
        } else {"g115"};
        let mut results=Vec::new();
        for row in rows {
            ensure(row["spec"]["split"]=="evaluation","training row in reserved evaluation")?;
            let t:TensorBitsV1=serde_json::from_value(row["data"]["tensor"].clone()).map_err(err)?;
            let s=policy.score_training_tensor_v4(&NativeFlatDecisionTensorV4{common:t.tensor()})?;
            let probabilities=probability(&s.logits);let mut best=0;for i in 1..s.logits.len(){if s.logits[i]>s.logits[best]{best=i;}}
            let winners:Vec<usize>=serde_json::from_value(row["data"]["winning_indices"].clone()).map_err(err)?;
            results.push(json!({"id":row["id"],"spec":row["spec"],"argmax":best,"winning_indices":winners,
                "winning_probability":winners.iter().map(|&i|probabilities[i]).sum::<f64>(),"argmax_wins":winners.contains(&best),
                "logits":bits(&s.logits),"probabilities":probabilities,"value_bits":s.value.to_bits()}));
        }
        models.push(json!({"label":label,"checkpoint":checkpoint,"model":policy.actual_model_identity_v1(),"records":results}));
    }
    let result=json!({"schema":"terminal-teacher-reserved-evaluation/v1","complete":true,"positions":24,"models":models,"dataset":c.dataset});
    fs::create_dir(&c.output_directory).map_err(err)?;publish_json(&c.output_directory,"result.json",&result)?;Ok(result)
}

pub fn run(command:Command)->Result<Value,String> {match command {Command::Train(c)=>train(c),Command::Evaluate(c)=>evaluate(c),Command::TrainingDiagnostic(c)=>training_diagnostic::run(c),Command::NaturalAudit(c)=>natural_audit::run(c),Command::RetainedEngineering(c)=>retained_engineering::run(c),Command::RetentionDataCheck(c)=>retention_data::run(c),Command::RetainedTrain(c)=>retained_campaign::run(c),Command::RetainedEvaluate(c)=>retained_evaluation::fixtures(c),Command::RetainedNatural(c)=>retained_evaluation::natural(c),Command::SemanticDiagnostic(c)=>retained_campaign::diagnostic(c)}}
