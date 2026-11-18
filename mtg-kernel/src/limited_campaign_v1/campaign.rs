//! The campaign loop (design Sections 3.1 to 3.4 and 5.2): collection,
//! the synchronous update, the stopping rules, transactional publication,
//! cold resume, the dev panel, the Gate 2 qualification workload and the
//! guarded launcher.
//!
//! Byte-compared files (`campaign.json`, `checkpoints/`, `iterations/`,
//! `evaluation.json`) hold no wall time, worker count, host class or
//! diagnostics flag. Those go to the sidecars `launches/`, `timing.json`,
//! `qualification_run.json` and `stage-diagnostics.jsonl`.
use super::checkpoint::{
    checkpoint_path_v1, iteration_dir_v1, CampaignLearnerV1, CheckpointPolicyV1, CheckpointV1,
    IterationCompleteV1, LearnerKindV1, OptimizerConfigV1, SourceIdentityV1, HAND_KNOWLEDGE_V1,
};
use super::collect::{
    pair_specs_v1, play_games_v1, GameLimitsV1, GameReceiptV1, GameRecordV1, PairPlanV1,
    SeatPolicyV1, TerminalV1, CAMPAIGN_LIMITS_V1,
};
use super::diagnostics::{measure_phase, DiagnosticsV1, RunStagesV1};
use super::host::{admit_receipt_host_class_v1, peak_rss_bytes_v1, HostClassV1};
use super::inventory::{
    load_train_decks_v1, read_inventory_v1, DeckV1, InventoryV1, DRAKE_HATCHER_TRAIN_DECKS_V1,
};
use super::schedule::{
    deal_v1, gate2_pairs_v1, is_league_pair_v1, league_opponent_step_v1, update_pairs_v1,
    Gate2PairsV1, DEV_INTERVAL_V1, DEV_PAIRS_V1, DEV_SEED_V1, LEAGUE_INTERVAL_V1,
    PAIRS_PER_UPDATE_V1, QUALIFICATION_DEV_PAIRS_V1, QUALIFICATION_SEED_V1,
    SELF_PLAY_PAIRS_PER_UPDATE_V1, TRAINING_SCHEDULE_SEED_V1,
};
use super::{is_sha256_hex, sha256_hex, RUNNER_CONTRACT_V1, SAMPLER_EXP_V1, SAMPLER_V1};
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::current_tree_learner_v1::{AdamConfigV1, LearnerV1, ModelV1, TerminalExampleV1};
use crate::current_tree_learner_v2::{
    pair_centered_terminal_rewards_v1, DecisionV2, LearnerV2, LearnerV2H16, OptimizerConfigV2,
    TerminalV2, TrajectoryV2,
};
use crate::limited_full_pool_consumer_v1 as full_pool;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

type Result<T> = std::result::Result<T, String>;

pub const CAMPAIGN_SCHEMA_V1: &str = "limited-campaign/v1";
pub const RECEIPT_SCHEMA_V1: &str = "limited-campaign-iteration-receipt/v1";
pub const DEV_SCHEMA_V1: &str = "limited-campaign-dev-panel/v1";
pub const FAILED_SCHEMA_V1: &str = "limited-campaign-failed-iteration/v1";
pub const STOPPED_SCHEMA_V1: &str = "limited-campaign-stopped-iteration/v1";
pub const COMPLETE_SCHEMA_V1: &str = "limited-campaign-complete/v1";
pub const LAUNCH_SCHEMA_V1: &str = "limited-campaign-launch/v1";
pub const EVALUATION_SCHEMA_V1: &str = "limited-campaign-qualification-evaluation/v1";
pub const QUALIFICATION_RUN_SCHEMA_V1: &str = "limited-campaign-qualification-run/v1";
pub const MANIFEST_SCHEMA_V1: &str = "limited-campaign-qualification-manifest/v1";
/// Admissible campaign horizons once qualified (design Section 3.3).
pub const CAMPAIGN_HORIZON_V1: std::ops::RangeInclusive<u64> = 100..=200;
/// A smoke run trains at most two updates on one self-play pair each.
pub const SMOKE_HORIZON_V1: std::ops::RangeInclusive<u64> = 1..=2;
/// Truncation gate: more than 2 truncated games in a batch on two
/// consecutive updates stops the campaign.
pub const TRUNCATION_GATE_GAMES_V1: u64 = 2;
pub const DEV_COLLAPSE_AFTER_UPDATE_V1: u64 = 40;
pub const DEV_COLLAPSE_THRESHOLD_V1: f64 = 0.40;
/// The engineering fault injection of the kill-and-resume check:
/// `after-checkpoint:U` aborts the process right after `step_U.json` is
/// published and before update `U` commits.
pub const FAULT_ENV_V1: &str = "MTG_LIMITED_CAMPAIGN_FAULT_V1";

/// The diagnostic control's Adam configuration on the v1 learner (which has
/// no schedule and no clip).
pub fn control_adam_v1() -> AdamConfigV1 {
    AdamConfigV1 {
        learning_rate: 3e-4,
        beta1: 0.9,
        beta2: 0.999,
        epsilon: 1e-8,
    }
}

/// What a run trains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeV1 {
    /// Sixteen pairs per update at seed 657_000.
    Campaign,
    /// One self-play pair per update (pair `16 u` of the campaign schedule).
    Smoke,
    /// The Gate 2 workload: one update on the ruled eight pairs at 657_001.
    Qualification,
}

impl ModeV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::Campaign => "campaign",
            Self::Smoke => "smoke",
            Self::Qualification => "qualification",
        }
    }
    fn parse(name: &str) -> Result<Self> {
        [Self::Campaign, Self::Smoke, Self::Qualification]
            .into_iter()
            .find(|mode| mode.name() == name)
            .ok_or_else(|| format!("unknown campaign mode {name:?}"))
    }
    fn schedule_seed(self) -> u64 {
        match self {
            Self::Qualification => QUALIFICATION_SEED_V1,
            _ => TRAINING_SCHEDULE_SEED_V1,
        }
    }
}

/// How the binary was launched. None of this enters a byte-compared file.
pub struct LaunchV1 {
    pub root: PathBuf,
    pub repo_root: PathBuf,
    pub workers: usize,
    pub stage_diagnostics: bool,
    pub git_head: String,
    pub binary_sha256: String,
    pub host_class: HostClassV1,
    pub qualification_manifest: Option<PathBuf>,
}

/// How a run ended without a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcomeV1 {
    Horizon,
    DevCollapse,
    ExcessTruncation,
}

impl RunOutcomeV1 {
    pub fn stop_reason(self) -> &'static str {
        match self {
            Self::Horizon => "horizon",
            Self::DevCollapse => "dev_collapse",
            Self::ExcessTruncation => "stopped_excess_truncation",
        }
    }
}

/// SHA-256 over the runner's source files (the binary and this module's
/// non-test files), each entered as its path, its length (`u64`,
/// little-endian) and its bytes.
pub fn runner_source_sha256_v1() -> String {
    let sources: [(&str, &[u8]); 9] = [
        (
            "mtg-kernel/src/bin/kernel_limited_campaign_v1.rs",
            include_bytes!("../bin/kernel_limited_campaign_v1.rs"),
        ),
        (
            "mtg-kernel/src/limited_campaign_v1.rs",
            include_bytes!("../limited_campaign_v1.rs"),
        ),
        (
            "mtg-kernel/src/limited_campaign_v1/campaign.rs",
            include_bytes!("campaign.rs"),
        ),
        (
            "mtg-kernel/src/limited_campaign_v1/checkpoint.rs",
            include_bytes!("checkpoint.rs"),
        ),
        (
            "mtg-kernel/src/limited_campaign_v1/collect.rs",
            include_bytes!("collect.rs"),
        ),
        (
            "mtg-kernel/src/limited_campaign_v1/diagnostics.rs",
            include_bytes!("diagnostics.rs"),
        ),
        (
            "mtg-kernel/src/limited_campaign_v1/host.rs",
            include_bytes!("host.rs"),
        ),
        (
            "mtg-kernel/src/limited_campaign_v1/inventory.rs",
            include_bytes!("inventory.rs"),
        ),
        (
            "mtg-kernel/src/limited_campaign_v1/schedule.rs",
            include_bytes!("schedule.rs"),
        ),
    ];
    let mut hash = Sha256::new();
    for (path, bytes) in sources {
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}

/// The source identity of this build and inventory.
pub fn source_identity_v1(
    git_head: &str,
    binary_sha256: &str,
    inventory: &InventoryV1,
) -> SourceIdentityV1 {
    SourceIdentityV1 {
        git_head: git_head.into(),
        binary_sha256: binary_sha256.into(),
        binary_source_sha256: runner_source_sha256_v1(),
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        sample_sha256_in_pool_order: inventory.sample_sha256_in_pool_order.clone(),
        actor_profile: full_pool::PROFILE_V1.into(),
        actor_producer_sha256: full_pool::producer_sha256_v1(),
        hand_knowledge: HAND_KNOWLEDGE_V1.into(),
        reveal_producer_sha256: sha256_hex(include_bytes!("../reveal_meaning_v3.rs")),
        reveal_reducer_sha256: sha256_hex(include_bytes!("../reveal_meaning_v3/reducer.rs")),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainDeckEntryV1 {
    pub deck: String,
    pub cards: usize,
    pub source_sha256: String,
}

/// `campaign.json`: everything a campaign's bytes depend on. Its SHA-256 is
/// every checkpoint's `campaign_sha256`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignConfigV1 {
    pub schema: String,
    pub mode: String,
    pub identity: SourceIdentityV1,
    pub runner_contract: Vec<String>,
    pub learner_contract: String,
    pub objective: String,
    pub feature_sha256: String,
    pub optimizer_config: OptimizerConfigV1,
    pub sampler: String,
    pub sampler_exp: String,
    pub limits: GameLimitsV1,
    pub schedule_seed: u64,
    pub pairs_per_update: u64,
    pub self_play_pairs_per_update: u64,
    pub league_interval: u64,
    pub dev_seed: u64,
    pub dev_pairs: u64,
    pub dev_interval: u64,
    pub dev_collapse_after_update: u64,
    pub dev_collapse_threshold: f64,
    pub truncation_gate_games: u64,
    pub u_max: u64,
    pub train_decks: Vec<TrainDeckEntryV1>,
    pub card_file_sha256: [String; 2],
    pub qualification_manifest_sha256: Option<String>,
    pub qualification_pairs: Option<Gate2PairsV1>,
}

fn optimizer_config_v1(learner: LearnerKindV1, u_max: u64) -> OptimizerConfigV1 {
    match learner {
        LearnerKindV1::ActorCriticH32 | LearnerKindV1::ActorCriticH16 => {
            OptimizerConfigV1::ActorCritic(OptimizerConfigV2::campaign_v1(u_max))
        }
        LearnerKindV1::PairCenteredControl => {
            OptimizerConfigV1::PairCenteredControl(control_adam_v1())
        }
    }
}

struct ConfigInputs<'a> {
    mode: ModeV1,
    learner: LearnerKindV1,
    u_max: u64,
    identity: SourceIdentityV1,
    inventory: &'a InventoryV1,
    decks: &'a [DeckV1],
    qualification_manifest_sha256: Option<String>,
    qualification_pairs: Option<Gate2PairsV1>,
}

fn campaign_config_v1(inputs: ConfigInputs<'_>) -> CampaignConfigV1 {
    let (pairs_per_update, self_play) = match inputs.mode {
        ModeV1::Campaign => (PAIRS_PER_UPDATE_V1, SELF_PLAY_PAIRS_PER_UPDATE_V1),
        ModeV1::Smoke => (1, 1),
        ModeV1::Qualification => {
            let count = inputs
                .qualification_pairs
                .as_ref()
                .map_or(0, |pairs| pairs.indices.len() as u64);
            (count, count)
        }
    };
    CampaignConfigV1 {
        schema: CAMPAIGN_SCHEMA_V1.into(),
        mode: inputs.mode.name().into(),
        identity: inputs.identity,
        runner_contract: RUNNER_CONTRACT_V1.iter().map(|line| line.to_string()).collect(),
        learner_contract: inputs.learner.contract().into(),
        objective: inputs.learner.objective().into(),
        feature_sha256: inputs.learner.feature_sha256(),
        optimizer_config: optimizer_config_v1(inputs.learner, inputs.u_max),
        sampler: SAMPLER_V1.into(),
        sampler_exp: SAMPLER_EXP_V1.into(),
        limits: CAMPAIGN_LIMITS_V1,
        schedule_seed: inputs.mode.schedule_seed(),
        pairs_per_update,
        self_play_pairs_per_update: self_play,
        league_interval: LEAGUE_INTERVAL_V1,
        dev_seed: DEV_SEED_V1,
        dev_pairs: match inputs.mode {
            ModeV1::Qualification => QUALIFICATION_DEV_PAIRS_V1,
            _ => DEV_PAIRS_V1,
        },
        dev_interval: DEV_INTERVAL_V1,
        dev_collapse_after_update: DEV_COLLAPSE_AFTER_UPDATE_V1,
        dev_collapse_threshold: DEV_COLLAPSE_THRESHOLD_V1,
        truncation_gate_games: TRUNCATION_GATE_GAMES_V1,
        u_max: inputs.u_max,
        train_decks: inputs
            .decks
            .iter()
            .map(|deck| TrainDeckEntryV1 {
                deck: deck.name.clone(),
                cards: deck.cards.len(),
                source_sha256: deck.source_sha256.clone(),
            })
            .collect(),
        card_file_sha256: inputs.inventory.card_file_sha256.clone(),
        qualification_manifest_sha256: inputs.qualification_manifest_sha256,
        qualification_pairs: inputs.qualification_pairs,
    }
}

fn io_error(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |error| format!("{}: {error}", path.display())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(path).map_err(io_error(path))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(io_error(path))
}

fn sync_directory(path: &Path) {
    // Directories cannot be opened for syncing on Windows; elsewhere this
    // makes a rename durable.
    if cfg!(unix) {
        if let Ok(directory) = File::open(path) {
            let _ = directory.sync_all();
        }
    }
}

/// `.NAME.staging` + `sync_all` + rename; returns the SHA-256 of `bytes`.
pub fn publish_atomic_v1(directory: &Path, name: &str, bytes: &[u8]) -> Result<String> {
    let staging = directory.join(format!(".{name}.staging"));
    if staging.exists() {
        fs::remove_file(&staging).map_err(io_error(&staging))?;
    }
    write_synced(&staging, bytes)?;
    let target = directory.join(name);
    fs::rename(&staging, &target).map_err(io_error(&target))?;
    sync_directory(directory);
    Ok(sha256_hex(bytes))
}

fn to_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    // `serde_json::Value` objects keep their keys sorted, so a `Value`
    // encodes canonically.
    serde_json::to_vec(value).map_err(|error| error.to_string())
}

fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(io_error(path))
}

fn initial_learner_v1(learner: LearnerKindV1, u_max: u64) -> Result<CampaignLearnerV1> {
    let refuse = |error: crate::current_mixed_policy_v1::Error| error.to_string();
    Ok(match learner {
        LearnerKindV1::ActorCriticH32 => CampaignLearnerV1::ActorCriticH32(
            LearnerV2::new(OptimizerConfigV2::campaign_v1(u_max)).map_err(refuse)?,
        ),
        LearnerKindV1::ActorCriticH16 => CampaignLearnerV1::ActorCriticH16(
            LearnerV2H16::new(OptimizerConfigV2::campaign_v1(u_max)).map_err(refuse)?,
        ),
        LearnerKindV1::PairCenteredControl => CampaignLearnerV1::PairCenteredControl(
            LearnerV1::new(ModelV1::deterministic_fixture(), control_adam_v1()).map_err(refuse)?,
        ),
    })
}

fn learner_seats(game: &GameRecordV1) -> Vec<u8> {
    (0u8..2)
        .filter(|&seat| game.receipt.learner_seats[usize::from(seat)])
        .collect()
}

/// Every learner seat's trajectory of the batch, in collection order.
fn trajectories_of(games: &[GameRecordV1]) -> Result<Vec<TrajectoryV2<'_>>> {
    let mut trajectories = Vec::new();
    for game in games {
        for seat in learner_seats(game) {
            let terminal = match &game.terminal {
                TerminalV1::Natural(returns) => TerminalV2::Natural {
                    learner_return: returns[usize::from(seat)],
                },
                TerminalV1::Truncated(_) => TerminalV2::Truncated,
                TerminalV1::Failed(reason) => return Err(format!("failed game: {reason}")),
            };
            trajectories.push(TrajectoryV2 {
                pair_index: game.receipt.pair,
                leg: game.receipt.leg,
                seat,
                terminal,
                decisions: game
                    .retained
                    .iter()
                    .filter(|decision| decision.seat == seat)
                    .map(|decision| DecisionV2 {
                        input: &decision.input,
                        selected: decision.selected,
                    })
                    .collect(),
            });
        }
    }
    Ok(trajectories)
}

/// The update of design Sections 3.2 and 3.3 over one collected batch. The
/// actor-critic arms hand every learner seat's trajectory to
/// `TreeLearnerV2::update`, truncated games as `TerminalV2::Truncated`
/// (no policy or value loss, not counted in `N`). The control arm gives each
/// retained decision of a natural game the pair-centered advantage against
/// the same seat (the same deck) in the pair's other leg, through
/// `pair_centered_terminal_rewards_v1`; truncated games add nothing.
pub fn update_learner_v1(learner: &mut CampaignLearnerV1, games: &[GameRecordV1]) -> Result<Value> {
    let refuse = |error: crate::current_mixed_policy_v1::Error| format!("update: {error}");
    match learner {
        CampaignLearnerV1::ActorCriticH32(learner) => {
            let report = learner.update(&trajectories_of(games)?).map_err(refuse)?;
            serde_json::to_value(report).map_err(|error| error.to_string())
        }
        CampaignLearnerV1::ActorCriticH16(learner) => {
            let report = learner.update(&trajectories_of(games)?).map_err(refuse)?;
            serde_json::to_value(report).map_err(|error| error.to_string())
        }
        CampaignLearnerV1::PairCenteredControl(learner) => {
            let natural = |pair: u64, leg: u8, seat: u8| {
                games
                    .iter()
                    .find(|game| game.receipt.pair == pair && game.receipt.leg == leg)
                    .and_then(|game| match game.terminal {
                        TerminalV1::Natural(returns) => Some(returns[usize::from(seat)]),
                        _ => None,
                    })
            };
            let mut examples = Vec::new();
            let mut counts = BTreeMap::<i8, u64>::new();
            for game in games {
                for seat in learner_seats(game) {
                    let own = match &game.terminal {
                        TerminalV1::Natural(returns) => returns[usize::from(seat)],
                        TerminalV1::Truncated(_) => continue,
                        TerminalV1::Failed(reason) => {
                            return Err(format!("failed game: {reason}"))
                        }
                    };
                    let other = natural(game.receipt.pair, 1 - game.receipt.leg, seat);
                    let rewards = pair_centered_terminal_rewards_v1(own, other).map_err(refuse)?;
                    for decision in game.retained.iter().filter(|d| d.seat == seat) {
                        for reward in rewards {
                            *counts.entry(reward).or_default() += 1;
                            examples.push(TerminalExampleV1 {
                                input: &decision.input,
                                selected: decision.selected,
                                reward,
                            });
                        }
                    }
                }
            }
            let loss = learner.update_terminal(&examples).map_err(refuse)?;
            Ok(json!({
                "loss": loss,
                "doubled_examples": examples.len(),
                "reward_counts": counts
                    .iter()
                    .map(|(reward, count)| (reward.to_string(), *count))
                    .collect::<BTreeMap<_, _>>(),
            }))
        }
    }
}

/// The dev score of one panel (design Section 4.3): candidate points `1`
/// for a win, `0.5` for a draw or a truncated game, `0` for a loss, as half
/// points over all games. The candidate plays seat `leg` (asymmetric
/// layout).
pub fn dev_half_points_v1(games: &[GameRecordV1]) -> Result<u64> {
    let mut half_points = 0;
    for game in games {
        let seat = usize::from(game.receipt.leg);
        half_points += match &game.terminal {
            TerminalV1::Natural(returns) => match returns[seat] {
                1 => 2,
                0 => 1,
                _ => 0,
            },
            TerminalV1::Truncated(_) => 1,
            TerminalV1::Failed(reason) => return Err(format!("dev game failed: {reason}")),
        };
    }
    Ok(half_points)
}

fn fault_point_v1() -> Option<u64> {
    let value = std::env::var(FAULT_ENV_V1).ok()?;
    value.strip_prefix("after-checkpoint:")?.parse().ok()
}

struct RunnerV1 {
    root: PathBuf,
    config: CampaignConfigV1,
    campaign_sha256: String,
    decks: Vec<DeckV1>,
    learner: CampaignLearnerV1,
    head: u64,
    head_sha256: String,
    snapshots: BTreeMap<u64, CheckpointPolicyV1>,
    previous_truncated: Option<u64>,
    dev_scores: BTreeMap<u64, f64>,
    workers: usize,
    diagnostics: Option<DiagnosticsV1>,
    stages: RunStagesV1,
    fault: Option<u64>,
    natural_games: u64,
    decisions: u64,
}

fn mode_of(config: &CampaignConfigV1) -> Result<ModeV1> {
    ModeV1::parse(&config.mode)
}

impl RunnerV1 {
    fn diagnostics(&self) -> Option<&DiagnosticsV1> {
        self.diagnostics.as_ref()
    }

    fn fail(&self, update: u64, error: String, games: &[GameReceiptV1]) -> String {
        let record = json!({
            "schema": FAILED_SCHEMA_V1,
            "campaign_sha256": self.campaign_sha256,
            "update": update,
            "error": error,
            "games": games,
        });
        let name = format!("failed-iteration-{update:04}.json");
        match to_bytes(&record).and_then(|bytes| publish_atomic_v1(&self.root, &name, &bytes)) {
            Ok(_) => format!("update {update} failed: {error}. See {name}"),
            Err(write) => format!("update {update} failed: {error}; {name} not written: {write}"),
        }
    }

    /// Writes the receipt into the staging directory, publishes the
    /// checkpoint and reads it back. Returns the staging directory, the
    /// receipt and checkpoint digests and the reloaded policy.
    fn stage_iteration(
        &mut self,
        update: u64,
        receipt: &Value,
    ) -> Result<(PathBuf, String, String, CheckpointPolicyV1)> {
        let iterations = self.root.join("iterations");
        let staging = iterations.join(format!(".iter_{update:04}.staging"));
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(io_error(&staging))?;
        }
        fs::create_dir(&staging).map_err(io_error(&staging))?;
        let receipt_bytes = to_bytes(receipt)?;
        write_synced(&staging.join("receipt.json"), &receipt_bytes)?;
        let receipt_sha256 = sha256_hex(&receipt_bytes);
        let parent = (update > 0).then(|| self.head_sha256.clone());
        let snapshot = self.learner.snapshot();
        let checkpoint = CheckpointV1::new(
            &self.campaign_sha256,
            &self.config.identity,
            parent,
            &receipt_sha256,
            snapshot.clone(),
        )?;
        let bytes = checkpoint.to_bytes()?;
        let checkpoints = self.root.join("checkpoints");
        let name = super::checkpoint::checkpoint_file_name_v1(update);
        let sha256 = publish_atomic_v1(&checkpoints, &name, &bytes)?;
        if self.fault == Some(update) {
            eprintln!("fault injection: aborting after publishing {name}");
            std::process::abort();
        }
        // Readback: the published file, parsed and restored, must give the
        // learner's exact state.
        let (reloaded, reloaded_sha256) = CheckpointV1::load_v1(&checkpoints.join(&name))?;
        let restored = reloaded.restore_learner_v1()?;
        if reloaded_sha256 != sha256 || reloaded != checkpoint || restored.snapshot() != snapshot {
            return Err(format!("{name} did not read back to the learner's state"));
        }
        Ok((staging, receipt_sha256, sha256, restored.policy()))
    }

    /// Writes the optional dev file and the complete record and renames the
    /// staging directory into place, which commits the update.
    fn finish_iteration(
        &mut self,
        update: u64,
        staged: (PathBuf, String, String),
        dev: Option<Vec<u8>>,
    ) -> Result<()> {
        let (staging, receipt_sha256, checkpoint_sha256) = staged;
        let dev_sha256 = match dev {
            Some(bytes) => {
                write_synced(&staging.join("dev.json"), &bytes)?;
                Some(sha256_hex(&bytes))
            }
            None => None,
        };
        let complete =
            IterationCompleteV1::new(update, &checkpoint_sha256, &receipt_sha256, dev_sha256);
        write_synced(&staging.join("complete.json"), &complete.to_bytes()?)?;
        sync_directory(&staging);
        let target = iteration_dir_v1(&self.root, update);
        fs::rename(&staging, &target).map_err(io_error(&target))?;
        sync_directory(&self.root.join("iterations"));
        self.head = update;
        self.head_sha256 = checkpoint_sha256;
        Ok(())
    }

    fn commit_initialization(&mut self) -> Result<()> {
        let receipt = json!({
            "schema": RECEIPT_SCHEMA_V1,
            "campaign_sha256": self.campaign_sha256,
            "update": 0,
            "kind": "initialization",
            "learner_contract": self.config.learner_contract,
        });
        let mut io = self.stages.io;
        let diagnostics = self.diagnostics.take();
        let result = measure_phase(diagnostics.as_ref(), "io", Some(&mut io), || {
            let (staging, receipt_sha256, sha256, policy) = self.stage_iteration(0, &receipt)?;
            self.finish_iteration(0, (staging, receipt_sha256, sha256), None)?;
            self.snapshots.insert(0, policy);
            Ok(())
        });
        self.diagnostics = diagnostics;
        self.stages.io = io;
        result
    }

    /// The pairs of the batch collected at `step`, each with its league
    /// opponent's step when it is a league pair.
    fn batch_pairs(&self, step: u64) -> Result<Vec<(u64, Option<u64>)>> {
        let seed = self.config.schedule_seed;
        Ok(match mode_of(&self.config)? {
            ModeV1::Campaign => update_pairs_v1(step)
                .map(|pair| {
                    let league = is_league_pair_v1(pair)
                        .then(|| league_opponent_step_v1(seed, step, pair));
                    (pair, league)
                })
                .collect(),
            ModeV1::Smoke => vec![(step * PAIRS_PER_UPDATE_V1, None)],
            ModeV1::Qualification => self
                .config
                .qualification_pairs
                .as_ref()
                .ok_or("qualification campaign lacks its pairs")?
                .indices
                .iter()
                .map(|&pair| (pair, None))
                .collect(),
        })
    }

    fn play(&mut self, specs: &[super::collect::GameSpecV1<'_>]) -> Vec<GameRecordV1> {
        let mut counter = self.stages.collection;
        let games = measure_phase(
            self.diagnostics.as_ref(),
            "collection",
            Some(&mut counter),
            || play_games_v1(specs, self.workers, self.diagnostics.as_ref()),
        );
        self.stages.collection = counter;
        for game in &games {
            if let Some(stages) = &game.stages {
                self.stages.games.merge(stages);
            }
            if matches!(game.terminal, TerminalV1::Natural(_)) {
                self.natural_games += 1;
            }
            self.decisions += game.receipt.decisions;
        }
        games
    }

    /// Plays a panel of `pairs` pairs at the dev seed: `candidate` against
    /// `step_0` on `train` decks, asymmetric layout. Returns the receipts and
    /// half points.
    fn panel(
        &mut self,
        candidate: &CheckpointPolicyV1,
        label: String,
        pairs: u64,
    ) -> Result<(Vec<GameReceiptV1>, u64)> {
        let opponent = self
            .snapshots
            .get(&0)
            .ok_or("step_0000 is not loaded")?
            .clone();
        let a = SeatPolicyV1 {
            policy: candidate,
            label,
        };
        let b = SeatPolicyV1 {
            policy: &opponent,
            label: "step_0000".into(),
        };
        let mut specs = Vec::new();
        let decks = self.decks.clone();
        for pair in 0..pairs {
            let plan = PairPlanV1 {
                deal: deal_v1(self.config.dev_seed, pair, decks.len())?,
                symmetric: false,
                stream_seed: self.config.dev_seed,
                train: false,
                limits: self.config.limits,
            };
            specs.extend(pair_specs_v1(plan, &decks, &a, &b));
        }
        let mut counter = self.stages.dev_panel;
        let started = Instant::now();
        let games = self.play(&specs);
        counter.add(super::diagnostics::elapsed_ns(started));
        self.stages.dev_panel = counter;
        let half_points = dev_half_points_v1(&games)?;
        Ok((games.into_iter().map(|game| game.receipt).collect(), half_points))
    }

    /// One update: collect with the policy at `update - 1`, apply the
    /// truncation gate, update, publish and commit, then the dev panel at a
    /// milestone. Returns a stop when a stopping rule fires.
    fn run_update(&mut self, update: u64) -> Result<Option<RunOutcomeV1>> {
        let step = update - 1;
        let seed = self.config.schedule_seed;
        let policy = self.learner.policy();
        let a = SeatPolicyV1 {
            policy: &policy,
            label: format!("learner:step_{step:04}"),
        };
        let decks = self.decks.clone();
        let snapshots = self.snapshots.clone();
        let mut specs = Vec::new();
        let mut pairs = Vec::new();
        for (pair, league) in self.batch_pairs(step)? {
            let deal = deal_v1(seed, pair, decks.len())?;
            let b = match league {
                Some(opponent) => SeatPolicyV1 {
                    policy: snapshots
                        .get(&opponent)
                        .ok_or_else(|| format!("league snapshot step {opponent} is not loaded"))?,
                    label: format!("league:step_{opponent:04}"),
                },
                None => SeatPolicyV1 {
                    policy: &policy,
                    label: a.label.clone(),
                },
            };
            pairs.push(json!({
                "pair": pair,
                "kind": if league.is_some() { "league" } else { "self_play" },
                "opponent_step": league,
                "decks": deal.decks.map(|index| decks[index].name.clone()),
                "env_seed": deal.env_seed,
                "starting_player": deal.starting.index(),
            }));
            let plan = PairPlanV1 {
                deal,
                symmetric: league.is_none(),
                stream_seed: seed,
                train: true,
                limits: self.config.limits,
            };
            specs.extend(pair_specs_v1(plan, &decks, &a, &b));
        }
        let games = self.play(&specs);
        let receipts = games
            .iter()
            .map(|game| game.receipt.clone())
            .collect::<Vec<_>>();
        if let Some(failed) = games.iter().find_map(|game| match &game.terminal {
            TerminalV1::Failed(reason) => Some(format!(
                "pair {} leg {}: {reason}",
                game.receipt.pair, game.receipt.leg
            )),
            _ => None,
        }) {
            return Err(self.fail(update, failed, &receipts));
        }
        let truncated = games
            .iter()
            .filter(|game| matches!(game.terminal, TerminalV1::Truncated(_)))
            .count() as u64;
        let natural = games.len() as u64 - truncated;
        let gate = self.config.truncation_gate_games;
        let previous = self.previous_truncated;
        if truncated > gate && previous.is_some_and(|previous| previous > gate) {
            let record = json!({
                "schema": STOPPED_SCHEMA_V1,
                "campaign_sha256": self.campaign_sha256,
                "update": update,
                "stop_reason": RunOutcomeV1::ExcessTruncation.stop_reason(),
                "truncated_games": truncated,
                "previous_truncated_games": previous,
                "games": receipts,
            });
            publish_atomic_v1(
                &self.root,
                &format!("stopped-iteration-{update:04}.json"),
                &to_bytes(&record)?,
            )?;
            return Ok(Some(RunOutcomeV1::ExcessTruncation));
        }
        let mut update_counter = self.stages.update;
        let report = measure_phase(
            self.diagnostics.as_ref(),
            "update",
            Some(&mut update_counter),
            || update_learner_v1(&mut self.learner, &games),
        );
        self.stages.update = update_counter;
        let report = match report {
            Ok(report) => report,
            Err(error) => return Err(self.fail(update, error, &receipts)),
        };
        let learner_trajectories = games
            .iter()
            .map(|game| game.receipt.learner_seats.iter().filter(|s| **s).count() as u64)
            .sum::<u64>();
        let retained = games
            .iter()
            .map(|game| game.retained.len() as u64)
            .sum::<u64>();
        drop(games);
        let receipt = json!({
            "schema": RECEIPT_SCHEMA_V1,
            "campaign_sha256": self.campaign_sha256,
            "update": update,
            "kind": "update",
            "policy_step": step,
            "parent_checkpoint_sha256": self.head_sha256,
            "pairs": pairs,
            "games": receipts,
            "natural_games": natural,
            "truncated_games": truncated,
            "learner_trajectories": learner_trajectories,
            "retained_decisions": retained,
            "truncation_gate": {
                "limit": gate,
                "previous_truncated_games": previous,
                "exceeded": truncated > gate,
            },
            "update_report": report,
        });
        let mut io = self.stages.io;
        let started = Instant::now();
        let (staging, receipt_sha256, checkpoint_sha256, reloaded) =
            self.stage_iteration(update, &receipt)?;
        io.add(super::diagnostics::elapsed_ns(started));
        let milestone = mode_of(&self.config)? != ModeV1::Qualification
            && update % self.config.dev_interval == 0;
        let mut dev_score = None;
        let dev = if milestone {
            let label = format!("candidate:step_{update:04}");
            let (dev_games, half_points) = match self.panel(&reloaded, label, self.config.dev_pairs)
            {
                Ok(panel) => panel,
                Err(error) => return Err(self.fail(update, error, &[])),
            };
            let games_count = dev_games.len() as u64;
            let score = half_points as f64 / (2 * games_count) as f64;
            dev_score = Some(score);
            Some(to_bytes(&json!({
                "schema": DEV_SCHEMA_V1,
                "campaign_sha256": self.campaign_sha256,
                "update": update,
                "candidate_checkpoint_sha256": checkpoint_sha256,
                "opponent": "step_0000",
                "seed": self.config.dev_seed,
                "pairs": self.config.dev_pairs,
                "games": dev_games,
                "half_points": half_points,
                "score": score,
            }))?)
        } else {
            None
        };
        let started = Instant::now();
        self.finish_iteration(update, (staging, receipt_sha256, checkpoint_sha256), dev)?;
        io.add(super::diagnostics::elapsed_ns(started));
        self.stages.io = io;
        if update % LEAGUE_INTERVAL_V1 == 0 {
            self.snapshots.insert(update, reloaded);
        }
        self.previous_truncated = Some(truncated);
        if let Some(score) = dev_score {
            self.dev_scores.insert(update, score);
            let threshold = self.config.dev_collapse_threshold;
            let earlier = update
                .checked_sub(self.config.dev_interval)
                .and_then(|earlier| self.dev_scores.get(&earlier));
            if update >= self.config.dev_collapse_after_update
                && score < threshold
                && earlier.is_some_and(|earlier| *earlier < threshold)
            {
                return Ok(Some(RunOutcomeV1::DevCollapse));
            }
        }
        Ok(None)
    }

    fn finish_campaign(&self, outcome: RunOutcomeV1) -> Result<()> {
        let record = json!({
            "schema": COMPLETE_SCHEMA_V1,
            "campaign_sha256": self.campaign_sha256,
            "final_update": self.head,
            "final_checkpoint_sha256": self.head_sha256,
            "stop_reason": outcome.stop_reason(),
        });
        publish_atomic_v1(&self.root, "complete.json", &to_bytes(&record)?)?;
        Ok(())
    }

    fn run_to_horizon(&mut self) -> Result<RunOutcomeV1> {
        while self.head < self.config.u_max {
            if let Some(outcome) = self.run_update(self.head + 1)? {
                self.finish_campaign(outcome)?;
                return Ok(outcome);
            }
        }
        self.finish_campaign(RunOutcomeV1::Horizon)?;
        Ok(RunOutcomeV1::Horizon)
    }
}

fn record_launch(launch: &LaunchV1, command: &str, head: Option<u64>) -> Result<()> {
    let launches = launch.root.join("launches");
    fs::create_dir_all(&launches).map_err(io_error(&launches))?;
    let mut index = 0;
    while launches.join(format!("launch_{index:04}.json")).exists() {
        index += 1;
    }
    let manifest_sha256 = match &launch.qualification_manifest {
        Some(path) => Some(sha256_hex(&read_bytes(path)?)),
        None => None,
    };
    let record = json!({
        "schema": LAUNCH_SCHEMA_V1,
        "command": command,
        "committed_head": head,
        "workers": launch.workers,
        "stage_diagnostics": launch.stage_diagnostics,
        "host_class": launch.host_class,
        "qualification_manifest_sha256": manifest_sha256,
    });
    publish_atomic_v1(
        &launches,
        &format!("launch_{index:04}.json"),
        &to_bytes(&record)?,
    )?;
    Ok(())
}

/// The qualification manifest (design Section 5.2 item 4 and 5): the
/// identity and host class the runs were made with, their byte-identical
/// outputs, the timing of each run and the locked `W*` and `U_max`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationManifestV1 {
    pub schema: String,
    pub binary_source_sha256: String,
    pub feature_sha256: String,
    pub card_db_hash: String,
    pub learner_contract: String,
    pub host_class: HostClassV1,
    pub qualification_pairs: Gate2PairsV1,
    pub step_0001_sha256: String,
    pub evaluation_sha256: String,
    pub runs: Vec<Value>,
    pub workers: usize,
    pub u_max: u64,
}

/// The guarded launcher (design Section 5.2 item 5): a run of more than
/// one update needs a qualification manifest whose binary source, feature
/// identity, card database, learner contract and host class (with the
/// trainer binary's digest) match this run, whose `U_max` and `W*` are the
/// run's, with `W* >= 2` and a receipt host class.
pub fn guard_launch_v1(
    manifest: Option<&Path>,
    u_max: u64,
    workers: usize,
    learner: LearnerKindV1,
    identity: &SourceIdentityV1,
    host: &HostClassV1,
    available_vcpus: usize,
) -> Result<Option<String>> {
    if u_max <= 1 {
        return match manifest {
            None => Ok(None),
            Some(_) => Err("a one-update run takes no qualification manifest".into()),
        };
    }
    let path = manifest.ok_or(
        "a campaign of more than one update needs -qualification-manifest (COMPUTE-POLICY item 5)",
    )?;
    let bytes = read_bytes(path)?;
    let parsed: QualificationManifestV1 =
        serde_json::from_slice(&bytes).map_err(|error| format!("manifest: {error}"))?;
    if to_bytes(&parsed)? != bytes || parsed.schema != MANIFEST_SCHEMA_V1 {
        return Err("qualification manifest is not a canonical v1 manifest".into());
    }
    let checks = [
        (
            parsed.binary_source_sha256 == identity.binary_source_sha256,
            "binary_source_sha256",
        ),
        (parsed.feature_sha256 == learner.feature_sha256(), "feature_sha256"),
        (parsed.card_db_hash == identity.card_db_hash, "card_db_hash"),
        (
            parsed.learner_contract == learner.contract(),
            "learner_contract",
        ),
        (parsed.host_class == *host, "host class"),
        (
            parsed.host_class.trainer_binary_sha256 == identity.binary_sha256,
            "trainer binary",
        ),
        (parsed.u_max == u_max, "U_max"),
        (parsed.workers == workers, "worker count W*"),
        (parsed.workers >= 2, "W* of at least 2"),
        (CAMPAIGN_HORIZON_V1.contains(&parsed.u_max), "U_max in 100..=200"),
    ];
    if let Some((_, name)) = checks.iter().find(|(holds, _)| !holds) {
        return Err(format!("qualification manifest refuses this run: {name}"));
    }
    admit_receipt_host_class_v1(host, available_vcpus)?;
    Ok(Some(sha256_hex(&bytes)))
}

fn available_vcpus_v1() -> usize {
    std::thread::available_parallelism().map_or(0, |count| count.get())
}

fn prepare_fresh_root(root: &Path) -> Result<()> {
    fs::create_dir(root).map_err(io_error(root))?;
    for directory in ["checkpoints", "iterations"] {
        let path = root.join(directory);
        fs::create_dir(&path).map_err(io_error(&path))?;
    }
    Ok(())
}

fn open_diagnostics(launch: &LaunchV1) -> Result<Option<DiagnosticsV1>> {
    launch
        .stage_diagnostics
        .then(|| DiagnosticsV1::open(&launch.root))
        .transpose()
}

/// Starts a fresh campaign, smoke or qualification run in `launch.root`
/// (which must not exist) and runs it to its horizon.
fn start_runner(
    launch: &LaunchV1,
    mode: ModeV1,
    learner: LearnerKindV1,
    u_max: u64,
) -> Result<RunnerV1> {
    if launch.workers == 0 {
        return Err("at least one worker".into());
    }
    let inventory = read_inventory_v1(&launch.repo_root)?;
    let decks = load_train_decks_v1(&launch.repo_root, &inventory)?;
    let identity = source_identity_v1(&launch.git_head, &launch.binary_sha256, &inventory);
    let manifest_sha256 = match mode {
        ModeV1::Campaign => guard_launch_v1(
            launch.qualification_manifest.as_deref(),
            u_max,
            launch.workers,
            learner,
            &identity,
            &launch.host_class,
            available_vcpus_v1(),
        )?,
        ModeV1::Smoke if !SMOKE_HORIZON_V1.contains(&u_max) => {
            return Err("a smoke run has one or two updates".into())
        }
        ModeV1::Qualification if u_max != 1 => {
            return Err("the qualification workload is one update".into())
        }
        _ if launch.qualification_manifest.is_some() => {
            return Err("smoke and qualification runs take no manifest".into())
        }
        _ => None,
    };
    if u_max == 0 {
        return Err("U_max must be at least 1".into());
    }
    let qualification_pairs = match mode {
        ModeV1::Qualification => {
            let names = decks
                .iter()
                .map(|deck| deck.name.clone())
                .collect::<Vec<_>>();
            Some(gate2_pairs_v1(&names, &DRAKE_HATCHER_TRAIN_DECKS_V1)?)
        }
        _ => None,
    };
    let config = campaign_config_v1(ConfigInputs {
        mode,
        learner,
        u_max,
        identity,
        inventory: &inventory,
        decks: &decks,
        qualification_manifest_sha256: manifest_sha256,
        qualification_pairs,
    });
    prepare_fresh_root(&launch.root)?;
    let campaign_sha256 = publish_atomic_v1(&launch.root, "campaign.json", &to_bytes(&config)?)?;
    record_launch(launch, "start", None)?;
    let mut runner = RunnerV1 {
        root: launch.root.clone(),
        learner: initial_learner_v1(learner, u_max)?,
        config,
        campaign_sha256,
        decks,
        head: 0,
        head_sha256: String::new(),
        snapshots: BTreeMap::new(),
        previous_truncated: None,
        dev_scores: BTreeMap::new(),
        workers: launch.workers,
        diagnostics: open_diagnostics(launch)?,
        stages: RunStagesV1::default(),
        fault: fault_point_v1(),
        natural_games: 0,
        decisions: 0,
    };
    runner.commit_initialization()?;
    Ok(runner)
}

/// `train` (mode campaign) and `smoke`.
pub fn start_v1(
    launch: &LaunchV1,
    mode: ModeV1,
    learner: LearnerKindV1,
    u_max: u64,
) -> Result<RunOutcomeV1> {
    if mode == ModeV1::Qualification {
        return Err("use qualify_v1 for the qualification workload".into());
    }
    start_runner(launch, mode, learner, u_max)?.run_to_horizon()
}

/// The highest `u` such that `iter_0000` to `iter_u` are all committed.
fn committed_head(root: &Path) -> Result<Option<u64>> {
    let mut head = None;
    let mut update = 0;
    while iteration_dir_v1(root, update).join("complete.json").exists() {
        head = Some(update);
        update += 1;
    }
    Ok(head)
}

/// Removes every uncommitted file a killed run can leave: staging
/// directories and files, and checkpoints after the committed head.
fn clean_uncommitted(root: &Path, head: u64) -> Result<()> {
    let iterations = root.join("iterations");
    for entry in fs::read_dir(&iterations).map_err(io_error(&iterations))? {
        let path = entry.map_err(io_error(&iterations))?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') && name.ends_with(".staging") {
            fs::remove_dir_all(&path).map_err(io_error(&path))?;
        } else if let Some(update) = name.strip_prefix("iter_").and_then(|u| u.parse::<u64>().ok())
        {
            if update > head {
                return Err(format!(
                    "{} exists after the committed head {head}: the chain has a gap",
                    path.display()
                ));
            }
        }
    }
    let checkpoints = root.join("checkpoints");
    for entry in fs::read_dir(&checkpoints).map_err(io_error(&checkpoints))? {
        let path = entry.map_err(io_error(&checkpoints))?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let uncommitted = name
            .strip_prefix("step_")
            .and_then(|rest| rest.strip_suffix(".json"))
            .and_then(|step| step.parse::<u64>().ok())
            .is_some_and(|step| step > head);
        if uncommitted || (name.starts_with('.') && name.ends_with(".staging")) {
            fs::remove_file(&path).map_err(io_error(&path))?;
        }
    }
    for entry in fs::read_dir(root).map_err(io_error(root))? {
        let path = entry.map_err(io_error(root))?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') && name.ends_with(".staging") {
            fs::remove_file(&path).map_err(io_error(&path))?;
        }
    }
    Ok(())
}

/// Cold resume (design Section 3.4): verifies `campaign.json` against this
/// build and inventory, verifies the whole chain to the highest contiguous
/// committed iteration, removes uncommitted staging, and continues at the
/// next update with the outputs an uninterrupted run would have written.
pub fn resume_v1(launch: &LaunchV1) -> Result<RunOutcomeV1> {
    let root = &launch.root;
    if root.join("complete.json").exists() {
        return Err("the campaign is already complete".into());
    }
    let config_bytes = read_bytes(&root.join("campaign.json"))?;
    let config: CampaignConfigV1 =
        serde_json::from_slice(&config_bytes).map_err(|error| format!("campaign.json: {error}"))?;
    if to_bytes(&config)? != config_bytes {
        return Err("campaign.json is not in canonical form".into());
    }
    let mode = mode_of(&config)?;
    if mode == ModeV1::Qualification {
        return Err("a qualification run is not resumed: run it again".into());
    }
    let learner = LearnerKindV1::from_contract(&config.learner_contract)?;
    let inventory = read_inventory_v1(&launch.repo_root)?;
    let decks = load_train_decks_v1(&launch.repo_root, &inventory)?;
    let identity = source_identity_v1(&launch.git_head, &launch.binary_sha256, &inventory);
    let manifest_sha256 = match mode {
        ModeV1::Campaign => guard_launch_v1(
            launch.qualification_manifest.as_deref(),
            config.u_max,
            launch.workers,
            learner,
            &identity,
            &launch.host_class,
            available_vcpus_v1(),
        )?,
        _ => None,
    };
    let expected = campaign_config_v1(ConfigInputs {
        mode,
        learner,
        u_max: config.u_max,
        identity,
        inventory: &inventory,
        decks: &decks,
        qualification_manifest_sha256: manifest_sha256,
        qualification_pairs: None,
    });
    if expected != config {
        return Err(
            "campaign.json differs from this build, inventory or manifest: refusing to resume"
                .into(),
        );
    }
    if learner == LearnerKindV1::PairCenteredControl {
        // The control arm uses the platform math library: it replays within
        // the host class of its first launch only.
        let first = read_bytes(&root.join("launches").join("launch_0000.json"))?;
        let first: Value = serde_json::from_slice(&first).map_err(|error| error.to_string())?;
        if first["host_class"] != serde_json::to_value(&launch.host_class).unwrap_or(Value::Null)
        {
            return Err("the control arm resumes on its first launch's host class only".into());
        }
    }
    let head = committed_head(root)?.ok_or("no committed iteration: start the run again")?;
    clean_uncommitted(root, head)?;
    let chain = super::checkpoint::verify_chain_v1(root, head)?;
    let campaign_sha256 = sha256_hex(&config_bytes);
    if chain
        .iter()
        .any(|link| link.checkpoint.campaign_sha256 != campaign_sha256)
    {
        return Err("a checkpoint belongs to another campaign".into());
    }
    let mut snapshots = BTreeMap::new();
    let mut dev_scores = BTreeMap::new();
    for link in &chain {
        let step = link.checkpoint.step;
        if step % LEAGUE_INTERVAL_V1 == 0 {
            snapshots.insert(step, link.checkpoint.policy_v1()?);
        }
        if link.complete.dev_sha256.is_some() {
            let dev = read_bytes(&iteration_dir_v1(root, step).join("dev.json"))?;
            let dev: Value = serde_json::from_slice(&dev).map_err(|error| error.to_string())?;
            let score = dev["score"].as_f64().ok_or("dev.json has no score")?;
            dev_scores.insert(step, score);
        }
    }
    let previous_truncated = if head == 0 {
        None
    } else {
        let receipt = read_bytes(&iteration_dir_v1(root, head).join("receipt.json"))?;
        let receipt: Value =
            serde_json::from_slice(&receipt).map_err(|error| error.to_string())?;
        Some(
            receipt["truncated_games"]
                .as_u64()
                .ok_or("receipt has no truncated_games")?,
        )
    };
    let last = chain.last().ok_or("empty chain")?;
    let learner_state = last.checkpoint.restore_learner_v1()?;
    let head_sha256 = last.sha256.clone();
    record_launch(launch, "resume", Some(head))?;
    let mut runner = RunnerV1 {
        root: root.clone(),
        config,
        campaign_sha256,
        decks,
        learner: learner_state,
        head,
        head_sha256,
        snapshots,
        previous_truncated,
        dev_scores,
        workers: launch.workers,
        diagnostics: open_diagnostics(launch)?,
        stages: RunStagesV1::default(),
        fault: fault_point_v1(),
        natural_games: 0,
        decisions: 0,
    };
    runner.run_to_horizon()
}

/// Verifies a campaign directory's chain to its committed head.
pub fn verify_v1(root: &Path) -> Result<u64> {
    let head = committed_head(root)?.ok_or("no committed iteration")?;
    super::checkpoint::verify_chain_v1(root, head)?;
    Ok(head)
}

/// The Gate 2 workload (design Section 5.2 item 3): the ruled eight pairs
/// at 657_001 with full retention, one update, checkpoint write and reload,
/// then two paired dev deals of `step_1` against `step_0` written to the
/// canonical `evaluation.json`. Timing goes to `timing.json` and the run's
/// description to `qualification_run.json`; neither is byte-compared.
pub fn qualify_v1(launch: &LaunchV1, learner: LearnerKindV1) -> Result<Value> {
    let started = Instant::now();
    let mut runner = start_runner(launch, ModeV1::Qualification, learner, 1)?;
    let collection_started = Instant::now();
    if let Some(outcome) = runner.run_update(1)? {
        return Err(format!(
            "the qualification update stopped: {}",
            outcome.stop_reason()
        ));
    }
    let training_ns = super::diagnostics::elapsed_ns(collection_started);
    let training_games = runner.natural_games;
    let training_decisions = runner.decisions;
    let (step_1, step_1_sha256) = CheckpointV1::load_v1(&checkpoint_path_v1(&launch.root, 1))?;
    let candidate = step_1.policy_v1()?;
    let evaluation_started = Instant::now();
    let (games, half_points) = runner.panel(
        &candidate,
        "candidate:step_0001".into(),
        runner.config.dev_pairs,
    )?;
    let evaluation_ns = super::diagnostics::elapsed_ns(evaluation_started);
    let games_count = games.len() as u64;
    let evaluation = json!({
        "schema": EVALUATION_SCHEMA_V1,
        "campaign_sha256": runner.campaign_sha256,
        "candidate_checkpoint_sha256": step_1_sha256,
        "opponent": "step_0000",
        "seed": runner.config.dev_seed,
        "pairs": runner.config.dev_pairs,
        "games": games,
        "half_points": half_points,
        "score": half_points as f64 / (2 * games_count) as f64,
    });
    let evaluation_sha256 =
        publish_atomic_v1(&launch.root, "evaluation.json", &to_bytes(&evaluation)?)?;
    runner.finish_campaign(RunOutcomeV1::Horizon)?;
    let wall_ns = super::diagnostics::elapsed_ns(started);
    let per_second = |count: u64, ns: u64| {
        if ns == 0 {
            0.0
        } else {
            count as f64 / (ns as f64 / 1e9)
        }
    };
    let timing = json!({
        "schema": "limited-campaign-qualification-timing/v1",
        "workers": launch.workers,
        "stage_diagnostics": launch.stage_diagnostics,
        "wall_ns": wall_ns,
        "training_ns": training_ns,
        "evaluation_ns": evaluation_ns,
        "training_natural_games": training_games,
        "training_decisions": training_decisions,
        "all_natural_games": runner.natural_games,
        "all_decisions": runner.decisions,
        "completed_natural_games_per_sec": per_second(training_games, training_ns),
        "decisions_per_sec": per_second(training_decisions, training_ns),
        "peak_rss_bytes": peak_rss_bytes_v1(),
        "peak_rss_note": "process peak; the workers are threads of one process",
        "stages": runner.stages,
    });
    publish_atomic_v1(&launch.root, "timing.json", &to_bytes(&timing)?)?;
    let run = json!({
        "schema": QUALIFICATION_RUN_SCHEMA_V1,
        "identity": runner.config.identity,
        "learner_contract": runner.config.learner_contract,
        "feature_sha256": runner.config.feature_sha256,
        "host_class": launch.host_class,
        "workers": launch.workers,
        "stage_diagnostics": launch.stage_diagnostics,
        "qualification_pairs": runner.config.qualification_pairs,
        "step_0001_sha256": step_1_sha256,
        "evaluation_sha256": evaluation_sha256,
        "timing": timing,
    });
    publish_atomic_v1(&launch.root, "qualification_run.json", &to_bytes(&run)?)?;
    Ok(run)
}

/// Locks `W*` and `U_max` into a qualification manifest from qualification
/// runs on one host: every run must have the same identity, learner, host
/// class, Gate 2 pairs, `step_0001.json` digest and `evaluation.json`
/// digest; the runs must include one worker, `W*`, diagnostics on and off;
/// the host class must be a receipt class; `W* >= 2` and `U_max` in
/// `100..=200`. Each run's speedup `T(1) / T(W)` and efficiency `S / W` are
/// computed from its training time against the fastest one-worker run.
pub fn lock_manifest_v1(
    run_roots: &[PathBuf],
    workers: usize,
    u_max: u64,
    available_vcpus: usize,
) -> Result<QualificationManifestV1> {
    let mut runs = Vec::new();
    for root in run_roots {
        let bytes = read_bytes(&root.join("qualification_run.json"))?;
        let run: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if run["schema"] != QUALIFICATION_RUN_SCHEMA_V1 {
            return Err(format!("{}: not a qualification run", root.display()));
        }
        // The run's files must still have the recorded digests.
        let step = sha256_hex(&read_bytes(&checkpoint_path_v1(root, 1))?);
        let evaluation = sha256_hex(&read_bytes(&root.join("evaluation.json"))?);
        if run["step_0001_sha256"] != step || run["evaluation_sha256"] != evaluation {
            return Err(format!("{}: files differ from its record", root.display()));
        }
        runs.push(run);
    }
    let first = runs.first().ok_or("no qualification runs")?.clone();
    for run in &runs {
        for key in [
            "identity",
            "learner_contract",
            "feature_sha256",
            "host_class",
            "qualification_pairs",
            "step_0001_sha256",
            "evaluation_sha256",
        ] {
            if run[key] != first[key] {
                return Err(format!("qualification runs differ in {key}"));
            }
        }
    }
    let worker_counts = runs
        .iter()
        .map(|run| run["workers"].as_u64().unwrap_or(0) as usize)
        .collect::<Vec<_>>();
    let diagnostics = runs
        .iter()
        .map(|run| run["stage_diagnostics"].as_bool().unwrap_or(false))
        .collect::<Vec<_>>();
    if !worker_counts.contains(&1)
        || !worker_counts.contains(&workers)
        || !diagnostics.contains(&true)
        || !diagnostics.contains(&false)
    {
        return Err(
            "the runs must include one worker, W*, and diagnostics both on and off".into(),
        );
    }
    if workers < 2 || !CAMPAIGN_HORIZON_V1.contains(&u_max) {
        return Err("W* must be at least 2 and U_max in 100..=200".into());
    }
    let host: HostClassV1 =
        serde_json::from_value(first["host_class"].clone()).map_err(|error| error.to_string())?;
    admit_receipt_host_class_v1(&host, available_vcpus)?;
    let identity: SourceIdentityV1 =
        serde_json::from_value(first["identity"].clone()).map_err(|error| error.to_string())?;
    if !is_sha256_hex(&identity.binary_sha256) || host.trainer_binary_sha256 != identity.binary_sha256
    {
        return Err("the host class does not name the runs' trainer binary".into());
    }
    let training_ns = |run: &Value| run["timing"]["training_ns"].as_u64().unwrap_or(0);
    let baseline = runs
        .iter()
        .filter(|run| run["workers"] == 1)
        .map(training_ns)
        .min()
        .unwrap_or(0);
    let run_rows = runs
        .iter()
        .zip(run_roots)
        .map(|(run, root)| {
            let w = run["workers"].as_u64().unwrap_or(0);
            let t = training_ns(run);
            let speedup = if t == 0 { 0.0 } else { baseline as f64 / t as f64 };
            json!({
                "directory": root.display().to_string(),
                "workers": w,
                "stage_diagnostics": run["stage_diagnostics"],
                "training_ns": t,
                "completed_natural_games_per_sec": run["timing"]["completed_natural_games_per_sec"],
                "decisions_per_sec": run["timing"]["decisions_per_sec"],
                "peak_rss_bytes": run["timing"]["peak_rss_bytes"],
                "speedup": speedup,
                "efficiency": if w == 0 { 0.0 } else { speedup / w as f64 },
            })
        })
        .collect();
    Ok(QualificationManifestV1 {
        schema: MANIFEST_SCHEMA_V1.into(),
        binary_source_sha256: identity.binary_source_sha256,
        feature_sha256: first["feature_sha256"]
            .as_str()
            .unwrap_or_default()
            .into(),
        card_db_hash: identity.card_db_hash,
        learner_contract: first["learner_contract"]
            .as_str()
            .unwrap_or_default()
            .into(),
        host_class: host,
        qualification_pairs: serde_json::from_value(first["qualification_pairs"].clone())
            .map_err(|error| error.to_string())?,
        step_0001_sha256: first["step_0001_sha256"]
            .as_str()
            .unwrap_or_default()
            .into(),
        evaluation_sha256: first["evaluation_sha256"]
            .as_str()
            .unwrap_or_default()
            .into(),
        runs: run_rows,
        workers,
        u_max,
    })
}

/// The canonical bytes of a manifest.
pub fn manifest_bytes_v1(manifest: &QualificationManifestV1) -> Result<Vec<u8>> {
    to_bytes(manifest)
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
#[path = "campaign_tests.rs"]
mod tests;
