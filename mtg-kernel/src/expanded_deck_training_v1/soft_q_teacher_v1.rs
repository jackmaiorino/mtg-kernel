//! SQ1 soft rollout-Q teaching (collab LANES/soft-q-teaching-20261018,
//! design v3.1, section 4): one root per game of an update, valued by
//! coupled terminal rollouts on lawful sampled worlds, taught as a soft
//! target through the ordinary full-network GAE update.
//!
//! - **Root:** among the learner's Surface decisions at substep 0 with 2 to 8
//!   legal actions, the smallest line (b) rank under the slot's root seed
//!   (actor-visible key only). A game without one is an unavailable slot.
//! - **Worlds:** ordinal k of K = 16 is the first accepted Stage 4a sample
//!   (`stage4a::world::sample`) among attempts 0 to 3 of its seed, under E's
//!   public deck prior conditioned on the learner's complete visible prefix
//!   and the root (`e_supported::public_prior`); the true opponent deck is
//!   never read. Rejections depend on the visible state and the sample only.
//! - **Rollouts:** in each ordinal's world every legal action is taken from
//!   an independent clone and the game continues with the start-of-update
//!   student and the episode's opponent under their ordinary deployed
//!   samplers (line (b)'s continuation loop, without its unclamped sampler),
//!   all actions of an ordinal sharing world, chance and policy streams.
//! - **Census:** a root is available only if every ordinal has a world and
//!   every rollout ends naturally (all or nothing); a defect fails the update.
//! - **Target:** Q̂ is the mean signed return over the 16 ordinals;
//!   q = softmax(z + Q̂ / 0.25) with z the recorded collection logits. The
//!   shuffled-Q arm permutes Q̂ across the root's actions by a root-keyed
//!   uniform permutation first. The update adds forward KL(q ‖ p) with
//!   coefficient 0.1 over the update's scheduled slots.
//!
//! Every seed is keyed by the case (lineage, run seed, update, slot), never
//! by the arm, so the treatment and shuffled arms share worlds and streams.
use super::line_b_root_selection_v1::{
    line_b_learner_row_v1, line_b_root_eligible_v1, line_b_root_rank_v1,
};
use super::line_b_teacher_operator_v1::{
    continue_v1, run_indexed_v1, search_defect_v1, LineBRolloutOutcomeV1, ProgressV1,
    RolloutForksV1,
};
use super::*;
use crate::line_b_teacher_target_v1::line_b_teacher_target_v1;
use crate::native_policy_train_step_v1::{SoftTargetPolicyInputV1, SoftTargetPolicyRowV1};
use crate::regret_census_v1::stage4a::e_supported::{self, VisibleFrame};
use crate::regret_census_v1::stage4a::world::{self, DeckPrior};

pub const SOFT_Q_METHOD_V1: &str = "sq1-soft-rollout-q-v1-20261018";
pub(super) const SOFT_Q_ORDINALS_V1: u32 = 16;
pub(super) const SOFT_Q_WORLD_ATTEMPTS_V1: u32 = 4;
pub(super) const SOFT_Q_TEMPERATURE_V1: f64 = 0.25;
pub(super) const SOFT_Q_COEFFICIENT_V1: f64 = 0.1;
const MAX_TEACHER_WORKERS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftQArmV1 {
    Treatment,
    ShuffledQ,
}

/// One update's teacher: every trajectory of the batch is a slot, case
/// `{case_prefix}/slot{index}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoftQTeacherOptionsV1 {
    pub arm: SoftQArmV1,
    pub case_prefix: String,
    pub prior_deck_indices: Vec<usize>,
    /// Threads for rollout work items; records are identical for any count.
    pub teacher_workers: usize,
    /// Engineering check only (design section 6.2): the whole teacher runs
    /// and its packet is published, but no target enters the update, which
    /// must then equal the ordinary update bit for bit.
    #[serde(default, skip_serializing_if = "is_false")]
    pub engineering_zero_treatment: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn validate_prior(prior: &[usize]) -> Result<(), String> {
    ensure(
        !prior.is_empty()
            && prior.iter().all(|&i| i < crate::runtime_decks::RUNTIME_DECKS.len())
            && prior.iter().collect::<BTreeSet<_>>().len() == prior.len(),
        "invalid SQ1 public prior",
    )
}

impl SoftQTeacherOptionsV1 {
    pub fn validate_v1(&self) -> Result<(), String> {
        validate_prior(&self.prior_deck_indices)?;
        ensure(
            !self.case_prefix.is_empty() && self.case_prefix.len() <= 256,
            "SQ1 case prefix must be 1 to 256 bytes",
        )?;
        ensure(
            (1..=MAX_TEACHER_WORKERS).contains(&self.teacher_workers),
            "SQ1 teacher workers outside 1..64",
        )
    }
}

/// Bound into every SQ1 checkpoint; a continuing update or run must
/// reproduce it.
pub fn continuation_identity(options: &SoftQTeacherOptionsV1) -> Value {
    json!({"method":SOFT_Q_METHOD_V1,"arm":options.arm,
        "engineering_zero_treatment":options.engineering_zero_treatment,
        "prior_deck_indices":options.prior_deck_indices,
        "ordinals":SOFT_Q_ORDINALS_V1,"world_attempts":SOFT_Q_WORLD_ATTEMPTS_V1,
        "temperature":SOFT_Q_TEMPERATURE_V1,"coefficient":SOFT_Q_COEFFICIENT_V1,
        "runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256")})
}

/// The design's seed rule: SHA-256 of compact JSON with sorted keys, the
/// first eight bytes little-endian, masked to 63 bits.
pub(super) fn soft_q_seed_v1(fields: &[(&str, Value)]) -> u64 {
    let map: std::collections::BTreeMap<&str, &Value> =
        fields.iter().map(|(k, v)| (*k, v)).collect();
    let digest = Sha256::digest(serde_json::to_vec(&map).expect("seed key serializes"));
    let mut first = [0_u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(first) & (u64::MAX >> 1)
}

fn case_seed(case: &str, role: &str, extra: &[(&str, Value)]) -> u64 {
    let mut fields = vec![
        ("namespace", json!(SOFT_Q_METHOD_V1)),
        ("case", json!(case)),
        ("role", json!(role)),
    ];
    fields.extend(extra.iter().cloned());
    soft_q_seed_v1(&fields)
}

/// A selected root: the replayed session before its recorded action, the
/// learner's visible prefix and its identity.
pub(super) struct SoftQRootV1 {
    pub(super) decision_index: usize,
    pub(super) eligible: usize,
    pub(super) rank: [u8; 32],
    pub(super) session: FastActorSessionV1,
    pub(super) history: Vec<VisibleFrame>,
    pub(super) actor: u8,
}

/// Replays `trajectory`, collecting the learner's visible prefix, and returns
/// its root for `case`, or `None` without an eligible decision. A replay
/// disagreement is an error.
pub(super) fn select_root_v1(
    trajectory: &ExpandedTrajectoryV1,
    case: &str,
) -> Result<Option<SoftQRootV1>, String> {
    let episode = &trajectory.episode;
    ensure(
        episode.reset.is_none(),
        "SQ1 ordinary-start teaching cannot erase a reset prefix",
    )?;
    let mut session = episode_session_v1(episode, &episode.configurations()?)?;
    let root_seed = case_seed(case, "root", &[]);
    let mut history: Vec<VisibleFrame> = Vec::new();
    let mut best: Option<SoftQRootV1> = None;
    let mut eligible = 0_usize;
    for (index, row) in trajectory.decisions.iter().enumerate() {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            return Err("SQ1 replay ended before recorded row".into());
        };
        ensure(
            d.step == row.step
                && d.physical_decision_id == row.physical_decision_id
                && d.substep_index == row.substep_index
                && d.substep_count == row.substep_count
                && seat(d.acting_player) == row.actor
                && d.legal_action_count as usize == row.logits.len(),
            "SQ1 replay row mismatch",
        )?;
        if row.actor == episode.learner_seat
            && line_b_root_eligible_v1(&d, line_b_learner_row_v1(episode, row.actor))
        {
            eligible += 1;
            let key = session
                .kernel_search_visible_key_v4(0)
                .map_err(|error| format!("SQ1 visible key at row {index}: {error:?}"))?;
            let rank = line_b_root_rank_v1(root_seed, &key);
            if best
                .as_ref()
                .is_none_or(|b| (rank, index) < (b.rank, b.decision_index))
            {
                best = Some(SoftQRootV1 {
                    decision_index: index,
                    eligible: 0,
                    rank,
                    session: session.clone(),
                    history: history.clone(),
                    actor: row.actor,
                });
            }
        }
        if row.actor == episode.learner_seat {
            history.push(e_supported::visible_frame(&session, d, row.selected)?);
        }
        session
            .step(d.episode_id, d.step, row.selected)
            .map_err(err)?;
    }
    ensure(
        matches!(session.current_response(), FastActorResponseV1::Terminal(t) if t == trajectory.terminal),
        "SQ1 replay terminal mismatch",
    )?;
    Ok(best.map(|mut root| {
        root.eligible = eligible;
        root
    }))
}

/// One ordinal's census: the accepted world attempt and each action's
/// outcome, or the rejections when every attempt failed.
#[derive(Clone, Debug, Serialize)]
pub(super) struct SoftQOrdinalV1 {
    pub(super) ordinal: u32,
    /// Declared rejections of earlier attempts (all four when no world).
    pub(super) rejections: Vec<String>,
    pub(super) world_attempt: Option<u32>,
    pub(super) prior_deck: Option<usize>,
    /// Signed natural-terminal return per legal action; `None` = censored.
    pub(super) returns: Vec<Option<i8>>,
    pub(super) policy_steps: u64,
}

/// Seed domain of a root's ordinals: `construction` for teaching (and the
/// diagnostic's domain A), `assessment` for the diagnostic's domain B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SoftQDomainV1 {
    Construction,
    Assessment,
}

impl SoftQDomainV1 {
    fn world_role(self) -> &'static str {
        match self {
            Self::Construction => "world",
            Self::Assessment => "assessment_world",
        }
    }
    fn continuation_role(self) -> &'static str {
        match self {
            Self::Construction => "continuation",
            Self::Assessment => "assessment_continuation",
        }
    }
}

/// Stage 4a sampler errors that depend on the sample (declared rejections:
/// the ordinal's next attempt follows); every other kind is an instrument or
/// binding fault that fails the update.
pub(super) const SAMPLER_REJECTIONS_V1: &[&str] = &[
    "SampleHookRejected",
    "HiddenReferenceConflict",
    "SampleBoundaryChanged",
    "SampleCandidateRebuildFailed",
    "SampleBoundaryEncodingFailed",
    "LibraryChoicePlanFailed",
    "LibraryChoiceRebuildFailed",
    "LibraryChoiceOriginFailed",
    "DecisionLocalLibrary",
];

/// `world::sample` errors read `{kind:?}` or `{kind:?}: {hook detail}`.
pub(super) fn sampler_rejection_v1(error: &str) -> bool {
    let kind = error.split(':').next().unwrap_or("").trim();
    SAMPLER_REJECTIONS_V1.contains(&kind)
}

/// One work item: ordinal `ordinal` of a root.
pub(super) struct SoftQWorkV1<'a> {
    pub(super) root: &'a SoftQRootV1,
    pub(super) prior: &'a DeckPrior,
    pub(super) case: &'a str,
    pub(super) student: &'a FrozenPlayPolicyV1,
    pub(super) opponent: &'a FrozenPlayPolicyV1,
    pub(super) learner_seat: u8,
    pub(super) domain: SoftQDomainV1,
}

pub(super) fn ordinal_v1(work: &SoftQWorkV1<'_>, ordinal: u32) -> Result<SoftQOrdinalV1, String> {
    let FastActorResponseV1::Decision(decision) = work.root.session.current_response() else {
        return Err("SQ1 root session is not at a live decision".into());
    };
    let mut record = SoftQOrdinalV1 {
        ordinal,
        rejections: Vec::new(),
        world_attempt: None,
        prior_deck: None,
        returns: Vec::new(),
        policy_steps: 0,
    };
    let mut sample = None;
    for attempt in 0..SOFT_Q_WORLD_ATTEMPTS_V1 {
        let seed = case_seed(
            work.case,
            work.domain.world_role(),
            &[("ordinal", json!(ordinal)), ("attempt", json!(attempt))],
        );
        match world::sample(&work.root.session, seed, work.prior) {
            Ok(accepted) => {
                sample = Some((attempt, accepted));
                break;
            }
            Err(kind) if sampler_rejection_v1(&kind) => record.rejections.push(kind),
            Err(kind) => return Err(format!("SQ1 sampler fault at ordinal {ordinal}: {kind}")),
        }
    }
    let Some((attempt, sample)) = sample else {
        return Ok(record);
    };
    record.world_attempt = Some(attempt);
    record.prior_deck = Some(sample.prior_deck);
    let seeds = [0_u64, 1].map(|s| {
        case_seed(
            work.case,
            work.domain.continuation_role(),
            &[("ordinal", json!(ordinal)), ("seat", json!(s))],
        )
    });
    let mut forks = RolloutForksV1 {
        student: work.student.fork_for_collection_v3()?,
        opponent: Some(work.opponent.fork_for_collection_v3()?),
    };
    let expected = work
        .root
        .session
        .kernel_search_action_token_v4(decision)
        .map_err(|error| format!("{:?}", search_defect_v1("SQ1 root token", error)))?;
    for action in 0..decision.legal_action_count {
        let mut session = sample.world.clone();
        let token = session
            .kernel_search_action_token_v4(decision)
            .map_err(|error| format!("{:?}", search_defect_v1("SQ1 world token", error)))?;
        ensure(
            token == expected,
            "SQ1 world menu binding differs from the root",
        )?;
        session
            .kernel_search_consume_v4(decision, token, action)
            .map_err(|error| format!("{:?}", search_defect_v1("SQ1 root action", error)))?;
        let mut progress = ProgressV1::default();
        let outcome = continue_v1(
            &mut session,
            &mut forks,
            seeds,
            work.learner_seat,
            work.root.actor,
            &mut progress,
        );
        record.policy_steps += progress.steps;
        record.returns.push(match outcome {
            LineBRolloutOutcomeV1::Complete { terminal_return } => Some(terminal_return),
            LineBRolloutOutcomeV1::Censored(_) => None,
            LineBRolloutOutcomeV1::Defect(defect) => {
                return Err(format!(
                    "SQ1 defect at ordinal {ordinal} action {action}: {defect:?}"
                ))
            }
        });
    }
    Ok(record)
}

/// A root's census and, when available, its Q̂ and target.
#[derive(Clone, Debug, Serialize)]
pub(super) struct SoftQRootTargetV1 {
    pub(super) status: &'static str,
    pub(super) mean_returns: Option<Vec<f64>>,
    /// Applied permutation (shuffled arm): Q̂ used at action i is
    /// mean_returns[permutation[i]].
    pub(super) permutation: Option<Vec<usize>>,
    pub(super) target: Option<Vec<f64>>,
    #[serde(skip)]
    pub(super) softmax: Option<crate::line_b_teacher_target_v1::LineBSoftmaxV1>,
}

/// Uniform permutation of `n` from a root-keyed seed (Fisher-Yates, SplitMix64).
pub(super) fn shuffle_permutation_v1(case: &str, n: usize) -> Vec<usize> {
    let mut rng = crate::state::SplitMix64::seed(case_seed(case, "shuffle", &[]));
    let mut order: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

pub(super) fn root_target_v1(
    ordinals: &[SoftQOrdinalV1],
    logits: &[f32],
    arm: SoftQArmV1,
    case: &str,
) -> Result<SoftQRootTargetV1, String> {
    let n = logits.len();
    ensure(
        ordinals.len() == SOFT_Q_ORDINALS_V1 as usize,
        "SQ1 root census is incomplete",
    )?;
    let unavailable = |status| SoftQRootTargetV1 {
        status,
        mean_returns: None,
        permutation: None,
        target: None,
        softmax: None,
    };
    if ordinals.iter().any(|o| o.world_attempt.is_none()) {
        return Ok(unavailable("rejected"));
    }
    ensure(
        ordinals.iter().all(|o| o.returns.len() == n),
        "SQ1 root census width differs from the menu",
    )?;
    if ordinals.iter().any(|o| o.returns.iter().any(Option::is_none)) {
        return Ok(unavailable("censored"));
    }
    let mean: Vec<f64> = (0..n)
        .map(|a| {
            ordinals
                .iter()
                .map(|o| f64::from(o.returns[a].expect("complete")))
                .sum::<f64>()
                / f64::from(SOFT_Q_ORDINALS_V1)
        })
        .collect();
    let permutation = match arm {
        SoftQArmV1::Treatment => None,
        SoftQArmV1::ShuffledQ => Some(shuffle_permutation_v1(case, n)),
    };
    let used: Vec<f64> = match &permutation {
        None => mean.clone(),
        Some(p) => p.iter().map(|&i| mean[i]).collect(),
    };
    let z: Vec<f64> = logits.iter().map(|&v| f64::from(v)).collect();
    let target = line_b_teacher_target_v1(&z, &used, SOFT_Q_TEMPERATURE_V1)
        .map_err(|error| error.to_string())?;
    Ok(SoftQRootTargetV1 {
        status: "available",
        mean_returns: Some(mean),
        permutation,
        target: Some(target.probabilities.clone()),
        softmax: Some(target),
    })
}

/// The deployed sampler's action distribution (exact masses of 2^64).
pub(super) fn deployed_masses_v1(logits: &[f32]) -> Result<Vec<String>, String> {
    let mut sampler = WideCategoricalScratchV1::default();
    Ok(sampler
        .apportion(logits)
        .map_err(err)?
        .iter()
        .map(|m| m.to_string())
        .collect())
}

/// Opponents loaded once per source within a call.
#[derive(Default)]
struct OpponentsV1 {
    loaded: Vec<(String, FrozenPlayPolicyV1, ExpandedInferenceIdentityV1)>,
}

impl OpponentsV1 {
    fn get(
        &mut self,
        trajectory: &ExpandedTrajectoryV1,
    ) -> Result<(usize, ExpandedInferenceIdentityV1), String> {
        let episode = &trajectory.episode;
        let source = episode
            .opponent
            .as_ref()
            .ok_or("SQ1 requires a frozen episode opponent")?;
        let key = serde_json::to_string(source).map_err(err)?;
        let index = match self.loaded.iter().position(|(k, _, _)| *k == key) {
            Some(index) => index,
            None => {
                let (policy, identity) = load_expanded_inference_v1(source)?;
                self.loaded.push((key, policy, identity));
                self.loaded.len() - 1
            }
        };
        let recorded = trajectory
            .seat_behaviors
            .as_ref()
            .ok_or("SQ1 opponent behavior identity missing")?;
        ensure(
            recorded[1 - usize::from(episode.learner_seat)].identity == self.loaded[index].2,
            "SQ1 frozen opponent identity mismatch",
        )?;
        Ok((index, self.loaded[index].2.clone()))
    }
}

/// Checks the student's live input at the root against the saved row.
fn bind_root_input_v1(
    trajectory: &ExpandedTrajectoryV1,
    root: &SoftQRootV1,
    student: &FrozenPlayPolicyV1,
) -> Result<Vec<u8>, String> {
    let row = &trajectory.decisions[root.decision_index];
    let mut capture = student.fork_for_collection_v3()?;
    capture.reset_sampling_v1([0, 0]);
    let (scores, tensor) = match trajectory_generation_v1(trajectory) {
        FreshLineageGenerationV1::V3 => {
            let (_, scores, tensor) = capture.select_with_training_tensor_v3(&root.session)?;
            (scores, TensorBitsV1::from_tensor(&tensor.common))
        }
        FreshLineageGenerationV1::V4 => {
            let (_, scores, tensor) = capture.select_with_training_tensor_v4(&root.session)?;
            (scores, TensorBitsV1::from_tensor(&tensor.common))
        }
    };
    let bytes = serde_json::to_vec(&tensor).map_err(err)?;
    ensure(
        bytes == serde_json::to_vec(&row.tensor).map_err(err)?,
        "SQ1 live encoded input differs from the learner row",
    )?;
    // The target and the deployed masses are built from the recorded logits:
    // they must be the student's own outputs at the root, bit for bit.
    ensure(
        scores.logits.iter().map(|z| z.to_bits()).collect::<Vec<_>>() == row.logits
            && scores.value.to_bits() == row.value,
        "SQ1 recorded root outputs differ from the student's forward",
    )?;
    Ok(bytes)
}

/// The update's teach step: one root per trajectory, all roots' ordinals on
/// `teacher_workers` threads, records in slot and ordinal order.
pub(super) fn teach(
    trajectories: &[ExpandedTrajectoryV1],
    student: &FrozenPlayPolicyV1,
    student_hash: &str,
    groups: &[LearnerTensorGroupV1<'_>],
    options: &SoftQTeacherOptionsV1,
) -> Result<(Value, SoftTargetPolicyInputV1), String> {
    options.validate_v1()?;
    let mut opponents = OpponentsV1::default();
    struct Slot {
        case: String,
        root: Option<SoftQRootV1>,
        prior: Option<DeckPrior>,
        opponent: Option<usize>,
        opponent_identity: Option<ExpandedInferenceIdentityV1>,
        input_sha256: Option<String>,
    }
    let mut slots = Vec::with_capacity(trajectories.len());
    for (index, trajectory) in trajectories.iter().enumerate() {
        ensure(
            trajectory.learner_sampler.is_none(),
            "SQ1 requires the unchanged deployment collection sampler",
        )?;
        let case = format!("{}/slot{index}", options.case_prefix);
        let root = select_root_v1(trajectory, &case)?;
        let (prior, opponent, opponent_identity, input_sha256) = match &root {
            None => (None, None, None, None),
            Some(root) => {
                let bytes = bind_root_input_v1(trajectory, root, student)?;
                let FastActorResponseV1::Decision(d) = root.session.current_response() else {
                    unreachable!("selected roots are live decisions")
                };
                let prior = e_supported::public_prior(
                    &root.session,
                    d,
                    &root.history,
                    &options.prior_deck_indices,
                )?;
                let (opponent, identity) = opponents.get(trajectory)?;
                (
                    Some(prior),
                    Some(opponent),
                    Some(identity),
                    Some(hex(&Sha256::digest(&bytes))),
                )
            }
        };
        slots.push(Slot {
            case,
            root,
            prior,
            opponent,
            opponent_identity,
            input_sha256,
        });
    }
    let items: Vec<(usize, u32)> = slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.root.is_some())
        .flat_map(|(i, _)| (0..SOFT_Q_ORDINALS_V1).map(move |k| (i, k)))
        .collect();
    let results = run_indexed_v1(items.len(), options.teacher_workers, |item| {
        let (i, k) = items[item];
        let slot = &slots[i];
        let work = SoftQWorkV1 {
            root: slot.root.as_ref().expect("filtered"),
            prior: slot.prior.as_ref().expect("filtered"),
            case: &slot.case,
            student,
            opponent: &opponents.loaded[slot.opponent.expect("filtered")].1,
            learner_seat: trajectories[i].episode.learner_seat,
            domain: SoftQDomainV1::Construction,
        };
        ordinal_v1(&work, k)
    })?;
    let mut results = results.into_iter();
    let mut rows = Vec::new();
    let mut packets = Vec::new();
    let mut available = 0_usize;
    for (index, slot) in slots.iter().enumerate() {
        let trajectory = &trajectories[index];
        let Some(root) = &slot.root else {
            packets.push(json!({"case":slot.case,"trajectory_index":index,
                "status":"no_eligible_root","learner_deck":trajectory.episode.selected
                    [usize::from(trajectory.episode.learner_seat)].label}));
            continue;
        };
        let mut ordinals = Vec::with_capacity(SOFT_Q_ORDINALS_V1 as usize);
        for _ in 0..SOFT_Q_ORDINALS_V1 {
            ordinals.push(results.next().ok_or("SQ1 ordinal missing")??);
        }
        let row = &trajectory.decisions[root.decision_index];
        let logits: Vec<f32> = row.logits.iter().map(|&b| f32::from_bits(b)).collect();
        ensure(
            logits.iter().all(|z| z.is_finite()),
            "SQ1 nonfinite student logits",
        )?;
        let target = root_target_v1(&ordinals, &logits, options.arm, &slot.case)?;
        if let Some(softmax) = &target.softmax {
            available += 1;
            let mut matches = groups
                .iter()
                .enumerate()
                .flat_map(|(g, (_, ordinal, steps))| {
                    steps
                        .iter()
                        .enumerate()
                        .filter(move |(_, (saved, _))| {
                            *ordinal == index && saved.step == row.step
                        })
                        .map(move |(s, (_, tensor))| (g, s, tensor))
                });
            let (group_index, substep_index, tensor) =
                matches.next().ok_or("SQ1 root missing from learner groups")?;
            ensure(
                matches.next().is_none(),
                "SQ1 root ambiguously bound to learner groups",
            )?;
            ensure(
                slot.input_sha256.as_deref()
                    == Some(hex(&Sha256::digest(
                        serde_json::to_vec(&TensorBitsV1::from_tensor(tensor)).map_err(err)?,
                    )))
                    .as_deref(),
                "SQ1 input differs from the prepared forward row",
            )?;
            rows.push(SoftTargetPolicyRowV1 {
                group_index,
                substep_index,
                target: softmax.clone(),
            });
        }
        let FastActorResponseV1::Decision(d) = root.session.current_response() else {
            unreachable!()
        };
        packets.push(json!({"case":slot.case,"trajectory_index":index,"status":target.status,
            "learner_deck":trajectory.episode.selected[usize::from(trajectory.episode.learner_seat)].label,
            "decision_index":root.decision_index,"eligible":root.eligible,"rank":hex(&root.rank),
            "decision":{"step":d.step,"physical_decision_id":d.physical_decision_id,
                "legal_width":d.legal_action_count,"actor":root.actor},
            "temporal_fraction":root.decision_index as f64 / trajectory.decisions.len() as f64,
            "actual_student_action":row.selected,"collection_logits":row.logits,
            "deployed_masses":deployed_masses_v1(&logits)?,
            "encoded_input_sha256":slot.input_sha256,"opponent_identity":slot.opponent_identity,
            "mean_returns":target.mean_returns,"permutation":target.permutation,
            "target":target.target,"ordinals":ordinals}));
    }
    ensure(results.next().is_none(), "SQ1 ordinal results left over")?;
    let packet = json!({"schema":"mtg-kernel-soft-q-teacher-packet/v1","method":SOFT_Q_METHOD_V1,
        "runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "student_state_sha256":student_hash,"student_identity":student.identity_v1(),
        "options":options,"continuation":continuation_identity(options),
        "scheduled":trajectories.len(),"available":available,"slots":packets});
    Ok((
        packet,
        SoftTargetPolicyInputV1 {
            scheduled: trajectories.len(),
            rows: if options.engineering_zero_treatment {
                Vec::new()
            } else {
                rows
            },
            coefficient: SOFT_Q_COEFFICIENT_V1,
        },
    ))
}

/// One diagnostic case: a source game of the parent and its case key.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoftQDiagnosticCaseV1 {
    pub case: String,
    pub trajectory: PinnedFileV1,
}

/// Design section 6.1: at each case's root, the construction target from
/// seed domain A and an independent assessment census from domain B, with
/// the deployed masses and logits. Readings are computed offline.
pub(super) fn diagnostic(
    parent: &ExpandedModelSourceV1,
    prior_deck_indices: &[usize],
    teacher_workers: usize,
    cases: &[SoftQDiagnosticCaseV1],
    output_directory: &Path,
) -> Result<Value, String> {
    validate_prior(prior_deck_indices)?;
    ensure(
        (1..=MAX_TEACHER_WORKERS).contains(&teacher_workers),
        "SQ1 teacher workers outside 1..64",
    )?;
    ensure(
        !cases.is_empty() && cases.len() <= 4096,
        "SQ1 diagnostic case count outside 1..4096",
    )?;
    ensure(
        !output_directory.exists(),
        "SQ1 diagnostic output directory must be fresh",
    )?;
    let (student, identity) = load_expanded_inference_v1(parent)?;
    ensure(
        student.collection_sampler_identity_v1().is_none(),
        "SQ1 diagnostic requires the unchanged deployment sampler",
    )?;
    let mut opponents = OpponentsV1::default();
    let mut ids = BTreeSet::new();
    let mut prepared = Vec::new();
    for case in cases {
        ensure(
            !case.case.is_empty() && ids.insert(&case.case),
            "duplicate or empty SQ1 diagnostic case",
        )?;
        let trajectory: ExpandedTrajectoryV1 = read_pinned(&case.trajectory)?;
        validate_trajectory(&trajectory)?;
        ensure(
            trajectory.learner_sampler.is_none(),
            "SQ1 diagnostic requires the unchanged deployment collection sampler",
        )?;
        let recorded = trajectory
            .seat_behaviors
            .as_ref()
            .ok_or("SQ1 diagnostic learner behavior identity missing")?;
        ensure(
            recorded[usize::from(trajectory.episode.learner_seat)].identity == identity,
            "SQ1 diagnostic source game was not played by the parent",
        )?;
        let root = select_root_v1(&trajectory, &case.case)?;
        let (prior, opponent) = match &root {
            None => (None, None),
            Some(root) => {
                bind_root_input_v1(&trajectory, root, &student)?;
                let FastActorResponseV1::Decision(d) = root.session.current_response() else {
                    unreachable!()
                };
                (
                    Some(e_supported::public_prior(
                        &root.session,
                        d,
                        &root.history,
                        prior_deck_indices,
                    )?),
                    Some(opponents.get(&trajectory)?.0),
                )
            }
        };
        prepared.push((trajectory, root, prior, opponent));
    }
    let items: Vec<(usize, SoftQDomainV1, u32)> = prepared
        .iter()
        .enumerate()
        .filter(|(_, p)| p.1.is_some())
        .flat_map(|(i, _)| {
            [SoftQDomainV1::Construction, SoftQDomainV1::Assessment]
                .into_iter()
                .flat_map(move |domain| (0..SOFT_Q_ORDINALS_V1).map(move |k| (i, domain, k)))
        })
        .collect();
    let results = run_indexed_v1(items.len(), teacher_workers, |item| {
        let (i, domain, k) = items[item];
        let (trajectory, root, prior, opponent) = &prepared[i];
        let work = SoftQWorkV1 {
            root: root.as_ref().expect("filtered"),
            prior: prior.as_ref().expect("filtered"),
            case: &cases[i].case,
            student: &student,
            opponent: &opponents.loaded[opponent.expect("filtered")].1,
            learner_seat: trajectory.episode.learner_seat,
            domain,
        };
        ordinal_v1(&work, k)
    })?;
    let mut results = results.into_iter();
    let mut rows = Vec::new();
    for (i, (trajectory, root, _, _)) in prepared.iter().enumerate() {
        let episode = &trajectory.episode;
        let base = json!({"case":cases[i].case,"trajectory":cases[i].trajectory,
            "source_game":{"episode_id":episode.id,"seed":episode.seed,
                "learner_seat":episode.learner_seat,"decks":episode.selected}});
        let Some(root) = root else {
            let mut row = base;
            row["status"] = json!("no_eligible_root");
            rows.push(row);
            continue;
        };
        let mut construction = Vec::with_capacity(SOFT_Q_ORDINALS_V1 as usize);
        let mut assessment = Vec::with_capacity(SOFT_Q_ORDINALS_V1 as usize);
        for into in [&mut construction, &mut assessment] {
            for _ in 0..SOFT_Q_ORDINALS_V1 {
                into.push(results.next().ok_or("SQ1 diagnostic ordinal missing")??);
            }
        }
        let saved = &trajectory.decisions[root.decision_index];
        let logits: Vec<f32> = saved.logits.iter().map(|&b| f32::from_bits(b)).collect();
        let target = root_target_v1(&construction, &logits, SoftQArmV1::Treatment, &cases[i].case)?;
        let mut row = base;
        row["status"] = json!(target.status);
        row["decision_index"] = json!(root.decision_index);
        row["eligible"] = json!(root.eligible);
        row["legal_width"] = json!(logits.len());
        row["logits_bits"] = json!(saved.logits);
        row["deployed_masses"] = json!(deployed_masses_v1(&logits)?);
        row["construction_mean_returns"] = json!(target.mean_returns);
        row["target"] = json!(target.target);
        row["construction"] = json!(construction);
        row["assessment"] = json!(assessment);
        rows.push(row);
    }
    ensure(results.next().is_none(), "SQ1 diagnostic results left over")?;
    let result = json!({"schema":"mtg-kernel-soft-q-diagnostic/v1","method":SOFT_Q_METHOD_V1,
        "complete":true,"parent":parent,"parent_identity":identity,
        "prior_deck_indices":prior_deck_indices,"ordinals":SOFT_Q_ORDINALS_V1,
        "world_attempts":SOFT_Q_WORLD_ATTEMPTS_V1,"temperature":SOFT_Q_TEMPERATURE_V1,
        "cases":rows,"runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256")});
    fs::create_dir_all(output_directory).map_err(err)?;
    publish_json(output_directory, "diagnostic.json", &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
