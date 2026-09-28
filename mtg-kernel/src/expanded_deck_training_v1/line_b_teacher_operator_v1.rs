//! G115 line (b) coupled terminal action search (design entry section 4;
//! CODEX 11:23 dispositions items 3 and 4; FABLE-REVIEW-20260927 exit-teacher
//! changes 1 and 5; director ruling of 2026-09-27 on the availability-aware
//! census; line (b) change 3, the learner-seat rollout sampler pin).
//!
//! For each ordinal `k < K` the root is redeterminized once under the world
//! seed `s_k` (the V4 whole-object determinization in `FutureChanceV3` mode:
//! unseen hidden placement and all future engine randomness replaced), and for
//! each legal action an independent clone of that world takes the action
//! through fresh, token-checked action authority. The game then continues
//! with fresh policy forks: the start-of-update student with the line (b)
//! collection sampler on learner rows and the episode's frozen opponent with
//! its production sampler, both on the ordinal's seat streams. All actions at
//! one ordinal share world, future chance and policy streams and differ only
//! in the root action; ordinals are independent. Seeds read only the teacher
//! seed, the actor-visible key and `k`, never an action index or hidden
//! content.
//!
//! Census: every rollout of a root runs. A natural terminal gives the root
//! actor's return; cap exhaustion (the rollout caps or the inherited episode
//! headroom) or an environmental interruption is Censored and disables the
//! root's auxiliary while the root stays in the denominator; an unsupported
//! branch, invalid state or operator error is a Defect and fails the update.
//! Work items (root, ordinal) run on a worker pool and are reduced in index
//! order, so the records are identical for any worker count.

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::line_b_root_selection_v1::LineBRootV1;
use super::{err, seat};
use crate::line_b_teacher_target_v1::{line_b_mean_return_v1, line_b_teacher_target_v1};
use crate::rl::TerminalClassificationV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1, V4SearchSampleMode};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::unclamped_softmax_sampler_v1::UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1;

pub(super) const LINE_B_TEACHER_OPERATOR_VERSION_V1: &str = "line-b-coupled-terminal-search-v1";
pub(super) const LINE_B_WORLD_DOMAIN_V1: &[u8] = b"mtg-kernel/line-b-teacher-world/v1\0";
pub(super) const LINE_B_POLICY_DOMAIN_V1: &[u8] = b"mtg-kernel/line-b-teacher-policy/v1\0";
/// Per-rollout caps on top of the inherited episode headroom.
pub(super) const LINE_B_ROLLOUT_MAX_PHYSICAL_V1: u64 = 40_000;
pub(super) const LINE_B_ROLLOUT_MAX_POLICY_STEPS_V1: u64 = 80_000;

fn first_eight_le(digest: &[u8]) -> u64 {
    let mut first = [0_u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(first)
}

/// `s_k`: LE-first-eight SHA-256(world domain || teacher seed LE || visible
/// key || k LE).
pub(super) fn line_b_world_seed_v1(teacher_seed: u64, visible_key: &[u8; 32], ordinal: u64) -> u64 {
    let mut hash = Sha256::new();
    hash.update(LINE_B_WORLD_DOMAIN_V1);
    hash.update(teacher_seed.to_le_bytes());
    hash.update(visible_key);
    hash.update(ordinal.to_le_bytes());
    first_eight_le(&hash.finalize())
}

/// Seat streams for ordinal `k`: the policy domain, the same fields and the
/// seat as u64 LE.
pub(super) fn line_b_policy_seeds_v1(
    teacher_seed: u64,
    visible_key: &[u8; 32],
    ordinal: u64,
) -> [u64; 2] {
    [0_u64, 1].map(|seat| {
        let mut hash = Sha256::new();
        hash.update(LINE_B_POLICY_DOMAIN_V1);
        hash.update(teacher_seed.to_le_bytes());
        hash.update(visible_key);
        hash.update(ordinal.to_le_bytes());
        hash.update(seat.to_le_bytes());
        first_eight_le(&hash.finalize())
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LineBCensorV1 {
    /// A rollout cap or the inherited episode headroom ran out.
    CapExhausted,
    /// Reserved for a driver; no in-process event produces it (review change
    /// 11 of FABLE-REVIEW-20260928). Inside the operator every failure is
    /// typed: a truncation or cap is `CapExhausted`, and a halted state, a
    /// step or policy error or a worker panic (joined as a step error) is a
    /// `LineBDefectV1`, which fails the update. An interruption of the host
    /// or process ends the update without a packet; the launch path's rule
    /// for a failed update (doc section 4) records it in the run's census,
    /// not in a packet.
    EnvironmentalInterruption,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LineBDefectV1 {
    UnsupportedBranch(String),
    InvalidState(String),
    OperatorError(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LineBRolloutOutcomeV1 {
    Complete { terminal_return: i8 },
    Censored(LineBCensorV1),
    Defect(LineBDefectV1),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct LineBRolloutRecordV1 {
    pub(super) ordinal: u32,
    pub(super) action: u32,
    pub(super) outcome: LineBRolloutOutcomeV1,
    /// Physical decisions and policy steps after the root action.
    pub(super) physical_decisions: u64,
    pub(super) policy_steps: u64,
    /// `diagnostic_state_hash` of the final session (replay receipts).
    pub(super) final_state_hash: u64,
}

/// The start-of-update student and the episode's frozen opponent (`None` in
/// self-play, where the student plays both seats).
pub(super) struct LineBRolloutPoliciesV1<'a> {
    pub(super) student: &'a FrozenPlayPolicyV1,
    pub(super) opponent: Option<&'a FrozenPlayPolicyV1>,
    pub(super) learner_seat: u8,
}

/// One selected root and what its rollouts need.
pub(super) struct LineBTeacherRootInputV1<'a> {
    pub(super) root: &'a LineBRootV1,
    /// The game's `/rollout` production seed (or an engineering seed).
    pub(super) teacher_seed: u64,
    /// The trajectory's recorded learner sampler: must be the unclamped one.
    pub(super) learner_sampler: Option<&'a str>,
    pub(super) policies: LineBRolloutPoliciesV1<'a>,
}

struct RolloutForksV1 {
    student: FrozenPlayPolicyV1,
    opponent: Option<FrozenPlayPolicyV1>,
}

fn forks_v1(policies: &LineBRolloutPoliciesV1<'_>) -> Result<RolloutForksV1, String> {
    let mut student = policies.student.fork_for_collection_v3()?;
    student.enable_unclamped_collection_sampler_v1();
    let opponent = policies
        .opponent
        .map(|other| {
            other
                .fork_for_collection_v3()
                .map(FrozenPlayPolicyV1::with_legacy_collection_sampler_v1)
        })
        .transpose()?;
    Ok(RolloutForksV1 { student, opponent })
}

fn search_defect_v1(context: &str, error: impl core::fmt::Debug) -> LineBDefectV1 {
    let text = format!("{context}: {error:?}");
    // Binding and liveness failures are invalid states; the rest are
    // determinization or library-choice branches the sampler does not support.
    if [
        "InvalidVisibleBinding",
        "NoLiveDecision",
        "UnsupportedActionContract",
        "StepFailed",
    ]
    .iter()
    .any(|name| text.ends_with(name))
    {
        LineBDefectV1::InvalidState(text)
    } else {
        LineBDefectV1::UnsupportedBranch(text)
    }
}

#[derive(Default)]
struct ProgressV1 {
    physical: u64,
    steps: u64,
}

/// Continue a world after its root action to a terminal or a cap. Every
/// rollout starts its seat streams afresh (no state from the collected game
/// or an earlier rollout).
fn continue_v1(
    session: &mut FastActorSessionV1,
    forks: &mut RolloutForksV1,
    seeds: [u64; 2],
    learner_seat: u8,
    root_actor: u8,
    progress: &mut ProgressV1,
) -> LineBRolloutOutcomeV1 {
    forks.student.reset_sampling_v1(seeds);
    if let Some(other) = forks.opponent.as_mut() {
        other.reset_sampling_v1(seeds);
    }
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                return match terminal.terminal_classification {
                    TerminalClassificationV1::Natural => {
                        match i8::try_from(terminal.terminal_reward[root_actor as usize]) {
                            Ok(value) if (-1..=1).contains(&value) => {
                                LineBRolloutOutcomeV1::Complete {
                                    terminal_return: value,
                                }
                            }
                            _ => LineBRolloutOutcomeV1::Defect(LineBDefectV1::InvalidState(
                                "natural terminal reward outside {-1, 0, 1}".into(),
                            )),
                        }
                    }
                    TerminalClassificationV1::Truncated => {
                        LineBRolloutOutcomeV1::Censored(LineBCensorV1::CapExhausted)
                    }
                    TerminalClassificationV1::Halted => {
                        LineBRolloutOutcomeV1::Defect(LineBDefectV1::InvalidState(format!(
                            "engine halted: {}",
                            terminal.terminal_reason
                        )))
                    }
                };
            }
            FastActorResponseV1::Decision(decision) => {
                if decision.substep_index == 0 {
                    if progress.physical == LINE_B_ROLLOUT_MAX_PHYSICAL_V1 {
                        return LineBRolloutOutcomeV1::Censored(LineBCensorV1::CapExhausted);
                    }
                    progress.physical += 1;
                }
                if progress.steps == LINE_B_ROLLOUT_MAX_POLICY_STEPS_V1 {
                    return LineBRolloutOutcomeV1::Censored(LineBCensorV1::CapExhausted);
                }
                progress.steps += 1;
                let actor = seat(decision.acting_player);
                let policy = match (&mut forks.opponent, actor == learner_seat) {
                    (Some(other), false) => other,
                    _ => &mut forks.student,
                };
                let selected = match policy.select_fast_session_v1(session) {
                    Ok(selected) => selected,
                    Err(error) => {
                        return LineBRolloutOutcomeV1::Defect(LineBDefectV1::OperatorError(error))
                    }
                };
                if let Err(error) = session.step(decision.episode_id, decision.step, selected) {
                    return LineBRolloutOutcomeV1::Defect(LineBDefectV1::InvalidState(err(error)));
                }
            }
        }
    }
}

/// The ordinal's world: one `FutureChanceV3` redeterminization of the root.
fn world_v1(
    root: &LineBRootV1,
    teacher_seed: u64,
    ordinal: u32,
    mode: V4SearchSampleMode,
) -> Result<FastActorSessionV1, LineBDefectV1> {
    let world_seed = line_b_world_seed_v1(teacher_seed, &root.visible_key, ordinal.into());
    root.session
        .kernel_search_redeterminized_clone_mode_v4(world_seed, mode)
        .map_err(|error| search_defect_v1("world", error))
}

/// One rollout: an independent clone of the ordinal's world takes `action`
/// through fresh action authority bound to the root's menu, then continues.
fn rollout_v1(
    root: &LineBRootV1,
    world: &Result<FastActorSessionV1, LineBDefectV1>,
    ordinal: u32,
    action: u32,
    forks: &mut RolloutForksV1,
    seeds: [u64; 2],
    learner_seat: u8,
) -> LineBRolloutRecordV1 {
    let mut progress = ProgressV1::default();
    let (outcome, final_state_hash) = match (root.session.current_response(), world) {
        (FastActorResponseV1::Decision(decision), Ok(world)) => {
            let mut session = world.clone();
            let outcome = match (
                root.session.kernel_search_action_token_v4(decision),
                session.kernel_search_action_token_v4(decision),
            ) {
                (Err(error), _) => {
                    LineBRolloutOutcomeV1::Defect(search_defect_v1("root token", error))
                }
                (_, Err(error)) => {
                    LineBRolloutOutcomeV1::Defect(search_defect_v1("world token", error))
                }
                (Ok(expected), Ok(token)) if token != expected => LineBRolloutOutcomeV1::Defect(
                    LineBDefectV1::InvalidState("world menu binding differs from the root".into()),
                ),
                (Ok(_), Ok(token)) => {
                    match session.kernel_search_consume_v4(decision, token, action) {
                        Err(error) => {
                            LineBRolloutOutcomeV1::Defect(search_defect_v1("root action", error))
                        }
                        Ok(_) => continue_v1(
                            &mut session,
                            forks,
                            seeds,
                            learner_seat,
                            root.actor,
                            &mut progress,
                        ),
                    }
                }
            };
            (outcome, session.diagnostic_state_hash())
        }
        (FastActorResponseV1::Decision(_), Err(defect)) => (
            LineBRolloutOutcomeV1::Defect(defect.clone()),
            root.session.diagnostic_state_hash(),
        ),
        (FastActorResponseV1::Terminal(_), _) => (
            LineBRolloutOutcomeV1::Defect(LineBDefectV1::OperatorError(
                "line (b) root session is not at a live decision".into(),
            )),
            root.session.diagnostic_state_hash(),
        ),
    };
    LineBRolloutRecordV1 {
        ordinal,
        action,
        outcome,
        physical_decisions: progress.physical,
        policy_steps: progress.steps,
        final_state_hash,
    }
}

/// The `n` rollouts of one ordinal, one per legal action, in action order.
/// One fork pair serves the ordinal; `continue_v1` restarts its streams for
/// every rollout, so each behaves as a fresh fork (tested).
fn ordinal_rollouts_v1(
    input: &LineBTeacherRootInputV1<'_>,
    ordinal: u32,
    mode: V4SearchSampleMode,
) -> Result<Vec<LineBRolloutRecordV1>, String> {
    let FastActorResponseV1::Decision(decision) = input.root.session.current_response() else {
        return Err("line (b) root session is not at a live decision".into());
    };
    let world = world_v1(input.root, input.teacher_seed, ordinal, mode);
    let seeds = line_b_policy_seeds_v1(input.teacher_seed, &input.root.visible_key, ordinal.into());
    let mut forks = forks_v1(&input.policies)?;
    Ok((0..decision.legal_action_count)
        .map(|action| {
            rollout_v1(
                input.root,
                &world,
                ordinal,
                action,
                &mut forks,
                seeds,
                input.policies.learner_seat,
            )
        })
        .collect())
}

/// One rollout with brand-new forks (test reference for fork reuse).
#[cfg(test)]
pub(super) fn line_b_rollout_with_new_forks_v1(
    input: &LineBTeacherRootInputV1<'_>,
    ordinal: u32,
    action: u32,
) -> Result<LineBRolloutRecordV1, String> {
    let world = world_v1(
        input.root,
        input.teacher_seed,
        ordinal,
        V4SearchSampleMode::FutureChanceV3,
    );
    let seeds = line_b_policy_seeds_v1(input.teacher_seed, &input.root.visible_key, ordinal.into());
    let mut forks = forks_v1(&input.policies)?;
    Ok(rollout_v1(
        input.root,
        &world,
        ordinal,
        action,
        &mut forks,
        seeds,
        input.policies.learner_seat,
    ))
}

/// Runs `task(0..count)` on `workers` scoped threads (index-strided) and
/// returns the results in index order.
fn run_indexed_v1<T: Send>(
    count: usize,
    workers: usize,
    task: impl Fn(usize) -> T + Sync,
) -> Result<Vec<T>, String> {
    if workers <= 1 || count <= 1 {
        return Ok((0..count).map(&task).collect());
    }
    let workers = workers.min(count);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            let task = &task;
            let handle = std::thread::Builder::new()
                .stack_size(16 * 1024 * 1024)
                .spawn_scoped(scope, move || {
                    (worker..count)
                        .step_by(workers)
                        .map(|index| (index, task(index)))
                        .collect::<Vec<_>>()
                })
                .map_err(err)?;
            handles.push(handle);
        }
        let mut slots: Vec<Option<T>> = (0..count).map(|_| None).collect();
        for handle in handles {
            let done = handle
                .join()
                .map_err(|_| "line (b) rollout worker panicked".to_string())?;
            for (index, value) in done {
                slots[index] = Some(value);
            }
        }
        slots
            .into_iter()
            .map(|slot| slot.ok_or_else(|| "line (b) rollout slot missing".to_string()))
            .collect()
    })
}

/// All `K x n` rollouts of every root: work items (root, ordinal) on
/// `workers` threads; each root's records in (ordinal, action) order.
pub(super) fn line_b_teacher_rollouts_v1(
    inputs: &[LineBTeacherRootInputV1<'_>],
    rollouts: u32,
    workers: usize,
) -> Result<Vec<Vec<LineBRolloutRecordV1>>, String> {
    rollouts_with_mode_v1(
        inputs,
        rollouts,
        workers,
        V4SearchSampleMode::FutureChanceV3,
    )
}

/// The same rollouts under another sample mode: the Legacy power check of the
/// invariance tests (Legacy keeps the true engine randomness).
#[cfg(test)]
pub(super) fn line_b_teacher_rollouts_mode_v1(
    inputs: &[LineBTeacherRootInputV1<'_>],
    rollouts: u32,
    workers: usize,
    mode: V4SearchSampleMode,
) -> Result<Vec<Vec<LineBRolloutRecordV1>>, String> {
    rollouts_with_mode_v1(inputs, rollouts, workers, mode)
}

fn rollouts_with_mode_v1(
    inputs: &[LineBTeacherRootInputV1<'_>],
    rollouts: u32,
    workers: usize,
    mode: V4SearchSampleMode,
) -> Result<Vec<Vec<LineBRolloutRecordV1>>, String> {
    for input in inputs {
        // Line (b) change 3: rollouts continue the student with the sampler
        // it collected with, which must be the unclamped one.
        if input.learner_sampler != Some(UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1) {
            return Err("line (b) teacher requires unclamped learner collection".into());
        }
    }
    let per_root = rollouts as usize;
    let items = run_indexed_v1(inputs.len() * per_root, workers, |index| {
        ordinal_rollouts_v1(&inputs[index / per_root], (index % per_root) as u32, mode)
    })?;
    let mut roots = Vec::with_capacity(inputs.len());
    let mut items = items.into_iter();
    for _ in inputs {
        let mut records = Vec::new();
        for _ in 0..per_root {
            records.extend(items.next().ok_or("line (b) rollout item missing")??);
        }
        roots.push(records);
    }
    Ok(roots)
}

/// A root's census outcome.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LineBRootStatusV1 {
    /// Every rollout ended naturally: the frozen target.
    Complete {
        mean_returns: Vec<f64>,
        log_target: Vec<f64>,
    },
    /// At least one censored rollout and no defect: the auxiliary is
    /// disabled for this root, which stays in the denominator.
    Censored { censored_rollouts: usize },
}

/// Census and target for one root. Any defect fails (zero tolerance).
pub(super) fn line_b_root_status_v1(
    records: &[LineBRolloutRecordV1],
    rollouts: u32,
    collection_logits: &[f32],
    temperature: f64,
) -> Result<LineBRootStatusV1, String> {
    let actions = collection_logits.len();
    if records.len() != actions * rollouts as usize {
        return Err("line (b) root census is incomplete".into());
    }
    let mut censored = 0_usize;
    let mut returns = vec![Vec::with_capacity(rollouts as usize); actions];
    for (index, record) in records.iter().enumerate() {
        if record.ordinal as usize != index / actions || record.action as usize != index % actions {
            return Err("line (b) root census is out of order".into());
        }
        match &record.outcome {
            LineBRolloutOutcomeV1::Complete { terminal_return } => {
                returns[record.action as usize].push(*terminal_return)
            }
            LineBRolloutOutcomeV1::Censored(_) => censored += 1,
            LineBRolloutOutcomeV1::Defect(defect) => {
                return Err(format!(
                    "line (b) defect at ordinal {} action {}: {defect:?}",
                    record.ordinal, record.action
                ))
            }
        }
    }
    if censored > 0 {
        return Ok(LineBRootStatusV1::Censored {
            censored_rollouts: censored,
        });
    }
    let mean_returns = returns
        .iter()
        .map(|row| line_b_mean_return_v1(row).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let logits: Vec<f64> = collection_logits.iter().map(|&z| f64::from(z)).collect();
    let target = line_b_teacher_target_v1(&logits, &mean_returns, temperature)
        .map_err(|error| error.to_string())?;
    Ok(LineBRootStatusV1::Complete {
        mean_returns,
        log_target: target.log_probabilities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = {
        let mut key = [0_u8; 32];
        let mut i = 0;
        while i < 32 {
            key[i] = i as u8;
            i += 1;
        }
        key
    };

    #[test]
    fn world_and_policy_seeds_follow_the_codex_derivation() {
        // hashlib.sha256(domain + seed LE + key + k LE (+ seat LE)), first
        // eight bytes little-endian.
        let seed = 0x0123_4567_89ab_cdef;
        assert_eq!(
            line_b_world_seed_v1(seed, &KEY, 0),
            6_349_128_873_021_299_388
        );
        assert_eq!(
            line_b_world_seed_v1(seed, &KEY, 15),
            14_448_092_881_910_330_880
        );
        assert_eq!(
            line_b_policy_seeds_v1(seed, &KEY, 0),
            [12_938_716_366_663_246_758, 9_810_008_458_411_905_612]
        );
        assert_eq!(
            line_b_policy_seeds_v1(seed, &KEY, 15),
            [1_028_251_311_821_595_271, 1_551_099_583_275_729_574]
        );
    }

    #[test]
    fn indexed_pool_returns_index_order_for_any_worker_count() {
        let expected: Vec<usize> = (0..23).map(|i| i * i + 1).collect();
        for workers in [0, 1, 2, 3, 7, 23, 40] {
            assert_eq!(
                run_indexed_v1(23, workers, |i| i * i + 1).unwrap(),
                expected,
                "workers {workers}"
            );
        }
        assert!(run_indexed_v1(0, 4, |i| i).unwrap().is_empty());
    }

    fn record(ordinal: u32, action: u32, outcome: LineBRolloutOutcomeV1) -> LineBRolloutRecordV1 {
        LineBRolloutRecordV1 {
            ordinal,
            action,
            outcome,
            physical_decisions: 3,
            policy_steps: 5,
            final_state_hash: 7,
        }
    }

    fn complete(value: i8) -> LineBRolloutOutcomeV1 {
        LineBRolloutOutcomeV1::Complete {
            terminal_return: value,
        }
    }

    #[test]
    fn root_status_is_a_target_a_censored_root_or_a_defect_failure() {
        let logits = [0.5_f32, -1.0, 2.0];
        // K = 2, returns by (ordinal, action): action 0 wins twice, action 1
        // splits, action 2 loses twice.
        let returns = [[1, 0, -1], [1, 1, -1]];
        let records: Vec<_> = (0..2_u32)
            .flat_map(|k| (0..3_u32).map(move |a| (k, a)))
            .map(|(k, a)| record(k, a, complete(returns[k as usize][a as usize])))
            .collect();
        let status = line_b_root_status_v1(&records, 2, &logits, 0.25).unwrap();
        let expected =
            line_b_teacher_target_v1(&logits.map(f64::from), &[1.0, 0.5, -1.0], 0.25).unwrap();
        assert_eq!(
            status,
            LineBRootStatusV1::Complete {
                mean_returns: vec![1.0, 0.5, -1.0],
                log_target: expected.log_probabilities,
            }
        );
        let mut censored = records.clone();
        censored[4].outcome = LineBRolloutOutcomeV1::Censored(LineBCensorV1::CapExhausted);
        censored[1].outcome =
            LineBRolloutOutcomeV1::Censored(LineBCensorV1::EnvironmentalInterruption);
        assert_eq!(
            line_b_root_status_v1(&censored, 2, &logits, 0.25).unwrap(),
            LineBRootStatusV1::Censored {
                censored_rollouts: 2
            }
        );
        // A defect fails even beside censored rollouts.
        let mut defect = censored.clone();
        defect[5].outcome = LineBRolloutOutcomeV1::Defect(LineBDefectV1::UnsupportedBranch(
            "world: HiddenStateContract".into(),
        ));
        assert!(line_b_root_status_v1(&defect, 2, &logits, 0.25).is_err());
        assert!(line_b_root_status_v1(&records[..5], 2, &logits, 0.25).is_err());
        let mut reordered = records.clone();
        reordered.swap(0, 1);
        assert!(line_b_root_status_v1(&reordered, 2, &logits, 0.25).is_err());
    }

    #[test]
    fn search_errors_split_into_invalid_state_and_unsupported_branch() {
        use crate::rl_session::V4SearchStateErrorV1 as E;
        for error in [E::InvalidVisibleBinding, E::NoLiveDecision, E::StepFailed] {
            assert!(matches!(
                search_defect_v1("world", error),
                LineBDefectV1::InvalidState(_)
            ));
        }
        for error in [
            E::HiddenStateContract,
            E::SampleBoundaryChanged,
            E::LibraryChoicePlanFailed,
        ] {
            assert!(matches!(
                search_defect_v1("world", error),
                LineBDefectV1::UnsupportedBranch(_)
            ));
        }
    }
}
