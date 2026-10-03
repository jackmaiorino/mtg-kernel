//! Stateless Spellbench scorer for fresh-lineage V4 policies (g115, A48,
//! c12). Loads one pinned model source, answers JSONL requests carrying the
//! `x_kernel_flat_v4` tensor, and returns the chosen row and candidate id.
//! Owns no game state. `xmage_observed_inference_v1` (V3 only) is unchanged.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::expanded_deck_training_v1::{load_expanded_inference_v1, ExpandedModelSourceV1};
use crate::fast_sampler::{WideCategoricalScratchV1, WIDE_CATEGORICAL_SAMPLER_VERSION_V1};
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v4::NativeFlatDecisionTensorV4;
use crate::sideboard_play_policy_v1::{
    FreshFeatureIdentityV1, FreshLineageGenerationV1, FrozenPlayPolicyV1,
};
use crate::xmage_observed_inference_v1::ObservedTensorBitsV1;
use serde::Deserialize;
use serde_json::{json, Value};
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Display;
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::Path;

pub const SCORER_CONFIG_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-config/v1";
pub const SCORER_REQUEST_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-request/v1";
pub const SCORER_PROPOSAL_SCHEMA_V1: &str = "mtg-kernel-spellbench-completion-proposals/v1";
pub const SCORER_CHOICE_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-choice/v1";
pub const SCORER_READY_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-ready/v1";
pub const SCORER_ERROR_SCHEMA_V1: &str = "mtg-kernel-spellbench-scorer-error/v1";
pub const SELECTION_SAMPLED_WIDE_V1: &str = "sampled-wide-v1";
pub const SELECTION_ARGMAX_FIRST_V1: &str = "argmax-first-v1";
pub const MAX_SCORER_REQUEST_BYTES_V1: usize = 8 * 1024 * 1024;
const MAX_CONFIG_BYTES_V1: u64 = 1024 * 1024;
/// Longest cause text on a diagnostics line, cut on a character boundary.
const MAX_CAUSE_BYTES_V1: usize = 512;

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

/// A versioned neutral action mapping, not the legacy policy's action space.
/// Each candidate has one deterministic legal completion, scored as the sum
/// of its native categorical log probabilities. No post-declaration view is
/// allowed in these trajectories; the bridge enforces the preview boundary.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletionProposalRequestV1 {
    schema: String,
    request_id: String,
    game_id: String,
    seat: String,
    #[allow(dead_code)]
    step: u64,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    row_candidate_ids: Vec<u32>,
    sample_seed: Option<u64>,
    #[serde(default)]
    tensor_encoding: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    proposals_zlib: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    proposals: Option<Vec<Vec<CompletionProposalStepV1>>>,
}

fn deserialize_present<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(deserializer: D)
    -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletionProposalStepV1 {
    tensor: ProposalTensorBitsV1,
    selected_row: usize,
}

fn decode_proposal_payload(encoded: &str, max_bytes: usize) -> Result<Vec<u8>, &'static str> {
    let compressed = base64::engine::general_purpose::STANDARD.decode(encoded)
        .map_err(|_| "proposal_compression")?;
    let mut decoder = flate2::Decompress::new(true);
    let mut decoded = Vec::new();
    let mut offset = 0;
    loop {
        let mut chunk = [0_u8; 8192];
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder.decompress(&compressed[offset..], &mut chunk, flate2::FlushDecompress::None)
            .map_err(|_| "proposal_compression")?;
        let consumed = (decoder.total_in() - before_in) as usize;
        let written = (decoder.total_out() - before_out) as usize;
        offset += consumed;
        if written > max_bytes.saturating_sub(decoded.len()) {
            return Err("proposal_compression");
        }
        decoded.extend_from_slice(&chunk[..written]);
        if status == flate2::Status::StreamEnd {
            if offset != compressed.len() {
                return Err("proposal_compression");
            }
            break;
        }
        if consumed == 0 && written == 0 {
            return Err("proposal_compression");
        }
    }
    Ok(decoded)
}

#[derive(Debug, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum ProposalWordsV1<T> {
    Plain(Vec<T>),
    Runs { length: usize, rle: Vec<(T, usize)> },
}

impl<T: Clone> ProposalWordsV1<T> {
    fn expand(self, allow_runs: bool, remaining: &mut usize) -> Result<Vec<T>, &'static str> {
        let length = match &self {
            Self::Plain(words) => words.len(),
            Self::Runs { length, .. } if allow_runs => *length,
            _ => return Err("proposal_tensor_encoding"),
        };
        // Bound expanded allocation before reserving it. Across a request,
        // at most 64M words are scored, one tensor at a time.
        if length > 1024 * 1024 || length > *remaining {
            return Err("proposal_tensor_size");
        }
        *remaining -= length;
        match self {
            Self::Plain(words) => Ok(words),
            Self::Runs { length, rle } => {
                let mut words = Vec::with_capacity(length);
                for (word, count) in rle {
                    let next = words.len().checked_add(count).ok_or("proposal_tensor_size")?;
                    if count == 0 || next > length {
                        return Err("proposal_tensor_encoding");
                    }
                    words.resize(next, word);
                }
                if words.len() != length {
                    return Err("proposal_tensor_encoding");
                }
                Ok(words)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalTensorBitsV1 {
    state: ProposalWordsV1<u32>,
    object_features: ProposalWordsV1<u32>,
    object_card_ids: ProposalWordsV1<i64>,
    object_groups: ProposalWordsV1<i64>,
    object_node_ids: ProposalWordsV1<i64>,
    edge_features: ProposalWordsV1<u32>,
    edge_source_indices: ProposalWordsV1<i64>,
    edge_target_indices: ProposalWordsV1<i64>,
    action_features: ProposalWordsV1<u32>,
    action_ref_features: ProposalWordsV1<u32>,
    action_ref_card_ids: ProposalWordsV1<i64>,
    action_ref_action_indices: ProposalWordsV1<i64>,
    action_ref_node_indices: ProposalWordsV1<i64>,
}

impl ProposalTensorBitsV1 {
    fn expand(self, allow_runs: bool, remaining: &mut usize) -> Result<ObservedTensorBitsV1, &'static str> {
        Ok(ObservedTensorBitsV1 {
            state: self.state.expand(allow_runs, remaining)?,
            object_features: self.object_features.expand(allow_runs, remaining)?,
            object_card_ids: self.object_card_ids.expand(allow_runs, remaining)?,
            object_groups: self.object_groups.expand(allow_runs, remaining)?,
            object_node_ids: self.object_node_ids.expand(allow_runs, remaining)?,
            edge_features: self.edge_features.expand(allow_runs, remaining)?,
            edge_source_indices: self.edge_source_indices.expand(allow_runs, remaining)?,
            edge_target_indices: self.edge_target_indices.expand(allow_runs, remaining)?,
            action_features: self.action_features.expand(allow_runs, remaining)?,
            action_ref_features: self.action_ref_features.expand(allow_runs, remaining)?,
            action_ref_card_ids: self.action_ref_card_ids.expand(allow_runs, remaining)?,
            action_ref_action_indices: self.action_ref_action_indices.expand(allow_runs, remaining)?,
            action_ref_node_indices: self.action_ref_node_indices.expand(allow_runs, remaining)?,
        })
    }
}

fn categorical_log_probability(logits: &[f32], selected: usize) -> Result<f64, &'static str> {
    if selected >= logits.len() || logits.iter().any(|value| !value.is_finite()) {
        return Err("invalid_model_output");
    }
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    let sum = logits.iter().map(|value| (*value as f64 - maximum).exp()).sum::<f64>();
    Ok(logits[selected] as f64 - maximum - sum.ln())
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

/// The canonical name of a supported selection, or `None`.
fn selection_v1(selection: &str) -> Option<&'static str> {
    match selection {
        SELECTION_SAMPLED_WIDE_V1 => Some(SELECTION_SAMPLED_WIDE_V1),
        SELECTION_ARGMAX_FIRST_V1 => Some(SELECTION_ARGMAX_FIRST_V1),
        _ => None,
    }
}

pub(crate) struct SpellbenchScorerV1 {
    policy: FrozenPlayPolicyV1,
    /// The loaded policy's own feature identity (V4 only): the ready record
    /// stamps it and every request must carry it.
    feature_identity: FreshFeatureIdentityV1,
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
        let selection = selection_v1(selection).ok_or("selection")?;
        let feature_identity = policy.feature_identity_v1();
        if feature_identity.generation != FreshLineageGenerationV1::V4 {
            return Err("model_generation_unsupported");
        }
        Ok(Self {
            policy,
            feature_identity,
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
            "feature_contract_digest": self.feature_identity.feature_contract_digest,
            "feature_encoding_digest": self.feature_identity.feature_encoding_digest,
            "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
            "selection": self.selection,
            "sampler_identity": WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
            "float_encoding": "ieee754-binary32-u32-bits",
            "max_request_bytes": MAX_SCORER_REQUEST_BYTES_V1,
            "game_state_owned": false,
        })
    }

    /// Scores one request line, writing a rejection's cause to stderr. The
    /// in-crate entry point for tests (here and in the replay verifier);
    /// the binary's path passes its diagnostics explicitly.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn score_line(&mut self, bytes: &[u8]) -> Result<Value, &'static str> {
        self.score_line_with_diagnostics(bytes, &mut stderr_diagnostics())
    }

    /// `score_line`, writing the cause of a rejection to `diagnostics`.
    fn score_line_with_diagnostics(
        &mut self,
        bytes: &[u8],
        diagnostics: &mut dyn Write,
    ) -> Result<Value, &'static str> {
        let header: Value = serde_json::from_slice(bytes)
            .map_err(|error| reject(diagnostics, "request_json", error))?;
        if header.get("schema").and_then(Value::as_str) == Some(SCORER_PROPOSAL_SCHEMA_V1) {
            return self.score_proposals(bytes, diagnostics);
        }
        let request: SpellbenchScorerRequestV1 = serde_json::from_slice(bytes)
            .map_err(|error| reject(diagnostics, "request_json", error))?;
        if request.schema != SCORER_REQUEST_SCHEMA_V1 {
            let cause = format!("schema {:?}", request.schema);
            return Err(reject(diagnostics, "request_schema", cause));
        }
        if request.feature_contract_digest != self.feature_identity.feature_contract_digest
            || request.feature_encoding_digest != self.feature_identity.feature_encoding_digest
        {
            let cause = format!(
                "contract {:?}, encoding {:?}",
                request.feature_contract_digest, request.feature_encoding_digest
            );
            return Err(reject(diagnostics, "request_feature_contract", cause));
        }
        if request.request_id.is_empty()
            || request.game_id.is_empty()
            || !matches!(request.seat.as_str(), "p0" | "p1")
        {
            let cause = format!(
                "request_id {:?}, game_id {:?}, seat {:?}",
                request.request_id, request.game_id, request.seat
            );
            return Err(reject(diagnostics, "request_identity", cause));
        }
        let rows = request.row_candidate_ids.len();
        let distinct = request
            .row_candidate_ids
            .iter()
            .collect::<BTreeSet<_>>()
            .len();
        if rows == 0 || distinct != rows {
            let cause = format!("{rows} ids, {distinct} distinct");
            return Err(reject(diagnostics, "row_candidate_ids", cause));
        }
        let seed = match (self.selection, request.sample_seed) {
            (SELECTION_SAMPLED_WIDE_V1, Some(seed)) => Some(seed),
            (SELECTION_ARGMAX_FIRST_V1, None) => None,
            (selection, seed) => {
                let cause = format!("selection {selection} with sample_seed {seed:?}");
                return Err(reject(diagnostics, "sample_seed", cause));
            }
        };
        let output = self
            .policy
            .score_training_tensor_v4(&into_tensor_v4(request.tensor))
            .map_err(|error| reject(diagnostics, "native_inference", error))?;
        if output.logits.len() != rows {
            let cause = format!("{} logits for {rows} rows", output.logits.len());
            return Err(reject(diagnostics, "row_count", cause));
        }
        if !output.value.is_finite() || output.logits.iter().any(|value| !value.is_finite()) {
            let cause = "a logit or the value is not finite";
            return Err(reject(diagnostics, "invalid_model_output", cause));
        }
        let selected = match seed {
            Some(seed) => self
                .sampler
                .sample(&output.logits, seed)
                .map_err(|error| reject(diagnostics, "sampler", error))?,
            None => first_argmax(&output.logits)
                .ok_or_else(|| reject(diagnostics, "invalid_model_output", "no finite maximum"))?,
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

    fn score_proposals(&mut self, bytes: &[u8], diagnostics: &mut dyn Write) -> Result<Value, &'static str> {
        let request: CompletionProposalRequestV1 = serde_json::from_slice(bytes)
            .map_err(|error| reject(diagnostics, "request_json", error))?;
        let proposals = match (request.proposals, request.proposals_zlib) {
            (Some(proposals), None) => proposals,
            (None, Some(encoded)) => {
            let decoded = decode_proposal_payload(&encoded, 64 * 1024 * 1024)
                .map_err(|code| reject(diagnostics, code, "invalid bounded zlib payload"))?;
            serde_json::from_slice(&decoded)
                .map_err(|error| reject(diagnostics, "request_json", error))?
            }
            _ => return Err(reject(diagnostics, "proposal_compression", "requires exactly one proposal representation")),
        };
        if request.schema != SCORER_PROPOSAL_SCHEMA_V1 || request.request_id.is_empty() || request.game_id.is_empty()
            || !matches!(request.seat.as_str(), "p0" | "p1") {
            return Err(reject(diagnostics, "request_identity", "invalid proposal identity"));
        }
        if request.feature_contract_digest != self.feature_identity.feature_contract_digest
            || request.feature_encoding_digest != self.feature_identity.feature_encoding_digest {
            return Err(reject(diagnostics, "request_feature_contract", "different proposal feature contract"));
        }
        let rows = request.row_candidate_ids.len();
        if rows == 0 || rows > 4096 || proposals.len() != rows
            || request.row_candidate_ids.iter().collect::<BTreeSet<_>>().len() != rows
            || proposals.iter().any(|steps| steps.is_empty() || steps.len() > 4096) {
            return Err(reject(diagnostics, "row_candidate_ids", "invalid proposal dimensions"));
        }
        let seed = match (self.selection, request.sample_seed) {
            (SELECTION_SAMPLED_WIDE_V1, Some(seed)) => Some(seed),
            (SELECTION_ARGMAX_FIRST_V1, None) => None,
            _ => return Err(reject(diagnostics, "sample_seed", "different proposal selection")),
        };
        let allow_runs = match request.tensor_encoding.as_deref() {
            None => false,
            Some("integer-rle/v1") => true,
            _ => return Err(reject(diagnostics, "proposal_tensor_encoding", "unknown tensor encoding")),
        };
        let mut remaining_words = 64 * 1024 * 1024;
        let mut logits = Vec::with_capacity(rows);
        for steps in proposals {
            let mut score = 0.0_f64;
            for step in steps {
                let tensor = step.tensor.expand(allow_runs, &mut remaining_words)
                    .map_err(|code| reject(diagnostics, code, "invalid bounded proposal tensor"))?;
                let output = self.policy.score_training_tensor_v4(&into_tensor_v4(tensor))
                    .map_err(|error| reject(diagnostics, "native_inference", error))?;
                score += categorical_log_probability(&output.logits, step.selected_row)
                    .map_err(|code| reject(diagnostics, code, "invalid proposal categorical row"))?;
            }
            let score = score as f32;
            if !score.is_finite() {
                return Err(reject(diagnostics, "invalid_model_output", "nonfinite proposal score"));
            }
            logits.push(score);
        }
        let selected = match seed {
            Some(seed) => self.sampler.sample(&logits, seed)
                .map_err(|error| reject(diagnostics, "sampler", error))?,
            None => first_argmax(&logits).ok_or("invalid_model_output")?,
        };
        Ok(json!({"schema": SCORER_CHOICE_SCHEMA_V1, "mapping": "deterministic-completion-logprob/v1",
            "request_id": request.request_id, "request_sha256": format!("{:x}", Sha256::digest(bytes)),
            "model_state_sha256": self.model_state_sha256, "selection": self.selection,
            "logits_bits": logits.iter().map(|value| value.to_bits()).collect::<Vec<_>>(),
            "value_bits": 0.0_f32.to_bits(), "selected_row": selected,
            "selected_candidate_id": request.row_candidate_ids[selected]}))
    }
}

/// Writes one diagnostics line, `spellbench scorer: <code>: <cause>`, and
/// returns `code`. Line breaks in the cause become spaces and it is cut to
/// `MAX_CAUSE_BYTES_V1`. The binary's diagnostics go to stderr; stdout
/// records carry only the code. Best effort: a failed diagnostics write
/// never changes what goes to stdout.
fn reject(diagnostics: &mut dyn Write, code: &'static str, cause: impl Display) -> &'static str {
    let mut cause = cause.to_string().replace(['\r', '\n'], " ");
    if cause.len() > MAX_CAUSE_BYTES_V1 {
        let mut end = MAX_CAUSE_BYTES_V1;
        while !cause.is_char_boundary(end) {
            end -= 1;
        }
        cause.truncate(end);
    }
    let line = format!("spellbench scorer: {code}: {cause}\n");
    let _ = diagnostics.write_all(line.as_bytes());
    let _ = diagnostics.flush();
    code
}

/// The public entry points' diagnostics: stderr, written directly so a
/// closed stderr cannot panic the scorer.
#[cfg(not(test))]
fn stderr_diagnostics() -> std::io::Stderr {
    std::io::stderr()
}

/// Under libtest the same lines go through `eprint!`, which keeps them in
/// each test's captured output instead of the console.
#[cfg(test)]
fn stderr_diagnostics() -> CapturedStderrV1 {
    CapturedStderrV1
}

#[cfg(test)]
struct CapturedStderrV1;

#[cfg(test)]
impl Write for CapturedStderrV1 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        eprint!("{}", String::from_utf8_lossy(bytes));
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// One bounded JSONL record; a trailing `\r` is dropped (xmage framing).
fn read_record<R: BufRead>(
    reader: &mut R,
    limit: usize,
    diagnostics: &mut dyn Write,
) -> Result<Option<Vec<u8>>, &'static str> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .map_err(|error| reject(diagnostics, "input_io", error))?;
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
            let cause = format!("a record exceeds {limit} bytes");
            return Err(reject(diagnostics, "request_too_large", cause));
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

fn write_record<W: Write>(
    writer: &mut W,
    value: &Value,
    diagnostics: &mut dyn Write,
) -> Result<(), String> {
    if let Err(error) = serde_json::to_writer(&mut *writer, value) {
        return Err(reject(diagnostics, "output_json", error).into());
    }
    if let Err(error) = writer.write_all(b"\n").and_then(|()| writer.flush()) {
        return Err(reject(diagnostics, "output_io", error).into());
    }
    Ok(())
}

fn error_record(code: &str) -> Value {
    json!({"schema": SCORER_ERROR_SCHEMA_V1, "code": code, "service_continues": false})
}

/// Writes the cause to diagnostics and the one error record for `code` to
/// `writer`, and ends the service with that code.
fn refuse<W: Write, T>(
    writer: &mut W,
    diagnostics: &mut dyn Write,
    code: &'static str,
    cause: impl Display,
) -> Result<T, String> {
    reject(diagnostics, code, cause);
    write_record(writer, &error_record(code), diagnostics)?;
    Err(code.into())
}

/// Answers request lines until EOF or the first rejection, writing causes to
/// stderr. Like `score_line`, an in-crate entry point for tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn serve_scorer_v1<R: BufRead, W: Write>(
    scorer: &mut SpellbenchScorerV1,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    serve_with_diagnostics_v1(scorer, reader, writer, &mut stderr_diagnostics())
}

fn serve_with_diagnostics_v1<R: BufRead, W: Write>(
    scorer: &mut SpellbenchScorerV1,
    reader: &mut R,
    writer: &mut W,
    diagnostics: &mut dyn Write,
) -> Result<(), String> {
    loop {
        // Rejections below have already written their cause to diagnostics.
        let result = match read_record(reader, MAX_SCORER_REQUEST_BYTES_V1, diagnostics) {
            Ok(None) => return Ok(()),
            Ok(Some(bytes)) => scorer.score_line_with_diagnostics(&bytes, diagnostics),
            Err(code) => Err(code),
        };
        match result {
            Ok(value) => write_record(writer, &value, diagnostics)?,
            Err(code) => {
                write_record(writer, &error_record(code), diagnostics)?;
                return Err(code.into());
            }
        }
    }
}

/// Every failure before the ready record, including an unknown selection
/// (checked before the model load), emits one error record.
pub fn run_spellbench_scorer_v1<R: BufRead, W: Write>(
    config: SpellbenchScorerConfigV1,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    run_with_diagnostics_v1(config, reader, writer, &mut stderr_diagnostics())
}

fn run_with_diagnostics_v1<R: BufRead, W: Write>(
    config: SpellbenchScorerConfigV1,
    reader: &mut R,
    writer: &mut W,
    diagnostics: &mut dyn Write,
) -> Result<(), String> {
    if config.schema != SCORER_CONFIG_SCHEMA_V1 {
        let cause = format!("schema {:?}", config.schema);
        return refuse(writer, diagnostics, "config_schema", cause);
    }
    let Some(selection) = selection_v1(&config.selection) else {
        let cause = format!("selection {:?}", config.selection);
        return refuse(writer, diagnostics, "selection", cause);
    };
    let (policy, identity) = match load_expanded_inference_v1(&config.source) {
        Ok(loaded) => loaded,
        Err(message) => return refuse(writer, diagnostics, "checkpoint_load", message),
    };
    let model = match serde_json::to_value(&identity) {
        Ok(model) => model,
        Err(error) => return refuse(writer, diagnostics, "identity_json", error),
    };
    let mut scorer =
        match SpellbenchScorerV1::new(policy, model, identity.state_sha256.clone(), selection) {
            Ok(scorer) => scorer,
            Err(code) => {
                let cause = "this scorer serves fresh-lineage V4 policies only";
                return refuse(writer, diagnostics, code, cause);
            }
        };
    write_record(writer, &scorer.ready_record(), diagnostics)?;
    serve_with_diagnostics_v1(&mut scorer, reader, writer, diagnostics)
}

/// As `run_spellbench_scorer_v1`; a config that cannot be opened, read or
/// parsed, or exceeds 1 MiB, also emits one error record.
pub fn run_spellbench_scorer_config_path_v1<R: BufRead, W: Write>(
    path: &Path,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    run_config_path_with_diagnostics_v1(path, reader, writer, &mut stderr_diagnostics())
}

fn run_config_path_with_diagnostics_v1<R: BufRead, W: Write>(
    path: &Path,
    reader: &mut R,
    writer: &mut W,
    diagnostics: &mut dyn Write,
) -> Result<(), String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            let cause = format!("{}: {error}", path.display());
            return refuse(writer, diagnostics, "config_open", cause);
        }
    };
    let mut bytes = Vec::new();
    if let Err(error) = file.take(MAX_CONFIG_BYTES_V1 + 1).read_to_end(&mut bytes) {
        let cause = format!("{}: {error}", path.display());
        return refuse(writer, diagnostics, "config_read", cause);
    }
    if bytes.len() as u64 > MAX_CONFIG_BYTES_V1 {
        let cause = format!("{} exceeds {MAX_CONFIG_BYTES_V1} bytes", path.display());
        return refuse(writer, diagnostics, "config_too_large", cause);
    }
    let config = match serde_json::from_slice(&bytes) {
        Ok(config) => config,
        Err(error) => {
            let cause = format!("{}: {error}", path.display());
            return refuse(writer, diagnostics, "config_json", cause);
        }
    };
    run_with_diagnostics_v1(config, reader, writer, diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_flat_tensorizer_v4::{
        FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4,
    };
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    use crate::state::SplitMix64;
    use std::io::Cursor;

    #[test]
    fn completion_logprob_is_shift_invariant_and_validates_rows() {
        let score = categorical_log_probability(&[0.0, 0.0], 1).unwrap();
        assert!((score + 2.0_f64.ln()).abs() < 1e-12);
        assert_eq!(score, categorical_log_probability(&[19.0, 19.0], 1).unwrap());
        assert!(categorical_log_probability(&[f32::NAN], 0).is_err());
        assert!(categorical_log_probability(&[0.0], 1).is_err());
    }

    #[test]
    fn completion_proposals_use_frozen_logits_and_native_sampler() {
        let (_, logits, _, bytes) = live_case(Some(123));
        let request: Value = serde_json::from_slice(&bytes).unwrap();
        let tensor = request["tensor"].clone();
        let probabilities: Vec<f32> = (0..logits.len())
            .map(|row| categorical_log_probability(&logits, row).unwrap() as f32).collect();
        let proposals: Vec<Value> = (0..logits.len())
            .map(|row| json!([{"tensor": tensor, "selected_row": row}])).collect();
        let rows: Vec<u32> = (0..logits.len() as u32).collect();
        let proposal = json!({"schema": SCORER_PROPOSAL_SCHEMA_V1, "request_id": "proposal-1",
            "game_id": "game", "seat": "p0", "step": 0,
            "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4,
            "sample_seed": 123, "row_candidate_ids": rows, "proposals": proposals});
        let result = scorer(SELECTION_SAMPLED_WIDE_V1).score_line(&serde_json::to_vec(&proposal).unwrap()).unwrap();
        let expected = WideCategoricalScratchV1::default().sample(&probabilities, 123).unwrap();
        assert_eq!(result["selected_row"], expected);
        assert_eq!(result["logits_bits"], json!(probabilities.iter().map(|value| value.to_bits()).collect::<Vec<_>>()));
        assert_eq!(result["mapping"], "deterministic-completion-logprob/v1");

        let mut packed = proposal.clone();
        packed["tensor_encoding"] = json!("integer-rle/v1");
        for steps in packed["proposals"].as_array_mut().unwrap() {
            for step in steps.as_array_mut().unwrap() {
                for words in step["tensor"].as_object_mut().unwrap().values_mut() {
                    let array = words.as_array().unwrap();
                    let mut runs: Vec<(Value, usize)> = Vec::new();
                    for value in array {
                        if let Some((_, count)) = runs.last_mut().filter(|(previous, _)| previous == value) {
                            *count += 1;
                        } else {
                            runs.push((value.clone(), 1));
                        }
                    }
                    *words = json!({"length": array.len(), "rle": runs});
                }
            }
        }
        let packed_result = scorer(SELECTION_SAMPLED_WIDE_V1).score_line(&serde_json::to_vec(&packed).unwrap()).unwrap();
        for field in ["logits_bits", "value_bits", "selected_row", "selected_candidate_id", "mapping"] {
            assert_eq!(packed_result[field], result[field], "lossless encoding changed {field}");
        }
        let mut compressed = packed.clone();
        let payload = serde_json::to_vec(&compressed.as_object_mut().unwrap().remove("proposals").unwrap()).unwrap();
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&payload).unwrap();
        compressed["proposals_zlib"] = json!(base64::engine::general_purpose::STANDARD.encode(encoder.finish().unwrap()));
        let compressed_result = scorer(SELECTION_SAMPLED_WIDE_V1).score_line(&serde_json::to_vec(&compressed).unwrap()).unwrap();
        for field in ["logits_bits", "value_bits", "selected_row", "selected_candidate_id", "mapping"] {
            assert_eq!(compressed_result[field], result[field], "zlib encoding changed {field}");
        }
        compressed["proposals"] = json!([]);
        assert_eq!(scorer(SELECTION_SAMPLED_WIDE_V1).score_line(&serde_json::to_vec(&compressed).unwrap()),
            Err("proposal_compression"));
        compressed["proposals"] = Value::Null;
        assert_eq!(scorer(SELECTION_SAMPLED_WIDE_V1).score_line(&serde_json::to_vec(&compressed).unwrap()),
            Err("request_json"));
        packed.as_object_mut().unwrap().remove("tensor_encoding");
        assert_eq!(scorer(SELECTION_SAMPLED_WIDE_V1).score_line(&serde_json::to_vec(&packed).unwrap()),
            Err("proposal_tensor_encoding"));
    }

    #[test]
    fn proposal_zlib_rejects_invalid_trailing_and_oversized_data() {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&[42; 100]).unwrap();
        let mut bytes = encoder.finish().unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        assert_eq!(decode_proposal_payload(&encoded, 100).unwrap(), vec![42; 100]);
        assert_eq!(decode_proposal_payload(&encoded, 99), Err("proposal_compression"));
        assert_eq!(decode_proposal_payload("bad-base64!", 100), Err("proposal_compression"));
        for missing in [1, 2, 4, bytes.len() / 2] {
            assert_eq!(decode_proposal_payload(&base64::engine::general_purpose::STANDARD.encode(&bytes[..bytes.len() - missing]), 100),
                Err("proposal_compression"));
        }
        bytes.push(0);
        assert_eq!(decode_proposal_payload(&base64::engine::general_purpose::STANDARD.encode(&bytes), 100),
            Err("proposal_compression"));
    }

    #[test]
    fn proposal_rle_preserves_u32_bits_and_rejects_invalid_expansion() {
        let mut remaining = 12;
        assert_eq!(ProposalWordsV1::Runs { length: 5,
            rle: vec![(0, 2), (0x80000000, 1), (0x7fc00001, 1), (u32::MAX, 1)] }
            .expand(true, &mut remaining).unwrap(), vec![0, 0, 0x80000000, 0x7fc00001, u32::MAX]);
        assert_eq!(remaining, 7);
        for (length, rle, error) in [
            (1, vec![(0, 0)], "proposal_tensor_encoding"),
            (1, vec![(0, 2)], "proposal_tensor_encoding"),
            (2, vec![(0, 1)], "proposal_tensor_encoding"),
            (8, vec![(0, 8)], "proposal_tensor_size"),
        ] {
            assert_eq!(ProposalWordsV1::Runs { length, rle }.expand(true, &mut 7), Err(error));
        }
    }

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

    /// The first `count` live Rally decisions with at least two candidates
    /// (the game takes row 0 at every decision in between).
    fn live_sessions(count: usize) -> Vec<FastActorSessionV1> {
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            4242,
            17,
            400,
            40_000,
            ["Rally".to_string(), "Rally".to_string()],
        )
        .unwrap();
        let mut found = Vec::new();
        while found.len() < count {
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("game ended")
            };
            if decision.legal_action_count >= 2 {
                found.push(session.clone());
            }
            session.step(decision.episode_id, decision.step, 0).unwrap();
        }
        found
    }

    /// The harness's logits on `session` and a scorer request for it with a
    /// reversed row map.
    fn request_for(
        session: &FastActorSessionV1,
        sample_seed: Option<u64>,
    ) -> (Vec<f32>, Vec<u32>, Vec<u8>) {
        let mut harness = FrozenPlayPolicyV1::training_fixture_v4();
        let scores = harness.score_fast_session_v1(session).unwrap();
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
        (scores.logits, rows, serde_json::to_vec(&request).unwrap())
    }

    /// The first live decision with at least two candidates, the harness's
    /// logits on it, and a scorer request with a reversed row map.
    fn live_case(sample_seed: Option<u64>) -> (FastActorSessionV1, Vec<f32>, Vec<u32>, Vec<u8>) {
        let session = live_sessions(1).remove(0);
        let (logits, rows, bytes) = request_for(&session, sample_seed);
        (session, logits, rows, bytes)
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
        // 64 seat seeds: the original pin, then 63 more from its stream.
        let first = 6_130_830_677_676_653_025_u64;
        let mut stream = SplitMix64::seed(first);
        let seat_seeds: Vec<u64> = std::iter::once(first)
            .chain((1..64).map(|_| stream.next_u64()))
            .collect();
        let mut scorer = scorer(SELECTION_SAMPLED_WIDE_V1);
        let mut harness = FrozenPlayPolicyV1::training_fixture_v4();
        let mut rows_ever_differ = false;
        for (decision, session) in live_sessions(3).iter().enumerate() {
            let (_, rows, bytes) = request_for(session, None);
            let mut request: Value = serde_json::from_slice(&bytes).unwrap();
            let mut selected = BTreeSet::new();
            for &seat_seed in &seat_seeds {
                // The bot sends the first draw of the acting seat's stream.
                request["sample_seed"] = json!(SplitMix64::seed(seat_seed).next_u64());
                let choice = scorer
                    .score_line(&serde_json::to_vec(&request).unwrap())
                    .unwrap();
                harness.reset_sampling_v1([seat_seed, seat_seed]);
                let harness_row = harness.select_fast_session_v1(session).unwrap() as usize;
                assert_eq!(
                    choice["selected_row"],
                    json!(harness_row),
                    "decision {decision}, seat seed {seat_seed}"
                );
                assert_eq!(choice["selected_candidate_id"], json!(rows[harness_row]));
                selected.insert(harness_row);
            }
            rows_ever_differ |= selected.len() > 1;
        }
        assert!(
            rows_ever_differ,
            "every seed chose the same row, so the comparison proves nothing about sampling"
        );
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

    /// A named in-place edit of a request's JSON.
    type Corruption<'a> = (&'a str, &'a dyn Fn(&mut Value));

    /// `bytes` with `edit` applied to its JSON.
    fn edited(bytes: &[u8], edit: &dyn Fn(&mut Value)) -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(bytes).unwrap();
        edit(&mut value);
        serde_json::to_vec(&value).unwrap()
    }

    #[test]
    fn a_request_with_another_schema_or_an_empty_or_unknown_identity_is_refused() {
        let (_, _, _, bytes) = live_case(Some(7));
        let mut scorer = scorer(SELECTION_SAMPLED_WIDE_V1);
        assert!(
            scorer.score_line(&bytes).is_ok(),
            "the unedited request scores"
        );
        assert_eq!(
            scorer.score_line(&edited(&bytes, &|v| v["schema"] =
                json!("mtg-kernel-spellbench-scorer-request/v2"))),
            Err("request_schema")
        );
        for (field, bad) in [
            ("request_id", json!("")),
            ("game_id", json!("")),
            ("seat", json!("p2")),
        ] {
            assert_eq!(
                scorer.score_line(&edited(&bytes, &|v| v[field] = bad.clone())),
                Err("request_identity"),
                "{field}"
            );
        }
    }

    #[test]
    fn a_tensor_the_forward_refuses_is_a_native_inference_rejection() {
        let (_, _, _, bytes) = live_case(Some(7));
        let mut scorer = scorer(SELECTION_SAMPLED_WIDE_V1);
        let corruptions: [Corruption<'_>; 3] = [
            ("state one value short", &|v| {
                v["tensor"]["state"].as_array_mut().unwrap().pop();
            }),
            ("NaN state bits", &|v| {
                v["tensor"]["state"][0] = json!(f32::NAN.to_bits())
            }),
            ("negative object group", &|v| {
                v["tensor"]["object_groups"][0] = json!(-1)
            }),
        ];
        for (name, corrupt) in corruptions {
            assert_eq!(
                scorer.score_line(&edited(&bytes, corrupt)),
                Err("native_inference"),
                "{name}"
            );
        }
    }

    #[test]
    fn a_record_of_exactly_the_limit_is_read_and_one_byte_more_is_refused() {
        // The limit counts every byte before the newline; JSON allows the
        // trailing spaces that pad a real request to the boundary.
        let (_, _, _, bytes) = live_case(Some(9));
        let mut at_limit = bytes.clone();
        at_limit.resize(MAX_SCORER_REQUEST_BYTES_V1, b' ');
        let mut input = at_limit.clone();
        input.push(b'\n');
        input.extend_from_slice(&at_limit);
        input.extend_from_slice(b" \n");
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();
        let result = serve_with_diagnostics_v1(
            &mut scorer(SELECTION_SAMPLED_WIDE_V1),
            &mut std::io::BufReader::with_capacity(64 * 1024, Cursor::new(input)),
            &mut output,
            &mut diagnostics,
        );
        assert_eq!(result, Err("request_too_large".to_string()));
        let lines = records(&output);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["schema"], SCORER_CHOICE_SCHEMA_V1);
        assert_eq!(
            lines[0]["request_sha256"],
            format!("{:x}", Sha256::digest(&at_limit))
        );
        assert_eq!(lines[1], error("request_too_large"));
        assert_eq!(
            only_cause(&diagnostics, "request_too_large"),
            format!("a record exceeds {MAX_SCORER_REQUEST_BYTES_V1} bytes")
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

    /// A well-formed config whose model files do not exist.
    fn missing_model_config() -> Value {
        json!({
            "schema": SCORER_CONFIG_SCHEMA_V1, "selection": SELECTION_SAMPLED_WIDE_V1,
            "source": {
                "play_import": {"path": "E:/does-not-exist/descriptor.json", "sha256": "0".repeat(64)},
                "feature_transfer": {
                    "expected_feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
                    "expected_feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4
                },
                "checkpoint": null
            }
        })
    }

    /// A fresh directory under the system temp dir.
    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mtg-kernel-spellbench-scorer-v1-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// Every JSONL record written to `output`.
    fn records(output: &[u8]) -> Vec<Value> {
        output
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_slice(l).unwrap())
            .collect()
    }

    fn error(code: &str) -> Value {
        json!({"schema": SCORER_ERROR_SCHEMA_V1, "code": code, "service_continues": false})
    }

    #[test]
    fn load_failure_emits_one_error_record_and_stops() {
        let config: SpellbenchScorerConfigV1 =
            serde_json::from_value(missing_model_config()).unwrap();
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
    fn config_failures_emit_one_error_record_and_stop() {
        let dir = scratch_dir("config-failures");
        let unparseable = dir.join("unparseable.json");
        std::fs::write(&unparseable, b"{\"schema\": ").unwrap();
        let too_large = dir.join("too-large.json");
        std::fs::write(&too_large, vec![b' '; MAX_CONFIG_BYTES_V1 as usize + 1]).unwrap();
        for (path, code) in [
            (dir.join("missing.json"), "config_open"),
            (unparseable, "config_json"),
            (too_large, "config_too_large"),
        ] {
            let mut output = Vec::new();
            let result = run_spellbench_scorer_config_path_v1(
                &path,
                &mut Cursor::new(Vec::new()),
                &mut output,
            );
            assert_eq!(result, Err(code.to_string()), "{code}");
            assert_eq!(records(&output), vec![error(code)], "{code}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The cause on the single diagnostics line a rejection wrote for `code`.
    fn only_cause(diagnostics: &[u8], code: &str) -> String {
        let text = std::str::from_utf8(diagnostics).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1, "{code}: {text:?}");
        let prefix = format!("spellbench scorer: {code}: ");
        assert!(lines[0].starts_with(&prefix), "{code}: {text:?}");
        lines[0][prefix.len()..].to_owned()
    }

    #[test]
    fn request_rejections_write_their_cause_to_diagnostics() {
        let (_, _, _, bytes) = live_case(Some(7));
        let mut scorer = scorer(SELECTION_SAMPLED_WIDE_V1);
        let mut diagnostics = Vec::new();
        scorer
            .score_line_with_diagnostics(&bytes, &mut diagnostics)
            .unwrap();
        assert!(diagnostics.is_empty(), "a scored request writes nothing");

        let mut diagnostics = Vec::new();
        assert_eq!(
            scorer.score_line_with_diagnostics(b"not json", &mut diagnostics),
            Err("request_json")
        );
        assert!(only_cause(&diagnostics, "request_json").contains("line 1 column"));

        let mut short_state: Value = serde_json::from_slice(&bytes).unwrap();
        short_state["tensor"]["state"].as_array_mut().unwrap().pop();
        let mut diagnostics = Vec::new();
        assert_eq!(
            scorer.score_line_with_diagnostics(
                &serde_json::to_vec(&short_state).unwrap(),
                &mut diagnostics
            ),
            Err("native_inference")
        );
        assert!(only_cause(&diagnostics, "native_inference")
            .starts_with("native policy/value v1 error: "));

        // A long cause with line breaks stays one bounded line.
        let mut long_schema: Value = serde_json::from_slice(&bytes).unwrap();
        long_schema["schema"] = json!("x\r\ny".repeat(4_096));
        let mut diagnostics = Vec::new();
        assert_eq!(
            scorer.score_line_with_diagnostics(
                &serde_json::to_vec(&long_schema).unwrap(),
                &mut diagnostics
            ),
            Err("request_schema")
        );
        assert!(only_cause(&diagnostics, "request_schema").len() <= MAX_CAUSE_BYTES_V1);
    }

    #[test]
    fn config_and_load_failures_write_their_cause_to_diagnostics() {
        let dir = scratch_dir("config-causes");
        let unparseable = dir.join("unparseable.json");
        std::fs::write(&unparseable, b"{\"schema\": ").unwrap();
        let mut diagnostics = Vec::new();
        let _ = run_config_path_with_diagnostics_v1(
            &dir.join("missing.json"),
            &mut Cursor::new(Vec::new()),
            &mut Vec::new(),
            &mut diagnostics,
        );
        assert!(!only_cause(&diagnostics, "config_open").is_empty());
        let mut diagnostics = Vec::new();
        let _ = run_config_path_with_diagnostics_v1(
            &unparseable,
            &mut Cursor::new(Vec::new()),
            &mut Vec::new(),
            &mut diagnostics,
        );
        assert!(only_cause(&diagnostics, "config_json").contains("line 1 column"));
        let mut diagnostics = Vec::new();
        let _ = run_with_diagnostics_v1(
            serde_json::from_value(missing_model_config()).unwrap(),
            &mut Cursor::new(Vec::new()),
            &mut Vec::new(),
            &mut diagnostics,
        );
        assert!(!only_cause(&diagnostics, "checkpoint_load").is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn selection_is_refused_before_the_model_load() {
        let mut config = missing_model_config();
        config["selection"] = json!("greedy-v1");
        let mut output = Vec::new();
        let result = run_spellbench_scorer_v1(
            serde_json::from_value(config).unwrap(),
            &mut Cursor::new(Vec::new()),
            &mut output,
        );
        assert_eq!(result, Err("selection".to_string()));
        assert_eq!(records(&output), vec![error("selection")]);
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

    /// Every ready-record field but the model's own identity equals the
    /// literal value Task 5's bot (`kernel_flat_bot.py`) pins or expects.
    fn assert_ready_pins(ready: &Value, selection: &str) {
        let keys: BTreeSet<&str> = ready
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            BTreeSet::from([
                "schema",
                "model",
                "model_state_sha256",
                "feature_contract_digest",
                "feature_encoding_digest",
                "card_db_hash",
                "selection",
                "sampler_identity",
                "float_encoding",
                "max_request_bytes",
                "game_state_owned",
            ])
        );
        let mut pinned = ready.clone();
        let fields = pinned.as_object_mut().unwrap();
        fields.remove("model");
        fields.remove("model_state_sha256");
        assert_eq!(
            pinned,
            json!({
                "schema": "mtg-kernel-spellbench-scorer-ready/v1",
                "feature_contract_digest": "c4af415a3b0cf1e9c9960dbe2bc2d134c63e9f08206a9a364e113121fea5538b",
                "feature_encoding_digest": "271c0e5a0fdce75663c897e89a9d7280ab1a3bbb6679bd10ecb5f524991952de",
                "card_db_hash": "064a7c989255ab3c",
                "selection": selection,
                "sampler_identity": "f32-q8-expq63-hamilton-splitmix64-wide-v1",
                "float_encoding": "ieee754-binary32-u32-bits",
                "max_request_bytes": 8_388_608,
                "game_state_owned": false,
            })
        );
    }

    #[test]
    fn ready_record_carries_exactly_the_values_the_bot_pins() {
        for selection in [SELECTION_SAMPLED_WIDE_V1, SELECTION_ARGMAX_FIRST_V1] {
            assert_ready_pins(&scorer(selection).ready_record(), selection);
        }
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
            assert_eq!(ready["model"]["state_sha256"], state, "{name}");
            assert_ready_pins(&ready, SELECTION_SAMPLED_WIDE_V1);
            if name == "g115" {
                assert_eq!(
                    ready["model"]["model"]["weights_sha256"],
                    "e2ca2f2b5dd750a59e24c71a4bac325ed7449d97b5892a79a80132e45d538333"
                );
            }
        }
    }
}
