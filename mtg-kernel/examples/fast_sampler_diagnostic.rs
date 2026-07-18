//! Privacy-safe candidate-versus-frozen-Decimal diagnostic.

use mtg_kernel::fast_sampler::{
    splitmix64_first, FastCategoricalScratch, FAST_CATEGORICAL_EXP_TABLE_Q63,
    FAST_CATEGORICAL_EXP_TABLE_SHA256, FAST_CATEGORICAL_MASS_TOTAL,
    FAST_CATEGORICAL_SAMPLER_CONTRACT_JSON, FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
    FAST_CATEGORICAL_SAMPLER_VERSION,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::error::Error;

const ORACLE_BYTES: &[u8] = include_bytes!("../../data/fast_sampler_decimal_oracle_v1.json");
const ORACLE_SHA256: &str = "bb42f0cacae9902d67851941678cf2fb34a90cb8459403126a8026085dcae033";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const RALLY_RAW_SOURCE_ARTIFACT_SHA256: &str =
    "682198c7e169a67a2c885dd8362db0c67c329b8cb1e6390f4fbc905c3f9bd7ee";
const RALLY_SOURCE_MANIFEST_SHA256: &str =
    "09e816949de05d76cf37148e015eb973b4f6568e256e755e5b727480df56d9d3";
const RALLY_ENVIRONMENT_BINARY_SHA256: &str =
    "b81b5ad88e6f728922b8635405aead28588066b2563cdd9644439100715d4c51";
const RALLY_BENCHMARK_BINARY_SHA256: &str =
    "04802ed2cb953b6ef0f071f42304221de16fd9f411b8decc025ffbfa56b1fbe8";

#[derive(Deserialize)]
struct OracleFile {
    schema_version: u32,
    oracle_sampler_version: String,
    generator: String,
    workload_width_profile: WorkloadWidthProfile,
    predeclared_candidate_bounds: CandidateBounds,
    independent_rng_and_selection_goldens: IndependentGoldens,
    cases: Vec<OracleCase>,
}

#[derive(Deserialize)]
struct WorkloadWidthProfile {
    status: String,
    claim_eligible: bool,
    scope: String,
    source_artifact: Option<String>,
    source_artifact_sha256: Option<String>,
    source_schema: Option<String>,
    source_aggregate_record_sha256: Option<String>,
    source_provenance_class: Option<String>,
    source_performance_gate_valid: Option<bool>,
    source_performance_rates_included: Option<bool>,
    source_coverage_scope: Option<String>,
    raw_source_artifact_sha256: Option<String>,
    raw_source_artifact_size_bytes: Option<u64>,
    source_head_before: Option<String>,
    source_head_after: Option<String>,
    source_worktree_state_before: Option<String>,
    source_worktree_state_after: Option<String>,
    source_status_sha256_before: Option<String>,
    source_status_sha256_after: Option<String>,
    source_manifest_file_count: Option<u64>,
    source_manifest_sha256_before: Option<String>,
    source_manifest_sha256_after: Option<String>,
    bound_build_source_manifest_sha256_before: Option<String>,
    bound_build_source_manifest_sha256_after: Option<String>,
    source_attestations_stable: Option<bool>,
    environment_binary_sha256_prebuild: Option<String>,
    environment_binary_sha256_postbuild: Option<String>,
    environment_binary_sha256_before: Option<String>,
    environment_binary_sha256_after: Option<String>,
    benchmark_binary_sha256_prebuild: Option<String>,
    benchmark_binary_sha256_postbuild: Option<String>,
    benchmark_binary_sha256_before: Option<String>,
    benchmark_binary_sha256_after: Option<String>,
    binary_attestations_stable: Option<bool>,
    formal_binary_source_attestation_present: Option<bool>,
    compiled_input_closure_attested: Option<bool>,
    statistics: WidthStatistics,
    final_all_nine_deck_gate: String,
}

#[derive(Deserialize)]
struct WidthStatistics {
    sample_count: u64,
    mean: f64,
    nearest_rank_p95: usize,
    maximum: usize,
}

#[derive(Deserialize)]
struct CandidateBounds {
    maximum_total_variation: f64,
    width_profile_weighted_mean_total_variation: f64,
    maximum_legacy_to_candidate_kl_nats: f64,
    aggregate_selected_index_agreement: f64,
}

#[derive(Deserialize)]
struct IndependentGoldens {
    producer: String,
    seed_range: SeedRange,
    splitmix_first_draws: SplitMixDrawGoldens,
    decimal_selected_indices: SelectedIndexGoldens,
}

#[derive(Deserialize)]
struct SeedRange {
    inclusive_start: u64,
    exclusive_end: u64,
}

#[derive(Deserialize)]
struct SplitMixDrawGoldens {
    encoding: String,
    bytes_hex: String,
    sha256: String,
}

#[derive(Deserialize)]
struct SelectedIndexGoldens {
    encoding: String,
    sha256: String,
}

#[derive(Deserialize)]
struct OracleCase {
    name: String,
    classification: String,
    width_profile_count: u64,
    logit_bits_hex: Vec<String>,
    decimal_mass: Vec<String>,
}

#[derive(Clone)]
struct CaseMetric {
    name: String,
    classification: String,
    width_profile_count: u64,
    width: usize,
    total_variation: f64,
    legacy_to_candidate_kl_nats: f64,
    selection_agreement: f64,
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_repo_relative_slash_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && value.split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".." && !part.eq_ignore_ascii_case(".git")
        })
}

fn table_sha256() -> String {
    let mut digest = Sha256::new();
    for value in FAST_CATEGORICAL_EXP_TABLE_Q63 {
        digest.update(value.to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn decode_lower_hex(encoded: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if !encoded.len().is_multiple_of(2)
        || encoded
            .bytes()
            .any(|value| !value.is_ascii_digit() && !(b'a'..=b'f').contains(&value))
    {
        return Err("golden bytes must be lowercase even-length hex".into());
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)?;
            Ok(u8::from_str_radix(text, 16)?)
        })
        .collect()
}

fn parse_python_draws(goldens: &IndependentGoldens) -> Result<Vec<u64>, Box<dyn Error>> {
    if goldens.producer != "independent Python integer implementation in generator"
        || goldens.seed_range.inclusive_start != 0
        || goldens.seed_range.exclusive_end != 4096
        || goldens.splitmix_first_draws.encoding
            != "seed-order little-endian u64 bytes encoded as lowercase hex"
        || !is_lower_sha256(&goldens.splitmix_first_draws.sha256)
        || goldens.decimal_selected_indices.encoding
            != "case-major then seed-major selected index as one u8"
        || !is_lower_sha256(&goldens.decimal_selected_indices.sha256)
    {
        return Err("independent Python golden contract drifted".into());
    }
    let seed_count = goldens
        .seed_range
        .exclusive_end
        .checked_sub(goldens.seed_range.inclusive_start)
        .ok_or("independent golden seed range is reversed")?;
    let bytes = decode_lower_hex(&goldens.splitmix_first_draws.bytes_hex)?;
    if bytes.len()
        != usize::try_from(seed_count)?
            .checked_mul(8)
            .ok_or("draw byte length overflow")?
        || sha256_hex(&bytes) != goldens.splitmix_first_draws.sha256
    {
        return Err("independent Python SplitMix draw bytes or digest drifted".into());
    }
    bytes
        .chunks_exact(8)
        .enumerate()
        .map(|(offset, chunk)| {
            let draw = u64::from_le_bytes(chunk.try_into()?);
            let seed = goldens.seed_range.inclusive_start + u64::try_from(offset)?;
            if splitmix64_first(seed) != draw {
                return Err(format!("Rust SplitMix first draw disagrees at seed {seed}").into());
            }
            Ok(draw)
        })
        .collect()
}

fn parse_case(case: &OracleCase) -> Result<(Vec<f32>, Vec<u128>), Box<dyn Error>> {
    let logits = case
        .logit_bits_hex
        .iter()
        .map(|encoded| {
            if encoded.len() != 8
                || encoded
                    .bytes()
                    .any(|value| !value.is_ascii_digit() && !(b'a'..=b'f').contains(&value))
            {
                return Err("oracle logit must be eight lowercase hex digits");
            }
            let value = f32::from_bits(
                u32::from_str_radix(encoded, 16).map_err(|_| "oracle logit hex is invalid")?,
            );
            if !value.is_finite() {
                return Err("oracle logit must be finite");
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let masses = case
        .decimal_mass
        .iter()
        .map(|encoded| encoded.parse::<u128>())
        .collect::<Result<Vec<_>, _>>()?;
    if !(1..=64).contains(&logits.len()) || logits.len() != masses.len() {
        return Err(format!("oracle case {} has mismatched vector lengths", case.name).into());
    }
    let mass_total = masses
        .iter()
        .try_fold(0_u128, |total, mass| total.checked_add(*mass))
        .ok_or("oracle mass sum overflow")?;
    if mass_total != FAST_CATEGORICAL_MASS_TOTAL {
        return Err(format!("oracle case {} has invalid mass total", case.name).into());
    }
    Ok((logits, masses))
}

fn validate_workload_profile(profile: &WorkloadWidthProfile) -> Result<(), Box<dyn Error>> {
    if profile.statistics.sample_count == 0
        || !profile.statistics.mean.is_finite()
        || profile.statistics.mean < 1.0
        || !(1..=64).contains(&profile.statistics.nearest_rank_p95)
        || !(1..=64).contains(&profile.statistics.maximum)
        || profile.statistics.nearest_rank_p95 > profile.statistics.maximum
        || profile.final_all_nine_deck_gate != "deferred"
    {
        return Err("workload profile aggregate contract drifted".into());
    }
    if profile.claim_eligible {
        let source_artifact = profile
            .source_artifact
            .as_deref()
            .ok_or("observed profile is missing its source artifact")?;
        if profile.status != "observed_provenance_bound"
            || profile.scope != "all_sampled_policy_decisions_in_rally_vs_rally_not_learner_only"
            || !is_repo_relative_slash_path(source_artifact)
            || profile
                .source_artifact_sha256
                .as_deref()
                .is_none_or(|value| !is_lower_sha256(value))
            || profile.source_schema.as_deref()
                != Some("kernel_rally_all_policy_legal_action_width_histogram/v1")
            || profile
                .source_aggregate_record_sha256
                .as_deref()
                .is_none_or(|value| !is_lower_sha256(value))
            || profile.source_provenance_class.as_deref()
                != Some("deterministic_workload_shape_only_not_performance_evidence")
            || profile.source_performance_gate_valid != Some(false)
            || profile.source_performance_rates_included != Some(false)
            || profile.source_coverage_scope.as_deref()
                != Some("rally_vs_rally_only_not_nine_deck_coverage")
            || profile.raw_source_artifact_sha256.as_deref()
                != Some(RALLY_RAW_SOURCE_ARTIFACT_SHA256)
            || profile.raw_source_artifact_size_bytes != Some(64_453)
            || profile.source_head_before.as_deref()
                != Some("d71dca82dfe36292328ecbc4962a0d6764d9ca5c")
            || profile.source_head_after != profile.source_head_before
            || profile.source_worktree_state_before.as_deref() != Some("clean")
            || profile.source_worktree_state_after.as_deref() != Some("clean")
            || profile.source_status_sha256_before.as_deref() != Some(EMPTY_SHA256)
            || profile.source_status_sha256_after != profile.source_status_sha256_before
            || profile.source_manifest_file_count != Some(132)
            || profile.source_manifest_sha256_before.as_deref()
                != Some(RALLY_SOURCE_MANIFEST_SHA256)
            || profile.source_manifest_sha256_after != profile.source_manifest_sha256_before
            || profile.bound_build_source_manifest_sha256_before
                != profile.source_manifest_sha256_before
            || profile.bound_build_source_manifest_sha256_after
                != profile.source_manifest_sha256_before
            || profile.source_attestations_stable != Some(true)
            || profile.environment_binary_sha256_prebuild.as_deref()
                != Some(RALLY_ENVIRONMENT_BINARY_SHA256)
            || profile.environment_binary_sha256_postbuild
                != profile.environment_binary_sha256_prebuild
            || profile.environment_binary_sha256_before
                != profile.environment_binary_sha256_prebuild
            || profile.environment_binary_sha256_after != profile.environment_binary_sha256_prebuild
            || profile.benchmark_binary_sha256_prebuild.as_deref()
                != Some(RALLY_BENCHMARK_BINARY_SHA256)
            || profile.benchmark_binary_sha256_postbuild != profile.benchmark_binary_sha256_prebuild
            || profile.benchmark_binary_sha256_before != profile.benchmark_binary_sha256_prebuild
            || profile.benchmark_binary_sha256_after != profile.benchmark_binary_sha256_prebuild
            || profile.binary_attestations_stable != Some(true)
            || profile.formal_binary_source_attestation_present != Some(false)
            || profile.compiled_input_closure_attested != Some(false)
        {
            return Err("observed workload profile provenance contract drifted".into());
        }
    } else if profile.status != "provisional_synthetic"
        || profile.scope != "provisional_synthetic_not_observed_policy_decisions"
        || profile.source_artifact.is_some()
        || profile.source_artifact_sha256.is_some()
        || profile.source_schema.is_some()
        || profile.source_aggregate_record_sha256.is_some()
        || profile.source_provenance_class.is_some()
        || profile.source_performance_gate_valid.is_some()
        || profile.source_performance_rates_included.is_some()
        || profile.source_coverage_scope.is_some()
        || profile.raw_source_artifact_sha256.is_some()
        || profile.raw_source_artifact_size_bytes.is_some()
        || profile.source_head_before.is_some()
        || profile.source_head_after.is_some()
        || profile.source_worktree_state_before.is_some()
        || profile.source_worktree_state_after.is_some()
        || profile.source_status_sha256_before.is_some()
        || profile.source_status_sha256_after.is_some()
        || profile.source_manifest_file_count.is_some()
        || profile.source_manifest_sha256_before.is_some()
        || profile.source_manifest_sha256_after.is_some()
        || profile.bound_build_source_manifest_sha256_before.is_some()
        || profile.bound_build_source_manifest_sha256_after.is_some()
        || profile.source_attestations_stable.is_some()
        || profile.environment_binary_sha256_prebuild.is_some()
        || profile.environment_binary_sha256_postbuild.is_some()
        || profile.environment_binary_sha256_before.is_some()
        || profile.environment_binary_sha256_after.is_some()
        || profile.benchmark_binary_sha256_prebuild.is_some()
        || profile.benchmark_binary_sha256_postbuild.is_some()
        || profile.benchmark_binary_sha256_before.is_some()
        || profile.benchmark_binary_sha256_after.is_some()
        || profile.binary_attestations_stable.is_some()
        || profile.formal_binary_source_attestation_present.is_some()
        || profile.compiled_input_closure_attested.is_some()
    {
        return Err("nonclaiming workload profile is not explicitly provisional".into());
    }
    Ok(())
}

fn select_mass(masses: &[u128], draw: u64) -> Result<usize, Box<dyn Error>> {
    let mut cumulative = 0_u128;
    for (index, mass) in masses.iter().copied().enumerate() {
        cumulative += mass;
        if u128::from(draw) < cumulative {
            return Ok(index);
        }
    }
    Err("oracle inverse CDF was not total".into())
}

fn probability(mass: u128) -> f64 {
    mass as f64 / FAST_CATEGORICAL_MASS_TOTAL as f64
}

fn compare_case(
    case: &OracleCase,
    python_draws: &[u64],
    scratch: &mut FastCategoricalScratch,
) -> Result<(CaseMetric, Vec<u8>), Box<dyn Error>> {
    let (logits, oracle) = parse_case(case)?;
    let candidate = scratch.apportion(&logits)?.to_vec();
    let absolute_mass_difference = candidate
        .iter()
        .copied()
        .zip(oracle.iter().copied())
        .map(|(left, right)| left.abs_diff(right))
        .sum::<u128>();
    let total_variation = 0.5 * probability(absolute_mass_difference);
    let legacy_to_candidate_kl_nats = oracle
        .iter()
        .copied()
        .zip(candidate.iter().copied())
        .filter(|(legacy, _)| *legacy != 0)
        .map(|(legacy, fast)| {
            let p = probability(legacy);
            let q = probability(fast);
            p * (p / q).ln()
        })
        .sum::<f64>()
        .max(0.0);

    let mut agreement_count = 0_u64;
    let mut python_selected_indices = Vec::with_capacity(python_draws.len());
    for draw in python_draws.iter().copied() {
        let legacy_index = select_mass(&oracle, draw)?;
        let fast_index = select_mass(&candidate, draw)?;
        python_selected_indices.push(u8::try_from(legacy_index)?);
        agreement_count += u64::from(legacy_index == fast_index);
    }

    Ok((
        CaseMetric {
            name: case.name.clone(),
            classification: case.classification.clone(),
            width_profile_count: case.width_profile_count,
            width: logits.len(),
            total_variation,
            legacy_to_candidate_kl_nats,
            selection_agreement: agreement_count as f64 / python_draws.len() as f64,
        },
        python_selected_indices,
    ))
}

fn run() -> Result<(), Box<dyn Error>> {
    let oracle_digest = sha256_hex(ORACLE_BYTES);
    let table_digest = table_sha256();
    let contract_digest = sha256_hex(FAST_CATEGORICAL_SAMPLER_CONTRACT_JSON.as_bytes());
    if oracle_digest != ORACLE_SHA256
        || table_digest != FAST_CATEGORICAL_EXP_TABLE_SHA256
        || contract_digest != FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256
    {
        return Err("a pinned sampler diagnostic digest drifted".into());
    }

    let oracle_file: OracleFile = serde_json::from_slice(ORACLE_BYTES)?;
    if oracle_file.schema_version != 2 {
        return Err("unsupported oracle schema".into());
    }
    validate_workload_profile(&oracle_file.workload_width_profile)?;
    let python_draws = parse_python_draws(&oracle_file.independent_rng_and_selection_goldens)?;
    let mut scratch = FastCategoricalScratch::default();
    let comparisons = oracle_file
        .cases
        .iter()
        .map(|case| compare_case(case, &python_draws, &mut scratch))
        .collect::<Result<Vec<_>, _>>()?;
    let mut python_selected_indices = Vec::new();
    let metrics = comparisons
        .into_iter()
        .map(|(metric, mut selected)| {
            python_selected_indices.append(&mut selected);
            metric
        })
        .collect::<Vec<_>>();
    if sha256_hex(&python_selected_indices)
        != oracle_file
            .independent_rng_and_selection_goldens
            .decimal_selected_indices
            .sha256
    {
        return Err("Rust Decimal-mass selection disagrees with independent Python goldens".into());
    }

    let maximum_total_variation = metrics
        .iter()
        .map(|metric| metric.total_variation)
        .fold(0.0_f64, f64::max);
    let mean_total_variation = metrics
        .iter()
        .map(|metric| metric.total_variation)
        .sum::<f64>()
        / metrics.len() as f64;
    let width_profile_weight = metrics
        .iter()
        .map(|metric| metric.width_profile_count)
        .sum::<u64>();
    if width_profile_weight != oracle_file.workload_width_profile.statistics.sample_count {
        return Err("case weights do not match the width profile sample count".into());
    }
    let width_profile_weighted_mean_total_variation = metrics
        .iter()
        .map(|metric| metric.total_variation * metric.width_profile_count as f64)
        .sum::<f64>()
        / width_profile_weight as f64;
    let maximum_legacy_to_candidate_kl_nats = metrics
        .iter()
        .map(|metric| metric.legacy_to_candidate_kl_nats)
        .fold(0.0_f64, f64::max);
    let mean_legacy_to_candidate_kl_nats = metrics
        .iter()
        .map(|metric| metric.legacy_to_candidate_kl_nats)
        .sum::<f64>()
        / metrics.len() as f64;
    let aggregate_selected_index_agreement = metrics
        .iter()
        .map(|metric| metric.selection_agreement)
        .sum::<f64>()
        / metrics.len() as f64;
    let width_profile_weighted_selected_index_agreement = metrics
        .iter()
        .map(|metric| metric.selection_agreement * metric.width_profile_count as f64)
        .sum::<f64>()
        / width_profile_weight as f64;

    let mut worst = metrics
        .iter()
        .filter(|metric| metric.classification != "width-profile-representative")
        .cloned()
        .collect::<Vec<_>>();
    worst.sort_by(|left, right| {
        right
            .total_variation
            .total_cmp(&left.total_variation)
            .then_with(|| left.name.cmp(&right.name))
    });
    let worst = worst
        .into_iter()
        .take(5)
        .map(|metric| {
            json!({
                "name": metric.name,
                "classification": metric.classification,
                "width": metric.width,
                "total_variation": metric.total_variation,
                "legacy_to_candidate_kl_nats": metric.legacy_to_candidate_kl_nats,
                "selected_index_agreement": metric.selection_agreement,
            })
        })
        .collect::<Vec<_>>();

    let bounds = &oracle_file.predeclared_candidate_bounds;
    let bound_checks = json!({
        "maximum_total_variation": maximum_total_variation <= bounds.maximum_total_variation,
        "width_profile_weighted_mean_total_variation": width_profile_weighted_mean_total_variation <= bounds.width_profile_weighted_mean_total_variation,
        "maximum_legacy_to_candidate_kl_nats": maximum_legacy_to_candidate_kl_nats <= bounds.maximum_legacy_to_candidate_kl_nats,
        "aggregate_selected_index_agreement": aggregate_selected_index_agreement >= bounds.aggregate_selected_index_agreement,
    });
    let diagnostic_checks_passed = bound_checks
        .as_object()
        .ok_or("bound check record is not an object")?
        .values()
        .all(|value| value == &json!(true));
    let canonical_observed_workload_claim_valid = diagnostic_checks_passed
        && oracle_file.workload_width_profile.claim_eligible
        && oracle_file.workload_width_profile.status == "observed_provenance_bound"
        && oracle_file.workload_width_profile.scope
            == "all_sampled_policy_decisions_in_rally_vs_rally_not_learner_only";
    let profile = &oracle_file.workload_width_profile;
    let workload_width_profile = json!({
        "status": &profile.status,
        "claim_eligible": profile.claim_eligible,
        "scope": &profile.scope,
        "source_artifact": &profile.source_artifact,
        "source_artifact_sha256": &profile.source_artifact_sha256,
        "source_schema": &profile.source_schema,
        "source_aggregate_record_sha256": &profile.source_aggregate_record_sha256,
        "source_provenance_class": &profile.source_provenance_class,
        "source_performance_gate_valid": profile.source_performance_gate_valid,
        "source_performance_rates_included": profile.source_performance_rates_included,
        "source_coverage_scope": &profile.source_coverage_scope,
        "raw_source_artifact_sha256": &profile.raw_source_artifact_sha256,
        "raw_source_artifact_size_bytes": profile.raw_source_artifact_size_bytes,
        "source_head_before": &profile.source_head_before,
        "source_head_after": &profile.source_head_after,
        "source_worktree_state_before": &profile.source_worktree_state_before,
        "source_worktree_state_after": &profile.source_worktree_state_after,
        "source_status_sha256_before": &profile.source_status_sha256_before,
        "source_status_sha256_after": &profile.source_status_sha256_after,
        "source_manifest_file_count": profile.source_manifest_file_count,
        "source_manifest_sha256_before": &profile.source_manifest_sha256_before,
        "source_manifest_sha256_after": &profile.source_manifest_sha256_after,
        "bound_build_source_manifest_sha256_before": &profile.bound_build_source_manifest_sha256_before,
        "bound_build_source_manifest_sha256_after": &profile.bound_build_source_manifest_sha256_after,
        "source_attestations_stable": profile.source_attestations_stable,
        "environment_binary_sha256_prebuild": &profile.environment_binary_sha256_prebuild,
        "environment_binary_sha256_postbuild": &profile.environment_binary_sha256_postbuild,
        "environment_binary_sha256_before": &profile.environment_binary_sha256_before,
        "environment_binary_sha256_after": &profile.environment_binary_sha256_after,
        "benchmark_binary_sha256_prebuild": &profile.benchmark_binary_sha256_prebuild,
        "benchmark_binary_sha256_postbuild": &profile.benchmark_binary_sha256_postbuild,
        "benchmark_binary_sha256_before": &profile.benchmark_binary_sha256_before,
        "benchmark_binary_sha256_after": &profile.benchmark_binary_sha256_after,
        "binary_attestations_stable": profile.binary_attestations_stable,
        "formal_binary_source_attestation_present": profile.formal_binary_source_attestation_present,
        "compiled_input_closure_attested": profile.compiled_input_closure_attested,
        "sample_count": profile.statistics.sample_count,
        "mean": profile.statistics.mean,
        "nearest_rank_p95": profile.statistics.nearest_rank_p95,
        "maximum": profile.statistics.maximum,
        "final_all_nine_deck_gate": &profile.final_all_nine_deck_gate,
    });

    let output = json!({
        "schema_version": 2,
        "privacy": "aggregate metrics, public case labels, fixed category strings, algorithm digests, and public width-profile provenance only",
        "candidate": {
            "sampler_version": FAST_CATEGORICAL_SAMPLER_VERSION,
            "contract_sha256": contract_digest,
            "exp_table_sha256": table_digest,
        },
        "oracle": {
            "sampler_version": &oracle_file.oracle_sampler_version,
            "fixture_sha256": oracle_digest,
            "generator": &oracle_file.generator,
            "case_count": metrics.len(),
            "fixed_seed_count_per_case": python_draws.len(),
        },
        "independent_rng_and_selection_goldens": {
            "producer": &oracle_file.independent_rng_and_selection_goldens.producer,
            "splitmix_encoding": &oracle_file.independent_rng_and_selection_goldens.splitmix_first_draws.encoding,
            "splitmix_draws_sha256": &oracle_file.independent_rng_and_selection_goldens.splitmix_first_draws.sha256,
            "rust_splitmix_matches_every_python_draw": true,
            "selected_index_encoding": &oracle_file.independent_rng_and_selection_goldens.decimal_selected_indices.encoding,
            "selected_indices_sha256": &oracle_file.independent_rng_and_selection_goldens.decimal_selected_indices.sha256,
            "rust_selection_matches_python_digest": true,
        },
        "workload_width_profile": workload_width_profile,
        "probability_comparison": {
            "maximum_total_variation": maximum_total_variation,
            "mean_total_variation_all_cases": mean_total_variation,
            "width_profile_weighted_mean_total_variation": width_profile_weighted_mean_total_variation,
            "maximum_legacy_to_candidate_kl_nats": maximum_legacy_to_candidate_kl_nats,
            "mean_legacy_to_candidate_kl_nats": mean_legacy_to_candidate_kl_nats,
        },
        "selection_comparison": {
            "aggregate_selected_index_agreement": aggregate_selected_index_agreement,
            "width_profile_weighted_selected_index_agreement": width_profile_weighted_selected_index_agreement,
        },
        "predeclared_candidate_bounds": {
            "maximum_total_variation": bounds.maximum_total_variation,
            "width_profile_weighted_mean_total_variation": bounds.width_profile_weighted_mean_total_variation,
            "maximum_legacy_to_candidate_kl_nats": bounds.maximum_legacy_to_candidate_kl_nats,
            "aggregate_selected_index_agreement": bounds.aggregate_selected_index_agreement,
        },
        "bound_checks": bound_checks,
        "worst_adversarial_cases": worst,
        "diagnostic_checks_passed": diagnostic_checks_passed,
        "canonical_observed_workload_claim_valid": canonical_observed_workload_claim_valid,
        "source_performance_claim_valid": false,
        "claim_boundary": "candidate diagnostics only; the claim-eligible profile establishes all-policy Rally workload-shape provenance, while its source timing/performance gate is invalid and no source rates are imported; the all-nine-deck sampler gate is deferred and no learning noninferiority is claimed",
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    if !diagnostic_checks_passed {
        return Err("one or more predeclared candidate bounds failed".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("FAST_SAMPLER_DIAGNOSTIC: FAIL: {error}");
        std::process::exit(1);
    }
}
