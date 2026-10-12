//! Generic, outcome-blind E teaching from one frozen student trajectory.
use super::*;
use crate::native_policy_train_step_v1::{ESupportedPolicyInputV1, ESupportedPolicyRowV1};
use crate::regret_census_v1::stage4a::e_supported::{self, VisibleFrame};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ESupportedSlotV1 {
    pub trajectory_index: usize,
    /// Registered case key, including lineage/phase/update/episode and role.
    pub case_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ESupportedTeacherOptionsV1 {
    pub teacher: ExpandedModelSourceV1,
    pub treatment: bool,
    /// Empty only for the declared ordinary persistence phase.
    pub slots: Vec<ESupportedSlotV1>,
    pub prior_deck_indices: Vec<usize>,
    pub selection_transition_cap: u64,
}

/// One fixed probe: a saved learner row and the recorded E target's live index.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ESupportedProbeV1 {
    pub probe_id: String,
    pub trajectory: PinnedFileV1,
    pub row_index: usize,
    pub target_live: usize,
}

/// Deployment-path CE and Hamilton mass of `source` on fixed E targets. The
/// saved trajectory is replayed to its row and the source's own collection
/// scorer runs there (no sampling, search, update or teacher query); the
/// encoded input must equal the saved row's.
pub(super) fn probe_score(
    source: &ExpandedModelSourceV1,
    probes: &[ESupportedProbeV1],
    output_directory: &Path,
) -> Result<Value, String> {
    ensure(
        !probes.is_empty() && probes.len() <= 4096,
        "E probe count outside 1..4096",
    )?;
    ensure(
        !output_directory.exists(),
        "E probe output directory must be fresh",
    )?;
    let (policy, identity) = load_expanded_inference_v1(source)?;
    let mut ids = BTreeSet::new();
    let mut rows = Vec::new();
    for probe in probes {
        ensure(ids.insert(&probe.probe_id), "duplicate E probe id")?;
        let trajectory: ExpandedTrajectoryV1 = read_pinned(&probe.trajectory)?;
        let row = trajectory
            .decisions
            .get(probe.row_index)
            .ok_or("E probe row outside trajectory")?;
        let episode = &trajectory.episode;
        ensure(
            episode.reset.is_none(),
            "E probes use ordinary-start trajectories",
        )?;
        let mut session = episode_session_v1(episode, &episode.configurations()?)?;
        for saved in &trajectory.decisions[..probe.row_index] {
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                return Err("E probe replay ended before its row".into());
            };
            ensure(
                d.step == saved.step && d.legal_action_count as usize == saved.logits.len(),
                "E probe replay row mismatch",
            )?;
            session
                .step(d.episode_id, d.step, saved.selected)
                .map_err(err)?;
        }
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            return Err("E probe replay ended before its row".into());
        };
        ensure(
            d.step == row.step
                && d.physical_decision_id == row.physical_decision_id
                && d.substep_index == row.substep_index
                && seat(d.acting_player) == row.actor
                && row.actor == episode.learner_seat
                && d.legal_action_count as usize == row.logits.len()
                && probe.target_live < row.logits.len(),
            "E probe root differs from its saved learner row",
        )?;
        let mut capture = policy.fork_for_collection_v3()?;
        capture.reset_sampling_v1([0, 0]);
        let (logits, tensor) = match trajectory_generation_v1(&trajectory) {
            FreshLineageGenerationV1::V3 => {
                let (_, scores, tensor) = capture.select_with_training_tensor_v3(&session)?;
                (scores.logits, TensorBitsV1::from_tensor(&tensor.common))
            }
            FreshLineageGenerationV1::V4 => {
                let (_, scores, tensor) = capture.select_with_training_tensor_v4(&session)?;
                (scores.logits, TensorBitsV1::from_tensor(&tensor.common))
            }
        };
        ensure(
            serde_json::to_vec(&tensor).map_err(err)?
                == serde_json::to_vec(&row.tensor).map_err(err)?,
            "E probe encoded input differs from the saved row",
        )?;
        ensure(
            logits.iter().all(|z| z.is_finite()),
            "E probe nonfinite logits",
        )?;
        let mut sampler = WideCategoricalScratchV1::default();
        let mass = sampler.apportion(&logits).map_err(err)?[probe.target_live];
        rows.push(json!({"probe_id":probe.probe_id,"trajectory":probe.trajectory,
            "row_index":probe.row_index,"target_live":probe.target_live,
            "ce":ce_readout(&logits, probe.target_live)?,
            "deployed_mass":mass as f64/(u64::MAX as f64+1.0),"deployed_mass_u128":mass.to_string(),
            "argmax":logits.iter().enumerate().fold(0, |b, (i, z)| if *z > logits[b] { i } else { b }),
            "logits_bits":logits.iter().map(|z| z.to_bits()).collect::<Vec<_>>()}));
    }
    let result = json!({"schema":"mtg-kernel-e-supported-probe-score/v1","method":e_supported::METHOD,
        "complete":true,"source":source,"identity":identity,"rows":rows,
        "runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256")});
    fs::create_dir_all(output_directory).map_err(err)?;
    publish_json(output_directory, "probes.json", &result)?;
    Ok(result)
}

fn rank(case: &str, row: usize) -> [u8; 32] {
    Sha256::digest(
        serde_json::to_vec(&json!({"case_id":case,"namespace":e_supported::METHOD,
        "role":"row_rank","row_ordinal":row}))
        .unwrap(),
    )
    .into()
}

pub(super) fn teach(
    trajectories: &[ExpandedTrajectoryV1],
    student: &FrozenPlayPolicyV1,
    student_hash: &str,
    groups: &[LearnerTensorGroupV1<'_>],
    options: &ESupportedTeacherOptionsV1,
) -> Result<(Value, ESupportedPolicyInputV1), String> {
    ensure(
        options.selection_transition_cap > 0 && options.selection_transition_cap <= 512_000,
        "invalid E transition cap",
    )?;
    ensure(
        !options.prior_deck_indices.is_empty()
            && options
                .prior_deck_indices
                .iter()
                .all(|&i| i < crate::runtime_decks::RUNTIME_DECKS.len()),
        "invalid E public prior",
    )?;
    ensure(
        options
            .prior_deck_indices
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            == options.prior_deck_indices.len(),
        "duplicate E prior deck",
    )?;
    let loaded = if options.slots.is_empty() {
        None
    } else {
        Some(load_expanded_inference_v1(&options.teacher)?)
    };
    if let Some((teacher, _)) = &loaded {
        ensure(
            teacher.feature_identity_v1() == student.feature_identity_v1(),
            "E teacher feature identity mismatch",
        )?;
    }
    let teacher_identity = loaded.as_ref().map(|(_, identity)| identity);
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    let mut packets = Vec::new();
    for slot in &options.slots {
        ensure(
            !slot.case_id.is_empty() && seen.insert(&slot.case_id),
            "duplicate or empty E case id",
        )?;
        let trajectory = trajectories
            .get(slot.trajectory_index)
            .ok_or("E scheduled trajectory outside batch")?;
        ensure(
            trajectory.learner_sampler.is_none(),
            "E requires the unchanged deployment collection sampler",
        )?;
        let episode = &trajectory.episode;
        let mut session = episode_session_v1(episode, &episode.configurations()?)?;
        // A reset cut lacks its complete visible prefix. Refuse rather than erase it.
        ensure(
            episode.reset.is_none(),
            "E ordinary-start teaching cannot erase a reset prefix",
        )?;
        let mut history: Vec<VisibleFrame> = Vec::new();
        let mut best: Option<(usize, [u8; 32], FastActorSessionV1, Vec<VisibleFrame>)> = None;
        let mut eligible = 0;
        for (index, row) in trajectory.decisions.iter().enumerate() {
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                return Err("E replay ended before recorded row".into());
            };
            ensure(
                d.step == row.step
                    && d.physical_decision_id == row.physical_decision_id
                    && d.substep_index == row.substep_index
                    && d.substep_count == row.substep_count
                    && seat(d.acting_player) == row.actor
                    && d.legal_action_count as usize == row.logits.len(),
                "E replay row mismatch",
            )?;
            if row.actor == episode.learner_seat && d.legal_action_count >= 2 {
                eligible += 1;
                let r = rank(&slot.case_id, index);
                if best
                    .as_ref()
                    .is_none_or(|(i, b, _, _)| (r, index) < (*b, *i))
                {
                    best = Some((index, r, session.clone(), history.clone()));
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
            matches!(session.current_response(),FastActorResponseV1::Terminal(t) if t==trajectory.terminal),
            "E replay terminal mismatch",
        )?;
        let Some((index, r, root, history)) = best else {
            packets.push(
                json!({"case_id":slot.case_id,"trajectory_index":slot.trajectory_index,
                "status":"no_eligible_row","episode":episode,"eligible":0}),
            );
            continue;
        };
        let row = &trajectory.decisions[index];
        let FastActorResponseV1::Decision(d) = root.current_response() else {
            unreachable!()
        };
        let mut capture = student.fork_for_collection_v3()?;
        capture.reset_sampling_v1([0, 0]);
        let actual_tensor = match trajectory_generation_v1(trajectory) {
            FreshLineageGenerationV1::V3 => {
                TensorBitsV1::from_tensor(&capture.select_with_training_tensor_v3(&root)?.2.common)
            }
            FreshLineageGenerationV1::V4 => {
                TensorBitsV1::from_tensor(&capture.select_with_training_tensor_v4(&root)?.2.common)
            }
        };
        let input_bytes = serde_json::to_vec(&actual_tensor).map_err(err)?;
        ensure(
            input_bytes == serde_json::to_vec(&row.tensor).map_err(err)?,
            "E live encoded input differs from learner row",
        )?;
        let opponent_source = episode
            .opponent
            .as_ref()
            .ok_or("E requires a frozen episode opponent")?;
        let (opponent, opponent_identity) = load_expanded_inference_v1(opponent_source)?;
        let recorded = trajectory
            .seat_behaviors
            .as_ref()
            .ok_or("E opponent behavior identity missing")?;
        ensure(
            recorded[1 - usize::from(episode.learner_seat)].identity == opponent_identity,
            "E frozen opponent identity mismatch",
        )?;
        let seeds = e_supported::RootSeeds {
            model: e_supported::METHOD.into(),
            root: slot.case_id.clone(),
        };
        let (target, mut packet) = e_supported::query(
            &root,
            d,
            &history,
            &loaded.as_ref().expect("queried slots loaded teacher").0,
            &opponent,
            &seeds,
            &options.prior_deck_indices,
            options.selection_transition_cap,
        )?;
        let mut matches = groups
            .iter()
            .enumerate()
            .flat_map(|(g, (_, ordinal, steps))| {
                steps
                    .iter()
                    .enumerate()
                    .filter(move |(_, (saved, _))| {
                        *ordinal == slot.trajectory_index && saved.step == row.step
                    })
                    .map(move |(s, (_, tensor))| (g, s, tensor))
            });
        let (group_index, substep_index, tensor) =
            matches.next().ok_or("E root missing from learner groups")?;
        ensure(
            matches.next().is_none(),
            "E root ambiguously bound to learner groups",
        )?;
        ensure(
            input_bytes == serde_json::to_vec(&TensorBitsV1::from_tensor(tensor)).map_err(err)?,
            "E input differs from prepared forward row",
        )?;
        let logits: Vec<f32> = row.logits.iter().map(|&b| f32::from_bits(b)).collect();
        ensure(
            logits.iter().all(|z| z.is_finite()),
            "E nonfinite student logits",
        )?;
        let target_readout = target
            .map(|a| {
                let mut sampler = WideCategoricalScratchV1::default();
                let mass = sampler.apportion(&logits).map_err(err)?[a];
                let ce = ce_readout(&logits, a)?;
                Ok::<_, String>(
                    json!({"ce":ce,"deployed_mass":mass as f64/(u64::MAX as f64+1.0),
                "deployed_mass_u128":mass.to_string(),"disagreement":row.selected as usize!=a}),
                )
            })
            .transpose()?;
        if let Some(target_index) = target {
            rows.push(ESupportedPolicyRowV1 {
                group_index,
                substep_index,
                legal_width: logits.len(),
                target_index,
            });
        }
        packet["case_id"] = json!(slot.case_id);
        packet["trajectory_index"] = json!(slot.trajectory_index);
        packet["row_ordinal"] = json!(index);
        packet["temporal_fraction"] = json!(index as f64 / trajectory.decisions.len() as f64);
        packet["rank"] = json!(hex(&r));
        packet["eligible"] = json!(eligible);
        packet["episode"] = json!(episode);
        packet["decision"] = json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
            "substep_index":d.substep_index,"substep_count":d.substep_count,"actor":row.actor,
            "environment_revision":d.environment_revision,"family":format!("{:?}",d.decision_kind),
            "legal_width":d.legal_action_count});
        packet["actual_student_action"] = json!(row.selected);
        packet["encoded_input_sha256"] = json!(hex(&Sha256::digest(&input_bytes)));
        packet["collection_logits"] = json!(row.logits);
        packet["readout"] = json!(target_readout);
        packet["opponent_identity"] = json!(opponent_identity);
        packets.push(packet);
    }
    let binary = fs::read(std::env::current_exe().map_err(err)?).map_err(err)?;
    let mut census: std::collections::BTreeMap<String, Value> = Default::default();
    for slot in &packets {
        let episode = &slot["episode"];
        let seat = episode["learner_seat"]
            .as_u64()
            .expect("typed learner seat") as usize;
        let key = serde_json::to_string(&json!({"deck":episode["selected"][seat]["label"],
            "seat":seat,"family":slot["decision"]["family"],"width":slot["decision"]["legal_width"],
            "temporal_quartile":slot["temporal_fraction"].as_f64().map(|f|(f*4.0) as u64)}))
        .map_err(err)?;
        let counts = census
            .entry(key)
            .or_insert(json!({"scheduled":0,"available":0,"valid":0,"disagreement":0}));
        for (name, increment) in [
            ("scheduled", true),
            ("available", slot["row_ordinal"].is_number()),
            ("valid", slot["selected_live"].is_number()),
            ("disagreement", slot["readout"]["disagreement"] == true),
        ] {
            if increment {
                counts[name] = json!(counts[name].as_u64().expect("counter") + 1);
            }
        }
    }
    let packet = json!({"schema":"mtg-kernel-e-supported-teacher-packet/v1","method":e_supported::METHOD,
        "runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),"runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "runtime_binary_sha256":hex(&Sha256::digest(binary)),"student_state_sha256":student_hash,
        "student_identity":student.identity_v1(),"feature_identity":{
            "contract":student.feature_identity_v1().feature_contract_digest,
            "encoding":student.feature_identity_v1().feature_encoding_digest},
        "teacher_identity":teacher_identity,"options":options,"scheduled":options.slots.len(),
        "valid":rows.len(),"census_by_deck_seat_family_width":census,"slots":packets});
    Ok((
        packet,
        ESupportedPolicyInputV1 {
            scheduled: options.slots.len(),
            rows,
            treatment: options.treatment,
        },
    ))
}

/// Bound into every E checkpoint; a continuing update or run must reproduce it.
pub fn continuation_identity(options: &ESupportedTeacherOptionsV1) -> Value {
    json!({"method":e_supported::METHOD,"teacher":options.teacher,"treatment":options.treatment,
        "prior_deck_indices":options.prior_deck_indices,"selection_transition_cap":options.selection_transition_cap,
        "runtime_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "runtime_tree":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256")})
}

fn ce_readout(logits: &[f32], target: usize) -> Result<f64, String> {
    ensure(
        logits.len() >= 2 && target < logits.len() && logits.iter().all(|z| z.is_finite()),
        "E invalid CE readout",
    )?;
    let maximum = logits
        .iter()
        .map(|&z| f64::from(z))
        .fold(f64::NEG_INFINITY, f64::max);
    let gaps: Vec<_> = logits.iter().map(|&z| f64::from(z) - maximum).collect();
    Ok(gaps.iter().map(|z| z.exp()).sum::<f64>().ln() - gaps[target])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn e_supported_ce_readout_preserves_large_finite_common_offsets() {
        let zero = ce_readout(&[0.0, 0.0], 0).unwrap();
        assert_eq!(zero, 2.0_f64.ln());
        for offset in [7.0, 1.0e20, -1.0e20, f32::MAX] {
            assert_eq!(ce_readout(&[offset, offset], 0).unwrap(), zero);
        }
        assert!(ce_readout(&[0.0, f32::NAN], 0).is_err());
    }
}
