//! Spy line retention trace (collab LANES/spy-line-retention-20261015,
//! DESIGN.md sections 3 and 4). It reads one completed ordinary training run
//! and changes nothing:
//! - for every update it groups the recorded trajectories into learner
//!   physical decisions as the update did (`replay_learner_groups_v1`,
//!   without its forward check, which the run already passed), recomputes
//!   the GAE advantages and value targets from the recorded values, and the
//!   policy and value sums and loss from the recorded logits, with the
//!   update's own arithmetic; it requires the update record's group count,
//!   learner substep count, normalized-advantage statistics, policy sum,
//!   value sum and loss bit for bit (X2);
//! - it replays each Spy learner episode (a reset episode from its cut) and
//!   lists every learner menu that offers Spy's self-target or Dread Return
//!   on the Giant (the archive's classifier), with the credit that update
//!   gave it.
//!
//! At a listed menu with normalized advantage A in an update of G learner
//! physical decisions, the policy term moves the menu's logits by A / G times
//! (1 at the chosen action, minus the softmax), before the step size, Adam's
//! scaling and shared parameters. The push is that change summed over the line
//! actions: A / G times (1 when a line action was chosen, else 0, minus the
//! line's softmax mass). The slope is the first-order change of the log of the
//! line's softmax mass under the same logit change; at a two-action menu it has
//! the push's sign, at wider menus it need not.

use super::*;
use std::collections::BTreeMap;

const TRACE_SCHEMA: &str = "mtg-kernel-line-retention-trace/v1";
const RUN_SCHEMA: &str = "mtg-kernel-native-expanded-training-run/v1";
const ITERATION_SCHEMA: &str = "mtg-kernel-native-expanded-iteration/v1";
const UPDATE_SCHEMA: &str = "mtg-kernel-expanded-deck-update/v1";

/// One learner physical decision of an update, in the update's order.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TraceGroupV1 {
    /// The trajectory's position in the update.
    pub(super) episode: usize,
    /// The learner rows of the physical decision (indices into `decisions`).
    pub(super) rows: Vec<usize>,
    /// The physical decision's first row's actor's terminal reward.
    pub(super) terminal_return: i8,
    /// The first learner row's recorded value.
    pub(super) value: f32,
}

/// The update's learner groups, as `replay_learner_groups_v1` forms them:
/// rows in blocks of `substep_count`, learner rows only, groups without a
/// learner row skipped, episodes in the update's trajectory order.
pub(super) fn learner_groups_v1(
    trajectories: &[ExpandedTrajectoryV1],
) -> Result<Vec<TraceGroupV1>, String> {
    let mut groups = Vec::new();
    for (ordinal, t) in trajectories.iter().enumerate() {
        validate_trajectory(t)?;
        let mut index = 0;
        while index < t.decisions.len() {
            let first = &t.decisions[index];
            let count = first.substep_count as usize;
            ensure(
                count >= 1 && index + count <= t.decisions.len(),
                "trajectory substep blocks do not tile its decisions",
            )?;
            let rows: Vec<usize> = (index..index + count)
                .filter(|&row| t.decisions[row].actor == t.episode.learner_seat)
                .collect();
            if let Some(&head) = rows.first() {
                groups.push(TraceGroupV1 {
                    episode: ordinal,
                    rows,
                    terminal_return: t.terminal.terminal_reward[first.actor as usize] as i8,
                    value: f32::from_bits(t.decisions[head].value),
                });
            }
            index += count;
        }
    }
    Ok(groups)
}

/// One group's credit: its value (the first learner substep's, which GAE
/// uses), the raw GAE advantage and value target, the normalized advantage,
/// the TD error, the next learner decision's value and the learner decisions
/// left, this one included.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TraceCreditV1 {
    pub(super) value: f32,
    pub(super) raw: f32,
    pub(super) value_target: f32,
    pub(super) normalized: f32,
    pub(super) td_error: f32,
    pub(super) next_value: Option<f32>,
    pub(super) groups_left: usize,
}

/// `compute_gae_targets_v1`'s episodes and arithmetic over `groups`, with
/// the TD error of `gae_episode_advantages_v1`'s recursion.
pub(super) fn credit_v1(
    groups: &[TraceGroupV1],
    gamma: f32,
    lambda: f32,
) -> Result<(Vec<TraceCreditV1>, AdvantageStatisticsV1), String> {
    ensure(!groups.is_empty(), "an update holds no learner decision")?;
    let mut raw = vec![0.0f32; groups.len()];
    let mut targets = vec![0.0f32; groups.len()];
    let mut td = vec![0.0f32; groups.len()];
    let mut next = vec![None; groups.len()];
    let mut left = vec![0usize; groups.len()];
    let mut start = 0usize;
    while start < groups.len() {
        let mut end = start + 1;
        while end < groups.len() && groups[end].episode == groups[start].episode {
            end += 1;
        }
        for group in &groups[start..end] {
            ensure(
                group.terminal_return == groups[start].terminal_return,
                "an episode's learner decisions differ in terminal return",
            )?;
        }
        let terminal_return = f32::from(groups[start].terminal_return);
        let decisions: Vec<(f32, bool)> = (start..end)
            .map(|index| (groups[index].value, index == end - 1))
            .collect();
        let results = gae_episode_advantages_v1(&decisions, terminal_return, gamma, lambda)?;
        for (offset, (advantage, value_target)) in results.into_iter().enumerate() {
            let index = start + offset;
            raw[index] = advantage;
            targets[index] = value_target;
            let is_final = index == end - 1;
            let next_value = if is_final {
                0.0
            } else {
                groups[index + 1].value
            };
            let reward = if is_final { terminal_return } else { 0.0 };
            td[index] = reward + gamma * next_value - groups[index].value;
            next[index] = (!is_final).then(|| groups[index + 1].value);
            left[index] = end - index;
        }
        start = end;
    }
    let (normalized, statistics) =
        normalize_advantages_v1(&raw, GAE_ADVANTAGE_NORMALIZATION_EPSILON_V1)?;
    let credit = (0..groups.len())
        .map(|index| TraceCreditV1 {
            value: groups[index].value,
            raw: raw[index],
            value_target: targets[index],
            normalized: normalized[index],
            td_error: td[index],
            next_value: next[index],
            groups_left: left[index],
        })
        .collect();
    Ok((credit, statistics))
}

/// The update's policy sum, value sum and loss from the groups' recorded
/// logits and the recomputed credit, in the update's f32 arithmetic and group
/// order (`gae_v1`'s forward loss with the entropy coefficient 0 that the GAE
/// loss requires).
pub(super) fn loss_sums_v1(
    trajectories: &[ExpandedTrajectoryV1],
    groups: &[TraceGroupV1],
    credit: &[TraceCreditV1],
    value_coefficient: f32,
) -> Result<(f32, f32, f32), String> {
    ensure(
        !groups.is_empty() && groups.len() == credit.len(),
        "loss sums need one credit per learner group",
    )?;
    let mut policy_sum = 0.0f32;
    let mut value_sum = 0.0f32;
    for (group, entry) in groups.iter().zip(credit) {
        let decisions = &trajectories[group.episode].decisions;
        let mut joint: Option<f32> = None;
        for &row in &group.rows {
            let record = &decisions[row];
            let (log_probability, _) = crate::native_policy_train_step_v1::selected_log_softmax(
                &recorded_logits(&record.logits),
                record.selected as usize,
            )
            .map_err(|error| format!("{error:?}"))?;
            joint = Some(match joint {
                None => log_probability,
                Some(active) => active + log_probability,
            });
        }
        let joint = joint.ok_or("a learner group holds no row")?;
        let value_error = entry.value - entry.value_target;
        policy_sum += -joint * entry.normalized;
        value_sum += value_error * value_error;
    }
    let count = groups.len() as f32;
    ensure(
        count as usize == groups.len(),
        "group count is not exact in f32",
    )?;
    Ok((
        policy_sum,
        value_sum,
        (policy_sum + value_coefficient * value_sum) / count,
    ))
}

/// The push on the line actions at one menu (module docs).
pub(super) fn push_v1(
    normalized_advantage: f32,
    group_count: usize,
    chosen: bool,
    line_mass: f64,
) -> f64 {
    f64::from(normalized_advantage) / group_count as f64 * (f64::from(u8::from(chosen)) - line_mass)
}

/// The first-order change of log(line mass) under the policy term's logit
/// change at one menu (module docs), or None when the line's softmax mass is 0.
pub(super) fn slope_v1(
    normalized_advantage: f32,
    group_count: usize,
    softmax: &[f64],
    line: &[usize],
    selected: usize,
) -> Option<f64> {
    let mass: f64 = line.iter().map(|&k| softmax[k]).sum();
    if mass <= 0.0 {
        return None;
    }
    let line_squares: f64 = line.iter().map(|&k| softmax[k] * softmax[k]).sum();
    let squares: f64 = softmax.iter().map(|p| p * p).sum();
    let chosen = if line.contains(&selected) {
        softmax[selected] / mass
    } else {
        0.0
    };
    let scale = f64::from(normalized_advantage) / group_count as f64;
    Some(scale * (chosen - line_squares / mass - softmax[selected] + squares))
}

/// Line decisions a listed menu can offer.
const DECISIONS: [&str; 2] = ["self", "giant"];

#[derive(Default)]
struct PushSumsV1 {
    /// [chosen won, chosen lost or drawn, declined won, declined lost or drawn].
    pushes: [f64; 4],
    slopes: [f64; 4],
    menus: [usize; 4],
}

impl PushSumsV1 {
    fn add(&mut self, chosen: bool, won: bool, push: f64, slope: Option<f64>) {
        let index = usize::from(!chosen) * 2 + usize::from(!won);
        self.pushes[index] += push;
        self.slopes[index] += slope.unwrap_or(0.0);
        self.menus[index] += 1;
    }

    fn json(&self) -> Value {
        let part = |index: usize| json!({"push":self.pushes[index],"slope":self.slopes[index],"menus":self.menus[index]});
        json!({"total":self.pushes.iter().sum::<f64>(),"slope_total":self.slopes.iter().sum::<f64>(),
            "chosen_won":part(0),"chosen_lost":part(1),"declined_won":part(2),"declined_lost":part(3)})
    }
}

/// Replays one Spy learner episode of an update, appends its listed menus to
/// `rows` and their pushes to `sums`, and returns its episode row.
/// `row_credit` maps each learner row index to its group's credit;
/// `group_count` is the update's learner physical decision count.
fn trace_episode_v1(
    iteration: usize,
    trajectory: &ExpandedTrajectoryV1,
    row_credit: &BTreeMap<usize, TraceCreditV1>,
    group_count: usize,
    sums: &mut [PushSumsV1; 2],
    rows: &mut Vec<Value>,
) -> Result<Value, String> {
    let episode = &trajectory.episode;
    let learner = episode.learner_seat;
    let learner_return = trajectory.terminal.terminal_reward[usize::from(learner)];
    let won = learner_return > 0;
    let mut session = episode_session_v1(episode, &episode.configurations()?)?;
    // A reset episode's recorded starting history: its trajectory, groups
    // and trace hold only the decisions after the cut.
    restore_reset_cut_v1(&mut session, episode)?;
    let mut offered = [0usize; 3];
    let mut chosen = [0usize; 3];
    let mut self_chosen_at: Option<u64> = None;
    let mut line_completed = false;
    for (index, row) in trajectory.decisions.iter().enumerate() {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            return Err("trace replay ends before its recorded row".into());
        };
        ensure(
            d.step == row.step
                && d.physical_decision_id == row.physical_decision_id
                && d.substep_index == row.substep_index
                && seat(d.acting_player) == row.actor
                && d.legal_action_count as usize == row.logits.len(),
            "trace replay row differs from its record",
        )?;
        if row.actor == learner && row.logits.len() >= 2 {
            let flags = spy_menu_flags_v1(&session, learner);
            let selected = row.selected as usize;
            let cast_actions: Vec<usize> = (0..flags.len()).filter(|&a| flags[a].0).collect();
            if !cast_actions.is_empty() {
                offered[0] += 1;
                chosen[0] += usize::from(cast_actions.contains(&selected));
            }
            let lines = [
                (0..flags.len()).filter(|&a| flags[a].1).collect::<Vec<_>>(),
                (0..flags.len()).filter(|&a| flags[a].2).collect::<Vec<_>>(),
            ];
            for (which, actions) in lines.iter().enumerate() {
                if actions.is_empty() {
                    continue;
                }
                let credit = *row_credit
                    .get(&index)
                    .ok_or("a listed menu is not in a learner group")?;
                let logits = recorded_logits(&row.logits);
                let softmax = line_b_softmax_v1(&f64_logits(&logits))
                    .map_err(err)?
                    .probabilities;
                let deployed = deployed_v1(&logits)?;
                let mass: f64 = actions.iter().map(|&a| softmax[a]).sum();
                let deployed_mass: f64 = actions.iter().map(|&a| deployed[a]).sum();
                let is_chosen = actions.contains(&selected);
                let push = push_v1(credit.normalized, group_count, is_chosen, mass);
                let slope = slope_v1(credit.normalized, group_count, &softmax, actions, selected);
                sums[which].add(is_chosen, won, push, slope);
                offered[which + 1] += 1;
                chosen[which + 1] += usize::from(is_chosen);
                if is_chosen && which == 0 && self_chosen_at.is_none() {
                    self_chosen_at = Some(row.step);
                }
                if is_chosen && which == 1 && self_chosen_at.is_some_and(|step| step < row.step) {
                    line_completed = true;
                }
                rows.push(json!({"iteration":iteration,"episode":episode.id,
                    "learner_seat":learner,"step":row.step,
                    "physical_decision_id":row.physical_decision_id,
                    "substep_index":row.substep_index,"decision":DECISIONS[which],
                    "legal_actions":row.logits.len(),"line_actions":actions,
                    "selected":selected,"chosen":is_chosen,"softmax_mass":mass,
                    "deployed_mass":deployed_mass,"value":credit.value,
                    "next_value":credit.next_value,"td_error":credit.td_error,
                    "raw_advantage":credit.raw,"normalized_advantage":credit.normalized,
                    "decisions_left":credit.groups_left,"group_count":group_count,
                    "terminal_return":learner_return,"push":push,"slope":slope}));
            }
        }
        session
            .step(d.episode_id, d.step, row.selected)
            .map_err(err)?;
    }
    ensure(
        matches!(session.current_response(), FastActorResponseV1::Terminal(t) if t == trajectory.terminal),
        "trace replay terminal differs from its record",
    )?;
    Ok(
        json!({"iteration":iteration,"episode":episode.id,"learner_seat":learner,
        "starting_player":episode.starting_player,
        "other_deck":episode.selected[usize::from(1 - learner)].label,"terminal_return":learner_return,
        "cast":{"offered":offered[0],"chosen":chosen[0]},
        "self":{"offered":offered[1],"chosen":chosen[1]},
        "giant":{"offered":offered[2],"chosen":chosen[2]},
        "line_completed":line_completed}),
    )
}

fn write_jsonl(directory: &Path, name: &str, rows: &[Value]) -> Result<PinnedFileV1, String> {
    let mut bytes = Vec::new();
    for row in rows {
        bytes.extend(serde_json::to_vec(row).map_err(err)?);
        bytes.push(b'\n');
    }
    let path = directory.join(name);
    ensure(!path.exists(), "trace output exists")?;
    fs::write(&path, &bytes).map_err(err)?;
    Ok(PinnedFileV1 {
        path,
        sha256: sha(&bytes),
    })
}

/// `LineRetentionTrace`: the trace of one completed run (module docs).
pub(in crate::expanded_deck_training_v1) fn retention_trace_v1(
    run: PinnedFileV1,
    completion: PinnedFileV1,
    output_directory: PathBuf,
) -> Result<Value, String> {
    let started = std::time::Instant::now();
    ensure(
        !output_directory.exists(),
        "trace output directory must be fresh",
    )?;
    ensure(
        run.path.parent().is_some() && run.path.parent() == completion.path.parent(),
        "run.json and completion.json are not one run's",
    )?;
    let manifest: Value = read_pinned(&run)?;
    ensure(
        manifest["schema"] == RUN_SCHEMA,
        "not a training run manifest",
    )?;
    // An absent selection is the run config's serde default (a v3 run).
    let loss: ExpandedLossSelectionV1 = match manifest["config"].get("loss_selection") {
        None => ExpandedLossSelectionV1::default(),
        Some(selection) => serde_json::from_value(selection.clone()).map_err(err)?,
    };
    let ExpandedLossSelectionV1::GaeAdvantageValueV1 { gamma, lambda, .. } = loss else {
        return Err("the trace reads gae_advantage_value runs only".into());
    };
    let value_coefficient: f32 =
        serde_json::from_value(manifest["config"]["value_coefficient"].clone()).map_err(err)?;
    let done: Value = read_pinned(&completion)?;
    ensure(
        done["schema"] == RUN_SCHEMA && done["complete"] == true,
        "not a complete run",
    )?;
    let receipts: Vec<PinnedFileV1> =
        serde_json::from_value(done["iterations"].clone()).map_err(err)?;
    ensure(
        !receipts.is_empty() && done["completed_iterations"] == receipts.len(),
        "the run's iteration receipts differ from its count",
    )?;
    let mut summaries = Vec::with_capacity(receipts.len());
    let mut rows = Vec::new();
    let mut episodes = Vec::new();
    for (iteration, receipt_pin) in receipts.iter().enumerate() {
        let receipt: Value = read_pinned(receipt_pin)?;
        ensure(
            receipt["schema"] == ITERATION_SCHEMA && receipt["iteration"] == iteration,
            "iteration receipt out of order",
        )?;
        let update_pin: PinnedFileV1 =
            serde_json::from_value(receipt["update"].clone()).map_err(err)?;
        let update: Value = read_pinned(&update_pin)?;
        ensure(
            update["schema"] == UPDATE_SCHEMA
                && update["complete"] == true
                && update["loss_identity"] == GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
            "not a complete gae update record",
        )?;
        let pins: Vec<PinnedFileV1> =
            serde_json::from_value(update["trajectories"].clone()).map_err(err)?;
        let trajectories = pins
            .iter()
            .map(read_pinned::<ExpandedTrajectoryV1>)
            .collect::<Result<Vec<_>, _>>()?;
        let groups = learner_groups_v1(&trajectories)?;
        let substeps: usize = groups.iter().map(|g| g.rows.len()).sum();
        let (credit, statistics) = credit_v1(&groups, gamma, lambda)?;
        let (policy_sum, value_sum, loss) =
            loss_sums_v1(&trajectories, &groups, &credit, value_coefficient)?;
        // X2: the update's own counts, statistics, sums and loss, bit for bit.
        ensure(
            update["physical_decisions"] == groups.len()
                && update["policy_substeps"] == substeps
                && update["advantage_statistics"] == json!(statistics)
                && update["policy_sum"] == json!(policy_sum)
                && update["value_sum"] == json!(value_sum)
                && update["loss"] == json!(loss),
            &format!(
                "X2 failed at iteration {iteration}: the recomputed update differs from its record"
            ),
        )?;
        let mut sums: [PushSumsV1; 2] = Default::default();
        let mut spy = 0usize;
        for (ordinal, trajectory) in trajectories.iter().enumerate() {
            let learner = usize::from(trajectory.episode.learner_seat);
            if !is_spy_label(&trajectory.episode.selected[learner].label) {
                continue;
            }
            spy += 1;
            let row_credit: BTreeMap<usize, TraceCreditV1> = groups
                .iter()
                .zip(&credit)
                .filter(|(g, _)| g.episode == ordinal)
                .flat_map(|(g, c)| g.rows.iter().map(move |&row| (row, *c)))
                .collect();
            let decisions = groups.iter().filter(|g| g.episode == ordinal).count();
            let mut episode = trace_episode_v1(
                iteration,
                trajectory,
                &row_credit,
                groups.len(),
                &mut sums,
                &mut rows,
            )?;
            episode["learner_decisions"] = json!(decisions);
            episodes.push(episode);
        }
        summaries.push(
            json!({"iteration":iteration,"trajectories":trajectories.len(),
            "physical_decisions":groups.len(),"policy_substeps":substeps,
            "advantage_statistics":statistics,"policy_sum":policy_sum,"value_sum":value_sum,
            "loss":loss,"x2":true,"spy_learner_episodes":spy,
            "push":{"self":sums[0].json(),"giant":sums[1].json()}}),
        );
    }
    fs::create_dir_all(&output_directory).map_err(err)?;
    let rows_pin = write_jsonl(&output_directory, "rows.jsonl", &rows)?;
    let episodes_pin = write_jsonl(&output_directory, "episodes.jsonl", &episodes)?;
    let result = json!({"schema":TRACE_SCHEMA,"complete":true,"run":run,"completion":completion,
        "gamma":gamma,"lambda":lambda,"value_coefficient":value_coefficient,
        "iterations":summaries,"x2":"passed",
        "rows":rows_pin,"listed_menus":rows.len(),"episodes":episodes_pin,
        "spy_learner_episodes":episodes.len(),
        "runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "wall_seconds":started.elapsed().as_secs_f64()});
    publish_json(&output_directory, "trace.json", &result)?;
    Ok(result)
}
