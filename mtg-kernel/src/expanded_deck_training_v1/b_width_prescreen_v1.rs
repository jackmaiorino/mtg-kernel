//! The B width pre-screen driver (collab-lead
//! LANES/b-width-prescreen-20261027/SCREEN-NOTE.md v1.2, frozen at 12aab0f2).
//! Every constant comes from one strict config file whose SHA-256 every
//! record carries; nothing below hard-codes a deck, count, seed domain, split,
//! chain, fitting constant or threshold.
//!
//! **Stages**, each with durable outputs under one absolute root and resume:
//! - `staging-check`: every chain's import, the files the import pins by
//!   absolute path, and the parent checkpoint exist with their pinned hashes
//!   and the note's feature digests. Loads no model and plays nothing.
//! - `qualify-collect`, `qualify-fit`: the throughput check
//!   (COMPUTE-POLICY.md). Each times the real workload at every candidate
//!   worker count and records the fastest; `collect`, `preserve` and `fit`
//!   refuse to start without a matching record for this host, binary and
//!   config.
//! - `collect`: each scheduled game of a chain is played exactly once (no
//!   retry, no replacement) by the parent against itself through the RT1
//!   collector's own episode path: [`collect_episode`] with the current-self
//!   opponent forked from the learner under the legacy (deployed) sampler,
//!   as `collect_parallel_v1` serves an RT1 `current` opponent. Natural games
//!   are stored in a compact binary store; a non-natural game keeps its
//!   terminal in the collection record and has no stored decisions. No JSON
//!   trajectory is written.
//! - `convert`: the store becomes `ScreenGameRecordV1`s, non-natural games
//!   are filtered out, and the chain's data record (section 7's per-chain
//!   items) is written. Once every chain has one, `SECTION7.json` is too.
//! - `preserve`: section 6.5's function preservation. On every recorded fit
//!   and holdout decision input, the widened net's value output and complete
//!   policy logits must be bit-identical to the parent's, through the fit
//!   entry's own forward, for every fitting seed.
//! - `fit`: the per-chain fits of every (seed, arm), run concurrently from one
//!   shared read-only record set, each from fresh Adam, each writing one
//!   report. A chain's step-0 held-out reports of w64 and w128 must then be
//!   bit-identical.
//!
//! The single read is the separate pinned analyzer
//! (`python/tools/b_width_prescreen_read_v1.py`), whose hash the config pins.
//!
//! **Store format.** `games.bin` is [`STORE_MAGIC_V1`] followed by each
//! natural game's bytes in schedule order; the collection record gives each
//! game's offset, length and SHA-256. A game is a little-endian u32 group
//! count, then per group the actor byte and a u32 substep count, then per
//! substep the u32 selected action and the thirteen tensor fields in
//! declaration order. An `i64` field is a u32 length and its values. An `f32`
//! field is a u32 length and runs of (u32 count of +0.0 words, u32 count of
//! literal words, the literal bit patterns), so a signed zero stays a
//! literal and every value round-trips bit for bit.

use super::phase1_parallel_collection::ordered_parallel_jobs_v1;
use super::*;
use crate::native_policy_train_step_v1::screen_fit_v1::{
    decision_source_sha256_v1, fit_screen_value_dims_v1, outcome_source_sha256_v1,
    ScreenDecisionGroupV1, ScreenFitConfigV1, ScreenFitReportV1, ScreenForwardV1,
    ScreenGameOutcomeV1, ScreenGameRecordV1, ScreenSubstepV1, TERMINAL_RETURN_TARGET_SOURCE_V1,
};
use crate::native_policy_train_step_v1::widening_v1::{
    w32_control_from_net8_snapshot_v1, widen_net8_snapshot_v1, TORCH_DEFAULT_UNIFORM_V1,
};
use crate::native_policy_train_step_v1::{
    NativePolicyValueTrainStateDimsV1, NativePolicyValueTrainStateWideV1, Net8DimsV1,
    Net8W128DimsV1, NetDimsV1,
};
use crate::native_policy_value_net_v1::W_MODEL_ARCHITECTURE_VERSION_V1;
use std::io::Write;
use std::time::Instant;

#[cfg(test)]
mod tests;

pub(crate) const CONFIG_SCHEMA_V1: &str = "mtg-kernel-b-width-prescreen-config/v1";
const STAGING_SCHEMA_V1: &str = "mtg-kernel-b-width-prescreen-staging-check/v1";
const QUALIFICATION_SCHEMA_V1: &str = "mtg-kernel-b-width-prescreen-qualification/v1";
const COLLECTION_SCHEMA_V1: &str = "mtg-kernel-b-width-prescreen-collection/v1";
const DATA_SCHEMA_V1: &str = "mtg-kernel-b-width-prescreen-data/v1";
const SECTION7_SCHEMA_V1: &str = "mtg-kernel-b-width-prescreen-section7/v1";
const PRESERVATION_SCHEMA_V1: &str = "mtg-kernel-b-width-prescreen-preservation/v1";
const FIT_SCHEMA_V1: &str = "mtg-kernel-b-width-prescreen-fit/v1";
pub(crate) const STORE_MAGIC_V1: &[u8] = b"mtg-kernel-b-width-prescreen-store/v1\n";
/// The one accepted bootstrap draw order (note section 5): per chain
/// occurrence, its seed draws, then its per-pair game draws.
pub(crate) const BOOTSTRAP_DRAW_ORDER_V1: &str = "occurrence-seeds-then-pairs-v1";
/// The arms, in record order (note section 3).
pub(crate) const ARM_NAMES_V1: [&str; 3] = ["w64", "w128", "w32"];
/// Decision inputs per preservation job.
const PRESERVATION_CHUNK_V1: usize = 256;

// ---------------------------------------------------------------------------
// Config.
// ---------------------------------------------------------------------------

/// The driver's config: every constant of the screen. Unknown or missing
/// fields are refused.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenConfigV1 {
    pub(crate) schema: String,
    pub(crate) note: PrescreenNoteV1,
    /// Note section 1: `v4`. The fit entry takes it as an argument.
    pub(crate) input_generation: String,
    pub(crate) feature_contract_digest: String,
    pub(crate) feature_encoding_digest: String,
    /// OH2's nine decks in OH2's deck order; deck indices are positions here.
    pub(crate) decks: Vec<ExpandedDeckListV1>,
    pub(crate) collection: PrescreenCollectionV1,
    pub(crate) chains: Vec<PrescreenChainV1>,
    pub(crate) arms: PrescreenArmsV1,
    pub(crate) fitting: PrescreenFittingV1,
    pub(crate) qualification: PrescreenQualificationV1,
    /// The analyzer's constants. The driver validates their shape; the
    /// analyzer reads them.
    pub(crate) analysis: PrescreenAnalysisV1,
    pub(crate) analyzer: PrescreenAnalyzerPinV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenNoteV1 {
    pub(crate) path: String,
    pub(crate) version: String,
    pub(crate) collab_commit: String,
    pub(crate) blob: String,
}

/// Which physical seat starts (note section 2, "ordinary starts" under the
/// OH2 RT1 recipe).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum StartingPlayerRuleV1 {
    /// RT1's scheduler rule (`tools/spy_reset/pilot.py`, `schedule`):
    /// physical seat `(index + focal + opposing) mod 2`.
    #[serde(rename = "index-plus-focal-plus-opposing-mod-2-v1")]
    IndexPlusFocalPlusOpposingMod2V1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenCollectionV1 {
    pub(crate) games_per_pair: u32,
    pub(crate) fit_indices: Vec<u32>,
    pub(crate) holdout_indices: Vec<u32>,
    pub(crate) seed_domain: String,
    /// The focal deck's physical seat, which is also the learner seat.
    pub(crate) focal_seat: u8,
    pub(crate) starting_player_rule: StartingPlayerRuleV1,
    /// Where the seat and starting-player rule come from (free text).
    pub(crate) seat_rule_basis: String,
    pub(crate) max_physical_decisions: u64,
    pub(crate) max_policy_steps: u64,
    pub(crate) runtime_rules: EpisodeRuntimeRulesV1,
    pub(crate) postboard: bool,
    pub(crate) collection_sampler: CollectionSamplerV1,
    pub(crate) training_math: TrainingMathV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenChainV1 {
    pub(crate) index: u32,
    pub(crate) id: String,
    /// The parent: its checkpoint pin is the note's section 1 hash.
    pub(crate) source: ExpandedModelSourceV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenArmsV1 {
    pub(crate) names: Vec<String>,
    pub(crate) widen_target_architecture: String,
    pub(crate) widen_init_law: String,
    pub(crate) fresh_adam: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenFittingV1 {
    pub(crate) value_target_source: String,
    pub(crate) learning_rate: f32,
    pub(crate) value_coefficient: f32,
    pub(crate) batch_groups: usize,
    /// steps = ceil(epochs * fit_groups / batch_groups).
    pub(crate) epochs: u64,
    /// heldout_every_steps = floor(steps / heldout_curve_points).
    pub(crate) heldout_curve_points: u64,
    pub(crate) fitting_seeds: Vec<u32>,
    pub(crate) minibatch_seed_domain: String,
    pub(crate) widen_seed_domain: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenQualificationV1 {
    /// Games per candidate worker count in `qualify-collect`.
    pub(crate) collect_games: usize,
    /// Updates per concurrent fit in `qualify-fit`.
    pub(crate) fit_steps: u64,
    /// Held-out games each `qualify-fit` fit evaluates.
    pub(crate) fit_heldout_games: usize,
    /// Seed domain of qualification games and widenings, disjoint from the
    /// screen's own by name.
    pub(crate) seed_domain: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenAnalysisV1 {
    pub(crate) bootstrap_draws: u64,
    pub(crate) bootstrap_seed_domain: String,
    pub(crate) bootstrap_seed_index: u64,
    pub(crate) bootstrap_draw_order: String,
    pub(crate) interval_lower_index: u64,
    pub(crate) interval_upper_index: u64,
    pub(crate) fund_lower_bound_above: f64,
    pub(crate) fund_min_point_r: f64,
    pub(crate) t_critical_df7: f64,
    pub(crate) max_pooled_nonnatural_holdout_fraction: f64,
    pub(crate) max_chain_nonnatural_holdout: u32,
    pub(crate) max_chain_nonnatural_fit: u32,
    pub(crate) min_holdout_games_per_pair: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrescreenAnalyzerPinV1 {
    /// Repository-relative path of the analyzer.
    pub(crate) path: String,
    pub(crate) sha256: String,
}

fn is_hex64_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn valid_domain_v1(domain: &str) -> bool {
    !domain.is_empty() && domain.is_ascii() && !domain.contains('|')
}

impl PrescreenConfigV1 {
    pub(crate) fn validate_v1(&self) -> Result<(), String> {
        self.validate_with_v1(true)
    }

    /// `require_parent_checkpoints` is false only for the CI fixture, whose
    /// synthetic parent is an import without a checkpoint.
    fn validate_with_v1(&self, require_parent_checkpoints: bool) -> Result<(), String> {
        ensure(self.schema == CONFIG_SCHEMA_V1, "config schema differs")?;
        ensure(
            self.input_generation == "v4",
            "input generation must be v4 (note section 1)",
        )?;
        ensure(
            self.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V4
                && self.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V4,
            "feature digests are not the compiled v4 pair",
        )?;
        ensure(
            !self.decks.is_empty() && self.decks.len() <= 64,
            "invalid deck count",
        )?;
        let mut labels = BTreeSet::new();
        for deck in &self.decks {
            deck.validated()?;
            ensure(labels.insert(&deck.label), "duplicate deck label")?;
        }
        let c = &self.collection;
        ensure(
            (1..=1000).contains(&c.games_per_pair),
            "invalid games_per_pair",
        )?;
        let fit: BTreeSet<u32> = c.fit_indices.iter().copied().collect();
        let holdout: BTreeSet<u32> = c.holdout_indices.iter().copied().collect();
        ensure(
            fit.len() == c.fit_indices.len()
                && holdout.len() == c.holdout_indices.len()
                && !fit.is_empty()
                && !holdout.is_empty()
                && fit.is_disjoint(&holdout)
                && fit.union(&holdout).copied().eq(0..c.games_per_pair),
            "fit and holdout indices must partition 0..games_per_pair",
        )?;
        ensure(
            valid_domain_v1(&c.seed_domain),
            "invalid collection seed domain",
        )?;
        ensure(c.focal_seat < 2, "invalid focal seat")?;
        ensure(
            (1..=100_000).contains(&c.max_physical_decisions)
                && (1..=1_000_000).contains(&c.max_policy_steps),
            "invalid episode limits",
        )?;
        ensure(
            c.collection_sampler.is_legacy(),
            "the deployed sampler is the legacy collection sampler",
        )?;
        ensure(
            c.training_math.is_platform_libm(),
            "OH2's recipe trains and plays under the platform libm",
        )?;
        ensure(!self.chains.is_empty(), "no chains")?;
        let mut indices = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for chain in &self.chains {
            ensure(chain.index >= 1, "chain indices start at 1")?;
            ensure(indices.insert(chain.index), "duplicate chain index")?;
            ensure(
                !chain.id.is_empty() && ids.insert(&chain.id),
                "invalid or duplicate chain id",
            )?;
            let transfer = &chain.source.feature_transfer;
            ensure(
                transfer.expected_feature_contract_digest == self.feature_contract_digest
                    && transfer.expected_feature_encoding_digest == self.feature_encoding_digest,
                "a parent's feature digests differ from the note's",
            )?;
            ensure(
                is_hex64_v1(&chain.source.play_import.sha256),
                "invalid import pin",
            )?;
            match &chain.source.checkpoint {
                Some(pin) => ensure(is_hex64_v1(&pin.sha256), "invalid parent pin")?,
                None => ensure(
                    !require_parent_checkpoints,
                    "every parent is a pinned checkpoint",
                )?,
            }
        }
        let a = &self.arms;
        ensure(
            a.names.iter().map(String::as_str).eq(ARM_NAMES_V1),
            "arms must be w64, w128 and w32",
        )?;
        ensure(
            a.widen_target_architecture == W_MODEL_ARCHITECTURE_VERSION_V1
                && a.widen_init_law == TORCH_DEFAULT_UNIFORM_V1,
            "unsupported widening",
        )?;
        ensure(a.fresh_adam, "every arm starts from fresh Adam")?;
        let f = &self.fitting;
        ensure(
            f.value_target_source == TERMINAL_RETURN_TARGET_SOURCE_V1,
            "unsupported value target source",
        )?;
        ensure(
            f.learning_rate.is_finite()
                && f.learning_rate > 0.0
                && f.value_coefficient.is_finite()
                && f.value_coefficient > 0.0
                && f.batch_groups >= 1
                && f.epochs >= 1
                && f.heldout_curve_points >= 1,
            "invalid fitting constants",
        )?;
        let seeds: BTreeSet<u32> = f.fitting_seeds.iter().copied().collect();
        ensure(
            !seeds.is_empty() && seeds.len() == f.fitting_seeds.len() && !seeds.contains(&0),
            "fitting seeds must be distinct and positive",
        )?;
        ensure(
            valid_domain_v1(&f.minibatch_seed_domain) && valid_domain_v1(&f.widen_seed_domain),
            "invalid fitting seed domain",
        )?;
        let q = &self.qualification;
        ensure(
            q.collect_games >= 1
                && q.fit_steps >= 1
                && q.fit_heldout_games >= 1
                && valid_domain_v1(&q.seed_domain),
            "invalid qualification settings",
        )?;
        let domains = BTreeSet::from([
            c.seed_domain.as_str(),
            f.minibatch_seed_domain.as_str(),
            f.widen_seed_domain.as_str(),
            q.seed_domain.as_str(),
            self.analysis.bootstrap_seed_domain.as_str(),
        ]);
        ensure(domains.len() == 5, "seed domains must be distinct")?;
        let n = &self.analysis;
        ensure(
            n.bootstrap_draws >= 1
                && n.interval_lower_index < n.interval_upper_index
                && n.interval_upper_index < n.bootstrap_draws
                && n.bootstrap_draw_order == BOOTSTRAP_DRAW_ORDER_V1
                && valid_domain_v1(&n.bootstrap_seed_domain)
                && n.fund_lower_bound_above.is_finite()
                && n.fund_min_point_r.is_finite()
                && n.t_critical_df7.is_finite()
                && n.t_critical_df7 > 0.0
                && (0.0..=1.0).contains(&n.max_pooled_nonnatural_holdout_fraction),
            "invalid analysis constants",
        )?;
        ensure(
            !self.analyzer.path.is_empty() && is_hex64_v1(&self.analyzer.sha256),
            "invalid analyzer pin",
        )?;
        ensure(
            self.note.collab_commit.len() >= 8 && self.note.blob.len() >= 8,
            "invalid note reference",
        )
    }

    pub(crate) fn generation_v1(&self) -> FreshLineageGenerationV1 {
        FreshLineageGenerationV1::V4
    }

    pub(crate) fn chain_v1(&self, index: u32) -> Result<&PrescreenChainV1, String> {
        self.chains
            .iter()
            .find(|chain| chain.index == index)
            .ok_or_else(|| format!("chain {index} is not in the config"))
    }
}

/// A validated config and the SHA-256 of its exact bytes.
#[derive(Clone, Debug)]
pub(crate) struct LoadedConfigV1 {
    pub(crate) config: PrescreenConfigV1,
    pub(crate) sha256: String,
}

pub(crate) fn load_config_v1(path: &Path) -> Result<LoadedConfigV1, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let config: PrescreenConfigV1 = serde_json::from_slice(&bytes).map_err(err)?;
    config.validate_v1()?;
    Ok(LoadedConfigV1 {
        config,
        sha256: sha(&bytes),
    })
}

// ---------------------------------------------------------------------------
// Seeds and schedule (note sections 2 and 3).
// ---------------------------------------------------------------------------

/// The first 8 bytes, little-endian, of SHA-256 of the UTF-8 string
/// `domain|p1|p2|...` with decimal integers.
pub(crate) fn domain_seed_v1(domain: &str, parts: &[u64]) -> u64 {
    let mut text = domain.to_owned();
    for part in parts {
        text.push('|');
        text.push_str(&part.to_string());
    }
    let digest = Sha256::digest(text.as_bytes());
    u64::from_le_bytes(
        digest[..8]
            .try_into()
            .expect("a SHA-256 digest has 8 bytes"),
    )
}

pub(crate) fn minibatch_seed_v1(config: &PrescreenConfigV1, chain: u32, seed: u32) -> u64 {
    domain_seed_v1(
        &config.fitting.minibatch_seed_domain,
        &[chain.into(), seed.into()],
    )
}

pub(crate) fn widen_seed_v1(config: &PrescreenConfigV1, chain: u32, seed: u32) -> u64 {
    domain_seed_v1(
        &config.fitting.widen_seed_domain,
        &[chain.into(), seed.into()],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SplitV1 {
    Fit,
    Holdout,
}

/// One scheduled game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GamePlanV1 {
    pub(crate) id: String,
    pub(crate) chain: u32,
    pub(crate) focal: u32,
    pub(crate) opposing: u32,
    pub(crate) index: u32,
    pub(crate) split: SplitV1,
    pub(crate) seed: u64,
    pub(crate) focal_seat: u8,
    pub(crate) starting_player: u8,
}

pub(crate) fn game_id_v1(chain: u32, focal: u32, opposing: u32, index: u32) -> String {
    format!("bwps-k{chain}-f{focal}-o{opposing}-i{index}")
}

/// A chain's games in schedule order: focal deck, then opposing deck, then
/// index.
pub(crate) fn chain_schedule_v1(config: &PrescreenConfigV1, chain: u32) -> Vec<GamePlanV1> {
    let c = &config.collection;
    let decks = config.decks.len() as u32;
    let mut plans = Vec::new();
    for focal in 0..decks {
        for opposing in 0..decks {
            for index in 0..c.games_per_pair {
                let split = if c.fit_indices.contains(&index) {
                    SplitV1::Fit
                } else {
                    SplitV1::Holdout
                };
                let starting_player = match c.starting_player_rule {
                    StartingPlayerRuleV1::IndexPlusFocalPlusOpposingMod2V1 => {
                        ((index + focal + opposing) % 2) as u8
                    }
                };
                plans.push(GamePlanV1 {
                    id: game_id_v1(chain, focal, opposing, index),
                    chain,
                    focal,
                    opposing,
                    index,
                    split,
                    seed: domain_seed_v1(
                        &c.seed_domain,
                        &[chain.into(), focal.into(), opposing.into(), index.into()],
                    ),
                    focal_seat: c.focal_seat,
                    starting_player,
                });
            }
        }
    }
    plans
}

/// The RT1 episode of one plan: the focal deck at the focal (learner) seat,
/// the parent at both seats, OH2's limits and runtime rules.
pub(crate) fn episode_v1(
    config: &PrescreenConfigV1,
    plan: &GamePlanV1,
    source: &ExpandedModelSourceV1,
) -> ExpandedEpisodeV1 {
    let c = &config.collection;
    let focal = config.decks[plan.focal as usize].clone();
    let opposing = config.decks[plan.opposing as usize].clone();
    let seats = if plan.focal_seat == 0 {
        [focal, opposing]
    } else {
        [opposing, focal]
    };
    ExpandedEpisodeV1 {
        id: plan.id.clone(),
        seed: plan.seed,
        starting_player: plan.starting_player,
        learner_seat: plan.focal_seat,
        opponent: Some(source.clone()),
        opponent_search: None,
        opponent_kind: None,
        registered: seats.clone(),
        selected: seats,
        postboard: c.postboard,
        max_physical_decisions: c.max_physical_decisions,
        max_policy_steps: c.max_policy_steps,
        runtime_rules: Some(c.runtime_rules),
        reset: None,
    }
}

// ---------------------------------------------------------------------------
// Store encoding.
// ---------------------------------------------------------------------------

fn put_u32_v1(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_len_v1(out: &mut Vec<u8>, len: usize) -> Result<(), String> {
    put_u32_v1(
        out,
        u32::try_from(len).map_err(|_| "store length overflow")?,
    );
    Ok(())
}

fn encode_f32s_v1(out: &mut Vec<u8>, values: &[f32]) -> Result<(), String> {
    put_len_v1(out, values.len())?;
    let mut i = 0;
    while i < values.len() {
        let zeros = i;
        while i < values.len() && values[i].to_bits() == 0 {
            i += 1;
        }
        let literals = i;
        while i < values.len() && values[i].to_bits() != 0 {
            i += 1;
        }
        put_len_v1(out, literals - zeros)?;
        put_len_v1(out, i - literals)?;
        for value in &values[literals..i] {
            put_u32_v1(out, value.to_bits());
        }
    }
    Ok(())
}

fn encode_i64s_v1(out: &mut Vec<u8>, values: &[i64]) -> Result<(), String> {
    put_len_v1(out, values.len())?;
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

/// One game's decision groups in the store format (module documentation).
pub(crate) fn encode_game_v1(groups: &[ScreenDecisionGroupV1]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    put_len_v1(&mut out, groups.len())?;
    for group in groups {
        out.push(group.actor);
        put_len_v1(&mut out, group.substeps.len())?;
        for substep in &group.substeps {
            put_len_v1(&mut out, substep.selected_action_index)?;
            let t = &substep.tensor;
            encode_f32s_v1(&mut out, &t.state)?;
            encode_f32s_v1(&mut out, &t.object_features)?;
            encode_i64s_v1(&mut out, &t.object_card_ids)?;
            encode_i64s_v1(&mut out, &t.object_groups)?;
            encode_i64s_v1(&mut out, &t.object_node_ids)?;
            encode_f32s_v1(&mut out, &t.edge_features)?;
            encode_i64s_v1(&mut out, &t.edge_source_indices)?;
            encode_i64s_v1(&mut out, &t.edge_target_indices)?;
            encode_f32s_v1(&mut out, &t.action_features)?;
            encode_f32s_v1(&mut out, &t.action_ref_features)?;
            encode_i64s_v1(&mut out, &t.action_ref_card_ids)?;
            encode_i64s_v1(&mut out, &t.action_ref_action_indices)?;
            encode_i64s_v1(&mut out, &t.action_ref_node_indices)?;
        }
    }
    Ok(out)
}

struct StoreReaderV1<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl StoreReaderV1<'_> {
    fn take_v1(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("store record is truncated")?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u32_v1(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take_v1(4)?.try_into().expect("four bytes"),
        ))
    }

    fn len_v1(&mut self) -> Result<usize, String> {
        Ok(self.u32_v1()? as usize)
    }

    fn f32s_v1(&mut self) -> Result<Vec<f32>, String> {
        let n = self.len_v1()?;
        ensure(
            n <= self.bytes.len().saturating_mul(1024),
            "store f32 length is implausible",
        )?;
        let mut values = Vec::with_capacity(n.min(1 << 20));
        while values.len() < n {
            let zeros = self.len_v1()?;
            let literals = self.len_v1()?;
            ensure(
                (zeros > 0 || values.is_empty())
                    && (literals > 0 || values.len() + zeros == n)
                    && zeros + literals > 0
                    && values.len() + zeros + literals <= n,
                "store f32 runs are not canonical",
            )?;
            values.resize(values.len() + zeros, 0.0);
            for _ in 0..literals {
                let bits = self.u32_v1()?;
                ensure(bits != 0, "store f32 runs are not canonical")?;
                values.push(f32::from_bits(bits));
            }
        }
        Ok(values)
    }

    fn i64s_v1(&mut self) -> Result<Vec<i64>, String> {
        let n = self.len_v1()?;
        let bytes = self.take_v1(n.checked_mul(8).ok_or("store length overflow")?)?;
        Ok(bytes
            .chunks_exact(8)
            .map(|chunk| i64::from_le_bytes(chunk.try_into().expect("eight bytes")))
            .collect())
    }
}

/// Decodes [`encode_game_v1`]'s bytes, refusing trailing bytes and any
/// non-canonical run.
pub(crate) fn decode_game_v1(bytes: &[u8]) -> Result<Vec<ScreenDecisionGroupV1>, String> {
    let mut r = StoreReaderV1 { bytes, at: 0 };
    let group_count = r.len_v1()?;
    let mut groups = Vec::with_capacity(group_count.min(1 << 16));
    for _ in 0..group_count {
        let actor = r.take_v1(1)?[0];
        let substep_count = r.len_v1()?;
        let mut substeps = Vec::with_capacity(substep_count.min(1 << 16));
        for _ in 0..substep_count {
            let selected_action_index = r.len_v1()?;
            let tensor = NativeFlatDecisionTensorV2 {
                state: r.f32s_v1()?,
                object_features: r.f32s_v1()?,
                object_card_ids: r.i64s_v1()?,
                object_groups: r.i64s_v1()?,
                object_node_ids: r.i64s_v1()?,
                edge_features: r.f32s_v1()?,
                edge_source_indices: r.i64s_v1()?,
                edge_target_indices: r.i64s_v1()?,
                action_features: r.f32s_v1()?,
                action_ref_features: r.f32s_v1()?,
                action_ref_card_ids: r.i64s_v1()?,
                action_ref_action_indices: r.i64s_v1()?,
                action_ref_node_indices: r.i64s_v1()?,
            };
            substeps.push(ScreenSubstepV1 {
                tensor,
                selected_action_index,
            });
        }
        groups.push(ScreenDecisionGroupV1 { actor, substeps });
    }
    ensure(r.at == bytes.len(), "store record has trailing bytes")?;
    Ok(groups)
}

// ---------------------------------------------------------------------------
// Files.
// ---------------------------------------------------------------------------

fn write_file_v1(path: &Path, bytes: &[u8], replace: bool) -> Result<(), String> {
    ensure(
        replace || !path.exists(),
        &format!("{} already exists", path.display()),
    )?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    let name = path
        .file_name()
        .ok_or("output path has no file name")?
        .to_string_lossy()
        .into_owned();
    let temporary = path.with_file_name(format!(".{name}.tmp-{}", std::process::id()));
    {
        let mut file = fs::File::create(&temporary).map_err(err)?;
        file.write_all(bytes).map_err(err)?;
        file.sync_all().map_err(err)?;
    }
    if replace && path.exists() {
        fs::remove_file(path).map_err(err)?;
    }
    fs::rename(&temporary, path).map_err(err)
}

fn json_bytes_v1<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(err)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn read_v1(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn collection_dir_v1(root: &Path, chain: u32) -> PathBuf {
    root.join("collect").join(format!("k{chain}"))
}

fn data_path_v1(root: &Path, chain: u32) -> PathBuf {
    root.join("data").join(format!("k{chain}.json"))
}

fn section7_path_v1(root: &Path) -> PathBuf {
    root.join("data").join("SECTION7.json")
}

fn preservation_path_v1(root: &Path, chain: u32) -> PathBuf {
    root.join("preserve").join(format!("k{chain}.json"))
}

pub(crate) fn fit_path_v1(root: &Path, chain: u32, seed: u32, arm: &str) -> PathBuf {
    root.join("fits")
        .join(format!("k{chain}-s{seed}-{arm}.json"))
}

fn engine_commit_v1() -> &'static str {
    env!("MTG_KERNEL_BUILD_GIT_HEAD")
}

fn host_name_v1() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

fn binary_sha256_v1() -> Result<String, String> {
    let path = std::env::current_exe().map_err(err)?;
    Ok(sha(&read_v1(&path)?))
}

// ---------------------------------------------------------------------------
// Parent and collection.
// ---------------------------------------------------------------------------

/// A chain's parent: the RT1 learner as `Collect` initializes it, and its
/// full train state.
pub(crate) struct ParentV1 {
    pub(crate) source: ExpandedModelSourceV1,
    policy: FrozenPlayPolicyV1,
    learner: ExpandedSeatBehaviorV1,
    pub(crate) state: NativePolicyValueTrainStateV1,
    pub(crate) state_sha256: String,
}

pub(crate) fn load_parent_v1(
    config: &PrescreenConfigV1,
    source: &ExpandedModelSourceV1,
) -> Result<ParentV1, String> {
    let (mut policy, mut state, transfer) = initialize_with_transfer_context(source)?;
    ensure(
        transfer.is_none(),
        "a registry-transfer source is not a screen parent",
    )?;
    let c = &config.collection;
    bind_training_math_v1(source, None, c.training_math, &mut policy, &mut state)?;
    c.collection_sampler.apply_to_learner_v1(&mut policy);
    let identity = policy.feature_identity_v1();
    ensure(
        identity.generation == config.generation_v1()
            && identity.feature_contract_digest == config.feature_contract_digest
            && identity.feature_encoding_digest == config.feature_encoding_digest
            && no_state_digest_v1::effective_contract_digest_v1(&policy)
                == config.feature_contract_digest,
        "the parent's input generation or feature digests differ from the note's",
    )?;
    let state_sha256 = hex(&state.state_sha256_v1().map_err(err)?);
    let learner = ExpandedSeatBehaviorV1 {
        source: source.clone(),
        identity: inference_identity_with_state_sha256_v1(
            source,
            &policy,
            &state,
            state_sha256.clone(),
        ),
    };
    Ok(ParentV1 {
        source: source.clone(),
        policy,
        learner,
        state,
        state_sha256,
    })
}

/// A natural game's groups from its trajectory, refusing any recorded
/// decision whose feature digests differ from the note's.
fn screen_groups_from_trajectory_v1(
    config: &PrescreenConfigV1,
    trajectory: &ExpandedTrajectoryV1,
) -> Result<Vec<ScreenDecisionGroupV1>, String> {
    ensure(
        trajectory.feature_contract_digest == config.feature_contract_digest
            && trajectory.feature_encoding_digest == config.feature_encoding_digest,
        "a recorded decision's feature digests differ from the note's",
    )?;
    let mut groups: Vec<ScreenDecisionGroupV1> = Vec::new();
    for row in &trajectory.decisions {
        if row.substep_index == 0 {
            groups.push(ScreenDecisionGroupV1 {
                actor: row.actor,
                substeps: Vec::new(),
            });
        }
        let group = groups
            .last_mut()
            .ok_or("a physical decision does not start at substep 0")?;
        ensure(
            group.actor == row.actor && group.substeps.len() == row.substep_index as usize,
            "a physical decision's substeps are not contiguous",
        )?;
        group.substeps.push(ScreenSubstepV1 {
            tensor: row.tensor.tensor(),
            selected_action_index: row.selected as usize,
        });
    }
    ensure(!groups.is_empty(), "a natural game recorded no decision")?;
    Ok(groups)
}

/// One played game: its terminal, and for a natural game its store bytes.
pub(crate) struct CollectedGameV1 {
    pub(crate) terminal: RlSessionTerminalV1,
    pub(crate) stored: Option<StoredGameV1>,
}

pub(crate) struct StoredGameV1 {
    pub(crate) bytes: Vec<u8>,
    pub(crate) groups: usize,
    pub(crate) substeps: usize,
}

struct ScreenCollectorV1 {
    policy: FrozenPlayPolicyV1,
    opponents: OpponentCacheV1,
}

/// Plays every plan once with `workers` collectors: no retry, no
/// replacement. A non-natural game returns its terminal; any other
/// collection error is fatal.
pub(crate) fn collect_games_v1(
    config: &PrescreenConfigV1,
    parent: &ParentV1,
    plans: &[GamePlanV1],
    workers: usize,
) -> Result<Vec<CollectedGameV1>, String> {
    ensure(!plans.is_empty(), "no games to collect")?;
    let episodes: Vec<ExpandedEpisodeV1> = plans
        .iter()
        .map(|plan| episode_v1(config, plan, &parent.source))
        .collect();
    let mut ids = BTreeSet::new();
    for episode in &episodes {
        episode.configurations()?;
        ensure(ids.insert(&episode.id), "duplicate episode id")?;
    }
    let verified = verify_collection_opponent_sources_v1(&episodes)?;
    let math = config.collection.training_math;
    let contexts = (0..workers.clamp(1, episodes.len()))
        .map(|_| {
            Ok(ScreenCollectorV1 {
                policy: parent.policy.fork_for_collection_v3()?,
                opponents: OpponentCacheV1::for_collection(verified.clone())
                    .with_training_math_v1(math),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let source = &parent.source;
    let learner = &parent.learner;
    let (games, _) = ordered_parallel_jobs_v1(
        contexts,
        episodes.len(),
        |collector, index| {
            // The current-self opponent: a private fork of the learner under
            // the legacy sampler, as `collect_parallel_v1` serves RT1's
            // `current` assignment.
            if !collector.opponents.contains(source) {
                collector.opponents.insert(LoadedOpponentV1 {
                    policy: collector
                        .policy
                        .fork_for_collection_v3()?
                        .with_legacy_collection_sampler_v1(),
                    behavior: learner.clone(),
                });
            }
            let opponent = collector.opponents.load(source)?;
            match collect_episode(
                &mut collector.policy,
                learner,
                Some(opponent),
                &episodes[index],
            ) {
                Ok(trajectory) => {
                    let groups = screen_groups_from_trajectory_v1(config, &trajectory)?;
                    Ok(CollectedGameV1 {
                        stored: Some(StoredGameV1 {
                            bytes: encode_game_v1(&groups)?,
                            groups: groups.len(),
                            substeps: groups.iter().map(|g| g.substeps.len()).sum(),
                        }),
                        terminal: trajectory.terminal,
                    })
                }
                Err(CollectEpisodeErrorV1::NonNatural(terminal)) => Ok(CollectedGameV1 {
                    terminal: *terminal,
                    stored: None,
                }),
                Err(CollectEpisodeErrorV1::Other(error)) => Err(error),
            }
        },
        || (),
    )?;
    Ok(games)
}

/// One attempted game in the collection record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CollectedGameEntryV1 {
    pub(crate) plan: GamePlanV1,
    pub(crate) natural: bool,
    pub(crate) terminal: RlSessionTerminalV1,
    pub(crate) groups: usize,
    pub(crate) substeps: usize,
    pub(crate) offset: u64,
    pub(crate) length: u64,
    pub(crate) sha256: Option<String>,
}

/// A chain's collection record (`collect/k<k>/collection.json`). It holds no
/// timing, so a rerun from the same inputs reproduces its bytes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CollectionRecordV1 {
    pub(crate) schema: String,
    pub(crate) config_sha256: String,
    pub(crate) engine_commit: String,
    pub(crate) chain: u32,
    pub(crate) chain_id: String,
    pub(crate) source: ExpandedModelSourceV1,
    pub(crate) parent_state_sha256: String,
    pub(crate) parent_adam_step: u64,
    pub(crate) learner: ExpandedSeatBehaviorV1,
    pub(crate) feature_contract_digest: String,
    pub(crate) feature_encoding_digest: String,
    pub(crate) collection: PrescreenCollectionV1,
    pub(crate) decks: Vec<ExpandedDeckListV1>,
    pub(crate) store_sha256: String,
    pub(crate) store_bytes: u64,
    pub(crate) games: Vec<CollectedGameEntryV1>,
}

/// `collect`: one chain's games, played once each, into its store and
/// collection record. A chain whose record exists is verified and skipped.
pub(crate) fn collect_chain_stage_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
    chain: &PrescreenChainV1,
    workers: usize,
) -> Result<Value, String> {
    let directory = collection_dir_v1(root, chain.index);
    if directory.exists() {
        read_collection_v1(loaded, root, chain)?;
        return Ok(json!({"chain": chain.index, "collect": "verified-existing"}));
    }
    let config = &loaded.config;
    let started = Instant::now();
    let parent = load_parent_v1(config, &chain.source)?;
    let plans = chain_schedule_v1(config, chain.index);
    let games = collect_games_v1(config, &parent, &plans, workers)?;
    let mut store = STORE_MAGIC_V1.to_vec();
    let mut entries = Vec::with_capacity(games.len());
    for (plan, game) in plans.into_iter().zip(games) {
        let offset = store.len() as u64;
        let (natural, groups, substeps, length, digest) = match &game.stored {
            Some(stored) => {
                store.extend_from_slice(&stored.bytes);
                (
                    true,
                    stored.groups,
                    stored.substeps,
                    stored.bytes.len() as u64,
                    Some(sha(&stored.bytes)),
                )
            }
            None => (false, 0, 0, 0, None),
        };
        entries.push(CollectedGameEntryV1 {
            plan,
            natural,
            terminal: game.terminal,
            groups,
            substeps,
            offset,
            length,
            sha256: digest,
        });
    }
    let record = CollectionRecordV1 {
        schema: COLLECTION_SCHEMA_V1.into(),
        config_sha256: loaded.sha256.clone(),
        engine_commit: engine_commit_v1().into(),
        chain: chain.index,
        chain_id: chain.id.clone(),
        source: chain.source.clone(),
        parent_state_sha256: parent.state_sha256.clone(),
        parent_adam_step: parent.state.adam_step_v1(),
        learner: parent.learner.clone(),
        feature_contract_digest: config.feature_contract_digest.clone(),
        feature_encoding_digest: config.feature_encoding_digest.clone(),
        collection: config.collection.clone(),
        decks: config.decks.clone(),
        store_sha256: sha(&store),
        store_bytes: store.len() as u64,
        games: entries,
    };
    // The chain directory appears whole or not at all.
    let partial =
        root.join("collect")
            .join(format!(".k{}-partial-{}", chain.index, std::process::id()));
    if partial.exists() {
        fs::remove_dir_all(&partial).map_err(err)?;
    }
    write_file_v1(&partial.join("games.bin"), &store, false)?;
    write_file_v1(
        &partial.join("collection.json"),
        &json_bytes_v1(&record)?,
        false,
    )?;
    fs::rename(&partial, &directory).map_err(err)?;
    let natural = record.games.iter().filter(|game| game.natural).count();
    let summary = json!({"chain": chain.index, "collect": "written", "games": record.games.len(),
        "natural": natural, "store_bytes": record.store_bytes,
        "elapsed_seconds": started.elapsed().as_secs_f64(), "workers": workers,
        "host": host_name_v1()});
    write_file_v1(
        &root
            .join("collect")
            .join(format!("k{}.timing.json", chain.index)),
        &json_bytes_v1(&summary)?,
        true,
    )?;
    Ok(summary)
}

/// A chain's verified collection: its record, the record's bytes and the
/// store.
pub(crate) struct ChainCollectionV1 {
    pub(crate) record: CollectionRecordV1,
    pub(crate) record_bytes: Vec<u8>,
    pub(crate) store: Vec<u8>,
}

pub(crate) fn read_collection_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
    chain: &PrescreenChainV1,
) -> Result<ChainCollectionV1, String> {
    let directory = collection_dir_v1(root, chain.index);
    let record_bytes = read_v1(&directory.join("collection.json"))?;
    let record: CollectionRecordV1 = serde_json::from_slice(&record_bytes).map_err(err)?;
    let config = &loaded.config;
    ensure(
        record.schema == COLLECTION_SCHEMA_V1
            && record.config_sha256 == loaded.sha256
            && record.chain == chain.index
            && record.source == chain.source,
        "the collection record is not this config's chain",
    )?;
    let store = read_v1(&directory.join("games.bin"))?;
    ensure(
        sha(&store) == record.store_sha256
            && store.len() as u64 == record.store_bytes
            && store.starts_with(STORE_MAGIC_V1),
        "the store differs from its collection record",
    )?;
    let plans = chain_schedule_v1(config, chain.index);
    ensure(
        record.games.len() == plans.len()
            && record
                .games
                .iter()
                .zip(&plans)
                .all(|(entry, plan)| entry.plan == *plan),
        "the collection record differs from the config's schedule",
    )?;
    for entry in &record.games {
        let natural = entry.terminal.terminal_classification == TerminalClassificationV1::Natural;
        ensure(
            entry.natural == natural && entry.natural == entry.sha256.is_some(),
            "a collection entry's natural flag differs from its terminal",
        )?;
        if let Some(digest) = &entry.sha256 {
            let start = usize::try_from(entry.offset).map_err(err)?;
            let end = start
                .checked_add(usize::try_from(entry.length).map_err(err)?)
                .filter(|end| *end <= store.len())
                .ok_or("a stored game lies outside the store")?;
            ensure(
                sha(&store[start..end]) == *digest,
                "a stored game differs from its collection entry",
            )?;
        }
    }
    Ok(ChainCollectionV1 {
        record,
        record_bytes,
        store,
    })
}

// ---------------------------------------------------------------------------
// Convert: retained games and the chain data record (note section 7).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SplitIdsV1 {
    pub(crate) fit: Vec<String>,
    pub(crate) holdout: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExcludedGameV1 {
    pub(crate) id: String,
    pub(crate) split: SplitV1,
    pub(crate) focal: u32,
    pub(crate) opposing: u32,
    pub(crate) terminal_classification: TerminalClassificationV1,
    pub(crate) terminal_reason: String,
}

/// A retained holdout game, for the analyzer's pair weighting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HoldoutGameV1 {
    pub(crate) id: String,
    pub(crate) focal: u32,
    pub(crate) opposing: u32,
    pub(crate) index: u32,
    pub(crate) groups: usize,
}

/// A chain's data record (`data/k<k>.json`): section 7's per-chain items.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DataRecordV1 {
    pub(crate) schema: String,
    pub(crate) config_sha256: String,
    pub(crate) chain: u32,
    pub(crate) chain_id: String,
    pub(crate) parent_checkpoint_sha256: Option<String>,
    pub(crate) parent_state_sha256: String,
    pub(crate) collection_record_sha256: String,
    pub(crate) store_sha256: String,
    pub(crate) attempted: SplitIdsV1,
    pub(crate) natural: SplitIdsV1,
    pub(crate) excluded: Vec<ExcludedGameV1>,
    pub(crate) fit_groups: usize,
    pub(crate) holdout_groups: usize,
    pub(crate) steps: u64,
    pub(crate) heldout_every_steps: u64,
    pub(crate) outcome_source_sha256: String,
    pub(crate) decision_source_sha256: String,
    pub(crate) holdout_games: Vec<HoldoutGameV1>,
}

/// A chain's retained records, shared read-only by every fit of the chain.
pub(crate) struct ChainDataV1 {
    pub(crate) record: DataRecordV1,
    pub(crate) record_bytes: Vec<u8>,
    pub(crate) fit: Vec<ScreenGameRecordV1>,
    pub(crate) holdout: Vec<ScreenGameRecordV1>,
}

/// The note's step rule: ceil(epochs * fit_groups / batch_groups), and
/// floor(steps / curve points) between held-out evaluations.
pub(crate) fn step_rule_v1(fitting: &PrescreenFittingV1, fit_groups: usize) -> (u64, u64) {
    let numerator = fitting.epochs * fit_groups as u64;
    let steps = numerator.div_ceil(fitting.batch_groups as u64);
    (steps, steps / fitting.heldout_curve_points)
}

/// The chain's data, rebuilt from its verified collection.
pub(crate) fn build_chain_data_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
    chain: &PrescreenChainV1,
) -> Result<ChainDataV1, String> {
    let collection = read_collection_v1(loaded, root, chain)?;
    let mut attempted = SplitIdsV1 {
        fit: Vec::new(),
        holdout: Vec::new(),
    };
    let mut natural = attempted.clone();
    let mut excluded = Vec::new();
    let mut fit = Vec::new();
    let mut holdout = Vec::new();
    let mut holdout_games = Vec::new();
    for entry in &collection.record.games {
        let plan = &entry.plan;
        let (ids, retained) = match plan.split {
            SplitV1::Fit => (&mut attempted.fit, &mut natural.fit),
            SplitV1::Holdout => (&mut attempted.holdout, &mut natural.holdout),
        };
        ids.push(plan.id.clone());
        if !entry.natural {
            excluded.push(ExcludedGameV1 {
                id: plan.id.clone(),
                split: plan.split,
                focal: plan.focal,
                opposing: plan.opposing,
                terminal_classification: entry.terminal.terminal_classification,
                terminal_reason: entry.terminal.terminal_reason.clone(),
            });
            continue;
        }
        retained.push(plan.id.clone());
        let start = entry.offset as usize;
        let groups = decode_game_v1(&collection.store[start..start + entry.length as usize])?;
        ensure(
            groups.len() == entry.groups,
            "a stored game's group count differs from its entry",
        )?;
        let game = ScreenGameRecordV1 {
            game_id: plan.id.clone(),
            outcome: ScreenGameOutcomeV1::from(&entry.terminal),
            groups,
        };
        match plan.split {
            SplitV1::Fit => fit.push(game),
            SplitV1::Holdout => {
                holdout_games.push(HoldoutGameV1 {
                    id: plan.id.clone(),
                    focal: plan.focal,
                    opposing: plan.opposing,
                    index: plan.index,
                    groups: entry.groups,
                });
                holdout.push(game);
            }
        }
    }
    let fit_groups: usize = fit.iter().map(|game| game.groups.len()).sum();
    let holdout_groups: usize = holdout.iter().map(|game| game.groups.len()).sum();
    let (steps, heldout_every_steps) = step_rule_v1(&loaded.config.fitting, fit_groups);
    let splits = [("fit", fit.as_slice()), ("heldout", holdout.as_slice())];
    let record = DataRecordV1 {
        schema: DATA_SCHEMA_V1.into(),
        config_sha256: loaded.sha256.clone(),
        chain: chain.index,
        chain_id: chain.id.clone(),
        parent_checkpoint_sha256: chain
            .source
            .checkpoint
            .as_ref()
            .map(|pin| pin.sha256.clone()),
        parent_state_sha256: collection.record.parent_state_sha256.clone(),
        collection_record_sha256: sha(&collection.record_bytes),
        store_sha256: collection.record.store_sha256.clone(),
        attempted,
        natural,
        excluded,
        fit_groups,
        holdout_groups,
        steps,
        heldout_every_steps,
        outcome_source_sha256: outcome_source_sha256_v1(splits),
        decision_source_sha256: decision_source_sha256_v1(splits),
        holdout_games,
    };
    Ok(ChainDataV1 {
        record_bytes: json_bytes_v1(&record)?,
        record,
        fit,
        holdout,
    })
}

/// The chain's data, which `convert` must already have written with exactly
/// these bytes.
pub(crate) fn load_chain_data_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
    chain: &PrescreenChainV1,
) -> Result<ChainDataV1, String> {
    let data = build_chain_data_v1(loaded, root, chain)?;
    ensure(
        read_v1(&data_path_v1(root, chain.index))? == data.record_bytes,
        "the chain's data record differs from its collection",
    )?;
    Ok(data)
}

/// `convert`: writes (or, when present, re-verifies) the chain's data record.
pub(crate) fn convert_chain_stage_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
    chain: &PrescreenChainV1,
) -> Result<Value, String> {
    let data = build_chain_data_v1(loaded, root, chain)?;
    let path = data_path_v1(root, chain.index);
    let status = if path.exists() {
        ensure(
            read_v1(&path)? == data.record_bytes,
            "the existing data record differs from its collection",
        )?;
        "verified-existing"
    } else {
        write_file_v1(&path, &data.record_bytes, false)?;
        "written"
    };
    Ok(json!({"chain": chain.index, "convert": status,
        "fit_games": data.fit.len(), "holdout_games": data.holdout.len(),
        "excluded": data.record.excluded.len(), "fit_groups": data.record.fit_groups,
        "steps": data.record.steps}))
}

/// Section 7 across every chain, once each has a data record. Returns None
/// while a chain is missing.
pub(crate) fn section7_stage_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
) -> Result<Option<Value>, String> {
    let mut chains = Vec::new();
    for chain in &loaded.config.chains {
        let path = data_path_v1(root, chain.index);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = read_v1(&path)?;
        let record: DataRecordV1 = serde_json::from_slice(&bytes).map_err(err)?;
        ensure(
            record.config_sha256 == loaded.sha256 && record.chain == chain.index,
            "a data record is not this config's chain",
        )?;
        chains.push(json!({"chain": record.chain, "chain_id": record.chain_id,
            "parent_checkpoint_sha256": record.parent_checkpoint_sha256,
            "collection_record_sha256": record.collection_record_sha256,
            "data_record_sha256": sha(&bytes),
            "attempted": record.attempted, "natural": record.natural, "excluded": record.excluded,
            "outcome_source_sha256": record.outcome_source_sha256,
            "decision_source_sha256": record.decision_source_sha256,
            "fit_groups": record.fit_groups, "steps": record.steps}));
    }
    let document = json!({"schema": SECTION7_SCHEMA_V1, "note": loaded.config.note,
        "config_sha256": loaded.sha256, "analyzer": loaded.config.analyzer,
        "engine_commit": engine_commit_v1(), "binary_sha256": binary_sha256_v1()?,
        "chains": chains});
    let bytes = json_bytes_v1(&document)?;
    let path = section7_path_v1(root);
    if path.exists() {
        ensure(
            read_v1(&path)? == bytes,
            "the existing SECTION7.json differs from the data records",
        )?;
    } else {
        write_file_v1(&path, &bytes, false)?;
    }
    Ok(Some(json!({"section7": path, "sha256": sha(&bytes)})))
}

// ---------------------------------------------------------------------------
// Arms (note section 3).
// ---------------------------------------------------------------------------

/// The parent's net-8 train state, rebuilt from its snapshot on the contract
/// template, as the widening and the w32 control build theirs.
pub(crate) fn narrow_state_v1(
    snapshot: &NativePolicyValueTrainSnapshotV1,
) -> Result<NativePolicyValueTrainStateV1, String> {
    let mut template =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(err)?;
    template
        .replace_parameter_snapshot_v1(&snapshot.parameters)
        .map_err(err)?;
    NativePolicyValueTrainStateV1::from_snapshot_v1(template, snapshot).map_err(err)
}

/// Fresh Adam (note section 3): the same model, parameters and gauge anchor,
/// with every first and second moment +0 and `adam_step` 0. Only optimizer
/// state changes.
pub(crate) fn fresh_adam_v1<D: NetDimsV1>(
    state: &NativePolicyValueTrainStateDimsV1<D>,
) -> Result<NativePolicyValueTrainStateDimsV1<D>, String> {
    ensure(
        state.training_math_dims_v1().is_platform_libm(),
        "the screen trains under the platform libm",
    )?;
    NativePolicyValueTrainStateDimsV1::<D>::new_dims_v1(state.model_dims_v1().clone()).map_err(err)
}

/// One arm's start before fresh Adam, with its provenance.
pub(crate) enum ArmStartV1 {
    Narrow(NativePolicyValueTrainStateV1),
    Wide(NativePolicyValueTrainStateWideV1),
}

pub(crate) struct ArmV1 {
    pub(crate) start: ArmStartV1,
    pub(crate) transform: Value,
    pub(crate) transformed_state_sha256: String,
    pub(crate) widen_seed: Option<u64>,
}

pub(crate) fn arm_start_v1(
    config: &PrescreenConfigV1,
    snapshot: &NativePolicyValueTrainSnapshotV1,
    parent_state_sha256: &str,
    arm: &str,
    widen_seed: u64,
) -> Result<ArmV1, String> {
    let arm = match arm {
        "w64" => ArmV1 {
            start: ArmStartV1::Narrow(narrow_state_v1(snapshot)?),
            transform: json!({"construction_version": "identity",
                "parent_state_sha256": parent_state_sha256,
                "child_state_sha256": parent_state_sha256}),
            transformed_state_sha256: parent_state_sha256.into(),
            widen_seed: None,
        },
        "w128" => {
            let widened = widen_net8_snapshot_v1(
                snapshot,
                &config.arms.widen_target_architecture,
                &config.arms.widen_init_law,
                widen_seed,
            )
            .map_err(err)?;
            ArmV1 {
                transformed_state_sha256: widened.provenance.child_state_sha256.clone(),
                transform: serde_json::to_value(&widened.provenance).map_err(err)?,
                start: ArmStartV1::Wide(widened.state),
                widen_seed: Some(widen_seed),
            }
        }
        "w32" => {
            let control = w32_control_from_net8_snapshot_v1(snapshot).map_err(err)?;
            ArmV1 {
                transformed_state_sha256: control.provenance.child_state_sha256.clone(),
                transform: serde_json::to_value(&control.provenance).map_err(err)?,
                start: ArmStartV1::Narrow(control.state),
                widen_seed: None,
            }
        }
        other => return Err(format!("unknown arm {other}")),
    };
    ensure(
        arm.transform["parent_state_sha256"] == json!(parent_state_sha256),
        "an arm's provenance does not chain to the parent",
    )?;
    let actual = match &arm.start {
        ArmStartV1::Narrow(state) => state.state_sha256_v1(),
        ArmStartV1::Wide(state) => state.state_sha256_v1(),
    }
    .map_err(err)?;
    ensure(
        hex(&actual) == arm.transformed_state_sha256,
        "an arm's state differs from its provenance",
    )?;
    Ok(arm)
}

// ---------------------------------------------------------------------------
// Preserve: step-0 function preservation (note section 6.5).
// ---------------------------------------------------------------------------

/// Runs `work` over `0..count` in chunks on `workers` threads, in order.
fn parallel_chunks_v1<T, F>(count: usize, workers: usize, work: F) -> Result<Vec<T>, String>
where
    T: Send,
    F: Fn(std::ops::Range<usize>) -> Result<T, String> + Sync,
{
    let jobs = count.div_ceil(PRESERVATION_CHUNK_V1);
    if jobs == 0 {
        return Ok(Vec::new());
    }
    let contexts = vec![(); workers.clamp(1, jobs)];
    let (outputs, _) = ordered_parallel_jobs_v1(
        contexts,
        jobs,
        |_, job| {
            let start = job * PRESERVATION_CHUNK_V1;
            work(start..(start + PRESERVATION_CHUNK_V1).min(count))
        },
        || (),
    )?;
    Ok(outputs)
}

/// Every recorded decision input of a chain, fit then holdout, with its
/// location.
fn decision_inputs_v1<'a>(
    fit: &'a [ScreenGameRecordV1],
    holdout: &'a [ScreenGameRecordV1],
) -> Vec<(String, usize, usize, &'a NativeFlatDecisionTensorV2)> {
    let mut inputs = Vec::new();
    for game in fit.iter().chain(holdout) {
        for (group_index, group) in game.groups.iter().enumerate() {
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                inputs.push((
                    game.game_id.clone(),
                    group_index,
                    substep_index,
                    &substep.tensor,
                ));
            }
        }
    }
    inputs
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct PreservationSeedV1 {
    pub(crate) seed: u32,
    pub(crate) widen_seed: u64,
    pub(crate) child_state_sha256: String,
    pub(crate) inputs_compared: usize,
    pub(crate) mismatches: usize,
    pub(crate) first_mismatch: Option<Value>,
}

/// Compares the parent's and each widened net's outputs on every input.
pub(crate) fn preservation_seeds_v1(
    config: &PrescreenConfigV1,
    chain: u32,
    snapshot: &NativePolicyValueTrainSnapshotV1,
    games: [&[ScreenGameRecordV1]; 2],
    seeds: &[u32],
    workers: usize,
) -> Result<(usize, Vec<PreservationSeedV1>), String> {
    let generation = config.generation_v1();
    let inputs = decision_inputs_v1(games[0], games[1]);
    let narrow =
        ScreenForwardV1::<Net8DimsV1>::of_state_v1(&narrow_state_v1(snapshot)?, generation);
    let narrow_outputs: Vec<(Vec<u32>, u32)> =
        parallel_chunks_v1(inputs.len(), workers, |range| {
            inputs[range]
                .iter()
                .map(|input| narrow.output_bits_v1(input.3).map_err(err))
                .collect::<Result<Vec<_>, String>>()
        })?
        .into_iter()
        .flatten()
        .collect();
    let mut results = Vec::new();
    for &seed in seeds {
        let widen_seed = widen_seed_v1(config, chain, seed);
        let widened = widen_net8_snapshot_v1(
            snapshot,
            &config.arms.widen_target_architecture,
            &config.arms.widen_init_law,
            widen_seed,
        )
        .map_err(err)?;
        let wide = ScreenForwardV1::<Net8W128DimsV1>::of_state_v1(&widened.state, generation);
        let chunks = parallel_chunks_v1(inputs.len(), workers, |range| {
            let mut mismatches = 0usize;
            let mut first = None;
            for index in range {
                let actual = wide.output_bits_v1(inputs[index].3).map_err(err)?;
                if actual != narrow_outputs[index] {
                    mismatches += 1;
                    if first.is_none() {
                        let (game, group, substep, _) = &inputs[index];
                        first = Some(json!({"input": index, "game_id": game, "group": group,
                            "substep": substep, "w64_logit_bits": narrow_outputs[index].0,
                            "w64_value_bits": narrow_outputs[index].1,
                            "w128_logit_bits": actual.0, "w128_value_bits": actual.1}));
                    }
                }
            }
            Ok((mismatches, first))
        })?;
        results.push(PreservationSeedV1 {
            seed,
            widen_seed,
            child_state_sha256: widened.provenance.child_state_sha256.clone(),
            inputs_compared: inputs.len(),
            mismatches: chunks.iter().map(|chunk| chunk.0).sum(),
            first_mismatch: chunks.into_iter().find_map(|chunk| chunk.1),
        });
    }
    Ok((inputs.len(), results))
}

/// `preserve`: section 6.5's forward comparison for one chain, every
/// fitting seed. Writes its record, then fails if any input differs.
pub(crate) fn preserve_chain_stage_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
    chain: &PrescreenChainV1,
    workers: usize,
) -> Result<Value, String> {
    let path = preservation_path_v1(root, chain.index);
    let data = load_chain_data_v1(loaded, root, chain)?;
    if path.exists() {
        let existing: Value = serde_json::from_slice(&read_v1(&path)?).map_err(err)?;
        ensure(
            existing["config_sha256"] == json!(loaded.sha256)
                && existing["data_record_sha256"] == json!(sha(&data.record_bytes)),
            "the existing preservation record is not this chain's data",
        )?;
        ensure(
            existing["pass"] == json!(true),
            "the existing preservation record failed",
        )?;
        return Ok(json!({"chain": chain.index, "preserve": "verified-existing"}));
    }
    let started = Instant::now();
    let parent = load_parent_v1(&loaded.config, &chain.source)?;
    ensure(
        parent.state_sha256 == data.record.parent_state_sha256,
        "the parent differs from the collection's",
    )?;
    let snapshot = parent.state.snapshot_v1().map_err(err)?;
    drop(parent);
    let (inputs, seeds) = preservation_seeds_v1(
        &loaded.config,
        chain.index,
        &snapshot,
        [data.fit.as_slice(), data.holdout.as_slice()],
        &loaded.config.fitting.fitting_seeds,
        workers,
    )?;
    let pass = seeds.iter().all(|seed| seed.mismatches == 0);
    let record = json!({"schema": PRESERVATION_SCHEMA_V1, "config_sha256": loaded.sha256,
        "engine_commit": engine_commit_v1(), "chain": chain.index,
        "data_record_sha256": sha(&data.record_bytes),
        "parent_state_sha256": data.record.parent_state_sha256,
        "forward": "screen_fit_v1::ScreenForwardV1 (forward_with_tape_dims_v1)",
        "comparison": "value bits and every policy logit's bits, IEEE bit patterns, no signed-zero exception",
        "inputs": inputs, "seeds": seeds, "pass": pass});
    write_file_v1(&path, &json_bytes_v1(&record)?, false)?;
    ensure(
        pass,
        &format!("chain {}: w128 differs from w64 at step 0", chain.index),
    )?;
    Ok(
        json!({"chain": chain.index, "preserve": "written", "inputs": inputs,
        "elapsed_seconds": started.elapsed().as_secs_f64()}),
    )
}

// ---------------------------------------------------------------------------
// Fit.
// ---------------------------------------------------------------------------

/// One fit's record (`fits/k<k>-s<s>-<arm>.json`). It holds no timing, so a
/// rerun from the same inputs reproduces its bytes.
#[derive(Serialize)]
struct FitRecordV1<'a> {
    schema: &'static str,
    config_sha256: &'a str,
    data_record_sha256: &'a str,
    engine_commit: &'static str,
    chain: u32,
    seed: u32,
    arm: &'a str,
    minibatch_seed: u64,
    widen_seed: Option<u64>,
    parent_state_sha256: &'a str,
    transform: Value,
    transformed_state_sha256: String,
    fresh_adam_state_sha256: String,
    report: ScreenFitReportV1,
}

pub(crate) struct FitInputsV1<'a> {
    pub(crate) loaded: &'a LoadedConfigV1,
    pub(crate) chain: u32,
    pub(crate) data: &'a ChainDataV1,
    pub(crate) snapshot: &'a NativePolicyValueTrainSnapshotV1,
    pub(crate) parent_state_sha256: &'a str,
    pub(crate) forward_workers: usize,
}

/// Fresh Adam on `start`, then the fit entry; returns the post-fresh-Adam
/// state hash and the report.
pub(crate) fn fit_state_v1<D: NetDimsV1>(
    start: &NativePolicyValueTrainStateDimsV1<D>,
    generation: FreshLineageGenerationV1,
    games: [&[ScreenGameRecordV1]; 2],
    config: &ScreenFitConfigV1,
) -> Result<(String, ScreenFitReportV1), String> {
    let mut state = fresh_adam_v1(start)?;
    let fresh = hex(&state.state_sha256_dims_v1().map_err(err)?);
    let report = fit_screen_value_dims_v1(&mut state, generation, games[0], games[1], config)
        .map_err(err)?;
    ensure(
        report.initial_state_sha256 == fresh && report.initial_adam_step == 0,
        "the fit did not start from the recorded fresh-Adam state",
    )?;
    Ok((fresh, report))
}

pub(crate) fn screen_fit_config_v1(
    fitting: &PrescreenFittingV1,
    steps: u64,
    heldout_every_steps: u64,
    minibatch_seed: u64,
    forward_workers: usize,
) -> ScreenFitConfigV1 {
    ScreenFitConfigV1 {
        value_target_source: fitting.value_target_source.clone(),
        steps,
        batch_groups: fitting.batch_groups,
        learning_rate: fitting.learning_rate,
        value_coefficient: fitting.value_coefficient,
        minibatch_seed,
        heldout_every_steps,
        forward_workers,
    }
}

/// One (seed, arm) fit's record bytes.
pub(crate) fn fit_record_bytes_v1(
    inputs: &FitInputsV1<'_>,
    seed: u32,
    arm: &str,
) -> Result<Vec<u8>, String> {
    let config = &inputs.loaded.config;
    let minibatch_seed = minibatch_seed_v1(config, inputs.chain, seed);
    let start = arm_start_v1(
        config,
        inputs.snapshot,
        inputs.parent_state_sha256,
        arm,
        widen_seed_v1(config, inputs.chain, seed),
    )?;
    let fit_config = screen_fit_config_v1(
        &config.fitting,
        inputs.data.record.steps,
        inputs.data.record.heldout_every_steps,
        minibatch_seed,
        inputs.forward_workers,
    );
    let generation = config.generation_v1();
    let games = [inputs.data.fit.as_slice(), inputs.data.holdout.as_slice()];
    let (fresh, report) = match &start.start {
        ArmStartV1::Narrow(state) => fit_state_v1(state, generation, games, &fit_config)?,
        ArmStartV1::Wide(state) => fit_state_v1(state, generation, games, &fit_config)?,
    };
    let data_record_sha256 = sha(&inputs.data.record_bytes);
    json_bytes_v1(&FitRecordV1 {
        schema: FIT_SCHEMA_V1,
        config_sha256: &inputs.loaded.sha256,
        data_record_sha256: &data_record_sha256,
        engine_commit: engine_commit_v1(),
        chain: inputs.chain,
        seed,
        arm,
        minibatch_seed,
        widen_seed: start.widen_seed,
        parent_state_sha256: inputs.parent_state_sha256,
        transform: start.transform,
        transformed_state_sha256: start.transformed_state_sha256,
        fresh_adam_state_sha256: fresh,
        report,
    })
}

/// A present fit record must belong to this config and data.
fn existing_fit_matches_v1(
    path: &Path,
    loaded: &LoadedConfigV1,
    data_sha256: &str,
) -> Result<(), String> {
    let existing: Value = serde_json::from_slice(&read_v1(path)?).map_err(err)?;
    ensure(
        existing["schema"] == json!(FIT_SCHEMA_V1)
            && existing["config_sha256"] == json!(loaded.sha256)
            && existing["data_record_sha256"] == json!(data_sha256),
        &format!("{} is not a report of this config and data", path.display()),
    )
}

/// The step-0 held-out reports of w64 and w128 must be bit-identical in
/// every fitting seed (note section 6.5). serde_json prints each f64 in its
/// shortest round-trip form, sign of zero included, so equal text is equal
/// bits.
fn step0_reports_identical_v1(root: &Path, chain: u32, seeds: &[u32]) -> Result<(), String> {
    for &seed in seeds {
        let report = |arm: &str| -> Result<Vec<u8>, String> {
            let record: Value =
                serde_json::from_slice(&read_v1(&fit_path_v1(root, chain, seed, arm))?)
                    .map_err(err)?;
            let first = &record["report"]["heldout"][0];
            ensure(first["after_step"] == json!(0), "missing step-0 report")?;
            serde_json::to_vec(&first["games"]).map_err(err)
        };
        ensure(
            report("w64")? == report("w128")?,
            &format!("chain {chain} seed {seed}: step-0 held-out reports of w64 and w128 differ"),
        )?;
    }
    Ok(())
}

/// `fit`: every missing (seed, arm) report of one chain, `fit_workers` at a
/// time from one shared read-only record set. Requires the chain's passed
/// preservation record and `SECTION7.json`.
pub(crate) fn fit_chain_stage_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
    chain: &PrescreenChainV1,
    fit_workers: usize,
    forward_workers: usize,
) -> Result<Value, String> {
    ensure(
        (1..=64).contains(&forward_workers),
        "forward workers must be 1 to 64",
    )?;
    let section7: Value = serde_json::from_slice(&read_v1(&section7_path_v1(root))?)
        .map_err(|e| format!("SECTION7.json is required before the first fit: {e}"))?;
    let data = load_chain_data_v1(loaded, root, chain)?;
    let data_sha256 = sha(&data.record_bytes);
    ensure(
        section7["config_sha256"] == json!(loaded.sha256)
            && section7["chains"].as_array().is_some_and(|chains| {
                chains.iter().any(|entry| {
                    entry["chain"] == json!(chain.index)
                        && entry["data_record_sha256"] == json!(data_sha256)
                })
            }),
        "SECTION7.json does not record this chain's data",
    )?;
    let preservation: Value =
        serde_json::from_slice(&read_v1(&preservation_path_v1(root, chain.index))?).map_err(err)?;
    ensure(
        preservation["pass"] == json!(true)
            && preservation["data_record_sha256"] == json!(data_sha256),
        "the chain needs a passed preservation record before fitting",
    )?;
    let seeds = &loaded.config.fitting.fitting_seeds;
    let mut jobs = Vec::new();
    for &seed in seeds {
        for arm in ARM_NAMES_V1 {
            let path = fit_path_v1(root, chain.index, seed, arm);
            if path.exists() {
                existing_fit_matches_v1(&path, loaded, &data_sha256)?;
            } else {
                jobs.push((seed, arm));
            }
        }
    }
    let started = Instant::now();
    if !jobs.is_empty() {
        let parent = load_parent_v1(&loaded.config, &chain.source)?;
        ensure(
            parent.state_sha256 == data.record.parent_state_sha256,
            "the parent differs from the collection's",
        )?;
        let snapshot = parent.state.snapshot_v1().map_err(err)?;
        let parent_state_sha256 = parent.state_sha256.clone();
        drop(parent);
        let inputs = FitInputsV1 {
            loaded,
            chain: chain.index,
            data: &data,
            snapshot: &snapshot,
            parent_state_sha256: &parent_state_sha256,
            forward_workers,
        };
        let contexts = vec![(); fit_workers.clamp(1, jobs.len())];
        ordered_parallel_jobs_v1(
            contexts,
            jobs.len(),
            |_, index| {
                let (seed, arm) = jobs[index];
                let bytes = fit_record_bytes_v1(&inputs, seed, arm)?;
                write_file_v1(&fit_path_v1(root, chain.index, seed, arm), &bytes, false)
            },
            || (),
        )?;
    }
    step0_reports_identical_v1(root, chain.index, seeds)?;
    Ok(json!({"chain": chain.index, "fits_written": jobs.len(),
        "fits_present": seeds.len() * ARM_NAMES_V1.len(),
        "elapsed_seconds": started.elapsed().as_secs_f64()}))
}

// ---------------------------------------------------------------------------
// Throughput qualification (COMPUTE-POLICY.md).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QualifiedStageV1 {
    Collect,
    Fit,
}

impl QualifiedStageV1 {
    fn name_v1(self) -> &'static str {
        match self {
            Self::Collect => "collect",
            Self::Fit => "fit",
        }
    }
}

fn qualification_path_v1(root: &Path, stage: QualifiedStageV1) -> PathBuf {
    root.join("qualification")
        .join(format!("{}-{}.json", stage.name_v1(), host_name_v1()))
}

/// Qualification games: chain `chain`'s schedule positions spread over the
/// pairs, with seeds and ids in the qualification domain.
pub(crate) fn qualification_plans_v1(config: &PrescreenConfigV1, chain: u32) -> Vec<GamePlanV1> {
    let schedule = chain_schedule_v1(config, chain);
    (0..config.qualification.collect_games)
        .map(|game| {
            let mut plan = schedule[(game * 37) % schedule.len()].clone();
            plan.id = format!("bwps-qualification-k{chain}-g{game}");
            plan.seed = domain_seed_v1(
                &config.qualification.seed_domain,
                &[chain.into(), game as u64],
            );
            plan
        })
        .collect()
}

/// Times the stage's real workload at each candidate worker count and
/// records the fastest (completed work per wall hour; ties go to fewer
/// workers).
pub(crate) fn qualify_stage_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
    stage: QualifiedStageV1,
    candidates: &[usize],
    forward_workers: usize,
) -> Result<Value, String> {
    ensure(
        !candidates.is_empty() && candidates.iter().all(|&w| (1..=1024).contains(&w)),
        "invalid candidate worker counts",
    )?;
    let config = &loaded.config;
    let chain = &config.chains[0];
    let parent = load_parent_v1(config, &chain.source)?;
    let mut rows = Vec::new();
    match stage {
        QualifiedStageV1::Collect => {
            let plans = qualification_plans_v1(config, chain.index);
            for &workers in candidates {
                let started = Instant::now();
                let games = collect_games_v1(config, &parent, &plans, workers)?;
                let seconds = started.elapsed().as_secs_f64();
                rows.push(json!({"workers": workers, "completed": games.len(),
                    "wall_seconds": seconds, "per_wall_hour": games.len() as f64 * 3600.0 / seconds}));
            }
        }
        QualifiedStageV1::Fit => {
            let data = load_chain_data_v1(loaded, root, chain)?;
            let snapshot = parent.state.snapshot_v1().map_err(err)?;
            let q = &config.qualification;
            let holdout: Vec<ScreenGameRecordV1> = data
                .holdout
                .iter()
                .take(q.fit_heldout_games)
                .cloned()
                .collect();
            let generation = config.generation_v1();
            for &workers in candidates {
                let started = Instant::now();
                let contexts = vec![(); workers];
                ordered_parallel_jobs_v1(
                    contexts,
                    workers,
                    |_, job| {
                        let seed =
                            domain_seed_v1(&q.seed_domain, &[chain.index.into(), job as u64]);
                        let widened = widen_net8_snapshot_v1(
                            &snapshot,
                            &config.arms.widen_target_architecture,
                            &config.arms.widen_init_law,
                            seed,
                        )
                        .map_err(err)?;
                        let mut state = fresh_adam_v1(&widened.state)?;
                        let fit_config = screen_fit_config_v1(
                            &config.fitting,
                            q.fit_steps,
                            q.fit_steps,
                            seed,
                            forward_workers,
                        );
                        fit_screen_value_dims_v1(
                            &mut state,
                            generation,
                            &data.fit,
                            &holdout,
                            &fit_config,
                        )
                        .map(|_| ())
                        .map_err(err)
                    },
                    || (),
                )?;
                let seconds = started.elapsed().as_secs_f64();
                let completed = workers as u64 * q.fit_steps;
                rows.push(json!({"workers": workers, "completed": completed,
                    "wall_seconds": seconds, "per_wall_hour": completed as f64 * 3600.0 / seconds}));
            }
        }
    }
    let mut selected = 0usize;
    for (index, row) in rows.iter().enumerate() {
        let rate = row["per_wall_hour"].as_f64().unwrap_or(0.0);
        let best = rows[selected]["per_wall_hour"].as_f64().unwrap_or(0.0);
        let fewer = row["workers"].as_u64() < rows[selected]["workers"].as_u64();
        if rate > best || (rate == best && fewer) {
            selected = index;
        }
    }
    let record = json!({"schema": QUALIFICATION_SCHEMA_V1, "stage": stage.name_v1(),
        "host": host_name_v1(), "config_sha256": loaded.sha256,
        "binary_sha256": binary_sha256_v1()?, "engine_commit": engine_commit_v1(),
        "forward_workers": forward_workers, "candidates": rows,
        "selected_workers": rows[selected]["workers"]});
    let path = qualification_path_v1(root, stage);
    write_file_v1(&path, &json_bytes_v1(&record)?, true)?;
    Ok(json!({"qualification": path, "record": record}))
}

/// The launch guard: a qualification record of this stage, host, binary and
/// config whose fastest worker count is the one requested.
pub(crate) fn require_qualification_v1(
    loaded: &LoadedConfigV1,
    path: &Path,
    stage: QualifiedStageV1,
    workers: usize,
) -> Result<Value, String> {
    let record: Value = serde_json::from_slice(&read_v1(path)?).map_err(err)?;
    ensure(
        record["schema"] == json!(QUALIFICATION_SCHEMA_V1)
            && record["stage"] == json!(stage.name_v1())
            && record["host"] == json!(host_name_v1())
            && record["config_sha256"] == json!(loaded.sha256)
            && record["binary_sha256"] == json!(binary_sha256_v1()?),
        "the throughput record is not this stage, host, binary and config",
    )?;
    ensure(
        record["selected_workers"] == json!(workers),
        "the requested worker count is not the qualified fastest",
    )?;
    Ok(json!({"path": path, "sha256": sha(&read_v1(path)?)}))
}

// ---------------------------------------------------------------------------
// Staging check.
// ---------------------------------------------------------------------------

fn nested_pins_v1(value: &Value, pins: &mut Vec<PinnedFileV1>) {
    match value {
        Value::Object(map) => {
            if map.len() == 2 && map.contains_key("path") && map.contains_key("sha256") {
                if let Ok(pin) = serde_json::from_value::<PinnedFileV1>(value.clone()) {
                    pins.push(pin);
                    return;
                }
            }
            for child in map.values() {
                nested_pins_v1(child, pins);
            }
        }
        Value::Array(items) => {
            for item in items {
                nested_pins_v1(item, pins);
            }
        }
        _ => {}
    }
}

#[derive(Deserialize)]
struct CheckpointDigestsV1 {
    feature_contract_digest: String,
    feature_encoding_digest: String,
}

fn check_pin_v1(pin: &PinnedFileV1) -> Result<Vec<u8>, String> {
    ensure(
        pin.path.is_absolute(),
        &format!("{} is not an absolute path", pin.path.display()),
    )?;
    read_pinned_bytes(pin).map_err(|e| format!("{}: {e}", pin.path.display()))
}

fn staging_chain_v1(config: &PrescreenConfigV1, chain: &PrescreenChainV1) -> Result<Value, String> {
    let import = check_pin_v1(&chain.source.play_import)?;
    let probe: Value = serde_json::from_slice(&import).map_err(err)?;
    ensure(
        probe["schema"] == json!(fresh_initialization_source::SOURCE_SCHEMA),
        "the import is not a fresh-initialization source",
    )?;
    let mut pins = Vec::new();
    nested_pins_v1(&probe, &mut pins);
    ensure(!pins.is_empty(), "the import pins no file")?;
    for pin in &pins {
        check_pin_v1(pin)?;
    }
    let checkpoint = chain
        .source
        .checkpoint
        .as_ref()
        .ok_or("the parent has no checkpoint")?;
    let digests: CheckpointDigestsV1 =
        serde_json::from_slice(&check_pin_v1(checkpoint)?).map_err(err)?;
    ensure(
        digests.feature_contract_digest == config.feature_contract_digest
            && digests.feature_encoding_digest == config.feature_encoding_digest,
        "the parent checkpoint's feature digests differ from the note's",
    )?;
    Ok(json!({"import_pins": pins, "checkpoint": checkpoint}))
}

/// `staging-check`: every chain's files exist at their absolute paths with
/// their pinned hashes. Writes `staging-check.json`; fails if any chain does.
pub(crate) fn staging_check_stage_v1(
    loaded: &LoadedConfigV1,
    root: &Path,
) -> Result<Value, String> {
    let mut chains = Vec::new();
    let mut pass = true;
    for chain in &loaded.config.chains {
        let row = match staging_chain_v1(&loaded.config, chain) {
            Ok(detail) => json!({"chain": chain.index, "pass": true, "detail": detail}),
            Err(error) => {
                pass = false;
                json!({"chain": chain.index, "pass": false, "error": error})
            }
        };
        chains.push(row);
    }
    let record = json!({"schema": STAGING_SCHEMA_V1, "config_sha256": loaded.sha256,
        "host": host_name_v1(), "chains": chains, "pass": pass});
    write_file_v1(
        &root.join("staging-check.json"),
        &json_bytes_v1(&record)?,
        true,
    )?;
    ensure(pass, "staging check failed; see staging-check.json")?;
    Ok(record)
}

// ---------------------------------------------------------------------------
// Command line.
// ---------------------------------------------------------------------------

const USAGE_V1: &str = "usage: b_width_prescreen_v1 STAGE --config ABS.json --root ABS_DIR \
[--chains 1,2,...] [--workers N] [--forward-workers N] [--qualification ABS.json] \
[--candidates 1,2,...]\nstages: staging-check qualify-collect qualify-fit collect convert preserve fit";

fn parse_list_v1<T: std::str::FromStr>(text: &str) -> Result<Vec<T>, String> {
    text.split(',')
        .map(|part| {
            part.trim()
                .parse::<T>()
                .map_err(|_| format!("invalid list item {part}"))
        })
        .collect()
}

/// The driver's command line (`src/bin/b_width_prescreen_v1.rs`).
pub fn run_b_width_prescreen_cli_v1(args: &[String]) -> Result<Value, String> {
    let stage = args.first().ok_or(USAGE_V1)?.as_str();
    let mut config = None;
    let mut root = None;
    let mut chains: Option<Vec<u32>> = None;
    let mut workers = None;
    let mut forward_workers = 1usize;
    let mut qualification = None;
    let mut candidates: Option<Vec<usize>> = None;
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or(USAGE_V1)?;
        match flag.as_str() {
            "--config" => config = Some(PathBuf::from(value)),
            "--root" => root = Some(PathBuf::from(value)),
            "--chains" => chains = Some(parse_list_v1(value)?),
            "--workers" => workers = Some(value.parse::<usize>().map_err(err)?),
            "--forward-workers" => forward_workers = value.parse::<usize>().map_err(err)?,
            "--qualification" => qualification = Some(PathBuf::from(value)),
            "--candidates" => candidates = Some(parse_list_v1(value)?),
            _ => return Err(USAGE_V1.into()),
        }
    }
    let config_path = config.ok_or(USAGE_V1)?;
    let root = root.ok_or(USAGE_V1)?;
    ensure(
        config_path.is_absolute() && root.is_absolute(),
        "config and root must be absolute",
    )?;
    let loaded = load_config_v1(&config_path)?;
    fs::create_dir_all(&root).map_err(err)?;
    let selected: Vec<&PrescreenChainV1> = match &chains {
        Some(indices) => indices
            .iter()
            .map(|&index| loaded.config.chain_v1(index))
            .collect::<Result<_, _>>()?,
        None => loaded.config.chains.iter().collect(),
    };
    let guard = |stage: QualifiedStageV1| -> Result<(usize, Value), String> {
        let workers = workers.ok_or("--workers is required")?;
        let path = qualification
            .as_ref()
            .ok_or("--qualification is required (COMPUTE-POLICY.md)")?;
        Ok((
            workers,
            require_qualification_v1(&loaded, path, stage, workers)?,
        ))
    };
    let mut results = Vec::new();
    let qualification_record = match stage {
        "staging-check" => {
            results.push(staging_check_stage_v1(&loaded, &root)?);
            Value::Null
        }
        "qualify-collect" | "qualify-fit" => {
            let stage = if stage == "qualify-collect" {
                QualifiedStageV1::Collect
            } else {
                QualifiedStageV1::Fit
            };
            let candidates = candidates.ok_or("--candidates is required")?;
            results.push(qualify_stage_v1(
                &loaded,
                &root,
                stage,
                &candidates,
                forward_workers,
            )?);
            Value::Null
        }
        "collect" => {
            let (workers, record) = guard(QualifiedStageV1::Collect)?;
            for chain in &selected {
                results.push(collect_chain_stage_v1(&loaded, &root, chain, workers)?);
            }
            record
        }
        "convert" => {
            for chain in &selected {
                results.push(convert_chain_stage_v1(&loaded, &root, chain)?);
            }
            if let Some(section7) = section7_stage_v1(&loaded, &root)? {
                results.push(section7);
            }
            Value::Null
        }
        "preserve" => {
            let (workers, record) = guard(QualifiedStageV1::Fit)?;
            for chain in &selected {
                results.push(preserve_chain_stage_v1(&loaded, &root, chain, workers)?);
            }
            record
        }
        "fit" => {
            let (workers, record) = guard(QualifiedStageV1::Fit)?;
            for chain in &selected {
                results.push(fit_chain_stage_v1(
                    &loaded,
                    &root,
                    chain,
                    workers,
                    forward_workers,
                )?);
            }
            record
        }
        _ => return Err(USAGE_V1.into()),
    };
    Ok(json!({"stage": stage, "config_sha256": loaded.sha256,
        "qualification": qualification_record, "results": results}))
}
