//! A public, retained saved-batch fixture of the ordinary native S GAE
//! update (T3, #597; `docs/design/s_update_fixture_v1.md`), extracted from
//! `s_learner_collects_updates_and_runs_against_net8_opponents`. That test
//! plays real S and net-8 games with a one-layer, eight-wide S and deletes
//! its output; this module builds the same sources and keeps the inputs.
//!
//! [`generate_v1`] writes, under one new directory, the sources and two
//! batches, each with a serial reference update:
//! - `initial`, collected at the fresh start (Adam step 0, zero moments)
//!   against the learner itself and net-8;
//! - `warmed`, collected at the state the `initial` reference publishes
//!   (step 1, nonzero moments) against itself and the initial S (and net-8
//!   from the third episode on).
//!
//! [`update_command_v1`] gives a batch's update command for any build's
//! `expanded_deck_training_v1` binary, baselines that predate this module
//! included; [`verify_v1`] checks a finished update against the batch's
//! reference; [`bench_v1`] runs and verifies updates in this process and
//! reports elapsed and CPU time, peak RSS and, in `s-update-profile-v1`
//! builds, the forward counts of `s_transformer_v1::update_profile_v1`.
//!
//! Episode files embed absolute paths (#475), so a fixture is used where it
//! was generated and moved by generating it again. [`BatchIdentityV1`] is
//! root-independent: two generations with the same options and build
//! compare exactly.

use super::s_learner_v1::{SLearnerCheckpointV1, SLearnerSourceV1, S_LEARNER_SOURCE_SCHEMA_V1};
use super::*;
use crate::s_transformer_v1::update_profile_v1 as s_update_profile;
use crate::s_transformer_v1::{
    decode_checkpoint_v1, encode_checkpoint_v1, SConfigV1, SInputV1, SModelV1, STapeV1,
    STrainStateV1,
};
use crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
use std::collections::BTreeMap;
use std::time::Instant;

pub const FIXTURE_SCHEMA_V1: &str = "mtg-kernel-s-update-fixture/v1";
pub const VERIFY_SCHEMA_V1: &str = "mtg-kernel-s-update-verify/v1";
pub const BENCH_SCHEMA_V1: &str = "mtg-kernel-s-update-bench/v1";
/// The manifest `generate_v1` writes at the fixture root.
pub const MANIFEST_FILE_V1: &str = "fixture.json";
/// The source test's S: one layer, eight wide, S's 257-row card table.
pub const TEST_CONFIG_V1: SConfigV1 = SConfigV1 {
    d_model: 8,
    layers: 1,
    heads: 2,
    ffn: 16,
    card_dim: 4,
    card_vocab: 257,
};
/// The source test's S initialization seed, for both shapes.
pub const S_INIT_SEED_V1: u64 = 20_261_103;
/// The source test's update settings (and the run driver test's).
pub const LEARNING_RATE_V1: f32 = 1.0e-3;
pub const VALUE_COEFFICIENT_V1: f32 = 0.5;
pub const DEFAULT_SEED_V1: u64 = 20_261_108;
pub const MAX_EPISODES_V1: usize = 64;
pub const MAX_COLLECTION_WORKERS_V1: usize = 64;
pub const MAX_REPS_V1: usize = 100;
/// The update's CPU ingestion bound (`MAX_BATCH_BYTES`), in MiB.
pub const MAX_BATCH_MEBIBYTES_V1: u64 = MAX_BATCH_BYTES / MEBIBYTE_V1;
const MEBIBYTE_V1: u64 = 1024 * 1024;
const MAX_MANIFEST_BYTES_V1: u64 = 16 * 1024 * 1024;
/// The batches in generation order, with the opponent cycle each one's
/// episodes follow: two episodes give the source test's two run-driver
/// iterations (self and net-8, then self and the initial S); more add
/// net-8 to the warmed batch.
const BATCHES_V1: [(&str, &[OpponentV1]); 2] = [
    ("initial", &[OpponentV1::Current, OpponentV1::Net8]),
    (
        "warmed",
        &[OpponentV1::Current, OpponentV1::Initial, OpponentV1::Net8],
    ),
];

/// The S model shape a fixture trains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureShapeV1 {
    /// The source test's S ([`TEST_CONFIG_V1`]).
    Test,
    /// W5's S ([`SConfigV1::s_v1`]: 96 wide, three layers): the
    /// representative forward and backward cost.
    S,
}

impl FixtureShapeV1 {
    pub fn config_v1(self) -> SConfigV1 {
        match self {
            Self::Test => TEST_CONFIG_V1,
            Self::S => SConfigV1::s_v1(),
        }
    }
}

/// An episode's opponent, resolved as the run driver resolves its
/// assignments: always an explicit source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpponentV1 {
    /// The learner's own source at the batch's state (self-play).
    Current,
    /// The synthetic net-8 shell.
    Net8,
    /// The learner's initial source (fresh start).
    Initial,
}

/// `generate_v1`'s options. Every size is explicitly bounded: the batch
/// limits are checked after collection, before the reference update.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureOptionsV1 {
    pub shape: FixtureShapeV1,
    /// Episodes per batch, 1..=`MAX_EPISODES_V1`.
    pub episodes: usize,
    /// Episode `i` of batch `b` plays seed `seed + 1000 * b + i`.
    pub seed: u64,
    /// `CollectParallel` workers (the trajectories do not depend on them).
    pub collection_workers: usize,
    /// Bound on a batch's trajectory file bytes, at most the update's
    /// 512 MiB ingestion bound.
    pub max_batch_mebibytes: u64,
    /// Bound on a batch's decoded tensor payload, as a prepared update
    /// counts it; prepared updates of the fixture use this bound.
    pub max_prepared_tensor_mebibytes: usize,
}

impl Default for FixtureOptionsV1 {
    fn default() -> Self {
        Self {
            shape: FixtureShapeV1::Test,
            episodes: 2,
            seed: DEFAULT_SEED_V1,
            collection_workers: 2,
            max_batch_mebibytes: MAX_BATCH_MEBIBYTES_V1,
            max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        }
    }
}

impl FixtureOptionsV1 {
    pub fn validate_v1(&self) -> Result<(), String> {
        ensure(
            (1..=MAX_EPISODES_V1).contains(&self.episodes),
            "episodes must be in 1..=64",
        )?;
        ensure(
            (1..=MAX_COLLECTION_WORKERS_V1).contains(&self.collection_workers),
            "collection_workers must be in 1..=64",
        )?;
        ensure(
            self.seed
                .checked_add(1000 * BATCHES_V1.len() as u64 + MAX_EPISODES_V1 as u64)
                .is_some(),
            "seed leaves no room for the episode seeds",
        )?;
        ensure(
            (1..=MAX_BATCH_MEBIBYTES_V1).contains(&self.max_batch_mebibytes),
            "max_batch_mebibytes must be in 1..=512",
        )?;
        validate_max_prepared_tensor_mebibytes_v1(self.max_prepared_tensor_mebibytes)
    }
}

/// Inclusive range and mean of one per-row quantity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeV1 {
    pub min: u64,
    pub max: u64,
    pub mean: f64,
}

impl RangeV1 {
    fn of_v1(values: &[u64]) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        Self {
            min: *values.iter().min().expect("nonempty"),
            max: *values.iter().max().expect("nonempty"),
            mean: values.iter().map(|v| *v as f64).sum::<f64>() / values.len() as f64,
        }
    }
}

/// What a batch holds: the rows the replay authenticates, the rows and
/// groups the learner trains on, and their sizes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchCensusV1 {
    pub episodes: usize,
    /// Every actor row: each one is replayed once.
    pub rows: usize,
    /// Learner-seat rows: replayed, then forwarded again by the learner.
    pub learner_rows: usize,
    /// Opponent-seat rows, by opponent: replayed, never trained on.
    pub other_rows: usize,
    pub other_rows_by_opponent: BTreeMap<String, usize>,
    /// Learner physical decisions (GAE groups), and how many of them have
    /// each substep count.
    pub learner_groups: usize,
    pub learner_groups_by_substeps: BTreeMap<String, usize>,
    /// Learner rows' S token count (`1 + objects + actions`).
    pub learner_row_tokens: RangeV1,
    /// Learner rows' forward tape bytes (`STapeV1::bytes_for_counts_v1`):
    /// what a retained tape of the row would hold.
    pub learner_row_tape_bytes: RangeV1,
    /// Sum of the learner rows' tape bytes: retaining every learner tape of
    /// the batch would hold this much.
    pub learner_tape_bytes_total: u64,
    pub trajectory_bytes: u64,
    /// Every row's decoded tensor payload, as a prepared update counts it.
    pub decoded_tensor_payload_bytes: u64,
}

/// A batch's root-independent identity: its games and the reference
/// update's outputs. `verify_v1` compares a finished update with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchIdentityV1 {
    /// SHA-256 over every episode's game content in batch order: id, seed,
    /// behavior state, deck configurations, decision rows and terminal.
    /// The trajectory pins embed absolute paths (#475); this does not.
    pub games_sha256: String,
    pub before_state_sha256: String,
    pub after_state_sha256: String,
    /// SHA-256 of the published S container: parameters, both Adam moments
    /// and step, bit for bit.
    pub after_container_sha256: String,
    pub parameter_sha256: String,
    pub adam_step: u64,
    pub physical_decisions: u64,
    pub policy_substeps: u64,
    /// The update's f32 loss terms, as its receipt records them (each JSON
    /// number identifies one f32 exactly).
    pub loss: f64,
    pub policy_sum: f64,
    pub value_sum: f64,
    pub policy_term: f64,
    pub value_term: f64,
    pub advantage_statistics: Value,
}

/// One retained batch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureBatchV1 {
    pub name: String,
    /// The learner source the batch was collected at; its update starts
    /// here.
    pub source: ExpandedModelSourceV1,
    /// Each episode's opponent, in order.
    pub opponents: Vec<OpponentV1>,
    pub collection: PinnedFileV1,
    pub trajectories: Vec<PinnedFileV1>,
    /// The serial reference update's receipt (`update.json`).
    pub reference: PinnedFileV1,
    pub identity: BatchIdentityV1,
    pub census: BatchCensusV1,
}

/// The update settings every command of the fixture uses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureLearningV1 {
    pub learning_rate: f32,
    pub value_coefficient: f32,
    pub loss_selection: ExpandedLossSelectionV1,
}

/// `fixture.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureManifestV1 {
    pub schema: String,
    /// The directory `generate_v1` wrote; every pin and source is under it.
    pub root: PathBuf,
    pub options: FixtureOptionsV1,
    pub config: SConfigV1,
    pub learning: FixtureLearningV1,
    pub net8: ExpandedModelSourceV1,
    pub initial: ExpandedModelSourceV1,
    pub batches: Vec<FixtureBatchV1>,
    /// SHA-256 of the batches' identities (canonical JSON, in order).
    pub identity_sha256: String,
    pub claim: String,
}

impl FixtureManifestV1 {
    pub fn batch_v1(&self, name: &str) -> Result<&FixtureBatchV1, String> {
        self.batches
            .iter()
            .find(|batch| batch.name == name)
            .ok_or_else(|| format!("the fixture has no batch {name:?} (initial or warmed)"))
    }
}

/// The update path a command or bench runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePathV1 {
    /// `Update`: the serial replay and the one-worker learner (the
    /// reference path).
    Serial,
    /// `UpdatePrepared`: ordered preparation and the learner, both with
    /// `workers` workers (the run driver's path for more than one).
    Prepared,
}

impl UpdatePathV1 {
    fn parse_v1(value: &str) -> Result<Self, String> {
        match value {
            "serial" => Ok(Self::Serial),
            "prepared" => Ok(Self::Prepared),
            _ => Err(format!("path must be serial or prepared, not {value:?}")),
        }
    }
}

pub fn loss_selection_v1() -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
    }
}

/// A fresh-initialization net-8 source (V4) without a checkpoint, written
/// under `root`: the S learner's FlatV4 shell and the net-8 opponent. The
/// model is the deterministic runner-fixed Net8, not a trained one.
pub fn net8_source_v1(root: &Path) -> Result<ExpandedModelSourceV1, String> {
    let identity = FRESH_FEATURE_IDENTITY_V4;
    let descriptor =
        super::fresh_initialization_source::try_write_synthetic_fresh_source_with_parameters_v1(
            &root.join("net8"),
            identity,
            |_| {},
        )?;
    let bytes = serde_json::to_vec(&descriptor).map_err(err)?;
    let path = root.join("net8-descriptor.json");
    fs::write(&path, &bytes).map_err(err)?;
    Ok(ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path,
            sha256: sha(&bytes),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
    })
}

/// `shell` with an S learner of shape `config`, written to `root/s0.bin`.
/// Its container carries one Adam step's nonzero moments and step 1, which
/// a fresh start must drop.
pub fn s_source_v1(
    root: &Path,
    shell: &ExpandedModelSourceV1,
    config: SConfigV1,
    seed: u64,
) -> Result<ExpandedModelSourceV1, String> {
    let model = SModelV1::init_pilot_scheme_v1(config, seed)?;
    let mut grad = vec![1.0e-3f32; model.values_v1().len()];
    for range in model.layout_v1().gauge_ranges_v1() {
        grad[range].fill(0.0);
    }
    let mut state = STrainStateV1::fresh_optimizer_v1(model);
    // One step with a nonzero gradient moves the parameters and leaves
    // nonzero moments and step 1, so the start's reset is visible.
    state.adam_step_with_gradient_v1(&grad, 1.0e-4)?;
    ensure(
        state.adam_step_v1() == 1
            && state.first_moments_v1().iter().any(|m| *m != 0.0)
            && state.second_moments_v1().iter().any(|m| *m != 0.0),
        "the initial S state lacks its step and nonzero moments",
    )?;
    let bytes = encode_checkpoint_v1(&state);
    let path = root.join("s0.bin");
    fs::write(&path, &bytes).map_err(err)?;
    Ok(ExpandedModelSourceV1 {
        s_learner: Some(Box::new(SLearnerSourceV1 {
            schema: S_LEARNER_SOURCE_SCHEMA_V1.into(),
            initial: PinnedFileV1 {
                path,
                sha256: sha(&bytes),
            },
        })),
        ..shell.clone()
    })
}

/// A checked-in Pauper deck's registered configuration.
pub fn deck_list_v1(id: &str) -> Result<ExpandedDeckListV1, String> {
    let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id)
        .map_err(|error| format!("checked-in Pauper deck {id}: {error:?}"))?;
    let cards = registration.registered_configuration();
    Ok(ExpandedDeckListV1 {
        label: id.into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    })
}

/// The source test's episode: Affinity (the learner, seat 0, first) against
/// Terror, preboard, with its decision limits.
pub fn episode_v1(
    id: &str,
    seed: u64,
    opponent: Option<ExpandedModelSourceV1>,
) -> Result<ExpandedEpisodeV1, String> {
    let decks = [deck_list_v1("Affinity")?, deck_list_v1("Terror")?];
    Ok(ExpandedEpisodeV1 {
        id: id.into(),
        seed,
        starting_player: 0,
        learner_seat: 0,
        opponent,
        opponent_kind: None,
        opponent_search: None,
        registered: decks.clone(),
        selected: decks,
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
        opening: None,
        mulligan_modes: None,
    })
}

/// The ordinary update command of `batch` on `path`, publishing into
/// `output_directory` (which must not exist). Defaults stay off the wire,
/// so any build's `expanded_deck_training_v1` binary reads it.
pub fn update_command_v1(
    manifest: &FixtureManifestV1,
    batch: &FixtureBatchV1,
    path: UpdatePathV1,
    workers: usize,
    output_directory: PathBuf,
) -> Result<ExpandedTrainingCommandV1, String> {
    let learning = &manifest.learning;
    Ok(match path {
        UpdatePathV1::Serial => {
            ensure(
                workers == 1,
                "path=serial runs one worker; use path=prepared for more",
            )?;
            ExpandedTrainingCommandV1::Update {
                source: batch.source.clone(),
                trajectories: batch.trajectories.clone(),
                learning_rate: learning.learning_rate,
                value_coefficient: learning.value_coefficient,
                update_backend: ExpandedUpdateBackendV1::Cpu,
                update_backward_execution: UpdateBackwardExecutionV1::Sequential,
                loss_selection: learning.loss_selection,
                line_b: None,
                training_math: TrainingMathV1::PlatformLibm,
                record_format: CheckpointRecordFormatV1::default(),
                output_directory,
            }
        }
        UpdatePathV1::Prepared => {
            validate_preparation_workers_v1(workers)?;
            ExpandedTrainingCommandV1::UpdatePrepared {
                source: batch.source.clone(),
                trajectories: batch.trajectories.clone(),
                learning_rate: learning.learning_rate,
                value_coefficient: learning.value_coefficient,
                update_backend: ExpandedUpdateBackendV1::Cpu,
                update_backward_execution: UpdateBackwardExecutionV1::Sequential,
                loss_selection: learning.loss_selection,
                preparation_workers: workers,
                max_prepared_tensor_mebibytes: manifest.options.max_prepared_tensor_mebibytes,
                line_b: None,
                training_math: TrainingMathV1::PlatformLibm,
                record_format: CheckpointRecordFormatV1::default(),
                record_envelope: RecordEnvelopeV1::default(),
                output_directory,
            }
        }
    })
}

fn fixture_episodes_v1(
    manifest: &FixtureManifestV1,
    source: &ExpandedModelSourceV1,
    ordinal: usize,
    opponents: &[OpponentV1],
) -> Result<Vec<ExpandedEpisodeV1>, String> {
    let (name, _) = BATCHES_V1[ordinal];
    opponents
        .iter()
        .enumerate()
        .map(|(index, opponent)| {
            let source = match opponent {
                OpponentV1::Current => source,
                OpponentV1::Net8 => &manifest.net8,
                OpponentV1::Initial => &manifest.initial,
            };
            let label = serde_json::to_value(opponent).map_err(err)?;
            let label = label.as_str().ok_or("opponent label")?;
            episode_v1(
                &format!("{name}-{index:02}-{label}"),
                manifest.options.seed + 1000 * ordinal as u64 + index as u64,
                Some(source.clone()),
            )
        })
        .collect()
}

/// Writes a new fixture at `out` (which must not exist) and returns its
/// manifest.
pub fn generate_v1(options: &FixtureOptionsV1, out: &Path) -> Result<FixtureManifestV1, String> {
    options.validate_v1()?;
    ensure(
        !out.exists(),
        "the fixture directory already exists; generate writes a new one",
    )?;
    fs::create_dir_all(out).map_err(err)?;
    let root = out.canonicalize().map_err(err)?;
    let sources = root.join("sources");
    fs::create_dir(&sources).map_err(err)?;
    let config = options.shape.config_v1();
    let net8 = net8_source_v1(&sources)?;
    let initial = s_source_v1(&sources, &net8, config, S_INIT_SEED_V1)?;
    let learning = FixtureLearningV1 {
        learning_rate: LEARNING_RATE_V1,
        value_coefficient: VALUE_COEFFICIENT_V1,
        loss_selection: loss_selection_v1(),
    };
    let mut manifest = FixtureManifestV1 {
        schema: FIXTURE_SCHEMA_V1.into(),
        root: root.clone(),
        options: options.clone(),
        config,
        learning,
        net8: net8.clone(),
        initial: initial.clone(),
        batches: Vec::new(),
        identity_sha256: String::new(),
        claim: "engineering measurement fixture: synthetic runner-fixed net-8 shell and opponent, \
                pilot-initialized S; no playing-strength or production-throughput claim"
            .into(),
    };
    let mut state = initial.clone();
    for (ordinal, (name, cycle)) in BATCHES_V1.iter().enumerate() {
        let directory = root.join(name);
        fs::create_dir(&directory).map_err(err)?;
        let opponents: Vec<OpponentV1> = (0..options.episodes)
            .map(|index| cycle[index % cycle.len()])
            .collect();
        let episodes = fixture_episodes_v1(&manifest, &state, ordinal, &opponents)?;
        let collection = execute_v1(ExpandedTrainingCommandV1::CollectParallel {
            source: state.clone(),
            episodes: episodes.clone(),
            workers: options.collection_workers,
            max_non_natural_episode_fraction: 0.0,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            record_envelope: RecordEnvelopeV1::default(),
            output_directory: directory.join("collect"),
        })?;
        let trajectories: Vec<PinnedFileV1> =
            serde_json::from_value(collection["trajectories"].clone()).map_err(err)?;
        let collection = pin_file_v1(&directory.join("collect").join("collection.json"))?;
        let (census, games_sha256) =
            census_v1(&trajectories, &episodes, &opponents, &config, options)?;
        let mut batch = FixtureBatchV1 {
            name: (*name).into(),
            source: state.clone(),
            opponents,
            collection,
            trajectories,
            reference: PinnedFileV1 {
                path: PathBuf::new(),
                sha256: String::new(),
            },
            identity: placeholder_identity_v1(),
            census,
        };
        let reference_directory = directory.join("reference");
        let result = execute_v1(update_command_v1(
            &manifest,
            &batch,
            UpdatePathV1::Serial,
            1,
            reference_directory.clone(),
        )?)?;
        batch.reference = pin_file_v1(&reference_directory.join("update.json"))?;
        batch.identity = identity_of_update_v1(&result, games_sha256)?;
        let checkpoint: PinnedFileV1 =
            serde_json::from_value(result["checkpoint"].clone()).map_err(err)?;
        state = ExpandedModelSourceV1 {
            checkpoint: Some(checkpoint),
            ..initial.clone()
        };
        manifest.batches.push(batch);
    }
    manifest.identity_sha256 = identity_sha256_v1(&manifest.batches)?;
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(err)?;
    fs::write(root.join(MANIFEST_FILE_V1), bytes).map_err(err)?;
    Ok(manifest)
}

/// Reads the fixture at `fixture`, refusing one that was generated
/// elsewhere: its pins and sources point at the root it was written under.
pub fn load_manifest_v1(fixture: &Path) -> Result<FixtureManifestV1, String> {
    let root = fixture
        .canonicalize()
        .map_err(|error| format!("fixture {}: {error}", fixture.display()))?;
    let path = root.join(MANIFEST_FILE_V1);
    let metadata = fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    ensure(
        metadata.is_file() && metadata.len() <= MAX_MANIFEST_BYTES_V1,
        "the fixture manifest is not a bounded regular file",
    )?;
    let manifest: FixtureManifestV1 =
        serde_json::from_slice(&fs::read(&path).map_err(err)?).map_err(err)?;
    ensure(
        manifest.schema == FIXTURE_SCHEMA_V1,
        "the fixture manifest has another schema",
    )?;
    if manifest.root != root {
        return Err(format!(
            "this fixture was generated at {} and is now at {}; its pins and sources are \
             absolute (#475), so generate it again here with its recorded options",
            manifest.root.display(),
            root.display()
        ));
    }
    ensure(
        manifest.identity_sha256 == identity_sha256_v1(&manifest.batches)?,
        "the fixture manifest's identity differs from its batches",
    )?;
    validate_fixture_v1(&manifest)?;
    Ok(manifest)
}

fn require_fixture_pin_path_v1(pin: &PinnedFileV1, expected: &Path) -> Result<(), String> {
    ensure(
        pin.path.as_path() == expected
            && pin.path.canonicalize().map_err(err)?.as_path() == expected,
        &format!(
            "fixture pin does not name its generated file {}",
            expected.display()
        ),
    )
}

fn validate_fixture_v1(manifest: &FixtureManifestV1) -> Result<(), String> {
    manifest.options.validate_v1()?;
    ensure(
        manifest.config == manifest.options.shape.config_v1()
            && manifest.learning.learning_rate.to_bits() == LEARNING_RATE_V1.to_bits()
            && manifest.learning.value_coefficient.to_bits() == VALUE_COEFFICIENT_V1.to_bits()
            && manifest.learning.loss_selection == loss_selection_v1(),
        "fixture config or learning settings differ from the generated fixture",
    )?;
    let sources = manifest.root.join("sources");
    let mut shell = manifest.initial.clone();
    let member = shell
        .s_learner
        .take()
        .ok_or("fixture initial S source is absent")?;
    ensure(
        shell == manifest.net8
            && shell.checkpoint.is_none()
            && member.schema == S_LEARNER_SOURCE_SCHEMA_V1,
        "fixture initial source does not use the generated net-8 shell",
    )?;
    require_fixture_pin_path_v1(&shell.play_import, &sources.join("net8-descriptor.json"))?;
    require_fixture_pin_path_v1(&member.initial, &sources.join("s0.bin"))?;
    let descriptor: super::fresh_initialization_source::ExpandedFreshInitializationSourceV1 =
        read_pinned(&shell.play_import)?;
    require_fixture_pin_path_v1(
        &descriptor.initialization,
        &sources.join("net8/initialization.json"),
    )?;
    require_fixture_pin_path_v1(
        &descriptor.parameters,
        &sources.join("net8/parameters.f32le"),
    )?;
    check_pinned_v1(&descriptor.initialization)?;
    check_pinned_v1(&descriptor.parameters)?;
    let initial = decode_checkpoint_v1(&read_pinned_bytes(&member.initial)?)?;
    ensure(
        initial.model_v1().config_v1() == manifest.config
            && initial.adam_step_v1() == 1
            && initial.first_moments_v1().iter().any(|m| *m != 0.0)
            && initial.second_moments_v1().iter().any(|m| *m != 0.0),
        "fixture starting container has another shape or optimizer state",
    )?;

    ensure(
        manifest.batches.len() == BATCHES_V1.len(),
        "fixture must contain the initial and warmed batches",
    )?;
    let mut source = manifest.initial.clone();
    let mut previous_state: Option<&str> = None;
    for (ordinal, (batch, (name, cycle))) in manifest.batches.iter().zip(BATCHES_V1).enumerate() {
        ensure(
            batch.name == name
                && batch.source == source
                && batch.opponents.len() == manifest.options.episodes
                && batch.trajectories.len() == manifest.options.episodes
                && batch.census.episodes == manifest.options.episodes
                && batch.opponents.iter().copied().eq(cycle
                    .iter()
                    .copied()
                    .cycle()
                    .take(manifest.options.episodes)),
            "fixture batch roster, source or opponent cycle differs",
        )?;
        ensure(
            batch.identity.adam_step == (ordinal + 1) as u64
                && previous_state
                    .is_none_or(|state| state == batch.identity.before_state_sha256.as_str()),
            "fixture Adam checkpoints are not one initial-to-warmed chain",
        )?;
        let directory = manifest.root.join(name);
        let collect = directory.join("collect");
        require_fixture_pin_path_v1(&batch.collection, &collect.join("collection.json"))?;
        for (index, pin) in batch.trajectories.iter().enumerate() {
            require_fixture_pin_path_v1(pin, &collect.join(format!("episode-{index:04}.json")))?;
        }
        let collection: Value = read_pinned(&batch.collection)?;
        ensure(
            collection["complete"] == json!(true)
                && collection["source"] == serde_json::to_value(&batch.source).map_err(err)?
                && collection["trajectories"]
                    == serde_json::to_value(&batch.trajectories).map_err(err)?
                && collection["behavior_state_sha256"] == batch.identity.before_state_sha256,
            "fixture collection does not bind its source, state and trajectory pins",
        )?;

        let reference = directory.join("reference");
        require_fixture_pin_path_v1(&batch.reference, &reference.join("update.json"))?;
        let result: Value = read_pinned(&batch.reference)?;
        let checkpoint: PinnedFileV1 =
            serde_json::from_value(result["checkpoint"].clone()).map_err(err)?;
        let state: PinnedFileV1 =
            serde_json::from_value(result["s_learner"]["state"].clone()).map_err(err)?;
        require_fixture_pin_path_v1(&checkpoint, &reference.join("checkpoint.json"))?;
        require_fixture_pin_path_v1(
            &state,
            &reference.join(s_learner_v1::S_LEARNER_STATE_FILE_V1),
        )?;
        let record: SLearnerCheckpointV1 = read_pinned(&checkpoint)?;
        ensure(
            record.state == state
                && record.state_sha256 == batch.identity.after_state_sha256
                && record.adam_step == batch.identity.adam_step
                && state.sha256 == batch.identity.after_container_sha256
                && record.trajectories == batch.trajectories,
            "fixture reference checkpoint does not bind its state and trajectories",
        )?;
        check_pinned_v1(&state)?;
        source.checkpoint = Some(checkpoint);
        previous_state = Some(batch.identity.after_state_sha256.as_str());
    }
    Ok(())
}

fn validate_bench_inputs_v1(
    manifest: &FixtureManifestV1,
    batch: &FixtureBatchV1,
) -> Result<(), String> {
    let ordinal = BATCHES_V1
        .iter()
        .position(|(name, _)| *name == batch.name)
        .ok_or("fixture batch is unknown")?;
    let episodes = fixture_episodes_v1(manifest, &batch.source, ordinal, &batch.opponents)?;
    let (census, games_sha256) = census_v1(
        &batch.trajectories,
        &episodes,
        &batch.opponents,
        &manifest.config,
        &manifest.options,
    )?;
    ensure(
        census == batch.census && games_sha256 == batch.identity.games_sha256,
        "fixture census or games identity differs from its pinned trajectories",
    )?;
    let reference: Value = read_pinned(&batch.reference)?;
    let verification = verify_v1(manifest, batch, &reference)?;
    ensure(
        verification["verified"] == json!(true),
        &format!(
            "fixture reference failed verification: {}",
            verification["mismatches"]
        ),
    )
}

/// Checks a finished update of `batch` (its receipt `result`) against the
/// batch's reference: every identity field, the published container bit
/// for bit, the checkpoint record outside its own path, and the run
/// driver's continuation read. Mismatches are listed, not raised.
pub fn verify_v1(
    manifest: &FixtureManifestV1,
    batch: &FixtureBatchV1,
    result: &Value,
) -> Result<Value, String> {
    let mut mismatches = Vec::new();
    let mut check = |ok: bool, field: &str| {
        if !ok {
            mismatches.push(field.to_owned());
        }
    };
    check(result["complete"] == json!(true), "complete");
    check(
        result["checkpoint_readback"] == json!(true),
        "checkpoint_readback",
    );
    check(
        result["source"] == serde_json::to_value(&batch.source).map_err(err)?,
        "source",
    );
    check(
        result["trajectories"] == serde_json::to_value(&batch.trajectories).map_err(err)?,
        "trajectories",
    );
    let identity = identity_of_update_v1(result, batch.identity.games_sha256.clone())?;
    let actual = serde_json::to_value(&identity).map_err(err)?;
    let expected = serde_json::to_value(&batch.identity).map_err(err)?;
    for (field, value) in expected.as_object().ok_or("identity is an object")? {
        // JSON float equality treats both signed zeros as equal.
        check(
            actual[field].to_string() == value.to_string(),
            field.as_str(),
        );
    }
    // The container, read through its pin: decoded, it must carry the
    // reference's state hash; any difference is localized.
    let container: PinnedFileV1 =
        serde_json::from_value(result["s_learner"]["state"].clone()).map_err(err)?;
    let reference_result: Value = read_pinned(&batch.reference)?;
    let reference_container: PinnedFileV1 =
        serde_json::from_value(reference_result["s_learner"]["state"].clone()).map_err(err)?;
    let state = decode_checkpoint_v1(&read_pinned_bytes(&container)?)?;
    check(
        state.state_sha256_v1() == batch.identity.after_state_sha256,
        "container_state_sha256",
    );
    if container.sha256 != reference_container.sha256 {
        let reference = decode_checkpoint_v1(&read_pinned_bytes(&reference_container)?)?;
        let same = |a: &[f32], b: &[f32]| {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
        };
        check(
            same(
                state.model_v1().values_v1(),
                reference.model_v1().values_v1(),
            ),
            "container_parameters",
        );
        check(
            same(state.first_moments_v1(), reference.first_moments_v1()),
            "container_first_moments",
        );
        check(
            same(state.second_moments_v1(), reference.second_moments_v1()),
            "container_second_moments",
        );
        check(
            state.adam_step_v1() == reference.adam_step_v1(),
            "container_adam_step",
        );
    }
    // The checkpoint record, outside the container's own path.
    let checkpoint: PinnedFileV1 =
        serde_json::from_value(result["checkpoint"].clone()).map_err(err)?;
    let reference_checkpoint: PinnedFileV1 =
        serde_json::from_value(reference_result["checkpoint"].clone()).map_err(err)?;
    let record = |pin: &PinnedFileV1| -> Result<SLearnerCheckpointV1, String> {
        let mut record: SLearnerCheckpointV1 = read_pinned(pin)?;
        record.state.path = PathBuf::new();
        Ok(record)
    };
    check(
        record(&checkpoint)? == record(&reference_checkpoint)?,
        "checkpoint_record",
    );
    // Recovery: the run driver's continuation read binds the update's
    // inputs and reaches the reference state.
    let next = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint),
        ..batch.source.clone()
    };
    match load_expanded_update_inference_v1(
        &next,
        &batch.trajectories,
        manifest.learning.learning_rate,
        manifest.learning.value_coefficient,
    ) {
        Ok(recovered) => check(
            recovered.state_sha256 == batch.identity.after_state_sha256
                && recovered.adam_step == batch.identity.adam_step,
            "recovery",
        ),
        Err(_) => check(false, "recovery"),
    }
    Ok(json!({"schema":VERIFY_SCHEMA_V1, "batch":batch.name,
        "verified":mismatches.is_empty(), "mismatches":mismatches,
        "after_state_sha256":identity.after_state_sha256,
        "after_container_sha256":identity.after_container_sha256,
        "adam_step":identity.adam_step}))
}

/// `bench_v1`'s options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchOptionsV1 {
    pub batch: String,
    pub path: UpdatePathV1,
    pub workers: usize,
    pub reps: usize,
}

/// Runs `reps` updates of one batch in this process, each published under
/// `out/rep-NNN` (`out` must not exist) and verified against the
/// reference, and writes `out/bench.json`. A failed verification is an
/// error after the receipt is written. One process per trial gives each
/// trial cold caches and its own peak RSS (`docs/design/s_update_fixture_v1.md`).
pub fn bench_v1(fixture: &Path, options: &BenchOptionsV1, out: &Path) -> Result<Value, String> {
    let manifest = load_manifest_v1(fixture)?;
    let batch = manifest.batch_v1(&options.batch)?;
    ensure(
        (1..=MAX_REPS_V1).contains(&options.reps),
        "reps must be in 1..=100",
    )?;
    // Validates the path and worker count before anything is written.
    update_command_v1(
        &manifest,
        batch,
        options.path,
        options.workers,
        PathBuf::new(),
    )?;
    ensure(
        !out.exists(),
        "the bench directory already exists; bench writes a new one",
    )?;
    validate_bench_inputs_v1(&manifest, batch)?;
    fs::create_dir_all(out).map_err(err)?;
    let out = out.canonicalize().map_err(err)?;
    let mut samples = Vec::with_capacity(options.reps);
    let mut failed = false;
    for rep in 0..options.reps {
        let command = update_command_v1(
            &manifest,
            batch,
            options.path,
            options.workers,
            out.join(format!("rep-{rep:03}")),
        )?;
        let peak_rss_reset = reset_peak_rss_v1();
        let cpu = crate::regret_census_v1::stage4a::process_cpu::seconds().ok();
        // The counters cover exactly this update: armed for the block.
        let (result, elapsed, totals) = {
            let armed = s_update_profile::arm_v1();
            let started = Instant::now();
            let result = execute_v1(command)?;
            (result, started.elapsed().as_secs_f64(), armed.totals_v1())
        };
        let cpu = cpu.and_then(|before| {
            crate::regret_census_v1::stage4a::process_cpu::seconds()
                .ok()
                .map(|after| after - before)
        });
        let peak_rss = peak_rss_bytes_v1();
        let verification = verify_v1(&manifest, batch, &result)?;
        let profile = forward_profile_v1(totals, &batch.census);
        let attribution_verified = (profile["enabled"] == json!(true)).then(|| {
            profile["counts_match_census"] == json!(true)
                && profile["learner_forwards_repeat_replayed_rows"] == json!(true)
        });
        failed = verification["verified"] != json!(true) || attribution_verified == Some(false);
        let mut sample = json!({"rep":rep, "elapsed_seconds":elapsed,
            "process_cpu_seconds":cpu, "peak_rss_bytes":peak_rss,
            "peak_rss_scope":if peak_rss_reset {"this update (VmHWM reset before it)"}
                else {"process lifetime (VmHWM reset unavailable)"},
            "numerical_backend":result["numerical_backend"],
            "verified":verification["verified"], "mismatches":verification["mismatches"],
            "attribution_verified":attribution_verified, "forward_profile":profile});
        for field in [
            "input_read_seconds",
            "behavior_replay_seconds",
            "learner_update_seconds",
            "checkpoint_io_seconds",
            "update_elapsed_seconds",
        ] {
            sample[field] = result[field].clone();
        }
        if let Some(preparation) = result.get("update_preparation") {
            sample["update_preparation"] = json!({
                "parallel_replay_seconds":preparation["parallel_replay_seconds"],
                "preparation_elapsed_seconds":preparation["preparation_elapsed_seconds"],
                "started_workers":preparation["started_workers"],
                "decoded_tensor_payload_bytes":preparation["decoded_tensor_payload_bytes"]});
        }
        samples.push(sample);
        if failed {
            break;
        }
    }
    let executable_sha256 = sha(&fs::read(std::env::current_exe().map_err(err)?).map_err(err)?);
    let receipt = json!({"schema":BENCH_SCHEMA_V1, "executable_sha256":executable_sha256, "fixture":manifest.root,
        "fixture_identity_sha256":manifest.identity_sha256, "shape":manifest.options.shape,
        "config":manifest.config, "batch":batch.name, "path":options.path,
        "workers":options.workers, "reps":options.reps, "census":batch.census,
        "reference":{"after_state_sha256":batch.identity.after_state_sha256,
            "after_container_sha256":batch.identity.after_container_sha256,
            "adam_step":batch.identity.adam_step},
        "build":build_v1(), "samples":samples,
        "claim":"timings of this build on this machine; a speedup claim needs A-B-B-A trials \
                 of baseline and candidate on one fixture (docs/design/s_update_fixture_v1.md)"});
    fs::write(
        out.join("bench.json"),
        serde_json::to_vec_pretty(&receipt).map_err(err)?,
    )
    .map_err(err)?;
    if failed {
        return Err(format!(
            "an update or its forward attribution failed verification; see {}",
            out.join("bench.json").display()
        ));
    }
    Ok(receipt)
}

/// The forward counts of one update next to the batch's census: whether
/// every row was replayed once and every learner row forwarded once more,
/// and whether those learner forwards reproduced the replayed outputs.
fn forward_profile_v1(
    totals: Option<[s_update_profile::StageTotalsV1; s_update_profile::STAGES_V1]>,
    census: &BatchCensusV1,
) -> Value {
    let Some(totals) = totals else {
        return json!({"enabled":false,
            "note":"forward counts need a build with --features s-update-profile-v1"});
    };
    use s_update_profile::StageV1;
    let stage = |stage: StageV1| totals[stage as usize];
    let mut stages = serde_json::Map::new();
    for (name, total) in s_update_profile::STAGE_NAMES_V1.iter().zip(totals) {
        stages.insert(
            (*name).into(),
            json!({"calls":total.calls, "thread_seconds":total.thread_nanos as f64 * 1e-9,
                "output_digest":format!("{:016x}", total.output_digest)}),
        );
    }
    let replay_learner = stage(StageV1::ReplayLearnerRow);
    let replay_other = stage(StageV1::ReplayOtherRow);
    let learner_forward = stage(StageV1::LearnerForward);
    let learner_backward = stage(StageV1::LearnerBackward);
    let forwards = replay_learner.calls + replay_other.calls + learner_forward.calls;
    let forward_nanos =
        replay_learner.thread_nanos + replay_other.thread_nanos + learner_forward.thread_nanos;
    let learner_rows = census.learner_rows as u64;
    let counts_match = replay_learner.calls == learner_rows
        && replay_other.calls == census.other_rows as u64
        && learner_forward.calls == learner_rows
        && learner_backward.calls == learner_rows;
    let repeat = counts_match && learner_forward.output_digest == replay_learner.output_digest;
    json!({"enabled":true, "stages":stages,
        "counts_match_census":counts_match,
        "learner_forwards_repeat_replayed_rows":repeat,
        "duplicate_forwards":repeat.then_some(learner_forward.calls),
        "forward_calls":forwards,
        "duplicate_share_of_forward_calls":(repeat && forwards != 0)
            .then(|| learner_forward.calls as f64 / forwards as f64),
        "duplicate_share_of_forward_thread_seconds":(repeat && forward_nanos != 0)
            .then(|| learner_forward.thread_nanos as f64 / forward_nanos as f64),
        "scope":"thread time of each call (wall time on the thread that ran it; parallel \
                 workers add up). learner_backward is backward_v1 on one worker and \
                 backward_into_v1 (term recording) on more; the ordered weight-gradient \
                 accumulation of the parallel learner is outside every stage"})
}

fn build_v1() -> Value {
    json!({"package_version":env!("CARGO_PKG_VERSION"),
        "debug_assertions":cfg!(debug_assertions),
        "s_update_profile_v1":s_update_profile::ENABLED_V1,
        "target":format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        "available_parallelism":std::thread::available_parallelism().map(|n| n.get()).ok()})
}

/// Linux: the process's peak resident set (`VmHWM`) in bytes.
fn peak_rss_bytes_v1() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kibibytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    kibibytes.checked_mul(1024)
}

/// Linux: resets `VmHWM` to the current resident set (`clear_refs` 5).
fn reset_peak_rss_v1() -> bool {
    fs::write("/proc/self/clear_refs", b"5").is_ok()
}

fn pin_file_v1(path: &Path) -> Result<PinnedFileV1, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(PinnedFileV1 {
        path: path.to_path_buf(),
        sha256: sha(&bytes),
    })
}

fn placeholder_identity_v1() -> BatchIdentityV1 {
    BatchIdentityV1 {
        games_sha256: String::new(),
        before_state_sha256: String::new(),
        after_state_sha256: String::new(),
        after_container_sha256: String::new(),
        parameter_sha256: String::new(),
        adam_step: 0,
        physical_decisions: 0,
        policy_substeps: 0,
        loss: 0.0,
        policy_sum: 0.0,
        value_sum: 0.0,
        policy_term: 0.0,
        value_term: 0.0,
        advantage_statistics: Value::Null,
    }
}

/// The root-independent identity of an S update's receipt.
fn identity_of_update_v1(result: &Value, games_sha256: String) -> Result<BatchIdentityV1, String> {
    let text = |value: &Value, field: &str| -> Result<String, String> {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("update receipt lacks {field}"))
    };
    let integer = |field: &str| -> Result<u64, String> {
        result[field]
            .as_u64()
            .ok_or_else(|| format!("update receipt lacks {field}"))
    };
    let number = |value: &Value, field: &str| -> Result<f64, String> {
        value
            .as_f64()
            .ok_or_else(|| format!("update receipt lacks {field}"))
    };
    let s = &result["s_learner"];
    Ok(BatchIdentityV1 {
        games_sha256,
        before_state_sha256: text(&result["before_state_sha256"], "before_state_sha256")?,
        after_state_sha256: text(&result["after_state_sha256"], "after_state_sha256")?,
        after_container_sha256: text(&s["state"]["sha256"], "s_learner.state.sha256")?,
        parameter_sha256: text(&s["parameter_sha256"], "s_learner.parameter_sha256")?,
        adam_step: integer("adam_step")?,
        physical_decisions: integer("physical_decisions")?,
        policy_substeps: integer("policy_substeps")?,
        loss: number(&result["loss"], "loss")?,
        policy_sum: number(&result["policy_sum"], "policy_sum")?,
        value_sum: number(&result["value_sum"], "value_sum")?,
        policy_term: number(&s["policy_term"], "s_learner.policy_term")?,
        value_term: number(&s["value_term"], "s_learner.value_term")?,
        advantage_statistics: result["advantage_statistics"].clone(),
    })
}

fn identity_sha256_v1(batches: &[FixtureBatchV1]) -> Result<String, String> {
    let identities: Vec<(&str, &BatchIdentityV1)> = batches
        .iter()
        .map(|batch| (batch.name.as_str(), &batch.identity))
        .collect();
    Ok(sha(&serde_json::to_vec(&identities).map_err(err)?))
}

/// A batch's census and games digest, from its trajectories.
fn census_v1(
    trajectories: &[PinnedFileV1],
    episodes: &[ExpandedEpisodeV1],
    opponents: &[OpponentV1],
    config: &SConfigV1,
    options: &FixtureOptionsV1,
) -> Result<(BatchCensusV1, String), String> {
    ensure(
        trajectories.len() == episodes.len() && trajectories.len() == opponents.len(),
        "the collection returned another episode count",
    )?;
    let trajectory_bytes = trajectories.iter().try_fold(0u64, |total, pin| {
        total
            .checked_add(fs::metadata(&pin.path).map_err(err)?.len())
            .ok_or_else(|| "trajectory file sizes overflow the fixture bound".to_owned())
    })?;
    ensure(
        trajectory_bytes <= options.max_batch_mebibytes * MEBIBYTE_V1,
        &format!(
            "batch holds {trajectory_bytes} trajectory bytes, over max_batch_mebibytes={}",
            options.max_batch_mebibytes
        ),
    )?;
    let mut census = BatchCensusV1 {
        episodes: trajectories.len(),
        trajectory_bytes,
        ..BatchCensusV1::default()
    };
    let mut games = Sha256::new();
    let mut tokens = Vec::new();
    let mut tape_bytes = Vec::new();
    for ((pin, episode), opponent) in trajectories.iter().zip(episodes).zip(opponents) {
        let trajectory: ExpandedTrajectoryV1 = read_pinned(pin)?;
        ensure(
            trajectory.episode == *episode,
            "fixture trajectory episode differs from the generated episode",
        )?;
        validate_trajectory(&trajectory)?;
        let content = serde_json::to_vec(&(
            &trajectory.episode.id,
            trajectory.episode.seed,
            &trajectory.behavior_state_sha256,
            &trajectory.configuration_sha256,
            &trajectory.decisions,
            &trajectory.terminal,
        ))
        .map_err(err)?;
        games.update(sha(&content).as_bytes());
        let label = serde_json::to_value(opponent).map_err(err)?;
        let label = label.as_str().ok_or("opponent label")?.to_owned();
        let learner = trajectory.episode.learner_seat;
        let mut index = 0;
        while index < trajectory.decisions.len() {
            let first = &trajectory.decisions[index];
            let count = first.substep_count as usize;
            ensure(
                count > 0 && index + count <= trajectory.decisions.len(),
                "a trajectory group overruns its decisions",
            )?;
            if first.actor == learner {
                census.learner_groups += 1;
                *census
                    .learner_groups_by_substeps
                    .entry(count.to_string())
                    .or_default() += 1;
            }
            index += count;
        }
        for row in &trajectory.decisions {
            census.rows += 1;
            census.decoded_tensor_payload_bytes +=
                super::ordered_update_preparation::decoded_payload_bytes(&row.tensor)? as u64;
            ensure(
                census.decoded_tensor_payload_bytes
                    <= options.max_prepared_tensor_mebibytes as u64 * MEBIBYTE_V1,
                "batch exceeds max_prepared_tensor_mebibytes",
            )?;
            if row.actor == learner {
                census.learner_rows += 1;
                let tensor = row.tensor.tensor();
                let counts = SInputV1::of_tensor_v1(&tensor).validate_v1(config)?;
                tokens.push((1 + counts.objects + counts.actions) as u64);
                tape_bytes.push(STapeV1::bytes_for_counts_v1(config, counts) as u64);
            } else {
                census.other_rows += 1;
                *census
                    .other_rows_by_opponent
                    .entry(label.clone())
                    .or_default() += 1;
            }
        }
    }
    census.learner_row_tokens = RangeV1::of_v1(&tokens);
    census.learner_row_tape_bytes = RangeV1::of_v1(&tape_bytes);
    census.learner_tape_bytes_total = tape_bytes.iter().sum();
    Ok((census, hex(&games.finalize())))
}

const USAGE_V1: &str = "usage:
  s_update_fixture_v1 generate out=<new dir> [shape=test|s] [episodes=1..64]
      [seed=<u64>] [collection_workers=1..64] [max_batch_mebibytes=1..512]
      [max_prepared_tensor_mebibytes=64..4096]
  s_update_fixture_v1 command fixture=<dir> batch=initial|warmed path=serial|prepared
      [workers=1..32] out=<new dir>
  s_update_fixture_v1 verify fixture=<dir> batch=initial|warmed result=<update.json>
  s_update_fixture_v1 bench fixture=<dir> batch=initial|warmed path=serial|prepared
      [workers=1..32] [reps=1..100] out=<new dir>
Unknown, repeated or out-of-range arguments are refused.";

/// Strict `key=value` arguments: each key at most once, every key known.
struct ArgumentsV1 {
    values: BTreeMap<String, String>,
}

impl ArgumentsV1 {
    fn parse_v1(rest: &[String], known: &[&str]) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        for arg in rest {
            let (key, value) = arg
                .split_once('=')
                .ok_or_else(|| format!("expected key=value, not {arg:?}\n{USAGE_V1}"))?;
            ensure(
                known.contains(&key),
                &format!("unknown argument {key:?}\n{USAGE_V1}"),
            )?;
            ensure(
                values.insert(key.to_owned(), value.to_owned()).is_none(),
                &format!("argument {key:?} is repeated"),
            )?;
        }
        Ok(Self { values })
    }

    fn text_v1(&self, key: &str) -> Result<&str, String> {
        self.values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("{key}= is required\n{USAGE_V1}"))
    }

    fn number_v1<T: std::str::FromStr>(&self, key: &str, default: T) -> Result<T, String> {
        self.values.get(key).map_or(Ok(default), |value| {
            value
                .parse()
                .map_err(|_| format!("{key}={value} is not a valid number"))
        })
    }
}

/// The `s_update_fixture_v1` binary.
pub fn main_v1(args: &[String]) -> Result<Value, String> {
    let (command, rest) = args.split_first().ok_or(USAGE_V1)?;
    match command.as_str() {
        "generate" => {
            let arguments = ArgumentsV1::parse_v1(
                rest,
                &[
                    "out",
                    "shape",
                    "episodes",
                    "seed",
                    "collection_workers",
                    "max_batch_mebibytes",
                    "max_prepared_tensor_mebibytes",
                ],
            )?;
            let defaults = FixtureOptionsV1::default();
            let shape = match arguments.values.get("shape").map(String::as_str) {
                None | Some("test") => FixtureShapeV1::Test,
                Some("s") => FixtureShapeV1::S,
                Some(other) => return Err(format!("shape must be test or s, not {other:?}")),
            };
            let options = FixtureOptionsV1 {
                shape,
                episodes: arguments.number_v1("episodes", defaults.episodes)?,
                seed: arguments.number_v1("seed", defaults.seed)?,
                collection_workers: arguments
                    .number_v1("collection_workers", defaults.collection_workers)?,
                max_batch_mebibytes: arguments
                    .number_v1("max_batch_mebibytes", defaults.max_batch_mebibytes)?,
                max_prepared_tensor_mebibytes: arguments.number_v1(
                    "max_prepared_tensor_mebibytes",
                    defaults.max_prepared_tensor_mebibytes,
                )?,
            };
            options.validate_v1()?;
            let out = PathBuf::from(arguments.text_v1("out")?);
            serde_json::to_value(generate_v1(&options, &out)?).map_err(err)
        }
        "command" => {
            let arguments =
                ArgumentsV1::parse_v1(rest, &["fixture", "batch", "path", "workers", "out"])?;
            let (fixture, name) = (arguments.text_v1("fixture")?, arguments.text_v1("batch")?);
            let path = UpdatePathV1::parse_v1(arguments.text_v1("path")?)?;
            let workers = arguments.number_v1("workers", 1)?;
            let out = PathBuf::from(arguments.text_v1("out")?);
            let manifest = load_manifest_v1(Path::new(fixture))?;
            let batch = manifest.batch_v1(name)?;
            ensure(
                !out.exists(),
                "out must name a new directory; the update creates it",
            )?;
            let parent = out
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .canonicalize()
                .map_err(|error| format!("out's parent directory: {error}"))?;
            let name = out.file_name().ok_or("out must name a directory")?;
            let command = update_command_v1(&manifest, batch, path, workers, parent.join(name))?;
            // Through text, so each f32 prints as its shortest decimal.
            serde_json::from_str(&serde_json::to_string(&command).map_err(err)?).map_err(err)
        }
        "verify" => {
            let arguments = ArgumentsV1::parse_v1(rest, &["fixture", "batch", "result"])?;
            let (fixture, name, path) = (
                arguments.text_v1("fixture")?,
                arguments.text_v1("batch")?,
                arguments.text_v1("result")?,
            );
            let manifest = load_manifest_v1(Path::new(fixture))?;
            let batch = manifest.batch_v1(name)?;
            let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
            let result: Value = serde_json::from_slice(&bytes).map_err(err)?;
            let verification = verify_v1(&manifest, batch, &result)?;
            if verification["verified"] != json!(true) {
                return Err(format!(
                    "the update differs from the reference: {}",
                    verification["mismatches"]
                ));
            }
            Ok(verification)
        }
        "bench" => {
            let arguments = ArgumentsV1::parse_v1(
                rest,
                &["fixture", "batch", "path", "workers", "reps", "out"],
            )?;
            let (fixture, out) = (arguments.text_v1("fixture")?, arguments.text_v1("out")?);
            let options = BenchOptionsV1 {
                batch: arguments.text_v1("batch")?.to_owned(),
                path: UpdatePathV1::parse_v1(arguments.text_v1("path")?)?,
                workers: arguments.number_v1("workers", 1)?,
                reps: arguments.number_v1("reps", 1)?,
            };
            bench_v1(Path::new(fixture), &options, Path::new(out))
        }
        _ => Err(USAGE_V1.into()),
    }
}

#[cfg(test)]
mod tests;
