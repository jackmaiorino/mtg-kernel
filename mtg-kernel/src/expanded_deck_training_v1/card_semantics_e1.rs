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
                && matches!(backend, ExpandedUpdateBackendV1::Cpu)
                && matches!(
                    loss,
                    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                        pregame_announcement_step_scale: None,
                        ..
                    }
                )
                && !auxiliary,
            "card semantics requires ordinary net8 CPU GAE",
        )?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct SectionProbeV1 {
    #[serde(default)]
    card_semantics: Option<CardSemanticsSectionV1>,
}

pub(super) fn load_training_v1(
    source: &ExpandedModelSourceV1,
    config: Option<&CardSemanticsConfigV1>,
) -> Result<Option<CardSemanticsV1>, String> {
    let saved = source
        .checkpoint
        .as_ref()
        .map(|pin| read_checkpoint_metadata_v1::<SectionProbeV1>(pin).map(|s| s.card_semantics))
        .transpose()?
        .flatten();
    bind_training_v1(
        config.map(CardSemanticsConfigV1::load_v1).transpose()?,
        saved.as_ref(),
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
            Ok(json!({"checkpoint":output}))
        }
        ToolV1::ForkImport {
            source,
            initial_source,
            tokens,
            card_semantics,
            output_directory,
        } => {
            load_training_v1(&source, None)?;
            let (_, state) = initialize_any_math_v1(&source)?;
            let pin = source
                .checkpoint
                .as_ref()
                .ok_or("fork-import requires checkpoint")?;
            let mut saved = read_checkpoint_v1(pin)?;
            ensure(
                initial_source.checkpoint.is_none() && initial_source.s_learner.is_none(),
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
            Ok(
                json!({"flag_off":off, "card_semantics":on, "tensors_and_moments_identical":identical}),
            )
        }
    }
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
            pregame_announcement_step_scale: None,
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
}
