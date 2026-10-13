use super::*;
use crate::expanded_deck_training_v1::tests::checkpoint_fixture_v1;

const LR: f32 = LEARNING_RATE;

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "archive-distillation-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn write_pinned(path: &Path, value: &impl Serialize) -> PinnedFileV1 {
    let bytes = serde_json::to_vec(value).unwrap();
    fs::write(path, &bytes).unwrap();
    PinnedFileV1 {
        path: path.to_path_buf(),
        sha256: sha(&bytes),
    }
}

fn pin(name: &str, fill: char) -> PinnedFileV1 {
    PinnedFileV1 {
        path: name.into(),
        sha256: fill.to_string().repeat(64),
    }
}

/// Recomputes a checkpoint's state hash from its own fields, where the
/// fields still hash (a moved gauge anchor no longer does).
fn rehash(saved: &mut ExpandedCheckpointV1, template: &[NativeNamedParameterV1]) {
    let snapshot = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters: restore_parameters(&saved.parameters, template).unwrap(),
        first_moments: restore_parameters(&saved.first_moments, template).unwrap(),
        second_moments: restore_parameters(&saved.second_moments, template).unwrap(),
    };
    if let Ok(hash) = snapshot.state_sha256_v1() {
        saved.state_sha256 = hex(&hash);
    }
}

/// An ordinary GAE parent with a trained-looking Adam state and update
/// inputs, written to disk.
fn parent_fixture(
    root: &Path,
) -> (
    FrozenPlayPolicyV1,
    NativePolicyValueNetV1,
    ExpandedCheckpointV1,
    PinnedFileV1,
) {
    let (policy, model, mut parent) = checkpoint_fixture_v1();
    parent.loss_identity = GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into();
    parent.gamma_bits = Some(1.0_f32.to_bits());
    parent.gae_lambda_bits = Some(0.9_f32.to_bits());
    parent.entropy_coefficient_bits = Some(0.0_f32.to_bits());
    parent.learning_rate_bits = 0.0001_f32.to_bits();
    parent.adam_step = 35_640;
    parent.first_moments[0].values[16] = 0.125_f32.to_bits();
    parent.second_moments[0].values[16] = 0.0625_f32.to_bits();
    parent.trajectories = vec![pin("episode-0000.json", 'c')];
    rehash(&mut parent, &model.parameter_snapshot_v1());
    let file = write_pinned(&root.join("parent.json"), &parent);
    (policy, model, parent, file)
}

fn provenance(
    parent: &ExpandedCheckpointV1,
    file: &PinnedFileV1,
) -> ArchiveDistillationProvenanceV1 {
    ArchiveDistillationProvenanceV1 {
        method: METHOD.into(),
        lineage: "r1".into(),
        arm: ArchiveDistillationArmV1::Uniform,
        parent: file.clone(),
        parent_state_sha256: parent.state_sha256.clone(),
        games: pin("games.jsonl", 'd'),
        archive_sources: vec![pin("ARCHIVE.jsonl", '5'), pin("a-0001.jsonl", '6')],
        bank: pin("bank.json", 'e'),
        bank_rows_sha256: "f".repeat(64),
        anchor_sha256: "1".repeat(64),
        steps: 179,
        learning_rate_bits: LR.to_bits(),
        phase1_state_sha256: "2".repeat(64),
        runtime_commit: "test".into(),
    }
}

/// The parent's parameters with one embedding value moved, as phase 1 would.
fn distilled(
    parent: &ExpandedCheckpointV1,
    template: &[NativeNamedParameterV1],
) -> Vec<NativeNamedParameterV1> {
    let mut parameters = restore_parameters(&parent.parameters, template).unwrap();
    parameters
        .iter_mut()
        .find(|p| p.name == "card_embedding.weight")
        .unwrap()
        .values[17] += 0.5;
    parameters
}

#[test]
fn a_phase1_checkpoint_carries_its_identity_and_the_parents_exact_optimizer_state() {
    let root = temp_root("checkpoint-shape");
    let (mut policy, model, parent, file) = parent_fixture(&root);
    let template = model.parameter_snapshot_v1();
    let parameters = distilled(&parent, &template);
    let saved = checkpoint_v1(&parent, &parameters, provenance(&parent, &file)).unwrap();
    assert_eq!(saved.loss_identity, PHASE1_LOSS_IDENTITY_V1);
    assert!(saved.trajectories.is_empty());
    assert!(saved.e_supported.is_none() && saved.exploration.is_none());
    assert_eq!(
        (
            saved.gamma_bits,
            saved.gae_lambda_bits,
            saved.entropy_coefficient_bits
        ),
        (None, None, None)
    );
    assert_eq!(saved.learning_rate_bits, 0.0001_f32.to_bits());
    assert_eq!(saved.value_coefficient_bits, 0.0_f32.to_bits());
    assert_eq!(saved.adam_step, 35_640);
    assert_eq!(
        saved.scorer_bias_anchor_bits,
        parent.scorer_bias_anchor_bits
    );
    assert_eq!(saved.first_moments, parent.first_moments);
    assert_eq!(saved.second_moments, parent.second_moments);
    assert_ne!(saved.parameters, parent.parameters);
    // The ordinary loader restores exactly the phase-1 parameters with the
    // parent's Adam state.
    let restored = restore_checkpoint_state_v1(&saved, &mut policy, model.clone()).unwrap();
    assert_eq!(
        hex(&restored.state_sha256_v1().unwrap()),
        saved.state_sha256
    );
    assert_eq!(restored.adam_step_v1(), 35_640);
    assert!(same_parameter_bits(
        &restored.snapshot_v1().unwrap().parameters,
        &parameters
    ));
    // An ordinary checkpoint carries no provenance and keeps its bytes.
    validate_phase1_checkpoint_v1(&parent).unwrap();
    assert!(serde_json::to_value(&parent)
        .unwrap()
        .get("archive_distillation")
        .is_none());
    let bytes = serde_json::to_vec(&saved).unwrap();
    let reread: ExpandedCheckpointV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&reread).unwrap(), bytes);
    let mut value = serde_json::to_value(&saved).unwrap();
    value["archive_distillation"]["extra"] = json!(1);
    assert!(serde_json::from_value::<ExpandedCheckpointV1>(value).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_phase1_checkpoint_fails_closed_unless_its_named_parent_matches_bit_for_bit() {
    let root = temp_root("checkpoint-ancestry");
    let (mut policy, model, parent, file) = parent_fixture(&root);
    let template = model.parameter_snapshot_v1();
    let parameters = distilled(&parent, &template);
    let saved = checkpoint_v1(&parent, &parameters, provenance(&parent, &file)).unwrap();
    type Case = (&'static str, &'static str, fn(&mut ExpandedCheckpointV1));
    let cases: [Case; 15] = [
        ("first moment", "exact optimizer state", |s| {
            s.first_moments[0].values[16] = 0.25_f32.to_bits()
        }),
        ("second moment", "exact optimizer state", |s| {
            s.second_moments[0].values[16] = 0.03125_f32.to_bits()
        }),
        ("Adam step", "exact optimizer state", |s| s.adam_step += 1),
        ("gauge anchor", "exact optimizer state", |s| {
            s.scorer_bias_anchor_bits ^= 1
        }),
        ("missing provenance", "identity differs", |s| {
            s.archive_distillation = None
        }),
        ("relabeled as GAE", "identity differs", |s| {
            s.loss_identity = GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into();
            s.gamma_bits = Some(1.0_f32.to_bits());
            s.gae_lambda_bits = Some(0.9_f32.to_bits());
            s.entropy_coefficient_bits = Some(0);
        }),
        ("GAE scalars", "fields differ", |s| {
            s.gamma_bits = Some(1.0_f32.to_bits())
        }),
        ("learning rate", "fields differ", |s| {
            s.learning_rate_bits = 0.0003_f32.to_bits()
        }),
        ("value coefficient", "fields differ", |s| {
            s.value_coefficient_bits = 0.5_f32.to_bits()
        }),
        ("update inputs", "fields differ", |s| {
            s.trajectories = vec![pin("episode-0000.json", 'c')]
        }),
        ("method", "fields differ", |s| {
            s.archive_distillation.as_mut().unwrap().method = "another".into()
        }),
        ("recorded learning rate", "fields differ", |s| {
            s.archive_distillation.as_mut().unwrap().learning_rate_bits = 0
        }),
        ("parent state", "named ordinary checkpoint", |s| {
            s.archive_distillation.as_mut().unwrap().parent_state_sha256 = "0".repeat(64)
        }),
        ("parent bytes", "SHA differs", |s| {
            s.archive_distillation.as_mut().unwrap().parent.sha256 = "0".repeat(64)
        }),
        ("parent path", "", |s| {
            s.archive_distillation.as_mut().unwrap().parent.path = "missing.json".into()
        }),
    ];
    for (name, expected, mutate) in cases {
        let mut changed = saved.clone();
        mutate(&mut changed);
        // Keep the state hash consistent, so only the ancestry check speaks.
        rehash(&mut changed, &template);
        let error = restore_checkpoint_state_v1(&changed, &mut policy, model.clone())
            .err()
            .unwrap_or_else(|| panic!("{name} accepted"));
        assert!(error.contains(expected), "{name}: {error}");
    }
    // A phase-1 checkpoint is never a parent.
    let chain_file = write_pinned(&root.join("chain.json"), &saved);
    let mut chained = checkpoint_v1(&saved, &parameters, provenance(&saved, &chain_file)).unwrap();
    let error = restore_checkpoint_state_v1(&chained, &mut policy, model.clone()).unwrap_err();
    assert!(error.contains("named ordinary checkpoint"), "{error}");
    // The parent's feature identity must be the checkpoint's own.
    let mut other = parent.clone();
    other.card_db_hash = "0".repeat(16);
    let other_file = write_pinned(&root.join("other.json"), &other);
    chained = checkpoint_v1(&parent, &parameters, provenance(&other, &other_file)).unwrap();
    let error = restore_checkpoint_state_v1(&chained, &mut policy, model).unwrap_err();
    assert!(
        error.contains("identity differs from its parent's"),
        "{error}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn archive_distillation_commands_keep_their_wire_shape() {
    let source = json!({"play_import":{"path":"import.json","sha256":"a".repeat(64)},
        "feature_transfer":{"expected_feature_contract_digest":"b",
            "expected_feature_encoding_digest":"c"},"checkpoint":null});
    let file = json!({"path":"x.json","sha256":"d".repeat(64)});
    let anchor = json!({"training":[file],"held_out":[file]});
    let phase1 = json!({"mode":"archive_distillation_phase1","parent":source,"lineage":"r1",
        "arm":"line_decisions","games":file,"bank":file,"anchor":anchor,
        "output_directory":"out"});
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(phase1.clone()).unwrap();
    let ExpandedTrainingCommandV1::ArchiveDistillationPhase1 {
        arm,
        stop_after_steps,
        dry_run,
        ..
    } = &command
    else {
        panic!("phase-1 command")
    };
    assert_eq!(*arm, ArchiveDistillationArmV1::LineDecisions);
    assert_eq!((*stop_after_steps, *dry_run), (None, false));
    // Defaults stay off the wire.
    assert_eq!(serde_json::to_value(&command).unwrap(), phase1);
    let mut rerun = phase1.clone();
    rerun["stop_after_steps"] = json!(32);
    rerun["dry_run"] = json!(true);
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(rerun.clone()).unwrap();
    assert_eq!(serde_json::to_value(&command).unwrap(), rerun);
    let mut unknown = phase1;
    unknown["learning_rate"] = json!(0.001);
    assert!(serde_json::from_value::<ExpandedTrainingCommandV1>(unknown).is_err());
    let bank = json!({"mode":"archive_distillation_bank","parent":source,"lineage":"r2",
        "games":file,"sources":[file],"output_directory":"out"});
    assert!(matches!(
        serde_json::from_value(bank).unwrap(),
        ExpandedTrainingCommandV1::ArchiveDistillationBank { .. }
    ));
    let probe = json!({"mode":"archive_distillation_probe","parent":source,"lineage":"r2",
        "candidates":[{"id":"parent","source":source}],
        "banks":[{"role":"training","games":file,"bank":file}],"anchor":anchor,
        "offers":[file],"output_directory":"out"});
    assert!(matches!(
        serde_json::from_value(probe).unwrap(),
        ExpandedTrainingCommandV1::ArchiveDistillationProbe { .. }
    ));
    assert_eq!(
        serde_json::to_value(ArchiveDistillationArmV1::Uniform).unwrap(),
        json!("uniform")
    );
}

fn line(
    self_offered: bool,
    self_chosen: bool,
    giant_offered: bool,
    giant_chosen: bool,
) -> LineFlagsV1 {
    LineFlagsV1 {
        self_offered,
        self_chosen,
        giant_offered,
        giant_chosen,
        ..LineFlagsV1::default()
    }
}

fn archive_row(
    game: &str,
    step: u64,
    line: LineFlagsV1,
) -> (ArchiveRowV1, NativeFlatDecisionTensorV2) {
    (
        ArchiveRowV1 {
            game: game.into(),
            step,
            physical_decision_id: step,
            substep: 0,
            k: 3,
            target: 1,
            chooser: "reused".into(),
            edge_backups: Some(8),
            game_quartile: 0,
            line,
            self_actions: Vec::new(),
            giant_actions: Vec::new(),
            parent_logits: vec![0; 3],
            parent_value: 0,
            tensor_sha256: String::new(),
        },
        NativeFlatDecisionTensorV2::default(),
    )
}

fn anchor_rows(count: usize) -> Vec<AnchorRowV1> {
    (0..count)
        .map(|index| AnchorRowV1 {
            episode: format!("anchor-{}", index / 10),
            step: (index % 10) as u64,
            learner_deck: "Affinity/0123456789ab".into(),
            logits: vec![0; 2],
            value: 0,
            tensor: NativeFlatDecisionTensorV2::default(),
        })
        .collect()
}

/// Rows 5 and 40 are tree-chosen line rows (self-target, Dread Return on
/// Giant); row 41 is a declined self-target offer.
fn bank_rows(count: usize) -> Vec<(ArchiveRowV1, NativeFlatDecisionTensorV2)> {
    (0..count)
        .map(|index| {
            let flags = match index {
                5 => line(true, true, false, false),
                40 => line(false, false, true, true),
                41 => line(true, false, false, false),
                _ => LineFlagsV1::default(),
            };
            archive_row(&format!("game-{}", index / 20), (index % 20) as u64, flags)
        })
        .collect()
}

fn sorted(mut values: Vec<usize>) -> Vec<usize> {
    values.sort_unstable();
    values
}

#[test]
fn uniform_plan_draws_every_row_once_per_epoch_and_cycles_the_anchor() {
    let rows = bank_rows(70);
    let selected: Vec<usize> = (0..70).collect();
    let anchor = anchor_rows(300);
    let plan = plan_v1(
        &rows,
        &selected,
        &anchor,
        "r1",
        ArchiveDistillationArmV1::Uniform,
    )
    .unwrap();
    assert_eq!(plan.epoch_steps, 3);
    assert_eq!(plan.checkpoints, vec![3, 6, 12, 24, 48, 96]);
    assert_eq!(plan.steps.len(), 96);
    let mut orders = BTreeSet::new();
    for epoch in plan.steps.chunks(3) {
        assert_eq!(
            epoch.iter().map(|s| s.archive.len()).collect::<Vec<_>>(),
            vec![32, 32, 6]
        );
        assert!(epoch.iter().all(|s| s.denominator == ARCHIVE_BATCH));
        let order: Vec<usize> = epoch.iter().flat_map(|s| s.archive.clone()).collect();
        assert_eq!(sorted(order.clone()), selected);
        orders.insert(order);
    }
    assert!(orders.len() > 1, "each epoch has its own order");
    let draws: Vec<usize> = plan.steps.iter().flat_map(|s| s.anchor.clone()).collect();
    assert!(plan.steps.iter().all(|s| s.anchor.len() == ANCHOR_BATCH));
    for pass in draws
        .chunks(anchor.len())
        .filter(|pass| pass.len() == anchor.len())
    {
        assert_eq!(sorted(pass.to_vec()), (0..anchor.len()).collect::<Vec<_>>());
    }
    // The same inputs give the same plan; another lineage, another order.
    let again = plan_v1(
        &rows,
        &selected,
        &anchor,
        "r1",
        ArchiveDistillationArmV1::Uniform,
    )
    .unwrap();
    assert!(plan
        .steps
        .iter()
        .zip(&again.steps)
        .all(|(a, b)| a.archive == b.archive && a.anchor == b.anchor));
    let other = plan_v1(
        &rows,
        &selected,
        &anchor,
        "r2",
        ArchiveDistillationArmV1::Uniform,
    )
    .unwrap();
    assert_ne!(plan.steps[0].archive, other.steps[0].archive);
    // Exposure at each checkpoint: every row drawn once per epoch, and the
    // line rows' summed archive-term weight.
    let exposure = exposure_v1(&plan, &rows, &selected, anchor.len());
    let exposure = exposure.as_array().unwrap();
    assert_eq!(exposure.len(), 6);
    for (entry, epochs) in exposure.iter().zip([1u64, 2, 4, 8, 16, 32]) {
        assert_eq!(entry["draws_per_row_min"], json!(epochs));
        assert_eq!(entry["draws_per_row_max"], json!(epochs));
        let chosen = entry["line_chosen_weight"].as_f64().unwrap();
        let offered = entry["line_offered_weight"].as_f64().unwrap();
        assert!((chosen - 2.0 * epochs as f64 / 32.0).abs() < 1e-12);
        assert!((offered - 3.0 * epochs as f64 / 32.0).abs() < 1e-12);
    }
    let last = &exposure[5];
    let anchor_draws = 96 * ANCHOR_BATCH / anchor.len();
    assert_eq!(last["anchor_draws_per_row_min"], json!(anchor_draws));
    assert_eq!(last["anchor_draws_per_row_max"], json!(anchor_draws + 1));
}

#[test]
fn line_plan_uses_every_row_each_step_up_to_one_batch() {
    let rows = bank_rows(70);
    let selected: Vec<usize> = (0..70)
        .filter(|&i| ArchiveDistillationArmV1::LineDecisions.admits(&rows[i].0))
        .collect();
    assert_eq!(selected, vec![5, 40, 41]);
    assert!((0..70).all(|i| ArchiveDistillationArmV1::Uniform.admits(&rows[i].0)));
    let anchor = anchor_rows(256);
    let plan = plan_v1(
        &rows,
        &selected,
        &anchor,
        "r1",
        ArchiveDistillationArmV1::LineDecisions,
    )
    .unwrap();
    assert_eq!(plan.checkpoints, LINE_STEPS.to_vec());
    assert_eq!(plan.steps.len(), 1024);
    assert!(plan
        .steps
        .iter()
        .all(|s| s.archive == selected && s.denominator == 3));
    // More line rows than one batch: U's epochs, still 1,024 steps.
    let many: Vec<usize> = (0..40).collect();
    let plan = plan_v1(
        &rows,
        &many,
        &anchor,
        "r1",
        ArchiveDistillationArmV1::LineDecisions,
    )
    .unwrap();
    assert_eq!(plan.epoch_steps, 2);
    assert_eq!(plan.steps.len(), 1024);
    assert_eq!(
        plan.steps[..2]
            .iter()
            .map(|s| s.archive.len())
            .collect::<Vec<_>>(),
        vec![32, 8]
    );
    assert!(plan.steps.iter().all(|s| s.denominator == ARCHIVE_BATCH));
    // No rows, or an anchor smaller than one batch, refuse.
    assert!(plan_v1(&rows, &[], &anchor, "r1", ArchiveDistillationArmV1::Uniform).is_err());
    assert!(plan_v1(
        &rows,
        &selected,
        &anchor_rows(255),
        "r1",
        ArchiveDistillationArmV1::Uniform
    )
    .is_err());
}

fn deck(id: &str) -> ExpandedDeckListV1 {
    let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let cards = registration.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    }
}

fn real_episode(id: &str, seed: u64, learner_seat: u8) -> ExpandedEpisodeV1 {
    let decks = if learner_seat == 0 {
        [deck("Spy"), deck("Terror")]
    } else {
        [deck("Terror"), deck("Spy")]
    };
    ExpandedEpisodeV1 {
        id: id.into(),
        seed,
        starting_player: 1,
        learner_seat,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: decks.clone(),
        selected: decks,
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 200_000,
        runtime_rules: Some(EpisodeRuntimeRulesV1::ResolutionBoundaryV1),
        reset: None,
    }
}

/// The parent's captures at the first `count` decisions with two or more
/// legal actions of a real game, taking action 0 throughout.
fn real_anchor_rows(policy: &FrozenPlayPolicyV1, count: usize) -> Vec<AnchorRowV1> {
    let episode = real_episode("real-anchor", 2_026_101_301, 0);
    let mut session = episode_session_v1(&episode, &episode.configurations().unwrap()).unwrap();
    let mut rows = Vec::new();
    while rows.len() < count {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("the fixture game ends before its rows")
        };
        if d.legal_action_count >= 2 {
            let (scores, tensor) = capture_v1(policy, &session).unwrap();
            rows.push(AnchorRowV1 {
                episode: episode.id.clone(),
                step: d.step,
                learner_deck: "Spy".into(),
                logits: bits(&scores.logits),
                value: scores.value.to_bits(),
                tensor,
            });
        }
        session.step(d.episode_id, d.step, 0).unwrap();
    }
    rows
}

fn softmax_target(probabilities: Vec<f64>) -> LineBSoftmaxV1 {
    LineBSoftmaxV1 {
        log_probabilities: probabilities.iter().map(|p| p.ln()).collect(),
        weight_sum: probabilities.iter().sum(),
        probabilities,
    }
}

fn anchor_input<'a>(
    rows: &'a [AnchorRowV1],
    targets: &'a [LineBSoftmaxV1],
) -> ExternalAuxiliaryInputV1<'a> {
    ExternalAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: ANCHOR_COEFFICIENT,
        denominator: rows.len(),
        rows: rows
            .iter()
            .zip(targets)
            .map(|(row, target)| ExternalAuxiliaryRowV1 {
                encoded: view_v1(&row.tensor, FreshLineageGenerationV1::V3),
                target,
            })
            .collect(),
    }
}

/// The pre-flight passes only with binary64 softmax targets of the recorded
/// logit bits, on fresh moments. Deployed probabilities, a one-ulp logit
/// change or a stepped optimizer fail it; on moved moments even exact
/// targets move the parameters.
#[test]
fn anchor_preflight_requires_exact_targets_on_fresh_moments() {
    let policy = FrozenPlayPolicyV1::training_fixture_v3();
    let model = policy.training_model_v3();
    let generation = FreshLineageGenerationV1::V3;
    let rows = real_anchor_rows(&policy, 3);
    // The collector's capture is the training forward, bit for bit.
    assert_eq!(
        verify_parent_outputs_v1(
            &model,
            generation,
            rows.iter()
                .map(|row| (&row.tensor, row.logits.as_slice(), row.value))
        ),
        Ok(3)
    );
    let fresh = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let exact = anchor_targets_v1(&rows).unwrap();
    assert_eq!(
        anchor_identity_preflight_v1(&fresh, generation, &rows, &exact),
        Ok(3)
    );
    assert_eq!(fresh.adam_step_v1(), 0, "the pre-flight steps a copy");
    let deployed: Vec<LineBSoftmaxV1> = rows
        .iter()
        .map(|row| softmax_target(deployed_v1(&recorded_logits(&row.logits)).unwrap()))
        .collect();
    let nudged: Vec<LineBSoftmaxV1> = rows
        .iter()
        .map(|row| {
            let mut logits = recorded_logits(&row.logits);
            logits[0] = f32::from_bits(logits[0].to_bits() + 1);
            line_b_softmax_v1(&f64_logits(&logits)).unwrap()
        })
        .collect();
    for (name, targets) in [("deployed", &deployed), ("nudged", &nudged)] {
        let error = anchor_identity_preflight_v1(&fresh, generation, &rows, targets).unwrap_err();
        assert!(error.contains("not exact"), "{name}: {error}");
    }
    let mut stepped = fresh.clone();
    stepped
        .train_step_auxiliary_only_v1(generation, LR, &[&anchor_input(&rows, &exact)])
        .unwrap();
    assert_eq!(stepped.adam_step_v1(), 1);
    assert!(same_parameter_bits(
        &stepped.model_v1().parameter_snapshot_v1(),
        &model.parameter_snapshot_v1()
    ));
    let error = anchor_identity_preflight_v1(&stepped, generation, &rows, &exact).unwrap_err();
    assert!(error.contains("fresh phase-1 optimizer"), "{error}");
    // Why fresh moments: after one step toward a far target, an exact
    // anchor step still moves the parameters through momentum.
    let far: Vec<LineBSoftmaxV1> = rows
        .iter()
        .map(|row| one_hot_target_v1(row.logits.len(), row.logits.len() - 1).unwrap())
        .collect();
    let mut moving = fresh.clone();
    moving
        .train_step_auxiliary_only_v1(generation, LR, &[&anchor_input(&rows, &far)])
        .unwrap();
    let before = moving.model_v1().parameter_snapshot_v1();
    moving
        .train_step_auxiliary_only_v1(generation, LR, &[&anchor_input(&rows, &exact)])
        .unwrap();
    assert!(!same_parameter_bits(
        &moving.model_v1().parameter_snapshot_v1(),
        &before
    ));
}

/// One real game recorded as the archive did (`guide_record` starts and
/// `guide` decisions): every focal physical decision start with its replay
/// identity, every focal choice with two or more legal actions with a
/// chooser and its Spy flags, and the focal end. Actions follow a fixed
/// arithmetic rule; forced focal decisions take action 0.
fn recorded_game(id: &str, episode: ExpandedEpisodeV1) -> ArchiveGameV1 {
    let mut session = episode_session_v1(&episode, &episode.configurations().unwrap()).unwrap();
    let focal = episode.learner_seat;
    let (mut actions, mut starts, mut decisions) = (Vec::new(), Vec::new(), Vec::new());
    let end = loop {
        let d = match session.current_response() {
            FastActorResponseV1::Decision(d) => d,
            FastActorResponseV1::Terminal(terminal) => {
                assert_eq!(
                    terminal.terminal_classification,
                    TerminalClassificationV1::Natural
                );
                break match terminal.winner {
                    None => "draw",
                    Some(winner) if seat(winner) == focal => "win",
                    Some(_) => "loss",
                };
            }
        };
        let k = d.legal_action_count;
        let action = if k >= 2 {
            ((d.step * 7 + 3) % u64::from(k)) as u32
        } else {
            0
        };
        if seat(d.acting_player) == focal {
            if d.substep_index == 0 {
                let (obs_hash, menu_hash) = decision_replay_key(&session, d).unwrap();
                starts.push(ArchiveStartV1 {
                    step: d.step,
                    physical_decision_id: d.physical_decision_id,
                    k,
                    obs_hash,
                    menu_hash,
                    state_hash: format!("{:016x}", session.diagnostic_state_hash()),
                });
            }
            if k >= 2 {
                let (flags, _, _) = line_flags_v1(&session, focal, action);
                let chooser = ["new_root", "reused", "plain"][decisions.len() % 3];
                decisions.push(ArchiveDecisionV1 {
                    step: d.step,
                    physical_decision_id: d.physical_decision_id,
                    substep: d.substep_index,
                    k,
                    chooser: chooser.into(),
                    edge_backups: (chooser != "plain").then_some(8 + d.step),
                    spy_cast_offered: flags.cast_offered,
                    spy_cast_chosen: flags.cast_chosen,
                    spy_self_offered: flags.self_offered,
                    spy_self_chosen: flags.self_chosen,
                    dr_giant_offered: flags.giant_offered,
                    dr_giant_chosen: flags.giant_chosen,
                });
            }
        }
        actions.push(action);
        session.step(d.episode_id, d.step, action).unwrap();
    };
    ArchiveGameV1 {
        id: id.into(),
        lineage: "r1".into(),
        episode,
        actions,
        starts,
        decisions,
        end: end.into(),
    }
}

/// Extraction keeps every supported tree choice of a faithful record, with
/// the parent's own capture there, and drops a game whole when any recorded
/// start, choice, flag or end differs from the replay.
#[test]
fn extraction_binds_rows_to_the_recorded_game_and_drops_any_mismatch() {
    let policy = FrozenPlayPolicyV1::training_fixture_v3();
    let model = policy.training_model_v3();
    let game = recorded_game(
        "archive-r1-k0-v6-s1",
        real_episode("archive-r1-k0-v6-s1", 2_026_101_302, 1),
    );
    let rows = extract_game_v1(&game, &policy).unwrap();
    let supported: Vec<&ArchiveDecisionV1> = game
        .decisions
        .iter()
        .filter(|d| d.chooser != "plain")
        .collect();
    assert!(supported.len() >= 3, "the fixture game has supported rows");
    assert_eq!(rows.len(), supported.len());
    for ((row, tensor), record) in rows.iter().zip(&supported) {
        assert_eq!(
            (row.step, row.physical_decision_id, row.substep, row.k),
            (
                record.step,
                record.physical_decision_id,
                record.substep,
                record.k
            )
        );
        assert_eq!(row.target, game.actions[row.step as usize]);
        assert_eq!(row.chooser, record.chooser);
        assert_eq!(row.edge_backups, record.edge_backups);
        assert_eq!(row.line, LineFlagsV1::of_record(record));
        assert_eq!(row.tensor_sha256, tensor_sha256_v1(tensor).unwrap());
        assert!(row.game_quartile <= 3);
    }
    assert_eq!(
        verify_parent_outputs_v1(
            &model,
            FreshLineageGenerationV1::V3,
            rows.iter().map(|(row, tensor)| (
                tensor,
                row.parent_logits.as_slice(),
                row.parent_value
            ))
        ),
        Ok(rows.len())
    );
    type Case = (&'static str, &'static str, fn(&mut ArchiveGameV1));
    let cases: [Case; 10] = [
        ("start state", "recorded observation, menu or state", |g| {
            g.starts[2].state_hash = "0".repeat(16)
        }),
        (
            "start observation",
            "recorded observation, menu or state",
            |g| g.starts[0].obs_hash = "0".repeat(64),
        ),
        ("game seed", "recorded observation, menu or state", |g| {
            g.episode.seed += 1
        }),
        ("choice substep", "differs from its record", |g| {
            g.decisions[1].substep += 1
        }),
        ("Spy flag", "Spy flags differ", |g| {
            g.decisions[0].spy_self_offered = !g.decisions[0].spy_self_offered
        }),
        ("chooser", "unknown archive chooser", |g| {
            g.decisions[0].chooser = "other".into()
        }),
        ("end", "end differs", |g| {
            g.end = if g.end == "win" { "loss" } else { "win" }.into()
        }),
        ("short actions", "does not end at its last action", |g| {
            g.actions.pop();
        }),
        ("extra start", "outlast the replay", |g| {
            let last = g.starts.last().unwrap().clone();
            g.starts.push(last);
        }),
        ("embedded opponent", "no recorded opponent", |g| {
            g.episode.opponent = Some(ExpandedModelSourceV1 {
                play_import: pin("import.json", 'a'),
                feature_transfer: FrozenPlayObservationTransferV3 {
                    expected_feature_contract_digest: String::new(),
                    expected_feature_encoding_digest: String::new(),
                },
                checkpoint: None,
            })
        }),
    ];
    let mut games = vec![game.clone()];
    for (name, expected, mutate) in cases {
        let mut changed = game.clone();
        mutate(&mut changed);
        let error = extract_game_v1(&changed, &policy)
            .err()
            .unwrap_or_else(|| panic!("{name} accepted"));
        assert!(error.contains(expected), "{name}: {error}");
        changed.id = format!("{}-{name}", game.id);
        games.push(changed);
    }
    // A dropped game is listed with its reason and leaves the others whole.
    let inputs = ArchiveInputsV1 {
        state: NativePolicyValueTrainStateV1::new_v1(model).unwrap(),
        policy,
        state_sha256: String::new(),
        games,
    };
    let (status, extracted) = extract_all_v1(&inputs);
    assert_eq!(status.len(), 11);
    assert!(status[0].kept && status[0].rows == rows.len());
    assert!(status[1..]
        .iter()
        .all(|s| !s.kept && s.rows == 0 && s.reason.is_some()));
    assert_eq!(extracted.len(), rows.len());
    assert!(extracted.iter().zip(&rows).all(|((a, _), (b, _))| a == b));
    let census = bank_census_v1(
        &status,
        &extracted.into_iter().map(|(r, _)| r).collect::<Vec<_>>(),
    );
    assert_eq!(census["games_kept"], 1);
    assert_eq!(census["games_dropped"], 10);
    assert_eq!(census["rows"], rows.len());
}

#[test]
fn probe_readings_follow_the_deployed_sampler_and_the_line_groups() {
    let logits = [0.5_f32, 2.0, 2.0, -40.0];
    let deployed = deployed_v1(&logits).unwrap();
    assert!((deployed.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!(deployed[3] < deployed[0] && deployed[1] == deployed[2]);
    assert_eq!(top_v1(&logits), 1, "the first maximal logit");
    assert_eq!(raw_gap_v1(&logits, 0), 1.5);
    assert_eq!(raw_gap_v1(&logits, 1), 0.0);
    assert_eq!(clamped_top_two_gap_v1(&[30.0, -1.0]), GAP_CLAMP);
    assert_eq!(clamped_top_two_gap_v1(&[1.0, 3.0, 2.5]), 0.5);
    assert_eq!(median(vec![3.0, 1.0, 2.0, 4.0]), Some(2.5));
    assert_eq!(median(Vec::new()), None);
    let reading = |deployed: f64, own: Option<f64>, giant: Option<f64>| ReadingV1 {
        deployed,
        raw: deployed,
        gap: 0.0,
        top: deployed > 0.5,
        self_deployed: own,
        giant_deployed: giant,
    };
    let chosen = vec![
        reading(0.75, Some(0.75), None),
        reading(0.25, Some(0.25), None),
    ];
    let declined = vec![reading(0.5, Some(0.125), None)];
    assert_eq!(
        discrimination_v1(Some(&chosen), Some(&declined), |r| r.self_deployed),
        json!(0.375)
    );
    assert_eq!(
        discrimination_v1(Some(&chosen), None, |r| r.self_deployed),
        Value::Null
    );
    assert_eq!(
        discrimination_v1(Some(&chosen), Some(&declined), |r| r.giant_deployed),
        Value::Null
    );
    let offered: Vec<ReadingV1> = chosen.iter().chain(&declined).copied().collect();
    assert_eq!(
        chosen_share_v1(Some(&chosen), Some(&offered)),
        json!(2.0 / 3.0)
    );
    assert_eq!(chosen_share_v1(None, Some(&declined)), json!(0.0));
    assert_eq!(chosen_share_v1(None, None), Value::Null);
    let summary = absorption_v1(&offered);
    assert_eq!(summary["rows"], 3);
    assert_eq!(summary["mean_deployed_probability"], json!(0.5));
    assert_eq!(summary["top_agreement"], json!(1.0 / 3.0));
    assert_eq!(summary["mean_deployed_self_probability"], json!(0.375));
    assert_eq!(summary["mean_deployed_giant_probability"], Value::Null);
    assert!(is_spy_label("Spy") && is_spy_label("Spy/0123456789ab"));
    assert!(!is_spy_label("SpyV2/0123456789ab") && !is_spy_label("Elves"));
}

/// A real parent (one ordinary GAE update from a synthetic fresh V3 source),
/// two faithful archive games and one tampered one, the parent's own anchor
/// play (two training episodes, one held out) and one Spy offer episode.
struct WorldV1 {
    root: PathBuf,
    parent: ExpandedModelSourceV1,
    games: PinnedFileV1,
    sources: Vec<PinnedFileV1>,
    anchor: ArchiveDistillationAnchorV1,
    offers: Vec<PinnedFileV1>,
}

fn collect_pins(
    source: &ExpandedModelSourceV1,
    episodes: Vec<ExpandedEpisodeV1>,
    out: PathBuf,
) -> Vec<PinnedFileV1> {
    let result = execute_v1(ExpandedTrainingCommandV1::Collect {
        source: source.clone(),
        episodes,
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        output_directory: out,
    })
    .unwrap();
    serde_json::from_value(result["trajectories"].clone()).unwrap()
}

fn world_v1(label: &str) -> WorldV1 {
    let root = temp_root(label);
    let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V3;
    let fresh = write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        feature_identity,
        |_parameters| {},
    );
    let mut parent = ExpandedModelSourceV1 {
        play_import: write_pinned(&root.join("descriptor.json"), &fresh),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
    };
    let trajectories = collect_pins(
        &parent,
        vec![real_episode("parent-play", 2_026_101_310, 0)],
        root.join("parent-collect"),
    );
    let update = execute_v1(ExpandedTrainingCommandV1::Update {
        source: parent.clone(),
        trajectories,
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 1.0,
            lambda: 0.9,
            entropy_coefficient: 0.0,
        },
        line_b: None,
        output_directory: root.join("parent-update"),
    })
    .unwrap();
    parent.checkpoint = Some(serde_json::from_value(update["checkpoint"].clone()).unwrap());
    let mut games = String::new();
    for (index, seed) in [2_026_101_311_u64, 2_026_101_312].into_iter().enumerate() {
        let id = format!("archive-r1-k{index}-v6-s{index}");
        let game = recorded_game(&id, real_episode(&id, seed, index as u8));
        games.push_str(&serde_json::to_string(&game).unwrap());
        games.push('\n');
        if index == 1 {
            let mut changed = game;
            changed.id = format!("{id}-tampered");
            changed.starts[1].menu_hash = "0".repeat(64);
            games.push_str(&serde_json::to_string(&changed).unwrap());
            games.push('\n');
        }
    }
    let games_path = root.join("games.jsonl");
    fs::write(&games_path, &games).unwrap();
    let games = PinnedFileV1 {
        path: games_path,
        sha256: sha(games.as_bytes()),
    };
    let sources = vec![write_pinned(
        &root.join("ARCHIVE.jsonl"),
        &json!({"frozen":true}),
    )];
    let mut anchor_pins = collect_pins(
        &parent,
        vec![
            real_episode("anchor-held", 2_026_101_322, 0),
            real_episode("anchor-0", 2_026_101_320, 0),
            real_episode("anchor-1", 2_026_101_321, 1),
        ],
        root.join("anchor-collect"),
    );
    // Two games hold fewer than one anchor batch of learner rows, so the
    // training split repeats them under fresh episode ids (rows repeat; the
    // plumbing is what this exercises).
    let mut rows = 0;
    let mut copy = 0;
    while rows < ANCHOR_BATCH {
        let mut trajectory: ExpandedTrajectoryV1 = read_pinned(&anchor_pins[1 + copy % 2]).unwrap();
        rows += anchor_rows_v1(&trajectory).len();
        if copy >= 2 {
            trajectory.episode.id = format!("{}-copy-{copy}", trajectory.episode.id);
            anchor_pins.push(write_pinned(
                &root.join(format!("anchor-copy-{copy}.json")),
                &trajectory,
            ));
        }
        copy += 1;
    }
    let offers = collect_pins(
        &parent,
        vec![real_episode("offer-0", 2_026_101_330, 1)],
        root.join("offer-collect"),
    );
    WorldV1 {
        root,
        parent,
        games,
        sources,
        anchor: ArchiveDistillationAnchorV1 {
            training: anchor_pins[1..].to_vec(),
            held_out: anchor_pins[..1].to_vec(),
        },
        offers,
    }
}

fn candidate(id: &str, source: &ExpandedModelSourceV1) -> ArchiveDistillationCandidateV1 {
    ArchiveDistillationCandidateV1 {
        id: id.into(),
        source: source.clone(),
    }
}

/// The three commands through `execute_v1` on real files: the bank keeps the
/// faithful games and lists the tampered one, the dry run checks every input
/// and the anchor pre-flight on the real anchor rows, a written phase-1
/// checkpoint reloads through the ordinary loader, and the probe reads the
/// parent as exactly itself. Real games make it slow in debug builds (about
/// four minutes), so it runs explicitly:
/// `cargo test --release -p mtg-kernel --lib commands_bank -- --ignored`.
#[test]
#[ignore = "end to end on real games; run explicitly"]
fn commands_bank_dry_run_checkpoint_and_probe_on_a_real_parent() {
    let world = world_v1("end-to-end");
    let bank = execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationBank {
        parent: world.parent.clone(),
        lineage: "r1".into(),
        games: world.games.clone(),
        sources: world.sources.clone(),
        output_directory: world.root.join("bank"),
    })
    .unwrap();
    let bank_pin: PinnedFileV1 = serde_json::from_value(bank["bank"].clone()).unwrap();
    let record: ArchiveBankV1 = read_pinned(&bank_pin).unwrap();
    assert_eq!(
        record
            .game_status
            .iter()
            .map(|g| g.kept)
            .collect::<Vec<_>>(),
        vec![true, true, false]
    );
    assert!(record.game_status[2]
        .reason
        .as_deref()
        .unwrap()
        .contains("recorded observation, menu or state"));
    assert_eq!(record.sources, world.sources);
    assert!(!record.rows.is_empty());
    // The bank is a pure function of its inputs: a second extraction agrees.
    let again = execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationBank {
        parent: world.parent.clone(),
        lineage: "r1".into(),
        games: world.games.clone(),
        sources: world.sources.clone(),
        output_directory: world.root.join("bank-again"),
    })
    .unwrap();
    assert_eq!(again["rows_sha256"], bank["rows_sha256"]);
    // Another lineage's games refuse.
    assert!(
        execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationBank {
            parent: world.parent.clone(),
            lineage: "r2".into(),
            games: world.games.clone(),
            sources: world.sources.clone(),
            output_directory: world.root.join("bank-r2"),
        })
        .is_err()
    );
    let dry = execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationPhase1 {
        parent: world.parent.clone(),
        lineage: "r1".into(),
        arm: ArchiveDistillationArmV1::Uniform,
        games: world.games.clone(),
        bank: bank_pin.clone(),
        anchor: world.anchor.clone(),
        stop_after_steps: None,
        dry_run: true,
        output_directory: world.root.join("phase1-dry"),
    })
    .unwrap();
    let anchor_rows = dry["census"]["anchor_training_rows"].as_u64().unwrap();
    assert!(anchor_rows >= ANCHOR_BATCH as u64);
    assert_eq!(dry["anchor_preflight_rows"], json!(anchor_rows));
    assert_eq!(
        dry["census"]["verified_parent_outputs"],
        json!(record.rows.len() as u64 + anchor_rows)
    );
    assert_eq!(dry["census"]["arm"]["rows"], json!(record.rows.len()));
    // A checkpoint written the way phase 1 writes it reloads exactly.
    let parent_pin = world.parent.checkpoint.clone().unwrap();
    let (policy, state) = initialize(&world.parent).unwrap();
    let parent_checkpoint = read_ordinary_checkpoint_v1(&parent_pin, policy.identity_v1()).unwrap();
    let mut parameters = state.model_v1().parameter_snapshot_v1();
    parameters
        .iter_mut()
        .find(|p| p.name == "scorer.2.weight")
        .unwrap()
        .values[0] += 0.25;
    let mut record_provenance = provenance(&parent_checkpoint, &parent_pin);
    record_provenance.steps = 7;
    let output = world.root.join("written");
    fs::create_dir_all(&output).unwrap();
    let (checkpoint_pin, source_pin, state_sha256) = write_checkpoint_v1(
        &output,
        &world.parent,
        &parent_checkpoint,
        &parameters,
        record_provenance,
    )
    .unwrap();
    let distilled: ExpandedModelSourceV1 = read_pinned(&source_pin).unwrap();
    assert_eq!(distilled.checkpoint, Some(checkpoint_pin));
    let (_, loaded) = initialize(&distilled).unwrap();
    assert_eq!(hex(&loaded.state_sha256_v1().unwrap()), state_sha256);
    assert_eq!(loaded.adam_step_v1(), parent_checkpoint.adam_step);
    let probe = execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationProbe {
        parent: world.parent.clone(),
        lineage: "r1".into(),
        candidates: vec![
            candidate("parent", &world.parent),
            candidate("distilled", &distilled),
        ],
        banks: vec![ArchiveDistillationProbeBankV1 {
            role: "training".into(),
            games: world.games.clone(),
            bank: bank_pin,
        }],
        anchor: world.anchor.clone(),
        offers: world.offers.clone(),
        output_directory: world.root.join("probe"),
    })
    .unwrap();
    let parent_reading = &probe["candidates"][0];
    let held_out = &parent_reading["anchor_held_out"];
    assert!(held_out["rows"].as_u64().unwrap() > 0);
    for kl in [
        "mean_kl_from_parent_all",
        "mean_kl_from_parent_spy",
        "mean_kl_from_parent_non_spy",
    ] {
        assert!(matches!(held_out[kl].as_f64(), None | Some(0.0)), "{kl}");
    }
    assert_eq!(held_out["mean_kl_from_parent_all"], json!(0.0));
    assert_eq!(held_out["mean_absolute_value_change"], json!(0.0));
    let all = &parent_reading["archive"]["training"]["groups"]["all"];
    assert_eq!(all["rows"], json!(record.rows.len()));
    let moved = &probe["candidates"][1]["anchor_held_out"];
    assert!(moved["mean_kl_from_parent_all"].as_f64().unwrap() > 0.0);
    assert_eq!(
        probe["candidates"][1]["adam_step"],
        json!(parent_checkpoint.adam_step)
    );
    // An offer episode is never an anchor episode.
    let error = execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationProbe {
        parent: world.parent.clone(),
        lineage: "r1".into(),
        candidates: vec![candidate("parent", &world.parent)],
        banks: vec![ArchiveDistillationProbeBankV1 {
            role: "training".into(),
            games: world.games.clone(),
            bank: serde_json::from_value(bank["bank"].clone()).unwrap(),
        }],
        anchor: world.anchor.clone(),
        offers: world.anchor.held_out.clone(),
        output_directory: world.root.join("probe-reused-offers"),
    })
    .unwrap_err();
    assert!(error.contains("appears twice"), "{error}");
    fs::remove_dir_all(world.root).unwrap();
}

/// A full phase-1 run of each arm through `execute_v1`: every scheduled
/// checkpoint reloads through the ordinary loader with the parent's Adam
/// state, a 32-step rerun reproduces the run's step-32 state, and the probe
/// reads every checkpoint. Run explicitly, in release:
/// `cargo test --release -p mtg-kernel --lib phase1_runs -- --ignored --nocapture`.
#[test]
#[ignore = "end to end on real games; run explicitly"]
fn phase1_runs_write_reloadable_checkpoints_and_reproduce_their_first_steps() {
    let world = world_v1("phase1-run");
    let bank = execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationBank {
        parent: world.parent.clone(),
        lineage: "r1".into(),
        games: world.games.clone(),
        sources: world.sources.clone(),
        output_directory: world.root.join("bank"),
    })
    .unwrap();
    let bank_pin: PinnedFileV1 = serde_json::from_value(bank["bank"].clone()).unwrap();
    let run = |arm, stop_after_steps, name: &str| {
        execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationPhase1 {
            parent: world.parent.clone(),
            lineage: "r1".into(),
            arm,
            games: world.games.clone(),
            bank: bank_pin.clone(),
            anchor: world.anchor.clone(),
            stop_after_steps,
            dry_run: false,
            output_directory: world.root.join(name),
        })
    };
    let started = std::time::Instant::now();
    let full = run(ArchiveDistillationArmV1::Uniform, None, "phase1-u").unwrap();
    let steps = full["steps"].as_u64().unwrap();
    eprintln!(
        "timing note: uniform phase 1, {steps} steps of {} archive and {ANCHOR_BATCH} anchor rows, {:.1} s",
        ARCHIVE_BATCH,
        started.elapsed().as_secs_f64()
    );
    let checkpoints = full["checkpoints"].as_array().unwrap();
    assert_eq!(checkpoints.len(), UNIFORM_EPOCHS.len());
    let parent_checkpoint: ExpandedCheckpointV1 =
        read_pinned(world.parent.checkpoint.as_ref().unwrap()).unwrap();
    let mut candidates = vec![candidate("parent", &world.parent)];
    for (index, entry) in checkpoints.iter().enumerate() {
        let source: ExpandedModelSourceV1 =
            read_pinned(&serde_json::from_value(entry["source"].clone()).unwrap()).unwrap();
        let (_, loaded) = initialize(&source).unwrap();
        assert_eq!(loaded.adam_step_v1(), parent_checkpoint.adam_step);
        assert_eq!(
            json!(hex(&loaded.state_sha256_v1().unwrap())),
            entry["state_sha256"]
        );
        let saved: ExpandedCheckpointV1 = read_pinned(source.checkpoint.as_ref().unwrap()).unwrap();
        let provenance = saved.archive_distillation.as_ref().unwrap();
        assert_eq!(json!(provenance.steps), entry["step"]);
        assert_eq!(
            json!(provenance.phase1_state_sha256),
            entry["phase1_state_sha256"]
        );
        assert_eq!(provenance.archive_sources, world.sources);
        candidates.push(candidate(&format!("u-{index}"), &source));
    }
    let rerun = run(
        ArchiveDistillationArmV1::Uniform,
        Some(REPRODUCTION_STEP),
        "phase1-u-rerun",
    )
    .unwrap();
    assert!(rerun["checkpoints"].as_array().unwrap().is_empty());
    let at = |result: &Value| {
        result["state_hashes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["step"] == json!(REPRODUCTION_STEP))
            .unwrap()["phase1_state_sha256"]
            .clone()
    };
    assert_eq!(at(&rerun), at(&full));
    let probe = execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationProbe {
        parent: world.parent.clone(),
        lineage: "r1".into(),
        candidates,
        banks: vec![ArchiveDistillationProbeBankV1 {
            role: "training".into(),
            games: world.games.clone(),
            bank: bank_pin.clone(),
        }],
        anchor: world.anchor.clone(),
        offers: world.offers.clone(),
        output_directory: world.root.join("probe"),
    })
    .unwrap();
    let fit: Vec<f64> = probe["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c["archive"]["training"]["groups"]["all"]["mean_deployed_probability"]
                .as_f64()
                .unwrap()
        })
        .collect();
    eprintln!("timing note: in-sample target probability by checkpoint {fit:?}");
    assert!(
        fit[UNIFORM_EPOCHS.len()] > fit[0],
        "distillation fits its rows"
    );
    // The line arm refuses a bank without line rows, or runs to 1,024 steps.
    match run(ArchiveDistillationArmV1::LineDecisions, None, "phase1-l") {
        Ok(line) => assert_eq!(line["steps"], json!(1024)),
        Err(error) => assert!(error.contains("no archive rows"), "{error}"),
    }
    fs::remove_dir_all(world.root).unwrap();
}
