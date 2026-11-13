//! EX1's update (collab LANES/spy-primary-direction-20261028/EX1-DESIGN-DRAFT.md
//! v3.3 sections 3, 5 and 7): the rows the collection hook recorded at the
//! learner's own decisions enter the same Adam step as forward-KL auxiliary
//! rows, `c / 128` each, beside the ordinary GAE batch.
//!
//! This is the collection-target adapter (section 7 item 4). It replaces B2's
//! `teach` authentication, which replays a bank game and refuses SELF
//! rollouts: every row is bound to this update's own trajectory batch by the
//! collection receipt, recorded separately from the learner's sampled action,
//! and checked twice at the live row: the target recomputed from the recorded
//! node counts and the trajectory's deployed logits equals the recorded
//! target bit for bit, and the training forward of the trajectory's tensor at
//! the update-start parameters equals those logits.
//!
//! At most 128 rows enter (the lowest `ex1/rowcap` keys; dropped rows are
//! counted). An update with no rows takes the inactive auxiliary path, bit
//! identical to ordinary, and step telemetry runs at every update.
//!
//! **W5 (`w5-xact-v1`, collab LANES/lead/W5-DESIGN.md v1.3 section 3):** with
//! `w5` set, every root row is one the search played. Its played action must
//! recompute from q and the episode's acting hash, and its policy gradient
//! is masked (every such row, kept or dropped by the row cap; the policy term
//! is normalized over the unmasked groups). Burst rows keep their policy
//! gradient. The paired Adam-step comparison runs on one update in 16 (and
//! on updates with no rows, where it is the inactive path).

use super::archive_distillation_v1::view_v1;
use super::ex1_collect_v1::{
    Ex1CollectOptionsV1, W5CollectOptionsV1, EX1_BUDGET_V1, EX1_USEFUL_LIFT_V1,
};
use super::search_targets_v1::SearchTargetCapV1;
use super::*;
use crate::ex1_v1::{
    ex1_target_v1, Ex1EdgeCountsV1, Ex1StratumV1, EX1_DENOMINATOR_V1, EX1_DIVERGENCE_V1,
    EX1_MASS_FLOOR_V1, EX1_ROW_CAP_V1, EX1_SUPPORT_MIN_V1, EX1_TARGET_VERSION_V1,
    EX1_TEMPERATURE_V1,
};
use crate::line_b_teacher_target_v1::{line_b_softmax_v1, LineBSoftmaxV1};
use crate::native_policy_train_step_v1::{
    input_major_weights_v1, training_forward_input_major_v1, ExternalAuxiliaryInputV1,
    ExternalAuxiliaryRowV1,
};
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use crate::w5_xact_v1::{
    w5_played_action_v1, w5_telemetry_update_v1, W5_PLACEMENT_MAX_V1, W5_TELEMETRY_STRIDE_V1,
    W5_XACT_VERSION_V1,
};
use std::collections::BTreeMap;

pub const EX1_METHOD_V1: &str = "ex1-search-in-training-v1-20261029";

/// W5's update options: the recipe and the run's frozen search budget B.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct W5UpdateOptionsV1 {
    pub recipe: String,
    pub budget: u64,
    /// The search's exact key memo limit in MiB (0: off).
    pub key_memo_mib: u64,
}

/// One update's EX1 options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ex1UpdateOptionsV1 {
    pub run: String,
    /// The global update index; equals the collection's.
    pub update: u64,
    pub coefficient: f64,
    pub cap: SearchTargetCapV1,
    /// This update's collection receipt (`collection.json`), which pins the
    /// hook's per-episode records.
    pub collection: PinnedFileV1,
    /// W5's recipe; omitted for EX1, so every EX1 command keeps its bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w5: Option<W5UpdateOptionsV1>,
}

impl Ex1UpdateOptionsV1 {
    pub fn validate_v1(&self) -> Result<(), String> {
        ensure(
            !self.run.is_empty() && self.run.len() <= 256,
            "ex1 run id empty or too long",
        )?;
        ensure(
            self.coefficient.is_finite() && self.coefficient > 0.0,
            "ex1 coefficient must be positive and finite",
        )?;
        if let SearchTargetCapV1::Ratio { kappa } = self.cap {
            ensure(
                kappa.is_finite() && kappa > 0.0,
                "ex1 cap ratio must be positive and finite",
            )?;
        }
        if let Some(w5) = &self.w5 {
            self.collect_options_v1().validate_v1()?;
            ensure(
                w5.recipe == W5_XACT_VERSION_V1,
                "w5 update names an unknown recipe",
            )?;
        }
        Ok(())
    }

    /// The collection options this update's collection must carry.
    pub fn collect_options_v1(&self) -> Ex1CollectOptionsV1 {
        Ex1CollectOptionsV1 {
            run: self.run.clone(),
            update: self.update,
            budget: self.w5.as_ref().map_or(EX1_BUDGET_V1, |w5| w5.budget),
            w5: self.w5.as_ref().map(|w5| W5CollectOptionsV1 {
                recipe: w5.recipe.clone(),
                key_memo_mib: w5.key_memo_mib,
                capture_0b: None,
            }),
        }
    }

    /// Whether this update runs the paired Adam-step comparison: every EX1
    /// update; one W5 update in 16.
    pub fn step_telemetry_v1(&self) -> bool {
        self.w5.is_none() || w5_telemetry_update_v1(self.update)
    }
}

/// The run-level identity a continuation must preserve: every frozen
/// constant, the target's identifier, the dose and the cap.
pub fn continuation_identity(options: &Ex1UpdateOptionsV1) -> Value {
    if let Some(w5) = &options.w5 {
        return json!({"method":W5_XACT_VERSION_V1,"target":EX1_TARGET_VERSION_V1,
            "support_min":EX1_SUPPORT_MIN_V1,"temperature_bits":EX1_TEMPERATURE_V1.to_bits(),
            "mass_floor_bits":EX1_MASS_FLOOR_V1.to_bits(),"divergence":EX1_DIVERGENCE_V1,
            "row_cap":EX1_ROW_CAP_V1,"denominator":EX1_DENOMINATOR_V1,"budget":w5.budget,
            "rollout":"SELF","search":crate::regret_census_v1::stage4a::ex1_search_identity_v1(),
            "eligibility":"w1-two-non-mana-candidates","placement":"w5/placement",
            "placement_max":W5_PLACEMENT_MAX_V1,"acting":"w5/act inverse-cdf of q",
            "key_memo_mib":w5.key_memo_mib,
            "policy_gradient":"masked at played roots","telemetry_stride":W5_TELEMETRY_STRIDE_V1,
            "run":options.run,"coefficient_bits":options.coefficient.to_bits(),
            "cap":options.cap.kappa_v1().map_or(json!({"mode":"off"}),
                |kappa| json!({"mode":"ratio","kappa_bits":kappa.to_bits()}))});
    }
    json!({"method":EX1_METHOD_V1,"target":EX1_TARGET_VERSION_V1,
        "support_min":EX1_SUPPORT_MIN_V1,"temperature_bits":EX1_TEMPERATURE_V1.to_bits(),
        "mass_floor_bits":EX1_MASS_FLOOR_V1.to_bits(),"divergence":EX1_DIVERGENCE_V1,
        "row_cap":EX1_ROW_CAP_V1,"denominator":EX1_DENOMINATOR_V1,"budget":EX1_BUDGET_V1,
        "rollout":"SELF","search":crate::regret_census_v1::stage4a::ex1_search_identity_v1(),
        "rates":Ex1StratumV1::ALL.iter().map(|s| json!({"stratum":s.as_str(),
            "p_bits":s.placement_rate_v1().to_bits()})).collect::<Vec<_>>(),
        "run":options.run,"coefficient_bits":options.coefficient.to_bits(),
        "cap":options.cap.kappa_v1().map_or(json!({"mode":"off"}),
            |kappa| json!({"mode":"ratio","kappa_bits":kappa.to_bits()}))})
}

/// A checkpoint's saved EX1 identity must equal this run's; a checkpoint
/// without one continues only an ordinary run. A W5 checkpoint also names
/// its next global update, which the continuing update must be.
pub(super) fn check_continuation_v1(
    saved: Option<&Value>,
    options: Option<&Ex1UpdateOptionsV1>,
) -> Result<(), String> {
    match (saved, options) {
        (None, _) => Ok(()),
        (Some(saved), options) => {
            ensure(
                options.map(continuation_identity).as_ref() == saved.get("continuation"),
                "ex1 continuation must preserve the target, constants, dose and cap",
            )?;
            let saved_w5 = saved["continuation"]["method"] == W5_XACT_VERSION_V1;
            match (saved.get("w5_next_update"), options) {
                (None, _) if saved_w5 => Err("a w5 checkpoint lacks its next global update".into()),
                (None, _) => Ok(()),
                (Some(next), Some(options)) => ensure(
                    next == &json!(options.update),
                    "w5 continuation must take the checkpoint's next global update",
                ),
                (Some(_), None) => Err("a w5 checkpoint continues only its chain".into()),
            }
        }
    }
}

/// One taught row: the trajectory's tensor, its target and its record.
pub(super) struct Ex1TaughtRowV1 {
    tensor: NativeFlatDecisionTensorV2,
    target: LineBSoftmaxV1,
    student_log: Vec<f64>,
    argmax: usize,
    self_index: Option<usize>,
    useful: bool,
    record: Value,
}

fn counts_v1(row: &Value) -> Result<Vec<Ex1EdgeCountsV1>, String> {
    let read = |key: &str| -> Result<Vec<u64>, String> {
        serde_json::from_value(row[key].clone()).map_err(|_| format!("ex1 row lacks {key}"))
    };
    let (n, wins, draws) = (read("n")?, read("wins")?, read("draws")?);
    ensure(
        n.len() == wins.len() && n.len() == draws.len(),
        "ex1 row counts differ in width",
    )?;
    Ok((0..n.len())
        .map(|a| Ex1EdgeCountsV1 {
            n: n[a],
            wins: wins[a],
            draws: draws[a],
        })
        .collect())
}

/// Reads and authenticates this update's rows (see the module comment).
/// Returns the receipt fields and the kept rows in cap order.
pub(super) fn ex1_teach_v1(
    options: &Ex1UpdateOptionsV1,
    trajectories: &[PinnedFileV1],
    episodes: &[ExpandedTrajectoryV1],
    state: &NativePolicyValueTrainStateV1,
    generation: FreshLineageGenerationV1,
) -> Result<(Value, Vec<Ex1TaughtRowV1>), String> {
    let model = state.model_v1();
    let parameters = model.parameter_snapshot_v1();
    let input_major = input_major_weights_v1(&parameters);
    ex1_teach_with_v1(options, trajectories, episodes, &mut |tensor| {
        training_forward_input_major_v1(
            model,
            generation,
            &parameters,
            Some(&input_major),
            view_v1(tensor, generation),
        )
        .map(|(logits, _)| logits)
        .map_err(|e| format!("ex1 training forward: {e:?}"))
    })
}

/// [`ex1_teach_v1`] for any learner: `forward` is the learner's training
/// forward of a trajectory tensor at the update-start parameters (its
/// logits), which must equal the deployed logits bit for bit.
pub(super) fn ex1_teach_with_v1(
    options: &Ex1UpdateOptionsV1,
    trajectories: &[PinnedFileV1],
    episodes: &[ExpandedTrajectoryV1],
    forward: &mut dyn FnMut(&NativeFlatDecisionTensorV2) -> Result<Vec<f32>, String>,
) -> Result<(Value, Vec<Ex1TaughtRowV1>), String> {
    options.validate_v1()?;
    let collection: Value =
        serde_json::from_slice(&read_pinned_bytes(&options.collection)?).map_err(err)?;
    let expected = options.collect_options_v1();
    ensure(
        collection["ex1"]["options"] == json!(expected),
        "ex1 collection belongs to another run, update or budget",
    )?;
    ensure(
        collection["trajectories"] == json!(trajectories),
        "ex1 collection's trajectories differ from the update's batch",
    )?;
    let sidecars: Vec<PinnedFileV1> =
        serde_json::from_value(collection["ex1"]["episodes"].clone()).map_err(err)?;
    ensure(
        sidecars.len() == episodes.len(),
        "ex1 collection lacks one record per trajectory",
    )?;
    let mut rows = Vec::new();
    let mut strata: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let (mut searches, mut failed, mut discarded_searches, mut offers) = (0u64, 0u64, 0u64, 0u64);
    let mut search_wall = 0.0_f64;
    let mut episode_records = Vec::with_capacity(episodes.len());
    // W5: the substeps the search played, `(slot, step)`, whose policy
    // gradient is masked.
    let mut pg_masked = Vec::new();
    let mut w5_totals: BTreeMap<String, u64> = BTreeMap::new();
    for (slot, (pin, trajectory)) in sidecars.iter().zip(episodes).enumerate() {
        let record: Value = serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        let attempts = record["attempts"]
            .as_array()
            .ok_or("ex1 record lacks attempts")?;
        let (last, earlier) = attempts.split_last().ok_or("ex1 record has no attempt")?;
        ensure(
            record["slot"] == json!(slot)
                && last["natural"] == true
                && earlier.iter().all(|a| a["natural"] == false)
                && last["episode_id"] == json!(trajectory.episode.id)
                && last["seed"] == json!(trajectory.episode.seed)
                && last["learner_seat"] == json!(trajectory.episode.learner_seat),
            "ex1 record does not match its trajectory",
        )?;
        for attempt in earlier {
            discarded_searches += attempt["searches"].as_array().map_or(0, |s| s.len() as u64);
        }
        for search in last["searches"].as_array().ok_or("ex1 searches missing")? {
            searches += 1;
            failed += u64::from(search["failed"] == true);
            search_wall += search["wall"].as_f64().unwrap_or(0.0);
        }
        offers += last["offers"].as_array().map_or(0, |o| o.len() as u64);
        ensure(
            options.w5.is_some() || trajectory.search_played.is_none(),
            "an ex1 trajectory names search-played rows",
        )?;
        if options.w5.is_some() {
            let placement = last["w5"]
                .as_object()
                .ok_or("w5 record lacks its placement")?;
            ensure(
                placement.get("slot") == Some(&json!(slot)),
                "w5 placement belongs to another slot",
            )?;
            // Every row the search played, and only those, is a root row
            // of the record with the learner's own draw beside it.
            let mut acted: Vec<(u64, Value)> = last["rows"]
                .as_array()
                .ok_or("ex1 rows missing")?
                .iter()
                .filter(|row| row["kind"] == "root")
                .map(|row| {
                    (
                        row["step"].as_u64().unwrap_or(u64::MAX),
                        row["policy_selected"].clone(),
                    )
                })
                .collect();
            acted.sort_by_key(|(step, _)| *step);
            let played: Vec<(u64, Value)> = trajectory
                .search_played
                .iter()
                .flatten()
                .map(|row| (row.step, json!(row.policy_selected)))
                .collect();
            ensure(
                acted == played,
                "w5 trajectory's search-played rows differ from the record's roots",
            )?;
            for key in ["attempted", "acted", "root_no_target"] {
                *w5_totals.entry(key.into()).or_default() +=
                    placement.get(key).and_then(Value::as_u64).unwrap_or(0);
            }
            *w5_totals.entry("reached".into()).or_default() +=
                u64::from(placement.get("reached") == Some(&json!(true)));
        }
        for (stratum, counts) in last["strata"].as_object().ok_or("ex1 strata missing")? {
            let total = strata.entry(stratum.clone()).or_default();
            for (key, value) in counts.as_object().ok_or("ex1 stratum counts")? {
                *total.entry(key.clone()).or_default() += value.as_u64().unwrap_or(0);
            }
        }
        let learner_seat = trajectory.episode.learner_seat;
        for row in last["rows"].as_array().ok_or("ex1 rows missing")? {
            let step = row["step"].as_u64().ok_or("ex1 row lacks a step")?;
            let decision = trajectory
                .decisions
                .iter()
                .find(|d| d.step == step)
                .ok_or("ex1 row names a step outside its trajectory")?;
            ensure(
                row["physical_decision_id"] == json!(decision.physical_decision_id)
                    && row["substep"] == json!(decision.substep_index)
                    && row["selected"] == json!(decision.selected)
                    && decision.actor == learner_seat,
                "ex1 row differs from its trajectory decision",
            )?;
            let counts = counts_v1(row)?;
            let target = ex1_target_v1(&decision.logits, &counts)?.map_err(|reason| {
                format!("ex1 row recomputes to no target ({})", reason.as_str())
            })?;
            let q_bits: Vec<u64> = target
                .target
                .probabilities
                .iter()
                .map(|x| x.to_bits())
                .collect();
            ensure(
                row["q_bits"] == json!(q_bits),
                "ex1 target differs from the one computed at the live row",
            )?;
            if options.w5.is_some() {
                let root = row["kind"] == "root";
                ensure(
                    row["acted"] == json!(root),
                    "w5 row: only a root is played by the search",
                )?;
                if root {
                    let played = w5_played_action_v1(
                        &target.target.probabilities,
                        &options.run,
                        options.update,
                        trajectory.episode.seed,
                        slot as u64,
                    )?;
                    ensure(
                        decision.selected as usize == played,
                        "w5 root's played action differs from q's draw",
                    )?;
                    pg_masked.push(json!({"slot":slot,"step":step}));
                }
            }
            let tensor = decision.tensor.tensor();
            let logits = forward(&tensor)?;
            ensure(
                logits.iter().map(|z| z.to_bits()).collect::<Vec<_>>() == decision.logits,
                "ex1 training forward differs from the deployed logits",
            )?;
            let self_index = row["self_target"]["index"].as_u64().map(|i| i as usize);
            let useful = row["self_target"]["useful"] == true;
            let mut kept = row.clone();
            kept["slot"] = json!(slot);
            kept["learner_deck"] = last["learner_deck"].clone();
            rows.push(Ex1TaughtRowV1 {
                tensor,
                argmax: target.target_argmax_v1(),
                student_log: target.student.log_probabilities.clone(),
                target: target.target,
                self_index,
                useful,
                record: kept,
            });
        }
        episode_records.push(json!({"slot":slot,"record":pin,"attempts":attempts.len()}));
    }
    let eligible = rows.len();
    let key = |row: &Ex1TaughtRowV1| {
        (
            row.record["rowcap_key"].as_u64().unwrap_or(u64::MAX),
            row.record["slot"].as_u64().unwrap_or(0),
            row.record["step"].as_u64().unwrap_or(0),
        )
    };
    rows.sort_by_key(key);
    let dropped: Vec<Value> = rows
        .iter()
        .skip(EX1_ROW_CAP_V1)
        .map(|row| {
            json!({"slot":row.record["slot"],"step":row.record["step"],
            "kind":row.record["kind"],"stratum":row.record["stratum"],
            "learner_deck":row.record["learner_deck"]})
        })
        .collect();
    rows.truncate(EX1_ROW_CAP_V1);
    let mut receipt = json!({"method":EX1_METHOD_V1,"collection":options.collection,
        "episodes":episode_records,"strata":strata,"searches":searches,"searches_failed":failed,
        "discarded_attempt_searches":discarded_searches,"search_wall_seconds":search_wall,
        "self_target_offers":offers,"rows_eligible":eligible,"rows":rows.len(),
        "rows_dropped":dropped.len(),"dropped":dropped,"row_cap_binds":eligible > EX1_ROW_CAP_V1,
        "denominator":EX1_DENOMINATOR_V1,"coefficient":options.coefficient,
        "coefficient_bits":options.coefficient.to_bits()});
    if let Some(w5) = &options.w5 {
        receipt["method"] = json!(W5_XACT_VERSION_V1);
        receipt["w5"] = json!({"budget":w5.budget,"placements":w5_totals,"pg_masked":pg_masked,
            "step_telemetry":options.step_telemetry_v1()});
    }
    Ok((receipt, rows))
}

/// The off-batch auxiliary input over the kept rows: forward KL, the fixed
/// denominator, the run's cap and step telemetry (every EX1 update; one W5
/// update in 16, and any W5 update with no rows). `policy_mask` is W5's
/// masked `(group, substep)` list, empty for EX1.
pub(super) fn ex1_auxiliary_input_v1<'a>(
    rows: &'a [Ex1TaughtRowV1],
    options: &Ex1UpdateOptionsV1,
    generation: FreshLineageGenerationV1,
    policy_mask: Vec<(usize, usize)>,
) -> ExternalAuxiliaryInputV1<'a> {
    ExternalAuxiliaryInputV1 {
        direction: EX1_DIVERGENCE_V1,
        coefficient: options.coefficient,
        denominator: EX1_DENOMINATOR_V1,
        cap: options.cap.kappa_v1(),
        step_telemetry: options.step_telemetry_v1() || rows.is_empty(),
        policy_mask,
        rows: rows
            .iter()
            .map(|row| ExternalAuxiliaryRowV1 {
                encoded: view_v1(&row.tensor, generation),
                target: &row.target,
            })
            .collect(),
    }
}

/// [`ex1_auxiliary_input_v1`] for the S learner (`s_transformer_v1`): the
/// same rows, targets, dose, cap, telemetry and mask.
pub(super) fn ex1_s_auxiliary_input_v1<'a>(
    rows: &'a [Ex1TaughtRowV1],
    options: &Ex1UpdateOptionsV1,
    policy_mask: Vec<(usize, usize)>,
) -> crate::s_transformer_v1::SAuxiliaryInputV1<'a> {
    crate::s_transformer_v1::SAuxiliaryInputV1 {
        direction: EX1_DIVERGENCE_V1,
        coefficient: options.coefficient,
        denominator: EX1_DENOMINATOR_V1,
        cap: options.cap.kappa_v1(),
        step_telemetry: options.step_telemetry_v1() || rows.is_empty(),
        policy_mask,
        rows: rows
            .iter()
            .map(|row| crate::s_transformer_v1::SAuxiliaryRowV1 {
                input: crate::s_transformer_v1::SInputV1::of_tensor_v1(&row.tensor),
                target: &row.target,
            })
            .collect(),
    }
}

/// W5: each played root's `(slot, step)` in the receipt as its `(group,
/// substep)` in this update's learner groups; empty without W5.
pub(super) fn w5_policy_mask_v1(
    receipt: &Value,
    tensor_groups: &[LearnerTensorGroupV1<'_>],
) -> Result<Vec<(usize, usize)>, String> {
    let mut policy_mask = Vec::new();
    if let Some(masked) = receipt["w5"]["pg_masked"].as_array() {
        let mut place = BTreeMap::new();
        for (group, (_, slot, substeps)) in tensor_groups.iter().enumerate() {
            for (substep, (row, _)) in substeps.iter().enumerate() {
                place.insert((*slot, row.step), (group, substep));
            }
        }
        for entry in masked {
            let key = (
                entry["slot"].as_u64().ok_or("w5 mask lacks a slot")? as usize,
                entry["step"].as_u64().ok_or("w5 mask lacks a step")?,
            );
            policy_mask.push(
                *place
                    .get(&key)
                    .ok_or("w5 masked row is not a learner substep of the batch")?,
            );
        }
    }
    Ok(policy_mask)
}

fn log_softmax_v1(logits: &[f32]) -> Result<Vec<f64>, String> {
    let z: Vec<f64> = logits.iter().map(|&x| f64::from(x)).collect();
    Ok(line_b_softmax_v1(&z)
        .map_err(|e| format!("ex1: {e}"))?
        .log_probabilities)
}

/// Section 5's movement telemetry on the same rows: log p of the argmax of q
/// over S at every taught row, and log p(self) at self-target rows, under the
/// deployed pre-step weights, after the applied combined Adam step and after
/// the ordinary-only step from identical state. Per row, in cap order.
pub(super) fn ex1_movement_v1(
    rows: &[Ex1TaughtRowV1],
    model: &NativePolicyValueNetV1,
    generation: FreshLineageGenerationV1,
    combined: &[NativeNamedParameterV1],
    ordinary: Option<&[NativeNamedParameterV1]>,
) -> Result<Value, String> {
    if rows.is_empty() {
        return ex1_movement_with_v1(rows, &mut |_| unreachable!("no rows"), None);
    }
    let combined_im = input_major_weights_v1(combined);
    let ordinary_im = ordinary.map(input_major_weights_v1);
    let forward =
        |parameters: &[NativeNamedParameterV1], im, tensor: &NativeFlatDecisionTensorV2| {
            training_forward_input_major_v1(
                model,
                generation,
                parameters,
                Some(im),
                view_v1(tensor, generation),
            )
            .map(|(logits, _)| logits)
            .map_err(|e| format!("ex1 movement forward: {e:?}"))
        };
    let mut alone = ordinary
        .zip(ordinary_im.as_ref())
        .map(|(p, im)| move |tensor: &NativeFlatDecisionTensorV2| forward(p, im, tensor));
    ex1_movement_with_v1(
        rows,
        &mut |tensor| forward(combined, &combined_im, tensor),
        alone.as_mut().map(|f| f as Ex1ForwardV1<'_>),
    )
}

/// A row's logits from one learner forward (the movement telemetry's).
pub(super) type Ex1ForwardV1<'f> =
    &'f mut dyn FnMut(&NativeFlatDecisionTensorV2) -> Result<Vec<f32>, String>;

/// [`ex1_movement_v1`] for any learner: `after` and `alone` give a row's
/// logits after the combined step and after the ordinary-only step.
pub(super) fn ex1_movement_with_v1(
    rows: &[Ex1TaughtRowV1],
    after: Ex1ForwardV1<'_>,
    mut alone: Option<Ex1ForwardV1<'_>>,
) -> Result<Value, String> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let after = log_softmax_v1(&after(&row.tensor)?)?;
        let alone = match alone.as_mut() {
            Some(forward) => Some(log_softmax_v1(&forward(&row.tensor)?)?),
            None => None,
        };
        let at = |index: usize| {
            json!({"index":index,"before":row.student_log[index],"after_combined":after[index],
                "after_ordinary":alone.as_ref().map(|a| a[index])})
        };
        out.push(json!({"slot":row.record["slot"],"step":row.record["step"],
            "kind":row.record["kind"],"stratum":row.record["stratum"],
            "learner_deck":row.record["learner_deck"],"tv":row.record["tv"],
            "mass":row.record["mass"],"argmax":at(row.argmax),
            "self":row.self_index.map(at),"useful":row.useful}));
    }
    Ok(json!(out))
}

/// The useful-row lift threshold, recorded beside the movement rows.
pub(super) fn ex1_thresholds_v1() -> Value {
    json!({"useful_lift":EX1_USEFUL_LIFT_V1})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(coefficient: f64, cap: SearchTargetCapV1, update: u64) -> Ex1UpdateOptionsV1 {
        Ex1UpdateOptionsV1 {
            run: "ex1-test".into(),
            update,
            coefficient,
            cap,
            collection: PinnedFileV1 {
                path: PathBuf::from(format!("collection-{update}.json")),
                sha256: String::new(),
            },
            w5: None,
        }
    }

    /// The identity binds the target, every frozen constant, the dose and
    /// the cap, never the update or its collection; an EX1 checkpoint
    /// refuses an ordinary continuation, an ordinary parent admits EX1.
    #[test]
    fn continuations_preserve_target_dose_and_cap() {
        let capped = SearchTargetCapV1::Ratio { kappa: 1.0 };
        let saved_options = options(2.5, capped, 7);
        let saved = json!({"continuation":continuation_identity(&saved_options)});
        assert!(check_continuation_v1(Some(&saved), Some(&options(2.5, capped, 8))).is_ok());
        for changed in [
            options(2.0, capped, 8),
            options(2.5, SearchTargetCapV1::Ratio { kappa: 2.0 }, 8),
            options(2.5, SearchTargetCapV1::Off, 8),
            Ex1UpdateOptionsV1 {
                run: "other".into(),
                ..options(2.5, capped, 8)
            },
        ] {
            assert!(check_continuation_v1(Some(&saved), Some(&changed))
                .unwrap_err()
                .contains("must preserve"));
        }
        assert!(check_continuation_v1(Some(&saved), None).is_err());
        assert!(check_continuation_v1(None, Some(&saved_options)).is_ok());
        let identity = continuation_identity(&saved_options);
        assert_eq!(identity["target"], "ex1-target-v1");
        assert_eq!(identity["divergence"], "forward-kl");
        assert_eq!(identity["row_cap"], 128);
        assert_eq!(identity["denominator"], 128);
        assert_eq!(identity["budget"], 64_000);
        assert_eq!(identity["rollout"], "SELF");
        assert!(options(0.0, capped, 0).validate_v1().is_err());
        assert!(options(1.0, SearchTargetCapV1::Ratio { kappa: 0.0 }, 0)
            .validate_v1()
            .is_err());
    }

    /// W5's identity binds its own method, budget and constants, so an EX1
    /// checkpoint never continues as W5 (or the reverse); its collection
    /// options carry the recipe and B; telemetry runs on one update in 16.
    #[test]
    fn w5_identity_budget_and_telemetry_stride() {
        let capped = SearchTargetCapV1::Ratio { kappa: 2.0 };
        let w5 = |update: u64, budget: u64| Ex1UpdateOptionsV1 {
            w5: Some(W5UpdateOptionsV1 {
                recipe: W5_XACT_VERSION_V1.into(),
                budget,
                key_memo_mib: 512,
            }),
            ..options(1.0, capped, update)
        };
        let identity = continuation_identity(&w5(5, 16_000));
        assert_eq!(identity["method"], W5_XACT_VERSION_V1);
        assert_eq!(identity["target"], "ex1-target-v1");
        assert_eq!(identity["budget"], 16_000);
        assert_eq!(identity["placement_max"], 128);
        assert_eq!(identity, continuation_identity(&w5(6, 16_000)));
        assert_ne!(identity, continuation_identity(&w5(6, 64_000)));
        let saved = json!({"continuation":identity});
        assert!(check_continuation_v1(Some(&saved), Some(&options(1.0, capped, 6))).is_err());
        // The saved cursor binds the next global update.
        let cursor = json!({"continuation":identity.clone(),"w5_next_update":6});
        assert!(check_continuation_v1(Some(&saved), Some(&w5(6, 16_000))).is_err());
        assert!(check_continuation_v1(Some(&cursor), Some(&w5(6, 16_000))).is_ok());
        assert!(check_continuation_v1(Some(&cursor), Some(&w5(7, 16_000))).is_err());
        assert!(check_continuation_v1(Some(&cursor), Some(&w5(5, 16_000))).is_err());
        assert!(check_continuation_v1(Some(&cursor), None).is_err());
        let collect = w5(5, 16_000).collect_options_v1();
        assert_eq!(collect.budget, 16_000);
        assert_eq!(collect.w5.as_ref().unwrap().recipe, W5_XACT_VERSION_V1);
        assert!(w5(5, 16_000).validate_v1().is_ok());
        assert!(w5(5, 0).validate_v1().is_err());
        let mut unknown = w5(5, 16_000);
        unknown.w5.as_mut().unwrap().recipe = "w5-other".into();
        assert!(unknown.validate_v1().is_err());
        assert!(w5(32, 16_000).step_telemetry_v1());
        assert!(!w5(33, 16_000).step_telemetry_v1());
        assert!(options(1.0, capped, 33).step_telemetry_v1());
        // EX1 options keep their serialized bytes.
        assert!(serde_json::to_value(options(1.0, capped, 3))
            .unwrap()
            .get("w5")
            .is_none());
    }
}
