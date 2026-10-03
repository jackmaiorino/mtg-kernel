//! Read-only final-endpoint training-fit audit. No heldout input or updates.
use super::*;

pub(super) fn run(c: Evaluate) -> Result<Value, String> {
    ensure(
        c.checkpoints.len() == 2,
        "training diagnostic needs both final endpoints",
    )?;
    let data: Value = serde_json::from_slice(&read_pinned_bytes(&c.dataset)?).map_err(err)?;
    ensure(
        data["schema"] == "public-terminal-teacher-data/v1"
            && data["source"] == serde_json::to_value(&c.source).map_err(err)?,
        "training diagnostic dataset identity differs",
    )?;
    let rows = data["records"]
        .as_array()
        .ok_or("training diagnostic records missing")?;
    ensure(
        rows.len() == 32,
        "training diagnostic requires32 training positions",
    )?;
    let mut ids = BTreeSet::new();
    // Validate the complete split before loading or scoring any model.
    for row in rows {
        ensure(
            row["spec"]["split"] == "train"
                && row["spec"]["no_win_control"] == false
                && ids.insert(row["id"].as_str().ok_or("training diagnostic id missing")?),
            "training diagnostic rejects heldout, targetless or duplicate rows",
        )?;
        let winners = row["data"]["winning_indices"]
            .as_array()
            .ok_or("missing winning indices")?;
        ensure(
            winners.len() == 1,
            "training diagnostic requires one certified winner",
        )?;
        let winner = winners[0].as_u64().ok_or("invalid winning index")? as usize;
        let outcomes = row["data"]["outcomes"]
            .as_array()
            .ok_or("missing outcomes")?;
        ensure(
            winner < outcomes.len()
                && outcomes[winner]["classification"] == "win"
                && outcomes[winner]["terminal"]["terminal_classification"] == "natural"
                && outcomes[winner]["terminal"]["winner"]
                    == row["data"]["visible"]["acting_player"],
            "training diagnostic target lacks natural winning witness",
        )?;
    }
    let mut models = Vec::new();
    let mut arms = BTreeSet::new();
    let mut sampler = WideCategoricalScratchV1::default();
    for checkpoint in std::iter::once(None).chain(c.checkpoints.iter().map(Some)) {
        let (mut policy, mut state) = initialize(&c.source)?;
        ensure(
            data["model"]
                == serde_json::to_value(policy.actual_model_identity_v1()).map_err(err)?,
            "training diagnostic parent identity differs",
        )?;
        let (label, permuted) = if let Some(pin) = checkpoint {
            let saved = restore(pin, &c.source, &mut policy, &mut state)?;
            ensure(
                saved.completed_updates == 32
                    && saved.dataset.sha256 == c.dataset.sha256
                    && saved.initial_adam == state.adam_step_v1() - 32
                    && arms.insert(saved.target_permuted),
                "training diagnostic endpoint or training dataset differs",
            )?;
            (
                if saved.target_permuted {
                    "permuted"
                } else {
                    "teacher"
                },
                saved.target_permuted,
            )
        } else {
            ("g115", false)
        };
        let before = hex(&state.state_sha256_v1().map_err(err)?);
        let mut results = Vec::new();
        for row in rows {
            let t: TensorBitsV1 =
                serde_json::from_value(row["data"]["tensor"].clone()).map_err(err)?;
            let s = policy
                .score_training_tensor_v4(&NativeFlatDecisionTensorV4 { common: t.tensor() })?;
            ensure(
                s.logits.len() == row["data"]["outcomes"].as_array().unwrap().len(),
                "training diagnostic menu differs",
            )?;
            let winner = row["data"]["winning_indices"][0].as_u64().unwrap() as usize;
            let target = if permuted {
                (winner + 1) % s.logits.len()
            } else {
                winner
            };
            let masses = sampler.apportion(&s.logits).map_err(err)?;
            ensure(
                masses.iter().sum::<u128>() == (1u128 << 64),
                "sampler masses do not sum to2^64",
            )?;
            let probabilities = probability(&s.logits);
            let mut best = 0;
            for i in 1..s.logits.len() {
                if s.logits[i] > s.logits[best] {
                    best = i;
                }
            }
            results.push(
                json!({"id":row["id"],"spec":row["spec"],"certified_winner":winner,
                "training_target":target,"argmax":best,"argmax_wins":best==winner,
                "winning_cross_entropy":cross_entropy(&s.logits,winner),
                "training_target_cross_entropy":cross_entropy(&s.logits,target),
                "winning_softmax_probability":probabilities[winner],
                "winning_sampler_probability":masses[winner] as f64 / 18446744073709551616.0f64,
                "sampler_masses_decimal":masses.iter().map(u128::to_string).collect::<Vec<_>>(),
                "logits_bits":bits(&s.logits),"value_bits":s.value.to_bits()}),
            );
        }
        ensure(
            hex(&state.state_sha256_v1().map_err(err)?) == before,
            "read-only diagnostic mutated optimizer",
        )?;
        models.push(
            json!({"label":label,"checkpoint":checkpoint,"state_sha256":before,
            "model":policy.actual_model_identity_v1(),"records":results}),
        );
    }
    let result = json!({"schema":"terminal-teacher-training-diagnostic/v1","complete":true,
        "positions":32,"updates":0,"evaluation_positions_read":0,"dataset":c.dataset,"models":models,
        "sampler":"WideCategoricalScratchV1 dispatches to production small-menu sampler; no random draws",
        "non_claim":"Training fit only; neither reserved validation nor whole-match playing strength."});
    fs::create_dir(&c.output_directory).map_err(err)?;
    publish_json(&c.output_directory, "result.json", &result)?;
    Ok(result)
}
