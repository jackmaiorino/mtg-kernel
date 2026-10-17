use super::*;
use crate::native_flat_tensorizer_v4::encoded_decision_view_v4;
use crate::native_policy_value_net_v1::own_registration::{
    NativeRegistrationNetV1, RegistrationInputWeightsV1,
};

/// Exact scalar reconstruction from saved ordinary training rows, followed
/// by three CPU forwards under the pinned parent. No games or device use.
#[test]
#[ignore = "parent-guarded packet preparation: reads pinned ordinary artifacts and writes fresh scratch output"]
fn prepare_saved_three_row_registration_gpu_packet() {
    let output = PathBuf::from(std::env::var("MTG_OWN_REGISTRATION_PACKET_DIRECTORY").unwrap());
    assert!(
        output.is_absolute() && !output.exists(),
        "requires fresh absolute output directory"
    );
    let parent = PinnedFileV1 {
        path: "E:/nine-deck-baseline-20261007/computehost-campaign/checkpoints/r1/block-20.json"
            .into(),
        sha256: "5c104b0d5bb532568549e9b2bdf6f787d32e51652b7154a148ffc94213c1a50b".into(),
    };
    let source_directory = Path::new("E:/nine-deck-baseline-20261007/computehost-campaign/retained/r1/b20/iterations/000001/attempt-000000/collect");
    let source_hashes = [
        "a1f6aceee53557cfe89a5f869ddfd936c0dc8d871c7b24ba1a3c6e2bdee093d9",
        "7cdd02a63d219a0bc47f0982d6c93c2744ab65f07e62e2151862331eb926f96b",
        "3d903651c30c15bb4529d22067dc52718fd5a466c83d218016b511e7a4d6b90a",
        "1b1e24900055d8f1f6566929b2eacbd02512ebb6f77b40bfc186828e75655dca",
        "7c6e744cf6a822a845f104a02432cf70e3b0b535221ee9d2bae17a49e61e29dc",
        "1ed3e015f9f32de0a7b1ee257b7689750ec2fb6a9b567f4dea315b9d95f62d38",
        "bc1989a98c760cf1ac2f652c329922e8d695e5ab281721a54374cf0b1cd237d0",
        "90f8f27e962c9107d9a87dad81150c7662e15bbdc455d01696f78a211065b319",
        "cec499b2cbbe061ce9d8d58f6ef10d75da73d96301af7e04da7de443d1ab7abb",
        "47d7eb2015227c9a853c499fade802e7b710668458163b945c53b9af1d797900",
    ];
    let source_pins: Vec<_> = source_hashes
        .iter()
        .enumerate()
        .map(|(index, hash)| PinnedFileV1 {
            path: source_directory.join(format!("episode-{index:04}.json")),
            sha256: (*hash).into(),
        })
        .collect();
    let episodes: Vec<ExpandedTrajectoryV1> = source_pins
        .iter()
        .map(|pin| {
            let trajectory: ExpandedTrajectoryV1 = read_pinned(pin).unwrap();
            validate_trajectory(&trajectory).unwrap();
            assert_eq!(
                trajectory.feature_contract_digest,
                FEATURE_CONTRACT_DIGEST_V4
            );
            assert_eq!(
                trajectory.feature_encoding_digest,
                FEATURE_ENCODING_DIGEST_V4
            );
            assert!(trajectory.training_math.is_platform_libm());
            trajectory
        })
        .collect();
    let saved: ExpandedCheckpointV1 = read_pinned(&parent).unwrap();
    assert_eq!(saved.schema, FRESH_CHECKPOINT_SCHEMA);
    identity_valid(
        &saved.feature_contract_digest,
        &saved.feature_encoding_digest,
        &saved.card_db_hash,
    )
    .unwrap();
    assert_eq!(saved.feature_contract_digest, FEATURE_CONTRACT_DIGEST_V4);
    assert_eq!(saved.feature_encoding_digest, FEATURE_ENCODING_DIGEST_V4);
    assert_eq!(saved.loss_identity, GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1);
    assert_eq!(saved.gamma_bits, Some(1.0f32.to_bits()));
    assert_eq!(saved.gae_lambda_bits, Some(0.9f32.to_bits()));
    assert_eq!(saved.entropy_coefficient_bits, Some(0.0f32.to_bits()));
    assert_eq!(saved.learning_rate_bits, 0.0001f32.to_bits());
    assert_eq!(saved.value_coefficient_bits, 0.5f32.to_bits());
    assert!(saved.training_math.is_platform_libm());
    assert_eq!(saved.adam_step, 35640);
    let mut base =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let template = base.parameter_snapshot_v1();
    let legacy = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters: restore_parameters(&saved.parameters, &template).unwrap(),
        first_moments: restore_parameters(&saved.first_moments, &template).unwrap(),
        second_moments: restore_parameters(&saved.second_moments, &template).unwrap(),
    };
    assert_eq!(hex(&legacy.state_sha256_v1().unwrap()), saved.state_sha256);
    base.replace_parameter_snapshot_v1(&legacy.parameters)
        .unwrap();
    let model = NativeRegistrationNetV1::new(
        base,
        RegistrationInputWeightsV1::zero(),
        RegistrationModeV1::Own,
    )
    .unwrap();

    // Match execute_update_v1's original learner-only grouping and tensor
    // wrappers. Validation above checks complete groups and sampler replay;
    // no old-policy forward is needed to reconstruct saved GAE scalars.
    let mut tensor_groups: Vec<LearnerTensorGroupV1<'_>> = Vec::new();
    let mut metadata = Vec::new();
    for (episode_ordinal, trajectory) in episodes.iter().enumerate() {
        let mut index = 0;
        let mut learner_ordinal = 0;
        while index < trajectory.decisions.len() {
            let first = &trajectory.decisions[index];
            let end = index + first.substep_count as usize;
            if first.actor == trajectory.episode.learner_seat {
                let rows = trajectory.decisions[index..end]
                    .iter()
                    .map(|row| (row, row.tensor.tensor()))
                    .collect();
                tensor_groups.push((
                    trajectory.terminal.terminal_reward[first.actor as usize] as i8,
                    episode_ordinal,
                    rows,
                ));
                metadata.push((episode_ordinal, learner_ordinal, index, end));
                learner_ordinal += 1;
            }
            index = end;
        }
    }
    let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = tensor_groups
        .iter()
        .map(|(_, _, rows)| {
            rows.iter()
                .map(|(row, tensor)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        encoded_decision_view_generic_v1(tensor, FreshLineageGenerationV1::V4),
                    )),
                    selected_action_index: row.selected as usize,
                    expected_raw_action_logit_bits: &row.logits,
                    expected_value_bits: row.value,
                })
                .collect()
        })
        .collect();
    let groups: Vec<_> = substeps
        .iter()
        .zip(&tensor_groups)
        .map(
            |(substeps, (terminal_return, _, _))| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: *terminal_return,
                baseline_bits: 0,
            },
        )
        .collect();
    let ordinals: Vec<_> = tensor_groups
        .iter()
        .map(|(_, episode, _)| *episode)
        .collect();
    let gae = compute_gae_targets_v1(&groups, &ordinals, 1.0, 0.9).unwrap();
    assert_eq!(gae.value_targets.len(), metadata.len());
    assert_eq!(gae.advantages.len(), metadata.len());

    let chosen = [(6usize, 0usize), (6usize, 8usize)];
    let mut samples = Vec::new();
    let mut selected = Vec::new();
    let mut group_indices = Vec::new();
    let mut first_substeps = Vec::new();
    let mut targets = Vec::new();
    let mut advantages = Vec::new();
    let mut selection_provenance = Vec::new();
    for (packet_group, &(episode_ordinal, learner_ordinal)) in chosen.iter().enumerate() {
        let global = metadata
            .iter()
            .position(|(episode, learner, _, _)| {
                *episode == episode_ordinal && *learner == learner_ordinal
            })
            .unwrap();
        let (_, _, start, end) = metadata[global];
        let trajectory = &episodes[episode_ordinal];
        let configurations = trajectory.episode.configurations().unwrap();
        first_substeps.push(samples.len());
        targets.push(gae.value_targets[global]);
        advantages.push(gae.advantages[global]);
        selection_provenance.push(json!({
            "packet_group":packet_group,"source_episode_ordinal":episode_ordinal,
            "source_learner_group_ordinal":learner_ordinal,"source_global_group_ordinal":global,
            "source_decision_range":[start,end],
            "source_behavior_state_sha256":trajectory.behavior_state_sha256,
        }));
        for row in &trajectory.decisions[start..end] {
            assert_eq!(row.actor, trajectory.episode.learner_seat);
            let selected_configuration = &configurations[row.actor as usize];
            let expected = expectation(&trajectory.episode, &configurations, row.actor).unwrap();
            let context = KnownOwnMainboardV1::new(
                &trajectory.episode.id,
                actor_seat(row.actor).unwrap(),
                selected_configuration,
            )
            .unwrap();
            let rows = context.rows(RegistrationModeV1::Own).unwrap();
            rows.validate_expected(RegistrationModeV1::Own, expected)
                .unwrap();
            let tensor = NativeFlatDecisionTensorV4 {
                common: row.tensor.tensor(),
            };
            let scores = model
                .forward_rows(encoded_decision_view_v4(&tensor), &rows, expected)
                .unwrap();
            let native = json!({
                "state":tensor.common.state,"object_features":tensor.common.object_features,
                "object_card_ids":tensor.common.object_card_ids,"object_groups":tensor.common.object_groups,
                "object_node_ids":tensor.common.object_node_ids,"edge_features":tensor.common.edge_features,
                "edge_source_indices":tensor.common.edge_source_indices,"edge_target_indices":tensor.common.edge_target_indices,
                "action_features":tensor.common.action_features,"action_ref_features":tensor.common.action_ref_features,
                "action_ref_card_ids":tensor.common.action_ref_card_ids,
                "action_ref_action_indices":tensor.common.action_ref_action_indices,
                "action_ref_node_indices":tensor.common.action_ref_node_indices,
            });
            samples.push(json!({
                "native":native,"registration":rows,
                "expected":{
                    "external_episode_id":trajectory.episode.id,"actor":row.actor,
                    "selected_mainboard":selected_configuration.mainboard(),
                    "selected_sideboard":selected_configuration.sideboard(),
                },
                "cpu_logits_bits":bits(&scores.logits),"cpu_value_bits":scores.value.to_bits(),
            }));
            selected.push(row.selected as usize);
            group_indices.push(packet_group);
        }
    }
    assert_eq!(samples.len(), 3);
    assert_eq!(selected, [0, 0, 1]);
    assert_eq!(group_indices, [0, 1, 1]);
    assert_eq!(first_substeps, [0, 1]);
    assert_eq!(
        metadata
            .iter()
            .filter(|(episode, _, _, _)| *episode == 6)
            .count(),
        29
    );
    assert_eq!(episodes[6].decisions[3].step, 3);
    assert_eq!(episodes[6].decisions[26].physical_decision_id, 26);
    assert_eq!(episodes[6].decisions[27].physical_decision_id, 26);
    let packet = json!({
        "schema":"own-registration-cuda-saved-batch/v2","samples":samples,
        "selected":selected,"group_indices":group_indices,"first_substeps":first_substeps,
        "targets":targets,"advantages":advantages,"max_chunk_substeps":2,
    });
    fs::create_dir(&output).unwrap();
    let packet_pin = publish_json(&output, "gpu-batch.json", &packet).unwrap();
    let manifest = json!({
        "schema":"own-registration-cuda-correctness/v1","gpu_ordinal":1,
        "checkpoint":parent.path,"checkpoint_sha256":parent.sha256,
        "batch":packet_pin.path,"batch_sha256":packet_pin.sha256,
    });
    let manifest_pin = publish_json(&output, "gpu-manifest.json", &manifest).unwrap();
    let provenance = json!({
        "schema":"own-registration-correctness-packet-provenance/v1",
        "parent":parent,"source_trajectories":source_pins,
        "packet":packet_pin,"gpu_manifest":manifest_pin,
        "ordinary_gae":{
            "gamma_bits":1.0f32.to_bits(),"lambda_bits":0.9f32.to_bits(),
            "normalization_epsilon_bits":GAE_ADVANTAGE_NORMALIZATION_EPSILON_V1.to_bits(),
            "source_episode_count":episodes.len(),"source_learner_group_count":groups.len(),
            "normalization_statistics":gae.statistics,
            "values":"recorded first substep per learner physical group",
            "normalization":"complete original ten-episode update before subset selection",
        },
        "selection":selection_provenance,
        "cpu_parent_reference_forwards":3,
        "scope":"fixed saved-row correctness diagnostic; no games, updates or GPU execution in preparation",
    });
    let provenance_pin = publish_json(&output, "provenance.json", &provenance).unwrap();
    eprintln!(
        "prepared registration correctness packet: {}",
        provenance_pin.path.display()
    );
}
