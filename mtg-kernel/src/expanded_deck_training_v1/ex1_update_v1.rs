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

use super::archive_distillation_v1::view_v1;
use super::ex1_collect_v1::{Ex1CollectOptionsV1, EX1_BUDGET_V1, EX1_USEFUL_LIFT_V1};
use super::search_targets_v1::SearchTargetCapV1;
use super::*;
use std::collections::BTreeMap;
use crate::ex1_v1::{
    ex1_target_v1, Ex1EdgeCountsV1, Ex1StratumV1, EX1_DENOMINATOR_V1, EX1_DIVERGENCE_V1,
    EX1_MASS_FLOOR_V1, EX1_ROW_CAP_V1, EX1_SUPPORT_MIN_V1, EX1_TARGET_VERSION_V1,
    EX1_TEMPERATURE_V1,
};
use crate::line_b_teacher_target_v1::{line_b_softmax_v1, LineBSoftmaxV1};
use crate::native_policy_train_step_v1::{
    training_forward_v1, ExternalAuxiliaryInputV1, ExternalAuxiliaryRowV1,
};
use crate::native_policy_value_net_v1::NativeNamedParameterV1;

pub const EX1_METHOD_V1: &str = "ex1-search-in-training-v1-20261029";

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
        Ok(())
    }
}

/// The run-level identity a continuation must preserve: every frozen
/// constant, the target's identifier, the dose and the cap.
pub fn continuation_identity(options: &Ex1UpdateOptionsV1) -> Value {
    json!({"method":EX1_METHOD_V1,"target":EX1_TARGET_VERSION_V1,
        "support_min":EX1_SUPPORT_MIN_V1,"temperature_bits":EX1_TEMPERATURE_V1.to_bits(),
        "mass_floor_bits":EX1_MASS_FLOOR_V1.to_bits(),"divergence":EX1_DIVERGENCE_V1,
        "row_cap":EX1_ROW_CAP_V1,"denominator":EX1_DENOMINATOR_V1,"budget":EX1_BUDGET_V1,
        "rollout":"SELF","rates":Ex1StratumV1::ALL.iter().map(|s| json!({"stratum":s.as_str(),
            "p_bits":s.placement_rate_v1().to_bits()})).collect::<Vec<_>>(),
        "run":options.run,"coefficient_bits":options.coefficient.to_bits(),
        "cap":options.cap.kappa_v1().map_or(json!({"mode":"off"}),
            |kappa| json!({"mode":"ratio","kappa_bits":kappa.to_bits()}))})
}

/// A checkpoint's saved EX1 identity must equal this run's; a checkpoint
/// without one continues only an ordinary run.
pub(super) fn check_continuation_v1(
    saved: Option<&Value>,
    options: Option<&Ex1UpdateOptionsV1>,
) -> Result<(), String> {
    match (saved, options) {
        (None, _) => Ok(()),
        (Some(saved), options) => ensure(
            options.map(continuation_identity).as_ref() == saved.get("continuation"),
            "ex1 continuation must preserve the target, constants, dose and cap",
        ),
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
    options.validate_v1()?;
    let collection: Value = serde_json::from_slice(&read_pinned_bytes(&options.collection)?)
        .map_err(err)?;
    let expected = Ex1CollectOptionsV1 {
        run: options.run.clone(),
        update: options.update,
        budget: EX1_BUDGET_V1,
    };
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
    let model = state.model_v1();
    let parameters = model.parameter_snapshot_v1();
    let mut rows = Vec::new();
    let mut strata: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let (mut searches, mut failed, mut discarded_searches, mut offers) = (0u64, 0u64, 0u64, 0u64);
    let mut search_wall = 0.0_f64;
    let mut episode_records = Vec::with_capacity(episodes.len());
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
            let target = ex1_target_v1(&decision.logits, &counts)?
                .map_err(|reason| format!("ex1 row recomputes to no target ({})", reason.as_str()))?;
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
            let tensor = decision.tensor.tensor();
            let (logits, _) =
                training_forward_v1(model, generation, &parameters, view_v1(&tensor, generation))
                    .map_err(|e| format!("ex1 training forward: {e:?}"))?;
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
        .map(|row| json!({"slot":row.record["slot"],"step":row.record["step"],
            "kind":row.record["kind"],"stratum":row.record["stratum"],
            "learner_deck":row.record["learner_deck"]}))
        .collect();
    rows.truncate(EX1_ROW_CAP_V1);
    let receipt = json!({"method":EX1_METHOD_V1,"collection":options.collection,
        "episodes":episode_records,"strata":strata,"searches":searches,"searches_failed":failed,
        "discarded_attempt_searches":discarded_searches,"search_wall_seconds":search_wall,
        "self_target_offers":offers,"rows_eligible":eligible,"rows":rows.len(),
        "rows_dropped":dropped.len(),"dropped":dropped,"row_cap_binds":eligible > EX1_ROW_CAP_V1,
        "denominator":EX1_DENOMINATOR_V1,"coefficient":options.coefficient,
        "coefficient_bits":options.coefficient.to_bits()});
    Ok((receipt, rows))
}

/// The off-batch auxiliary input over the kept rows: forward KL, the fixed
/// denominator, the run's cap and step telemetry at every update.
pub(super) fn ex1_auxiliary_input_v1<'a>(
    rows: &'a [Ex1TaughtRowV1],
    options: &Ex1UpdateOptionsV1,
    generation: FreshLineageGenerationV1,
) -> ExternalAuxiliaryInputV1<'a> {
    ExternalAuxiliaryInputV1 {
        direction: EX1_DIVERGENCE_V1,
        coefficient: options.coefficient,
        denominator: EX1_DENOMINATOR_V1,
        cap: options.cap.kappa_v1(),
        step_telemetry: true,
        rows: rows
            .iter()
            .map(|row| ExternalAuxiliaryRowV1 {
                encoded: view_v1(&row.tensor, generation),
                target: &row.target,
            })
            .collect(),
    }
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
    let forward = |parameters: &[NativeNamedParameterV1], row: &Ex1TaughtRowV1| {
        training_forward_v1(model, generation, parameters, view_v1(&row.tensor, generation))
            .map_err(|e| format!("ex1 movement forward: {e:?}"))
            .and_then(|(logits, _)| log_softmax_v1(&logits))
    };
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let after = forward(combined, row)?;
        let alone = ordinary.map(|p| forward(p, row)).transpose()?;
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
}
