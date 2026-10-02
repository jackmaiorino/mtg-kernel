//! Complete, explicitly versioned optimizer-state transport. Campaign loss,
//! rollout seeds and provenance still belong in the enclosing run manifest.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bits {
    name: String,
    shape: Vec<usize>,
    values: Vec<u32>,
}

fn pack(rows: &[NativeNamedParameterV1]) -> Vec<Bits> {
    rows.iter()
        .map(|r| Bits {
            name: r.name.into(),
            shape: r.shape.clone(),
            values: r.values.iter().map(|v| v.to_bits()).collect(),
        })
        .collect()
}

fn unpack(rows: Vec<Bits>) -> Result<Vec<NativeNamedParameterV1>, Box<dyn Error>> {
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?;
    let expected = model.parameter_snapshot_v1();
    if rows.len() != expected.len() {
        return Err(training_error("stack snapshot legacy tensor count"));
    }
    rows.into_iter()
        .zip(expected)
        .map(|(r, e)| {
            if r.name != e.name || r.shape != e.shape || r.values.len() != e.values.len() {
                return Err(training_error("stack snapshot legacy tensor layout"));
            }
            Ok(NativeNamedParameterV1 {
                name: e.name,
                shape: r.shape,
                values: r.values.into_iter().map(f32::from_bits).collect(),
            })
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    schema: String,
    feature_contract: String,
    feature_encoding: String,
    auxiliary_contract: String,
    architecture: String,
    projection_sha256: String,
    legacy_state_sha256: String,
    legacy_adam_step: u64,
    scorer_bias_anchor_bits: u32,
    parameters: Vec<Bits>,
    first_moments: Vec<Bits>,
    second_moments: Vec<Bits>,
    stack: StackProjectionSnapshot,
}

pub(crate) fn encode(
    legacy: &NativePolicyValueTrainSnapshotV1,
    stack: &StackProjectionSnapshot,
) -> Result<Vec<u8>, Box<dyn Error>> {
    stack.validate()?;
    let contract_sha = format!("{:x}", Sha256::digest(CONTRACT));
    let state_hash = legacy.state_sha256_v1()?;
    let saved = Saved {
        schema: "mtg-kernel-stack-input-optimizer-state/v1".into(),
        feature_contract: crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4.into(),
        feature_encoding: crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4.into(),
        auxiliary_contract: contract_sha,
        architecture: crate::native_policy_value_net_v1::stack_inputs_v1::ARCHITECTURE.into(),
        projection_sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(stack)?)),
        legacy_state_sha256: state_hash.iter().map(|b| format!("{b:02x}")).collect(),
        legacy_adam_step: legacy.adam_step,
        scorer_bias_anchor_bits: legacy.scorer_bias_anchor_bits,
        parameters: pack(&legacy.parameters),
        first_moments: pack(&legacy.first_moments),
        second_moments: pack(&legacy.second_moments),
        stack: stack.clone(),
    };
    Ok(serde_json::to_vec(&saved)?)
}

pub(crate) fn decode(
    bytes: &[u8],
) -> Result<(NativePolicyValueTrainSnapshotV1, StackProjectionSnapshot), Box<dyn Error>> {
    let saved: Saved = serde_json::from_slice(bytes)?;
    let contract_sha = format!("{:x}", Sha256::digest(CONTRACT));
    if saved.schema != "mtg-kernel-stack-input-optimizer-state/v1"
        || saved.auxiliary_contract != contract_sha
        || saved.architecture != crate::native_policy_value_net_v1::stack_inputs_v1::ARCHITECTURE
        || saved.projection_sha256
            != format!("{:x}", Sha256::digest(serde_json::to_vec(&saved.stack)?))
        || saved.feature_contract != crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4
        || saved.feature_encoding != crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4
    {
        return Err(training_error("stack optimizer-state identity differs"));
    }
    saved.stack.validate()?;
    let legacy = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.legacy_adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters: unpack(saved.parameters)?,
        first_moments: unpack(saved.first_moments)?,
        second_moments: unpack(saved.second_moments)?,
    };
    let hash: String = legacy
        .state_sha256_v1()?
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if hash != saved.legacy_state_sha256 {
        return Err(training_error("legacy optimizer state hash differs"));
    }
    Ok((legacy, saved.stack))
}

#[cfg(test)]
pub(super) fn export_gradient_bits(rows: &[NativeNamedParameterV1]) -> serde_json::Value {
    serde_json::to_value(pack(rows)).unwrap()
}
