use super::*;
use crate::expanded_deck_training_v1::tests::checkpoint_fixture_v1;

const LR: f32 = LEARNING_RATE;

/// The screen's frozen recipe (no cell options).
fn frozen_recipe() -> RecipeV1 {
    recipe_v1(ArchiveDistillationArmV1::Uniform, None).unwrap()
}

fn cell(label: &str) -> ArchiveDistillationOptionsV1 {
    ArchiveDistillationOptionsV1 {
        cell: label.into(),
        learning_rate: None,
        anchor_coefficient: None,
        min_edge_backups: None,
        count_control: None,
        checkpoint_steps: None,
        keep_sets: None,
    }
}

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
        cell: None,
        update_scope: ArchiveDistillationUpdateScopeV1::FullNetwork,
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
fn scorer_only_checkpoint_records_scope_and_refuses_frozen_parameter_changes() {
    let root = temp_root("scorer-scope");
    let (_, model, parent, file) = parent_fixture(&root);
    let mut parameters =
        restore_parameters(&parent.parameters, &model.parameter_snapshot_v1()).unwrap();
    parameters
        .iter_mut()
        .find(|p| p.name == "scorer.0.weight")
        .unwrap()
        .values[0] += 0.125;
    let mut record = provenance(&parent, &file);
    assert!(serde_json::to_value(&record)
        .unwrap()
        .get("update_scope")
        .is_none());
    record.update_scope = ArchiveDistillationUpdateScopeV1::ScorerOnly;
    let saved = checkpoint_v1(&parent, &parameters, record.clone()).unwrap();
    validate_phase1_checkpoint_v1(&saved).unwrap();
    assert_eq!(
        serde_json::to_value(&saved).unwrap()["archive_distillation"]["update_scope"],
        json!("scorer_only")
    );
    for name in [
        "card_embedding.weight",
        "state_encoder.0.weight",
        "value_head.0.weight",
    ] {
        let mut altered = parameters.clone();
        *altered
            .iter_mut()
            .find(|p| p.name == name)
            .unwrap()
            .values
            .last_mut()
            .unwrap() += 0.125;
        let altered = checkpoint_v1(&parent, &altered, record.clone()).unwrap();
        let result = validate_phase1_checkpoint_v1(&altered);
        assert!(
            result.unwrap_err().contains("changed a frozen parameter"),
            "{name}"
        );
    }
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
    let mut scoped = phase1.clone();
    scoped["update_scope"] = json!("scorer_only");
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(scoped.clone()).unwrap();
    assert_eq!(serde_json::to_value(&command).unwrap(), scoped);
    scoped["update_scope"] = json!("heads");
    assert!(serde_json::from_value::<ExpandedTrainingCommandV1>(scoped).is_err());
    let mut explicit_full = phase1.clone();
    explicit_full["update_scope"] = json!("full_network");
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(explicit_full).unwrap();
    assert_eq!(serde_json::to_value(&command).unwrap(), phase1);
    let mut rerun = phase1.clone();
    rerun["stop_after_steps"] = json!(32);
    rerun["dry_run"] = json!(true);
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(rerun.clone()).unwrap();
    assert_eq!(serde_json::to_value(&command).unwrap(), rerun);
    let mut celled = phase1.clone();
    celled["arm"] = json!("uniform");
    celled["options"] = json!({"cell":"U-s32","min_edge_backups":32,
        "checkpoint_steps":[8, 16, 32]});
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(celled.clone()).unwrap();
    assert_eq!(serde_json::to_value(&command).unwrap(), celled);
    let mut kept = celled.clone();
    kept["options"] = json!({"cell":"BT1-T","keep_sets":file,"checkpoint_steps":[8, 16, 32]});
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(kept.clone()).unwrap();
    assert_eq!(serde_json::to_value(&command).unwrap(), kept);
    let mut wrong = celled;
    wrong["options"]["anchor"] = json!(4);
    assert!(serde_json::from_value::<ExpandedTrainingCommandV1>(wrong).is_err());
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
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(probe.clone()).unwrap();
    assert!(matches!(
        command,
        ExpandedTrainingCommandV1::ArchiveDistillationProbe { evidence: None, .. }
    ));
    // Without evidence files the field stays off the wire.
    assert_eq!(serde_json::to_value(&command).unwrap(), probe);
    let mut evidenced = probe.clone();
    evidenced["evidence"] = json!({"training":file,"held_out":file});
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(evidenced.clone()).unwrap();
    assert_eq!(serde_json::to_value(&command).unwrap(), evidenced);
    // Evidence logits are opt-in and round-trip; off stays off the wire.
    let mut logged = evidenced.clone();
    logged["evidence_logits"] = json!(true);
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(logged.clone()).unwrap();
    assert!(matches!(
        command,
        ExpandedTrainingCommandV1::ArchiveDistillationProbe { evidence_logits: true, .. }
    ));
    assert_eq!(serde_json::to_value(&command).unwrap(), logged);
    let mut partial = probe.clone();
    partial["evidence"] = json!({"training":file});
    assert!(serde_json::from_value::<ExpandedTrainingCommandV1>(partial).is_err());
    let trace = json!({"mode":"line_retention_trace","run":file,"completion":file,
        "output_directory":"out"});
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(trace.clone()).unwrap();
    assert!(matches!(
        command,
        ExpandedTrainingCommandV1::LineRetentionTrace { .. }
    ));
    assert_eq!(serde_json::to_value(&command).unwrap(), trace);
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
        None,
        0,
    )
    .unwrap();
    assert_eq!(plan.epoch_steps, 3);
    assert_eq!(plan.checkpoints, vec![3, 6, 12, 24, 48, 96]);
    assert_eq!(plan.steps.len(), 96);
    let mut orders = BTreeSet::new();
    for epoch in plan.steps.chunks(3) {
        // 70 rows in three batches as equal as possible, each step's archive
        // term the mean over its own batch.
        assert_eq!(
            epoch.iter().map(|s| s.archive.len()).collect::<Vec<_>>(),
            vec![23, 23, 24]
        );
        assert!(epoch.iter().all(|s| s.denominator == s.archive.len()));
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
        None,
        0,
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
        None,
        0,
    )
    .unwrap();
    assert_ne!(plan.steps[0].archive, other.steps[0].archive);
    // Exposure at each checkpoint: every row drawn once per epoch, and the
    // line rows' summed archive-term weight, one draw per epoch at the
    // weight of the batch it fell in.
    let exposure = exposure_v1(&plan, &rows, &selected, anchor.len());
    let exposure = exposure.as_array().unwrap();
    assert_eq!(exposure.len(), 6);
    let weight = |members: &[usize], steps: usize| -> f64 {
        plan.steps[..steps]
            .iter()
            .map(|s| {
                s.archive.iter().filter(|i| members.contains(i)).count() as f64
                    / s.denominator as f64
            })
            .sum()
    };
    for (entry, epochs) in exposure.iter().zip([1u64, 2, 4, 8, 16, 32]) {
        assert_eq!(entry["draws_per_row_min"], json!(epochs));
        assert_eq!(entry["draws_per_row_max"], json!(epochs));
        let chosen = entry["line_chosen_weight"].as_f64().unwrap();
        let offered = entry["line_offered_weight"].as_f64().unwrap();
        let steps = 3 * epochs as usize;
        assert!((chosen - weight(&[5, 40], steps)).abs() < 1e-12);
        assert!((offered - weight(&[5, 40, 41], steps)).abs() < 1e-12);
        for (value, rows) in [(chosen, 2.0), (offered, 3.0)] {
            let per_epoch = value / epochs as f64;
            assert!(rows / 24.0 - 1e-12 <= per_epoch && per_epoch <= rows / 23.0 + 1e-12);
        }
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
        None,
        0,
    )
    .unwrap();
    assert_eq!(plan.checkpoints, LINE_STEPS.to_vec());
    assert_eq!(plan.steps.len(), 1024);
    assert!(plan
        .steps
        .iter()
        .all(|s| s.archive == selected && s.denominator == 3));
    // More line rows than one batch: U's epochs, still 1,024 steps. One row
    // past a batch splits evenly instead of leaving a one-row step, so the
    // archive term keeps coefficient 1 at every step.
    for (count, sizes) in [(40, vec![20, 20]), (33, vec![16, 17]), (64, vec![32, 32])] {
        let many: Vec<usize> = (0..count).collect();
        let plan = plan_v1(
            &rows,
            &many,
            &anchor,
            "r1",
            ArchiveDistillationArmV1::LineDecisions,
            None,
            0,
        )
        .unwrap();
        assert_eq!(plan.epoch_steps, 2);
        assert_eq!(plan.steps.len(), 1024);
        assert_eq!(
            plan.steps[..2]
                .iter()
                .map(|s| s.archive.len())
                .collect::<Vec<_>>(),
            sizes
        );
        assert!(plan.steps.iter().all(|s| s.denominator == s.archive.len()));
    }
    // No rows, or an anchor smaller than one batch, refuse.
    assert!(plan_v1(
        &rows,
        &[],
        &anchor,
        "r1",
        ArchiveDistillationArmV1::Uniform,
        None,
        0
    )
    .is_err());
    assert!(plan_v1(
        &rows,
        &selected,
        &anchor_rows(255),
        "r1",
        ArchiveDistillationArmV1::Uniform,
        None,
        0
    )
    .is_err());
}

#[test]
fn cell_options_resolve_to_one_recorded_recipe_and_refuse_anything_else() {
    let frozen = frozen_recipe();
    assert_eq!(frozen.learning_rate.to_bits(), LEARNING_RATE.to_bits());
    assert_eq!(
        frozen.anchor_coefficient.to_bits(),
        ANCHOR_COEFFICIENT.to_bits()
    );
    assert!(frozen.checkpoint_steps.is_none() && frozen.cell.is_none());
    let mut options = cell("U-a4");
    options.learning_rate = Some(2.5e-5);
    options.anchor_coefficient = Some(4.0);
    options.checkpoint_steps = Some(vec![8, 16, 32, 189]);
    let recipe = recipe_v1(ArchiveDistillationArmV1::Uniform, Some(&options)).unwrap();
    assert_eq!(recipe.learning_rate.to_bits(), 2.5e-5_f32.to_bits());
    assert_eq!(recipe.anchor_coefficient, 4.0);
    assert_eq!(recipe.checkpoint_steps, Some(vec![8, 16, 32, 189]));
    assert_eq!(
        recipe.cell,
        Some(ArchiveDistillationCellV1 {
            cell: "U-a4".into(),
            anchor_coefficient_bits: 4.0_f64.to_bits(),
            min_edge_backups: None,
            count_control: None,
            checkpoint_steps: vec![8, 16, 32, 189],
            keep_sets: None,
        })
    );
    // A label alone keeps every frozen value and the epoch schedule.
    let plain = recipe_v1(ArchiveDistillationArmV1::Uniform, Some(&cell("U0"))).unwrap();
    assert_eq!(plain.learning_rate.to_bits(), LEARNING_RATE.to_bits());
    assert_eq!(plain.anchor_coefficient, ANCHOR_COEFFICIENT);
    assert!(plain.checkpoint_steps.is_none());
    assert_eq!(plain.cell.unwrap().checkpoint_steps, Vec::<u64>::new());
    type Case = (
        &'static str,
        &'static str,
        fn(&mut ArchiveDistillationOptionsV1),
    );
    let cases: [Case; 13] = [
        ("empty label", "cell label", |o| o.cell = String::new()),
        ("label with a space", "cell label", |o| {
            o.cell = "U a4".into()
        }),
        ("long label", "cell label", |o| o.cell = "U".repeat(33)),
        ("zero step size", "step size", |o| {
            o.learning_rate = Some(0.0)
        }),
        ("NaN step size", "step size", |o| {
            o.learning_rate = Some(f32::NAN)
        }),
        ("negative anchor", "anchor coefficient", |o| {
            o.anchor_coefficient = Some(-1.0)
        }),
        ("infinite anchor", "anchor coefficient", |o| {
            o.anchor_coefficient = Some(f64::INFINITY)
        }),
        ("two row rules", "one rule", |o| {
            o.min_edge_backups = Some(32);
            o.count_control = Some(10)
        }),
        ("keep sets and a count control", "one rule", |o| {
            o.keep_sets = Some(pin("keep.json", 'k'));
            o.count_control = Some(10)
        }),
        ("keep sets and a support floor", "one rule", |o| {
            o.keep_sets = Some(pin("keep.json", 'k'));
            o.min_edge_backups = Some(32)
        }),
        ("empty count control", "at least one row", |o| {
            o.count_control = Some(0)
        }),
        ("unordered checkpoints", "strictly increasing", |o| {
            o.checkpoint_steps = Some(vec![8, 64, 32])
        }),
        ("short schedule", "reproduction step", |o| {
            o.checkpoint_steps = Some(vec![8, 16])
        }),
    ];
    for (name, expected, mutate) in cases {
        let mut options = cell("U-x");
        mutate(&mut options);
        let error = recipe_v1(ArchiveDistillationArmV1::Uniform, Some(&options))
            .err()
            .unwrap_or_else(|| panic!("{name} accepted"));
        assert!(error.contains(expected), "{name}: {error}");
    }
    let error = recipe_v1(ArchiveDistillationArmV1::LineDecisions, Some(&cell("L0")))
        .err()
        .unwrap();
    assert!(error.contains("arm U only"), "{error}");
}

#[test]
fn cell_rows_keep_a_support_floor_or_a_label_free_count() {
    let mut rows = bank_rows(200);
    for (index, (row, _)) in rows.iter_mut().enumerate() {
        row.edge_backups = Some(8 + (index as u64 * 37) % 400);
    }
    let all: Vec<usize> = (0..200).collect();
    let none = select_rows_v1(&rows, ArchiveDistillationArmV1::Uniform, "r1", None, None).unwrap();
    assert_eq!(none, all);
    assert_eq!(
        select_rows_v1(
            &rows,
            ArchiveDistillationArmV1::LineDecisions,
            "r1",
            None,
            None
        )
        .unwrap(),
        vec![5, 40, 41]
    );
    // A support floor keeps exactly the rows at or above it, in bank order.
    let mut floor = cell("U-s32");
    floor.min_edge_backups = Some(32);
    let kept = select_rows_v1(
        &rows,
        ArchiveDistillationArmV1::Uniform,
        "r1",
        Some(&floor),
        None,
    )
    .unwrap();
    let expected: Vec<usize> = all
        .iter()
        .copied()
        .filter(|&i| rows[i].0.edge_backups.unwrap() >= 32)
        .collect();
    assert_eq!(kept, expected);
    assert!(kept.len() < 200 && !kept.is_empty());
    // A count control keeps n rows in bank order, the same n every time,
    // whatever the rows' labels and counts.
    let mut count = cell("U-c32");
    count.count_control = Some(kept.len());
    let control = select_rows_v1(
        &rows,
        ArchiveDistillationArmV1::Uniform,
        "r1",
        Some(&count),
        None,
    )
    .unwrap();
    assert_eq!(control.len(), kept.len());
    assert!(control.windows(2).all(|w| w[0] < w[1]));
    assert_ne!(control, kept);
    let mut relabeled = rows.clone();
    for (row, _) in relabeled.iter_mut() {
        row.edge_backups = Some(9);
        row.line = line(true, true, false, false);
        row.target = 0;
    }
    assert_eq!(
        select_rows_v1(
            &relabeled,
            ArchiveDistillationArmV1::Uniform,
            "r1",
            Some(&count),
            None
        )
        .unwrap(),
        control
    );
    assert_ne!(
        select_rows_v1(
            &rows,
            ArchiveDistillationArmV1::Uniform,
            "r2",
            Some(&count),
            None
        )
        .unwrap(),
        control
    );
    count.count_control = Some(201);
    assert!(select_rows_v1(
        &rows,
        ArchiveDistillationArmV1::Uniform,
        "r1",
        Some(&count),
        None
    )
    .unwrap_err()
    .contains("more rows than the arm has"));
}

fn logits_bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

fn keep_file(rows: &[(&str, u64, Vec<u32>)]) -> ArchiveDistillationKeepSetsV1 {
    ArchiveDistillationKeepSetsV1 {
        schema: KEEP_SETS_SCHEMA_V1.into(),
        lineage: "r1".into(),
        bank_rows_sha256: "b".repeat(64),
        rows: rows
            .iter()
            .map(|(game, step, keep)| ArchiveDistillationKeepRowV1 {
                game: (*game).into(),
                step: *step,
                keep: keep.clone(),
            })
            .collect(),
    }
}

/// A keep-set cell trains exactly its listed rows, each toward the parent's
/// own binary64 softmax over the kept actions: one kept action is the
/// one-hot target, every action the parent itself. The file binds to the
/// lineage and the bank's rows, and every row and keep set is checked.
#[test]
fn keep_set_cells_train_listed_rows_toward_the_parents_kept_softmax() {
    let root = temp_root("keep-sets");
    let mut rows = bank_rows(60);
    rows[5].0.parent_logits = logits_bits(&[1.0, -0.5, 2.0]);
    rows[5].0.self_actions = vec![2];
    rows[40].0.parent_logits = logits_bits(&[0.25, 3.0, -1.0]);
    rows[40].0.giant_actions = vec![0, 1];
    rows[41].0.parent_logits = logits_bits(&[-2.0, 0.5, 0.75]);
    rows[41].0.self_actions = vec![0];
    let listed = keep_file(&[
        ("game-2", 1, vec![0, 1, 2]),
        ("game-0", 5, vec![0, 2]),
        ("game-2", 0, vec![1]),
    ]);
    let file = write_pinned(&root.join("keep.json"), &listed);
    let bank_sha = "b".repeat(64);
    let keep = keep_sets_v1(&file, "r1", &bank_sha, &rows).unwrap();
    assert_eq!(keep.keys().copied().collect::<Vec<_>>(), vec![5, 40, 41]);
    let mut options = cell("BT1-T");
    options.keep_sets = Some(file.clone());
    let uniform = ArchiveDistillationArmV1::Uniform;
    assert_eq!(
        select_rows_v1(&rows, uniform, "r1", Some(&options), Some(&keep)).unwrap(),
        vec![5, 40, 41]
    );
    for (options, keep) in [(Some(&options), None), (None, Some(&keep))] {
        assert!(select_rows_v1(&rows, uniform, "r1", options, keep)
            .unwrap_err()
            .contains("loaded keep sets"));
    }
    // Targets: one kept action is the one-hot target bit for bit; every
    // action is the parent's softmax; two of three keep the parent's ratio.
    let parent = |i: usize| {
        line_b_softmax_v1(&f64_logits(&recorded_logits(&rows[i].0.parent_logits))).unwrap()
    };
    assert_eq!(
        kept_target_v1(&rows[40].0, &[1]).unwrap(),
        one_hot_target_v1(3, 1).unwrap()
    );
    assert_eq!(kept_target_v1(&rows[41].0, &[0, 1, 2]).unwrap(), parent(41));
    let pair = kept_target_v1(&rows[5].0, &[0, 2]).unwrap();
    let full = parent(5);
    assert_eq!(pair.probabilities[1], 0.0);
    assert_eq!(pair.log_probabilities[1], f64::NEG_INFINITY);
    let renormalized = full.probabilities[0] / (full.probabilities[0] + full.probabilities[2]);
    assert!((pair.probabilities[0] - renormalized).abs() < 1e-15);
    assert!((pair.probabilities[0] + pair.probabilities[2] - 1.0).abs() < 1e-15);
    let sub = line_b_softmax_v1(&[1.0, 2.0]).unwrap();
    assert_eq!(
        (pair.probabilities[0], pair.probabilities[2]),
        (sub.probabilities[0], sub.probabilities[1])
    );
    // The census reads each listed row's kept mass, line mass and KL.
    let targets: Vec<LineBSoftmaxV1> = rows
        .iter()
        .enumerate()
        .map(|(i, (row, _))| match keep.get(&i) {
            Some(kept) => kept_target_v1(row, kept).unwrap(),
            None => one_hot_target_v1(3, 1).unwrap(),
        })
        .collect();
    let census = keep_census_v1(&rows, &keep, &targets).unwrap();
    let census = census.as_array().unwrap();
    assert_eq!(census.len(), 3);
    assert_eq!(
        (&census[0]["game"], &census[0]["step"]),
        (&json!("game-0"), &json!(5))
    );
    assert_eq!(census[0]["target_self_mass"], json!(pair.probabilities[2]));
    assert_eq!(census[0]["target_giant_mass"], Value::Null);
    let kept_mass = full.probabilities[0] + full.probabilities[2];
    assert_eq!(census[0]["kept_parent_probability"], json!(kept_mass));
    let kl = census[0]["kl_target_from_parent"].as_f64().unwrap();
    assert!((kl + kept_mass.ln()).abs() < 1e-12, "{kl}");
    assert_eq!(census[1]["target_giant_mass"], json!(1.0));
    let kl = census[1]["kl_target_from_parent"].as_f64().unwrap();
    assert!(
        (kl + parent(40).probabilities[1].ln()).abs() < 1e-12,
        "{kl}"
    );
    assert_eq!(census[2]["kl_target_from_parent"], json!(0.0));
    assert_eq!(
        census[2]["target_self_mass"],
        json!(parent(41).probabilities[0])
    );
    // Refusals.
    type Case = (
        &'static str,
        &'static str,
        fn(&mut ArchiveDistillationKeepSetsV1),
    );
    let cases: [Case; 9] = [
        ("schema", "identity differs", |f| f.schema = "other".into()),
        ("lineage", "identity differs", |f| f.lineage = "r2".into()),
        ("bank", "identity differs", |f| {
            f.bank_rows_sha256 = "c".repeat(64)
        }),
        ("no rows", "at least one row", |f| f.rows.clear()),
        ("unknown row", "not a bank row", |f| f.rows[0].step = 99),
        ("listed twice", "listed twice", |f| {
            let again = f.rows[1].clone();
            f.rows.push(again)
        }),
        ("empty keep", "inside its menu", |f| f.rows[0].keep.clear()),
        ("unordered keep", "inside its menu", |f| {
            f.rows[1].keep = vec![2, 0]
        }),
        ("outside the menu", "inside its menu", |f| {
            f.rows[2].keep = vec![3]
        }),
    ];
    for (name, expected, mutate) in cases {
        let mut changed = listed.clone();
        mutate(&mut changed);
        let pin = write_pinned(&root.join(format!("keep-{name}.json")), &changed);
        let error = keep_sets_v1(&pin, "r1", &bank_sha, &rows).unwrap_err();
        assert!(error.contains(expected), "{name}: {error}");
    }
    let mut tampered = file.clone();
    tampered.sha256 = "0".repeat(64);
    assert!(keep_sets_v1(&tampered, "r1", &bank_sha, &rows)
        .unwrap_err()
        .contains("SHA differs"));
    // A kept action with no parent probability (below the softmax floor).
    let mut far = rows.clone();
    far[40].0.parent_logits = logits_bits(&[0.0, -1.0e4, 0.0]);
    let error = keep_sets_v1(&file, "r1", &bank_sha, &far).unwrap_err();
    assert!(error.contains("no parent probability"), "{error}");
    // A row the arm does not admit refuses (arm L admits only line rows).
    let mut options_l = options.clone();
    options_l.cell = "L-keep".into();
    let line_keep: BTreeMap<usize, Vec<u32>> = [(5, vec![0]), (6, vec![1])].into();
    assert!(select_rows_v1(
        &rows,
        ArchiveDistillationArmV1::LineDecisions,
        "r1",
        Some(&options_l),
        Some(&line_keep)
    )
    .unwrap_err()
    .contains("outside the arm"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cell_plans_keep_the_screens_batches_and_order_at_their_own_checkpoints() {
    let rows = bank_rows(70);
    let all: Vec<usize> = (0..70).collect();
    let anchor = anchor_rows(300);
    let frozen = plan_v1(
        &rows,
        &all,
        &anchor,
        "r1",
        ArchiveDistillationArmV1::Uniform,
        None,
        0,
    )
    .unwrap();
    // Own checkpoint steps, past the frozen schedule's end: the same
    // archive and anchor batches at every shared step.
    let steps = [8, 16, 32, 50, 128];
    let cell_plan = plan_v1(
        &rows,
        &all,
        &anchor,
        "r1",
        ArchiveDistillationArmV1::Uniform,
        Some(&steps),
        0,
    )
    .unwrap();
    assert_eq!(cell_plan.checkpoints, steps.to_vec());
    assert_eq!(cell_plan.steps.len(), 128);
    assert_eq!(cell_plan.epoch_steps, frozen.epoch_steps);
    assert!(frozen
        .steps
        .iter()
        .zip(&cell_plan.steps)
        .all(|(a, b)| a.archive == b.archive
            && a.denominator == b.denominator
            && a.anchor == b.anchor));
    // A subset's epochs keep U's relative order and the shared anchor order.
    let subset: Vec<usize> = all.iter().copied().filter(|i| i % 3 != 0).collect();
    let sub = plan_v1(
        &rows,
        &subset,
        &anchor,
        "r1",
        ArchiveDistillationArmV1::Uniform,
        Some(&steps),
        0,
    )
    .unwrap();
    assert_eq!(sub.epoch_steps, (subset.len() as u64).div_ceil(32));
    for epoch in 0..3usize {
        let per = frozen.epoch_steps as usize;
        let full: Vec<usize> = frozen.steps[epoch * per..(epoch + 1) * per]
            .iter()
            .flat_map(|s| s.archive.clone())
            .filter(|i| subset.contains(i))
            .collect();
        let per = sub.epoch_steps as usize;
        let kept: Vec<usize> = sub.steps[epoch * per..(epoch + 1) * per]
            .iter()
            .flat_map(|s| s.archive.clone())
            .collect();
        assert_eq!(kept, full, "epoch {epoch}");
    }
    assert!(sub
        .steps
        .iter()
        .zip(&cell_plan.steps)
        .all(|(a, b)| a.anchor == b.anchor));
    // Arm L takes no cell schedule; unordered steps refuse.
    assert!(plan_v1(
        &rows,
        &[5, 40, 41],
        &anchor,
        "r1",
        ArchiveDistillationArmV1::LineDecisions,
        Some(&steps),
        0
    )
    .is_err());
    assert!(plan_v1(
        &rows,
        &all,
        &anchor,
        "r1",
        ArchiveDistillationArmV1::Uniform,
        Some(&[32, 16]),
        0
    )
    .is_err());
}

/// Every recorded cell label (and the frozen recipe, and `SD1-T-s0`) reads
/// order seed 0, so its archive-order and anchor-order hash inputs are the
/// unseeded ones byte for byte, and so is its plan.
#[test]
fn recorded_cell_labels_keep_byte_identical_order_hash_inputs() {
    let unseeded_archive =
        br#"["spy-archive-distillation-v1","archive-order","r1","uniform",3,"game-1",7]"#;
    let unseeded_anchor = br#"["spy-archive-distillation-v1","anchor-order","r1",2,"anchor-4",9]"#;
    let rows = bank_rows(70);
    let all: Vec<usize> = (0..70).collect();
    let anchor = anchor_rows(300);
    let unseeded = |arm| plan_v1(&rows, &all, &anchor, "r1", arm, None, 0).unwrap();
    let recorded = [
        "U0", "U-lr", "U-a4", "U-a16", "U-s32", "U-s256", "U-c32", "U-c256", "BT1-T", "BT1-P",
        "SD1-T-s0",
    ];
    let mut seeds = vec![("frozen", frozen_recipe().order_seed)];
    for label in recorded {
        assert_eq!(order_seed_v1(label), 0, "{label}");
        let recipe = recipe_v1(ArchiveDistillationArmV1::Uniform, Some(&cell(label))).unwrap();
        seeds.push((label, recipe.order_seed));
    }
    for (label, seed) in seeds {
        assert_eq!(seed, 0, "{label}");
        for arm in [
            ArchiveDistillationArmV1::Uniform,
            ArchiveDistillationArmV1::LineDecisions,
        ] {
            let key = serde_json::to_vec(&archive_order_key_v1("r1", arm, 3, "game-1", 7, seed));
            let literal = serde_json::to_vec(&json!([
                ARCHIVE_DISTILLATION_NAMESPACE_V1,
                "archive-order",
                "r1",
                arm.label(),
                3,
                "game-1",
                7
            ]));
            assert_eq!(key.unwrap(), literal.unwrap(), "{label}");
        }
        assert_eq!(
            serde_json::to_vec(&archive_order_key_v1(
                "r1",
                ArchiveDistillationArmV1::Uniform,
                3,
                "game-1",
                7,
                seed
            ))
            .unwrap(),
            unseeded_archive.to_vec(),
            "{label}"
        );
        assert_eq!(
            serde_json::to_vec(&anchor_order_key_v1("r1", 2, "anchor-4", 9, seed)).unwrap(),
            unseeded_anchor.to_vec(),
            "{label}"
        );
        let base = unseeded(ArchiveDistillationArmV1::Uniform);
        let plan = plan_v1(
            &rows,
            &all,
            &anchor,
            "r1",
            ArchiveDistillationArmV1::Uniform,
            None,
            seed,
        )
        .unwrap();
        assert!(plan
            .steps
            .iter()
            .zip(&base.steps)
            .all(|(a, b)| a.archive == b.archive && a.anchor == b.anchor));
    }
}

/// `SD1-T-s{k}` with k from 1 to 7 appends k to both hash inputs: each seed
/// has its own epoch and anchor orders over the same rows, batch sizes and
/// checkpoints. A label without the `SD1-T-s` prefix reads seed 0 (one with
/// it is refused unless exact: `sd1_seed_label_near_misses_are_refused`),
/// and the cell records no new field.
#[test]
fn sd1_seed_labels_reorder_both_orders_over_the_same_rows() {
    for (label, seed) in [
        ("SD1-T-s0", 0),
        ("SD1-T-s1", 1),
        ("SD1-T-s4", 4),
        ("SD1-T-s7", 7),
        ("SD1-T-S1", 0),
        ("sd1-T-s1", 0),
        ("SD1-P-s1", 0),
        ("xSD1-T-s1", 0),
    ] {
        assert_eq!(order_seed_v1(label), seed, "{label}");
        let recipe = recipe_v1(ArchiveDistillationArmV1::Uniform, Some(&cell(label))).unwrap();
        assert_eq!(recipe.order_seed, seed, "{label}");
        let fields: Vec<String> = serde_json::to_value(recipe.cell.as_ref().unwrap())
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            fields,
            ["anchor_coefficient_bits", "cell", "checkpoint_steps"],
            "{label}"
        );
    }
    assert_eq!(
        serde_json::to_vec(&archive_order_key_v1(
            "r1",
            ArchiveDistillationArmV1::Uniform,
            3,
            "game-1",
            7,
            5
        ))
        .unwrap(),
        br#"["spy-archive-distillation-v1","archive-order","r1","uniform",3,"game-1",7,5]"#
            .to_vec()
    );
    assert_eq!(
        serde_json::to_vec(&anchor_order_key_v1("r1", 2, "anchor-4", 9, 5)).unwrap(),
        br#"["spy-archive-distillation-v1","anchor-order","r1",2,"anchor-4",9,5]"#.to_vec()
    );
    let rows = bank_rows(70);
    let all: Vec<usize> = (0..70).collect();
    let anchor = anchor_rows(300);
    let steps = [8, 16, 32];
    let plans: Vec<PlanV1> = (0..8)
        .map(|seed| {
            plan_v1(
                &rows,
                &all,
                &anchor,
                "r1",
                ArchiveDistillationArmV1::Uniform,
                Some(&steps),
                seed,
            )
            .unwrap()
        })
        .collect();
    let mut first_epochs = BTreeSet::new();
    let mut first_anchors = BTreeSet::new();
    for plan in &plans {
        assert_eq!(plan.checkpoints, steps.to_vec());
        assert_eq!(plan.epoch_steps, plans[0].epoch_steps);
        assert_eq!(plan.steps.len(), plans[0].steps.len());
        let per = plan.epoch_steps as usize;
        for epoch in plan.steps.chunks(per).filter(|e| e.len() == per) {
            assert_eq!(
                epoch.iter().map(|s| s.archive.len()).collect::<Vec<_>>(),
                vec![23, 23, 24]
            );
            let order: Vec<usize> = epoch.iter().flat_map(|s| s.archive.clone()).collect();
            assert_eq!(sorted(order), all);
        }
        let draws: Vec<usize> = plan.steps.iter().flat_map(|s| s.anchor.clone()).collect();
        for pass in draws
            .chunks(anchor.len())
            .filter(|pass| pass.len() == anchor.len())
        {
            assert_eq!(sorted(pass.to_vec()), (0..anchor.len()).collect::<Vec<_>>());
        }
        first_epochs.insert(
            plan.steps[..per]
                .iter()
                .flat_map(|s| s.archive.clone())
                .collect::<Vec<_>>(),
        );
        first_anchors.insert(draws[..anchor.len()].to_vec());
    }
    assert_eq!(first_epochs.len(), 8, "each seed has its own archive order");
    assert_eq!(first_anchors.len(), 8, "each seed has its own anchor order");
}

/// A label with the `SD1-T-s` prefix that is not exactly `SD1-T-s{k}`, one
/// digit k from 0 to 7 and nothing after it, is refused: it would read
/// seed 0 and repeat D1-0's batch orders under another label. The eight
/// exact labels read their seeds.
#[test]
fn sd1_seed_label_near_misses_are_refused() {
    for (label, expected) in [
        ("SD1-T-s8", "SD1-T-s0 to SD1-T-s7"),
        ("SD1-T-s07", "SD1-T-s0 to SD1-T-s7"),
        ("SD1-T-s-1", "SD1-T-s0 to SD1-T-s7"),
        ("SD1-T-s1x", "SD1-T-s0 to SD1-T-s7"),
        ("SD1-T-s1-x", "SD1-T-s0 to SD1-T-s7"),
        ("SD1-T-s", "SD1-T-s0 to SD1-T-s7"),
        ("SD1-T-s1 ", "ASCII letters, digits or '-'"),
    ] {
        assert_eq!(order_seed_v1(label), 0, "{label}");
        let error = recipe_v1(ArchiveDistillationArmV1::Uniform, Some(&cell(label)))
            .err()
            .unwrap_or_else(|| panic!("{label:?} accepted"));
        assert!(error.contains(expected), "{label:?}: {error}");
    }
    for seed in 0..8 {
        let label = format!("SD1-T-s{seed}");
        let recipe = recipe_v1(ArchiveDistillationArmV1::Uniform, Some(&cell(&label))).unwrap();
        assert_eq!(recipe.order_seed, seed, "{label}");
    }
}

#[test]
fn a_cell_checkpoint_loads_only_at_its_recorded_step_size_on_arm_u() {
    let root = temp_root("checkpoint-cell");
    let (mut policy, model, parent, file) = parent_fixture(&root);
    let template = model.parameter_snapshot_v1();
    let parameters = distilled(&parent, &template);
    let mut record = provenance(&parent, &file);
    record.learning_rate_bits = 2.5e-5_f32.to_bits();
    record.cell = Some(ArchiveDistillationCellV1 {
        cell: "U-lr".into(),
        anchor_coefficient_bits: 1.0_f64.to_bits(),
        min_edge_backups: None,
        count_control: None,
        checkpoint_steps: vec![8, 16, 32],
        keep_sets: Some(pin("keep.json", 'e')),
    });
    let saved = checkpoint_v1(&parent, &parameters, record.clone()).unwrap();
    assert_eq!(saved.learning_rate_bits, 2.5e-5_f32.to_bits());
    let restored = restore_checkpoint_state_v1(&saved, &mut policy, model.clone()).unwrap();
    assert_eq!(
        hex(&restored.state_sha256_v1().unwrap()),
        saved.state_sha256
    );
    // The cell round-trips, and a checkpoint without one keeps its bytes.
    let bytes = serde_json::to_vec(&saved).unwrap();
    let reread: ExpandedCheckpointV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        reread.archive_distillation.as_ref().unwrap().cell,
        record.cell
    );
    let frozen = checkpoint_v1(&parent, &parameters, provenance(&parent, &file)).unwrap();
    assert!(
        serde_json::to_value(&frozen).unwrap()["archive_distillation"]
            .get("cell")
            .is_none()
    );
    type Case = (&'static str, fn(&mut ExpandedCheckpointV1));
    let cases: [Case; 4] = [
        ("saved step size differs", |s| {
            s.learning_rate_bits = LEARNING_RATE.to_bits()
        }),
        ("recorded step size differs", |s| {
            s.archive_distillation.as_mut().unwrap().learning_rate_bits = LEARNING_RATE.to_bits()
        }),
        ("cell on arm L", |s| {
            s.archive_distillation.as_mut().unwrap().arm = ArchiveDistillationArmV1::LineDecisions
        }),
        ("cell removed", |s| {
            s.archive_distillation.as_mut().unwrap().cell = None
        }),
    ];
    for (name, mutate) in cases {
        let mut changed = saved.clone();
        mutate(&mut changed);
        rehash(&mut changed, &template);
        assert!(
            restore_checkpoint_state_v1(&changed, &mut policy, model.clone()).is_err(),
            "{name} accepted"
        );
    }
    fs::remove_dir_all(root).unwrap();
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
        anchor_identity_preflight_v1(
            &fresh,
            generation,
            &rows,
            &exact,
            &frozen_recipe(),
            ArchiveDistillationUpdateScopeV1::FullNetwork
        ),
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
        let error = anchor_identity_preflight_v1(
            &fresh,
            generation,
            &rows,
            targets,
            &frozen_recipe(),
            ArchiveDistillationUpdateScopeV1::FullNetwork,
        )
        .unwrap_err();
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
    let error = anchor_identity_preflight_v1(
        &stepped,
        generation,
        &rows,
        &exact,
        &frozen_recipe(),
        ArchiveDistillationUpdateScopeV1::FullNetwork,
    )
    .unwrap_err();
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

fn archive_game(id: &str, episode: ExpandedEpisodeV1) -> ArchiveGameV1 {
    ArchiveGameV1 {
        id: id.into(),
        lineage: "r1".into(),
        episode,
        actions: Vec::new(),
        starts: Vec::new(),
        decisions: Vec::new(),
        end: "win".into(),
    }
}

/// SD1's evidence inputs: two keep-set files on the training bank listing
/// disjoint games, each row with its game's ordered matchup; the separated
/// mass and KL summaries, overall, per focal deck and per held-out row; and
/// the held-out anchor readings over all decks, the non-Spy decks and each
/// deck.
#[test]
fn probe_evidence_reads_separated_mass_kl_matchups_and_decks() {
    assert_eq!(
        separated_mass_v1(&[0.5, 0.25, 0.125, 0.125], &[0, 2]),
        0.375
    );
    assert_eq!(separated_mass_v1(&[0.5, 0.25, 0.25], &[1]), 0.75);
    let root = temp_root("probe-evidence");
    let mut rows = bank_rows(60);
    rows[3].0.parent_logits = logits_bits(&[1.0, -0.5, 2.0]);
    // game-0 and game-1 have Spy as the focal deck (seats 0 and 1); game-2's
    // focal deck is Terror.
    let mut terror = real_episode("game-2", 3, 0);
    terror.selected.swap(0, 1);
    let games = vec![
        archive_game("game-0", real_episode("game-0", 1, 0)),
        archive_game("game-1", real_episode("game-1", 2, 1)),
        archive_game("game-2", terror),
    ];
    let bank_sha = "b".repeat(64);
    let pins =
        |tag: &str, training: &[(&str, u64, Vec<u32>)], held_out: &[(&str, u64, Vec<u32>)]| {
            ArchiveDistillationProbeEvidenceV1 {
                training: write_pinned(
                    &root.join(format!("{tag}-training.json")),
                    &keep_file(training),
                ),
                held_out: write_pinned(
                    &root.join(format!("{tag}-held-out.json")),
                    &keep_file(held_out),
                ),
            }
        };
    let evidence = pins(
        "base",
        &[("game-1", 0, vec![1]), ("game-0", 3, vec![0, 2])],
        &[("game-2", 5, vec![0, 1])],
    );
    let [training, held_out] =
        evidence_split_v1(&evidence, "r1", &bank_sha, &rows, &games).unwrap();
    assert_eq!(
        training.iter().map(|e| e.index).collect::<Vec<_>>(),
        vec![3, 20],
        "bank order"
    );
    assert_eq!(training[0].keep, vec![0, 2]);
    assert_eq!(
        training[0].target,
        kept_target_v1(&rows[3].0, &[0, 2]).unwrap()
    );
    let spy = ["Spy".to_string(), "Terror".to_string()];
    assert!(training.iter().all(|e| e.matchup == spy));
    assert_eq!(held_out[0].index, 45);
    assert_eq!(
        held_out[0].matchup,
        ["Terror".to_string(), "Spy".to_string()]
    );
    for (training, held, games, wanted) in [
        (
            vec![("game-0", 3, vec![0, 2])],
            vec![("game-0", 4, vec![1])],
            &games[..],
            "share a game",
        ),
        (
            vec![("game-0", 3, vec![0, 1, 2])],
            vec![("game-2", 5, vec![0, 1])],
            &games[..],
            "separates at least one action",
        ),
        (
            vec![("game-0", 3, vec![0, 2])],
            vec![("game-2", 5, vec![0, 1])],
            &games[..2],
            "not a bank game",
        ),
    ] {
        let error = evidence_split_v1(
            &pins(wanted, &training, &held),
            "r1",
            &bank_sha,
            &rows,
            games,
        )
        .err()
        .unwrap();
        assert!(error.contains(wanted), "{error}");
    }
    let error = evidence_split_v1(&evidence, "r1", &"c".repeat(64), &rows, &games)
        .err()
        .unwrap();
    assert!(error.contains("identity differs"), "{error}");
    // The fit summaries: means overall and per focal deck, and each row's
    // values with its matchup only where asked.
    let terror_spy = ["Terror".to_string(), "Spy".to_string()];
    let reading = |game, step, matchup, separated_mass, kl_target| EvidenceReadingV1 {
        game,
        step,
        matchup,
        separated_mass,
        kl_target,
    };
    let readings = [
        reading("game-0", 3, &spy, 0.25, 0.5),
        reading("game-1", 0, &spy, 0.5, 1.0),
        reading("game-2", 5, &terror_spy, 0.125, 0.25),
    ];
    let summary = evidence_summary_v1(&readings, false);
    assert_eq!(
        summary,
        json!({"rows":3,"mean_separated_mass":0.875 / 3.0,"mean_kl_target":1.75 / 3.0,
            "by_deck":{"Spy":{"rows":2,"mean_separated_mass":0.375,"mean_kl_target":0.75},
                "Terror":{"rows":1,"mean_separated_mass":0.125,"mean_kl_target":0.25}}})
    );
    let held = evidence_summary_v1(&readings[2..], true);
    assert_eq!(
        held["row_values"],
        json!([{"game":"game-2","step":5,"matchup":["Terror","Spy"],
            "separated_mass":0.125,"kl_target":0.25}])
    );
    assert_eq!(
        evidence_summary_v1(&[], true),
        json!({"rows":0,"mean_separated_mass":null,"mean_kl_target":null,"by_deck":{},
            "row_values":[]})
    );
    // Held-out anchor readings: all nine decks, the non-Spy decks, each deck.
    let anchor = |deck, gap, agrees, kl| AnchorReadingV1 {
        deck,
        gap,
        agrees,
        kl,
    };
    let decks = anchor_decks_v1(&[
        anchor("Spy", 1.0, true, 0.25),
        anchor("Spy", 3.0, false, 0.75),
        anchor("Terror", 2.0, true, 0.5),
        anchor("Elves", 16.0, true, 0.0),
    ]);
    assert_eq!(
        decks,
        json!({
            "all":{"rows":4,"median_clamped_top_two_gap":2.5,"top_agreement":0.75,
                "mean_kl_from_parent":0.375},
            "non_spy":{"rows":2,"median_clamped_top_two_gap":9.0,"top_agreement":1.0,
                "mean_kl_from_parent":0.25},
            "by_deck":{
                "Elves":{"rows":1,"median_clamped_top_two_gap":16.0,"top_agreement":1.0,
                    "mean_kl_from_parent":0.0},
                "Spy":{"rows":2,"median_clamped_top_two_gap":2.0,"top_agreement":0.5,
                    "mean_kl_from_parent":0.5},
                "Terror":{"rows":1,"median_clamped_top_two_gap":2.0,"top_agreement":1.0,
                    "mean_kl_from_parent":0.5}}})
    );
    assert_eq!(
        anchor_decks_v1(&[anchor("Spy", 1.0, true, 0.25)])["non_spy"],
        json!({"rows":0,"median_clamped_top_two_gap":null,"top_agreement":null,
            "mean_kl_from_parent":null})
    );
    fs::remove_dir_all(root).unwrap();
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
        training_math: TrainingMathV1::PlatformLibm,
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
        training_math: TrainingMathV1::PlatformLibm,
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
        options: None,
        update_scope: ArchiveDistillationUpdateScopeV1::FullNetwork,
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
        evidence: None,
        evidence_logits: false,
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
        evidence: None,
        evidence_logits: false,
        output_directory: world.root.join("probe-reused-offers"),
    })
    .unwrap_err();
    assert!(error.contains("appears twice"), "{error}");
    // With SD1's evidence files (game k0's rows trained, game k1's held out,
    // each keeping every action but its last), each readout gains the
    // evidence fit and the per-deck anchor readings and keeps every other
    // value; the probe records the files.
    let bank_pin: PinnedFileV1 = serde_json::from_value(bank["bank"].clone()).unwrap();
    let games: Vec<ArchiveGameV1> = fs::read_to_string(&world.games.path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let rows_of = |game: &str| {
        record
            .rows
            .iter()
            .filter(|r| r.game == game)
            .collect::<Vec<_>>()
    };
    let evidence_file = |game: &str, name: &str| {
        let rows = rows_of(game)
            .iter()
            .map(|r| ArchiveDistillationKeepRowV1 {
                game: r.game.clone(),
                step: r.step,
                keep: (0..r.k - 1).collect(),
            })
            .collect();
        write_pinned(
            &world.root.join(name),
            &ArchiveDistillationKeepSetsV1 {
                schema: KEEP_SETS_SCHEMA_V1.into(),
                lineage: "r1".into(),
                bank_rows_sha256: record.rows_sha256.clone(),
                rows,
            },
        )
    };
    let (trained_game, held_game) = (games[0].id.as_str(), games[1].id.as_str());
    let evidence = ArchiveDistillationProbeEvidenceV1 {
        training: evidence_file(trained_game, "keep-training.json"),
        held_out: evidence_file(held_game, "keep-held-out.json"),
    };
    let probe_with = |evidence: ArchiveDistillationProbeEvidenceV1, out: &str| {
        execute_v1(ExpandedTrainingCommandV1::ArchiveDistillationProbe {
            parent: world.parent.clone(),
            lineage: "r1".into(),
            candidates: vec![
                candidate("parent", &world.parent),
                candidate("distilled", &distilled),
            ],
            banks: vec![ArchiveDistillationProbeBankV1 {
                role: "training".into(),
                games: world.games.clone(),
                bank: bank_pin.clone(),
            }],
            anchor: world.anchor.clone(),
            offers: world.offers.clone(),
            evidence: Some(evidence),
            evidence_logits: false,
            output_directory: world.root.join(out),
        })
    };
    let evidenced = probe_with(evidence.clone(), "probe-evidence").unwrap();
    assert_eq!(evidenced["evidence"], json!(evidence));
    assert!(probe.get("evidence").is_none());
    let pairs = evidenced["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .zip(probe["candidates"].as_array().unwrap());
    for (with, without) in pairs {
        let mut trimmed = with.clone();
        let fields = trimmed.as_object_mut().unwrap();
        assert!(fields.remove("evidence").is_some());
        assert!(fields.remove("anchor_held_out_decks").is_some());
        assert_eq!(&trimmed, without);
        let (held, decks) = (&with["anchor_held_out"], &with["anchor_held_out_decks"]);
        assert_eq!(decks["all"]["rows"], held["rows"]);
        assert_eq!(
            decks["all"]["median_clamped_top_two_gap"],
            held["median_clamped_top_two_gap"]
        );
        assert_eq!(
            decks["all"]["mean_kl_from_parent"],
            held["mean_kl_from_parent_all"]
        );
        assert_eq!(decks["non_spy"]["rows"], held["non_spy_rows"]);
        assert_eq!(
            decks["non_spy"]["top_agreement"],
            held["non_spy_top_agreement"]
        );
        assert_eq!(
            decks["non_spy"]["mean_kl_from_parent"],
            held["mean_kl_from_parent_non_spy"]
        );
    }
    let parent_reading = &evidenced["candidates"][0];
    assert_eq!(
        parent_reading["anchor_held_out_decks"]["all"]["top_agreement"],
        json!(1.0)
    );
    // The parent's fit, recomputed from the bank's recorded parent logits.
    let fit = &parent_reading["evidence"];
    for (name, game) in [("training", trained_game), ("held_out", held_game)] {
        let members = rows_of(game);
        assert!(!members.is_empty());
        let mut masses = Vec::new();
        let mut kls = Vec::new();
        for r in &members {
            let logits = recorded_logits(&r.parent_logits);
            masses.push(deployed_v1(&logits).unwrap()[r.k as usize - 1]);
            let keep: Vec<u32> = (0..r.k - 1).collect();
            let target = kept_target_v1(r, &keep).unwrap();
            kls.push(
                line_b_divergence_v1(LineBDivergenceV1::Forward, &f64_logits(&logits), &target)
                    .unwrap()
                    .divergence,
            );
        }
        assert_eq!(fit[name]["rows"], json!(members.len()));
        assert_eq!(fit[name]["mean_separated_mass"], json!(mean(&masses)));
        assert_eq!(fit[name]["mean_kl_target"], json!(mean(&kls)));
        let episode = &games.iter().find(|g| g.id == game).unwrap().episode;
        let seat = usize::from(episode.learner_seat);
        let matchup = json!([
            episode.selected[seat].label,
            episode.selected[1 - seat].label
        ]);
        assert_eq!(
            fit[name]["by_deck"][&episode.selected[seat].label]["rows"],
            json!(members.len())
        );
        if name == "held_out" {
            let values = fit[name]["row_values"].as_array().unwrap();
            assert_eq!(values.len(), members.len());
            for (value, r) in values.iter().zip(&members) {
                assert_eq!(value["game"], json!(game));
                assert_eq!(value["step"], json!(r.step));
                assert_eq!(value["matchup"], matchup);
            }
        } else {
            assert!(fit[name].get("row_values").is_none());
        }
    }
    // The two files must list disjoint games.
    let shared = ArchiveDistillationProbeEvidenceV1 {
        training: evidence.training.clone(),
        held_out: evidence.training.clone(),
    };
    let error = probe_with(shared, "probe-evidence-shared").unwrap_err();
    assert!(error.contains("share a game"), "{error}");
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
            options: None,
            update_scope: ArchiveDistillationUpdateScopeV1::FullNetwork,
            output_directory: world.root.join(name),
        })
    };
    let started = std::time::Instant::now();
    let full = run(ArchiveDistillationArmV1::Uniform, None, "phase1-u").unwrap();
    let steps = full["steps"].as_u64().unwrap();
    eprintln!(
        "timing note: uniform phase 1, {steps} steps of up to {} archive and {ANCHOR_BATCH} anchor rows, {:.1} s",
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
    // A keep-set cell listing every row with its executed action alone
    // trains arm U's one-hot targets in U's order: the same bits at the
    // reproduction step. Keeping every action makes every target the
    // parent's own softmax: zero archive loss, and the parent's parameters
    // unchanged to the last checkpoint, which carries the file's pin.
    let bank_record: ArchiveBankV1 = read_pinned(&bank_pin).unwrap();
    let keep_pin = |name: &str, keep: fn(&ArchiveRowV1) -> Vec<u32>| {
        write_pinned(
            &world.root.join(name),
            &ArchiveDistillationKeepSetsV1 {
                schema: KEEP_SETS_SCHEMA_V1.into(),
                lineage: "r1".into(),
                bank_rows_sha256: bank_record.rows_sha256.clone(),
                rows: bank_record
                    .rows
                    .iter()
                    .map(|row| ArchiveDistillationKeepRowV1 {
                        game: row.game.clone(),
                        step: row.step,
                        keep: keep(row),
                    })
                    .collect(),
            },
        )
    };
    let kept_run = |label: &str, keep_sets: &PinnedFileV1, name: &str| {
        let mut options = cell(label);
        options.keep_sets = Some(keep_sets.clone());
        options.checkpoint_steps = Some(vec![8, 16, REPRODUCTION_STEP]);
        // Built from its wire form, which every full-network run shares.
        let command: ExpandedTrainingCommandV1 = serde_json::from_value(json!({
            "mode": "archive_distillation_phase1",
            "parent": world.parent,
            "lineage": "r1",
            "arm": ArchiveDistillationArmV1::Uniform,
            "games": world.games,
            "bank": bank_pin,
            "anchor": world.anchor,
            "options": options,
            "output_directory": world.root.join(name),
        }))
        .unwrap();
        execute_v1(command).unwrap()
    };
    let executed = keep_pin("keep-executed.json", |row| vec![row.target]);
    let one_hot = kept_run("K-executed", &executed, "phase1-k-executed");
    assert_eq!(at(&one_hot), at(&full));
    let census = one_hot["census"]["keep_rows"].as_array().unwrap();
    assert_eq!(census.len(), bank_record.rows.len());
    let everything = keep_pin("keep-all.json", |row| (0..row.k).collect());
    let parent_target = kept_run("K-all", &everything, "phase1-k-all");
    assert_ne!(at(&parent_target), at(&full));
    assert!(parent_target["census"]["keep_rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["kl_target_from_parent"] == json!(0.0)));
    let telemetry: Vec<Value> =
        read_pinned(&serde_json::from_value(parent_target["telemetry"].clone()).unwrap()).unwrap();
    assert!(telemetry
        .iter()
        .all(|step| step["archive_loss"] == json!(0.0)));
    let last = parent_target["checkpoints"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let source: ExpandedModelSourceV1 =
        read_pinned(&serde_json::from_value(last["source"].clone()).unwrap()).unwrap();
    let saved: ExpandedCheckpointV1 = read_pinned(source.checkpoint.as_ref().unwrap()).unwrap();
    assert_eq!(saved.parameters, parent_checkpoint.parameters);
    let recorded = saved.archive_distillation.unwrap().cell.unwrap();
    assert_eq!(recorded.keep_sets, Some(everything));
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
        evidence: None,
        evidence_logits: false,
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

fn trace_group(episode: usize, value: f32, terminal_return: i8) -> retention_trace::TraceGroupV1 {
    retention_trace::TraceGroupV1 {
        episode,
        rows: vec![0],
        terminal_return,
        value,
    }
}

/// The retention trace's credit is the GAE recursion over each episode's
/// learner decisions, with their TD errors, next values and decisions left,
/// normalized over the whole update; an episode's decisions share one return.
#[test]
fn retention_credit_follows_the_gae_recursion_and_the_updates_normalization() {
    let groups = vec![
        trace_group(0, 0.25, 1),
        trace_group(0, 0.5, 1),
        trace_group(0, -0.125, 1),
        trace_group(2, 0.0, -1),
        trace_group(2, 0.75, -1),
    ];
    let (credit, statistics) = retention_trace::credit_v1(&groups, 1.0, 0.9).unwrap();
    // By hand: the TD errors, then the advantages back from each episode's
    // end with gamma times lambda 0.9.
    let td: [f32; 5] = [0.25, -0.625, 1.125, 0.75, -1.75];
    let raw: [f32; 5] = [
        0.25 + 0.9 * (-0.625 + 0.9 * 1.125),
        -0.625 + 0.9 * 1.125,
        1.125,
        0.75 + 0.9 * -1.75,
        -1.75,
    ];
    for (index, entry) in credit.iter().enumerate() {
        assert!((entry.td_error - td[index]).abs() < 1e-6, "{index}");
        assert!((entry.raw - raw[index]).abs() < 1e-6, "{index}");
        assert_eq!(entry.value, groups[index].value);
    }
    assert_eq!(
        credit.iter().map(|c| c.next_value).collect::<Vec<_>>(),
        vec![Some(0.5f32), Some(-0.125), None, Some(0.75), None]
    );
    assert_eq!(
        credit.iter().map(|c| c.groups_left).collect::<Vec<_>>(),
        vec![3usize, 2, 1, 2, 1]
    );
    let raw: Vec<f32> = credit.iter().map(|c| c.raw).collect();
    let (normalized, expected) =
        normalize_advantages_v1(&raw, GAE_ADVANTAGE_NORMALIZATION_EPSILON_V1).unwrap();
    assert_eq!(statistics, expected);
    assert_eq!(
        credit.iter().map(|c| c.normalized).collect::<Vec<_>>(),
        normalized
    );
    let mut mixed = groups.clone();
    mixed[1].terminal_return = -1;
    let error = retention_trace::credit_v1(&mixed, 1.0, 0.9).unwrap_err();
    assert!(error.contains("terminal return"), "{error}");
    assert!(retention_trace::credit_v1(&[], 1.0, 0.9).is_err());
}

/// The push is the normalized advantage over the update's group count times
/// (chosen, less the line mass). The slope is the first-order change of
/// log(line mass) under the same logit change: 2 (1 - p)^2 and -2 p (1 - p)
/// times A / G at a two-action menu, and of the opposite sign to the push
/// when a declined menu's chosen action is unlikely and a third action holds
/// most of the mass.
#[test]
fn retention_push_follows_the_advantage_and_the_choice() {
    assert_eq!(retention_trace::push_v1(2.0, 4, true, 0.25), 0.375);
    assert_eq!(retention_trace::push_v1(2.0, 4, false, 0.25), -0.125);
    assert_eq!(retention_trace::push_v1(-1.0, 2, true, 0.25), -0.375);
    assert_eq!(retention_trace::push_v1(-1.0, 1, false, 1.0), 1.0);
    let close = |a: Option<f64>, b: f64| assert!((a.unwrap() - b).abs() < 1e-12, "{a:?} {b}");
    let two = [0.25, 0.75];
    close(
        retention_trace::slope_v1(2.0, 4, &two, &[0], 0),
        0.5 * 2.0 * 0.75 * 0.75,
    );
    close(
        retention_trace::slope_v1(2.0, 4, &two, &[0], 1),
        -0.5 * 2.0 * 0.25 * 0.75,
    );
    let wide = [0.2, 0.1, 0.7];
    assert_eq!(retention_trace::push_v1(1.0, 1, false, 0.2), -0.2);
    close(retention_trace::slope_v1(1.0, 1, &wide, &[0], 1), 0.24);
    assert_eq!(
        retention_trace::slope_v1(1.0, 1, &[0.0, 1.0], &[0], 1),
        None
    );
}

/// A reset start of a real game whose Spy seat follows a scripted line (cast
/// Spy, target itself, Dread Return on the Giant, else play a land, cast a
/// spell or pass; the other seat passes), cut at the Spy seat's first menu
/// that offers `decision` (1 the self-target, 2 Dread Return on the Giant).
/// The learner's first decision after the cut offers that line choice.
fn scripted_line_reset(id: &str, seed: u64, spy_seat: u8, decision: usize) -> ExpandedEpisodeV1 {
    use crate::rl::ActionSemanticV1;
    let mut episode = real_episode(id, seed, spy_seat);
    let mut session = episode_session_v1(&episode, &episode.configurations().unwrap()).unwrap();
    let mut prefix = Vec::new();
    loop {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("the scripted game ends before the line")
        };
        let semantics = session
            .diagnostic_current_action_semantics()
            .unwrap_or_default();
        let pick = |pred: &dyn Fn(&ActionSemanticV1) -> bool| semantics.iter().position(pred);
        let action = if seat(d.acting_player) == spy_seat && d.legal_action_count >= 2 {
            let flags = spy_menu_flags_v1(&session, spy_seat);
            let line = |which: usize| flags.iter().position(|f| [f.0, f.1, f.2][which]);
            if line(decision).is_some() {
                assert_eq!(d.substep_index, 0, "the line menu starts a decision");
                let (observation, menu) =
                    crate::regret_census_v1::decision_replay_key(&session, d).unwrap();
                episode.reset = Some(ExpandedResetStartV1 {
                    guide: "scripted-line".into(),
                    game_seed: seed,
                    prefix,
                    cut_physical_decision_id: d.physical_decision_id,
                    cut_observation_sha256: observation,
                    cut_menu_sha256: menu,
                    cut_state_hash: format!("{:016x}", session.diagnostic_state_hash()),
                });
                return episode;
            }
            line(0)
                .or_else(|| line(1))
                .or_else(|| line(2))
                .or_else(|| pick(&|x| matches!(x, ActionSemanticV1::PlayLand { .. })))
                .or_else(|| pick(&|x| matches!(x, ActionSemanticV1::CastSpell { .. })))
                .or_else(|| pick(&|x| matches!(x, ActionSemanticV1::Pass { .. })))
                .unwrap_or(0)
        } else {
            pick(&|x| matches!(x, ActionSemanticV1::Pass { .. })).unwrap_or(0)
        } as u32;
        prefix.push(action);
        session.step(d.episode_id, d.step, action).unwrap();
    }
}

/// A real GAE run through the production run path: two updates of Spy
/// learner games at both seats (ordinary games, and reset starts at the
/// line's self-target and Giant menus), and one game with Spy as the other
/// deck.
fn retention_run_config_v1(
    root: &Path,
) -> crate::native_expanded_training_run_v1::NativeExpandedTrainingRunV1 {
    use crate::native_expanded_training_run_v1::{
        ExpandedOpponentAssignmentV1, ExpandedRunIterationV1, NativeExpandedTrainingRunV1,
        ScheduledExpandedEpisodeV1,
    };
    let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V3;
    let fresh = write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        feature_identity,
        |_parameters| {},
    );
    let initial_source = ExpandedModelSourceV1 {
        play_import: write_pinned(&root.join("descriptor.json"), &fresh),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
    };
    // `real_episode` seats Spy at its seat argument.
    let scheduled = |id: &str, seed: u64, learner_seat: u8, spy_learner: bool| {
        let spy_seat = if spy_learner {
            learner_seat
        } else {
            1 - learner_seat
        };
        let mut episode = real_episode(id, seed, spy_seat);
        episode.learner_seat = learner_seat;
        ScheduledExpandedEpisodeV1 {
            episode,
            opponent: ExpandedOpponentAssignmentV1::Current,
        }
    };
    let reset = |id: &str, seed: u64, spy_seat: u8, decision: usize| ScheduledExpandedEpisodeV1 {
        episode: scripted_line_reset(id, seed, spy_seat, decision),
        opponent: ExpandedOpponentAssignmentV1::Current,
    };
    let iterations = vec![
        ExpandedRunIterationV1 {
            episodes: vec![
                scheduled("trace-u0-spy-s0", 2_026_101_501, 0, true),
                scheduled("trace-u0-other-s0", 2_026_101_502, 0, false),
                reset("trace-u0-self-s0", 2_026_101_511, 0, 1),
                reset("trace-u0-giant-s1", 2_026_101_513, 1, 2),
            ],
        },
        ExpandedRunIterationV1 {
            episodes: vec![
                scheduled("trace-u1-spy-s1", 2_026_101_505, 1, true),
                reset("trace-u1-self-s1", 2_026_101_516, 1, 1),
                reset("trace-u1-giant-s0", 2_026_101_515, 0, 2),
            ],
        },
    ];
    NativeExpandedTrainingRunV1 {
        schema: "mtg-kernel-native-expanded-training-run/v1".into(),
        initial_source,
        opponents: vec![],
        iterations,
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        loss_selection: ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 1.0,
            lambda: 0.9,
            entropy_coefficient: 0.0,
        },
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        collection_workers: 1,
        preparation_workers: 1,
        max_non_natural_episode_fraction: 0.0,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        line_b: None,
        e_supported: None,
        output_directory: root.join("run"),
    }
}

fn file_pin(path: PathBuf) -> PinnedFileV1 {
    let bytes = fs::read(&path).unwrap();
    PinnedFileV1 {
        sha256: sha(&bytes),
        path,
    }
}

fn read_jsonl(pin: &Value) -> Vec<Value> {
    let pin: PinnedFileV1 = serde_json::from_value(pin.clone()).unwrap();
    let bytes = fs::read(&pin.path).unwrap();
    assert_eq!(sha(&bytes), pin.sha256);
    bytes
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

/// The retention trace of a real two-update GAE run. X2 holds on both
/// updates, policy sum, value sum and loss included; only the Spy learner's
/// episodes are traced, and each one's offers and choices match a recount
/// with the archive's classifier; each listed menu's push and slope follow
/// its normalized advantage, the update's group count and the softmax of its
/// recorded logits, and the update sums add up. A changed statistic, one ulp
/// of the recorded loss, a changed recorded value, a run that is not GAE, a
/// mismatched run pair and a used output directory are refused. Run
/// explicitly, in release:
/// `cargo test --release -p mtg-kernel --lib retention_trace_reads -- --ignored`.
#[test]
#[ignore = "end to end on real games; run explicitly"]
fn retention_trace_reads_a_real_gae_run() {
    let root = temp_root("retention-trace");
    let config = retention_run_config_v1(&root);
    let started = std::time::Instant::now();
    let completion =
        crate::native_expanded_training_run_v1::run_native_expanded_training_v1(&config, None)
            .unwrap();
    eprintln!(
        "timing note: the two-update run took {:.1} s",
        started.elapsed().as_secs_f64()
    );
    assert_eq!(completion["complete"], true);
    let run_pin = file_pin(config.output_directory.join("run.json"));
    let completion_pin = file_pin(config.output_directory.join("completion.json"));
    let trace = |run: &PinnedFileV1, completion: &PinnedFileV1, name: &str| {
        execute_v1(ExpandedTrainingCommandV1::LineRetentionTrace {
            run: run.clone(),
            completion: completion.clone(),
            output_directory: root.join(name),
        })
    };
    let result = trace(&run_pin, &completion_pin, "trace").unwrap();
    assert_eq!(result["x2"], "passed");

    // The update records' counts and statistics, and the Spy learner's
    // trajectories in update order.
    let receipts: Vec<PinnedFileV1> =
        serde_json::from_value(completion["iterations"].clone()).unwrap();
    let mut spy = Vec::new();
    for (iteration, receipt) in receipts.iter().enumerate() {
        let receipt: Value = read_pinned(receipt).unwrap();
        let update: Value = read_pinned::<Value>(
            &serde_json::from_value::<PinnedFileV1>(receipt["update"].clone()).unwrap(),
        )
        .unwrap();
        let summary = &result["iterations"][iteration];
        for key in [
            "physical_decisions",
            "policy_substeps",
            "advantage_statistics",
            "policy_sum",
            "value_sum",
            "loss",
        ] {
            assert_eq!(summary[key], update[key], "{key}");
        }
        let pins: Vec<PinnedFileV1> =
            serde_json::from_value(update["trajectories"].clone()).unwrap();
        for pin in &pins {
            let t: ExpandedTrajectoryV1 = read_pinned(pin).unwrap();
            if is_spy_label(&t.episode.selected[usize::from(t.episode.learner_seat)].label) {
                spy.push((iteration, t));
            }
        }
    }
    assert_eq!(spy.len(), 6);
    assert_eq!(result["spy_learner_episodes"], json!(spy.len()));

    // A recount of every Spy learner episode with the archive's classifier.
    let rows = read_jsonl(&result["rows"]);
    let episodes = read_jsonl(&result["episodes"]);
    assert_eq!(episodes.len(), spy.len());
    let mut listed = 0;
    for ((iteration, t), episode_row) in spy.iter().zip(&episodes) {
        let learner = t.episode.learner_seat;
        let mut session =
            episode_session_v1(&t.episode, &t.episode.configurations().unwrap()).unwrap();
        restore_reset_cut_v1(&mut session, &t.episode).unwrap();
        let mut counts = [[0usize; 2]; 3];
        let mut learner_decisions = 0;
        for row in &t.decisions {
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                panic!("the replay ends before its rows")
            };
            if row.actor == learner && row.substep_index == 0 {
                learner_decisions += 1;
            }
            if row.actor == learner && row.logits.len() >= 2 {
                let (flags, own, giant) = line_flags_v1(&session, learner, row.selected);
                let pairs = [
                    (flags.cast_offered, flags.cast_chosen),
                    (flags.self_offered, flags.self_chosen),
                    (flags.giant_offered, flags.giant_chosen),
                ];
                for (which, (offered, chosen)) in pairs.into_iter().enumerate() {
                    counts[which][0] += usize::from(offered);
                    counts[which][1] += usize::from(chosen);
                }
                for (which, actions) in [own, giant].iter().enumerate() {
                    if actions.is_empty() {
                        continue;
                    }
                    let entry = &rows[listed];
                    listed += 1;
                    assert_eq!(entry["iteration"], json!(iteration));
                    assert_eq!(entry["episode"], json!(t.episode.id));
                    assert_eq!(entry["step"], json!(row.step));
                    assert_eq!(entry["decision"], ["self", "giant"][which]);
                    assert_eq!(entry["line_actions"], json!(actions));
                    let softmax = line_b_softmax_v1(&f64_logits(&recorded_logits(&row.logits)))
                        .unwrap()
                        .probabilities;
                    let mass: f64 = actions.iter().map(|&a| softmax[a as usize]).sum();
                    let chosen = actions.contains(&row.selected);
                    assert_eq!(entry["chosen"], chosen);
                    assert!((entry["softmax_mass"].as_f64().unwrap() - mass).abs() < 1e-12);
                    let normalized = entry["normalized_advantage"].as_f64().unwrap() as f32;
                    let groups = result["iterations"][*iteration]["physical_decisions"]
                        .as_u64()
                        .unwrap() as usize;
                    assert_eq!(entry["group_count"], json!(groups));
                    let push = retention_trace::push_v1(normalized, groups, chosen, mass);
                    assert!((entry["push"].as_f64().unwrap() - push).abs() < 1e-12);
                    let line: Vec<usize> = actions.iter().map(|&a| a as usize).collect();
                    let slope = retention_trace::slope_v1(
                        normalized,
                        groups,
                        &softmax,
                        &line,
                        row.selected as usize,
                    );
                    assert_eq!(entry["slope"].as_f64(), slope);
                }
            }
            session.step(d.episode_id, d.step, row.selected).unwrap();
        }
        assert_eq!(episode_row["episode"], json!(t.episode.id));
        for (which, name) in ["cast", "self", "giant"].into_iter().enumerate() {
            assert_eq!(
                episode_row[name]["offered"],
                json!(counts[which][0]),
                "{name}"
            );
            assert_eq!(
                episode_row[name]["chosen"],
                json!(counts[which][1]),
                "{name}"
            );
        }
        assert_eq!(episode_row["learner_decisions"], json!(learner_decisions));
        assert_eq!(
            episode_row["terminal_return"],
            json!(t.terminal.terminal_reward[usize::from(learner)])
        );
    }
    assert_eq!(listed, rows.len());
    eprintln!(
        "timing note: {} listed menus in {} Spy learner episodes",
        rows.len(),
        spy.len()
    );
    for decision in ["self", "giant"] {
        assert!(
            rows.iter().any(|r| r["decision"] == decision),
            "the fixture's reset starts offer {decision}"
        );
    }

    // Each update's summed push, by decision, choice and result.
    let parts = ["chosen_won", "chosen_lost", "declined_won", "declined_lost"];
    for (iteration, summary) in result["iterations"].as_array().unwrap().iter().enumerate() {
        for name in ["self", "giant"] {
            let mut sums = [0.0f64; 4];
            let mut slopes = [0.0f64; 4];
            let mut menus = [0usize; 4];
            for entry in rows
                .iter()
                .filter(|r| r["iteration"] == json!(iteration) && r["decision"] == name)
            {
                let index = usize::from(entry["chosen"] != true) * 2
                    + usize::from(entry["terminal_return"].as_i64().unwrap() <= 0);
                sums[index] += entry["push"].as_f64().unwrap();
                slopes[index] += entry["slope"].as_f64().unwrap_or(0.0);
                menus[index] += 1;
            }
            for (index, part) in parts.iter().enumerate() {
                let recorded = &summary["push"][name][*part];
                assert_eq!(recorded["menus"], json!(menus[index]), "{name} {part}");
                assert!((recorded["push"].as_f64().unwrap() - sums[index]).abs() < 1e-9);
                assert!((recorded["slope"].as_f64().unwrap() - slopes[index]).abs() < 1e-9);
            }
        }
    }

    // Copies of the run's receipt chain with one change to update 0's
    // record, in a fresh directory.
    let rechain = |name: &str, change: &dyn Fn(&mut Value, &Path)| {
        let directory = root.join(name);
        fs::create_dir_all(&directory).unwrap();
        let receipt_pin: PinnedFileV1 =
            serde_json::from_value(completion["iterations"][0].clone()).unwrap();
        let mut receipt: Value = read_pinned(&receipt_pin).unwrap();
        let update_pin: PinnedFileV1 = serde_json::from_value(receipt["update"].clone()).unwrap();
        let mut update: Value = read_pinned(&update_pin).unwrap();
        change(&mut update, &directory);
        receipt["update"] = json!(write_pinned(&directory.join("update.json"), &update));
        let mut done = completion.clone();
        done["iterations"][0] = json!(write_pinned(&directory.join("complete.json"), &receipt));
        let manifest: Value = read_pinned(&run_pin).unwrap();
        (
            write_pinned(&directory.join("run.json"), &manifest),
            write_pinned(&directory.join("completion.json"), &done),
        )
    };
    let (run, done) = rechain("unchanged", &|_, _| {});
    trace(&run, &done, "trace-unchanged").unwrap();
    let (run, done) = rechain("statistic", &|update, _| {
        update["advantage_statistics"]["max"] = json!(1.5);
    });
    let error = trace(&run, &done, "trace-statistic").unwrap_err();
    assert!(error.contains("X2 failed at iteration 0"), "{error}");
    // One ulp of the recorded loss.
    let (run, done) = rechain("loss", &|update, _| {
        let loss = update["loss"].as_f64().unwrap() as f32;
        update["loss"] = json!(f32::from_bits(loss.to_bits() + 1));
    });
    let error = trace(&run, &done, "trace-loss").unwrap_err();
    assert!(error.contains("X2 failed at iteration 0"), "{error}");
    // A recorded value that enters the update's advantages.
    let (run, done) = rechain("value", &|update, directory| {
        let pin: PinnedFileV1 = serde_json::from_value(update["trajectories"][0].clone()).unwrap();
        let mut t: ExpandedTrajectoryV1 = read_pinned(&pin).unwrap();
        let learner = t.episode.learner_seat;
        let row = t
            .decisions
            .iter_mut()
            .find(|row| row.actor == learner && row.substep_index == 0)
            .unwrap();
        row.value = (f32::from_bits(row.value) + 0.5).to_bits();
        update["trajectories"][0] = json!(write_pinned(&directory.join("trajectory.json"), &t));
    });
    let error = trace(&run, &done, "trace-value").unwrap_err();
    assert!(error.contains("X2 failed at iteration 0"), "{error}");
    let (_, done) = rechain("pair", &|_, _| {});
    let error = trace(&run_pin, &done, "trace-pair").unwrap_err();
    assert!(error.contains("not one run's"), "{error}");
    let directory = root.join("legacy");
    fs::create_dir_all(&directory).unwrap();
    let mut manifest: Value = read_pinned(&run_pin).unwrap();
    manifest["config"]
        .as_object_mut()
        .unwrap()
        .remove("loss_selection");
    let legacy = write_pinned(&directory.join("run.json"), &manifest);
    let legacy_done = write_pinned(
        &directory.join("completion.json"),
        &read_pinned::<Value>(&completion_pin).unwrap(),
    );
    let error = trace(&legacy, &legacy_done, "trace-legacy").unwrap_err();
    assert!(error.contains("gae_advantage_value runs only"), "{error}");
    let error = trace(&run_pin, &completion_pin, "trace").unwrap_err();
    assert!(error.contains("fresh"), "{error}");
    fs::remove_dir_all(root).unwrap();
}
