//! Tensorize-cost lane (opus/tensorize-cost-v1): identity goldens and the
//! per-stage cost breakdown for `NativeFlatTensorizerV2`.
//!
//! Test-only. Two ignored harnesses share the D5 corpus generator (the exact
//! splitmix64 Burn/Rally rollout of `device_tensorization_hash_share_probe_at_scale_v1`):
//!
//! - `tensorize_cost_d5_golden_v1` fingerprints every decision's full output
//!   (all thirteen tensors, the state canonical JSON, and every action's
//!   canonical JSON and SHA-512 blocks) and either writes the golden file
//!   (`MTG_KERNEL_TENSORIZE_GOLDEN_WRITE=<path>`) or compares against the
//!   committed one (`MTG_KERNEL_TENSORIZE_GOLDEN=<path>`, default the pinned
//!   file under `docs/reports/tensorize_cost_v1/`).
//! - `tensorize_cost_breakdown_v1` times the production `fill` and each of its
//!   stages (serialize, hash, other) serially and on N worker threads.

use super::tests::OwnedScoringDecisionV2;
use super::*;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use sha2::Sha256;
use std::io::Write as _;

pub(super) fn d5_next_random_v1(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// The D5 corpus, generated exactly as the at-scale hash-share harness does.
pub(super) fn build_d5_corpus_v1(target_decisions: usize) -> (Vec<OwnedScoringDecisionV2>, u64) {
    let deck_combos: [[&str; 2]; 4] = [
        ["Burn", "Burn"],
        ["Rally", "Rally"],
        ["Burn", "Rally"],
        ["Rally", "Burn"],
    ];
    const STEPS_PER_SCENARIO: usize = 96;
    const MAX_SCENARIOS: usize = 4_000;
    let mut corpus = Vec::with_capacity(target_decisions + STEPS_PER_SCENARIO);
    let mut scenario_index: u64 = 0;
    while corpus.len() < target_decisions {
        assert!((scenario_index as usize) < MAX_SCENARIOS);
        let episode_id = 91_000_u64 + scenario_index;
        let combo = &deck_combos[(scenario_index as usize) % deck_combos.len()];
        let deck_ids = [combo[0].to_string(), combo[1].to_string()];
        let mut seed_mix = 0x1235_9871_ab34_de56_u64 ^ episode_id.rotate_left(29);
        let environment_seed =
            d5_next_random_v1(&mut seed_mix) ^ d5_next_random_v1(&mut seed_mix).rotate_left(13);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            512,
            65_536,
            deck_ids,
        )
        .unwrap();
        let mut random_state = environment_seed ^ episode_id.rotate_left(17);
        for _ in 0..STEPS_PER_SCENARIO {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                break;
            };
            corpus.push(OwnedScoringDecisionV2::from_session(&session));
            let selected = (d5_next_random_v1(&mut random_state)
                % u64::from(expected.legal_action_count)) as u32;
            session
                .step(expected.episode_id, expected.step, selected)
                .unwrap();
        }
        scenario_index += 1;
    }
    (corpus, scenario_index)
}

fn hash_f32s(hasher: &mut Sha256, values: &[f32]) {
    hasher.update((values.len() as u64).to_le_bytes());
    for value in values {
        hasher.update(value.to_bits().to_le_bytes());
    }
}

fn hash_i64s(hasher: &mut Sha256, values: &[i64]) {
    hasher.update((values.len() as u64).to_le_bytes());
    for value in values {
        hasher.update(value.to_le_bytes());
    }
}

/// Canonical state JSON through the production writer.
pub(super) fn state_canonical_json_v1(decision: FlatScoringDecisionViewV1<'_>) -> Vec<u8> {
    let objects = encode_objects_v2(decision).unwrap();
    let mut json = Vec::new();
    write_canonical_observation_v2(decision, &objects.projection, &mut json).unwrap();
    json
}

/// SHA-256 over every production output of one decision: the thirteen
/// tensors (bit patterns, with lengths), the state canonical JSON, and each
/// action's canonical JSON and six SHA-512 blocks.
pub(super) fn decision_fingerprint_v1(
    tensorizer: &mut NativeFlatTensorizerV2,
    decision: FlatScoringDecisionViewV1<'_>,
) -> [u8; 32] {
    let mut output = NativeFlatDecisionTensorV2::default();
    tensorizer.fill(decision, &mut output).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(b"tensorize-cost-v1/decision-fingerprint");
    hash_f32s(&mut hasher, &output.state);
    hash_f32s(&mut hasher, &output.object_features);
    hash_i64s(&mut hasher, &output.object_card_ids);
    hash_i64s(&mut hasher, &output.object_groups);
    hash_i64s(&mut hasher, &output.object_node_ids);
    hash_f32s(&mut hasher, &output.edge_features);
    hash_i64s(&mut hasher, &output.edge_source_indices);
    hash_i64s(&mut hasher, &output.edge_target_indices);
    hash_f32s(&mut hasher, &output.action_features);
    hash_f32s(&mut hasher, &output.action_ref_features);
    hash_i64s(&mut hasher, &output.action_ref_card_ids);
    hash_i64s(&mut hasher, &output.action_ref_action_indices);
    hash_i64s(&mut hasher, &output.action_ref_node_indices);
    let state_json = state_canonical_json_v1(decision);
    hasher.update((state_json.len() as u64).to_le_bytes());
    hasher.update(&state_json);
    let bindings = diagnostic_native_flat_action_semantic_bindings_v2(decision).unwrap();
    hasher.update((bindings.len() as u64).to_le_bytes());
    for binding in &bindings {
        hasher.update((binding.canonical_json.len() as u64).to_le_bytes());
        hasher.update(&binding.canonical_json);
        for block in &binding.sha512_blocks {
            hasher.update(block);
        }
    }
    hasher.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

pub(super) fn default_golden_path_v1() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/reports/tensorize_cost_v1/goldens/d5-corpus-fingerprints-v1.txt")
}

/// Renders the golden text for a corpus: one header, one line per decision
/// (`index actions state_json_bytes fingerprint`), and the rollup.
pub(super) fn golden_text_v1(
    corpus_label: &str,
    corpus: &[OwnedScoringDecisionV2],
    tensorizer: &mut NativeFlatTensorizerV2,
) -> String {
    let mut rollup = Sha256::new();
    let mut body = String::new();
    for (index, owned) in corpus.iter().enumerate() {
        let view = owned.view();
        let fingerprint = decision_fingerprint_v1(tensorizer, view);
        rollup.update(fingerprint);
        body.push_str(&format!(
            "{index} {} {} {}\n",
            view.actions().len(),
            state_canonical_json_v1(view).len(),
            hex(&fingerprint)
        ));
    }
    format!(
        "# tensorize-cost-v1 golden: {corpus_label} decisions={} rollup_sha256={}\n{body}",
        corpus.len(),
        hex(&rollup.finalize())
    )
}

/// Writes (`MTG_KERNEL_TENSORIZE_GOLDEN_WRITE`) or checks the D5 corpus golden.
///
/// ```text
/// cargo test --release --lib native_flat_tensorizer_v2::cost_tests_v1::tensorize_cost_d5_golden_v1 -- --ignored --exact --nocapture
/// ```
#[test]
#[ignore = "identity golden over the 10k D5 corpus: run explicitly"]
fn tensorize_cost_d5_golden_v1() {
    let target = env_usize(
        "MTG_KERNEL_TIMING_HARNESS_TENSORIZE_TARGET_DECISIONS",
        10_000,
    );
    let (corpus, scenarios) = build_d5_corpus_v1(target);
    let label = format!("d5 target={target} scenarios={scenarios}");
    let mut tensorizer = NativeFlatTensorizerV2::new();
    let text = golden_text_v1(&label, &corpus, &mut tensorizer);
    let header = text.lines().next().unwrap().to_owned();
    println!("{header}");
    if let Ok(path) = std::env::var("MTG_KERNEL_TENSORIZE_GOLDEN_WRITE") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text.as_bytes()).unwrap();
        println!("golden written to {}", path.display());
        return;
    }
    let path = std::env::var("MTG_KERNEL_TENSORIZE_GOLDEN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| default_golden_path_v1());
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read golden {}: {error}", path.display()))
        .replace("\r\n", "\n");
    if expected != text {
        let first = expected
            .lines()
            .zip(text.lines())
            .position(|(left, right)| left != right);
        panic!(
            "tensorize output differs from the pinned golden {} (first differing line {first:?}; \
             expected header `{}`, actual `{header}`)",
            path.display(),
            expected.lines().next().unwrap_or("")
        );
    }
    println!("golden identical: {}", path.display());
}

#[derive(Default, Clone, Copy)]
struct StageNs {
    fill: u64,
    objects_edges: u64,
    state_serialize: u64,
    state_hash: u64,
    state_other: u64,
    actions_total: u64,
    actions_hash: u64,
    actions_skip: u64,
}

impl StageNs {
    fn add(&mut self, other: &StageNs) {
        self.fill += other.fill;
        self.objects_edges += other.objects_edges;
        self.state_serialize += other.state_serialize;
        self.state_hash += other.state_hash;
        self.state_other += other.state_other;
        self.actions_total += other.actions_total;
        self.actions_hash += other.actions_hash;
        self.actions_skip += other.actions_skip;
    }
}

fn nanos(start: std::time::Instant) -> u64 {
    start.elapsed().as_nanos() as u64
}

/// One timed pass over `corpus`, starting at `offset` and wrapping around (so
/// concurrent threads never walk the same decisions in lockstep, which would
/// hand the process-wide state prefix cache each other's identical JSONs).
/// Each stage is timed in isolation on the same inputs; `fill` is the
/// production call, measured separately.
fn stage_pass(
    corpus: &[OwnedScoringDecisionV2],
    action_json: &[Vec<Vec<u8>>],
    offset: usize,
) -> StageNs {
    let mut tensorizer = NativeFlatTensorizerV2::new();
    let mut output = NativeFlatDecisionTensorV2::default();
    let mut scratch = Vec::with_capacity(16 * 1024);
    let mut total = StageNs::default();
    let rotated = corpus
        .iter()
        .zip(action_json)
        .cycle()
        .skip(offset)
        .take(corpus.len());
    for (owned, jsons) in rotated {
        let view = owned.view();
        let start = std::time::Instant::now();
        tensorizer.fill(view, &mut output).unwrap();
        total.fill += nanos(start);
        std::hint::black_box(&output);

        let start = std::time::Instant::now();
        validate_auxiliary_tables_v2(view).unwrap();
        let objects = encode_objects_v2(view).unwrap();
        let edges = encode_edges_v2(view, &objects.projection).unwrap();
        total.objects_edges += nanos(start);
        std::hint::black_box(&edges.features);

        let start = std::time::Instant::now();
        write_canonical_observation_v2(view, &objects.projection, &mut scratch).unwrap();
        total.state_serialize += nanos(start);

        let start = std::time::Instant::now();
        let mut digest = Vec::with_capacity(NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2);
        append_digest_features_v2(
            &mut digest,
            b"observation-state",
            &scratch,
            NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2,
        );
        total.state_hash += nanos(start);
        std::hint::black_box(&digest);

        let start = std::time::Instant::now();
        let state = encode_state_skip_hash_v1(view).unwrap();
        total.state_other += nanos(start);
        std::hint::black_box(&state);

        let start = std::time::Instant::now();
        let actions = encode_action_half_with_projection_and_scratch_v2(
            view,
            Some(&objects.projection),
            &mut scratch,
            None,
        )
        .unwrap();
        total.actions_total += nanos(start);
        std::hint::black_box(&actions.action_features);

        let start = std::time::Instant::now();
        for json in jsons {
            std::hint::black_box(action_hash_features_v1(json));
        }
        total.actions_hash += nanos(start);

        let refs = view.action_refs();
        let start = std::time::Instant::now();
        for (action_index, action) in view.actions().iter().enumerate() {
            let begin = action.ref_start as usize;
            let end = begin + usize::from(action.ref_len);
            let encoded = encode_action_with_scratch_skip_hash_v1(
                view,
                action_index,
                action,
                &refs[begin..end],
                Some(&objects.projection),
            )
            .unwrap();
            std::hint::black_box(&encoded.features);
        }
        total.actions_skip += nanos(start);
    }
    total
}

fn report(label: &str, workers: usize, decisions: usize, wall_ns: u64, stages: &StageNs) {
    let per = |ns: u64| ns as f64 / decisions as f64 / 1_000.0;
    let fill = per(stages.fill);
    let serialize = per(stages.state_serialize)
        + (per(stages.actions_total) - per(stages.actions_hash) - per(stages.actions_skip));
    let hash = per(stages.state_hash) + per(stages.actions_hash);
    let other = fill - serialize - hash;
    println!(
        "breakdown {label} workers={workers} decisions={decisions} fill_us={fill:.2} \
         serialize_us={serialize:.2} hash_us={hash:.2} other_us={other:.2} \
         [objects_edges={:.2} state_serialize={:.2} state_hash={:.2} state_other={:.2} \
         actions_total={:.2} actions_hash={:.2} actions_skip={:.2}] \
         fill_throughput_decisions_per_s={:.0}",
        per(stages.objects_edges),
        per(stages.state_serialize),
        per(stages.state_hash),
        per(stages.state_other),
        per(stages.actions_total),
        per(stages.actions_hash),
        per(stages.actions_skip),
        decisions as f64 / (wall_ns as f64 / 1e9),
    );
}

/// Per-stage cost of `NativeFlatTensorizerV2::fill` over the D5 corpus,
/// serially and with `MTG_KERNEL_TENSORIZE_WORKERS` (default 24) threads each
/// running the whole corpus concurrently. Wall throughput for the parallel
/// case counts every thread's decisions.
///
/// ```text
/// cargo test --release --lib native_flat_tensorizer_v2::cost_tests_v1::tensorize_cost_breakdown_v1 -- --ignored --exact --nocapture
/// ```
#[test]
#[ignore = "timing harness: run explicitly on a quiet machine"]
fn tensorize_cost_breakdown_v1() {
    let target = env_usize(
        "MTG_KERNEL_TIMING_HARNESS_TENSORIZE_TARGET_DECISIONS",
        10_000,
    );
    let rounds = env_usize("MTG_KERNEL_TENSORIZE_ROUNDS", 3);
    let workers = env_usize("MTG_KERNEL_TENSORIZE_WORKERS", 24);
    let (corpus, scenarios) = build_d5_corpus_v1(target);
    let action_json: Vec<Vec<Vec<u8>>> = corpus
        .iter()
        .map(|owned| {
            diagnostic_native_flat_action_semantic_bindings_v2(owned.view())
                .unwrap()
                .into_iter()
                .map(|binding| binding.canonical_json)
                .collect()
        })
        .collect();
    let state_bytes: usize = corpus
        .iter()
        .map(|owned| state_canonical_json_v1(owned.view()).len())
        .sum();
    let action_bytes: usize = action_json.iter().flatten().map(Vec::len).sum();
    let action_rows: usize = action_json.iter().map(Vec::len).sum();
    println!(
        "corpus decisions={} scenarios={scenarios} mean_state_json_bytes={:.1} \
         mean_action_rows={:.2} mean_action_json_bytes={:.1}",
        corpus.len(),
        state_bytes as f64 / corpus.len() as f64,
        action_rows as f64 / corpus.len() as f64,
        action_bytes as f64 / action_rows.max(1) as f64,
    );
    // Prefix-reuse potential for a midstate cache: for each decision, the
    // longest common prefix with any of the previous 8 decisions' state JSON,
    // counted in whole 128-byte SHA-512 blocks after the 21-byte header.
    let states: Vec<Vec<u8>> = corpus
        .iter()
        .map(|owned| state_canonical_json_v1(owned.view()))
        .collect();
    let mut shared_blocks = 0usize;
    let mut total_blocks = 0usize;
    for (index, json) in states.iter().enumerate() {
        let blocks = (21 + json.len() + 17).div_ceil(128);
        total_blocks += blocks;
        let best = states[index.saturating_sub(8)..index]
            .iter()
            .map(|other| json.iter().zip(other).take_while(|(a, b)| a == b).count())
            .max()
            .unwrap_or(0);
        shared_blocks += ((21 + best) / 128).min(blocks - 1);
    }
    println!(
        "state_prefix_reuse window=8 shared_block_fraction={:.4}",
        shared_blocks as f64 / total_blocks as f64
    );
    let _ = std::io::stdout().flush();

    // Warm-up.
    std::hint::black_box(stage_pass(&corpus, &action_json, 0));
    for round in 0..rounds {
        let start = std::time::Instant::now();
        let stages = stage_pass(&corpus, &action_json, 0);
        report(
            &format!("serial round={round}"),
            1,
            corpus.len(),
            stages.fill,
            &stages,
        );
        std::hint::black_box(start);
    }
    // Phase split of the production path (serial, one decision at a time):
    // prepare (JSON writes and the non-digest tensors), hash (every digest),
    // finish (digest features, output checks).
    {
        let batch = 1;
        let mut slots: Vec<DigestSlotV1> = (0..batch).map(|_| DigestSlotV1::default()).collect();
        let mut digests = DigestBatchV1::default();
        let (mut prepare_ns, mut hash_ns, mut finish_ns) = (0u64, 0u64, 0u64);
        for chunk in corpus.chunks(batch) {
            let start = std::time::Instant::now();
            let prepared: Vec<_> = chunk
                .iter()
                .zip(slots.iter_mut())
                .map(|(owned, slot)| prepare_full_decision_v2(owned.view(), slot).unwrap())
                .collect();
            prepare_ns += nanos(start);
            let start = std::time::Instant::now();
            let offsets = hash_prepared_slots_v2(&slots[..chunk.len()], &mut digests);
            hash_ns += nanos(start);
            let start = std::time::Instant::now();
            for (index, (owned, value)) in chunk.iter().zip(prepared).enumerate() {
                let blocks = &digests.blocks[offsets[index]..offsets[index + 1]];
                std::hint::black_box(finish_full_decision_v2(owned.view(), value, blocks).unwrap());
            }
            finish_ns += nanos(start);
        }
        let per = |ns: u64| ns as f64 / corpus.len() as f64 / 1_000.0;
        println!(
            "phases prepare_us={:.2} hash_us={:.2} finish_us={:.2}",
            per(prepare_ns),
            per(hash_ns),
            per(finish_ns)
        );
    }
    if workers > 1 {
        for round in 0..rounds {
            let barrier = std::sync::Barrier::new(workers);
            let start = std::time::Instant::now();
            let results: Vec<StageNs> = std::thread::scope(|scope| {
                let handles: Vec<_> = (0..workers)
                    .map(|worker| {
                        let barrier = &barrier;
                        let corpus = &corpus;
                        let action_json = &action_json;
                        scope.spawn(move || {
                            barrier.wait();
                            stage_pass(corpus, action_json, worker * corpus.len() / workers)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap())
                    .collect()
            });
            std::hint::black_box(nanos(start));
            let mut total = StageNs::default();
            for stages in &results {
                total.add(stages);
            }
            // Throughput from a fill-only pass: every thread runs the
            // production call over the whole corpus at once, each from its
            // own rotation offset.
            let barrier = std::sync::Barrier::new(workers);
            let start = std::time::Instant::now();
            std::thread::scope(|scope| {
                for worker in 0..workers {
                    let barrier = &barrier;
                    let corpus = &corpus;
                    scope.spawn(move || {
                        let mut tensorizer = NativeFlatTensorizerV2::new();
                        let mut output = NativeFlatDecisionTensorV2::default();
                        let offset = worker * corpus.len() / workers;
                        barrier.wait();
                        for owned in corpus.iter().cycle().skip(offset).take(corpus.len()) {
                            tensorizer.fill(owned.view(), &mut output).unwrap();
                            std::hint::black_box(&output);
                        }
                    });
                }
            });
            let wall = nanos(start);
            report(
                &format!("parallel round={round}"),
                workers,
                corpus.len() * workers,
                wall,
                &total,
            );
        }
    }
}

/// Leaf evaluator that records every root prior and every eighth leaf
/// decision the search encodes (the real forward still runs, so the tree is
/// the production tree).
struct CapturingEvaluatorV1<'a> {
    inner: crate::model_guided_search_core_v1::ModelGuidedSearchRealForwardValueEvaluatorV1<'a>,
    captured: std::cell::RefCell<Vec<OwnedScoringDecisionV2>>,
    leaf_events: std::cell::Cell<u64>,
}

impl crate::model_guided_search_core_v1::ModelGuidedSearchLeafEvaluatorV1
    for CapturingEvaluatorV1<'_>
{
    fn evaluate_leaf_v1(
        &self,
        session: &FastActorSessionV1,
        leaf_key: [u8; 32],
        legal_action_count: u32,
        site: crate::model_guided_search_core_v1::ModelGuidedSearchLeafSiteV1,
    ) -> Result<
        crate::model_guided_search_core_v1::ModelGuidedSearchLeafForwardV1,
        crate::model_guided_search_core_v1::ModelGuidedSearchCoreErrorV1,
    > {
        let ordinal = self.leaf_events.get();
        self.leaf_events.set(ordinal + 1);
        let keep = matches!(
            site,
            crate::model_guided_search_core_v1::ModelGuidedSearchLeafSiteV1::RootPrior
        ) || ordinal.is_multiple_of(8);
        if keep && matches!(session.current_response(), FastActorResponseV1::Decision(_)) {
            self.captured
                .borrow_mut()
                .push(OwnedScoringDecisionV2::from_session(session));
        }
        self.inner
            .evaluate_leaf_v1(session, leaf_key, legal_action_count, site)
    }
}

/// One search-wrapped match, recorded as its reset parameters and the root
/// action the search chose at every decision. The search's simulation seeds
/// are bound to the build commit (by design), so a match cannot be replayed
/// by searching again at another commit; replaying the recorded root actions
/// reproduces every root decision exactly, because the engine is
/// deterministic given its environment seed.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SearchMatchReplayV1 {
    pub(super) decks: [String; 2],
    pub(super) episode_id: u64,
    pub(super) seed: u64,
    pub(super) actions: Vec<u32>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SearchReplayFileV1 {
    pub(super) schema: String,
    pub(super) engine_commit: String,
    pub(super) search: String,
    pub(super) matches: Vec<SearchMatchReplayV1>,
}

fn search_session_v1(decks: &[String; 2], episode_id: u64, seed: u64) -> FastActorSessionV1 {
    FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
        episode_id,
        seed,
        512,
        65_536,
        [decks[0].clone(), decks[1].clone()],
    )
    .unwrap()
}

/// Plays one complete match with model-guided search (T512, the real
/// forward, runner-fixed weights) choosing for both seats. Returns the root
/// actions and the captured decisions (root priors plus every eighth leaf).
pub(super) fn play_search_match_v1(
    decks: &[String; 2],
    episode_id: u64,
    seed: u64,
) -> (Vec<u32>, Vec<OwnedScoringDecisionV2>) {
    use crate::kernel_native_search_opponent_v1::KernelNativeSearchTierV1;
    use crate::model_guided_search_authority_v1::{
        ModelGuidedSearchAuthorityV1, ModelGuidedSearchConsumptionModeV1,
        MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1,
    };
    use crate::model_guided_search_core_v1::{
        ModelGuidedSearchCoreV1, ModelGuidedSearchRealForwardValueEvaluatorV1,
    };
    use crate::model_guided_search_value_quantization_v1::ModelGuidedSearchValueHeadDomainV1;
    use crate::native_policy_value_net_v1::{
        NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    };

    let authority = ModelGuidedSearchAuthorityV1::new(
        KernelNativeSearchTierV1::T512,
        MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[0],
        crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
        "D:/mtg-kernel-store/test-lineage",
        0,
        &"1".repeat(64),
        "net8-family/test-v1",
        ModelGuidedSearchConsumptionModeV1::SearchAsOpponent,
    )
    .unwrap();
    let searcher = ModelGuidedSearchCoreV1::new(authority).unwrap();
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let value_domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
        lower: -8.0,
        upper: 8.0,
    };
    let evaluator = CapturingEvaluatorV1 {
        inner: ModelGuidedSearchRealForwardValueEvaluatorV1::new(&model, value_domain),
        captured: std::cell::RefCell::new(Vec::new()),
        leaf_events: std::cell::Cell::new(0),
    };
    let mut session = search_session_v1(decks, episode_id, seed);
    let mut actions = Vec::new();
    while let FastActorResponseV1::Decision(expected) = session.current_response() {
        let result = searcher
            .select_action_v1(&session, expected, &evaluator, &value_domain)
            .unwrap();
        session
            .step(expected.episode_id, expected.step, result.selected_index)
            .unwrap();
        actions.push(result.selected_index);
        assert!(actions.len() <= 2_000, "search match did not terminate");
    }
    (actions, evaluator.captured.into_inner())
}

/// Replays recorded matches and returns every root decision with its seat.
pub(super) fn replay_search_corpus_v1(
    replay: &SearchReplayFileV1,
) -> (Vec<OwnedScoringDecisionV2>, [usize; 2]) {
    let mut corpus = Vec::new();
    let mut seats = [0usize; 2];
    for recorded in &replay.matches {
        let mut session = search_session_v1(&recorded.decks, recorded.episode_id, recorded.seed);
        for &action in &recorded.actions {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("replay ended before its recorded actions");
            };
            let seat = match expected.acting_player {
                crate::rl::PlayerSeatV1::P0 => 0,
                _ => 1,
            };
            seats[seat] += 1;
            corpus.push(OwnedScoringDecisionV2::from_session(&session));
            session
                .step(expected.episode_id, expected.step, action)
                .unwrap();
        }
        assert!(
            !matches!(session.current_response(), FastActorResponseV1::Decision(_)),
            "replayed match did not reach its recorded end"
        );
    }
    (corpus, seats)
}

pub(super) fn search_replay_path_v1() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/reports/tensorize_cost_v1/goldens/search-matches-replay-v1.json")
}

pub(super) fn default_search_golden_path_v1() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/reports/tensorize_cost_v1/goldens/search-corpus-fingerprints-v1.txt")
}

/// Records the search-wrapped matches (run once, at the base commit): both
/// deck orders of Rally/Burn plus both mirrors, two seeds each.
/// `MTG_KERNEL_TENSORIZE_SEARCH_REPLAY_WRITE=<path>` overrides the location.
#[test]
#[ignore = "records search-wrapped matches: run explicitly"]
fn tensorize_cost_search_record_v1() {
    let pairings = [
        ["Rally", "Burn"],
        ["Burn", "Rally"],
        ["Burn", "Burn"],
        ["Rally", "Rally"],
    ];
    let mut matches = Vec::new();
    for (pairing_index, pairing) in pairings.iter().enumerate() {
        for seed in [41_777_u64, 41_778] {
            let decks = [pairing[0].to_string(), pairing[1].to_string()];
            let episode_id = 60_001 + pairing_index as u64;
            let (actions, _) = play_search_match_v1(&decks, episode_id, seed);
            println!(
                "recorded {} vs {} seed={seed} roots={}",
                decks[0],
                decks[1],
                actions.len()
            );
            matches.push(SearchMatchReplayV1 {
                decks,
                episode_id,
                seed,
                actions,
            });
        }
    }
    let replay = SearchReplayFileV1 {
        schema: "tensorize-cost-v1/search-match-replay".to_owned(),
        engine_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_owned(),
        search: "model-guided T512, real forward, runner-fixed weights, both seats".to_owned(),
        matches,
    };
    let path = std::env::var("MTG_KERNEL_TENSORIZE_SEARCH_REPLAY_WRITE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| search_replay_path_v1());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_vec_pretty(&replay).unwrap()).unwrap();
    println!("replay written to {}", path.display());
}

/// Writes (`MTG_KERNEL_TENSORIZE_GOLDEN_SEARCH_WRITE`) or checks
/// (`MTG_KERNEL_TENSORIZE_GOLDEN_SEARCH`) the golden over every root decision
/// of the recorded search-wrapped matches.
#[test]
#[ignore = "identity golden over recorded search-wrapped matches: run explicitly"]
fn tensorize_cost_search_golden_v1() {
    let replay: SearchReplayFileV1 =
        serde_json::from_slice(&std::fs::read(search_replay_path_v1()).unwrap()).unwrap();
    let (corpus, seats) = replay_search_corpus_v1(&replay);
    let label = format!(
        "search-replay matches={} recorded_at={} seat0={} seat1={}",
        replay.matches.len(),
        replay.engine_commit,
        seats[0],
        seats[1]
    );
    assert!(
        corpus.len() >= 1_000 && seats[0] > 0 && seats[1] > 0,
        "{label}"
    );
    let mut tensorizer = NativeFlatTensorizerV2::new();
    let text = golden_text_v1(&label, &corpus, &mut tensorizer);
    let header = text.lines().next().unwrap().to_owned();
    println!("{header}");
    if let Ok(path) = std::env::var("MTG_KERNEL_TENSORIZE_GOLDEN_SEARCH_WRITE") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text.as_bytes()).unwrap();
        println!("golden written to {}", path.display());
        return;
    }
    let path = std::env::var("MTG_KERNEL_TENSORIZE_GOLDEN_SEARCH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| default_search_golden_path_v1());
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read golden {}: {error}", path.display()))
        .replace("\r\n", "\n");
    assert!(
        expected == text,
        "tensorize output differs from the pinned search golden {} (expected header `{}`, actual `{header}`)",
        path.display(),
        expected.lines().next().unwrap_or("")
    );
    println!("golden identical: {}", path.display());
}

/// Live differential at the current commit: plays one search-wrapped match
/// and checks every captured decision (root priors and every eighth leaf)
/// against the reference encoder (serde-Value state JSON, per-message sha2).
#[test]
#[ignore = "plays a search-wrapped match: run explicitly"]
fn tensorize_cost_search_leaf_differential_v1() {
    let decks = ["Rally".to_string(), "Burn".to_string()];
    let (actions, captured) = play_search_match_v1(&decks, 60_101, 41_779);
    let mut tensorizer = NativeFlatTensorizerV2::new();
    let mut output = NativeFlatDecisionTensorV2::default();
    for (index, owned) in captured.iter().enumerate() {
        tensorizer.fill(owned.view(), &mut output).unwrap();
        let (reference, _) = encode_full_decision_reference_v2(owned.view()).unwrap();
        assert!(
            output == reference,
            "decision {index} differs from the reference encoder"
        );
    }
    println!(
        "leaf differential: roots={} decisions={} all identical to the reference encoder",
        actions.len(),
        captured.len()
    );
}

/// The shared state-prefix cache changes cost, never output: eight threads
/// tensorize the D5 corpus concurrently, each in its own permutation (so the
/// cache sees different histories), and every decision's fingerprint must
/// equal the pinned golden's.
#[test]
#[ignore = "identity under concurrent, reordered cache use over the 10k D5 corpus: run explicitly"]
fn tensorize_cost_d5_concurrent_shuffled_identity_v1() {
    let target = env_usize(
        "MTG_KERNEL_TIMING_HARNESS_TENSORIZE_TARGET_DECISIONS",
        10_000,
    );
    let (corpus, _) = build_d5_corpus_v1(target);
    let golden = std::fs::read_to_string(default_golden_path_v1())
        .unwrap()
        .replace("\r\n", "\n");
    let expected: Vec<String> = golden
        .lines()
        .skip(1)
        .map(|line| line.rsplit(' ').next().unwrap().to_owned())
        .collect();
    assert_eq!(expected.len(), corpus.len());
    std::thread::scope(|scope| {
        for thread in 0..8_u64 {
            let corpus = &corpus;
            let expected = &expected;
            scope.spawn(move || {
                let mut order: Vec<usize> = (0..corpus.len()).collect();
                let mut random = 0xfeed_0000_u64 + thread;
                for index in (1..order.len()).rev() {
                    let pick = (d5_next_random_v1(&mut random) % (index as u64 + 1)) as usize;
                    order.swap(index, pick);
                }
                let mut tensorizer = NativeFlatTensorizerV2::new();
                for index in order {
                    let fingerprint =
                        decision_fingerprint_v1(&mut tensorizer, corpus[index].view());
                    assert_eq!(
                        hex(&fingerprint),
                        expected[index],
                        "thread {thread} decision {index}"
                    );
                }
            });
        }
    });
    println!(
        "concurrent shuffled identity: 8 threads x {} decisions",
        corpus.len()
    );
}
