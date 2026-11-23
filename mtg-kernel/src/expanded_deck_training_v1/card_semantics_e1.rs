//! Training admission, checkpoint state and offline import/export tools.
use super::*;

pub(crate) fn validate_update_v1(
    config: Option<&CardSemanticsConfigV1>,
    source: &ExpandedModelSourceV1,
    backend: ExpandedUpdateBackendV1,
    loss: &ExpandedLossSelectionV1,
    auxiliary: bool,
) -> Result<(), String> {
    if config.is_some() {
        ensure(
            source.s_learner.is_none()
                && source.w_learner.is_none()
                && !flat_v8_requested_v1(source)?
                && matches!(backend, ExpandedUpdateBackendV1::Cpu)
                && matches!(loss, ExpandedLossSelectionV1::GaeAdvantageValueV1 { .. })
                && !auxiliary,
            "card semantics requires ordinary net8 CPU GAE",
        )?;
    }
    Ok(())
}

/// Missing members default to None; an explicitly present null is invalid.
pub(crate) fn deserialize_present_v1<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom("card_semantics must not be null"))
}

/// The caller supplies the section from its existing checkpoint parse or
/// loaded-source cache. This admission never performs another checkpoint read.
pub(crate) fn load_training_v1(
    source: &ExpandedModelSourceV1,
    config: Option<&CardSemanticsConfigV1>,
    saved: Option<&CardSemanticsSectionV1>,
) -> Result<Option<CardSemanticsV1>, String> {
    bind_training_v1(
        config.map(CardSemanticsConfigV1::load_v1).transpose()?,
        saved,
        source.checkpoint.is_some(),
    )
}

fn bind_training_v1(
    mut state: Option<CardSemanticsV1>,
    saved: Option<&CardSemanticsSectionV1>,
    checkpoint: bool,
) -> Result<Option<CardSemanticsV1>, String> {
    match (&mut state, saved) {
        (Some(state), Some(saved)) => state.restore_v1(saved)?,
        (None, Some(_)) => return Err("card semantics checkpoint requires card_semantics config (export for flag-off training)".into()),
        (Some(_), None) if checkpoint => return Err("card semantics checkpoint section missing; use fork-import".into()),
        _ => {}
    }
    Ok(state)
}

// A one-shot tool command; the size difference between variants is irrelevant.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
enum ToolV1 {
    Export {
        source: ExpandedModelSourceV1,
        output_directory: PathBuf,
    },
    ForkImport {
        source: ExpandedModelSourceV1,
        initial_source: ExpandedModelSourceV1,
        tokens: Vec<usize>,
        card_semantics: CardSemanticsConfigV1,
        output_directory: PathBuf,
    },
}

pub fn tool_json_v1(text: &str) -> Result<Value, String> {
    crate::rl::parse_strict_json_value(text).map_err(err)?;
    match serde_json::from_str(text).map_err(err)? {
        ToolV1::Export {
            source,
            output_directory,
        } => {
            let (_, state) = initialize_any_math_v1(&source)?;
            let pin = source
                .checkpoint
                .as_ref()
                .ok_or("export requires checkpoint")?;
            let mut saved = read_checkpoint_v1(pin)?;
            ensure(
                saved.card_semantics.is_some(),
                "export requires card semantics section",
            )?;
            saved.card_semantics = None;
            saved.state_sha256 = hex(&state.state_sha256_v1().map_err(err)?);
            ensure(
                !output_directory.exists(),
                "output directory already exists",
            )?;
            fs::create_dir(&output_directory).map_err(err)?;
            let output = publish_json(&output_directory, "checkpoint.json", &saved)?;
            let back = readback_v1(&source, &output, None)?;
            ensure(
                same_tensors_v1(&saved, &back),
                "export readback tensor identity differs",
            )?;
            Ok(json!({"checkpoint":output}))
        }
        ToolV1::ForkImport {
            source,
            initial_source,
            tokens,
            card_semantics,
            output_directory,
        } => {
            let pin = source
                .checkpoint
                .as_ref()
                .ok_or("fork-import requires checkpoint")?;
            let admission: ForkAdmissionV1 = read_checkpoint_metadata_v1(pin)?;
            admission.validate_v1()?;
            let mut saved = read_checkpoint_v1(pin)?;
            admit_fork_v1(&saved)?;
            ensure(
                source.s_learner.is_none()
                    && source.w_learner.is_none()
                    && !flat_v8_requested_v1(&source)?,
                "fork-import requires an ordinary 33-tensor source",
            )?;
            let (_, state) = initialize_any_math_v1(&source)?;
            ensure(
                initial_source.checkpoint.is_none()
                    && initial_source.s_learner.is_none()
                    && initial_source.w_learner.is_none(),
                "initial source must be fresh initialization",
            )?;
            let bytes = read_pinned_bytes(&initial_source.play_import)?;
            let descriptor = fresh_initialization_source::parse_source_v1(&bytes)?;
            let initial = fresh_initialization_source::load_policy_v1(
                &descriptor,
                &initial_source.feature_transfer,
            )?;
            ensure(
                initial.identity_v1() == &saved.source_import,
                "initialization source differs from checkpoint lineage",
            )?;
            let parameters = initial.training_parameters_v3();
            let index = saved
                .parameters
                .iter()
                .position(|t| t.name == "card_embedding.weight")
                .ok_or("missing card embedding")?;
            ensure(
                saved.parameters[index].shape.len() == 2 && saved.parameters[index].shape[1] == 16,
                "card semantics refuses wide network",
            )?;
            let initial_table = parameters
                .iter()
                .find(|t| t.name == "card_embedding.weight")
                .ok_or("missing initial card embedding")?;
            ensure(
                initial_table.shape == saved.parameters[index].shape,
                "initial card table shape differs",
            )?;
            let mut seen = BTreeSet::new();
            ensure(!tokens.is_empty(), "fork-import requires card tokens")?;
            for token in tokens {
                ensure(
                    token > 0 && token < initial_table.shape[0] && seen.insert(token),
                    "invalid or duplicate card token",
                )?;
                let range = token * 16..(token + 1) * 16;
                for (out, value) in saved.parameters[index].values[range.clone()]
                    .iter_mut()
                    .zip(&initial_table.values[range.clone()])
                {
                    *out = value.to_bits();
                }
                saved.first_moments[index].values[range.clone()].fill(0);
                saved.second_moments[index].values[range].fill(0);
            }
            let semantics = card_semantics.load_v1()?;
            let tensor_hash = checkpoint_tensors_v1(&saved, state.model_v1())?
                .state_sha256_v1()
                .map_err(err)?;
            saved.state_sha256 = hex(&tensor_hash);
            let mut enabled = saved.clone();
            enabled.card_semantics = Some(semantics.section_v1());
            enabled.state_sha256 =
                checkpoint_hash_v1(&tensor_hash, enabled.card_semantics.as_ref())?;
            let identical = saved.parameters == enabled.parameters
                && saved.first_moments == enabled.first_moments
                && saved.second_moments == enabled.second_moments
                && saved.adam_step == enabled.adam_step;
            ensure(
                identical && saved.parameters.len() == 33,
                "fork tensor identity differs",
            )?;
            ensure(
                !output_directory.exists(),
                "output directory already exists",
            )?;
            fs::create_dir(&output_directory).map_err(err)?;
            let off = publish_json(&output_directory, "flag_off.json", &saved)?;
            let on = publish_json(&output_directory, "card_semantics.json", &enabled)?;
            let off_back = readback_v1(&source, &off, None)?;
            let on_back = readback_v1(&source, &on, Some(&card_semantics))?;
            ensure(
                same_tensors_v1(&saved, &off_back)
                    && same_tensors_v1(&enabled, &on_back)
                    && same_tensors_v1(&off_back, &on_back),
                "fork readback tensor identity differs",
            )?;
            Ok(
                json!({"flag_off":off, "card_semantics":on, "tensors_and_moments_identical":identical}),
            )
        }
    }
}

#[derive(Deserialize)]
struct ForkAdmissionV1 {
    #[serde(default, deserialize_with = "present_value_v1")]
    archive_distillation: Option<Value>,
    #[serde(default, deserialize_with = "present_value_v1")]
    e_supported: Option<Value>,
    #[serde(default, deserialize_with = "present_value_v1")]
    soft_q: Option<Value>,
    #[serde(default, deserialize_with = "present_value_v1")]
    search_targets: Option<Value>,
    #[serde(default, deserialize_with = "present_value_v1")]
    ex1: Option<Value>,
    #[serde(default, deserialize_with = "present_value_v1")]
    exploration: Option<Value>,
    #[serde(default, deserialize_with = "present_value_v1")]
    registry_transfer: Option<Value>,
    #[serde(default, deserialize_with = "present_value_v1")]
    input_contract: Option<Value>,
    #[serde(default, deserialize_with = "present_value_v1")]
    card_semantics: Option<Value>,
}
impl ForkAdmissionV1 {
    fn validate_v1(&self) -> Result<(), String> {
        ensure(self.archive_distillation.is_none()
            && self.e_supported.is_none()
            && self.soft_q.is_none()
            && self.search_targets.is_none()
            && self.ex1.is_none()
            && self.exploration.is_none()
            && self.registry_transfer.is_none()
            && self.input_contract.is_none()
            && self.card_semantics.is_none()
            , "fork-import requires an ordinary 33-tensor checkpoint without optional training sections")
    }
}

fn admit_fork_v1(saved: &ExpandedCheckpointV1) -> Result<(), String> {
    ensure(
        saved.card_semantics.is_none()
            && saved.archive_distillation.is_none()
            && saved.e_supported.is_none()
            && saved.soft_q.is_none()
            && saved.search_targets.is_none()
            && saved.ex1.is_none()
            && saved.exploration.is_none()
            && saved.registry_transfer.is_none()
            && saved.input_contract.is_none()
            && saved.parameters.len() == 33,
        "fork-import requires an ordinary 33-tensor checkpoint without optional training sections",
    )
}

fn same_tensors_v1(a: &ExpandedCheckpointV1, b: &ExpandedCheckpointV1) -> bool {
    a.parameters.len() == 33
        && b.parameters.len() == 33
        && a.parameters == b.parameters
        && a.first_moments == b.first_moments
        && a.second_moments == b.second_moments
        && a.adam_step == b.adam_step
}

fn readback_v1(
    source: &ExpandedModelSourceV1,
    pin: &PinnedFileV1,
    config: Option<&CardSemanticsConfigV1>,
) -> Result<ExpandedCheckpointV1, String> {
    let resumed = ExpandedModelSourceV1 {
        checkpoint: Some(pin.clone()),
        ..source.clone()
    };
    let (_, state) = initialize_any_math_v1(&resumed)?;
    load_training_v1(&resumed, config, state.loaded_card_semantics.as_ref())?;
    let saved = read_checkpoint_v1(pin)?;
    let snapshot = state.snapshot_v1().map_err(err)?;
    let parameters: Vec<_> = snapshot
        .parameters
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    let first: Vec<_> = snapshot
        .first_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    let second: Vec<_> = snapshot
        .second_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    ensure(
        parameters.len() == 33
            && parameters == saved.parameters
            && first == saved.first_moments
            && second == saved.second_moments
            && snapshot.adam_step == saved.adam_step,
        "normal checkpoint loader readback differs from published tensors",
    )?;
    Ok(saved)
}

/// Run preflight reuses the source's normal checkpoint load and source cache:
/// the run's initial load (`load_initial_for_run_v1`) passes the policy and
/// train state it loaded, with whether the source is a registry transfer.
pub(crate) fn admit_run_initial_v1(
    source: &ExpandedModelSourceV1,
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
    transfer: bool,
    config: Option<&CardSemanticsConfigV1>,
) -> Result<(), String> {
    if config.is_some() {
        ensure(
            !transfer
                && policy.input_contract_v1().is_none()
                && state.model_v1().parameter_snapshot_v1().len() == 33,
            "card semantics requires an ordinary 33-tensor learner",
        )?;
    }
    load_training_v1(source, config, state.loaded_card_semantics.as_ref())?;
    Ok(())
}

/// The run's initial load at a fresh start, as the run driver calls it.
#[cfg(test)]
pub(crate) fn load_run_initial_v1(
    source: &ExpandedModelSourceV1,
    math: TrainingMathV1,
    config: Option<&CardSemanticsConfigV1>,
) -> Result<ExpandedInferenceIdentityV1, String> {
    load_initial_for_run_v1(source, math, true, config).map(|loaded| loaded.identity)
}

/// Bind both the update receipt and the loaded checkpoint to the run. The
/// checkpoint section comes from the normal loader, including cache hits.
pub(crate) fn validate_binding_v1(
    section: Option<&CardSemanticsSectionV1>,
    binding: Option<&str>,
) -> Result<(), String> {
    ensure(
        section.map(|s| s.binding.as_str()) == binding
            && section.is_none_or(|s| s.schema == "card_semantics_e1"),
        "checkpoint card semantics binding differs from the run",
    )
}

pub(crate) fn validate_receipt_v1(document: &Value, binding: Option<&str>) -> Result<(), String> {
    let expected = binding.map(|binding| json!({"binding": binding}));
    ensure(
        document.get("card_semantics") == expected.as_ref(),
        "update card semantics binding differs from the run",
    )
}

#[cfg(test)]
pub(crate) fn checkpoint_test_bytes_v1(
    state: &NativePolicyValueTrainStateV1,
    semantics: Option<&CardSemanticsV1>,
) -> Vec<u8> {
    let (_, _, mut saved) = super::tests::checkpoint_fixture_v1();
    let snapshot = state.snapshot_v1().unwrap();
    saved.adam_step = snapshot.adam_step;
    saved.scorer_bias_anchor_bits = snapshot.scorer_bias_anchor_bits;
    saved.parameters = snapshot
        .parameters
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.first_moments = snapshot
        .first_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.second_moments = snapshot
        .second_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.card_semantics = semantics.map(CardSemanticsV1::section_v1);
    saved.state_sha256 = checkpoint_hash_v1(
        &snapshot.state_sha256_v1().unwrap(),
        saved.card_semantics.as_ref(),
    )
    .unwrap();
    serde_json::to_vec(&saved).unwrap()
}

#[cfg(test)]
pub(crate) fn restore_test_bytes_v1(
    bytes: &[u8],
    semantics: Option<CardSemanticsV1>,
) -> Result<(NativePolicyValueTrainStateV1, Option<CardSemanticsV1>), String> {
    let (mut policy, model, _) = super::tests::checkpoint_fixture_v1();
    let saved: ExpandedCheckpointV1 = serde_json::from_slice(bytes).map_err(err)?;
    let semantics = bind_training_v1(semantics, saved.card_semantics.as_ref(), true)?;
    let state = restore_checkpoint_fields_v1(&saved, &mut policy, model)?;
    Ok((state, semantics))
}

#[cfg(test)]
pub(crate) fn fixture_files_v1(
    label: &str,
) -> (
    PathBuf,
    ExpandedModelSourceV1,
    CardSemanticsConfigV1,
    ExpandedCheckpointV1,
) {
    let root = std::env::temp_dir().join(format!(
        "projection-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
    let descriptor = fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        identity,
        |_| {},
    );
    let source = ExpandedModelSourceV1 {
        play_import: publish_json(&root, "source.json", &descriptor).unwrap(),
        checkpoint: None,
        s_learner: None,
        w_learner: None,
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
        },
    };
    let (policy, state) = initialize_any_math_v1(&source).unwrap();
    let mut saved: ExpandedCheckpointV1 =
        serde_json::from_slice(&checkpoint_test_bytes_v1(&state, None)).unwrap();
    saved.schema = ordinary_checkpoint_schema_v1(policy.identity_v1()).into();
    saved.source_import = policy.identity_v1().clone();
    saved.feature_contract_digest = identity.feature_contract_digest.into();
    saved.feature_encoding_digest = identity.feature_encoding_digest.into();
    let config = fixture_table_v1(&root);
    (root, source, config, saved)
}

#[cfg(test)]
pub(crate) fn fixture_plain_checkpoint_v1(
    root: &Path,
    source: &ExpandedModelSourceV1,
) -> PinnedFileV1 {
    let (policy, state) = initialize_any_math_v1(source).unwrap();
    let mut saved: ExpandedCheckpointV1 =
        serde_json::from_slice(&checkpoint_test_bytes_v1(&state, None)).unwrap();
    saved.schema = ordinary_checkpoint_schema_v1(policy.identity_v1()).into();
    saved.source_import = policy.identity_v1().clone();
    let identity = policy.feature_identity_v1();
    saved.feature_contract_digest = identity.feature_contract_digest.into();
    saved.feature_encoding_digest = identity.feature_encoding_digest.into();
    publish_json(root, "plain-checkpoint.json", &saved).unwrap()
}

#[cfg(test)]
pub(crate) fn fixture_table_v1(root: &Path) -> CardSemanticsConfigV1 {
    let table = json!({"schema":"card_semantics_table/v1", "kernel_carddb_hash":format!("{KERNEL_CARDDB_HASH:016x}"),
        "vector_width":2, "card_count":2, "cards":[{"card_id":0,"vector":[2,1]},{"card_id":1,"vector":[-1,0]}]});
    let pin = publish_json(root, "table.json", &table).unwrap();
    CardSemanticsConfigV1 {
        table_path: pin.path,
        table_sha256: pin.sha256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_off_load_and_integrity_are_enforced() {
        let (_, model, _) = super::super::tests::checkpoint_fixture_v1();
        let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let semantics = CardSemanticsV1::fixture_v1();
        let bytes = checkpoint_test_bytes_v1(&state, Some(&semantics));
        assert!(restore_test_bytes_v1(&bytes, None).is_err());
        let absent = checkpoint_test_bytes_v1(&state, None);
        assert!(restore_test_bytes_v1(&absent, Some(semantics.clone())).is_err());
        let mut saved: ExpandedCheckpointV1 = serde_json::from_slice(&bytes).unwrap();
        saved.card_semantics.as_mut().unwrap().binding = "different".into();
        assert!(restore_test_bytes_v1(
            &serde_json::to_vec(&saved).unwrap(),
            Some(semantics.clone())
        )
        .unwrap_err()
        .contains("binding"));
        let mut saved: ExpandedCheckpointV1 = serde_json::from_slice(&bytes).unwrap();
        saved.card_semantics.as_mut().unwrap().p_step += 1;
        assert!(
            restore_test_bytes_v1(&serde_json::to_vec(&saved).unwrap(), Some(semantics))
                .unwrap_err()
                .contains("hash")
        );
    }
    #[test]
    fn other_learner_paths_refuse_configuration() {
        let config = CardSemanticsConfigV1 {
            table_path: PathBuf::new(),
            table_sha256: String::new(),
        };
        let source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: PathBuf::new(),
                sha256: String::new(),
            },
            checkpoint: None,
            s_learner: None,
            w_learner: None,
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            },
        };
        assert!(validate_update_v1(
            Some(&config),
            &source,
            ExpandedUpdateBackendV1::Cpu,
            &ExpandedLossSelectionV1::TerminalReinforceValueV3,
            false
        )
        .is_err());
        let gae = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 1.0,
            lambda: 1.0,
            entropy_coefficient: 0.0,
            pregame_advantage: PregameAdvantageV1::Gae,
        };
        assert!(validate_update_v1(
            Some(&config),
            &source,
            ExpandedUpdateBackendV1::Cpu,
            &gae,
            true
        )
        .is_err());
        assert!(validate_update_v1(
            Some(&config),
            &source,
            ExpandedUpdateBackendV1::Cuda { device_ordinal: 0 },
            &gae,
            false
        )
        .is_err());
    }
    fn write_saved(
        root: &Path,
        name: &str,
        source: &ExpandedModelSourceV1,
        saved: &mut ExpandedCheckpointV1,
    ) -> ExpandedModelSourceV1 {
        let (_, state) = initialize_any_math_v1(source).unwrap();
        let hash = checkpoint_tensors_v1(saved, state.model_v1())
            .unwrap()
            .state_sha256_v1()
            .unwrap();
        saved.state_sha256 = checkpoint_hash_v1(&hash, saved.card_semantics.as_ref()).unwrap();
        ExpandedModelSourceV1 {
            checkpoint: Some(publish_json(root, name, saved).unwrap()),
            ..source.clone()
        }
    }

    #[test]
    fn tools_export_and_fork_read_back_exact_tensors_moments_and_age() {
        let (root, initial, config, mut saved) = fixture_files_v1("tools");
        saved.adam_step = 17;
        saved.parameters[0].values[16] = 0.25f32.to_bits();
        saved.first_moments[0].values[16] = 0.5f32.to_bits();
        saved.second_moments[0].values[16] = 0.25f32.to_bits();
        let source = write_saved(&root, "parent.json", &initial, &mut saved);
        let fork = tool_json_v1(
            &json!({"mode":"fork-import", "source":source,
            "initial_source":initial, "tokens":[1], "card_semantics":config,
            "output_directory":root.join("fork")})
            .to_string(),
        )
        .unwrap();
        let off: PinnedFileV1 = serde_json::from_value(fork["flag_off"].clone()).unwrap();
        let on: PinnedFileV1 = serde_json::from_value(fork["card_semantics"].clone()).unwrap();
        let a = readback_v1(&initial, &off, None).unwrap();
        let b = readback_v1(&initial, &on, Some(&config)).unwrap();
        assert!(same_tensors_v1(&a, &b));
        assert_eq!(a.adam_step, 17);
        let (_, initial_state) = initialize_any_math_v1(&initial).unwrap();
        let expected = initial_state.snapshot_v1().unwrap();
        assert_eq!(
            &a.parameters[0].values[16..32],
            &bits(&expected.parameters[0].values[16..32])
        );
        assert_eq!(&a.first_moments[0].values[16..32], &[0; 16]);
        assert_eq!(&a.second_moments[0].values[16..32], &[0; 16]);
        let section = b.card_semantics.as_ref().unwrap();
        assert_eq!(section.p_step, 0);
        assert!(section
            .weight
            .iter()
            .chain(&section.first_moments)
            .chain(&section.second_moments)
            .all(|&v| v == 0));
        let mut export_checkpoint = b.clone();
        let section = export_checkpoint.card_semantics.as_mut().unwrap();
        section.weight[0] = 0.125f32.to_bits();
        section.p_step = 3;
        export_checkpoint.parameters[0].values[16] =
            (f32::from_bits(export_checkpoint.parameters[0].values[16]) + 0.125).to_bits();
        let enabled = write_saved(&root, "export-input.json", &initial, &mut export_checkpoint);
        let export = tool_json_v1(
            &json!({"mode":"export", "source":enabled,
            "output_directory":root.join("export")})
            .to_string(),
        )
        .unwrap();
        let pin: PinnedFileV1 = serde_json::from_value(export["checkpoint"].clone()).unwrap();
        let exported = readback_v1(&initial, &pin, None).unwrap();
        assert!(same_tensors_v1(&exported, &export_checkpoint));
        assert!(exported.card_semantics.is_none());
        // Container metadata has empty tensor lists. The ordinary tensor
        // count must come from its full decode, not from the metadata probe.
        checkpoint_container_v1::admit_writer_v1(&saved).unwrap();
        let encoded = checkpoint_container_v1::encode_v1(&saved).unwrap();
        let container = publish_checkpoint_file_v1(&root.join("parent.bin"), &encoded).unwrap();
        let container_source = ExpandedModelSourceV1 {
            checkpoint: Some(container),
            ..initial.clone()
        };
        let fork_v2 = tool_json_v1(
            &json!({"mode":"fork-import", "source":container_source,
            "initial_source":initial,"tokens":[1],"card_semantics":config,
            "output_directory":root.join("fork-v2")})
            .to_string(),
        )
        .unwrap();
        assert_eq!(fork_v2["flag_off"]["sha256"], fork["flag_off"]["sha256"]);
        assert_eq!(
            fork_v2["card_semantics"]["sha256"],
            fork["card_semantics"]["sha256"]
        );
        assert!(tool_json_v1(
            &json!({"mode":"fork-import", "source":source,
            "initial_source":initial,"tokens":[1],"card_semantics":null,
            "output_directory":root.join("null-config")})
            .to_string()
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fork_refuses_every_optional_training_section_and_flat_v8() {
        let (root, initial, config, saved) = fixture_files_v1("fork-refusals");
        for field in [
            "archive_distillation",
            "e_supported",
            "soft_q",
            "search_targets",
            "ex1",
            "exploration",
            "registry_transfer",
            "input_contract",
            "card_semantics",
            "flat_v8",
            "card_semantics_null",
        ] {
            let mut document = serde_json::to_value(&saved).unwrap();
            if field == "flat_v8" {
                let tensors = document["parameters"].as_array_mut().unwrap();
                for _ in 0..4 {
                    tensors.push(tensors[0].clone());
                }
            } else if field == "card_semantics_null" {
                document["card_semantics"] = Value::Null;
            } else {
                // Admission must precede following any unsupported section's
                // external references or trying to interpret its schema.
                document[field] = json!({"unsupported":true});
            }
            let pin = publish_json(&root, &format!("{field}.json"), &document).unwrap();
            let source = ExpandedModelSourceV1 {
                checkpoint: Some(pin),
                ..initial.clone()
            };
            let output = root.join(format!("out-{field}"));
            let error = tool_json_v1(
                &json!({"mode":"fork-import", "source":source,
                "initial_source":initial, "tokens":[1], "card_semantics":config,
                "output_directory":output})
                .to_string(),
            )
            .unwrap_err();
            assert!(error.contains("ordinary 33-tensor"), "{field}: {error}");
            assert!(!output.exists());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn production_loaders_refuse_missing_off_binding_and_schema_even_on_cache_hits() {
        let (root, initial, config, mut saved) = fixture_files_v1("admission");
        let plain = write_saved(&root, "plain.json", &initial, &mut saved);
        let error = load_run_initial_v1(&plain, TrainingMathV1::PlatformLibm, Some(&config))
            .err()
            .unwrap();
        assert!(error.contains("section missing"));
        saved.card_semantics = Some(config.load_v1().unwrap().section_v1());
        let enabled = write_saved(&root, "enabled.json", &initial, &mut saved);
        for _ in 0..2 {
            load_run_initial_v1(&enabled, TrainingMathV1::PlatformLibm, Some(&config)).unwrap();
            assert!(
                load_run_initial_v1(&enabled, TrainingMathV1::PlatformLibm, None)
                    .err()
                    .unwrap()
                    .contains("requires")
            );
            assert!(initialize(&enabled)
                .err()
                .unwrap()
                .contains("card semantics"));
        }
        assert!(load_ordinary_bo3_parent_v1(&enabled)
            .err()
            .unwrap()
            .contains("BO3 transition refuses card semantics"));
        assert!(weights_snapshot_v1::weights_snapshot_v1(
            enabled.checkpoint.as_ref().unwrap(),
            &root.join("weights.bin")
        )
        .unwrap_err()
        .contains("weights snapshot refuses card semantics"));
        assert!(checkpoint_container_v1::admit_writer_v1(&saved)
            .unwrap_err()
            .contains("v2"));
        for field in ["binding", "schema"] {
            let mut invalid = saved.clone();
            let section = invalid.card_semantics.as_mut().unwrap();
            if field == "binding" {
                section.binding = "wrong".into();
            } else {
                section.schema = "wrong".into();
            }
            let source = write_saved(&root, &format!("{field}.json"), &initial, &mut invalid);
            assert!(
                load_run_initial_v1(&source, TrainingMathV1::PlatformLibm, Some(&config))
                    .err()
                    .unwrap()
                    .contains("binding")
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovered_update_binds_both_receipt_and_checkpoint_to_run() {
        let (root, source, config, mut saved) = fixture_files_v1("recovery");
        let (_, before) =
            load_expanded_inference_bound_v1(&source, TrainingMathV1::PlatformLibm).unwrap();
        saved.adam_step = 1;
        saved.trajectories.clear();
        let section = config.load_v1().unwrap().section_v1();
        // The receipt always matches the declared run. Substitution of a
        // valid, correctly hashed checkpoint must still fail admission.
        for (case, run_binding, checkpoint_binding) in [
            (
                "on-ok",
                Some(section.binding.as_str()),
                Some(section.binding.as_str()),
            ),
            ("off-ok", None, None),
            ("missing", Some(section.binding.as_str()), None),
            ("unexpected", None, Some(section.binding.as_str())),
            ("wrong", Some(section.binding.as_str()), Some("different")),
            (
                "schema",
                Some(section.binding.as_str()),
                Some(section.binding.as_str()),
            ),
        ] {
            saved.card_semantics = checkpoint_binding.map(|binding| CardSemanticsSectionV1 {
                binding: binding.into(),
                ..section.clone()
            });
            if case == "schema" {
                saved.card_semantics.as_mut().unwrap().schema = "wrong".into();
            }
            let next = write_saved(&root, &format!("cp-{case}.json"), &source, &mut saved);
            let (_, after) =
                load_expanded_inference_bound_v1(&next, TrainingMathV1::PlatformLibm).unwrap();
            let mut document = json!({"schema":"mtg-kernel-expanded-deck-update/v1", "complete":true, "source":source, "trajectories":[],
                "checkpoint":next.checkpoint, "before_state_sha256":before.state_sha256,
                "after_state_sha256":after.state_sha256,"adam_step":1,"numerical_backend":"native-cpu-sequential"});
            if let Some(binding) = run_binding {
                document["card_semantics"] = json!({"binding":binding});
            }
            let pin = publish_json(&root, &format!("update-{case}.json"), &document).unwrap();
            let validate = |pin: &PinnedFileV1| {
                crate::native_expanded_training_run_v1::validate_update(
                    pin,
                    &source,
                    &[],
                    &before,
                    f32::from_bits(saved.learning_rate_bits),
                    f32::from_bits(saved.value_coefficient_bits),
                    ExpandedUpdateBackendV1::Cpu,
                    1,
                    DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
                    1,
                    None,
                    None,
                    None,
                    None,
                    None,
                    TrainingMathV1::PlatformLibm,
                    None,
                    &ExpandedLossSelectionV1::TerminalReinforceValueV3,
                    run_binding,
                )
            };
            let result = validate(&pin);
            if case.ends_with("ok") {
                result.unwrap();
            } else {
                assert!(
                    result.err().unwrap().contains("card semantics binding"),
                    "{case}"
                );
            }
            for (label, invalid) in [("null", Value::Null), ("wrong", json!({"binding":"wrong"}))] {
                document["card_semantics"] = invalid;
                let invalid_pin =
                    publish_json(&root, &format!("receipt-{case}-{label}.json"), &document)
                        .unwrap();
                assert!(validate(&invalid_pin)
                    .err()
                    .unwrap()
                    .contains("update card semantics binding"));
            }
            if run_binding.is_some() {
                document.as_object_mut().unwrap().remove("card_semantics");
                let missing =
                    publish_json(&root, &format!("receipt-{case}-missing.json"), &document)
                        .unwrap();
                assert!(validate(&missing)
                    .err()
                    .unwrap()
                    .contains("update card semantics binding"));
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    /// The loaded-source cache serves update validation an identity without
    /// a train state. Its entry keeps the section, so a hit still checks it.
    #[test]
    fn update_validation_checks_the_section_on_an_identity_only_cache_hit() {
        let (root, initial, config, mut saved) = fixture_files_v1("identity-hit");
        let section = config.load_v1().unwrap().section_v1();
        saved.card_semantics = Some(section.clone());
        let enabled = write_saved(&root, "enabled.json", &initial, &mut saved);
        // An inference load keeps a policy-only entry: no train state.
        load_expanded_inference_bound_v1(&enabled, TrainingMathV1::PlatformLibm).unwrap();
        let load = |binding: Option<&str>| {
            let hits = loaded_source_cache_v1::matches_for_test_v1();
            let result = load_expanded_update_bound_projection_v1(
                &enabled,
                &saved.trajectories,
                f32::from_bits(saved.learning_rate_bits),
                f32::from_bits(saved.value_coefficient_bits),
                binding,
            );
            assert_eq!(loaded_source_cache_v1::matches_for_test_v1(), hits + 1);
            result
        };
        load(Some(&section.binding)).unwrap();
        for binding in [None, Some("different")] {
            assert!(load(binding)
                .unwrap_err()
                .contains("card semantics binding"));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_and_probe_refuse_explicit_null() {
        let (_, _, saved) = super::super::tests::checkpoint_fixture_v1();
        let mut document = serde_json::to_value(saved).unwrap();
        document["card_semantics"] = Value::Null;
        assert!(
            serde_json::from_value::<ExpandedCheckpointV1>(document.clone())
                .unwrap_err()
                .to_string()
                .contains("must not be null")
        );
        assert!(
            serde_json::from_value::<CheckpointESupportedProbeV1>(document)
                .err()
                .unwrap()
                .to_string()
                .contains("must not be null")
        );
    }
    #[test]
    fn flag_off_admission_reuses_loaded_checkpoint_without_metadata_probe() {
        let (root, source, _, _) = fixture_files_v1("off-reads");
        let source = ExpandedModelSourceV1 {
            checkpoint: Some(fixture_plain_checkpoint_v1(&root, &source)),
            ..source
        };
        for _ in 0..2 {
            take_checkpoint_metadata_reads_for_test_v1();
            initialize(&source).unwrap();
            assert_eq!(take_checkpoint_metadata_reads_for_test_v1(), 0);
        }
        fs::remove_dir_all(root).unwrap();
    }
}
