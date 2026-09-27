//! Stateless Spellbench scorer for fresh-lineage V4 policies (g115, A48,
//! c12). Loads one pinned model source, answers JSONL requests carrying the
//! `x_kernel_flat_v4` tensor, and returns the chosen row and candidate id.
//! Owns no game state. `xmage_observed_inference_v1` (V3 only) is unchanged.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::expanded_deck_training_v1::{load_expanded_inference_v1, ExpandedModelSourceV1};
use crate::fast_sampler::{WideCategoricalScratchV1, WIDE_CATEGORICAL_SAMPLER_VERSION_V1};
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v4::{
    NativeFlatDecisionTensorV4, FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4,
};
use crate::sideboard_play_policy_v1::{FreshLineageGenerationV1, FrozenPlayPolicyV1};
use crate::xmage_observed_inference_v1::ObservedTensorBitsV1;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::Path;

pub const SCORER_CONFIG_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-config/v1";
pub const SCORER_REQUEST_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-request/v1";
pub const SCORER_CHOICE_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-choice/v1";
pub const SCORER_READY_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-ready/v1";
pub const SCORER_ERROR_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-error/v1";
pub const SELECTION_SAMPLED_WIDE_V1: &str = "sampled-wide-v1";
pub const SELECTION_ARGMAX_FIRST_V1: &str = "argmax-first-v1";
pub const MAX_SCORER_REQUEST_BYTES_V1: usize = 8 * 1024 * 1024;
const MAX_CONFIG_BYTES_V1: u64 = 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellbenchScorerConfigV1 {
    pub schema: String,
    pub source: ExpandedModelSourceV1,
    pub selection: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpellbenchScorerRequestV1 {
    schema: String,
    request_id: String,
    game_id: String,
    seat: String,
    // Part of the checked wire; the stateless scorer does not use it.
    #[allow(dead_code)]
    step: u64,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    row_candidate_ids: Vec<u32>,
    #[serde(default)]
    sample_seed: Option<u64>,
    tensor: ObservedTensorBitsV1,
}

fn into_tensor_v4(bits: ObservedTensorBitsV1) -> NativeFlatDecisionTensorV4 {
    let floats = |values: Vec<u32>| values.into_iter().map(f32::from_bits).collect();
    NativeFlatDecisionTensorV4 {
        common: NativeFlatDecisionTensorV2 {
            state: floats(bits.state),
            object_features: floats(bits.object_features),
            object_card_ids: bits.object_card_ids,
            object_groups: bits.object_groups,
            object_node_ids: bits.object_node_ids,
            edge_features: floats(bits.edge_features),
            edge_source_indices: bits.edge_source_indices,
            edge_target_indices: bits.edge_target_indices,
            action_features: floats(bits.action_features),
            action_ref_features: floats(bits.action_ref_features),
            action_ref_card_ids: bits.action_ref_card_ids,
            action_ref_action_indices: bits.action_ref_action_indices,
            action_ref_node_indices: bits.action_ref_node_indices,
        },
    }
}

fn first_argmax(logits: &[f32]) -> Option<usize> {
    if logits.is_empty() || logits.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut selected = 0;
    for index in 1..logits.len() {
        if logits[index] > logits[selected] {
            selected = index;
        }
    }
    Some(selected)
}

pub(crate) struct SpellbenchScorerV1 {
    policy: FrozenPlayPolicyV1,
    model: Value,
    model_state_sha256: String,
    selection: &'static str,
    sampler: WideCategoricalScratchV1,
}

impl SpellbenchScorerV1 {
    pub(crate) fn new(
        policy: FrozenPlayPolicyV1,
        model: Value,
        model_state_sha256: String,
        selection: &str,
    ) -> Result<Self, &'static str> {
        let selection = match selection {
            SELECTION_SAMPLED_WIDE_V1 => SELECTION_SAMPLED_WIDE_V1,
            SELECTION_ARGMAX_FIRST_V1 => SELECTION_ARGMAX_FIRST_V1,
            _ => return Err("selection"),
        };
        if policy.feature_identity_v1().generation != FreshLineageGenerationV1::V4 {
            return Err("model_generation_unsupported");
        }
        Ok(Self {
            policy,
            model,
            model_state_sha256,
            selection,
            sampler: WideCategoricalScratchV1::default(),
        })
    }

    pub(crate) fn ready_record(&self) -> Value {
        json!({
            "schema": SCORER_READY_SCHEMA_V1,
            "model": self.model,
            "model_state_sha256": self.model_state_sha256,
            "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4,
            "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
            "selection": self.selection,
            "sampler_identity": WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
            "float_encoding": "ieee754-binary32-u32-bits",
            "max_request_bytes": MAX_SCORER_REQUEST_BYTES_V1,
            "game_state_owned": false,
        })
    }

    pub(crate) fn score_line(&mut self, bytes: &[u8]) -> Result<Value, &'static str> {
        let request: SpellbenchScorerRequestV1 =
            serde_json::from_slice(bytes).map_err(|_| "request_json")?;
        if request.schema != SCORER_REQUEST_SCHEMA_V1 {
            return Err("request_schema");
        }
        if request.feature_contract_digest != FEATURE_CONTRACT_DIGEST_V4
            || request.feature_encoding_digest != FEATURE_ENCODING_DIGEST_V4
        {
            return Err("request_feature_contract");
        }
        if request.request_id.is_empty()
            || request.game_id.is_empty()
            || !matches!(request.seat.as_str(), "p0" | "p1")
        {
            return Err("request_identity");
        }
        let rows = request.row_candidate_ids.len();
        if rows == 0
            || request
                .row_candidate_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != rows
        {
            return Err("row_candidate_ids");
        }
        let seed = match (self.selection, request.sample_seed) {
            (SELECTION_SAMPLED_WIDE_V1, Some(seed)) => Some(seed),
            (SELECTION_ARGMAX_FIRST_V1, None) => None,
            _ => return Err("sample_seed"),
        };
        let output = self
            .policy
            .score_training_tensor_v4(&into_tensor_v4(request.tensor))
            .map_err(|_| "native_inference")?;
        if output.logits.len() != rows {
            return Err("row_count");
        }
        if !output.value.is_finite() || output.logits.iter().any(|value| !value.is_finite()) {
            return Err("invalid_model_output");
        }
        let selected = match seed {
            Some(seed) => self
                .sampler
                .sample(&output.logits, seed)
                .map_err(|_| "sampler")?,
            None => first_argmax(&output.logits).ok_or("invalid_model_output")?,
        };
        Ok(json!({
            "schema": SCORER_CHOICE_SCHEMA_V1,
            "request_id": request.request_id,
            "request_sha256": format!("{:x}", Sha256::digest(bytes)),
            "model_state_sha256": self.model_state_sha256,
            "selection": self.selection,
            "logits_bits": output.logits.iter().map(|value| value.to_bits()).collect::<Vec<_>>(),
            "value_bits": output.value.to_bits(),
            "selected_row": selected,
            "selected_candidate_id": request.row_candidate_ids[selected],
        }))
    }
}

/// One bounded JSONL record; a trailing `\r` is dropped (xmage framing).
fn read_record<R: BufRead>(reader: &mut R, limit: usize) -> Result<Option<Vec<u8>>, &'static str> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().map_err(|_| "input_io")?;
        if buffer.is_empty() {
            return Ok((!bytes.is_empty()).then_some(bytes));
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let take = newline.unwrap_or(buffer.len());
        if bytes
            .len()
            .checked_add(take)
            .is_none_or(|size| size > limit)
        {
            return Err("request_too_large");
        }
        bytes.extend_from_slice(&buffer[..take]);
        reader.consume(take + usize::from(newline.is_some()));
        if newline.is_some() {
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return Ok(Some(bytes));
        }
    }
}

fn write_record<W: Write>(writer: &mut W, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|_| "output_json".to_string())?;
    writer
        .write_all(b"\n")
        .map_err(|_| "output_io".to_string())?;
    writer.flush().map_err(|_| "output_io".to_string())
}

fn error_record(code: &str) -> Value {
    json!({"schema": SCORER_ERROR_SCHEMA_V1, "code": code, "service_continues": false})
}

pub(crate) fn serve_scorer_v1<R: BufRead, W: Write>(
    scorer: &mut SpellbenchScorerV1,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    loop {
        let result = match read_record(reader, MAX_SCORER_REQUEST_BYTES_V1) {
            Ok(None) => return Ok(()),
            Ok(Some(bytes)) => scorer.score_line(&bytes),
            Err(code) => Err(code),
        };
        match result {
            Ok(value) => write_record(writer, &value)?,
            Err(code) => {
                write_record(writer, &error_record(code))?;
                return Err(code.into());
            }
        }
    }
}

pub fn run_spellbench_scorer_v1<R: BufRead, W: Write>(
    config: SpellbenchScorerConfigV1,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    if config.schema != SCORER_CONFIG_SCHEMA_V1 {
        write_record(writer, &error_record("config_schema"))?;
        return Err("config_schema".into());
    }
    let (policy, identity) = match load_expanded_inference_v1(&config.source) {
        Ok(loaded) => loaded,
        Err(message) => {
            eprintln!("spellbench scorer: checkpoint load failed: {message}");
            write_record(writer, &error_record("checkpoint_load"))?;
            return Err("checkpoint_load".into());
        }
    };
    let model = serde_json::to_value(&identity).map_err(|_| "identity_json".to_string())?;
    let mut scorer = match SpellbenchScorerV1::new(
        policy,
        model,
        identity.state_sha256.clone(),
        &config.selection,
    ) {
        Ok(scorer) => scorer,
        Err(code) => {
            write_record(writer, &error_record(code))?;
            return Err(code.into());
        }
    };
    write_record(writer, &scorer.ready_record())?;
    serve_scorer_v1(&mut scorer, reader, writer)
}

pub fn run_spellbench_scorer_config_path_v1<R: BufRead, W: Write>(
    path: &Path,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "config_open".to_string())?
        .take(MAX_CONFIG_BYTES_V1 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "config_read".to_string())?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES_V1 {
        return Err("config_too_large".into());
    }
    let config = serde_json::from_slice(&bytes).map_err(|_| "config_json".to_string())?;
    run_spellbench_scorer_v1(config, reader, writer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    use crate::state::SplitMix64;
    use std::io::Cursor;

    fn request_bits(tensor: &NativeFlatDecisionTensorV4) -> ObservedTensorBitsV1 {
        let b = |values: &[f32]| {
            values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<u32>>()
        };
        let t = &tensor.common;
        ObservedTensorBitsV1 {
            state: b(&t.state),
            object_features: b(&t.object_features),
            object_card_ids: t.object_card_ids.clone(),
            object_groups: t.object_groups.clone(),
            object_node_ids: t.object_node_ids.clone(),
            edge_features: b(&t.edge_features),
            edge_source_indices: t.edge_source_indices.clone(),
            edge_target_indices: t.edge_target_indices.clone(),
            action_features: b(&t.action_features),
            action_ref_features: b(&t.action_ref_features),
            action_ref_card_ids: t.action_ref_card_ids.clone(),
            action_ref_action_indices: t.action_ref_action_indices.clone(),
            action_ref_node_indices: t.action_ref_node_indices.clone(),
        }
    }

    /// A live Rally decision with at least two candidates, the harness's
    /// logits on it, and a scorer request with a reversed row map.
    fn live_case(sample_seed: Option<u64>) -> (FastActorSessionV1, Vec<f32>, Vec<u32>, Vec<u8>) {
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            4242,
            17,
            400,
            40_000,
            ["Rally".to_string(), "Rally".to_string()],
        )
        .unwrap();
        loop {
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("game ended")
            };
            if decision.legal_action_count >= 2 {
                break;
            }
            session.step(decision.episode_id, decision.step, 0).unwrap();
        }
        let mut harness = FrozenPlayPolicyV1::training_fixture_v4();
        let scores = harness.score_fast_session_v1(&session).unwrap();
        let tensor = harness.last_scored_training_tensor_v4().unwrap().clone();
        let rows: Vec<u32> = (0..scores.logits.len() as u32).rev().collect();
        let request = json!({
            "schema": SCORER_REQUEST_SCHEMA_V1, "request_id": "m0000p0000g0:5", "game_id": "m0000p0000g0",
            "seat": "p0", "step": 5,
            "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4,
            "row_candidate_ids": rows, "sample_seed": sample_seed,
            "tensor": serde_json::to_value(request_bits(&tensor)).unwrap(),
        });
        (
            session,
            scores.logits,
            rows,
            serde_json::to_vec(&request).unwrap(),
        )
    }

    fn scorer(selection: &str) -> SpellbenchScorerV1 {
        SpellbenchScorerV1::new(
            FrozenPlayPolicyV1::training_fixture_v4(),
            json!({"fixture": true}),
            "0".repeat(64),
            selection,
        )
        .unwrap()
    }

    #[test]
    fn argmax_scorer_reproduces_the_harness_logits_and_first_argmax() {
        let (_, logits, rows, bytes) = live_case(None);
        let choice = scorer(SELECTION_ARGMAX_FIRST_V1)
            .score_line(&bytes)
            .unwrap();
        let expected: Vec<u32> = logits.iter().map(|value| value.to_bits()).collect();
        assert_eq!(choice["logits_bits"], json!(expected));
        let row = first_argmax(&logits).unwrap();
        assert_eq!(choice["selected_row"], json!(row));
        assert_eq!(choice["selected_candidate_id"], json!(rows[row]));
        assert_eq!(
            choice["request_sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
    }

    #[test]
    fn sampled_scorer_matches_the_harness_selection_for_the_same_seat_stream() {
        let seat_seed = 6_130_830_677_676_653_025_u64;
        let sample_seed = SplitMix64::seed(seat_seed).next_u64();
        let (session, _, rows, bytes) = live_case(Some(sample_seed));
        let choice = scorer(SELECTION_SAMPLED_WIDE_V1)
            .score_line(&bytes)
            .unwrap();
        let mut harness = FrozenPlayPolicyV1::training_fixture_v4();
        harness.reset_sampling_v1([seat_seed, seat_seed]);
        let harness_row = harness.select_fast_session_v1(&session).unwrap() as usize;
        assert_eq!(choice["selected_row"], json!(harness_row));
        assert_eq!(choice["selected_candidate_id"], json!(rows[harness_row]));
    }

    #[test]
    fn malformed_requests_fail_closed() {
        let (_, _, _, sampled) = live_case(Some(7));
        let (_, _, _, unseeded) = live_case(None);
        let edit = |bytes: &[u8], edit: &dyn Fn(&mut Value)| {
            let mut value: Value = serde_json::from_slice(bytes).unwrap();
            edit(&mut value);
            serde_json::to_vec(&value).unwrap()
        };
        let mut sampler = scorer(SELECTION_SAMPLED_WIDE_V1);
        assert_eq!(
            sampler.score_line(&edit(&sampled, &|v| v["feature_contract_digest"] =
                json!("0".repeat(64)))),
            Err("request_feature_contract")
        );
        assert_eq!(
            sampler.score_line(&edit(&sampled, &|v| v["row_candidate_ids"][1] =
                v["row_candidate_ids"][0].clone())),
            Err("row_candidate_ids")
        );
        assert_eq!(
            sampler.score_line(&edit(&sampled, &|v| {
                v["row_candidate_ids"].as_array_mut().unwrap().pop();
            })),
            Err("row_count")
        );
        assert_eq!(
            sampler.score_line(&edit(&sampled, &|v| v["opponent_hand"] = json!([1]))),
            Err("request_json")
        );
        assert_eq!(sampler.score_line(&unseeded), Err("sample_seed"));
        assert_eq!(
            scorer(SELECTION_ARGMAX_FIRST_V1).score_line(&sampled),
            Err("sample_seed")
        );
    }

    #[test]
    fn serve_loop_answers_each_line_then_stops_at_the_first_rejection() {
        let (_, _, _, bytes) = live_case(Some(9));
        let mut input = bytes.clone();
        input.extend_from_slice(b"\r\n");
        input.extend_from_slice(&bytes);
        input.extend_from_slice(b"\nnot json\n");
        let mut output = Vec::new();
        let mut scorer = scorer(SELECTION_SAMPLED_WIDE_V1);
        let result = serve_scorer_v1(&mut scorer, &mut Cursor::new(input), &mut output);
        assert_eq!(result, Err("request_json".to_string()));
        let lines: Vec<Value> = output
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_slice(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["schema"], SCORER_CHOICE_SCHEMA_V1);
        assert_eq!(lines[1], lines[0]);
        assert_eq!(lines[2]["code"], "request_json");
    }

    #[test]
    fn load_failure_emits_one_error_record_and_stops() {
        let config: SpellbenchScorerConfigV1 = serde_json::from_value(json!({
            "schema": SCORER_CONFIG_SCHEMA_V1, "selection": SELECTION_SAMPLED_WIDE_V1,
            "source": {
                "play_import": {"path": "E:/does-not-exist/descriptor.json", "sha256": "0".repeat(64)},
                "feature_transfer": {
                    "expected_feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
                    "expected_feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4
                },
                "checkpoint": null
            }
        }))
        .unwrap();
        let mut output = Vec::new();
        let result = run_spellbench_scorer_v1(config, &mut Cursor::new(Vec::new()), &mut output);
        assert_eq!(result, Err("checkpoint_load".to_string()));
        let record: Value =
            serde_json::from_slice(output.split(|b| *b == b'\n').next().unwrap()).unwrap();
        assert_eq!(
            record,
            json!({"schema": SCORER_ERROR_SCHEMA_V1, "code": "checkpoint_load", "service_continues": false})
        );
    }

    #[test]
    fn ready_record_names_the_v4_identity_and_selection() {
        let ready = scorer(SELECTION_SAMPLED_WIDE_V1).ready_record();
        assert_eq!(ready["schema"], SCORER_READY_SCHEMA_V1);
        assert_eq!(ready["feature_contract_digest"], FEATURE_CONTRACT_DIGEST_V4);
        assert_eq!(ready["card_db_hash"], "064a7c989255ab3c");
        assert_eq!(ready["selection"], "sampled-wide-v1");
        assert_eq!(
            ready["sampler_identity"],
            "f32-q8-expq63-hamilton-splitmix64-wide-v1"
        );
    }

    /// Needs SPELLBENCH_KERNEL_BOT_CONFIG_DIR=E:/spellbench-archive/kernel-bots.
    #[test]
    #[ignore = "loads the real Phase 1 checkpoints"]
    fn real_phase1_models_load_with_their_pinned_state() {
        let dir =
            std::path::PathBuf::from(std::env::var("SPELLBENCH_KERNEL_BOT_CONFIG_DIR").unwrap());
        for (name, state) in [
            (
                "g115",
                "8139016ca561961714f25e22a9d6f7fc888548fc332e45b6bce402dfc43159f2",
            ),
            (
                "a48",
                "774e936da26468358257774fe41ae94578265b331d2edb49d017c0835b08699a",
            ),
            (
                "c12",
                "5c14c025e3cc87fb2c3eea28e1dbd770d7d257344174296d6259020f720da9de",
            ),
        ] {
            let mut output = Vec::new();
            run_spellbench_scorer_config_path_v1(
                &dir.join(format!("{name}.scorer.json")),
                &mut Cursor::new(Vec::new()),
                &mut output,
            )
            .unwrap();
            let ready: Value =
                serde_json::from_slice(output.split(|b| *b == b'\n').next().unwrap()).unwrap();
            assert_eq!(ready["model_state_sha256"], state, "{name}");
            if name == "g115" {
                assert_eq!(
                    ready["model"]["model"]["weights_sha256"],
                    "e2ca2f2b5dd750a59e24c71a4bac325ed7449d97b5892a79a80132e45d538333"
                );
            }
        }
    }
}
