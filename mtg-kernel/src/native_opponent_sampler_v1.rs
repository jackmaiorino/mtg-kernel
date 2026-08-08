//! Frozen uniform-modulo opponent selection for the native trainer.
//!
//! The native schedule derives one leaf seed for every physical action
//! substep.  Selection is then the unsigned seed modulo the legal action
//! count.  In particular, a width-one substep still derives and records its
//! leaf seed before returning index zero; the following substep uses the next
//! substep index.

use crate::native_trainer_schedule_v1::{
    derive_native_trainer_opponent_action_seed_v1, NativeTrainerScheduleErrorV1,
};
use core::fmt;

pub const UNIFORM_INDEX_MODULO_U64_IDENTITY_V1: &str = "mtg-kernel-uniform-index-modulo-u64-v1";
pub const UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1: &str =
    "selected-index-equals-action-seed-mod-legal-count";
pub const UNIFORM_INDEX_MODULO_U64_ZERO_RULE_V1: &str = "legal-count-zero-rejects-before-modulo";
pub const UNIFORM_INDEX_MODULO_U64_WIDTH_ONE_SEED_RULE_V1: &str =
    "derive-and-record-one-leaf-seed-for-every-substep-including-legal-count-one;then-selected-index-is-zero-and-the-next-substep-index-advances";
pub const UNIFORM_INDEX_MODULO_U64_BIAS_RULE_V1: &str =
    "intentional-modulo-bias-no-rejection-sampling;when-legal-count-does-not-divide-the-action-seed-domain-low-residues-have-one-extra-preimage;changing-this-rule-requires-a-new-sampler-identity";
pub const NATIVE_OPPONENT_SAMPLER_VECTORS_FILE_SHA256_V1: &str =
    "9e5898308d30614a4a09cecb584200521b1a3b727606d8cf78dbe70b51106e18";
pub const NATIVE_OPPONENT_SAMPLER_VECTOR_STREAM_SHA256_V1: &str =
    "2b65520a528dcf9eba8d7baded50cc9ad50cf507704c2b4410e2afb4b34d7fad";
pub const NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_IDENTITY_V1: &str =
    "mtg-kernel-trainer-uniform-policy-v1";
pub const NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_MODEL_RULE_V1: &str =
    "no-model-uniform-legal-index";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniformIndexModuloU64ErrorV1 {
    EmptyLegalActionSet,
}

impl fmt::Display for UniformIndexModuloU64ErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLegalActionSet => formatter.write_str("empty legal action set"),
        }
    }
}

impl std::error::Error for UniformIndexModuloU64ErrorV1 {}

/// Select from `legal_count` entries using the frozen unsigned-modulo rule.
///
/// Modulo bias is intentional under this identity.  No rejection draw or seed
/// advancement occurs inside this leaf operation.
#[inline]
pub fn select_uniform_index_modulo_u64_v1(
    action_seed: u64,
    legal_count: u32,
) -> Result<u32, UniformIndexModuloU64ErrorV1> {
    if legal_count == 0 {
        return Err(UniformIndexModuloU64ErrorV1::EmptyLegalActionSet);
    }
    let selected = action_seed % u64::from(legal_count);
    Ok(u32::try_from(selected).expect("modulo result is strictly below a u32 divisor"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTrainerOpponentSelectionV1 {
    pub(crate) action_seed: u64,
    pub(crate) selected_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeTrainerOpponentSelectionErrorV1 {
    Schedule(NativeTrainerScheduleErrorV1),
    Sampler(UniformIndexModuloU64ErrorV1),
}

impl From<NativeTrainerScheduleErrorV1> for NativeTrainerOpponentSelectionErrorV1 {
    fn from(value: NativeTrainerScheduleErrorV1) -> Self {
        Self::Schedule(value)
    }
}

impl From<UniformIndexModuloU64ErrorV1> for NativeTrainerOpponentSelectionErrorV1 {
    fn from(value: UniformIndexModuloU64ErrorV1) -> Self {
        Self::Sampler(value)
    }
}

/// Derive and retain the native trainer's opponent leaf seed, then select.
///
/// Seed derivation is deliberately unconditional on `legal_count`, including
/// width one.  This is the production entry point intended for native trainer
/// opponent substeps.
pub(crate) fn select_native_trainer_opponent_action_v1(
    base_seed: u64,
    episode_index: u64,
    opponent_physical_decision_index: u64,
    substep_index: u32,
    legal_count: u32,
) -> Result<NativeTrainerOpponentSelectionV1, NativeTrainerOpponentSelectionErrorV1> {
    let action_seed = derive_native_trainer_opponent_action_seed_v1(
        base_seed,
        episode_index,
        opponent_physical_decision_index,
        substep_index,
    )?;
    let selected_index = select_uniform_index_modulo_u64_v1(action_seed, legal_count)?;
    Ok(NativeTrainerOpponentSelectionV1 {
        action_seed,
        selected_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_trainer_schedule_v1::{
        derive_native_trainer_opponent_group_seed_v1, NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
        NATIVE_TRAINER_SCHEDULE_VERSION_V1, PYTHON_REFERENCE_SEED_VERSION_V1,
    };
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    const VECTOR_BYTES: &[u8] =
        include_bytes!("../../data/native_opponent_sampler_vectors_v1.json");
    const GENERATOR_BYTES: &[u8] =
        include_bytes!("../../python/tools/generate_native_opponent_sampler_vectors_v1.py");
    const SCHEMA: &str = "mtg-kernel-native-opponent-sampler-cross-language-vectors/v1";
    const GENERATOR_IDENTITY: &str = "stdlib-only-independent-sha256-uniform-modulo-reference-v1";
    const SEED_ATOM_FRAMING_IDENTITY: &str = "u32be-tag-length-u64be-payload-length-atom-v1";
    const SEMANTIC_STREAM_FRAMING_IDENTITY: &str = "ordered-atom-stream-u32be-tag-u64be-payload-v1";
    const ATOM_FORMULA: &str =
        "u32be(tag_utf8_byte_length)||tag_utf8||u64be(payload_byte_length)||payload";
    const SEED_DERIVATION_ALGORITHM: &str =
        "sha256(ATOM(version,python-reference-seed-version)||ATOM(namespace,namespace)||ordered-ATOM(field-name,name)||ATOM(u63,u64be(value)))[:8]be&0x7fff_ffff_ffff_ffff";
    const SEED_TEXT_PAYLOAD_ENCODING: &str =
        "UTF-8 for version, namespace, and field-name payloads";
    const SEED_U63_PAYLOAD_ENCODING: &str = "exactly-8-byte-big-endian";
    const WIDTH_ONE_WITNESS_RULE: &str =
        "for-each-chain-emit-a-witness-for-every-non-final-substep-with-legal-count-one;pair-it-with-the-immediate-successor;exclude-a-final-width-one-substep-because-no-successor-exists;counterfactual_nonconsuming_next_substep_index_u32-equals-width_one_substep_index_u32-and-counterfactual_nonconsuming_next_action_seed_u63-equals-width_one_action_seed_u63-that-would-be-reused-if-the-substep-index-did-not-advance";

    fn sha256_hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn value_str<'a>(value: &'a Value, key: &str) -> &'a str {
        value[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} must be a string"))
    }

    fn value_u32(value: &Value, key: &str) -> u32 {
        u32::try_from(
            value[key]
                .as_u64()
                .unwrap_or_else(|| panic!("{key} must be an unsigned integer")),
        )
        .unwrap_or_else(|_| panic!("{key} must fit u32"))
    }

    fn decimal_u64(value: &Value, key: &str) -> u64 {
        value_str(value, key)
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("{key} must be a canonical decimal u64"))
    }

    fn decode_lower_hex_32(encoded: &str) -> [u8; 32] {
        assert_eq!(encoded.len(), 64);
        assert!(encoded
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value)));
        let mut decoded = [0_u8; 32];
        for (target, pair) in decoded.iter_mut().zip(encoded.as_bytes().chunks_exact(2)) {
            *target = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        decoded
    }

    fn append_atom(stream: &mut Vec<u8>, tag: &str, payload: &[u8]) {
        stream.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
        stream.extend_from_slice(tag.as_bytes());
        stream.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
        stream.extend_from_slice(payload);
    }

    fn append_text(stream: &mut Vec<u8>, tag: &str, payload: &str) {
        append_atom(stream, tag, payload.as_bytes());
    }

    fn append_u32(stream: &mut Vec<u8>, tag: &str, payload: u32) {
        append_atom(stream, tag, &payload.to_be_bytes());
    }

    fn append_u64(stream: &mut Vec<u8>, tag: &str, payload: u64) {
        append_atom(stream, tag, &payload.to_be_bytes());
    }

    fn semantic_stream(fixture: &Value) -> Vec<u8> {
        let mut stream = Vec::new();
        let sampler = &fixture["sampler"];
        let seed_chain = &fixture["seed_chain"];
        append_text(&mut stream, "schema", value_str(fixture, "schema"));
        append_u32(
            &mut stream,
            "vector-schema-version",
            value_u32(fixture, "vector_schema_version"),
        );
        append_text(
            &mut stream,
            "sampler-identity",
            value_str(sampler, "identity"),
        );
        append_text(
            &mut stream,
            "sampler-algorithm",
            value_str(sampler, "algorithm"),
        );
        append_text(
            &mut stream,
            "legal-count-zero-rule",
            value_str(sampler, "legal_count_zero_rule"),
        );
        append_text(
            &mut stream,
            "width-one-seed-rule",
            value_str(sampler, "width_one_seed_rule"),
        );
        append_text(
            &mut stream,
            "modulo-bias-rule",
            value_str(sampler, "modulo_bias_rule"),
        );
        append_text(
            &mut stream,
            "trainer-schedule-version",
            value_str(seed_chain, "trainer_schedule_version"),
        );
        append_text(
            &mut stream,
            "python-reference-seed-version",
            value_str(seed_chain, "python_reference_seed_version"),
        );
        append_atom(
            &mut stream,
            "schedule-goldens-sha256",
            &decode_lower_hex_32(value_str(seed_chain, "schedule_goldens_sha256")),
        );
        append_text(
            &mut stream,
            "seed-atom-framing-identity",
            value_str(seed_chain, "atom_framing_identity"),
        );
        append_text(
            &mut stream,
            "seed-text-payload-encoding",
            value_str(&seed_chain["payload_encodings"], "text"),
        );
        append_text(
            &mut stream,
            "seed-u63-payload-encoding",
            value_str(&seed_chain["payload_encodings"], "u63"),
        );
        append_text(
            &mut stream,
            "seed-derivation-algorithm",
            value_str(seed_chain, "derivation_algorithm"),
        );
        append_text(
            &mut stream,
            "width-one-witness-rule",
            value_str(seed_chain, "witness_rule"),
        );
        append_text(
            &mut stream,
            "group-namespace",
            value_str(seed_chain, "group_namespace"),
        );
        let group_fields = seed_chain["group_fields_ordered"].as_array().unwrap();
        append_u32(
            &mut stream,
            "group-field-count",
            u32::try_from(group_fields.len()).unwrap(),
        );
        for field in group_fields {
            append_text(&mut stream, "group-field", field.as_str().unwrap());
        }
        append_text(
            &mut stream,
            "substep-namespace",
            value_str(seed_chain, "substep_namespace"),
        );
        let substep_fields = seed_chain["substep_fields_ordered"].as_array().unwrap();
        append_u32(
            &mut stream,
            "substep-field-count",
            u32::try_from(substep_fields.len()).unwrap(),
        );
        for field in substep_fields {
            append_text(&mut stream, "substep-field", field.as_str().unwrap());
        }

        let points = fixture["points"].as_array().unwrap();
        append_u32(
            &mut stream,
            "point-count",
            u32::try_from(points.len()).unwrap(),
        );
        for point in points {
            append_text(&mut stream, "point-name", value_str(point, "name"));
            append_u64(
                &mut stream,
                "action-seed-u64",
                decimal_u64(point, "action_seed_u64"),
            );
            append_u32(
                &mut stream,
                "legal-count-u32",
                value_u32(point, "legal_count_u32"),
            );
            append_u32(
                &mut stream,
                "selected-index-u32",
                value_u32(point, "selected_index_u32"),
            );
        }

        let rejections = fixture["rejections"].as_array().unwrap();
        append_u32(
            &mut stream,
            "rejection-count",
            u32::try_from(rejections.len()).unwrap(),
        );
        for rejection in rejections {
            append_text(&mut stream, "rejection-name", value_str(rejection, "name"));
            append_u64(
                &mut stream,
                "action-seed-u64",
                decimal_u64(rejection, "action_seed_u64"),
            );
            append_u32(
                &mut stream,
                "legal-count-u32",
                value_u32(rejection, "legal_count_u32"),
            );
            append_text(
                &mut stream,
                "error-code",
                value_str(&rejection["expected_error"], "code"),
            );
        }

        let chains = fixture["chains"].as_array().unwrap();
        append_u32(
            &mut stream,
            "chain-count",
            u32::try_from(chains.len()).unwrap(),
        );
        for chain in chains {
            append_text(&mut stream, "chain-name", value_str(chain, "name"));
            append_u64(
                &mut stream,
                "base-seed-u63",
                decimal_u64(chain, "base_seed_u63"),
            );
            append_u64(
                &mut stream,
                "episode-index-u63",
                decimal_u64(chain, "episode_index_u63"),
            );
            append_u64(
                &mut stream,
                "opponent-physical-decision-index-u63",
                decimal_u64(chain, "opponent_physical_decision_index_u63"),
            );
            append_u64(
                &mut stream,
                "opponent-group-seed-u63",
                decimal_u64(chain, "opponent_group_seed_u63"),
            );
            let substeps = chain["substeps"].as_array().unwrap();
            append_u32(
                &mut stream,
                "substep-count",
                u32::try_from(substeps.len()).unwrap(),
            );
            for substep in substeps {
                append_u32(
                    &mut stream,
                    "substep-index-u32",
                    value_u32(substep, "substep_index_u32"),
                );
                append_u32(
                    &mut stream,
                    "legal-count-u32",
                    value_u32(substep, "legal_count_u32"),
                );
                append_u64(
                    &mut stream,
                    "action-seed-u63",
                    decimal_u64(substep, "action_seed_u63"),
                );
                append_u32(
                    &mut stream,
                    "selected-index-u32",
                    value_u32(substep, "selected_index_u32"),
                );
            }
            let witnesses = chain["width_one_advancement_witnesses"].as_array().unwrap();
            append_u32(
                &mut stream,
                "width-one-witness-count",
                u32::try_from(witnesses.len()).unwrap(),
            );
            for witness in witnesses {
                append_u32(
                    &mut stream,
                    "width-one-substep-index-u32",
                    value_u32(witness, "width_one_substep_index_u32"),
                );
                append_u64(
                    &mut stream,
                    "width-one-action-seed-u63",
                    decimal_u64(witness, "width_one_action_seed_u63"),
                );
                append_u32(
                    &mut stream,
                    "next-substep-index-u32",
                    value_u32(witness, "next_substep_index_u32"),
                );
                append_u64(
                    &mut stream,
                    "next-action-seed-u63",
                    decimal_u64(witness, "next_action_seed_u63"),
                );
                append_u32(
                    &mut stream,
                    "counterfactual-nonconsuming-next-substep-index-u32",
                    value_u32(
                        witness,
                        "counterfactual_nonconsuming_next_substep_index_u32",
                    ),
                );
                append_u64(
                    &mut stream,
                    "counterfactual-nonconsuming-next-action-seed-u63",
                    decimal_u64(witness, "counterfactual_nonconsuming_next_action_seed_u63"),
                );
            }
        }
        stream
    }

    #[test]
    fn cross_language_vectors_bind_production_modulo_and_full_seed_chain() {
        assert_eq!(
            sha256_hex(VECTOR_BYTES),
            NATIVE_OPPONENT_SAMPLER_VECTORS_FILE_SHA256_V1
        );
        let fixture: Value = serde_json::from_slice(VECTOR_BYTES).unwrap();
        assert_eq!(value_str(&fixture, "schema"), SCHEMA);
        assert_eq!(value_u32(&fixture, "vector_schema_version"), 1);

        let sampler = &fixture["sampler"];
        assert_eq!(
            value_str(sampler, "identity"),
            UNIFORM_INDEX_MODULO_U64_IDENTITY_V1
        );
        assert_eq!(
            value_str(sampler, "algorithm"),
            UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1
        );
        assert_eq!(
            value_str(sampler, "legal_count_zero_rule"),
            UNIFORM_INDEX_MODULO_U64_ZERO_RULE_V1
        );
        assert_eq!(
            value_str(sampler, "width_one_seed_rule"),
            UNIFORM_INDEX_MODULO_U64_WIDTH_ONE_SEED_RULE_V1
        );
        assert_eq!(
            value_str(sampler, "modulo_bias_rule"),
            UNIFORM_INDEX_MODULO_U64_BIAS_RULE_V1
        );

        let seed_chain = &fixture["seed_chain"];
        assert_eq!(
            value_str(seed_chain, "trainer_schedule_version"),
            NATIVE_TRAINER_SCHEDULE_VERSION_V1
        );
        assert_eq!(
            value_str(seed_chain, "python_reference_seed_version"),
            PYTHON_REFERENCE_SEED_VERSION_V1
        );
        assert_eq!(
            value_str(seed_chain, "schedule_goldens_sha256"),
            NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1
        );
        assert_eq!(
            value_str(seed_chain, "atom_framing_identity"),
            SEED_ATOM_FRAMING_IDENTITY
        );
        assert_eq!(value_str(seed_chain, "atom_formula"), ATOM_FORMULA);
        assert_eq!(
            value_str(seed_chain, "derivation_algorithm"),
            SEED_DERIVATION_ALGORITHM
        );
        assert_eq!(
            value_str(&seed_chain["payload_encodings"], "text"),
            SEED_TEXT_PAYLOAD_ENCODING
        );
        assert_eq!(
            value_str(&seed_chain["payload_encodings"], "u63"),
            SEED_U63_PAYLOAD_ENCODING
        );
        assert_eq!(
            value_str(seed_chain, "witness_rule"),
            WIDTH_ONE_WITNESS_RULE
        );
        assert_eq!(
            seed_chain["group_fields_ordered"],
            serde_json::json!([
                "base_seed",
                "episode_index",
                "opponent_physical_decision_index"
            ])
        );
        assert_eq!(
            seed_chain["substep_fields_ordered"],
            serde_json::json!(["group_seed", "substep_index"])
        );

        let points = fixture["points"].as_array().unwrap();
        assert_eq!(points.len(), 33);
        assert_eq!(value_u32(&fixture, "point_count"), 33);
        let count_two_indices = points
            .iter()
            .filter(|point| value_u32(point, "legal_count_u32") == 2)
            .map(|point| value_u32(point, "selected_index_u32"))
            .collect::<Vec<_>>();
        assert_eq!(count_two_indices, vec![0, 1, 0, 1]);
        for point in points {
            let actual = select_uniform_index_modulo_u64_v1(
                decimal_u64(point, "action_seed_u64"),
                value_u32(point, "legal_count_u32"),
            )
            .unwrap();
            assert_eq!(actual, value_u32(point, "selected_index_u32"));
        }

        let rejections = fixture["rejections"].as_array().unwrap();
        assert_eq!(rejections.len(), 2);
        assert_eq!(value_u32(&fixture, "rejection_count"), 2);
        for rejection in rejections {
            assert_eq!(
                value_str(&rejection["expected_error"], "code"),
                "empty-legal-action-set"
            );
            assert_eq!(
                select_uniform_index_modulo_u64_v1(
                    decimal_u64(rejection, "action_seed_u64"),
                    value_u32(rejection, "legal_count_u32"),
                ),
                Err(UniformIndexModuloU64ErrorV1::EmptyLegalActionSet)
            );
        }

        let chains = fixture["chains"].as_array().unwrap();
        assert_eq!(chains.len(), 3);
        assert_eq!(value_u32(&fixture, "chain_count"), 3);
        for chain in chains {
            let base_seed = decimal_u64(chain, "base_seed_u63");
            let episode_index = decimal_u64(chain, "episode_index_u63");
            let decision_index = decimal_u64(chain, "opponent_physical_decision_index_u63");
            assert_eq!(
                derive_native_trainer_opponent_group_seed_v1(
                    base_seed,
                    episode_index,
                    decision_index
                )
                .unwrap(),
                decimal_u64(chain, "opponent_group_seed_u63")
            );
            let substeps = chain["substeps"].as_array().unwrap();
            for (position, substep) in substeps.iter().enumerate() {
                let substep_index = value_u32(substep, "substep_index_u32");
                assert_eq!(usize::try_from(substep_index).unwrap(), position);
                let selected = select_native_trainer_opponent_action_v1(
                    base_seed,
                    episode_index,
                    decision_index,
                    substep_index,
                    value_u32(substep, "legal_count_u32"),
                )
                .unwrap();
                assert_eq!(
                    selected.action_seed,
                    decimal_u64(substep, "action_seed_u63")
                );
                assert_eq!(
                    selected.selected_index,
                    value_u32(substep, "selected_index_u32")
                );
                if value_u32(substep, "legal_count_u32") == 1 {
                    assert_eq!(selected.selected_index, 0);
                }
            }

            let witnesses = chain["width_one_advancement_witnesses"].as_array().unwrap();
            assert_eq!(witnesses.len(), 1);
            for witness in witnesses {
                let width_one_index = value_u32(witness, "width_one_substep_index_u32");
                let next_index = value_u32(witness, "next_substep_index_u32");
                assert_eq!(next_index, width_one_index + 1);
                assert_eq!(
                    value_u32(
                        witness,
                        "counterfactual_nonconsuming_next_substep_index_u32"
                    ),
                    width_one_index
                );
                let width_one = &substeps[usize::try_from(width_one_index).unwrap()];
                let next = &substeps[usize::try_from(next_index).unwrap()];
                assert_eq!(value_u32(width_one, "legal_count_u32"), 1);
                assert_eq!(
                    decimal_u64(witness, "width_one_action_seed_u63"),
                    decimal_u64(width_one, "action_seed_u63")
                );
                assert_eq!(
                    decimal_u64(witness, "next_action_seed_u63"),
                    decimal_u64(next, "action_seed_u63")
                );
                assert_eq!(
                    decimal_u64(witness, "counterfactual_nonconsuming_next_action_seed_u63"),
                    decimal_u64(width_one, "action_seed_u63")
                );
                assert_ne!(
                    decimal_u64(witness, "next_action_seed_u63"),
                    decimal_u64(witness, "counterfactual_nonconsuming_next_action_seed_u63")
                );
            }
            if value_str(chain, "name") == "u63-boundary-width-one-before-tail" {
                let trailing_index = substeps.len() - 1;
                assert_eq!(value_u32(&substeps[trailing_index], "legal_count_u32"), 1);
                assert!(witnesses.iter().all(|witness| {
                    usize::try_from(value_u32(witness, "width_one_substep_index_u32")).unwrap()
                        != trailing_index
                }));
            }
        }

        let semantic = &fixture["semantic_stream"];
        assert_eq!(
            value_str(semantic, "framing_identity"),
            SEMANTIC_STREAM_FRAMING_IDENTITY
        );
        assert_eq!(value_str(semantic, "atom_formula"), ATOM_FORMULA);
        assert_eq!(
            value_str(semantic, "sha256"),
            NATIVE_OPPONENT_SAMPLER_VECTOR_STREAM_SHA256_V1
        );
        assert_eq!(
            sha256_hex(&semantic_stream(&fixture)),
            NATIVE_OPPONENT_SAMPLER_VECTOR_STREAM_SHA256_V1
        );
        assert_eq!(
            value_str(&fixture["authority"], "implementation"),
            GENERATOR_IDENTITY
        );
        assert_eq!(
            value_str(&fixture["authority"], "generator_sha256"),
            sha256_hex(GENERATOR_BYTES)
        );
    }
}
