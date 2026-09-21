//! Exactly one terminal-winner imitation update for engineering qualification.
//! No episode rewards or ordinary GAE checkpoint identity are rewritten.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub source:ExpandedModelSourceV1,
    pub dataset:PinnedFileV1,
    pub output_directory:PathBuf,
    pub target_permuted:bool,
    pub workers:usize,
}

fn cross_entropy(logits:&[f32],selected:usize)->f64 {
    let largest=logits.iter().copied().fold(f32::NEG_INFINITY,f32::max) as f64;
    largest+logits.iter().map(|v|(*v as f64-largest).exp()).sum::<f64>().ln()-logits[selected] as f64
}

pub fn run(command:Command)->Result<Value,String> {
    ensure(matches!(command.workers,1|4),"single-update worker count must be 1 or 4")?;
    let dataset:Value=serde_json::from_slice(&read_pinned_bytes(&command.dataset)?).map_err(err)?;
    ensure(dataset["schema"]=="public-terminal-teacher-data/v1","teacher data schema differs")?;
    ensure(dataset["source"]==serde_json::to_value(&command.source).map_err(err)?,"teacher source differs")?;
    let records=dataset["records"].as_array().ok_or("teacher records missing")?;
    ensure(records.len()==32,"single-update check requires exactly32 training records")?;
    let (mut policy,mut state)=initialize(&command.source)?;
    ensure(dataset["model"]==serde_json::to_value(policy.actual_model_identity_v1()).map_err(err)?,"teacher model identity differs")?;
    ensure(policy.feature_identity_v1().generation==FreshLineageGenerationV1::V4,"teacher requires V4")?;
    let before_state=hex(&state.state_sha256_v1().map_err(err)?);let initial_adam=state.adam_step_v1();
    let mut tensors=Vec::new();let mut targets=Vec::new();let mut before=Vec::new();let mut ids=BTreeSet::new();
    for row in records {
        ensure(row["spec"]["split"]=="train" && row["spec"]["no_win_control"]==false,"evaluation/targetless record cannot train")?;
        ensure(ids.insert(row["id"].as_str().ok_or("missing teacher case id")?.to_string()),"duplicate teacher case")?;
        let data=&row["data"];let winners=data["winning_indices"].as_array().ok_or("teacher winners missing")?;
        ensure(winners.len()==1,"single-update check requires one certified winner")?;
        let winner=winners[0].as_u64().ok_or("invalid winner index")? as usize;
        let outcomes=data["outcomes"].as_array().ok_or("teacher outcomes missing")?;
        let actor=data["visible"]["acting_player"].as_str().ok_or("teacher actor missing")?;
        ensure(winner<outcomes.len() && outcomes[winner]["classification"]=="win"
            && outcomes[winner]["terminal"]["terminal_classification"]=="natural"
            && outcomes[winner]["terminal"]["winner"]==actor,"teacher target lacks natural winning receipt")?;
        let tensor:TensorBitsV1=serde_json::from_value(data["tensor"].clone()).map_err(err)?;
        let tensor=NativeFlatDecisionTensorV4{common:tensor.tensor()};
        let scores=policy.score_training_tensor_v4(&tensor)?;
        ensure(scores.logits.len()==outcomes.len(),"teacher menu/tensor mismatch")?;
        let selected=if command.target_permuted {(winner+1)%scores.logits.len()} else {winner};
        targets.push(selected);
        before.push(json!({"id":row["id"],"selected":selected,"certified_winner":winner,"logits":bits(&scores.logits),
            "value_bits":scores.value.to_bits(),"target_cross_entropy":cross_entropy(&scores.logits,selected)}));
        tensors.push((tensor,bits(&scores.logits),scores.value.to_bits()));
    }
    let substeps:Vec<Vec<NativePolicySubstepV1<'_>>>=tensors.iter().zip(&targets).map(|((t,logits,value),&selected)|vec![NativePolicySubstepV1{
        forward:NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_generic_v1(&t.common,FreshLineageGenerationV1::V4))),
        selected_action_index:selected,expected_raw_action_logit_bits:logits,expected_value_bits:*value}]).collect();
    // This API's terminal_return field is diagnostic only for its supplied-
    // coefficient path. Zero is a placeholder for permuted controls, not a draw
    // label; no such physical_terms are exported as gameplay evidence.
    let groups:Vec<_>=substeps.iter().map(|substeps|NativePolicyPhysicalDecisionV1{substeps,
        terminal_return:if command.target_permuted {0} else {1},baseline_bits:0}).collect();
    let coefficients=vec![1f32;groups.len()];let unused_value_targets=vec![0f32;groups.len()];
    let ordinary_rejection=state.train_step_gae_feature_transfer_v4_fixed_partition_v1(&groups,&unused_value_targets,&coefficients,0.0,0.0001,command.workers);
    ensure(matches!(ordinary_rejection,Err(crate::native_policy_train_step_v1::NativePolicyTrainErrorV1::InvalidValueCoefficient)),"ordinary GAE zero-value guard changed")?;
    ensure(hex(&state.state_sha256_v1().map_err(err)?)==before_state,"rejected GAE update mutated optimizer")?;
    let update=state.train_step_terminal_winner_imitation_v4(&groups,0.0001,command.workers).map_err(err)?;
    let expected=before.iter().map(|r|r["target_cross_entropy"].as_f64().unwrap()).sum::<f64>()/before.len() as f64;
    ensure((update.loss as f64-expected).abs()<=1e-4+1e-5*expected.abs(),"imitation loss differs from direct cross entropy")?;
    let snapshot=state.snapshot_v1().map_err(err)?;
    ensure(snapshot.adam_step==initial_adam+1,"teacher update count differs")?;
    policy.replace_training_parameters_v3(&snapshot.parameters)?;
    let mut after=Vec::new();
    for (((t,_,_),&selected),row) in tensors.iter().zip(&targets).zip(records) {
        let scores=policy.score_training_tensor_v4(t)?;
        after.push(json!({"id":row["id"],"selected":selected,"logits":bits(&scores.logits),"value_bits":scores.value.to_bits(),
            "target_cross_entropy":cross_entropy(&scores.logits,selected)}));
    }
    let snapshot=json!({"schema":"public-terminal-teacher-checkpoint/v1","loss_identity":"certified-terminal-winner-cross-entropy/v1",
        "source":command.source,"dataset":command.dataset,"target_permuted":command.target_permuted,
        "adam_step":snapshot.adam_step,"scorer_bias_anchor_bits":snapshot.scorer_bias_anchor_bits,
        "parameters":snapshot.parameters.iter().map(ParameterBitsV1::from_native).collect::<Vec<_>>(),
        "first_moments":snapshot.first_moments.iter().map(ParameterBitsV1::from_native).collect::<Vec<_>>(),
        "second_moments":snapshot.second_moments.iter().map(ParameterBitsV1::from_native).collect::<Vec<_>>()});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory,"checkpoint.json",&snapshot)?;
    let result=json!({"schema":"public-terminal-teacher-single-update/v1","updates":1,"training_positions":32,"evaluation_positions_read":0,
        "workers":command.workers,"target_permuted":command.target_permuted,"before_state":before_state,
        "after_state":hex(&state.state_sha256_v1().map_err(err)?),"initial_adam":initial_adam,"final_adam":state.adam_step_v1(),
        "loss":update.loss,"direct_cross_entropy":expected,"before":before,"after":after,"model":policy.actual_model_identity_v1(),
        "optimizer":"Continue all g115 Adam moments and age; zero value-loss coefficient does not freeze moment-driven value parameters.",
        "non_claim":"One synthetic supervised engineering update, not on-policy GAE, gameplay rewards, a deployed checkpoint or strength evidence."});
    publish_json(&command.output_directory,"result.json",&result)?;Ok(result)
}
