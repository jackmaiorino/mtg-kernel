//! Frozen natural-tensor ablations; never legal counterfactual game actions.
use super::*;
pub(super) fn run(command: Command) -> Result<Value, String> {
    let input_pin = command
        .natural_hand_channels
        .as_ref()
        .ok_or("missing natural input")?;
    let input: Value = serde_json::from_slice(&read_pinned_bytes(input_pin)?).map_err(err)?;
    ensure(
        input["schema"] == "natural-hand-channel-input/v1",
        "wrong natural input schema",
    )?;
    let rows = input["records"]
        .as_array()
        .ok_or("missing natural records")?;
    ensure(rows.len() == 8, "exactly8 natural roots required")?;
    let (policy, _) = initialize(&command.source)?;
    let mut records = Vec::new();
    for entry in rows {
        let row = &entry["row"];
        let node = entry["hand_node"].as_u64().ok_or("missing node")? as usize;
        let original = &row["tensor"];
        ensure(
            original["object_groups"][node] == 0 && entry["replacement_token"] == 77,
            "invalid hand ablation",
        )?;
        ensure(
            !original["action_ref_node_indices"]
                .as_array()
                .ok_or("refs")?
                .contains(&json!(node)),
            "ablated hand node is action referenced",
        )?;
        for token in [false, true] {
            for digest in [false, true] {
                let mut mixed = original.clone();
                if token {
                    mixed["object_card_ids"][node] = json!(77);
                }
                if digest {
                    let state = mixed["state"].as_array_mut().ok_or("state")?;
                    ensure(state.len() == 219, "state width differs")?;
                    for value in &mut state[123..] {
                        *value = json!(0u32);
                    }
                }
                let encoded: TensorBitsV1 = serde_json::from_value(mixed).map_err(err)?;
                let scores = policy.score_training_tensor_v4(&NativeFlatDecisionTensorV4 {
                    common: encoded.tensor(),
                })?;
                if !token && !digest {
                    ensure(
                        json!(bits(&scores.logits)) == row["logits"]
                            && json!(scores.value.to_bits()) == row["value"],
                        &format!("archived g115 baseline differs at step {}", row["step"]),
                    )?;
                }
                records.push(json!({"step":row["step"],"physical_decision_id":row["physical_decision_id"],"token_replaced":token,"digest_zeroed":digest,"artificial":token||digest,"original_token":original["object_card_ids"][node],"hand_node":node,"selected":row["selected"],"logits":scores.logits,"probabilities":probability(&scores.logits),"value":scores.value}));
            }
        }
    }
    let result = json!({"schema":"natural-hand-channel-result/v1","positions":8,"combinations":32,"input":input_pin,"source":command.source,"records":records,"non_claim":"Eight correlated natural roots from one episode, with artificial token replacement and zero-digest ablations. No tactical labels, optimality, natural win-rate, training effect or promotion claim."});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "result.json", &result)?;
    Ok(result)
}
