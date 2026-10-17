//! Spy archive distillation screen (collab
//! LANES/spy-archive-distillation-20261013): the archive-row bank, the
//! phase-1 trainer, its checkpoint identity and the offline probe.
//!
//! - The bank replays each archived E search game on this binary from its
//!   seed, decks, seat and starting player. It checks every focal decision
//!   start against the archive's observation, menu and state hashes, every
//!   searched decision against its record (step, physical decision, substep,
//!   legal-action count and Spy flags), and the recorded end. At each
//!   eligible row (a supported tree choice among two or more legal actions)
//!   it captures the parent's own training input with the collector's call.
//!   A game failing any check is dropped whole and listed.
//! - Phase 1 takes auxiliary-only Adam steps from the parent on a fresh
//!   optimizer: a one-hot forward KL at archive rows plus a forward-KL anchor
//!   to the parent's softmax on the parent's own ordinary rows. Before the
//!   first step, the anchor alone over every anchor-training row, on the
//!   same fresh moments, must leave the parent unchanged bit for bit. A
//!   keep-set cell (collab LANES/bestline-target-test-20261015) trains only
//!   the rows its file lists, each toward the parent's own softmax over the
//!   live actions the file keeps there, in place of the one-hot target.
//! - Each phase-1 checkpoint holds the phase-1 parameters with the parent's
//!   exact Adam state under its own loss identity and a typed provenance
//!   record. The loader reads the named parent checkpoint and requires that
//!   state bit for bit, so ordinary training resumes exactly the parent's
//!   optimizer and distilled parameters never pass as ordinary ones.
//! - The probe reads candidates offline, with forward passes only. Given
//!   the search-evidence keep-set files of SD1 (collab
//!   LANES/search-evidence-distillation-20261016), it also reads each
//!   candidate's fit to those targets on training and held-out evidence
//!   rows, and its held-out anchor readings over all decks, the non-Spy
//!   decks and each deck.
//! - A cell labeled exactly `SD1-T-s{k}`, k from 0 to 7, seeds phase 1's
//!   batch orders with k ([`order_seed_v1`]); seed 0 and every label
//!   without the `SD1-T-s` prefix keep the unseeded orders, and any other
//!   label with that prefix is refused.
//! - Phase-1 options run one cell of the U collapse diagnostic (collab
//!   LANES/spy-u-collapse-20261014): arm U with one change to its step size,
//!   anchor coefficient, rows or checkpoint steps. Absent, phase 1 is the
//!   screen's frozen recipe exactly.
//! - The independent optional `update_scope: "scorer_only"` freezes every
//!   non-scorer parameter and both Adam moments. Both scorer layers update
//!   with the same Adam age; the final bias stays in its canonical gauge.
//!   Omission keeps historical full-network updates and serialized fields.
//!   Gradient telemetry still describes the unmasked loss derivatives.
//!
//! No policy plays a game here, and no outcome is ever a label or a readout.

use super::*;
use crate::line_b_teacher_target_v1::{
    line_b_divergence_v1, line_b_softmax_v1, LineBDivergenceV1, LineBSoftmaxV1,
};
use crate::native_policy_train_step_v1::{
    one_hot_target_v1, ArchiveDistillationUpdateScopeV1, ExternalAuxiliaryInputV1,
    ExternalAuxiliaryRowV1,
};
use crate::regret_census_v1::stage4a::{decision_replay_key, spy_menu_flags_v1};
use crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1;
use std::collections::{BTreeMap, HashMap};

pub const ARCHIVE_DISTILLATION_NAMESPACE_V1: &str = "spy-archive-distillation-v1";
/// The U collapse diagnostic's namespace (its count-control subsets).
pub const U_COLLAPSE_NAMESPACE_V1: &str = "spy-u-collapse-v1";
/// The loss identity of a phase-1 checkpoint: the parameters came from
/// archive distillation steps, not from the ordinary loss.
pub const PHASE1_LOSS_IDENTITY_V1: &str = "archive_distillation_phase1/v1";
const METHOD: &str = "spy-archive-distillation-phase1-v1";
const BANK_SCHEMA: &str = "mtg-kernel-archive-distillation-bank/v1";
const PHASE1_SCHEMA: &str = "mtg-kernel-archive-distillation-phase1/v1";
const PROBE_SCHEMA: &str = "mtg-kernel-archive-distillation-probe/v1";
pub const KEEP_SETS_SCHEMA_V1: &str = "mtg-kernel-archive-distillation-keep-sets/v1";
/// The frozen phase-1 recipe (design section 5).
const LEARNING_RATE: f32 = 1e-4;
const ARCHIVE_BATCH: usize = 32;
const ANCHOR_BATCH: usize = 256;
const ARCHIVE_COEFFICIENT: f64 = 1.0;
const ANCHOR_COEFFICIENT: f64 = 1.0;
const UNIFORM_EPOCHS: [u64; 6] = [1, 2, 4, 8, 16, 32];
const LINE_STEPS: [u64; 6] = [32, 64, 128, 256, 512, 1024];
/// The step whose end state a G3 rerun must reproduce.
const REPRODUCTION_STEP: u64 = 32;
const SPY_DECK: &str = "Spy";
/// The deployed sampler's gap clamp, in nats (the collapse probe's cap).
const GAP_CLAMP: f64 = 16.0;
/// Trajectories per anchor split or offer set.
const MAX_TRAJECTORIES: usize = 1024;

/// One focal physical decision start of an archive game, as the archive's
/// replay recorded it (`s4a-budget-replay` starts).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveStartV1 {
    pub step: u64,
    pub physical_decision_id: u64,
    pub k: u32,
    pub obs_hash: String,
    pub menu_hash: String,
    pub state_hash: String,
}

/// One focal decision with two or more legal actions, as the archive game
/// recorded it (`budget_game` decisions, projected to these fields).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDecisionV1 {
    pub step: u64,
    pub physical_decision_id: u64,
    pub substep: u32,
    pub k: u32,
    /// `new_root` or `reused` (the tree executed a supported edge), or
    /// `plain` (the plain policy chose).
    pub chooser: String,
    pub edge_backups: Option<u64>,
    pub spy_cast_offered: bool,
    pub spy_cast_chosen: bool,
    pub spy_self_offered: bool,
    pub spy_self_chosen: bool,
    pub dr_giant_offered: bool,
    pub dr_giant_chosen: bool,
}

/// One archive game. `episode` rebuilds its opening exactly as the archive
/// did (game seed as `seed`, focal seat as `learner_seat`, no reset and no
/// opponent); `actions` holds every applied action of both seats; `end` is
/// the focal player's recorded natural result.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveGameV1 {
    pub id: String,
    pub lineage: String,
    pub episode: ExpandedEpisodeV1,
    pub actions: Vec<u32>,
    pub starts: Vec<ArchiveStartV1>,
    pub decisions: Vec<ArchiveDecisionV1>,
    pub end: String,
}

/// A row's Spy line flags on its live menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineFlagsV1 {
    pub cast_offered: bool,
    pub cast_chosen: bool,
    pub self_offered: bool,
    pub self_chosen: bool,
    pub giant_offered: bool,
    pub giant_chosen: bool,
}

impl LineFlagsV1 {
    fn of_record(d: &ArchiveDecisionV1) -> Self {
        Self {
            cast_offered: d.spy_cast_offered,
            cast_chosen: d.spy_cast_chosen,
            self_offered: d.spy_self_offered,
            self_chosen: d.spy_self_chosen,
            giant_offered: d.dr_giant_offered,
            giant_chosen: d.dr_giant_chosen,
        }
    }

    /// The menu offers Spy's self-target or Dread Return's Giant target.
    pub fn line_offered(&self) -> bool {
        self.self_offered || self.giant_offered
    }

    /// The executed action is Spy's self-target or Dread Return on Giant.
    pub fn line_chosen(&self) -> bool {
        self.self_chosen || self.giant_chosen
    }
}

/// One eligible archive row: E's executed choice and the parent's own
/// training input and outputs there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveRowV1 {
    pub game: String,
    pub step: u64,
    pub physical_decision_id: u64,
    pub substep: u32,
    pub k: u32,
    /// The executed action, a live index (one-hot target).
    pub target: u32,
    pub chooser: String,
    pub edge_backups: Option<u64>,
    /// `floor(4 * step / actions)`, capped at 3.
    pub game_quartile: u8,
    pub line: LineFlagsV1,
    /// The live menu's Spy self-target actions and Dread-Return-on-Giant
    /// actions (the discrimination probe reads their probability).
    pub self_actions: Vec<u32>,
    pub giant_actions: Vec<u32>,
    /// The parent's raw logits and value at the row, as binary32 bits.
    pub parent_logits: Vec<u32>,
    pub parent_value: u32,
    /// SHA-256 of the row's `TensorBitsV1` JSON.
    pub tensor_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveGameStatusV1 {
    pub id: String,
    pub kept: bool,
    pub reason: Option<String>,
    pub rows: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveBankV1 {
    pub schema: String,
    pub namespace: String,
    pub lineage: String,
    pub parent: ExpandedModelSourceV1,
    pub parent_state_sha256: String,
    pub games: PinnedFileV1,
    /// The raw archive files `games` was built from (the archive file and
    /// every rows-frozen file holding a decision record), each verified
    /// against its SHA-256 before extraction.
    pub sources: Vec<PinnedFileV1>,
    pub runtime_commit: String,
    pub runtime_tree: String,
    pub runtime_binary_sha256: String,
    pub game_status: Vec<ArchiveGameStatusV1>,
    pub rows: Vec<ArchiveRowV1>,
    pub rows_sha256: String,
}

/// Which archive rows phase 1 trains on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveDistillationArmV1 {
    /// U: every eligible row, the method (no Spy label selects a row).
    Uniform,
    /// L: rows whose menu offers Spy's self-target or Dread Return's Giant
    /// target, whatever the tree chose. A forced-line capacity test, never
    /// a method (Jack's decision of 2026-10-13).
    LineDecisions,
}

impl ArchiveDistillationArmV1 {
    fn label(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::LineDecisions => "line_decisions",
        }
    }

    fn admits(self, row: &ArchiveRowV1) -> bool {
        match self {
            Self::Uniform => true,
            Self::LineDecisions => row.line.line_offered(),
        }
    }
}

/// One cell of the U collapse diagnostic (collab
/// LANES/spy-u-collapse-20261014): arm U's frozen recipe with the fields
/// given here replaced. Only arm U takes options, and at most one row rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationOptionsV1 {
    /// The cell's label (ASCII letters, digits and `-`), recorded in the
    /// recipe and in every checkpoint's provenance.
    pub cell: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learning_rate: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_coefficient: Option<f64>,
    /// Keeps the rows whose executed edge had at least this many backups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_edge_backups: Option<u64>,
    /// Keeps this many rows: the lowest by a hash of each row's game and
    /// step, which reads no label, outcome or count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_control: Option<usize>,
    /// Checkpoint steps, strictly increasing, in place of the epoch
    /// schedule; the last is the run's length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_steps: Option<Vec<u64>>,
    /// Keeps exactly the rows this [`ArchiveDistillationKeepSetsV1`] file
    /// lists, each with the parent's softmax over its kept live actions as
    /// the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_sets: Option<PinnedFileV1>,
}

/// A cell as its checkpoints record it: the resolved recipe, in bits. The
/// step size is the provenance's own `learning_rate_bits`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationCellV1 {
    pub cell: String,
    pub anchor_coefficient_bits: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_edge_backups: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_control: Option<u64>,
    pub checkpoint_steps: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_sets: Option<PinnedFileV1>,
}

/// A keep-set file: one lineage's rows of one bank, and at each row the
/// live actions its target keeps, strictly increasing. Phase 1 trains on
/// exactly these rows. A row's target is the binary64 softmax of the
/// parent's raw logits over its kept actions, built as the anchor's targets
/// are, with probability 0 at every other action; one kept action is the
/// one-hot target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationKeepSetsV1 {
    pub schema: String,
    pub lineage: String,
    /// The bank's own `rows_sha256`.
    pub bank_rows_sha256: String,
    pub rows: Vec<ArchiveDistillationKeepRowV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationKeepRowV1 {
    pub game: String,
    pub step: u64,
    pub keep: Vec<u32>,
}

/// The phase-1 recipe values a run uses: the frozen ones, or a cell's.
struct RecipeV1 {
    learning_rate: f32,
    anchor_coefficient: f64,
    checkpoint_steps: Option<Vec<u64>>,
    cell: Option<ArchiveDistillationCellV1>,
    /// The batch-order seed the cell's label names ([`order_seed_v1`]); 0
    /// without a cell.
    order_seed: u64,
}

/// The prefix of SD1's seed labels, `SD1-T-s{k}`.
const SD1_SEED_PREFIX_V1: &str = "SD1-T-s";

/// The batch-order seed a cell label names: k when the whole label is
/// exactly `SD1-T-s{k}` with k from 0 to 7, otherwise 0. Every other label
/// reads as before, so no recorded cell changes meaning (CU1's `U-s32` and
/// `U-s256`, where s means support, included); [`recipe_v1`] refuses a
/// label with the `SD1-T-s` prefix that is not of that form. The seed
/// enters the per-epoch archive order and the per-pass anchor order,
/// appended only for k from 1 to 7 ([`archive_order_key_v1`],
/// [`anchor_order_key_v1`]); seed 0 keeps the unseeded hash inputs byte for
/// byte.
fn order_seed_v1(cell: &str) -> u64 {
    match cell.strip_prefix(SD1_SEED_PREFIX_V1).map(str::as_bytes) {
        Some(&[digit @ b'0'..=b'7']) => u64::from(digit - b'0'),
        _ => 0,
    }
}

fn recipe_v1(
    arm: ArchiveDistillationArmV1,
    options: Option<&ArchiveDistillationOptionsV1>,
) -> Result<RecipeV1, String> {
    let Some(options) = options else {
        return Ok(RecipeV1 {
            learning_rate: LEARNING_RATE,
            anchor_coefficient: ANCHOR_COEFFICIENT,
            checkpoint_steps: None,
            cell: None,
            order_seed: 0,
        });
    };
    ensure(
        arm == ArchiveDistillationArmV1::Uniform,
        "phase-1 options are for arm U only",
    )?;
    ensure(
        !options.cell.is_empty()
            && options.cell.len() <= 32
            && options
                .cell
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "a cell label is 1 to 32 ASCII letters, digits or '-'",
    )?;
    // A label with the seed prefix is exactly the label of the seed it
    // reads: a near miss (`SD1-T-s8`, `SD1-T-s07`) would read seed 0 and
    // repeat D1-0's batch orders under another label.
    let order_seed = order_seed_v1(&options.cell);
    ensure(
        !options.cell.starts_with(SD1_SEED_PREFIX_V1)
            || options.cell == format!("{SD1_SEED_PREFIX_V1}{order_seed}"),
        "a cell label starting with SD1-T-s is one of SD1-T-s0 to SD1-T-s7",
    )?;
    let learning_rate = options.learning_rate.unwrap_or(LEARNING_RATE);
    let anchor_coefficient = options.anchor_coefficient.unwrap_or(ANCHOR_COEFFICIENT);
    ensure(
        learning_rate.is_finite() && learning_rate > 0.0,
        "the step size is a positive finite number",
    )?;
    ensure(
        anchor_coefficient.is_finite() && anchor_coefficient > 0.0,
        "the anchor coefficient is a positive finite number",
    )?;
    ensure(
        [
            options.min_edge_backups.is_some(),
            options.count_control.is_some(),
            options.keep_sets.is_some(),
        ]
        .into_iter()
        .filter(|&rule| rule)
        .count()
            <= 1,
        "a cell keeps rows by one rule at most",
    )?;
    ensure(
        options.count_control.is_none_or(|n| n > 0),
        "a count control keeps at least one row",
    )?;
    if let Some(steps) = &options.checkpoint_steps {
        ensure(
            steps.first().is_some_and(|&s| s > 0)
                && steps.windows(2).all(|w| w[0] < w[1])
                && steps.last().is_some_and(|&s| s >= REPRODUCTION_STEP),
            "checkpoint steps are positive, strictly increasing and reach the reproduction step",
        )?;
    }
    Ok(RecipeV1 {
        learning_rate,
        anchor_coefficient,
        checkpoint_steps: options.checkpoint_steps.clone(),
        cell: Some(ArchiveDistillationCellV1 {
            cell: options.cell.clone(),
            anchor_coefficient_bits: anchor_coefficient.to_bits(),
            min_edge_backups: options.min_edge_backups,
            count_control: options.count_control.map(|n| n as u64),
            checkpoint_steps: options.checkpoint_steps.clone().unwrap_or_default(),
            keep_sets: options.keep_sets.clone(),
        }),
        order_seed,
    })
}

/// The arm's rows, then a cell's row rule: a support floor on the executed
/// edge's backups, a count control ranked by
/// `[U_COLLAPSE_NAMESPACE_V1, lineage, "count-control", n, game, step]`, or
/// a keep-set file's rows ([`keep_sets_v1`]). The result is in bank order;
/// the plan orders each epoch itself.
fn select_rows_v1(
    rows: &[(ArchiveRowV1, NativeFlatDecisionTensorV2)],
    arm: ArchiveDistillationArmV1,
    lineage: &str,
    options: Option<&ArchiveDistillationOptionsV1>,
    keep: Option<&BTreeMap<usize, Vec<u32>>>,
) -> Result<Vec<usize>, String> {
    ensure(
        keep.is_some() == options.is_some_and(|o| o.keep_sets.is_some()),
        "a keep-set cell reads its loaded keep sets",
    )?;
    let mut selected: Vec<usize> = (0..rows.len())
        .filter(|&i| arm.admits(&rows[i].0))
        .collect();
    if let Some(keep) = keep {
        selected.retain(|i| keep.contains_key(i));
        ensure(
            selected.len() == keep.len(),
            "a keep-set row is outside the arm",
        )?;
    }
    if let Some(floor) = options.and_then(|o| o.min_edge_backups) {
        selected.retain(|&i| rows[i].0.edge_backups.is_some_and(|b| b >= floor));
    }
    if let Some(n) = options.and_then(|o| o.count_control) {
        ensure(
            n <= selected.len(),
            "a count control keeps more rows than the arm has",
        )?;
        selected.sort_by_cached_key(|&i| {
            let row = &rows[i].0;
            (
                key_hash(json!([
                    U_COLLAPSE_NAMESPACE_V1,
                    lineage,
                    "count-control",
                    n,
                    row.game,
                    row.step
                ])),
                i,
            )
        });
        selected.truncate(n);
        selected.sort_unstable();
    }
    Ok(selected)
}

/// Reads a keep-set file against the reproduced bank: its lineage and the
/// bank's rows hash; every listed row one bank row, listed once; every
/// keep set non-empty, strictly increasing and inside the row's menu, with
/// positive parent probability. Returns each listed row's bank index and
/// its kept actions.
fn keep_sets_v1(
    pin: &PinnedFileV1,
    lineage: &str,
    bank_rows_sha256: &str,
    rows: &[(ArchiveRowV1, NativeFlatDecisionTensorV2)],
) -> Result<BTreeMap<usize, Vec<u32>>, String> {
    let file: ArchiveDistillationKeepSetsV1 = read_pinned(pin)?;
    ensure(
        file.schema == KEEP_SETS_SCHEMA_V1
            && file.lineage == lineage
            && file.bank_rows_sha256 == bank_rows_sha256,
        "keep-set file identity differs from the bank",
    )?;
    ensure(
        !file.rows.is_empty(),
        "a keep-set file lists at least one row",
    )?;
    let index: HashMap<(&str, u64), usize> = rows
        .iter()
        .enumerate()
        .map(|(i, (row, _))| ((row.game.as_str(), row.step), i))
        .collect();
    ensure(
        index.len() == rows.len(),
        "bank rows repeat a game and step",
    )?;
    let mut keep = BTreeMap::new();
    for listed in &file.rows {
        let &i = index
            .get(&(listed.game.as_str(), listed.step))
            .ok_or("a keep-set row is not a bank row")?;
        let row = &rows[i].0;
        ensure(
            !listed.keep.is_empty()
                && listed.keep.windows(2).all(|w| w[0] < w[1])
                && listed.keep.last().is_some_and(|&a| a < row.k),
            "a keep set is non-empty, strictly increasing and inside its menu",
        )?;
        let parent = line_b_softmax_v1(&f64_logits(&recorded_logits(&row.parent_logits)))
            .map_err(err)?
            .probabilities;
        ensure(
            listed.keep.iter().map(|&a| parent[a as usize]).sum::<f64>() > 0.0,
            "a keep set holds no parent probability",
        )?;
        ensure(
            keep.insert(i, listed.keep.clone()).is_none(),
            "a keep-set row is listed twice",
        )?;
    }
    Ok(keep)
}

/// A keep-set row's target: the binary64 softmax of the parent's raw logits
/// over the kept actions, built as the anchor's targets are, placed at
/// those actions with probability 0 and log-probability minus infinity at
/// every other action. One kept action gives the one-hot target exactly.
fn kept_target_v1(row: &ArchiveRowV1, keep: &[u32]) -> Result<LineBSoftmaxV1, String> {
    let logits = f64_logits(&recorded_logits(&row.parent_logits));
    let kept: Vec<f64> = keep.iter().map(|&a| logits[a as usize]).collect();
    let softmax = line_b_softmax_v1(&kept).map_err(err)?;
    let mut probabilities = vec![0.0; logits.len()];
    let mut log_probabilities = vec![f64::NEG_INFINITY; logits.len()];
    for (j, &a) in keep.iter().enumerate() {
        probabilities[a as usize] = softmax.probabilities[j];
        log_probabilities[a as usize] = softmax.log_probabilities[j];
    }
    Ok(LineBSoftmaxV1 {
        probabilities,
        log_probabilities,
        weight_sum: softmax.weight_sum,
    })
}

/// A keep-set cell's rows as its census records them: the kept actions,
/// the parent's probability on them, the target's mass on the menu's Spy
/// self-target and Dread-Return-on-Giant actions (null when the menu has
/// none) and the forward KL from the parent's softmax to the target.
fn keep_census_v1(
    rows: &[(ArchiveRowV1, NativeFlatDecisionTensorV2)],
    keep: &BTreeMap<usize, Vec<u32>>,
    targets: &[LineBSoftmaxV1],
) -> Result<Value, String> {
    let mut out = Vec::with_capacity(keep.len());
    for (&i, kept) in keep {
        let row = &rows[i].0;
        let logits = f64_logits(&recorded_logits(&row.parent_logits));
        let parent = line_b_softmax_v1(&logits).map_err(err)?.probabilities;
        let target = &targets[i];
        let mass = |actions: &[u32]| -> Option<f64> {
            (!actions.is_empty()).then(|| {
                actions
                    .iter()
                    .map(|&a| target.probabilities[a as usize])
                    .sum()
            })
        };
        let kl = line_b_divergence_v1(LineBDivergenceV1::Forward, &logits, target)
            .map_err(err)?
            .divergence;
        out.push(
            json!({"game":row.game,"step":row.step,"k":row.k,"keep":kept,
            "kept_parent_probability":kept.iter().map(|&a| parent[a as usize]).sum::<f64>(),
            "target_self_mass":mass(&row.self_actions),
            "target_giant_mass":mass(&row.giant_actions),
            "kl_target_from_parent":kl}),
        );
    }
    Ok(Value::Array(out))
}

/// The parent's own ordinary play, split by unit before collection (design
/// section 3): `training` feeds the anchor term, `held_out` the collapse
/// probe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationAnchorV1 {
    pub training: Vec<PinnedFileV1>,
    pub held_out: Vec<PinnedFileV1>,
}

/// The pinned inputs and recipe a phase-1 checkpoint came from. The loader
/// reads `parent` and requires its exact optimizer state
/// ([`validate_phase1_checkpoint_v1`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationProvenanceV1 {
    pub method: String,
    pub lineage: String,
    pub arm: ArchiveDistillationArmV1,
    /// The parent's checkpoint file.
    pub parent: PinnedFileV1,
    pub parent_state_sha256: String,
    pub games: PinnedFileV1,
    /// The raw archive files the bank names.
    pub archive_sources: Vec<PinnedFileV1>,
    pub bank: PinnedFileV1,
    pub bank_rows_sha256: String,
    /// SHA-256 of the anchor's two pinned trajectory lists, as JSON.
    pub anchor_sha256: String,
    pub steps: u64,
    pub learning_rate_bits: u32,
    /// The phase-1 trainer's own state at `steps` (the phase-1 parameters
    /// with its fresh-moment Adam), for audit against the phase-1 record.
    pub phase1_state_sha256: String,
    pub runtime_commit: String,
    /// The U collapse diagnostic's cell, when phase 1 ran with options;
    /// absent for the screen's frozen recipe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<ArchiveDistillationCellV1>,
    /// Omitted for historical full-network checkpoints.
    #[serde(
        default,
        skip_serializing_if = "ArchiveDistillationUpdateScopeV1::is_full_network_v1"
    )]
    pub update_scope: ArchiveDistillationUpdateScopeV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationCandidateV1 {
    pub id: String,
    pub source: ExpandedModelSourceV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationProbeBankV1 {
    /// `training` (the archive phase 1 read) or `completion` (held out).
    pub role: String,
    pub games: PinnedFileV1,
    pub bank: PinnedFileV1,
}

/// SD1's search-evidence keep-set files, both keyed to the probe's training
/// bank and listing disjoint games: `training` lists the training games'
/// evidence rows (the rows phase 1 trains), `held_out` the held-out games'
/// (never trained). At each listed row the kept actions are every action
/// the search did not separate from the executed one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDistillationProbeEvidenceV1 {
    pub training: PinnedFileV1,
    pub held_out: PinnedFileV1,
}

/// A phase-1 checkpoint loads only as itself: its identity and provenance
/// come together; its scalars are phase 1's (learning rate 1e-4, or a cell's
/// recorded one on arm U, and a zero value coefficient because phase 1 has
/// no value term); it has no GAE scalars or update inputs; and its named
/// parent holds exactly its optimizer state (moments, Adam step and gauge
/// anchor) and feature identity. Any other checkpoint carries no provenance.
pub(super) fn validate_phase1_checkpoint_v1(saved: &ExpandedCheckpointV1) -> Result<(), String> {
    let is_phase1 = saved.loss_identity == PHASE1_LOSS_IDENTITY_V1;
    ensure(
        is_phase1 == saved.archive_distillation.is_some(),
        "checkpoint archive-distillation identity differs",
    )?;
    let Some(provenance) = &saved.archive_distillation else {
        return Ok(());
    };
    let learning_rate_bits = match &provenance.cell {
        None => LEARNING_RATE.to_bits(),
        Some(_) => {
            ensure(
                provenance.arm == ArchiveDistillationArmV1::Uniform
                    && f32::from_bits(provenance.learning_rate_bits).is_finite()
                    && f32::from_bits(provenance.learning_rate_bits) > 0.0,
                "a phase-1 cell is arm U at a positive finite step size",
            )?;
            provenance.learning_rate_bits
        }
    };
    ensure(
        provenance.method == METHOD
            && provenance.learning_rate_bits == learning_rate_bits
            && saved.learning_rate_bits == learning_rate_bits
            && saved.value_coefficient_bits == 0.0_f32.to_bits()
            && saved.trajectories.is_empty()
            && saved.e_supported.is_none()
            && saved.exploration.is_none()
            && saved.gamma_bits.is_none()
            && saved.gae_lambda_bits.is_none()
            && saved.entropy_coefficient_bits.is_none()
            && saved.registry_transfer.is_none(),
        "phase-1 checkpoint fields differ from its identity",
    )?;
    let parent = read_ordinary_checkpoint_v1(&provenance.parent, &saved.source_import)?;
    ensure(
        parent.archive_distillation.is_none()
            && parent.loss_identity != PHASE1_LOSS_IDENTITY_V1
            && parent.state_sha256 == provenance.parent_state_sha256,
        "a phase-1 parent is the named ordinary checkpoint",
    )?;
    ensure(
        parent.schema == saved.schema
            && parent.feature_contract_digest == saved.feature_contract_digest
            && parent.feature_encoding_digest == saved.feature_encoding_digest
            && parent.card_db_hash == saved.card_db_hash
            && parent.source_import == saved.source_import,
        "phase-1 checkpoint identity differs from its parent's",
    )?;
    ensure(
        parent.adam_step == saved.adam_step
            && parent.scorer_bias_anchor_bits == saved.scorer_bias_anchor_bits
            && parent.first_moments == saved.first_moments
            && parent.second_moments == saved.second_moments,
        "phase-1 checkpoint does not hold its parent's exact optimizer state",
    )?;
    if provenance.update_scope == ArchiveDistillationUpdateScopeV1::ScorerOnly {
        ensure(
            saved.parameters.len() == parent.parameters.len()
                && saved
                    .parameters
                    .iter()
                    .zip(&parent.parameters)
                    .all(|(after, before)| {
                        matches!(
                            after.name.as_str(),
                            "scorer.0.weight" | "scorer.0.bias" | "scorer.2.weight"
                        ) || after == before
                    }),
            "scorer-only phase-1 checkpoint changed a frozen parameter",
        )?;
    }
    Ok(())
}

/// A deck label names Spy: the runtime deck id itself, or a nine-deck
/// registration label `Spy/<list SHA-256 prefix>`.
fn is_spy_label(label: &str) -> bool {
    label.split('/').next() == Some(SPY_DECK)
}

fn key_hash(parts: Value) -> [u8; 32] {
    Sha256::digest(serde_json::to_vec(&parts).expect("JSON key")).into()
}

fn runtime_binary_sha256() -> Result<String, String> {
    let binary = fs::read(std::env::current_exe().map_err(err)?).map_err(err)?;
    Ok(sha(&binary))
}

/// A live menu's Spy line flags for `action`, and its self-target and
/// Dread-Return-on-Giant actions, from the archive's own classifier.
fn line_flags_v1(
    session: &FastActorSessionV1,
    focal: u8,
    action: u32,
) -> (LineFlagsV1, Vec<u32>, Vec<u32>) {
    let menu = spy_menu_flags_v1(session, focal);
    let mut flags = LineFlagsV1::default();
    let (mut own_actions, mut giant_actions) = (Vec::new(), Vec::new());
    for (index, &(cast, own, giant)) in menu.iter().enumerate() {
        flags.cast_offered |= cast;
        flags.self_offered |= own;
        flags.giant_offered |= giant;
        if own {
            own_actions.push(index as u32);
        }
        if giant {
            giant_actions.push(index as u32);
        }
    }
    if let Some(&(cast, own, giant)) = menu.get(action as usize) {
        flags.cast_chosen = cast;
        flags.self_chosen = own;
        flags.giant_chosen = giant;
    }
    (flags, own_actions, giant_actions)
}

/// The parent's scores and training input at the live decision, from a
/// fresh collection fork with sampling reset to `[0, 0]` (E's adapter call).
fn capture_v1(
    parent: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
) -> Result<(FrozenPlayDecisionScoresV1, NativeFlatDecisionTensorV2), String> {
    let mut capture = parent.fork_for_collection_v3()?;
    capture.reset_sampling_v1([0, 0]);
    Ok(match parent.feature_identity_v1().generation {
        FreshLineageGenerationV1::V3 => {
            let (_, scores, tensor) = capture.select_with_training_tensor_v3(session)?;
            (scores, tensor.common)
        }
        FreshLineageGenerationV1::V4 => {
            let (_, scores, tensor) = capture.select_with_training_tensor_v4(session)?;
            (scores, tensor.common)
        }
    })
}

fn tensor_sha256_v1(tensor: &NativeFlatDecisionTensorV2) -> Result<String, String> {
    Ok(sha(
        &serde_json::to_vec(&TensorBitsV1::from_tensor(tensor)).map_err(err)?
    ))
}

/// One game's eligible rows with their tensors, after every binding check.
fn extract_game_v1(
    game: &ArchiveGameV1,
    parent: &FrozenPlayPolicyV1,
) -> Result<Vec<(ArchiveRowV1, NativeFlatDecisionTensorV2)>, String> {
    let episode = &game.episode;
    ensure(
        episode.reset.is_none() && episode.opponent.is_none(),
        "an archive game starts from its opening with no recorded opponent",
    )?;
    ensure(
        !game.actions.is_empty(),
        "an archive game records its actions",
    )?;
    let configs = episode.configurations()?;
    let mut session = episode_session_v1(episode, &configs)?;
    let focal = episode.learner_seat;
    let mut starts = game.starts.iter();
    let mut decisions = game.decisions.iter();
    let mut rows = Vec::new();
    let length = game.actions.len() as u64;
    for (index, &action) in game.actions.iter().enumerate() {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            return Err("archive replay reaches a terminal before its last action".into());
        };
        ensure(d.step == index as u64, "archive replay step differs")?;
        ensure(
            action < d.legal_action_count,
            "archive action outside the legal menu",
        )?;
        if seat(d.acting_player) == focal {
            if d.substep_index == 0 {
                let start = starts
                    .next()
                    .ok_or("archive replay reaches an unrecorded focal start")?;
                let (observation, menu) = decision_replay_key(&session, d)?;
                ensure(
                    start.step == d.step
                        && start.physical_decision_id == d.physical_decision_id
                        && start.k == d.legal_action_count
                        && start.obs_hash == observation
                        && start.menu_hash == menu
                        && start.state_hash == format!("{:016x}", session.diagnostic_state_hash()),
                    "archive focal start differs from its recorded observation, menu or state",
                )?;
            }
            if d.legal_action_count >= 2 {
                let record = decisions
                    .next()
                    .ok_or("archive replay reaches an unrecorded focal choice")?;
                ensure(
                    record.step == d.step
                        && record.physical_decision_id == d.physical_decision_id
                        && record.substep == d.substep_index
                        && record.k == d.legal_action_count,
                    "archive focal choice differs from its record",
                )?;
                let (line, self_actions, giant_actions) = line_flags_v1(&session, focal, action);
                ensure(
                    line == LineFlagsV1::of_record(record),
                    "archive Spy flags differ from the live menu",
                )?;
                match record.chooser.as_str() {
                    "new_root" | "reused" => {
                        let (scores, tensor) = capture_v1(parent, &session)?;
                        ensure(
                            scores.logits.len() == d.legal_action_count as usize
                                && scores.logits.iter().all(|z| z.is_finite()),
                            "parent scores differ from the archive menu",
                        )?;
                        rows.push((
                            ArchiveRowV1 {
                                game: game.id.clone(),
                                step: d.step,
                                physical_decision_id: d.physical_decision_id,
                                substep: d.substep_index,
                                k: d.legal_action_count,
                                target: action,
                                chooser: record.chooser.clone(),
                                edge_backups: record.edge_backups,
                                game_quartile: ((4 * d.step) / length).min(3) as u8,
                                line,
                                self_actions,
                                giant_actions,
                                parent_logits: bits(&scores.logits),
                                parent_value: scores.value.to_bits(),
                                tensor_sha256: tensor_sha256_v1(&tensor)?,
                            },
                            tensor,
                        ));
                    }
                    "plain" => {}
                    _ => return Err("unknown archive chooser".into()),
                }
            } else {
                ensure(action == 0, "a forced archive decision takes action 0")?;
            }
        }
        session.step(d.episode_id, d.step, action).map_err(err)?;
    }
    ensure(
        starts.next().is_none() && decisions.next().is_none(),
        "archive records outlast the replay",
    )?;
    let FastActorResponseV1::Terminal(terminal) = session.current_response() else {
        return Err("archive replay does not end at its last action".into());
    };
    ensure(
        terminal.terminal_classification == TerminalClassificationV1::Natural,
        "archive replay ends unnaturally",
    )?;
    let end = match terminal.winner {
        None => "draw",
        Some(winner) if seat(winner) == focal => "win",
        Some(_) => "loss",
    };
    ensure(
        end == game.end,
        "archive replay end differs from its record",
    )?;
    Ok(rows)
}

/// A loaded parent and the games of one lineage, in file order.
struct ArchiveInputsV1 {
    policy: FrozenPlayPolicyV1,
    state: NativePolicyValueTrainStateV1,
    state_sha256: String,
    games: Vec<ArchiveGameV1>,
}

/// The parent (an ordinary checkpoint, never a phase-1 one) and one
/// lineage's games, each id once.
fn load_inputs_v1(
    parent: &ExpandedModelSourceV1,
    lineage: &str,
    games: &PinnedFileV1,
) -> Result<ArchiveInputsV1, String> {
    let pin = parent
        .checkpoint
        .as_ref()
        .ok_or("archive distillation starts from the parent's checkpoint")?;
    let (policy, state) = initialize(parent)?;
    let saved = read_ordinary_checkpoint_v1(pin, policy.identity_v1())?;
    let state_sha256 = hex(&state.state_sha256_v1().map_err(err)?);
    ensure(
        saved.e_supported.is_none()
            && saved.exploration.is_none()
            && saved.archive_distillation.is_none()
            && saved.state_sha256 == state_sha256,
        "archive distillation starts from an ordinary parent checkpoint",
    )?;
    let text = String::from_utf8(read_pinned_bytes(games)?).map_err(err)?;
    let mut ids = BTreeSet::new();
    let mut parsed = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let game: ArchiveGameV1 = serde_json::from_str(line).map_err(err)?;
        ensure(game.lineage == lineage, "archive game from another lineage")?;
        ensure(ids.insert(game.id.clone()), "duplicate archive game id")?;
        parsed.push(game);
    }
    ensure(!parsed.is_empty(), "no archive games")?;
    Ok(ArchiveInputsV1 {
        policy,
        state,
        state_sha256,
        games: parsed,
    })
}

type ExtractedRowsV1 = Vec<(ArchiveRowV1, NativeFlatDecisionTensorV2)>;

fn extract_all_v1(inputs: &ArchiveInputsV1) -> (Vec<ArchiveGameStatusV1>, ExtractedRowsV1) {
    let mut status = Vec::with_capacity(inputs.games.len());
    let mut rows = Vec::new();
    for game in &inputs.games {
        match extract_game_v1(game, &inputs.policy) {
            Ok(game_rows) => {
                status.push(ArchiveGameStatusV1 {
                    id: game.id.clone(),
                    kept: true,
                    reason: None,
                    rows: game_rows.len(),
                });
                rows.extend(game_rows);
            }
            Err(reason) => status.push(ArchiveGameStatusV1 {
                id: game.id.clone(),
                kept: false,
                reason: Some(reason),
                rows: 0,
            }),
        }
    }
    (status, rows)
}

fn rows_sha256_v1(rows: &[ArchiveRowV1]) -> Result<String, String> {
    Ok(sha(&serde_json::to_vec(rows).map_err(err)?))
}

/// A row census (design section 4), from the rows alone.
fn bank_census_v1(game_status: &[ArchiveGameStatusV1], rows: &[ArchiveRowV1]) -> Value {
    let count = |f: &dyn Fn(&ArchiveRowV1) -> bool| rows.iter().filter(|r| f(r)).count();
    let share = |part: usize, whole: usize| (whole > 0).then(|| part as f64 / whole as f64);
    let disagreement = count(&|r| top_v1(&recorded_logits(&r.parent_logits)) != r.target as usize);
    let mut by_chooser = std::collections::BTreeMap::<&str, usize>::new();
    let mut by_quartile = [0usize; 4];
    let mut by_width = std::collections::BTreeMap::<&str, usize>::new();
    let mut by_support = std::collections::BTreeMap::<&str, usize>::new();
    for row in rows {
        *by_chooser.entry(row.chooser.as_str()).or_default() += 1;
        by_quartile[usize::from(row.game_quartile)] += 1;
        let width = match row.k {
            0..=2 => "2",
            3..=4 => "3-4",
            5..=8 => "5-8",
            9..=16 => "9-16",
            _ => "17+",
        };
        *by_width.entry(width).or_default() += 1;
        let support = match row.edge_backups {
            None => "unrecorded",
            Some(0..=31) => "8-31",
            Some(32..=255) => "32-255",
            Some(256..=4095) => "256-4095",
            Some(_) => "4096+",
        };
        *by_support.entry(support).or_default() += 1;
    }
    let kept = game_status.iter().filter(|g| g.kept).count();
    json!({"games_kept":kept,"games_dropped":game_status.len() - kept,"rows":rows.len(),
        "by_chooser":by_chooser,"by_game_quartile":by_quartile,"by_width":by_width,
        "by_support":by_support,
        "self_chosen":count(&|r| r.line.self_chosen),
        "self_declined":count(&|r| r.line.self_offered && !r.line.self_chosen),
        "giant_chosen":count(&|r| r.line.giant_chosen),
        "giant_declined":count(&|r| r.line.giant_offered && !r.line.giant_chosen),
        "self_chosen_share":share(count(&|r| r.line.self_chosen), count(&|r| r.line.self_offered)),
        "giant_chosen_share":share(count(&|r| r.line.giant_chosen), count(&|r| r.line.giant_offered)),
        "parent_disagreement":(!rows.is_empty()).then(|| disagreement as f64 / rows.len() as f64)})
}

/// `ArchiveDistillationBank`: extraction with every binding check. The
/// census is written beside the bank; a sealed set's caller keeps it unread
/// until its freeze.
pub(super) fn bank_v1(
    parent: ExpandedModelSourceV1,
    lineage: String,
    games: PinnedFileV1,
    sources: Vec<PinnedFileV1>,
    output_directory: PathBuf,
) -> Result<Value, String> {
    ensure(
        !output_directory.exists(),
        "bank output directory must be fresh",
    )?;
    ensure(
        !sources.is_empty() && sources.len() <= 4096,
        "invalid archive source count",
    )?;
    let mut paths = BTreeSet::new();
    for source in &sources {
        ensure(
            paths.insert(source.path.clone()),
            "an archive source appears twice",
        )?;
        read_pinned_bytes(source)?;
    }
    let inputs = load_inputs_v1(&parent, &lineage, &games)?;
    let (game_status, extracted) = extract_all_v1(&inputs);
    let rows: Vec<ArchiveRowV1> = extracted.into_iter().map(|(row, _)| row).collect();
    let census = bank_census_v1(&game_status, &rows);
    let bank = ArchiveBankV1 {
        schema: BANK_SCHEMA.into(),
        namespace: ARCHIVE_DISTILLATION_NAMESPACE_V1.into(),
        lineage,
        parent,
        parent_state_sha256: inputs.state_sha256,
        games,
        sources,
        runtime_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").into(),
        runtime_tree: env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256").into(),
        runtime_binary_sha256: runtime_binary_sha256()?,
        rows_sha256: rows_sha256_v1(&rows)?,
        game_status,
        rows,
    };
    fs::create_dir_all(&output_directory).map_err(err)?;
    let pin = publish_json(&output_directory, "bank.json", &bank)?;
    let census_pin = publish_json(&output_directory, "census.json", &census)?;
    Ok(json!({"schema":BANK_SCHEMA,"bank":pin,"census":census_pin,
        "rows_sha256":bank.rows_sha256}))
}

/// Re-extracts a pinned bank's games and requires the same rows bit for bit.
fn reproduce_bank_v1(
    inputs: &ArchiveInputsV1,
    parent: &ExpandedModelSourceV1,
    lineage: &str,
    games: &PinnedFileV1,
    bank_pin: &PinnedFileV1,
) -> Result<(ArchiveBankV1, ExtractedRowsV1), String> {
    let bank: ArchiveBankV1 = read_pinned(bank_pin)?;
    ensure(
        bank.schema == BANK_SCHEMA
            && bank.namespace == ARCHIVE_DISTILLATION_NAMESPACE_V1
            && bank.lineage == lineage
            && bank.parent == *parent
            && bank.parent_state_sha256 == inputs.state_sha256
            && bank.games == *games
            && bank.rows_sha256 == rows_sha256_v1(&bank.rows)?,
        "archive bank identity differs",
    )?;
    let (status, extracted) = extract_all_v1(inputs);
    ensure(
        status == bank.game_status
            && extracted.len() == bank.rows.len()
            && extracted
                .iter()
                .zip(&bank.rows)
                .all(|((row, _), recorded)| row == recorded),
        "archive rows differ from the pinned bank",
    )?;
    Ok((bank, extracted))
}

/// One learner row of the parent's own ordinary play.
struct AnchorRowV1 {
    episode: String,
    step: u64,
    learner_deck: String,
    logits: Vec<u32>,
    value: u32,
    tensor: NativeFlatDecisionTensorV2,
}

/// Reads one pinned trajectory of the parent's own ordinary play: a valid
/// trajectory of this generation whose learner is the parent's exact state,
/// with the legacy sampler, no reset, and an episode id not seen before.
fn read_parent_trajectory_v1(
    pin: &PinnedFileV1,
    policy: &FrozenPlayPolicyV1,
    parent_state_sha256: &str,
    ids: &mut BTreeSet<String>,
) -> Result<ExpandedTrajectoryV1, String> {
    let trajectory: ExpandedTrajectoryV1 = read_pinned(pin)?;
    validate_trajectory(&trajectory)?;
    ensure(
        trajectory_generation_v1(&trajectory) == policy.feature_identity_v1().generation
            && trajectory.source_import == *policy.identity_v1()
            && trajectory.behavior_state_sha256 == parent_state_sha256
            && trajectory.learner_sampler.is_none()
            && trajectory.episode.reset.is_none(),
        "trajectory is not the parent's own ordinary play",
    )?;
    ensure(
        ids.insert(trajectory.episode.id.clone()),
        "an episode appears twice across the anchor and offer sets",
    )?;
    Ok(trajectory)
}

/// The learner rows with two or more legal actions, in step order.
fn anchor_rows_v1(trajectory: &ExpandedTrajectoryV1) -> Vec<AnchorRowV1> {
    let seat = trajectory.episode.learner_seat;
    let deck = trajectory.episode.selected[usize::from(seat)].label.clone();
    trajectory
        .decisions
        .iter()
        .filter(|row| row.actor == seat && row.logits.len() >= 2)
        .map(|row| AnchorRowV1 {
            episode: trajectory.episode.id.clone(),
            step: row.step,
            learner_deck: deck.clone(),
            logits: row.logits.clone(),
            value: row.value,
            tensor: row.tensor.tensor(),
        })
        .collect()
}

/// One split's rows, each trajectory read, reduced and dropped in turn.
fn load_anchor_split_v1(
    pins: &[PinnedFileV1],
    policy: &FrozenPlayPolicyV1,
    parent_state_sha256: &str,
    ids: &mut BTreeSet<String>,
) -> Result<Vec<AnchorRowV1>, String> {
    ensure(
        !pins.is_empty() && pins.len() <= MAX_TRAJECTORIES,
        "invalid anchor trajectory count",
    )?;
    let mut rows = Vec::new();
    for pin in pins {
        let trajectory = read_parent_trajectory_v1(pin, policy, parent_state_sha256, ids)?;
        rows.extend(anchor_rows_v1(&trajectory));
    }
    Ok(rows)
}

/// The exact anchor targets: `line_b_softmax_v1` in binary64 of each row's
/// recorded parent logit bits, rebuilt in memory, never stored and never the
/// deployed (quantized, clamped) probabilities (design section 3).
fn anchor_targets_v1(rows: &[AnchorRowV1]) -> Result<Vec<LineBSoftmaxV1>, String> {
    rows.iter()
        .map(|row| line_b_softmax_v1(&f64_logits(&recorded_logits(&row.logits))).map_err(err))
        .collect()
}

/// A held-out learner Spy menu offering the self-target.
struct OfferRowV1 {
    episode: String,
    step: u64,
    logits: Vec<u32>,
    value: u32,
    tensor: NativeFlatDecisionTensorV2,
    self_actions: Vec<usize>,
}

/// Replays one offer-set episode (the parent's own Spy play) and lists its
/// learner menus offering the self-target, by the archive's classifier on
/// the live menu.
fn offers_v1(trajectory: &ExpandedTrajectoryV1) -> Result<Vec<OfferRowV1>, String> {
    let episode = &trajectory.episode;
    let learner = episode.learner_seat;
    ensure(
        is_spy_label(&episode.selected[usize::from(learner)].label),
        "an offer-set episode plays Spy as the learner's deck",
    )?;
    let mut session = episode_session_v1(episode, &episode.configurations()?)?;
    let mut offers = Vec::new();
    for row in &trajectory.decisions {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            return Err("offer replay ends before its recorded row".into());
        };
        ensure(
            d.step == row.step
                && d.physical_decision_id == row.physical_decision_id
                && d.substep_index == row.substep_index
                && seat(d.acting_player) == row.actor
                && d.legal_action_count as usize == row.logits.len(),
            "offer replay row differs from its record",
        )?;
        if row.actor == learner && row.logits.len() >= 2 {
            let self_actions: Vec<usize> = spy_menu_flags_v1(&session, learner)
                .iter()
                .enumerate()
                .filter(|(_, flags)| flags.1)
                .map(|(action, _)| action)
                .collect();
            if !self_actions.is_empty() {
                offers.push(OfferRowV1 {
                    episode: episode.id.clone(),
                    step: row.step,
                    logits: row.logits.clone(),
                    value: row.value,
                    tensor: row.tensor.tensor(),
                    self_actions,
                });
            }
        }
        session
            .step(d.episode_id, d.step, row.selected)
            .map_err(err)?;
    }
    ensure(
        matches!(session.current_response(), FastActorResponseV1::Terminal(t) if t == trajectory.terminal),
        "offer replay terminal differs from its record",
    )?;
    Ok(offers)
}

fn view_v1(
    tensor: &NativeFlatDecisionTensorV2,
    generation: FreshLineageGenerationV1,
) -> crate::native_policy_value_net_v1::NativeEncodedDecisionViewV1<'_> {
    encoded_decision_view_generic_v1(tensor, generation)
}

fn forward_v1(
    model: &NativePolicyValueNetV1,
    tensor: &NativeFlatDecisionTensorV2,
    generation: FreshLineageGenerationV1,
) -> Result<(Vec<f32>, f32), String> {
    let view = view_v1(tensor, generation);
    let output = match generation {
        FreshLineageGenerationV1::V3 => model.forward_feature_transfer_v3(view),
        FreshLineageGenerationV1::V4 => model.forward_feature_transfer_v4(view),
    }
    .map_err(err)?;
    Ok((output.logits, output.value))
}

fn f64_logits(logits: &[f32]) -> Vec<f64> {
    logits.iter().map(|&z| f64::from(z)).collect()
}

fn recorded_logits(bits: &[u32]) -> Vec<f32> {
    bits.iter().map(|&b| f32::from_bits(b)).collect()
}

/// The training forward reproduces every recorded parent output bit for bit.
fn verify_parent_outputs_v1<'a>(
    model: &NativePolicyValueNetV1,
    generation: FreshLineageGenerationV1,
    rows: impl Iterator<Item = (&'a NativeFlatDecisionTensorV2, &'a [u32], u32)>,
) -> Result<usize, String> {
    let mut count = 0;
    for (tensor, logits, value) in rows {
        let (actual, actual_value) = forward_v1(model, tensor, generation)?;
        ensure(
            bits(&actual) == logits && actual_value.to_bits() == value,
            "the parent's forward differs from a recorded parent output",
        )?;
        count += 1;
    }
    Ok(count)
}

/// The anchor alone at the parent (design section 5, pre-flight): one step
/// over every anchor-training row, with the archive term omitted, on a copy
/// of the fresh phase-1 trainer, must leave every parameter bit unchanged.
/// Fresh moments matter: on the parent's own moments, momentum would move
/// parameters even at a zero gradient.
fn anchor_identity_preflight_v1(
    fresh: &NativePolicyValueTrainStateV1,
    generation: FreshLineageGenerationV1,
    rows: &[AnchorRowV1],
    targets: &[LineBSoftmaxV1],
    recipe: &RecipeV1,
    update_scope: ArchiveDistillationUpdateScopeV1,
) -> Result<usize, String> {
    ensure(
        fresh.adam_step_v1() == 0,
        "the anchor pre-flight runs on the fresh phase-1 optimizer",
    )?;
    let mut copy = fresh.clone();
    let before = copy.model_v1().parameter_snapshot_v1();
    let input = ExternalAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: recipe.anchor_coefficient,
        denominator: rows.len(),
        rows: rows
            .iter()
            .zip(targets)
            .map(|(row, target)| ExternalAuxiliaryRowV1 {
                encoded: view_v1(&row.tensor, generation),
                target,
            })
            .collect(),
    };
    let result = match update_scope {
        ArchiveDistillationUpdateScopeV1::FullNetwork => {
            copy.train_step_auxiliary_only_v1(generation, recipe.learning_rate, &[&input])
        }
        ArchiveDistillationUpdateScopeV1::ScorerOnly => copy.train_step_auxiliary_only_scoped_v1(
            generation,
            recipe.learning_rate,
            &[&input],
            update_scope,
        ),
    };
    result.map_err(|e| format!("anchor pre-flight step: {e:?}"))?;
    let after = copy.model_v1().parameter_snapshot_v1();
    ensure(
        same_parameter_bits(&before, &after),
        "the anchor alone moves the parent: its targets are not exact",
    )?;
    Ok(rows.len())
}

/// One phase-1 step: archive row indices, the archive term's denominator,
/// and anchor row indices.
struct PlannedStepV1 {
    archive: Vec<usize>,
    denominator: usize,
    anchor: Vec<usize>,
}

struct PlanV1 {
    steps: Vec<PlannedStepV1>,
    checkpoints: Vec<u64>,
    epoch_steps: u64,
}

/// One archive row's hash input in an epoch's order; a seed from 1 to 7 is
/// appended ([`order_seed_v1`]).
fn archive_order_key_v1(
    lineage: &str,
    arm: ArchiveDistillationArmV1,
    epoch: u64,
    game: &str,
    step: u64,
    seed: u64,
) -> Value {
    let mut key = json!([
        ARCHIVE_DISTILLATION_NAMESPACE_V1,
        "archive-order",
        lineage,
        arm.label(),
        epoch,
        game,
        step
    ]);
    if seed > 0 {
        key.as_array_mut().expect("a key array").push(json!(seed));
    }
    key
}

/// One anchor row's hash input in a pass's order; a seed from 1 to 7 is
/// appended ([`order_seed_v1`]).
fn anchor_order_key_v1(lineage: &str, pass: u64, episode: &str, step: u64, seed: u64) -> Value {
    let mut key = json!([
        ARCHIVE_DISTILLATION_NAMESPACE_V1,
        "anchor-order",
        lineage,
        pass,
        episode,
        step
    ]);
    if seed > 0 {
        key.as_array_mut().expect("a key array").push(json!(seed));
    }
    key
}

fn archive_order_v1(
    rows: &[(ArchiveRowV1, NativeFlatDecisionTensorV2)],
    selected: &[usize],
    lineage: &str,
    arm: ArchiveDistillationArmV1,
    epoch: u64,
    seed: u64,
) -> Vec<usize> {
    let mut order = selected.to_vec();
    order.sort_by_cached_key(|&i| {
        let row = &rows[i].0;
        (
            key_hash(archive_order_key_v1(
                lineage, arm, epoch, &row.game, row.step, seed,
            )),
            i,
        )
    });
    order
}

/// The frozen schedule. U: epochs of `ceil(n / 32)` steps, rows without
/// replacement in a per-epoch hash order cut into that many consecutive
/// batches whose sizes differ by at most one (none above 32); checkpoints
/// after 1, 2, 4, 8, 16 and 32 epochs. L: every row each step when it has 32
/// or fewer, otherwise U's epochs; checkpoints after 32 to 1,024 steps. Each
/// step's archive term is the mean over its own batch, so it keeps
/// coefficient 1 at every step and no row's weight depends on where an epoch
/// ends. The anchor takes the next 256 training rows of a per-pass hash order
/// every step, shared by both arms. A cell's order seed ([`order_seed_v1`])
/// enters both hash orders; seed 0 is the unseeded plan.
fn plan_v1(
    rows: &[(ArchiveRowV1, NativeFlatDecisionTensorV2)],
    selected: &[usize],
    anchor: &[AnchorRowV1],
    lineage: &str,
    arm: ArchiveDistillationArmV1,
    checkpoint_steps: Option<&[u64]>,
    order_seed: u64,
) -> Result<PlanV1, String> {
    let n = selected.len();
    ensure(n > 0, "the arm has no archive rows")?;
    ensure(
        anchor.len() >= ANCHOR_BATCH,
        "anchor training split is smaller than one batch",
    )?;
    let epoch_steps = n.div_ceil(ARCHIVE_BATCH) as u64;
    let (checkpoints, all_rows_each_step): (Vec<u64>, bool) = match (arm, checkpoint_steps) {
        // A cell's own steps (arm U only), still drawn in U's epochs.
        (ArchiveDistillationArmV1::Uniform, Some(steps)) => (steps.to_vec(), false),
        (ArchiveDistillationArmV1::Uniform, None) => (
            UNIFORM_EPOCHS.iter().map(|e| e * epoch_steps).collect(),
            false,
        ),
        (ArchiveDistillationArmV1::LineDecisions, None) => {
            (LINE_STEPS.to_vec(), n <= ARCHIVE_BATCH)
        }
        (ArchiveDistillationArmV1::LineDecisions, Some(_)) => {
            return Err("phase-1 options are for arm U only".into())
        }
    };
    ensure(
        checkpoints.first().is_some_and(|&s| s > 0) && checkpoints.windows(2).all(|w| w[0] < w[1]),
        "checkpoint steps are positive and strictly increasing",
    )?;
    let total = *checkpoints.last().expect("at least one checkpoint");
    let mut archive: Vec<(Vec<usize>, usize)> = Vec::with_capacity(total as usize);
    let mut epoch = 0;
    while (archive.len() as u64) < total {
        if all_rows_each_step {
            archive.push((selected.to_vec(), n));
            continue;
        }
        let order = archive_order_v1(rows, selected, lineage, arm, epoch, order_seed);
        let batches = n.div_ceil(ARCHIVE_BATCH);
        for b in 0..batches {
            if archive.len() as u64 == total {
                break;
            }
            let batch = order[b * n / batches..(b + 1) * n / batches].to_vec();
            let denominator = batch.len();
            archive.push((batch, denominator));
        }
        epoch += 1;
    }
    let mut anchor_rows = Vec::with_capacity(total as usize);
    let mut pass = 0u64;
    let mut order: Vec<usize> = Vec::new();
    let mut cursor = 0;
    for _ in 0..total {
        let mut batch = Vec::with_capacity(ANCHOR_BATCH);
        while batch.len() < ANCHOR_BATCH {
            if cursor == order.len() {
                order = (0..anchor.len()).collect();
                order.sort_by_cached_key(|&i| {
                    let row = &anchor[i];
                    (
                        key_hash(anchor_order_key_v1(
                            lineage,
                            pass,
                            &row.episode,
                            row.step,
                            order_seed,
                        )),
                        i,
                    )
                });
                pass += 1;
                cursor = 0;
            }
            batch.push(order[cursor]);
            cursor += 1;
        }
        anchor_rows.push(batch);
    }
    Ok(PlanV1 {
        steps: archive
            .into_iter()
            .zip(anchor_rows)
            .map(|((archive, denominator), anchor)| PlannedStepV1 {
                archive,
                denominator,
                anchor,
            })
            .collect(),
        checkpoints,
        epoch_steps,
    })
}

/// Exposure per checkpoint (design section 4): draws per archive and anchor
/// row, and the summed archive-term weight on tree-chosen and on all line
/// rows.
fn exposure_v1(
    plan: &PlanV1,
    rows: &[(ArchiveRowV1, NativeFlatDecisionTensorV2)],
    selected: &[usize],
    anchor_rows: usize,
) -> Value {
    let mut draws = vec![0u64; rows.len()];
    let mut anchor_draws = vec![0u64; anchor_rows];
    let (mut chosen_weight, mut line_weight) = (0.0_f64, 0.0_f64);
    let mut out = Vec::new();
    let mut next = plan.checkpoints.iter().peekable();
    for (index, step) in plan.steps.iter().enumerate() {
        for &i in &step.archive {
            draws[i] += 1;
            let line = rows[i].0.line;
            let weight = ARCHIVE_COEFFICIENT / step.denominator as f64;
            if line.line_chosen() {
                chosen_weight += weight;
            }
            if line.line_offered() {
                line_weight += weight;
            }
        }
        for &i in &step.anchor {
            anchor_draws[i] += 1;
        }
        if next.peek() == Some(&&(index as u64 + 1)) {
            next.next();
            let counts: Vec<u64> = selected.iter().map(|&i| draws[i]).collect();
            out.push(json!({"step":index + 1,
                "epochs":(index as u64 + 1) as f64 / plan.epoch_steps as f64,
                "draws_per_row_min":counts.iter().min(),"draws_per_row_max":counts.iter().max(),
                "anchor_draws_per_row_min":anchor_draws.iter().min(),
                "anchor_draws_per_row_max":anchor_draws.iter().max(),
                "line_chosen_weight":chosen_weight,"line_offered_weight":line_weight}));
        }
    }
    Value::Array(out)
}

/// The phase-1 checkpoint: the parent's checkpoint with the phase-1
/// parameters, a recomputed state hash, the phase-1 identity and scalars,
/// no GAE scalars, no update inputs and the typed provenance. Moments, Adam
/// step and gauge anchor stay the parent's own, which the loader checks.
fn checkpoint_v1(
    parent: &ExpandedCheckpointV1,
    parameters: &[NativeNamedParameterV1],
    provenance: ArchiveDistillationProvenanceV1,
) -> Result<ExpandedCheckpointV1, String> {
    let snapshot = NativePolicyValueTrainSnapshotV1 {
        adam_step: parent.adam_step,
        scorer_bias_anchor_bits: parent.scorer_bias_anchor_bits,
        parameters: parameters.to_vec(),
        first_moments: restore_parameters(&parent.first_moments, parameters)?,
        second_moments: restore_parameters(&parent.second_moments, parameters)?,
    };
    let mut checkpoint = parent.clone();
    checkpoint.state_sha256 = hex(&snapshot.state_sha256_v1().map_err(err)?);
    checkpoint.parameters = parameters
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    checkpoint.trajectories = Vec::new();
    checkpoint.e_supported = None;
    checkpoint.loss_identity = PHASE1_LOSS_IDENTITY_V1.into();
    checkpoint.gamma_bits = None;
    checkpoint.gae_lambda_bits = None;
    checkpoint.entropy_coefficient_bits = None;
    checkpoint.exploration = None;
    checkpoint.learning_rate_bits = provenance.learning_rate_bits;
    // Phase 1 has no value term, so its value coefficient is zero.
    checkpoint.value_coefficient_bits = 0.0_f32.to_bits();
    checkpoint.archive_distillation = Some(provenance);
    Ok(checkpoint)
}

fn same_parameter_bits(a: &[NativeNamedParameterV1], b: &[NativeNamedParameterV1]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.name == b.name && a.shape == b.shape && bits(&a.values) == bits(&b.values)
        })
}

/// Writes one phase-1 checkpoint and its model source. The written
/// checkpoint must load through the ordinary loader, whose ancestry check
/// reads the parent, with exactly these parameters and the parent's Adam
/// state. Returns the checkpoint and source pins and the state hash.
fn write_checkpoint_v1(
    output_directory: &Path,
    parent: &ExpandedModelSourceV1,
    parent_checkpoint: &ExpandedCheckpointV1,
    parameters: &[NativeNamedParameterV1],
    provenance: ArchiveDistillationProvenanceV1,
) -> Result<(PinnedFileV1, PinnedFileV1, String), String> {
    let steps = provenance.steps;
    let checkpoint = checkpoint_v1(parent_checkpoint, parameters, provenance)?;
    let checkpoint_pin = publish_json(
        output_directory,
        &format!("checkpoint-step-{steps}.json"),
        &checkpoint,
    )?;
    let source = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint_pin.clone()),
        ..parent.clone()
    };
    let (_, loaded) = initialize(&source)?;
    ensure(
        same_parameter_bits(&loaded.snapshot_v1().map_err(err)?.parameters, parameters)
            && loaded.adam_step_v1() == parent_checkpoint.adam_step
            && hex(&loaded.state_sha256_v1().map_err(err)?) == checkpoint.state_sha256,
        "a phase-1 checkpoint does not reload exactly",
    )?;
    let source_pin = publish_json(
        output_directory,
        &format!("source-step-{steps}.json"),
        &source,
    )?;
    Ok((checkpoint_pin, source_pin, checkpoint.state_sha256))
}

/// `ArchiveDistillationPhase1`.
#[allow(clippy::too_many_arguments)]
pub(super) fn phase1_v1(
    parent: ExpandedModelSourceV1,
    lineage: String,
    arm: ArchiveDistillationArmV1,
    games: PinnedFileV1,
    bank: PinnedFileV1,
    anchor: ArchiveDistillationAnchorV1,
    stop_after_steps: Option<u64>,
    dry_run: bool,
    options: Option<ArchiveDistillationOptionsV1>,
    update_scope: ArchiveDistillationUpdateScopeV1,
    output_directory: PathBuf,
) -> Result<Value, String> {
    let started = std::time::Instant::now();
    ensure(
        !output_directory.exists(),
        "phase-1 output directory must be fresh",
    )?;
    let recipe_values = recipe_v1(arm, options.as_ref())?;
    ensure(
        stop_after_steps.is_none_or(|n| n == REPRODUCTION_STEP),
        "a reproduction run stops after the reproduction step",
    )?;
    let inputs = load_inputs_v1(&parent, &lineage, &games)?;
    let generation = inputs.policy.feature_identity_v1().generation;
    let parent_pin = parent.checkpoint.clone().expect("checked by load");
    let parent_checkpoint = read_ordinary_checkpoint_v1(&parent_pin, inputs.policy.identity_v1())?;
    let (bank_record, rows) = reproduce_bank_v1(&inputs, &parent, &lineage, &games, &bank)?;
    let keep = options
        .as_ref()
        .and_then(|o| o.keep_sets.as_ref())
        .map(|pin| keep_sets_v1(pin, &lineage, &bank_record.rows_sha256, &rows))
        .transpose()?;
    let selected = select_rows_v1(&rows, arm, &lineage, options.as_ref(), keep.as_ref())?;
    let mut ids = BTreeSet::new();
    let anchor_train = load_anchor_split_v1(
        &anchor.training,
        &inputs.policy,
        &inputs.state_sha256,
        &mut ids,
    )?;
    let held_out_rows = load_anchor_split_v1(
        &anchor.held_out,
        &inputs.policy,
        &inputs.state_sha256,
        &mut ids,
    )?
    .len();
    let parent_model = inputs.state.model_v1();
    let verified = verify_parent_outputs_v1(
        parent_model,
        generation,
        rows.iter()
            .map(|(row, tensor)| (tensor, row.parent_logits.as_slice(), row.parent_value))
            .chain(
                anchor_train
                    .iter()
                    .map(|row| (&row.tensor, row.logits.as_slice(), row.value)),
            ),
    )?;
    let plan = plan_v1(
        &rows,
        &selected,
        &anchor_train,
        &lineage,
        arm,
        recipe_values.checkpoint_steps.as_deref(),
        recipe_values.order_seed,
    )?;
    let archive_targets = rows
        .iter()
        .enumerate()
        .map(
            |(i, (row, _))| match keep.as_ref().and_then(|k| k.get(&i)) {
                Some(kept) => kept_target_v1(row, kept),
                None => one_hot_target_v1(row.k as usize, row.target as usize).map_err(err),
            },
        )
        .collect::<Result<Vec<_>, _>>()?;
    let anchor_targets = anchor_targets_v1(&anchor_train)?;
    let anchor_sha256 = sha(&serde_json::to_vec(&anchor).map_err(err)?);
    let mut recipe = json!({"method":METHOD,"namespace":ARCHIVE_DISTILLATION_NAMESPACE_V1,
        "learning_rate":recipe_values.learning_rate,"archive_batch":ARCHIVE_BATCH,
        "anchor_batch":ANCHOR_BATCH,
        "archive_batching":"each epoch's row order cut into ceil(n/32) consecutive batches \
            whose sizes differ by at most one (L with at most 32 rows: every row each step); \
            the archive term is the mean over its batch",
        "archive_coefficient":ARCHIVE_COEFFICIENT,
        "anchor_coefficient":recipe_values.anchor_coefficient,
        "direction":"forward","adam":"fresh moments, betas 0.9 and 0.999, epsilon 1e-8",
        "checkpoints":plan.checkpoints,"epoch_steps":plan.epoch_steps});
    if let Some(options) = &options {
        recipe["options"] = json!(options);
    }
    if !update_scope.is_full_network_v1() {
        recipe["update_scope"] = json!(update_scope);
    }
    let arm_rows: Vec<ArchiveRowV1> = selected.iter().map(|&i| rows[i].0.clone()).collect();
    let mut census = json!({"bank":bank_census_v1(&bank_record.game_status, &bank_record.rows),
        "arm":bank_census_v1(&[], &arm_rows),
        "anchor_training_episodes":anchor.training.len(),"anchor_training_rows":anchor_train.len(),
        "anchor_held_out_episodes":anchor.held_out.len(),"anchor_held_out_rows":held_out_rows,
        "verified_parent_outputs":verified,
        "exposure":exposure_v1(&plan, &rows, &selected, anchor_train.len())});
    if options.is_some() {
        // A cell's kept line rows, so a reading can split the bank's line
        // rows into kept and dropped ones.
        census["kept_line_rows"] = json!(arm_rows
            .iter()
            .filter(|r| r.line.line_offered())
            .map(
                |r| json!({"game":r.game,"step":r.step,"edge_backups":r.edge_backups,
                "line":r.line})
            )
            .collect::<Vec<_>>());
    }
    if let Some(keep) = &keep {
        census["keep_rows"] = keep_census_v1(&rows, keep, &archive_targets)?;
    }
    let identity = json!({"lineage":lineage,"arm":arm,"parent":parent,
        "parent_state_sha256":inputs.state_sha256,"games":games,"bank":bank,
        "anchor":anchor,"anchor_sha256":anchor_sha256,"recipe":recipe,
        "runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "runtime_binary_sha256":runtime_binary_sha256()?});
    fs::create_dir_all(&output_directory).map_err(err)?;
    let mut state = NativePolicyValueTrainStateV1::new_v1(parent_model.clone()).map_err(err)?;
    ensure(
        state.scorer_bias_anchor_f32_bits_v1() == parent_checkpoint.scorer_bias_anchor_bits,
        "the parent's scorer bias differs from its gauge anchor",
    )?;
    let preflight_rows = anchor_identity_preflight_v1(
        &state,
        generation,
        &anchor_train,
        &anchor_targets,
        &recipe_values,
        update_scope,
    )?;
    if dry_run {
        let result = json!({"schema":PHASE1_SCHEMA,"dry_run":true,"identity":identity,
            "census":census,"anchor_preflight_rows":preflight_rows});
        publish_json(&output_directory, "phase1.json", &result)?;
        return Ok(result);
    }
    let last = stop_after_steps.unwrap_or(plan.steps.len() as u64);
    let mut telemetry = Vec::with_capacity(last as usize);
    let mut state_hashes = Vec::new();
    let mut checkpoints = Vec::new();
    for (index, step) in plan.steps.iter().take(last as usize).enumerate() {
        let number = index as u64 + 1;
        let anchor_input = ExternalAuxiliaryInputV1 {
            direction: LineBDivergenceV1::Forward,
            coefficient: recipe_values.anchor_coefficient,
            denominator: ANCHOR_BATCH,
            rows: step
                .anchor
                .iter()
                .map(|&i| ExternalAuxiliaryRowV1 {
                    encoded: view_v1(&anchor_train[i].tensor, generation),
                    target: &anchor_targets[i],
                })
                .collect(),
        };
        let archive_input = ExternalAuxiliaryInputV1 {
            direction: LineBDivergenceV1::Forward,
            coefficient: ARCHIVE_COEFFICIENT,
            denominator: step.denominator,
            rows: step
                .archive
                .iter()
                .map(|&i| ExternalAuxiliaryRowV1 {
                    encoded: view_v1(&rows[i].1, generation),
                    target: &archive_targets[i],
                })
                .collect(),
        };
        let (terms, gauge) = state
            .train_step_auxiliary_only_scoped_v1(
                generation,
                recipe_values.learning_rate,
                &[&anchor_input, &archive_input],
                update_scope,
            )
            .map_err(|e| format!("phase-1 step {number}: {e:?}"))?;
        let [anchor_term, archive_term] = terms.as_slice() else {
            unreachable!("two terms in, two results out")
        };
        let line_divergences: Vec<Value> = step
            .archive
            .iter()
            .zip(&archive_term.divergences_before)
            .filter(|(&i, _)| rows[i].0.line.line_offered())
            .map(|(&i, divergence)| {
                json!({"game":rows[i].0.game,"step":rows[i].0.step,
                    "chosen":rows[i].0.line.line_chosen(),"divergence":divergence})
            })
            .collect();
        telemetry.push(json!({"step":number,"archive_rows":step.archive.len(),
            "anchor_loss":anchor_term.auxiliary_loss,"archive_loss":archive_term.auxiliary_loss,
            "anchor_components":anchor_term.gradient_components,
            "archive_components":archive_term.gradient_components,
            "line_rows":line_divergences,
            "gauge_raw_residual":gauge.raw_gradient_residual,
            "gauge_bound":gauge.derived_absolute_bound}));
        let phase1_state = hex(&state.state_sha256_v1().map_err(err)?);
        if number == REPRODUCTION_STEP || plan.checkpoints.contains(&number) {
            state_hashes.push(json!({"step":number,"phase1_state_sha256":phase1_state}));
        }
        if stop_after_steps.is_none() && plan.checkpoints.contains(&number) {
            let parameters = state.model_v1().parameter_snapshot_v1();
            let provenance = ArchiveDistillationProvenanceV1 {
                method: METHOD.into(),
                lineage: lineage.clone(),
                arm,
                parent: parent_pin.clone(),
                parent_state_sha256: inputs.state_sha256.clone(),
                games: games.clone(),
                archive_sources: bank_record.sources.clone(),
                bank: bank.clone(),
                bank_rows_sha256: bank_record.rows_sha256.clone(),
                anchor_sha256: anchor_sha256.clone(),
                steps: number,
                learning_rate_bits: recipe_values.learning_rate.to_bits(),
                phase1_state_sha256: phase1_state.clone(),
                runtime_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").into(),
                cell: recipe_values.cell.clone(),
                update_scope,
            };
            let (checkpoint_pin, source_pin, state_sha256) = write_checkpoint_v1(
                &output_directory,
                &parent,
                &parent_checkpoint,
                &parameters,
                provenance,
            )?;
            checkpoints.push(json!({"step":number,
                "epochs":number as f64 / plan.epoch_steps as f64,"checkpoint":checkpoint_pin,
                "source":source_pin,"state_sha256":state_sha256,
                "phase1_state_sha256":phase1_state}));
        }
    }
    let telemetry_pin = publish_json(&output_directory, "telemetry.json", &telemetry)?;
    let result = json!({"schema":PHASE1_SCHEMA,"complete":true,"identity":identity,
        "census":census,"anchor_preflight_rows":preflight_rows,"steps":last,
        "stop_after_steps":stop_after_steps,"state_hashes":state_hashes,
        "checkpoints":checkpoints,"telemetry":telemetry_pin,
        "wall_seconds":started.elapsed().as_secs_f64()});
    publish_json(&output_directory, "phase1.json", &result)?;
    Ok(result)
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// The deployed sampler's probability of each action.
fn deployed_v1(logits: &[f32]) -> Result<Vec<f64>, String> {
    let mut sampler = WideCategoricalScratchV1::default();
    Ok(sampler
        .apportion(logits)
        .map_err(err)?
        .iter()
        .map(|&mass| mass as f64 / (u64::MAX as f64 + 1.0))
        .collect())
}

/// The first maximal logit's index.
fn top_v1(logits: &[f32]) -> usize {
    let mut best = 0;
    for (index, &z) in logits.iter().enumerate() {
        if z > logits[best] {
            best = index;
        }
    }
    best
}

/// The largest other logit minus the target's (positive while the target is
/// not the top action).
fn raw_gap_v1(logits: &[f32], target: usize) -> f64 {
    let other = logits
        .iter()
        .enumerate()
        .filter(|&(index, _)| index != target)
        .map(|(_, &z)| f64::from(z))
        .fold(f64::NEG_INFINITY, f64::max);
    other - f64::from(logits[target])
}

/// The top logit minus the second, capped at the sampler's clamp.
fn clamped_top_two_gap_v1(logits: &[f32]) -> f64 {
    let top = top_v1(logits);
    raw_gap_v1(logits, top).abs().min(GAP_CLAMP)
}

/// One archive row's reading under a candidate.
#[derive(Clone, Copy)]
struct ReadingV1 {
    deployed: f64,
    raw: f64,
    gap: f64,
    top: bool,
    /// The deployed probability of the menu's Spy self-target actions, when
    /// it offers them.
    self_deployed: Option<f64>,
    /// The deployed probability of the menu's Dread-Return-on-Giant actions,
    /// when it offers them.
    giant_deployed: Option<f64>,
}

/// Absorption of one row group: target probability, gap and agreement, and
/// the line actions' probability where the group's menus offer them.
fn absorption_v1(readings: &[ReadingV1]) -> Value {
    let pick = |f: &dyn Fn(&ReadingV1) -> f64| readings.iter().map(f).collect::<Vec<_>>();
    let line = |f: &dyn Fn(&ReadingV1) -> Option<f64>| {
        mean(&readings.iter().filter_map(f).collect::<Vec<_>>())
    };
    json!({"rows":readings.len(),"mean_deployed_probability":mean(&pick(&|r| r.deployed)),
        "mean_raw_probability":mean(&pick(&|r| r.raw)),
        "median_raw_gap":median(pick(&|r| r.gap)),
        "top_agreement":mean(&pick(&|r| f64::from(u8::from(r.top)))),
        "mean_deployed_self_probability":line(&|r| r.self_deployed),
        "mean_deployed_giant_probability":line(&|r| r.giant_deployed)})
}

/// A line decision's mean deployed probability at the tree's chosen offers
/// minus that at its declined supported offers (design section 5).
fn discrimination_v1(
    chosen: Option<&Vec<ReadingV1>>,
    declined: Option<&Vec<ReadingV1>>,
    line: fn(&ReadingV1) -> Option<f64>,
) -> Value {
    let line_mean = |group: Option<&Vec<ReadingV1>>| {
        group.and_then(|rows| mean(&rows.iter().filter_map(line).collect::<Vec<_>>()))
    };
    match (line_mean(chosen), line_mean(declined)) {
        (Some(a), Some(b)) => json!(a - b),
        _ => Value::Null,
    }
}

/// The tree's chosen share at its supported offers of one line decision.
fn chosen_share_v1(chosen: Option<&Vec<ReadingV1>>, offered: Option<&Vec<ReadingV1>>) -> Value {
    match offered.map(Vec::len) {
        Some(n) if n > 0 => json!(chosen.map_or(0, Vec::len) as f64 / n as f64),
        _ => Value::Null,
    }
}

/// One evidence row of a keep-set file: its training-bank row, its kept
/// actions, its target (the parent's softmax over the kept actions,
/// [`kept_target_v1`]) and its game's ordered matchup (focal deck label,
/// other deck label).
struct EvidenceRowV1 {
    index: usize,
    keep: Vec<u32>,
    target: LineBSoftmaxV1,
    matchup: [String; 2],
}

/// Each game's ordered matchup: its focal seat's deck label, then the other
/// seat's.
fn matchups_v1(games: &[ArchiveGameV1]) -> HashMap<String, [String; 2]> {
    games
        .iter()
        .map(|game| {
            let seat = usize::from(game.episode.learner_seat);
            let selected = &game.episode.selected;
            (
                game.id.clone(),
                [
                    selected[seat].label.clone(),
                    selected[1 - seat].label.clone(),
                ],
            )
        })
        .collect()
}

/// One keep-set file's evidence rows on the training bank, in bank order.
/// Every listed row separates at least one action (its keep set is not the
/// whole menu).
fn evidence_rows_v1(
    pin: &PinnedFileV1,
    lineage: &str,
    bank_rows_sha256: &str,
    rows: &[(ArchiveRowV1, NativeFlatDecisionTensorV2)],
    matchups: &HashMap<String, [String; 2]>,
) -> Result<Vec<EvidenceRowV1>, String> {
    keep_sets_v1(pin, lineage, bank_rows_sha256, rows)?
        .into_iter()
        .map(|(index, keep)| {
            let row = &rows[index].0;
            ensure(
                keep.len() < row.k as usize,
                "an evidence row's keep set separates at least one action",
            )?;
            let matchup = matchups
                .get(&row.game)
                .ok_or("an evidence row's game is not a bank game")?
                .clone();
            Ok(EvidenceRowV1 {
                index,
                target: kept_target_v1(row, &keep)?,
                keep,
                matchup,
            })
        })
        .collect()
}

/// Both keep-set files' evidence rows; the two files list disjoint games.
fn evidence_split_v1(
    evidence: &ArchiveDistillationProbeEvidenceV1,
    lineage: &str,
    bank_rows_sha256: &str,
    rows: &[(ArchiveRowV1, NativeFlatDecisionTensorV2)],
    games: &[ArchiveGameV1],
) -> Result<[Vec<EvidenceRowV1>; 2], String> {
    let matchups = matchups_v1(games);
    let training = evidence_rows_v1(
        &evidence.training,
        lineage,
        bank_rows_sha256,
        rows,
        &matchups,
    )?;
    let held_out = evidence_rows_v1(
        &evidence.held_out,
        lineage,
        bank_rows_sha256,
        rows,
        &matchups,
    )?;
    let trained: BTreeSet<&str> = training
        .iter()
        .map(|e| rows[e.index].0.game.as_str())
        .collect();
    ensure(
        held_out
            .iter()
            .all(|e| !trained.contains(rows[e.index].0.game.as_str())),
        "the training and held-out evidence files share a game",
    )?;
    Ok([training, held_out])
}

/// The deployed mass on a row's separated actions: every action outside its
/// (strictly increasing) keep set, summed in action order.
fn separated_mass_v1(deployed: &[f64], keep: &[u32]) -> f64 {
    (0..deployed.len())
        .filter(|&a| keep.binary_search(&(a as u32)).is_err())
        .map(|a| deployed[a])
        .sum()
}

/// One candidate's reading at one evidence row.
struct EvidenceReadingV1<'a> {
    game: &'a str,
    step: u64,
    matchup: &'a [String; 2],
    separated_mass: f64,
    /// KL(q || p_theta): the row's target against the candidate's softmax.
    kl_target: f64,
}

/// The evidence fit over one file's rows: the mean deployed mass on
/// separated actions and the mean KL(q || p_theta), overall and per focal
/// deck, with each row's values and held-out matchup when asked.
fn evidence_summary_v1(readings: &[EvidenceReadingV1], row_values: bool) -> Value {
    let group = |members: &[&EvidenceReadingV1]| {
        let masses: Vec<f64> = members.iter().map(|r| r.separated_mass).collect();
        let kls: Vec<f64> = members.iter().map(|r| r.kl_target).collect();
        json!({"rows":members.len(),"mean_separated_mass":mean(&masses),
            "mean_kl_target":mean(&kls)})
    };
    let mut decks: BTreeMap<&str, Vec<&EvidenceReadingV1>> = BTreeMap::new();
    for reading in readings {
        decks.entry(&reading.matchup[0]).or_default().push(reading);
    }
    let mut out = group(&readings.iter().collect::<Vec<_>>());
    out["by_deck"] = decks
        .iter()
        .map(|(deck, members)| (deck.to_string(), group(members)))
        .collect::<serde_json::Map<_, _>>()
        .into();
    if row_values {
        out["row_values"] = readings
            .iter()
            .map(|r| {
                json!({"game":r.game,"step":r.step,"matchup":r.matchup,
                    "separated_mass":r.separated_mass,"kl_target":r.kl_target})
            })
            .collect::<Vec<_>>()
            .into();
    }
    out
}

/// One held-out anchor row's reading: its deck label, its clamped top-two
/// gap, whether the parent's top action is still top, and KL(p_parent || p).
struct AnchorReadingV1<'a> {
    deck: &'a str,
    gap: f64,
    agrees: bool,
    kl: f64,
}

/// Held-out anchor readings over all decks, the non-Spy decks and each deck:
/// the median clamped top-two gap, the share of rows whose parent top action
/// is still top, and the mean KL from the parent.
fn anchor_decks_v1(readings: &[AnchorReadingV1]) -> Value {
    let group = |members: &[&AnchorReadingV1]| {
        let agree = members.iter().filter(|r| r.agrees).count();
        json!({"rows":members.len(),
            "median_clamped_top_two_gap":median(members.iter().map(|r| r.gap).collect()),
            "top_agreement":(!members.is_empty())
                .then(|| agree as f64 / members.len() as f64),
            "mean_kl_from_parent":mean(&members.iter().map(|r| r.kl).collect::<Vec<_>>())})
    };
    let mut decks: BTreeMap<&str, Vec<&AnchorReadingV1>> = BTreeMap::new();
    for reading in readings {
        decks.entry(reading.deck).or_default().push(reading);
    }
    let all: Vec<&AnchorReadingV1> = readings.iter().collect();
    let non_spy: Vec<&AnchorReadingV1> =
        readings.iter().filter(|r| !is_spy_label(r.deck)).collect();
    json!({"all":group(&all),"non_spy":group(&non_spy),
        "by_deck":decks.iter().map(|(deck, members)| (deck.to_string(), group(members)))
            .collect::<serde_json::Map<_, _>>()})
}

/// `ArchiveDistillationProbe`: forward-only readouts of each candidate.
#[allow(clippy::too_many_arguments)]
pub(super) fn probe_v1(
    parent: ExpandedModelSourceV1,
    lineage: String,
    candidates: Vec<ArchiveDistillationCandidateV1>,
    banks: Vec<ArchiveDistillationProbeBankV1>,
    anchor: ArchiveDistillationAnchorV1,
    offers: Vec<PinnedFileV1>,
    evidence: Option<ArchiveDistillationProbeEvidenceV1>,
    evidence_logits: bool,
    output_directory: PathBuf,
) -> Result<Value, String> {
    let started = std::time::Instant::now();
    ensure(
        !evidence_logits || evidence.is_some(),
        "evidence logits need the evidence keep sets",
    )?;
    ensure(
        !output_directory.exists(),
        "probe output directory must be fresh",
    )?;
    ensure(
        !candidates.is_empty() && candidates.len() <= 64,
        "invalid candidate count",
    )?;
    ensure(
        !offers.is_empty() && offers.len() <= MAX_TRAJECTORIES,
        "invalid offer-set trajectory count",
    )?;
    let mut ids = BTreeSet::new();
    for candidate in &candidates {
        ensure(
            ids.insert(candidate.id.clone()),
            "duplicate probe candidate",
        )?;
    }
    let mut roles = BTreeSet::new();
    let mut extracted = Vec::new();
    let mut parent_inputs: Option<ArchiveInputsV1> = None;
    let mut evidence_rows: Option<[Vec<EvidenceRowV1>; 2]> = None;
    for bank in &banks {
        ensure(
            (bank.role == "training" || bank.role == "completion")
                && roles.insert(bank.role.clone()),
            "probe banks are one training and at most one completion bank",
        )?;
        let inputs = load_inputs_v1(&parent, &lineage, &bank.games)?;
        let (record, rows) =
            reproduce_bank_v1(&inputs, &parent, &lineage, &bank.games, &bank.bank)?;
        if let (Some(pins), "training") = (&evidence, bank.role.as_str()) {
            evidence_rows = Some(evidence_split_v1(
                pins,
                &lineage,
                &record.rows_sha256,
                &rows,
                &inputs.games,
            )?);
        }
        extracted.push((bank.role.clone(), rows));
        parent_inputs.get_or_insert(inputs);
    }
    ensure(
        roles.contains("training"),
        "the probe reads the training bank",
    )?;
    let inputs = parent_inputs.ok_or("the probe reads at least one bank")?;
    let generation = inputs.policy.feature_identity_v1().generation;
    let mut episode_ids = BTreeSet::new();
    let held_out = load_anchor_split_v1(
        &anchor.held_out,
        &inputs.policy,
        &inputs.state_sha256,
        &mut episode_ids,
    )?;
    let mut offer_rows = Vec::new();
    for pin in &offers {
        let trajectory =
            read_parent_trajectory_v1(pin, &inputs.policy, &inputs.state_sha256, &mut episode_ids)?;
        offer_rows.extend(offers_v1(&trajectory)?);
    }
    let verified = verify_parent_outputs_v1(
        inputs.state.model_v1(),
        generation,
        extracted
            .iter()
            .flat_map(|(_, rows)| rows.iter())
            .map(|(row, tensor)| (tensor, row.parent_logits.as_slice(), row.parent_value))
            .chain(
                held_out
                    .iter()
                    .map(|row| (&row.tensor, row.logits.as_slice(), row.value)),
            )
            .chain(
                offer_rows
                    .iter()
                    .map(|row| (&row.tensor, row.logits.as_slice(), row.value)),
            ),
    )?;
    let parent_targets = anchor_targets_v1(&held_out)?;
    let mut readouts = Vec::new();
    for candidate in &candidates {
        let (policy, state) = initialize(&candidate.source)?;
        ensure(
            policy.feature_identity_v1() == inputs.policy.feature_identity_v1(),
            "candidate feature identity differs from the parent's",
        )?;
        let model = state.model_v1();
        let mut archive = serde_json::Map::new();
        for (role, rows) in &extracted {
            let mut groups: std::collections::BTreeMap<&str, Vec<ReadingV1>> = Default::default();
            let mut line_rows = Vec::new();
            for (row, tensor) in rows {
                let (logits, _) = forward_v1(model, tensor, generation)?;
                let target = row.target as usize;
                let deployed = deployed_v1(&logits)?;
                let raw = line_b_softmax_v1(&f64_logits(&logits))
                    .map_err(err)?
                    .probabilities;
                let line_probability = |actions: &[u32]| -> f64 {
                    actions.iter().map(|&a| deployed[a as usize]).sum()
                };
                let line = row.line;
                let reading = ReadingV1 {
                    deployed: deployed[target],
                    raw: raw[target],
                    gap: raw_gap_v1(&logits, target),
                    top: top_v1(&logits) == target,
                    self_deployed: line
                        .self_offered
                        .then(|| line_probability(&row.self_actions)),
                    giant_deployed: line
                        .giant_offered
                        .then(|| line_probability(&row.giant_actions)),
                };
                groups.entry("all").or_default().push(reading);
                for (name, member) in [
                    ("line_offered", line.line_offered()),
                    ("self_chosen", line.self_chosen),
                    ("self_declined", line.self_offered && !line.self_chosen),
                    ("self_offered", line.self_offered),
                    ("giant_chosen", line.giant_chosen),
                    ("giant_declined", line.giant_offered && !line.giant_chosen),
                    ("giant_offered", line.giant_offered),
                ] {
                    if member {
                        groups.entry(name).or_default().push(reading);
                    }
                }
                if line.line_offered() {
                    line_rows.push(json!({"game":row.game,"step":row.step,
                        "chosen":line.line_chosen(),"self_offered":line.self_offered,
                        "giant_offered":line.giant_offered,
                        "deployed_probability":reading.deployed,"raw_probability":reading.raw,
                        "raw_gap":reading.gap,"self_deployed_probability":reading.self_deployed,
                        "giant_deployed_probability":reading.giant_deployed}));
                }
            }
            let summary: serde_json::Map<String, Value> = groups
                .iter()
                .map(|(name, readings)| (name.to_string(), absorption_v1(readings)))
                .collect();
            let discrimination = json!({
                "self":discrimination_v1(groups.get("self_chosen"), groups.get("self_declined"),
                    |r| r.self_deployed),
                "giant":discrimination_v1(groups.get("giant_chosen"), groups.get("giant_declined"),
                    |r| r.giant_deployed)});
            let chosen_share = json!({
                "self":chosen_share_v1(groups.get("self_chosen"), groups.get("self_offered")),
                "giant":chosen_share_v1(groups.get("giant_chosen"), groups.get("giant_offered"))});
            archive.insert(
                role.clone(),
                json!({"groups":summary,"discrimination":discrimination,
                    "tree_chosen_share":chosen_share,"line_rows":line_rows}),
            );
        }
        let mut gaps = Vec::with_capacity(held_out.len());
        let (mut kl_all, mut kl_spy, mut kl_non_spy) = (Vec::new(), Vec::new(), Vec::new());
        let mut value_change = Vec::with_capacity(held_out.len());
        let (mut agree, mut non_spy) = (0usize, 0usize);
        let mut deck_readings = Vec::with_capacity(held_out.len());
        for (row, parent_target) in held_out.iter().zip(&parent_targets) {
            let (logits, value) = forward_v1(model, &row.tensor, generation)?;
            let gap = clamped_top_two_gap_v1(&logits);
            gaps.push(gap);
            let kl = line_b_divergence_v1(
                LineBDivergenceV1::Forward,
                &f64_logits(&logits),
                parent_target,
            )
            .map_err(err)?
            .divergence;
            kl_all.push(kl);
            value_change.push((f64::from(value) - f64::from(f32::from_bits(row.value))).abs());
            let agrees = top_v1(&logits) == top_v1(&recorded_logits(&row.logits));
            if is_spy_label(&row.learner_deck) {
                kl_spy.push(kl);
            } else {
                kl_non_spy.push(kl);
                non_spy += 1;
                agree += usize::from(agrees);
            }
            deck_readings.push(AnchorReadingV1 {
                deck: &row.learner_deck,
                gap,
                agrees,
                kl,
            });
        }
        let mut offer_readings = Vec::with_capacity(offer_rows.len());
        let (mut offer_deployed, mut offer_raw, mut offer_top) = (Vec::new(), Vec::new(), 0usize);
        for offer in &offer_rows {
            let (logits, _) = forward_v1(model, &offer.tensor, generation)?;
            let deployed = deployed_v1(&logits)?;
            let raw = line_b_softmax_v1(&f64_logits(&logits))
                .map_err(err)?
                .probabilities;
            let d: f64 = offer.self_actions.iter().map(|&a| deployed[a]).sum();
            let r: f64 = offer.self_actions.iter().map(|&a| raw[a]).sum();
            let top = offer.self_actions.contains(&top_v1(&logits));
            offer_deployed.push(d);
            offer_raw.push(r);
            offer_top += usize::from(top);
            offer_readings.push(json!({"episode":offer.episode,"step":offer.step,
                "deployed_self_probability":d,"raw_self_probability":r,"self_top":top}));
        }
        let mut readout = json!({"id":candidate.id,"source":candidate.source,
            "state_sha256":hex(&state.state_sha256_v1().map_err(err)?),
            "adam_step":state.adam_step_v1(),"archive":archive,
            "anchor_held_out":{"rows":held_out.len(),
                "median_clamped_top_two_gap":median(gaps),
                "mean_kl_from_parent_non_spy":mean(&kl_non_spy),
                "mean_kl_from_parent_spy":mean(&kl_spy),
                "mean_kl_from_parent_all":mean(&kl_all),
                "mean_absolute_value_change":mean(&value_change),"non_spy_rows":non_spy,
                "non_spy_top_agreement":(non_spy > 0).then(|| agree as f64 / non_spy as f64)},
            "learner_offers":{"offers":offer_rows.len(),
                "mean_deployed_self_probability":mean(&offer_deployed),
                "mean_raw_self_probability":mean(&offer_raw),
                "self_top_share":(!offer_rows.is_empty())
                    .then(|| offer_top as f64 / offer_rows.len() as f64),
                "rows":offer_readings}});
        if let Some([training, held]) = &evidence_rows {
            let bank_rows = &extracted
                .iter()
                .find(|(role, _)| role == "training")
                .expect("the probe reads the training bank")
                .1;
            let mut fit = serde_json::Map::new();
            let mut scored = serde_json::Map::new();
            for (name, members) in [("training", training), ("held_out", held)] {
                let mut readings = Vec::with_capacity(members.len());
                let mut rows = Vec::new();
                for member in members {
                    let (row, tensor) = &bank_rows[member.index];
                    let (logits, _) = forward_v1(model, tensor, generation)?;
                    if evidence_logits {
                        rows.push(json!({"game":row.game,"step":row.step,
                            "logit_bits":logits.iter().map(|x| x.to_bits()).collect::<Vec<u32>>(),
                            "deployed":deployed_v1(&logits)?}));
                    }
                    readings.push(EvidenceReadingV1 {
                        game: &row.game,
                        step: row.step,
                        matchup: &member.matchup,
                        separated_mass: separated_mass_v1(&deployed_v1(&logits)?, &member.keep),
                        kl_target: line_b_divergence_v1(
                            LineBDivergenceV1::Forward,
                            &f64_logits(&logits),
                            &member.target,
                        )
                        .map_err(err)?
                        .divergence,
                    });
                }
                fit.insert(
                    name.into(),
                    evidence_summary_v1(&readings, name == "held_out"),
                );
                if evidence_logits {
                    scored.insert(name.into(), rows.into());
                }
            }
            readout["evidence"] = fit.into();
            if evidence_logits {
                readout["evidence_rows"] = scored.into();
            }
            readout["anchor_held_out_decks"] = anchor_decks_v1(&deck_readings);
        }
        readouts.push(readout);
    }
    let mut result = json!({"schema":PROBE_SCHEMA,"complete":true,"lineage":lineage,
        "parent":parent,"parent_state_sha256":inputs.state_sha256,"banks":banks,
        "anchor":anchor,"offers":offers,"anchor_held_out_rows":held_out.len(),
        "offer_episodes":offers.len(),"learner_offers":offer_rows.len(),
        "verified_parent_outputs":verified,"candidates":readouts,
        "runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "wall_seconds":started.elapsed().as_secs_f64()});
    if let Some(pins) = &evidence {
        result["evidence"] = json!(pins);
    }
    fs::create_dir_all(&output_directory).map_err(err)?;
    publish_json(&output_directory, "probe.json", &result)?;
    Ok(result)
}

mod retention_trace;
pub(super) use retention_trace::retention_trace_v1;

#[cfg(test)]
mod tests;
