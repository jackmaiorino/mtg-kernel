//! TP1 Stage B searched-game targets (collab LANES/teacher-price-20261019,
//! STAGE-B.md): each update reads one packet of separation keep sets that the
//! lane's orchestrator built from a searched game the start-of-update student
//! played. Every root is authenticated as `s4a-root-actions` does, encoded by
//! the student with bit-equal logits to the game's own record, and taught a
//! forward KL toward SD1's kept-set target (the student's binary64 softmax
//! over the kept actions), `coefficient / denominator` times the sum, as
//! off-batch rows beside GAE in one Adam step.
//!
//! Stage B2 (STAGE-B2.md) adds the centered win-frequency target, computed
//! here from the searched game's own node statistics with each arm's
//! permutation (identity for M, a seeded shuffle for S), and a per-update cap
//! on the auxiliary gradient's whole-network L2 relative to the ordinary one.

use super::archive_distillation_v1::{capture_v1, view_v1};
use super::*;
use crate::line_b_teacher_target_v1::{line_b_softmax_v1, LineBDivergenceV1, LineBSoftmaxV1};
use crate::native_policy_train_step_v1::{
    training_forward_v1, ExternalAuxiliaryInputV1, ExternalAuxiliaryRowV1,
};

mod m_target_v1;
pub use m_target_v1::{M_PERMUTATION_VERSION_V1, M_SUPPORT_MIN_V1, M_TARGET_VERSION_V1};

#[cfg(feature = "gameplay-decision-trace-v1")]
use crate::regret_census_v1::stage4a::SearchedRootV1 as SearchedRootT;
#[cfg(not(feature = "gameplay-decision-trace-v1"))]
use SearchedRootStubV1 as SearchedRootT;

pub const SEARCH_TARGETS_METHOD_V1: &str = "tp1b-search-targets-v1-20261019";
const PACKET_SCHEMA: &str = "tp1b-search-targets/v1";

/// One opponent of the searched games, in game-setup order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchTargetsOpponentV1 {
    pub label: String,
    pub source: ExpandedModelSourceV1,
}

/// The target family (STAGE-B.md's kept set; STAGE-B2.md's centered win
/// frequency).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchTargetKindV1 {
    #[default]
    KeptSet,
    CenteredWinFrequency,
}

/// The per-update cap on the auxiliary gradient (STAGE-B2.md section 4 item
/// 2): off, or the auxiliary whole-network L2 norm at most `kappa` times the
/// ordinary one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SearchTargetCapV1 {
    #[default]
    Off,
    Ratio {
        kappa: f64,
    },
}

impl SearchTargetCapV1 {
    pub(crate) fn kappa_v1(&self) -> Option<f64> {
        match self {
            Self::Off => None,
            Self::Ratio { kappa } => Some(*kappa),
        }
    }
}

/// One update's searched-game targets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchTargetsOptionsV1 {
    pub packet: PinnedFileV1,
    pub bank_spec: PinnedFileV1,
    pub opponents: Vec<SearchTargetsOpponentV1>,
    pub coefficient: f64,
    /// R0: the fixed denominator, at least the packet's row count.
    pub denominator: usize,
    /// The search allocation every searched game must record (E4), from the run.
    pub allocation: Value,
    /// This update's identity as its run sees it ({"run","arm","seed","update"});
    /// the packet's binding must carry the same values and the update's own
    /// start checkpoint.
    pub expected: Value,
    #[serde(default)]
    pub target: SearchTargetKindV1,
    #[serde(default)]
    pub cap: SearchTargetCapV1,
}

impl SearchTargetsOptionsV1 {
    pub fn validate_v1(&self) -> Result<(), String> {
        // One allocation throughout: the packet binding is checked against the
        // expectation, the searched game against `allocation`.
        ensure(
            self.expected.get("allocation") == Some(&self.allocation),
            "search-target expectation and allocation differ",
        )?;
        ensure(
            self.coefficient.is_finite() && self.coefficient > 0.0,
            "search-target coefficient must be positive and finite",
        )?;
        ensure(
            (1..=4096).contains(&self.denominator),
            "search-target denominator outside 1..4096",
        )?;
        if let SearchTargetCapV1::Ratio { kappa } = self.cap {
            ensure(
                kappa.is_finite() && kappa > 0.0,
                "search-target cap ratio must be positive and finite",
            )?;
        }
        if self.target == SearchTargetKindV1::CenteredWinFrequency {
            ensure(
                matches!(
                    self.expected.get("arm").and_then(Value::as_str),
                    Some("M" | "S")
                ) && self.expected.get("seed").and_then(Value::as_u64).is_some()
                    && self
                        .expected
                        .get("update")
                        .and_then(Value::as_u64)
                        .is_some(),
                "centered win-frequency targets need arm M or S, a seed and an update",
            )?;
        }
        ensure(
            !self.opponents.is_empty(),
            "search targets need the searched games' opponents",
        )
    }

    /// Stage B2's extra identity, absent for Stage B's kept-set runs.
    fn b2_identity_v1(&self) -> Option<Value> {
        (self.target != SearchTargetKindV1::KeptSet || self.cap != SearchTargetCapV1::Off).then(|| {
            json!({"target":self.target,"target_version":M_TARGET_VERSION_V1,
                "support_min":M_SUPPORT_MIN_V1,"temperature_bits":0.25_f64.to_bits(),
                "permutation":M_PERMUTATION_VERSION_V1,"cap":self.cap.kappa_v1().map_or(
                    json!({"mode":"off"}),|kappa|json!({"mode":"ratio","kappa_bits":kappa.to_bits()}))})
        })
    }
}

/// The run-level identity a continuation must preserve.
pub fn continuation_identity(options: &SearchTargetsOptionsV1) -> Value {
    let mut identity = json!({"method":SEARCH_TARGETS_METHOD_V1,"bank_spec":options.bank_spec,
        "opponents":options.opponents,"coefficient_bits":options.coefficient.to_bits(),
        "denominator":options.denominator,"allocation":options.allocation});
    if let Some(b2) = options.b2_identity_v1() {
        identity["stage_b2"] = b2;
    }
    identity
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PacketRowV1 {
    search: PinnedFileV1,
    case: Value,
    root: PacketRootV1,
    /// Kept-set rows: live indices of the actions the target keeps.
    #[serde(default)]
    keep: Option<Vec<u32>>,
    /// Centered win-frequency rows: `{"kind":"m","perm":[...]}`.
    #[serde(default)]
    target: Option<PacketTargetV1>,
    /// The student's root logit bits the searched game recorded.
    logit_bits: Vec<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PacketTargetV1 {
    kind: String,
    perm: Vec<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PacketRootV1 {
    step: u64,
    physical_decision_id: u64,
    substep: u32,
}

#[derive(Debug, Deserialize)]
struct PacketV1 {
    schema: String,
    binding: Value,
    rows: Vec<PacketRowV1>,
}

fn expected_allocation(expected: &Value) -> Result<Value, String> {
    expected
        .get("allocation")
        .cloned()
        .ok_or_else(|| "search-target expectation lacks the run's allocation".into())
}

/// A retained packet and its searched game, still byte-identical to their pins
/// (continuation and recovery checks).
pub(crate) fn revalidate_packet_v1(packet: &PinnedFileV1) -> Result<(), String> {
    let bytes = read_pinned_bytes(packet)?;
    let document: Value = serde_json::from_slice(&bytes).map_err(err)?;
    let game: PinnedFileV1 =
        serde_json::from_value(document["binding"]["game"].clone()).map_err(err)?;
    read_pinned_bytes(&game).map(|_| ())
}

/// The packet belongs to this update: its run, arm, seed and global update
/// equal the run's, its start checkpoint is the update's own source, and
/// every row names the binding's searched case.
fn check_binding_v1(
    packet: &PacketV1,
    source: &ExpandedModelSourceV1,
    expected: &Value,
) -> Result<(), String> {
    let binding = &packet.binding;
    for key in ["run", "arm", "seed", "update"] {
        ensure(
            expected.get(key).is_some() && binding.get(key) == expected.get(key),
            "search-target packet belongs to another run, arm, seed or update",
        )?;
    }
    ensure(
        binding.get("start_checkpoint") == Some(&json!(source.checkpoint)),
        "search-target packet was built from another start checkpoint",
    )?;
    ensure(
        binding.get("allocation") == Some(&expected_allocation(expected)?),
        "search-target packet allocation differs from the run's",
    )?;
    let game: PinnedFileV1 = serde_json::from_value(
        binding
            .get("game")
            .cloned()
            .ok_or("search-target packet lacks its game pin")?,
    )
    .map_err(err)?;
    read_pinned_bytes(&game)?;
    let mut roots = BTreeSet::new();
    for row in &packet.rows {
        ensure(
            Some(&row.case) == binding.get("case") && row.search == game,
            "a search-target row names another case or game than its packet",
        )?;
        ensure(
            roots.insert((row.root.step, row.root.substep)),
            "a search-target packet repeats a root",
        )?;
    }
    Ok(())
}

/// One taught row: the student's encoded root and its target.
pub(super) struct TaughtRowV1 {
    pub(super) tensor: NativeFlatDecisionTensorV2,
    pub(super) target: LineBSoftmaxV1,
}

/// SD1's kept-set target: the binary64 softmax of the logits over the kept
/// actions, placed at them; probability 0 and log-probability minus infinity
/// elsewhere. Every kept action must be a legal index, sorted and unique.
fn kept_target_v1(logit_bits: &[u32], keep: &[u32]) -> Result<LineBSoftmaxV1, String> {
    ensure(
        !keep.is_empty()
            && keep.windows(2).all(|w| w[0] < w[1])
            && keep.iter().all(|&a| (a as usize) < logit_bits.len()),
        "search-target keep set must be sorted, unique and legal",
    )?;
    let kept: Vec<f64> = keep
        .iter()
        .map(|&a| f64::from(f32::from_bits(logit_bits[a as usize])))
        .collect();
    let softmax = line_b_softmax_v1(&kept).map_err(|e| format!("{e:?}"))?;
    let mut probabilities = vec![0.0; logit_bits.len()];
    let mut log_probabilities = vec![f64::NEG_INFINITY; logit_bits.len()];
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

/// Read, authenticate and encode one update's packet. `state` is the
/// start-of-update trainer: each root's training forward must reproduce the
/// deployed scorer's logits, which must equal the game's record. Returns the
/// receipt fields and the taught rows (empty when the packet has none).
pub(super) fn teach(
    source: &ExpandedModelSourceV1,
    state: &NativePolicyValueTrainStateV1,
    options: &SearchTargetsOptionsV1,
) -> Result<(Value, Vec<TaughtRowV1>), String> {
    options.validate_v1()?;
    let packet: PacketV1 =
        serde_json::from_slice(&read_pinned_bytes(&options.packet)?).map_err(err)?;
    ensure(
        packet.schema == PACKET_SCHEMA,
        "search-target packet schema differs",
    )?;
    check_binding_v1(&packet, source, &options.expected)?;
    ensure(
        packet.rows.len() <= options.denominator,
        "search-target packet has more rows than its denominator",
    )?;
    // The searched game itself, rows or none: its envelope, case, the run's
    // required allocation and the bank binding of this student and opponents.
    let (student, _) = load_expanded_inference_v1(source)?;
    let generation = student.feature_identity_v1().generation;
    let mut labels = Vec::new();
    let mut opponents = Vec::new();
    for opponent in &options.opponents {
        labels.push(opponent.label.clone());
        opponents.push(load_expanded_inference_v1(&opponent.source)?.0);
    }
    let spec = read_pinned_bytes(&options.bank_spec)?;
    let game: PinnedFileV1 = serde_json::from_value(packet.binding["game"].clone()).map_err(err)?;
    let record: Value = serde_json::from_slice(&read_pinned_bytes(&game)?).map_err(err)?;
    validate_game_v1(
        &spec,
        labels.clone(),
        opponents_clone(&options.opponents)?,
        &student,
        &record,
        &packet.binding["case"],
        &options.allocation,
    )?;
    if packet.rows.is_empty() {
        return Ok((
            json!({"rows":0,"denominator":options.denominator,"binding":packet.binding}),
            Vec::new(),
        ));
    }
    // Every row names the bound game (checked above): parse it once.
    let roots: Vec<_> = packet
        .rows
        .iter()
        .map(|row| SearchedRootT {
            search: &record,
            case: &row.case,
            step: row.root.step,
            physical_decision_id: row.root.physical_decision_id,
            substep: row.root.substep,
        })
        .collect();
    let sessions = authenticate_v1(&spec, labels, opponents, &student, &roots)?;
    let model = state.model_v1();
    let parameters = model.parameter_snapshot_v1();
    let mut taught = Vec::with_capacity(packet.rows.len());
    let mut census = Vec::with_capacity(packet.rows.len());
    ensure(
        record.get("allocation") == Some(&options.allocation),
        "the searched game's allocation differs from the run's",
    )?;
    let mut recorded_rows = Vec::with_capacity(packet.rows.len());
    for row in &packet.rows {
        let recorded = record["game"]["decisions"]
            .as_array()
            .and_then(|decisions| {
                decisions
                    .iter()
                    .find(|d| d["step"] == row.root.step && d["substep"] == row.root.substep)
            })
            .ok_or("the searched game lacks the packet's root decision")?;
        ensure(
            recorded["node_stats"]["root_logit_bits"] == json!(row.logit_bits),
            "a packet's logits differ from the searched game's node statistics",
        )?;
        recorded_rows.push(recorded);
    }
    for ((row, recorded), (session, chooser, identity)) in
        packet.rows.iter().zip(recorded_rows).zip(sessions)
    {
        let (scores, tensor) = capture_v1(&student, &session)?;
        let deployed: Vec<u32> = scores.logits.iter().map(|z| z.to_bits()).collect();
        ensure(
            deployed == row.logit_bits,
            "the student's root logits differ from the searched game's record",
        )?;
        let (logits, _) =
            training_forward_v1(model, generation, &parameters, view_v1(&tensor, generation))
                .map_err(|e| format!("training forward: {e:?}"))?;
        ensure(
            logits.iter().map(|z| z.to_bits()).collect::<Vec<_>>() == deployed,
            "the training forward differs from the student's deployed scores",
        )?;
        let target = match (options.target, &row.keep, &row.target) {
            (SearchTargetKindV1::KeptSet, Some(keep), None) => {
                census.push(
                    json!({"step":row.root.step,"substep":row.root.substep,"chooser":chooser,
                    "root":identity,"k":row.logit_bits.len(),"keep":keep,
                    "removed_mass":removed_mass_v1(&row.logit_bits, keep)?}),
                );
                kept_target_v1(&row.logit_bits, keep)?
            }
            (SearchTargetKindV1::CenteredWinFrequency, None, Some(target)) => {
                ensure(
                    target.kind == "m",
                    "a search-target row's kind differs from the run's",
                )?;
                let counts =
                    m_target_v1::live_counts_v1(&recorded["node_stats"], row.logit_bits.len())?;
                let support = m_target_v1::support_v1(&counts);
                let expected = m_target_v1::expected_perm_v1(
                    options.expected["arm"].as_str().unwrap_or(""),
                    options.expected["seed"].as_u64().unwrap_or(0),
                    options.expected["update"].as_u64().unwrap_or(0),
                    row.root.step,
                    row.root.substep,
                    &support,
                )?;
                let perm: Vec<usize> = target.perm.iter().map(|&a| a as usize).collect();
                ensure(
                    perm == expected,
                    "an m-kind row's permutation differs from its arm's",
                )?;
                let q = m_target_v1::m_target_v1(&row.logit_bits, &counts, &perm)?;
                census.push(
                    json!({"step":row.root.step,"substep":row.root.substep,"chooser":chooser,
                    "root":identity,"k":row.logit_bits.len(),"support":support,"perm":perm,
                    "tv":m_target_v1::total_variation_v1(&row.logit_bits, &q)?}),
                );
                q
            }
            _ => return Err("a search-target row's form differs from the run's target".into()),
        };
        taught.push(TaughtRowV1 { tensor, target });
    }
    Ok((
        json!({"rows":taught.len(),"denominator":options.denominator,"binding":packet.binding,
            "census":census}),
        taught,
    ))
}

fn opponents_clone(
    opponents: &[SearchTargetsOpponentV1],
) -> Result<Vec<FrozenPlayPolicyV1>, String> {
    opponents
        .iter()
        .map(|o| load_expanded_inference_v1(&o.source).map(|(p, _)| p))
        .collect()
}

#[cfg(feature = "gameplay-decision-trace-v1")]
fn validate_game_v1(
    spec: &[u8],
    labels: Vec<String>,
    opponents: Vec<FrozenPlayPolicyV1>,
    student: &FrozenPlayPolicyV1,
    search: &Value,
    case: &Value,
    allocation: &Value,
) -> Result<(), String> {
    crate::regret_census_v1::stage4a::validate_searched_game_v1(
        spec, labels, opponents, student, search, case, allocation,
    )
}

#[cfg(not(feature = "gameplay-decision-trace-v1"))]
fn validate_game_v1(
    _spec: &[u8],
    _labels: Vec<String>,
    _opponents: Vec<FrozenPlayPolicyV1>,
    _student: &FrozenPlayPolicyV1,
    _search: &Value,
    _case: &Value,
    _allocation: &Value,
) -> Result<(), String> {
    Err("search targets require gameplay-decision-trace-v1".into())
}

#[cfg(feature = "gameplay-decision-trace-v1")]
fn authenticate_v1(
    spec: &[u8],
    labels: Vec<String>,
    opponents: Vec<FrozenPlayPolicyV1>,
    student: &FrozenPlayPolicyV1,
    roots: &[crate::regret_census_v1::stage4a::SearchedRootV1<'_>],
) -> Result<Vec<(FastActorSessionV1, Value, Value)>, String> {
    crate::regret_census_v1::stage4a::authenticated_roots_v1(
        spec, labels, opponents, student, roots,
    )
}

#[cfg(not(feature = "gameplay-decision-trace-v1"))]
fn authenticate_v1(
    _spec: &[u8],
    _labels: Vec<String>,
    _opponents: Vec<FrozenPlayPolicyV1>,
    _student: &FrozenPlayPolicyV1,
    _roots: &[SearchedRootStubV1<'_>],
) -> Result<Vec<(FastActorSessionV1, Value, Value)>, String> {
    Err("search targets require gameplay-decision-trace-v1".into())
}

/// Without the trace feature the root type is a local stand-in.
#[cfg(not(feature = "gameplay-decision-trace-v1"))]
#[allow(dead_code)]
struct SearchedRootStubV1<'a> {
    search: &'a Value,
    case: &'a Value,
    step: u64,
    physical_decision_id: u64,
    substep: u32,
}

/// The student's softmax mass outside the keep set (what the target removes).
fn removed_mass_v1(logit_bits: &[u32], keep: &[u32]) -> Result<f64, String> {
    let logits: Vec<f64> = logit_bits
        .iter()
        .map(|&b| f64::from(f32::from_bits(b)))
        .collect();
    let p = line_b_softmax_v1(&logits).map_err(|e| format!("{e:?}"))?;
    Ok((0..logits.len())
        .filter(|a| !keep.contains(&(*a as u32)))
        .map(|a| p.probabilities[a])
        .sum())
}

/// The off-batch auxiliary input over the taught rows.
pub(super) fn auxiliary_input_v1<'a>(
    rows: &'a [TaughtRowV1],
    options: &SearchTargetsOptionsV1,
    generation: FreshLineageGenerationV1,
) -> ExternalAuxiliaryInputV1<'a> {
    ExternalAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: options.coefficient,
        denominator: options.denominator,
        cap: options.cap.kappa_v1(),
        step_telemetry: options.b2_identity_v1().is_some(),
        rows: rows
            .iter()
            .map(|row| ExternalAuxiliaryRowV1 {
                encoded: view_v1(&row.tensor, generation),
                target: &row.target,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// STAGE-B2.md section 4 item 4: m-kind targets equal the mirrored Python
    /// reference (collab LANES/teacher-price-20261019/tools/tp1b2_mref.py,
    /// `fixtures`) bit for bit, with each arm's permutation.
    const M_FIXTURES: &str = r#"[{"name":"fill_example","logit_bits":[3222494606,3222494606,3194257346],"n":[20,20,0],"wins":[0,20,0],"arm":"M","seed":1,"update":0,"step":0,"substep":0,"perm":[0,1],"q_bits":[4554031391363452265,4606029007083813578,4593771128546524085],"log_q_bits":[13844142394595313870,13817477020883497866,13835187729121289017]},{"name":"noncontiguous_unequal","logit_bits":[1056964608,3214934016,1073741824,0,3227516928],"n":[30,2,12,9,40],"wins":[21,1,3,9,10],"arm":"M","seed":1,"update":0,"step":0,"substep":0,"perm":[0,2,3,4],"q_bits":[4593772699231650457,4566725224600569721,4580168201699469293,4605871818880496763,4544347872815167102],"log_q_bits":[13835186960887090394,13842063139027917465,13839738697702681811,13818205426526034539,13844998200863610474]},{"name":"three_cycle_s","logit_bits":[1056964608,3214934016,1073741824,0,3227516928],"n":[30,2,12,9,40],"wins":[21,1,3,9,10],"arm":"S","seed":3,"update":7,"step":2,"substep":2,"perm":[3,2,4,0],"q_bits":[4583817376409311271,4574907531893074300,4593739429975503082,4603835804643986082,4596705000675226174],"log_q_bits":[13838580989140637716,13840618797663870530,13835203289420109844,13825411289280043580,13833096723744370727]},{"name":"large_offset","logit_bits":[3657433088,3657433088],"n":[8,8],"wins":[5,6],"arm":"M","seed":1,"update":0,"step":0,"substep":0,"perm":[0,1],"q_bits":[4598516437858171526,4604760009829884605],"log_q_bits":[13831965260873969737,13822686877649387812]},{"name":"zero_offset","logit_bits":[0,0],"n":[8,8],"wins":[5,6],"arm":"M","seed":1,"update":0,"step":0,"substep":0,"perm":[0,1],"q_bits":[4598516437858171526,4604760009829884605],"log_q_bits":[13831965260873969737,13822686877649387812]},{"name":"cutoff_fallback","logit_bits":[0,3291578368,3291594752],"n":[2,16,16],"wins":[1,16,0],"arm":"M","seed":1,"update":0,"step":0,"substep":0,"perm":[1,2],"q_bits":[4607182418800017408,19694110477389797,0],"log_q_bits":[0,13872793294347436032,13872872459184635904]}]"#;

    #[test]
    fn m_targets_equal_the_mirrored_reference_bit_for_bit() {
        let fixtures: Vec<Value> = serde_json::from_str(M_FIXTURES).unwrap();
        for f in &fixtures {
            let bits: Vec<u32> = serde_json::from_value(f["logit_bits"].clone()).unwrap();
            let counts = m_target_v1::LiveCountsV1 {
                n: serde_json::from_value(f["n"].clone()).unwrap(),
                wins: serde_json::from_value(f["wins"].clone()).unwrap(),
            };
            let support = m_target_v1::support_v1(&counts);
            let perm = m_target_v1::expected_perm_v1(
                f["arm"].as_str().unwrap(),
                f["seed"].as_u64().unwrap(),
                f["update"].as_u64().unwrap(),
                f["step"].as_u64().unwrap(),
                f["substep"].as_u64().unwrap() as u32,
                &support,
            )
            .unwrap();
            let want: Vec<usize> = serde_json::from_value(f["perm"].clone()).unwrap();
            assert_eq!(perm, want, "{}", f["name"]);
            let q = m_target_v1::m_target_v1(&bits, &counts, &perm).unwrap();
            let q_bits: Vec<u64> = q.probabilities.iter().map(|x| x.to_bits()).collect();
            let log_bits: Vec<u64> = q.log_probabilities.iter().map(|x| x.to_bits()).collect();
            assert_eq!(json!(q_bits), f["q_bits"], "{}", f["name"]);
            assert_eq!(json!(log_bits), f["log_q_bits"], "{}", f["name"]);
        }
        // A large common logit offset leaves the target unchanged.
        let by_name = |name: &str| fixtures.iter().find(|f| f["name"] == name).unwrap();
        assert_eq!(
            by_name("large_offset")["q_bits"],
            by_name("zero_offset")["q_bits"]
        );
    }

    #[test]
    fn m_targets_refuse_malformed_rows_and_wrong_permutations() {
        let counts = m_target_v1::LiveCountsV1 {
            n: vec![30, 2, 12, 9, 40],
            wins: vec![21, 1, 3, 9, 10],
        };
        let bits: Vec<u32> = [0.5_f32, -1.25, 2.0, 0.0, -3.5]
            .iter()
            .map(|z| z.to_bits())
            .collect();
        // Not a permutation of the support, or touching an unsupported action.
        for perm in [vec![0, 2, 3], vec![0, 1, 3, 4], vec![0, 0, 3, 4]] {
            assert!(m_target_v1::m_target_v1(&bits, &counts, &perm).is_err());
        }
        // Fewer than two supported actions.
        let thin = m_target_v1::LiveCountsV1 {
            n: vec![8, 7],
            wins: vec![1, 1],
        };
        assert!(m_target_v1::m_target_v1(&bits[..2], &thin, &[0]).is_err());
        // Arms other than M and S have no m-kind permutation.
        assert!(m_target_v1::expected_perm_v1("T", 1, 0, 0, 0, &[0, 2]).is_err());
        // Node statistics: a non-identity storage-to-live map, then defects.
        let stats = json!({"n":[12, 30, 2],"wins":[3, 21, 1],"storage_to_live":[2, 0, 1]});
        let live = m_target_v1::live_counts_v1(&stats, 3).unwrap();
        assert_eq!(
            live,
            m_target_v1::LiveCountsV1 {
                n: vec![30, 2, 12],
                wins: vec![21, 1, 3]
            }
        );
        for bad in [
            json!({"n":[12, 30, 2],"wins":[3, 21, 1],"storage_to_live":[2, 2, 1]}),
            json!({"n":[12, 30, 2],"wins":[13, 21, 1],"storage_to_live":[2, 0, 1]}),
            json!({"n":[12, 30],"wins":[3, 21],"storage_to_live":[1, 0]}),
            json!({"n":[12, 30, 2.5],"wins":[3, 21, 1],"storage_to_live":[2, 0, 1]}),
        ] {
            assert!(m_target_v1::live_counts_v1(&bad, 3).is_err(), "{bad}");
        }
    }

    #[test]
    fn stage_b2_options_bind_target_and_cap_into_the_continuation_identity() {
        let pin = PinnedFileV1 {
            path: PathBuf::from("unused"),
            sha256: String::new(),
        };
        let e4 = json!({"initial":32000,"refresh":16000,"total":512000});
        let base = SearchTargetsOptionsV1 {
            packet: pin.clone(),
            bank_spec: pin,
            opponents: Vec::new(),
            coefficient: 1.0,
            denominator: 32,
            allocation: e4.clone(),
            expected: json!({"run":"r","arm":"M","seed":1,"update":0,"allocation":e4}),
            target: SearchTargetKindV1::KeptSet,
            cap: SearchTargetCapV1::Off,
        };
        // Stage B's identity is unchanged.
        assert!(continuation_identity(&base).get("stage_b2").is_none());
        let m = SearchTargetsOptionsV1 {
            target: SearchTargetKindV1::CenteredWinFrequency,
            ..base.clone()
        };
        let capped = SearchTargetsOptionsV1 {
            cap: SearchTargetCapV1::Ratio { kappa: 1.0 },
            ..m.clone()
        };
        let other_kappa = SearchTargetsOptionsV1 {
            cap: SearchTargetCapV1::Ratio { kappa: 2.0 },
            ..m.clone()
        };
        let other_c = SearchTargetsOptionsV1 {
            coefficient: 2.0,
            ..capped.clone()
        };
        let identities: Vec<Value> = [&base, &m, &capped, &other_kappa, &other_c]
            .iter()
            .map(|o| continuation_identity(o))
            .collect();
        for i in 0..identities.len() {
            for j in 0..i {
                assert_ne!(identities[i], identities[j], "{i} {j}");
            }
        }
        // M targets need arm M or S; a cap must be positive and finite.
        let t_arm = SearchTargetsOptionsV1 {
            expected: json!({"run":"r","arm":"T","seed":1,"update":0,"allocation":e4}),
            ..m.clone()
        };
        assert!(t_arm.validate_v1().unwrap_err().contains("arm M or S"));
        let bad_cap = SearchTargetsOptionsV1 {
            cap: SearchTargetCapV1::Ratio { kappa: 0.0 },
            ..m
        };
        assert!(bad_cap.validate_v1().unwrap_err().contains("cap ratio"));
    }

    #[test]
    fn options_refuse_contradictory_allocations() {
        let pin = PinnedFileV1 {
            path: PathBuf::from("unused"),
            sha256: String::new(),
        };
        let e4 = json!({"initial":32000,"refresh":16000,"total":512000});
        let mut options = SearchTargetsOptionsV1 {
            packet: pin.clone(),
            bank_spec: pin,
            opponents: Vec::new(),
            coefficient: 1.0,
            denominator: 32,
            allocation: e4.clone(),
            expected: json!({"run":"r","arm":"T","seed":1,"update":0,
                "allocation":{"initial":8000,"refresh":4000,"total":128000}}),
            target: SearchTargetKindV1::KeptSet,
            cap: SearchTargetCapV1::Off,
        };
        let error = options.validate_v1().unwrap_err();
        assert!(
            error.contains("expectation and allocation differ"),
            "{error}"
        );
        options.expected["allocation"] = e4;
        let error = options.validate_v1().unwrap_err();
        assert!(error.contains("opponents"), "{error}");
    }

    #[test]
    fn kept_targets_renormalize_the_logits_over_the_keep_set() {
        let bits: Vec<u32> = [0.5_f32, -1.0, 2.0, 0.0]
            .iter()
            .map(|z| z.to_bits())
            .collect();
        let t = kept_target_v1(&bits, &[0, 2, 3]).unwrap();
        assert_eq!(t.probabilities[1], 0.0);
        assert_eq!(t.log_probabilities[1], f64::NEG_INFINITY);
        let total: f64 = t.probabilities.iter().sum();
        assert!((total - 1.0).abs() < 1e-12);
        let z = [0.5_f64, 2.0, 0.0];
        let e: Vec<f64> = z.iter().map(|x| (x - 2.0).exp()).collect();
        let s: f64 = e.iter().sum();
        for (j, &a) in [0usize, 2, 3].iter().enumerate() {
            assert!((t.probabilities[a] - e[j] / s).abs() < 1e-12);
        }
        // Keeping everything gives the student's own softmax: no gradient.
        let all = kept_target_v1(&bits, &[0, 1, 2, 3]).unwrap();
        let own = line_b_softmax_v1(&[0.5, -1.0, 2.0, 0.0]).unwrap();
        assert_eq!(all.probabilities, own.probabilities);
        for bad in [vec![], vec![2, 0], vec![1, 1], vec![4]] {
            assert!(kept_target_v1(&bits, &bad).is_err(), "{bad:?}");
        }
        assert!((removed_mass_v1(&bits, &[0, 2, 3]).unwrap() - own.probabilities[1]).abs() < 1e-15);
    }
}
