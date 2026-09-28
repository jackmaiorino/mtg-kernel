//! CPU-only scoring of the same archived actor-visible tensors by fixed models.
//! No session reconstruction, optimizer update, sampler draw or terminal lookup.
use super::*;
use crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Model {
    Parent {
        label: String,
        source: ExpandedModelSourceV1,
    },
    Checkpoint {
        label: String,
        config: PinnedFileV1,
        checkpoint: PinnedFileV1,
    },
    /// Ordinary expanded-trainer checkpoint, with its original source schema.
    NativeExpanded {
        label: String,
        source: ExpandedModelSourceV1,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryKind {
    #[default]
    PublicInput,
    NativeExpandedV3,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditCommand {
    #[serde(default)]
    pub trajectory_kind: TrajectoryKind,
    pub models: Vec<Model>,
    pub trajectories: Vec<PinnedFileV1>,
    pub output_directory: PathBuf,
    pub workers: usize,
    pub max_choice_rows_per_trajectory: usize,
    /// Qualification only: a loaded model must reproduce this many behavior
    /// rows exactly when its optimizer identity matches the archive.
    pub minimum_behavior_replay_rows: usize,
}

struct Loaded {
    label: String,
    policy: AuditPolicy,
    state_hash: String,
    identity: Value,
}

enum AuditPolicy {
    Public(PublicInputPlayPolicyV1),
    Native(FrozenPlayPolicyV1),
}

impl AuditPolicy {
    fn fork(&self) -> Result<Self, String> {
        match self {
            Self::Public(policy) => Ok(Self::Public(policy.fork_for_collection()?)),
            Self::Native(policy) => Ok(Self::Native(policy.fork_for_collection_v3()?)),
        }
    }

    fn score(
        &self,
        row: &DecisionRecordV1,
        auxiliary: Option<&PublicFeatureRowsV1>,
    ) -> Result<FrozenPlayDecisionScoresV1, String> {
        match self {
            Self::Public(policy) => {
                let auxiliary = auxiliary.ok_or("learner public row missing")?;
                let tensor = NativeFlatDecisionTensorV4 {
                    common: row.tensor.tensor(),
                };
                auxiliary.validate_tokens(&tensor.common.object_card_ids)?;
                policy.replay(&tensor, auxiliary)
            }
            Self::Native(policy) => match policy.feature_identity_v1().generation {
                FreshLineageGenerationV1::V3 => {
                    policy.score_training_tensor_v3(&NativeFlatDecisionTensorV3 {
                        common: row.tensor.tensor(),
                    })
                }
                FreshLineageGenerationV1::V4 => {
                    policy.score_training_tensor_v4(&NativeFlatDecisionTensorV4 {
                        common: row.tensor.tensor(),
                    })
                }
            },
        }
    }
}

/// Only ordinary public trajectories are audited. A search-opponent
/// trajectory carries its own schema and is refused rather than audited as
/// if a net had sampled its opponent rows.
pub(super) fn admits_public_trajectory(t: &Trajectory) -> bool {
    t.schema == "mtg-kernel-public-input-trajectory/v1"
        && t.search.is_none()
        && t.decisions.len() == t.auxiliary.len()
}

/// Internal view only. The input's original schema and state identity are
/// retained; native records never acquire synthetic public-feature rows.
struct SavedTrajectory {
    state_hash: String,
    episode: ExpandedEpisodeV1,
    decisions: Vec<DecisionRecordV1>,
    auxiliary: Vec<Option<PublicFeatureRowsV1>>,
}

fn read_trajectory(
    bytes: &[u8],
    kind: TrajectoryKind,
    models: &[Loaded],
) -> Result<SavedTrajectory, String> {
    match kind {
        TrajectoryKind::PublicInput => {
            ensure(
                models
                    .iter()
                    .all(|m| matches!(&m.policy, AuditPolicy::Public(_))),
                "public trajectories require public models",
            )?;
            let t: Trajectory = serde_json::from_slice(bytes).map_err(err)?;
            ensure(admits_public_trajectory(&t), "trajectory schema/rows differ")?;
            Ok(SavedTrajectory {
                state_hash: t.optimizer_state_sha256,
                episode: t.episode,
                decisions: t.decisions,
                auxiliary: t.auxiliary,
            })
        }
        TrajectoryKind::NativeExpandedV3 => {
            let t: ExpandedTrajectoryV1 = serde_json::from_slice(bytes).map_err(err)?;
            ensure(
                t.schema == FRESH_TRAJECTORY_SCHEMA,
                "native trajectory schema differs",
            )?;
            ensure(t.episode.learner_seat < 2, "invalid native learner seat")?;
            if let Some(seats) = &t.seat_behaviors {
                ensure(
                    seats[t.episode.learner_seat as usize].identity.state_sha256
                        == t.behavior_state_sha256,
                    "native learner behavior identity differs",
                )?;
            }
            for model in models {
                let AuditPolicy::Native(policy) = &model.policy else {
                    return Err("native trajectories require native expanded models".into());
                };
                let identity = policy.actual_model_identity_v1();
                ensure(
                    identity.feature_contract_digest == t.feature_contract_digest
                        && identity.feature_encoding_digest == t.feature_encoding_digest
                        && identity.card_db_hash == t.card_db_hash,
                    "native trajectory/model feature identity differs",
                )?;
            }
            Ok(SavedTrajectory {
                state_hash: t.behavior_state_sha256,
                episode: t.episode,
                decisions: t.decisions,
                auxiliary: Vec::new(),
            })
        }
    }
}

fn load(model: &Model) -> Result<Loaded, String> {
    match model {
        Model::Parent { label, source } => {
            let (base, initial) = initialize(source)?;
            let legacy = initial.snapshot_v1().map_err(err)?;
            let public = ProjectionSnapshot::zero();
            let state_hash =
                sha(&public_training::snapshot::encode(&legacy, &public).map_err(err)?);
            let identity = json!({"label":label,"source":source,"optimizer_sha256":state_hash,
                "base_model":base.actual_model_identity_v1(),"public_weights":"zero"});
            Ok(Loaded {
                label: label.clone(),
                policy: AuditPolicy::Public(PublicInputPlayPolicyV1::new(base, weights(&public)?)?),
                state_hash,
                identity,
            })
        }
        Model::Checkpoint {
            label,
            config,
            checkpoint,
        } => {
            let (policy, identity) = load_for_evaluation(config, checkpoint)?;
            let state_hash = identity["optimizer_sha256"]
                .as_str()
                .ok_or("missing optimizer identity")?
                .to_string();
            Ok(Loaded {
                label: label.clone(),
                policy: AuditPolicy::Public(policy),
                state_hash,
                identity: json!({"label":label,"model":identity}),
            })
        }
        Model::NativeExpanded { label, source } => {
            let (policy, identity) = load_expanded_inference_v1(source)?;
            Ok(Loaded {
                label: label.clone(),
                state_hash: identity.state_sha256.clone(),
                policy: AuditPolicy::Native(policy),
                identity: json!({"label":label,"source":source,"model":identity}),
            })
        }
    }
}

fn audit_one(
    pin: &PinnedFileV1,
    models: &[Loaded],
    maximum: usize,
    kind: TrajectoryKind,
) -> Result<(Value, usize), String> {
    let bytes = read_pinned_bytes(pin)?;
    let trajectory = read_trajectory(&bytes, kind, models)?;
    let learner = trajectory.episode.learner_seat;
    let eligible: Vec<_> = trajectory
        .decisions
        .iter()
        .enumerate()
        .filter(|(_, r)| r.actor == learner && r.logits.len() > 1)
        .map(|(i, _)| i)
        .collect();
    ensure(!eligible.is_empty(), "trajectory has no learner choices")?;
    let selected: Vec<_> = if eligible.len() <= maximum {
        eligible.clone()
    } else {
        (0..maximum)
            .map(|i| eligible[i * (eligible.len() - 1) / (maximum - 1)])
            .collect()
    };
    let mut rows = Vec::new();
    let mut replayed = 0;
    let mut sampler = WideCategoricalScratchV1::default();
    for index in selected {
        let row = &trajectory.decisions[index];
        let auxiliary = trajectory.auxiliary.get(index).and_then(Option::as_ref);
        ensure(
            (row.selected as usize) < row.logits.len(),
            "behavior selection out of bounds",
        )?;
        let mut scores = Vec::new();
        let mut distributions = Vec::new();
        let mut tops = Vec::new();
        for model in models {
            let output = model.policy.score(row, auxiliary)?;
            ensure(
                output.logits.len() == row.logits.len(),
                "model changed legal menu length",
            )?;
            if trajectory.state_hash == model.state_hash {
                ensure(
                    bits(&output.logits) == row.logits && output.value.to_bits() == row.value,
                    "same-state policy replay differs from archived behavior",
                )?;
                replayed += 1;
            }
            let masses = sampler.apportion(&output.logits).map_err(err)?;
            let denominator = 18446744073709551616.0f64;
            let probabilities: Vec<_> = masses.iter().map(|m| *m as f64 / denominator).collect();
            let mut top = 0;
            for i in 1..masses.len() {
                if masses[i] > masses[top] {
                    top = i;
                }
            }
            let entropy = -probabilities
                .iter()
                .filter(|p| **p > 0.0)
                .map(|p| p * p.ln())
                .sum::<f64>();
            scores.push(json!({"label":model.label,"logits_bits":bits(&output.logits),
                "value_bits":output.value.to_bits(),"top_action":top,"top_probability":probabilities[top],
                "entropy":entropy}));
            tops.push(top);
            distributions.push(probabilities);
        }
        let mut comparisons = Vec::new();
        for a in 0..models.len() {
            for b in a + 1..models.len() {
                let p = &distributions[a];
                let q = &distributions[b];
                let kl = p
                    .iter()
                    .zip(q)
                    .map(|(p, q)| p * (p / q).ln())
                    .sum::<f64>()
                    .max(0.0);
                let tv = p.iter().zip(q).map(|(p, q)| (p - q).abs()).sum::<f64>() * 0.5;
                comparisons.push(json!({"from":models[a].label,"to":models[b].label,
                "forward_kl":kl,"total_variation":tv,"top_action_changed":tops[a]!=tops[b]}));
            }
        }
        rows.push(json!({"archive_row":index,"step":row.step,"physical_decision_id":row.physical_decision_id,
            "substep_index":row.substep_index,"substep_count":row.substep_count,
            "behavior_selected":row.selected,"action_count":row.logits.len(),
            "tensor_sha256":sha(&serde_json::to_vec(&row.tensor).map_err(err)?),
            "models":scores,"comparisons":comparisons}));
    }
    Ok((
        json!({"schema":match kind { TrajectoryKind::PublicInput => "public-policy-replay-audit/v1",
                                    TrajectoryKind::NativeExpandedV3 => "native-expanded-policy-replay-audit/v1" },"trajectory":pin,
        "episode_id":trajectory.episode.id,"own":trajectory.episode.registered[learner as usize].label,
        "opponent":trajectory.episode.registered[1-learner as usize].label,
        "learner_seat":learner,"postboard":trajectory.episode.postboard,
        "eligible_choices":eligible.len(),"behavior_replay_rows":replayed,"rows":rows}),
        replayed,
    ))
}

pub fn run(command: AuditCommand) -> Result<Value, String> {
    ensure(
        (1..=8).contains(&command.workers)
            && (2..=256).contains(&command.max_choice_rows_per_trajectory),
        "audit worker/row bound differs",
    )?;
    ensure(
        (2..=4).contains(&command.models.len()) && (1..=256).contains(&command.trajectories.len()),
        "audit model/trajectory bound differs",
    )?;
    let models = command
        .models
        .iter()
        .map(load)
        .collect::<Result<Vec<_>, _>>()?;
    let mut labels = BTreeSet::new();
    for m in &models {
        ensure(
            !m.label.is_empty() && labels.insert(&m.label),
            "duplicate/empty model label",
        )?;
    }
    let identities: Vec<_> = models.iter().map(|m| m.identity.clone()).collect();
    let workers = (0..command.workers.min(command.trajectories.len()))
        .map(|_| {
            models
                .iter()
                .map(|m| {
                    Ok(Loaded {
                        label: m.label.clone(),
                        policy: m.policy.fork()?,
                        state_hash: m.state_hash.clone(),
                        identity: m.identity.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let next = AtomicUsize::new(0);
    let mut result = std::thread::scope(|scope| -> Result<Vec<(usize, Value, usize)>, String> {
        let mut handles = Vec::new();
        for owned in workers {
            let next = &next;
            let command = &command;
            handles.push(
                std::thread::Builder::new()
                    .stack_size(16 * 1024 * 1024)
                    .spawn_scoped(scope, move || {
                        let mut out = Vec::new();
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(pin) = command.trajectories.get(i) else {
                                break;
                            };
                            let (value, count) = audit_one(
                                pin,
                                &owned,
                                command.max_choice_rows_per_trajectory,
                                command.trajectory_kind,
                            )?;
                            out.push((i, value, count));
                        }
                        Ok::<_, String>(out)
                    })
                    .map_err(err)?,
            );
        }
        let mut all = Vec::new();
        let mut failure = None;
        for h in handles {
            match h.join() {
                Ok(Ok(mut rows)) => all.append(&mut rows),
                Ok(Err(e)) => failure = Some(e),
                Err(_) => failure = Some("audit worker panicked".into()),
            }
        }
        if let Some(e) = failure {
            return Err(e);
        }
        Ok(all)
    })?;
    result.sort_by_key(|(i, _, _)| *i);
    ensure(
        result.len() == command.trajectories.len()
            && result.iter().enumerate().all(|(i, r)| i == r.0),
        "audit missing/duplicate trajectory",
    )?;
    let replayed: usize = result.iter().map(|r| r.2).sum();
    ensure(
        replayed >= command.minimum_behavior_replay_rows,
        "too few exact behavior replay rows",
    )?;
    fs::create_dir(&command.output_directory).map_err(err)?;
    let mut rows = 0;
    for (i, value, _) in &result {
        rows += value["rows"].as_array().ok_or("missing audit rows")?.len();
        publish_json(
            &command.output_directory,
            &format!("trajectory-{i:03}.json"),
            value,
        )?;
    }
    let completion = json!({"schema":match command.trajectory_kind {
        TrajectoryKind::PublicInput => "public-policy-replay-audit-completion/v1",
        TrajectoryKind::NativeExpandedV3 => "native-expanded-policy-replay-audit-completion/v1" },"models":identities,
        "trajectories":result.len(),"choice_rows":rows,"exact_behavior_replay_rows":replayed,
        "sampler":WIDE_CATEGORICAL_SAMPLER_VERSION_V1,"probability_arithmetic":"Hamilton masses converted to f64; no draws",
        "terminal_outcomes_used":false});
    publish_json(&command.output_directory, "completion.json", &completion)?;
    Ok(completion)
}
