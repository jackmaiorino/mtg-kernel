//! The V4 inference export on a small synthetic FlatV7 checkpoint: every
//! parameter and moment bit kept, only the two identity values changed, the
//! V4 load and the V4 scorer's ready record, in-game equality through both
//! scorers, and one refusal per admission check.

use super::*;
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::expanded_deck_training_v1::{
    initialize, publish_json, write_synthetic_fresh_source_with_parameters_v1,
    GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
};
use crate::spellbench_scorer_v1::{
    run_spellbench_scorer_v1, SpellbenchScorerConfigV1, SCORER_CONFIG_SCHEMA_V1,
};

/// The eight decks of the Spellbench pauper-ladder-v3 board.
const BOARD_DECKS: [&str; 8] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
];

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "v4-inference-export-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn transfer(contract: &str, encoding: &str) -> FrozenPlayObservationTransferV3 {
    FrozenPlayObservationTransferV3 {
        expected_feature_contract_digest: contract.into(),
        expected_feature_encoding_digest: encoding.into(),
    }
}

struct Fixture {
    root: PathBuf,
    /// The V4 fresh source, without a checkpoint.
    v4: ExpandedModelSourceV1,
    /// The FlatV7 source of the FlatV7 checkpoint.
    v7: ExpandedModelSourceV1,
    /// The FlatV7 checkpoint record.
    saved: ExpandedCheckpointV1,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = temp_root(label);
        // Publication writes into an existing directory only.
        fs::create_dir_all(root.join("out")).unwrap();
        let descriptor = write_synthetic_fresh_source_with_parameters_v1(
            &root.join("source"),
            FRESH_FEATURE_IDENTITY_V4,
            |_| {},
        );
        let bytes = serde_json::to_vec(&descriptor).unwrap();
        let path = root.join("descriptor.json");
        fs::write(&path, &bytes).unwrap();
        let v4 = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path,
                sha256: sha(&bytes),
            },
            feature_transfer: transfer(FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4),
            checkpoint: None,
            s_learner: None,
        };
        // A trained-looking state: moved parameters, nonzero moments and step.
        let (policy, state) = initialize(&v4).unwrap();
        let mut snapshot = state.snapshot_v1().unwrap();
        // The last element of each tensor: never the card embedding's zero
        // padding row, and the one-element gauge anchor (the scorer's second
        // bias) is left where it is.
        for (index, row) in snapshot.parameters.iter_mut().enumerate() {
            if let Some(value) = row.values.last_mut().filter(|_| row.shape != [1]) {
                *value += 0.0625 * (1 + index % 5) as f32;
            }
        }
        for row in &mut snapshot.first_moments {
            if row.shape != [1] {
                *row.values.last_mut().unwrap() = 0.125;
            }
        }
        for row in &mut snapshot.second_moments {
            if row.shape != [1] {
                *row.values.last_mut().unwrap() = 0.0625;
            }
        }
        snapshot.adam_step = 12;
        let as_bits = |rows: &[crate::native_policy_value_net_v1::NativeNamedParameterV1]| {
            rows.iter().map(ParameterBitsV1::from_native).collect()
        };
        let saved = ExpandedCheckpointV1 {
            schema: FRESH_CHECKPOINT_SCHEMA.into(),
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V7.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V7.into(),
            card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            source_import: policy.identity_v1().clone(),
            state_sha256: hex(&snapshot.state_sha256_v1().unwrap()),
            e_supported: None,
            soft_q: None,
            search_targets: None,
            ex1: None,
            adam_step: snapshot.adam_step,
            scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
            parameters: as_bits(&snapshot.parameters),
            first_moments: as_bits(&snapshot.first_moments),
            second_moments: as_bits(&snapshot.second_moments),
            trajectories: (0..2)
                .map(|index| PinnedFileV1 {
                    path: root.join(format!("episode-{index:04}.json")),
                    sha256: format!("{index}").repeat(64),
                })
                .collect(),
            loss_identity: GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into(),
            gamma_bits: Some(1.0_f32.to_bits()),
            gae_lambda_bits: Some(0.9_f32.to_bits()),
            entropy_coefficient_bits: Some(0.0_f32.to_bits()),
            exploration: None,
            learning_rate_bits: 0.0001_f32.to_bits(),
            value_coefficient_bits: 0.5_f32.to_bits(),
            registry_transfer: None,
            archive_distillation: None,
            training_math: TrainingMathV1::PlatformLibm,
            input_contract: None,
        };
        let mut fixture = Self {
            v7: v4.clone(),
            root,
            v4,
            saved,
        };
        let pin = fixture.write_record("v7", |_| {});
        fixture.v7 = ExpandedModelSourceV1 {
            feature_transfer: transfer(FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7),
            checkpoint: Some(pin),
            ..fixture.v4.clone()
        };
        fixture
    }

    /// The FlatV7 record, changed by `change`, published under `name`.
    fn write_record(
        &self,
        name: &str,
        change: impl FnOnce(&mut ExpandedCheckpointV1),
    ) -> PinnedFileV1 {
        let mut saved = self.saved.clone();
        change(&mut saved);
        let directory = self.root.join(name);
        fs::create_dir_all(&directory).unwrap();
        publish_json(&directory, "checkpoint.json", &saved).unwrap()
    }

    /// The FlatV7 source with the checkpoint `pin`.
    fn v7_with(&self, pin: PinnedFileV1) -> ExpandedModelSourceV1 {
        ExpandedModelSourceV1 {
            checkpoint: Some(pin),
            ..self.v7.clone()
        }
    }

    fn output(&self, name: &str) -> PathBuf {
        self.root.join("out").join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn parameter_bits(policy: &FrozenPlayPolicyV1) -> Vec<(String, Vec<usize>, Vec<u32>)> {
    policy
        .training_parameters_v3()
        .into_iter()
        .map(|row| {
            let bits = row.values.iter().map(|value| value.to_bits()).collect();
            (row.name.to_string(), row.shape, bits)
        })
        .collect()
}

#[test]
fn the_export_keeps_every_parameter_and_moment_bit_and_changes_only_the_identity() {
    let fixture = Fixture::new("keeps");
    let output = fixture.output("checkpoint-v4.json");
    let receipt = export_flat_v7_to_v4_inference_v1(&fixture.v7, &output).unwrap();
    let input = fs::read(&fixture.v7.checkpoint.as_ref().unwrap().path).unwrap();
    let exported = fs::read(&output).unwrap();

    // The bytes: the input with the two FlatV7 digest values replaced by
    // V4's, and nothing else.
    let expected = String::from_utf8(input.clone())
        .unwrap()
        .replacen(FEATURE_CONTRACT_DIGEST_V7, FEATURE_CONTRACT_DIGEST_V4, 1)
        .replacen(FEATURE_ENCODING_DIGEST_V7, FEATURE_ENCODING_DIGEST_V4, 1);
    assert_eq!(exported, expected.into_bytes());
    assert_ne!(exported, input);
    assert_eq!(
        receipt["differing_bytes"]
            .as_u64()
            .map(|n| n > 0 && n <= 128),
        Some(true)
    );
    assert_eq!(receipt["input"]["canonical"], json!(true));

    // The record: every tensor bit and every other member equal.
    let record: ExpandedCheckpointV1 = serde_json::from_slice(&exported).unwrap();
    assert_eq!(record.feature_contract_digest, FEATURE_CONTRACT_DIGEST_V4);
    assert_eq!(record.feature_encoding_digest, FEATURE_ENCODING_DIGEST_V4);
    let tensors = |saved: &ExpandedCheckpointV1| {
        serde_json::to_value((
            &saved.parameters,
            &saved.first_moments,
            &saved.second_moments,
        ))
        .unwrap()
    };
    assert_eq!(tensors(&record), tensors(&fixture.saved));
    assert_eq!(record.state_sha256, fixture.saved.state_sha256);
    assert_eq!(record.source_import, fixture.saved.source_import);
    assert_eq!(record.adam_step, 12);

    // The receipt: equal parameter and moment digests and state digests.
    for member in [
        "parameter_bytes_sha256",
        "first_moment_bytes_sha256",
        "second_moment_bytes_sha256",
        "state_sha256",
    ] {
        assert_eq!(
            receipt["input"][member], receipt["output"][member],
            "{member}"
        );
    }
    assert_eq!(
        receipt["output"]["state_sha256"],
        json!(fixture.saved.state_sha256)
    );
    assert_eq!(
        receipt["output"]["checkpoint"]["sha256"],
        json!(sha(&exported))
    );

    // The V4 load: the V4 identity, the same parameters and state.
    let v4_source: ExpandedModelSourceV1 =
        serde_json::from_value(receipt["v4_source"].clone()).unwrap();
    assert_eq!(v4_source.play_import, fixture.v7.play_import);
    let (v4_policy, v4_identity) = load_expanded_inference_v1(&v4_source).unwrap();
    let (v7_policy, v7_identity) = load_expanded_inference_v1(&fixture.v7).unwrap();
    assert_eq!(v4_policy.feature_identity_v1(), FRESH_FEATURE_IDENTITY_V4);
    assert_eq!(v7_policy.feature_identity_v1(), FRESH_FEATURE_IDENTITY_V7);
    assert_eq!(v4_identity.state_sha256, v7_identity.state_sha256);
    assert_eq!(parameter_bits(&v4_policy), parameter_bits(&v7_policy));
    assert_eq!(
        v4_policy.actual_model_identity_v1().weights_sha256,
        v7_policy.actual_model_identity_v1().weights_sha256
    );
    // Without the export, the V4 source refuses the FlatV7 record.
    assert!(load_expanded_inference_v1(&v4_source_v1(
        &fixture.v7,
        fixture.v7.checkpoint.clone().unwrap()
    ))
    .is_err());

    // Deterministic: a second export writes the same bytes.
    let again = fixture.output("checkpoint-v4-again.json");
    export_flat_v7_to_v4_inference_v1(&fixture.v7, &again).unwrap();
    assert_eq!(fs::read(&again).unwrap(), exported);

    // The board's scorer on the export: the V4 identity and the FlatV7
    // checkpoint's state digest.
    let mut ready = Vec::new();
    run_spellbench_scorer_v1(
        SpellbenchScorerConfigV1 {
            schema: SCORER_CONFIG_SCHEMA_V1.into(),
            source: v4_source,
            selection: SELECTION_SAMPLED_WIDE_V1.into(),
        },
        &mut std::io::Cursor::new(Vec::<u8>::new()),
        &mut ready,
    )
    .unwrap();
    let ready: Value =
        serde_json::from_slice(ready.split(|&b| b == b'\n').next().unwrap()).unwrap();
    assert_eq!(ready["feature_contract_digest"], FEATURE_CONTRACT_DIGEST_V4);
    assert_eq!(ready["feature_encoding_digest"], FEATURE_ENCODING_DIGEST_V4);
    assert_eq!(
        ready["model_state_sha256"],
        json!(fixture.saved.state_sha256)
    );
}

/// In-game equality through both scorers, over all eight board decks: the
/// FlatV7 policy on its FlatV7 tensors and the export on its V4 tensors give
/// the same tensors, logits, value and sampled row at every decision.
#[cfg(not(feature = "limited-fdn-fixtures"))]
#[test]
fn the_export_scores_in_game_decisions_as_the_flat_v7_policy_does() {
    let fixture = Fixture::new("equality");
    let output = fixture.output("checkpoint-v4.json");
    let receipt = export_flat_v7_to_v4_inference_v1(&fixture.v7, &output).unwrap();
    let v4_checkpoint: PinnedFileV1 =
        serde_json::from_value(receipt["output"]["checkpoint"].clone()).unwrap();
    let games = (0..4)
        .map(|index| V7V4EqualityGameV1 {
            seed: 0x5eed_0000 + index as u64,
            starting_player: (index % 2) as u8,
            decks: [
                BOARD_DECKS[2 * index].into(),
                BOARD_DECKS[2 * index + 1].into(),
            ],
        })
        .collect();
    let request = V7V4EqualityRequestV1 {
        schema: V7_V4_EQUALITY_REQUEST_SCHEMA_V1.into(),
        v7_source: fixture.v7.clone(),
        v4_checkpoint: v4_checkpoint.clone(),
        max_decisions_per_game: 60,
        neutral_choices: true,
        games,
    };
    let result = flat_v7_v4_equality_v1(&request).unwrap();
    assert_eq!(result["mismatches"], json!(0), "{result:#}");
    assert_eq!(result["v7_only_refusals"], json!(0), "{result:#}");
    assert_eq!(result["equal"], json!(true));
    assert!(result["rows"].as_u64().unwrap() >= 120, "{result:#}");
    assert_eq!(
        result["model_state_sha256"],
        json!(fixture.saved.state_sha256)
    );
    for game in result["games"].as_array().unwrap() {
        assert_eq!(game["v4_refusal"], Value::Null, "{game:#}");
    }

    // Safeguard: a V4 checkpoint of another state is refused, not compared.
    let other = fixture.write_record("other-v4", |saved| {
        saved.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V4.into();
        saved.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V4.into();
        saved.adam_step = 13;
        saved.state_sha256 = {
            let template = NativePolicyValueNetV1::runner_fixed_v1(
                NativePolicyValueModelConfigV1::contract_v1(),
            )
            .unwrap()
            .parameter_snapshot_v1();
            let snapshot = NativePolicyValueTrainSnapshotV1 {
                adam_step: 13,
                scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
                parameters: restore_parameters(&saved.parameters, &template).unwrap(),
                first_moments: restore_parameters(&saved.first_moments, &template).unwrap(),
                second_moments: restore_parameters(&saved.second_moments, &template).unwrap(),
            };
            hex(&snapshot.state_sha256_v1().unwrap())
        };
    });
    let refused = flat_v7_v4_equality_v1(&V7V4EqualityRequestV1 {
        v4_checkpoint: other,
        ..request
    })
    .unwrap_err();
    assert_eq!(
        refused,
        "the V4 checkpoint is not the FlatV7 checkpoint's state"
    );
}

/// Each admission check, with the refusal it gives; the output is never
/// written. Without the check, each case either exports (the V4-to-V7
/// transfer case) or fails later with another message.
#[test]
fn the_export_refuses_every_input_that_is_not_a_flat_v7_checkpoint() {
    let fixture = Fixture::new("refusals");
    let v7_pin = fixture.v7.checkpoint.clone().unwrap();
    let mut cases: Vec<(&str, ExpandedModelSourceV1, &str)> = Vec::new();
    // The source.
    cases.push((
        "a V4 source",
        ExpandedModelSourceV1 {
            feature_transfer: transfer(FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4),
            ..fixture.v7.clone()
        },
        "the V4 inference export takes a FlatV7 source (both FlatV7 feature digests)",
    ));
    cases.push((
        "a half-V7 source",
        ExpandedModelSourceV1 {
            feature_transfer: transfer(FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V4),
            ..fixture.v7.clone()
        },
        "the V4 inference export takes a FlatV7 source (both FlatV7 feature digests)",
    ));
    cases.push((
        "a source without a checkpoint",
        ExpandedModelSourceV1 {
            checkpoint: None,
            ..fixture.v7.clone()
        },
        "the V4 inference export requires the FlatV7 checkpoint",
    ));
    cases.push((
        "a native S source",
        ExpandedModelSourceV1 {
            s_learner: Some(Box::new(super::super::s_learner_v1::SLearnerSourceV1 {
                schema: "s".into(),
                initial: v7_pin.clone(),
            })),
            ..fixture.v7.clone()
        },
        "the V4 inference export takes a net-8 source; a native S source refuses it",
    ));
    // The checkpoint record.
    let v4_record = fixture.write_record("v4-record", |saved| {
        saved.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V4.into();
        saved.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V4.into();
    });
    cases.push((
        "a V4 record (the V4-to-V7 transfer)",
        fixture.v7_with(v4_record),
        "the V4 inference export takes a checkpoint with the FlatV7 contract digest",
    ));
    let v8_record = fixture.write_record("v8-record", |saved| {
        saved.feature_contract_digest =
            crate::native_flat_tensorizer_v8::FEATURE_CONTRACT_DIGEST_V8.into();
        saved.feature_encoding_digest =
            crate::native_flat_tensorizer_v8::FEATURE_ENCODING_DIGEST_V8.into();
    });
    cases.push((
        "a FlatV8 record",
        fixture.v7_with(v8_record),
        "the V4 inference export takes a checkpoint with the FlatV7 contract digest",
    ));
    let half_record = fixture.write_record("half-record", |saved| {
        saved.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V4.into();
    });
    cases.push((
        "a half-V7 record",
        fixture.v7_with(half_record),
        "the V4 inference export takes a checkpoint with the FlatV7 encoding digest",
    ));
    let legacy_record = fixture.write_record("legacy-record", |saved| {
        saved.schema = CHECKPOINT_SCHEMA.into();
    });
    cases.push((
        "a legacy-schema record",
        fixture.v7_with(legacy_record),
        "the V4 inference export takes a fresh-lineage checkpoint",
    ));
    // The full FlatV7 load runs before anything is written.
    let tampered = fixture.write_record("tampered-record", |saved| {
        *saved.parameters[1].values.last_mut().unwrap() ^= 1;
    });
    cases.push((
        "a record whose tensors do not match its state digest",
        fixture.v7_with(tampered),
        "checkpoint state hash differs",
    ));
    // Every case is run before the verdict, so a build without one check
    // names each case that check guards.
    let mut failures = Vec::new();
    for (index, (label, source, message)) in cases.into_iter().enumerate() {
        let output = fixture.output(&format!("refused-{index}.json"));
        let outcome = export_flat_v7_to_v4_inference_v1(&source, &output);
        if outcome.as_ref().err().map(String::as_str) != Some(message) || output.exists() {
            failures.push(format!(
                "{label}: expected refusal {message:?}, got {:?}, output written {}",
                outcome.map(|_| "an export"),
                output.exists()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );

    // An existing output is never replaced.
    let output = fixture.output("existing.json");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, b"keep").unwrap();
    let error = export_flat_v7_to_v4_inference_v1(&fixture.v7, &output).unwrap_err();
    assert_eq!(error, "a fresh absolute output path is required");
    assert_eq!(fs::read(&output).unwrap(), b"keep");
}

/// The cards of the eight board decks whose cast announces X (a mana cost
/// or bestow cost with X): the only cards that can pose the forced X = 0
/// numeric choice.
#[cfg(not(feature = "limited-fdn-fixtures"))]
#[test]
fn the_board_decks_announce_x_only_for_nyxborn_hydra() {
    let mut found = BTreeSet::new();
    for deck in BOARD_DECKS {
        let deck = crate::runtime_decks::runtime_deck_by_id(deck).unwrap();
        for &card in deck.card_ids {
            let def = &crate::card_def::CARD_DEFS[card as usize];
            if def.cost.x_count > 0 || def.bestow.as_ref().is_some_and(|b| b.cost.x_count > 0) {
                found.insert((deck.id, def.name));
            }
        }
    }
    assert_eq!(
        found.into_iter().collect::<Vec<_>>(),
        vec![("Elves", "Nyxborn Hydra"), ("Wildfire", "Nyxborn Hydra")]
    );
}

/// The forced X = 0 cast of Nyxborn Hydra under the board's neutral choices:
/// a singleton numeric choice that the V4 tensorizer encodes (column 15) and
/// the FlatV7 one refuses; choosing it gives the game state the trainer's
/// automatic X = 0 gives, byte for byte.
#[test]
fn the_forced_x_zero_choice_equals_the_trainers_automatic_x() {
    use crate::engine::{advance_until_decision, step, Action, Decision};
    use crate::policy_observation_v6::tests::{put, ready_state};
    let mut state = ready_state();
    state.engine.spellbench_neutral_v1.choices = true;
    state.players[0].mana_pool[crate::mana::ManaColor::G.pool_index()] = 1;
    let hydra = put(
        &mut state,
        PlayerId::P0,
        "Nyxborn Hydra",
        crate::state::Zone::Hand,
    );
    advance_until_decision(&mut state);
    step(&mut state, Action::CastSpell(hydra)).unwrap();
    assert!(matches!(
        advance_until_decision(&mut state),
        Decision::ChooseSpellMode { .. }
    ));
    step(&mut state, Action::ChooseSpellMode(0)).unwrap();
    assert!(matches!(
        advance_until_decision(&mut state),
        Decision::ChooseEffectOption {
            option_count: 1,
            ..
        }
    ));
    let trainer = forced_x_trainer_path_v1(&state).unwrap();
    assert_eq!(trainer["equal"], json!(true), "{trainer:#}");
    assert_eq!(trainer["board_announced_x"], json!(0));
    assert_eq!(trainer["state_sha256"][0], trainer["state_sha256"][1]);

    // The row: V4 encodes it with the numeric kind column, FlatV7 refuses.
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut v4 = FrozenPlayPolicyV1::training_fixture_v4();
    let scores = v4.score_fast_session_v1(&session).unwrap();
    assert_eq!(scores.logits.len(), 1);
    let tensor = &v4.last_scored_training_tensor_v4().unwrap().common;
    assert_eq!(tensor.action_features[NUMERIC_CHOICE_KIND_COLUMN_V1], 1.0);
    let mut v7 = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    assert!(v7.score_fast_session_v1(&session).is_err());
}
