//! Distinct per-seat inference and sideboarding. This path never passes
//! through the legacy CLI's single-model setup or output schema.

use super::*;
use mtg_kernel::expanded_deck_training_v1::ExpandedInferenceIdentityV1;
use mtg_kernel::learned_bo3_v1::run_population_bo3_v1;

fn embeddings_for_seat<'a>(
    values: &'a [f32],
    receipt: &ExpandedInferenceIdentityV1,
) -> Result<FrozenSideboardEmbeddingsV1<'a>, String> {
    FrozenSideboardEmbeddingsV1::new_v1(
        values,
        SideboardPlayIdentityV1 {
            weights_sha256: receipt.model.weights_sha256.clone(),
            git_head: receipt.source_import.origin_git_commit_v1().to_owned(),
        },
    )
    .map_err(|error| error.to_string())
}

/// All head/registration/teacher combinations are validated before any model
/// reset or episode. Each head receives its own physical seat's embedding table.
fn preflight_policies_v1(
    prepared: &[PreparedPolicyV1; 2],
    matches: &[ExpandedBo3MatchV1],
    registrations: &[[RegisteredDeckV1; 2]],
    embeddings: &[FrozenSideboardEmbeddingsV1<'_>; 2],
) -> Result<(), String> {
    if matches.len() != registrations.len() {
        return Err("population registration count differs from matches".into());
    }
    for (item, registered) in matches.iter().zip(registrations) {
        for seat in 0..2 {
            let _ =
                prepared[seat].bind_explicit(&item.config, registered, seat, &embeddings[seat])?;
        }
    }
    Ok(())
}

pub(super) fn run_population_command_v1(
    config_path: &Path,
    config_bytes: &[u8],
    command: &CommandV1,
) -> Result<(), String> {
    let CommandV1::RunPopulationBatch {
        model_sources,
        output_directory: output,
        policies,
        matches,
    } = command
    else {
        return Err("expected population BO3 command".into());
    };
    absolute(output)?;
    if matches.is_empty() || matches.len() > 1024 {
        return Err("match count must be 1..1024".into());
    }
    let (p0, i0) = load_expanded_inference_v1(&model_sources[0])
        .map_err(|error| format!("seat 0 model: {error}"))?;
    let (p1, i1) = load_expanded_inference_v1(&model_sources[1])
        .map_err(|error| format!("seat 1 model: {error}"))?;
    let mut play = [p0, p1];
    let identities = [i0, i1];
    let values = play
        .each_ref()
        .map(|policy| policy.embedding_rows_v1().to_vec());
    let embeddings = [
        embeddings_for_seat(&values[0], &identities[0])?,
        embeddings_for_seat(&values[1], &identities[1])?,
    ];
    let mut inputs = vec![file_receipt(config_path, config_bytes)];
    for seat in 0..2 {
        inputs.push(
            json!({"seat":seat, "path":model_sources[seat].play_import.path,
            "sha256":model_sources[seat].play_import.sha256,
            "validated_by":"load_expanded_inference_v1"}),
        );
        if let Some(pin) = &model_sources[seat].checkpoint {
            inputs.push(json!({"seat":seat,"path":pin.path,
                "sha256":identities[seat].checkpoint_sha256,
                "validated_by":"load_expanded_inference_v1"}));
        }
    }
    let prepared = [
        prepare_policy(&policies[0], &mut inputs)?,
        prepare_policy(&policies[1], &mut inputs)?,
    ];
    let registrations = matches
        .iter()
        .map(ExpandedBo3MatchV1::registrations)
        .collect::<Result<Vec<_>, _>>()?;
    preflight_policies_v1(&prepared, matches, &registrations, &embeddings)?;
    let policy_identities = [
        hash(&serde_json::to_vec(&policies[0]).map_err(|error| error.to_string())?),
        hash(&serde_json::to_vec(&policies[1]).map_err(|error| error.to_string())?),
    ];
    let tags = checked_in_tags()?;
    fs::create_dir(output)
        .map_err(|error| format!("fresh output directory {}: {error}", output.display()))?;
    write_new(&output.join("config.json"), config_bytes)?;
    write_json(&output.join("play-models.json"), &identities)?;
    write_json(&output.join("inputs.json"), &inputs)?;
    write_json(
        &output.join("run-start.json"),
        &json!({
            "schema":"kernel-population-sideboard-cli-start/v1", "command":command,
            "inputs":inputs, "binary":binary_receipt()?, "compiled_sources":compiled_sources(),
            "build":{"git_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
                "git_clean":env!("MTG_KERNEL_BUILD_GIT_CLEAN"),
                "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
                "toolchain_pin":include_str!("../../../../rust-toolchain.toml")},
            "play_models":identities, "sideboard_policy_identities":policy_identities,
            "execution":{"device":"cpu","gpu_ordinal":null}, "tag_file_sha256":hash(TAG_BYTES),
            "nonclaims":["per-seat inference engineering; no strength or promotion claim", "no training performed"]
        }),
    )?;
    let execution = (|| -> Result<Value, String> {
        let mut total_games = 0usize;
        for (index, (item, registered)) in matches.iter().zip(registrations).enumerate() {
            let mut s0 = prepared[0].bind_explicit(&item.config, &registered, 0, &embeddings[0])?;
            let mut s1 = prepared[1].bind_explicit(&item.config, &registered, 1, &embeddings[1])?;
            let result = run_population_bo3_v1(
                item.config.clone(),
                registered,
                identities.clone(),
                policy_identities.clone(),
                &tags,
                &mut play,
                [&mut s0, &mut s1],
            )?;
            total_games += result.games.len();
            write_json(&output.join(format!("match-{index:06}.json")), &result)?;
            println!(
                "{}",
                json!({"completed_match":index, "physical_games":result.games.len(),
                "artifact":format!("match-{index:06}.json")})
            );
        }
        Ok(
            json!({"mode":"run_population_batch", "completed_matches":matches.len(),
            "physical_games":total_games, "inputs":inputs, "play_models":identities,
            "sideboard_policy_identities":policy_identities,
            "no_training_performed":true, "strength_claim":false}),
        )
    })();
    match execution {
        Ok(completion) => {
            write_json(&output.join("completion.json"), &completion)?;
            println!(
                "{}",
                json!({"status":"completed", "output_directory":output,"completion":completion})
            );
            Ok(())
        }
        Err(error) => {
            write_json(
                &output.join("failure.json"),
                &json!({"schema":"kernel-population-sideboard-cli-failure/v1",
                "error":error,"inputs":inputs,"play_models":identities}),
            )?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn population_preflight_binds_each_head_to_its_own_seat_model() {
        let mut values = [vec![0.0; 65537 * 16], vec![0.0; 65537 * 16]];
        values[1][16] = 0.5;
        let embeddings = [
            FrozenSideboardEmbeddingsV1::new_v1(
                &values[0],
                SideboardPlayIdentityV1 {
                    weights_sha256: "a".repeat(64),
                    git_head: "b".repeat(40),
                },
            )
            .unwrap(),
            FrozenSideboardEmbeddingsV1::new_v1(
                &values[1],
                SideboardPlayIdentityV1 {
                    weights_sha256: "c".repeat(64),
                    git_head: "b".repeat(40),
                },
            )
            .unwrap(),
        ];
        let registrations =
            [["Rally", "Burn"].map(|id| checked_in_pauper_registered_deck_by_id_v1(id).unwrap())];
        let matches = [ExpandedBo3MatchV1 {
            config: LearnedBo3RunConfigV1 {
                deck_ids: ["Rally".into(), "Burn".into()],
                seed: 5,
                game_one_chooser: mtg_kernel::ids::PlayerId::P0,
                max_physical_games: 3,
                max_physical_decisions: 4000,
                max_policy_steps: 40000,
                opening_protocol: Default::default(),
            },
            registered: registrations[0].each_ref().map(|deck| ExpandedDeckListV1 {
                label: deck.deck_id().to_owned(),
                mainboard: deck.registered_configuration().mainboard().to_vec(),
                sideboard: deck.registered_configuration().sideboard().to_vec(),
            }),
        }];
        let heads = [
            LearnedSideboardModelV1::new_v1(1, &embeddings[0]),
            LearnedSideboardModelV1::new_v1(2, &embeddings[1]),
        ];
        let good = heads.clone().map(PreparedPolicyV1::Learned);
        assert!(preflight_policies_v1(&good, &matches, &registrations, &embeddings).is_ok());
        for wrong_seat in 0..2 {
            let mut wrong = heads.clone();
            wrong[wrong_seat] = heads[1 - wrong_seat].clone();
            let error = preflight_policies_v1(
                &wrong.map(PreparedPolicyV1::Learned),
                &matches,
                &registrations,
                &embeddings,
            )
            .unwrap_err();
            assert!(error.contains(&format!("seat {wrong_seat} learned checkpoint preflight")));
        }
    }

    #[test]
    fn population_command_requires_exactly_two_strict_model_sources() {
        let source = json!({"play_import":{"path":"C:/import.json","sha256":"a".repeat(64)},
            "feature_transfer":{"expected_feature_contract_digest":"b".repeat(64),
                "expected_feature_encoding_digest":"c".repeat(64)},"checkpoint":null});
        let mut config = json!({"mode":"run_population_batch", "model_sources":[source.clone(),source],
            "output_directory":"C:/new", "policies":[{"kind":"keep"},{"kind":"keep"}], "matches":[]});
        assert!(serde_json::from_value::<CommandV1>(config.clone()).is_ok());
        config["model_sources"].as_array_mut().unwrap().pop();
        assert!(serde_json::from_value::<CommandV1>(config).is_err());
    }
}
