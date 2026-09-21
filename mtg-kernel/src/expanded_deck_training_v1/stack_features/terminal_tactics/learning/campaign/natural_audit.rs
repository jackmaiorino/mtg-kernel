//! Read-only policy movement on archived actor-visible natural-game inputs.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    source: ExpandedModelSourceV1,
    checkpoint: PinnedFileV1,
    trajectories: Vec<PinnedFileV1>,
    output_directory: PathBuf,
}

fn top(logits: &[f32]) -> usize {
    let mut best = 0;
    for i in 1..logits.len() { if logits[i] > logits[best] { best = i; } }
    best
}

pub(super) fn run(c: Command) -> Result<Value, String> {
    ensure((1..=10).contains(&c.trajectories.len()), "natural audit archive bound differs")?;
    let (parent, parent_state) = initialize(&c.source)?;
    let (mut teacher, mut state) = initialize(&c.source)?;
    let saved = restore(&c.checkpoint, &c.source, &mut teacher, &mut state)?;
    ensure(saved.completed_updates == 32 && !saved.target_permuted,
        "natural audit requires the correct-label final endpoint")?;
    let parent_hash = hex(&parent_state.state_sha256_v1().map_err(err)?);
    let teacher_hash = hex(&state.state_sha256_v1().map_err(err)?);
    let mut sampler = WideCategoricalScratchV1::default();
    let mut archives = Vec::new();
    let mut ids = BTreeSet::new();
    for pin in &c.trajectories {
        let t: Trajectory = serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        ensure(t.schema == "mtg-kernel-public-stack-trajectory/v1"
            && t.decisions.len() == t.auxiliary.len() && !t.decisions.is_empty()
            && t.episode.learner_seat < 2 && ids.insert(t.episode.id.clone()),
            "natural audit archive identity differs")?;
        ensure(serde_json::to_value(&t.episode.opponent).map_err(err)? == serde_json::to_value(&c.source).map_err(err)?,
            "natural audit archived opponent is not the parent source")?;
        let mut rows = Vec::new();
        let mut exact = 0;
        for (index, row) in t.decisions.iter().enumerate() {
            ensure(row.actor < 2 && (row.selected as usize) < row.logits.len(), "invalid archive action")?;
            let tensor = NativeFlatDecisionTensorV4 { common: row.tensor.tensor() };
            let p = parent.score_training_tensor_v4(&tensor)?;
            let q = teacher.score_training_tensor_v4(&tensor)?;
            ensure(p.logits.len() == row.logits.len() && q.logits.len() == p.logits.len(),
                "natural audit menu differs")?;
            let is_parent_actor = row.actor != t.episode.learner_seat;
            if is_parent_actor {
                ensure(bits(&p.logits) == row.logits && p.value.to_bits() == row.value,
                    "natural audit parent behavior replay differs")?;
                exact += 1;
            }
            let pp: Vec<_> = sampler.apportion(&p.logits).map_err(err)?.iter()
                .map(|&m| m as f64 / 18446744073709551616.0).collect();
            let qp: Vec<_> = sampler.apportion(&q.logits).map_err(err)?.iter()
                .map(|&m| m as f64 / 18446744073709551616.0).collect();
            let tv = pp.iter().zip(&qp).map(|(a,b)| (a-b).abs()).sum::<f64>() * 0.5;
            let js = pp.iter().zip(&qp).map(|(&a,&b)| {
                let m = (a+b)*0.5;
                (if a>0.0 {a*(a/m).ln()} else {0.0})*0.5
                    +(if b>0.0 {b*(b/m).ln()} else {0.0})*0.5
            }).sum::<f64>().max(0.0);
            let width = crate::native_flat_tensorizer_v2::NATIVE_FLAT_ACTION_FEATURE_DIM_V2;
            ensure(row.tensor.action_features.len() == p.logits.len()*width, "action feature shape differs")?;
            let mut kinds = BTreeSet::new();
            for action in row.tensor.action_features.chunks_exact(width) {
                let one: Vec<_> = action[..27].iter().enumerate().filter(|(_,v)| **v == 1f32.to_bits()).map(|(i,_)|i).collect();
                ensure(one.len()==1 && action[..27].iter().all(|v| *v==0f32.to_bits() || *v==1f32.to_bits()),
                    "invalid archived action-kind one-hot")?;
                kinds.insert(one[0]);
            }
            let pt = top(&p.logits); let qt = top(&q.logits);
            rows.push(json!({"archive_row":index,"step":row.step,"physical_decision_id":row.physical_decision_id,
                "substep_index":row.substep_index,"substep_count":row.substep_count,"actor":row.actor,
                "deck":t.episode.selected[row.actor as usize].label,"parent_actor":is_parent_actor,
                "action_count":p.logits.len(),"action_kinds":kinds,"behavior_selected":row.selected,
                "parent_top":pt,"teacher_top":qt,"top_changed":pt!=qt,"total_variation":tv,"jensen_shannon":js,
                "parent_selected_probability":pp[row.selected as usize],"teacher_selected_probability":qp[row.selected as usize],
                "parent_probabilities":pp,"teacher_probabilities":qp,"parent_logits_bits":bits(&p.logits),
                "teacher_logits_bits":bits(&q.logits),"parent_value_bits":p.value.to_bits(),"teacher_value_bits":q.value.to_bits(),
                "absolute_value_change":(p.value as f64-q.value as f64).abs()}));
        }
        ensure(exact>0, "natural audit lacks parent behavior rows")?;
        archives.push(json!({"trajectory":pin,"episode_id":t.episode.id,"learner_seat":t.episode.learner_seat,
            "postboard":t.episode.postboard,"parent_behavior_rows":exact,"rows":rows}));
    }
    ensure(parent_hash == hex(&parent_state.state_sha256_v1().map_err(err)?)
        && teacher_hash == hex(&state.state_sha256_v1().map_err(err)?), "natural audit mutated optimizer")?;
    let result = json!({"schema":"terminal-teacher-natural-audit/v1","complete":true,
        "parent_state":parent_hash,"teacher_state":teacher_hash,"archives":archives,
        "updates":0,"new_games":0,"outcomes_exported":false,
        "non_claim":"Archived actor-visible input comparison only; policy movement is not improvement or loss of playing strength."});
    fs::create_dir(&c.output_directory).map_err(err)?;
    publish_json(&c.output_directory,"result.json",&result)?; Ok(result)
}
