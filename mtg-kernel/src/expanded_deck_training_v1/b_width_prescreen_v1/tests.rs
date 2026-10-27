//! The screen note's pre-data tests (section 8) that CI can run, on the
//! shipped config and a synthetic fresh v4 parent, plus the HaleysPC launch
//! preflight on a real b512 (ignored here).

use super::*;
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;

const CONFIG_JSON_V1: &str = include_str!("../../../../data/b_width_prescreen_v1/config.json");
const ANALYZER_V1: &[u8] = include_bytes!("../../../../python/tools/b_width_prescreen_read_v1.py");

/// Note section 1.
const PARENT_SHA256_V1: [&str; 8] = [
    "8a52be51b92d9f7bbca211c5f0a20c7f291c1c78a74f1dd83fb71c1cb27c8837",
    "40ab50bfd74043364f643da11d488547dba74b72a9a410dd3589a303e4caac29",
    "acd5b487709294d31f9b9aa53a91e30a8a5eb00f5a25de194a5aedc3b54967ef",
    "b8ffe7c013eb697b4586b21e5058f967087f09b678fe633587b9e496335fcd03",
    "cd08845aefa69abfa24e59b37cde11166906479a18d7ce27fd81fba5d2df7d15",
    "15f582d65c367c98ca2caa4691b6e754057ae93de9aeafa82e6389f8d3f7fa0d",
    "a991b72b88df2b2c01ebe17ad006113c0e6461098d17fc56597e4f0802ad9ebd",
    "c4d2b6499135fd0a74b73fb8e2a82b53af83b021f15d35c33452fd412e2a4967",
];

fn shipped_v1() -> PrescreenConfigV1 {
    serde_json::from_str(CONFIG_JSON_V1).unwrap()
}

#[test]
fn shipped_config_is_the_frozen_note() {
    let config = shipped_v1();
    config.validate_v1().unwrap();
    assert_eq!(config.note.blob, "8b9c882ff7b42263c31e70bf9e500f9bc208d47e");
    assert_eq!(config.input_generation, "v4");
    assert_eq!(config.feature_contract_digest, FEATURE_CONTRACT_DIGEST_V4);
    assert_eq!(
        config.feature_contract_digest,
        "c4af415a3b0cf1e9c9960dbe2bc2d134c63e9f08206a9a364e113121fea5538b"
    );
    assert_eq!(
        config.feature_encoding_digest,
        "271c0e5a0fdce75663c897e89a9d7280ab1a3bbb6679bd10ecb5f524991952de"
    );
    let names: Vec<&str> = config
        .decks
        .iter()
        .map(|deck| deck.label.split('/').next().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates",
            "Faeries"
        ]
    );
    let c = &config.collection;
    assert_eq!(
        (
            c.games_per_pair,
            c.fit_indices.as_slice(),
            c.holdout_indices.as_slice()
        ),
        (5, &[0, 1, 2][..], &[3, 4][..])
    );
    assert_eq!(c.seed_domain, "b-width-prescreen-collect-v1");
    assert_eq!(
        (c.focal_seat, c.max_physical_decisions, c.max_policy_steps),
        (0, 100_000, 200_000)
    );
    assert_eq!(c.runtime_rules, EpisodeRuntimeRulesV1::ResolutionBoundaryV1);
    let parents: Vec<(u32, &str)> = config
        .chains
        .iter()
        .map(|chain| {
            (
                chain.index,
                chain.source.checkpoint.as_ref().unwrap().sha256.as_str(),
            )
        })
        .collect();
    let expected: Vec<(u32, &str)> = (1..=8).zip(PARENT_SHA256_V1).collect();
    assert_eq!(parents, expected);
    let f = &config.fitting;
    assert_eq!(
        f.learning_rate.to_bits(),
        953_267_991,
        "OH2's learning_rate_bits"
    );
    assert_eq!(
        f.value_coefficient.to_bits(),
        1_056_964_608,
        "OH2's value_coefficient_bits"
    );
    assert_eq!(
        (f.batch_groups, f.epochs, f.heldout_curve_points),
        (256, 8, 8)
    );
    assert_eq!(f.fitting_seeds, [1, 2, 3, 4]);
    assert_eq!(f.minibatch_seed_domain, "b-width-prescreen-minibatch-v1");
    assert_eq!(f.widen_seed_domain, "b-width-prescreen-widen-v1");
    let a = &config.analysis;
    assert_eq!(
        (
            a.bootstrap_draws,
            a.interval_lower_index,
            a.interval_upper_index
        ),
        (20_000, 500, 19_499)
    );
    assert_eq!(a.bootstrap_seed_domain, "b-width-prescreen-bootstrap-v1");
    assert_eq!((a.fund_lower_bound_above, a.fund_min_point_r), (0.0, 0.02));
    assert_eq!(
        (
            a.max_chain_nonnatural_holdout,
            a.max_chain_nonnatural_fit,
            a.min_holdout_games_per_pair
        ),
        (16, 24, 1)
    );
    // The analyzer the config pins is the committed one.
    assert_eq!(
        config.analyzer.path,
        "python/tools/b_width_prescreen_read_v1.py"
    );
    assert_eq!(config.analyzer.sha256, sha(ANALYZER_V1));
}

#[test]
fn seeds_split_and_schedule_match_the_note() {
    // SHA-256 of the UTF-8 strings, first 8 bytes little-endian.
    assert_eq!(
        domain_seed_v1("b-width-prescreen-collect-v1", &[1, 0, 0, 0]),
        1_174_745_202_221_360_953
    );
    assert_eq!(
        domain_seed_v1("b-width-prescreen-collect-v1", &[8, 8, 7, 4]),
        13_076_049_139_035_602_503
    );
    assert_eq!(
        domain_seed_v1("b-width-prescreen-bootstrap-v1", &[0]),
        9_237_536_898_453_982_863
    );
    let config = shipped_v1();
    assert_eq!(minibatch_seed_v1(&config, 1, 1), 15_313_765_789_897_877_101);
    assert_eq!(widen_seed_v1(&config, 3, 4), 14_853_655_151_267_175);
    for chain in 1..=8 {
        let plans = chain_schedule_v1(&config, chain);
        assert_eq!(plans.len(), 405);
        let fit = plans.iter().filter(|p| p.split == SplitV1::Fit).count();
        assert_eq!((fit, plans.len() - fit), (243, 162));
        let ids: BTreeSet<&str> = plans.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.len(), 405);
        for plan in &plans {
            assert_eq!(plan.split == SplitV1::Fit, plan.index <= 2);
            assert_eq!(
                plan.starting_player as u32,
                (plan.index + plan.focal + plan.opposing) % 2
            );
            assert_eq!(plan.focal_seat, 0);
            assert_eq!(
                plan.seed,
                domain_seed_v1(
                    "b-width-prescreen-collect-v1",
                    &[
                        chain.into(),
                        plan.focal.into(),
                        plan.opposing.into(),
                        plan.index.into()
                    ]
                )
            );
            let episode = episode_v1(&config, plan, &config.chains[0].source);
            assert_eq!(episode.learner_seat, 0);
            assert_eq!(episode.registered[0], config.decks[plan.focal as usize]);
            assert_eq!(episode.registered[1], config.decks[plan.opposing as usize]);
            assert_eq!(episode.selected, episode.registered);
            assert_eq!(episode.seed, plan.seed);
            assert!(episode.reset.is_none() && !episode.postboard);
        }
        // Schedule order: focal, then opposing, then index.
        assert_eq!(plans[0].id, format!("bwps-k{chain}-f0-o0-i0"));
        assert_eq!(plans[404].id, format!("bwps-k{chain}-f8-o8-i4"));
    }
    // (k, f, o, i) = (8, 8, 7, 4) is schedule position 8 * 45 + 7 * 5 + 4.
    let plan = &chain_schedule_v1(&config, 8)[399];
    assert_eq!(plan.id, "bwps-k8-f8-o7-i4");
    assert_eq!(plan.seed, 13_076_049_139_035_602_503);
}

#[test]
fn config_refuses_unknown_fields_and_note_mismatches() {
    let mut value: Value = serde_json::from_str(CONFIG_JSON_V1).unwrap();
    value["fitting"]["momentum"] = json!(0.9);
    assert!(serde_json::from_value::<PrescreenConfigV1>(value).is_err());
    let mut value: Value = serde_json::from_str(CONFIG_JSON_V1).unwrap();
    value["fitting"].as_object_mut().unwrap().remove("epochs");
    assert!(serde_json::from_value::<PrescreenConfigV1>(value).is_err());
    let refused = |change: &dyn Fn(&mut PrescreenConfigV1)| {
        let mut config = shipped_v1();
        change(&mut config);
        config.validate_v1().unwrap_err()
    };
    refused(&|c| c.input_generation = "v3".into());
    refused(&|c| c.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V3.into());
    refused(&|c| c.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V3.into());
    refused(&|c| {
        c.chains[2]
            .source
            .feature_transfer
            .expected_feature_contract_digest = FEATURE_CONTRACT_DIGEST_V3.into()
    });
    refused(&|c| c.chains[0].source.checkpoint = None);
    refused(&|c| c.collection.holdout_indices = vec![2, 3, 4]);
    refused(&|c| c.collection.fit_indices = vec![0, 1]);
    refused(&|c| c.arms.names.swap(1, 2));
    refused(&|c| c.arms.fresh_adam = false);
    refused(&|c| c.fitting.widen_seed_domain = c.fitting.minibatch_seed_domain.clone());
    refused(&|c| c.fitting.fitting_seeds = vec![1, 1]);
    refused(&|c| c.collection.seed_domain = "a|b".into());
    refused(&|c| c.analysis.bootstrap_draw_order = "pairs-first".into());
    let mut fixture = shipped_v1();
    fixture.chains[0].source.checkpoint = None;
    fixture.validate_with_v1(false).unwrap();
}

fn tensor_v1(seed: f32) -> NativeFlatDecisionTensorV2 {
    NativeFlatDecisionTensorV2 {
        state: vec![0.0, -0.0, seed, 0.0, 0.0, f32::MIN_POSITIVE / 4.0, 0.0],
        object_features: vec![0.0; 9],
        object_card_ids: vec![-1, 0, i64::MAX],
        object_groups: vec![],
        object_node_ids: vec![3],
        edge_features: vec![seed, seed, -0.0],
        edge_source_indices: vec![],
        edge_target_indices: vec![i64::MIN],
        action_features: vec![],
        action_ref_features: vec![1.0, 0.0, 2.0],
        action_ref_card_ids: vec![7],
        action_ref_action_indices: vec![0],
        action_ref_node_indices: vec![1],
    }
}

fn bit_image_v1(groups: &[ScreenDecisionGroupV1]) -> Vec<Vec<u32>> {
    groups
        .iter()
        .flat_map(|group| &group.substeps)
        .map(|substep| {
            let t = &substep.tensor;
            [
                &t.state,
                &t.object_features,
                &t.edge_features,
                &t.action_features,
                &t.action_ref_features,
            ]
            .iter()
            .flat_map(|values| values.iter().map(|value| value.to_bits()))
            .chain([substep.selected_action_index as u32])
            .collect()
        })
        .collect()
}

#[test]
fn store_round_trips_every_bit_and_refuses_damage() {
    let groups = vec![
        ScreenDecisionGroupV1 {
            actor: 0,
            substeps: vec![ScreenSubstepV1 {
                tensor: tensor_v1(1.5),
                selected_action_index: 2,
            }],
        },
        ScreenDecisionGroupV1 {
            actor: 1,
            substeps: vec![
                ScreenSubstepV1 {
                    tensor: tensor_v1(-3.25),
                    selected_action_index: 0,
                },
                ScreenSubstepV1 {
                    tensor: NativeFlatDecisionTensorV2::default(),
                    selected_action_index: 1,
                },
            ],
        },
    ];
    let bytes = encode_game_v1(&groups).unwrap();
    let decoded = decode_game_v1(&bytes).unwrap();
    assert_eq!(decoded, groups);
    assert_eq!(bit_image_v1(&decoded), bit_image_v1(&groups));
    assert_eq!(encode_game_v1(&decoded).unwrap(), bytes);
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_game_v1(&trailing).is_err());
    assert!(decode_game_v1(&bytes[..bytes.len() - 1]).is_err());
    // One group with one substep whose state is a single literal +0.0 word:
    // not canonical, since +0.0 words are always run-length coded.
    let mut literal_zero = Vec::new();
    put_len_v1(&mut literal_zero, 1).unwrap();
    literal_zero.push(0);
    put_len_v1(&mut literal_zero, 1).unwrap();
    put_len_v1(&mut literal_zero, 0).unwrap();
    put_len_v1(&mut literal_zero, 1).unwrap();
    put_len_v1(&mut literal_zero, 0).unwrap();
    put_len_v1(&mut literal_zero, 1).unwrap();
    put_u32_v1(&mut literal_zero, 0);
    assert!(decode_game_v1(&literal_zero)
        .unwrap_err()
        .contains("canonical"));
}

/// A net-8 snapshot with nonzero moments and a nonzero Adam step wherever
/// the optimizer state may be nonzero.
fn snapshot_with_moments_v1() -> NativePolicyValueTrainSnapshotV1 {
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let mut snapshot = NativePolicyValueTrainStateV1::new_v1(model)
        .unwrap()
        .snapshot_v1()
        .unwrap();
    let moments = |parameters: &[NativeNamedParameterV1], scale: f32| {
        parameters
            .iter()
            .enumerate()
            .map(|(tensor, parameter)| {
                let mut values: Vec<f32> = parameter
                    .values
                    .iter()
                    .map(|value| value.abs() * scale + 0.125)
                    .collect();
                if tensor == 0 {
                    // The card embedding's padding row keeps +0 moments.
                    let width = parameter.shape[1];
                    values[..width].fill(0.0);
                }
                if parameter.name == "scorer.2.bias" {
                    values.fill(0.0);
                }
                NativeNamedParameterV1 {
                    values,
                    ..parameter.clone()
                }
            })
            .collect::<Vec<_>>()
    };
    snapshot.first_moments = moments(&snapshot.parameters, 0.5);
    snapshot.second_moments = moments(&snapshot.parameters, 0.25);
    snapshot.adam_step = 9;
    snapshot
}

fn assert_fresh_v1(
    before: &NativePolicyValueTrainSnapshotV1,
    after: &NativePolicyValueTrainSnapshotV1,
) {
    assert_eq!(after.adam_step, 0);
    assert_eq!(
        after.scorer_bias_anchor_bits,
        before.scorer_bias_anchor_bits
    );
    assert_eq!(after.parameters.len(), before.parameters.len());
    for (a, b) in after.parameters.iter().zip(&before.parameters) {
        assert_eq!((a.name, &a.shape), (b.name, &b.shape));
        let bits =
            |p: &NativeNamedParameterV1| p.values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(a), bits(b), "{}", a.name);
    }
    for moment in after.first_moments.iter().chain(&after.second_moments) {
        assert!(moment.values.iter().all(|value| value.to_bits() == 0));
    }
    assert!(before
        .first_moments
        .iter()
        .any(|moment| moment.values.iter().any(|value| *value != 0.0)));
}

#[test]
fn fresh_adam_touches_only_optimizer_state() {
    let snapshot = snapshot_with_moments_v1();
    let narrow = narrow_state_v1(&snapshot).unwrap();
    assert_eq!(narrow.snapshot_v1().unwrap(), snapshot);
    let fresh = fresh_adam_v1(&narrow).unwrap();
    assert_fresh_v1(&snapshot, &fresh.snapshot_v1().unwrap());
    let parent = hex(&snapshot.state_sha256_v1().unwrap());
    let widened = arm_start_v1(&shipped_v1(), &snapshot, &parent, "w128", 77).unwrap();
    let ArmStartV1::Wide(wide) = &widened.start else {
        panic!("w128 is wide")
    };
    let wide_before = wide.snapshot_v1().unwrap();
    assert_eq!(wide_before.adam_step, 9);
    assert_fresh_v1(
        &wide_before,
        &fresh_adam_v1(wide).unwrap().snapshot_v1().unwrap(),
    );
    assert_eq!(widened.widen_seed, Some(77));
    let control = arm_start_v1(&shipped_v1(), &snapshot, &parent, "w32", 77).unwrap();
    let ArmStartV1::Narrow(control_state) = &control.start else {
        panic!("w32 is narrow")
    };
    let control_before = control_state.snapshot_v1().unwrap();
    assert_fresh_v1(
        &control_before,
        &fresh_adam_v1(control_state).unwrap().snapshot_v1().unwrap(),
    );
    assert_eq!(control.transform["parent_state_sha256"], json!(parent));
    assert!(arm_start_v1(&shipped_v1(), &snapshot, &parent, "w16", 77).is_err());
    assert!(arm_start_v1(&shipped_v1(), &snapshot, &"0".repeat(64), "w64", 77).is_err());
}

// ---------------------------------------------------------------------------
// End to end on a synthetic fresh v4 parent (CI-sized).
// ---------------------------------------------------------------------------

struct FixtureV1 {
    loaded: LoadedConfigV1,
    root: PathBuf,
}

impl Drop for FixtureV1 {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fixture_v1(label: &str) -> FixtureV1 {
    let root = std::env::temp_dir().join(format!("bwps-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let source = write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        FRESH_FEATURE_IDENTITY_V4,
        |_| {},
    );
    let descriptor = root.join("descriptor.json");
    let bytes = serde_json::to_vec(&source).unwrap();
    fs::write(&descriptor, &bytes).unwrap();
    let mut config = shipped_v1();
    // Two of OH2's decks (Affinity and Terror), one fit and one holdout game
    // per ordered pair, one fitting seed and a tiny fitting budget.
    config.decks = vec![config.decks[2].clone(), config.decks[6].clone()];
    config.collection.games_per_pair = 2;
    config.collection.fit_indices = vec![0];
    config.collection.holdout_indices = vec![1];
    config.chains = vec![PrescreenChainV1 {
        index: 1,
        id: "fixture".into(),
        source: ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: descriptor.canonicalize().unwrap(),
                sha256: sha(&bytes),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
                expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
            },
            checkpoint: None,
        },
    }];
    config.fitting.batch_groups = 8;
    config.fitting.epochs = 1;
    config.fitting.heldout_curve_points = 2;
    config.fitting.fitting_seeds = vec![1];
    config.validate_with_v1(false).unwrap();
    let sha256 = sha(&serde_json::to_vec(&config).unwrap());
    FixtureV1 {
        loaded: LoadedConfigV1 { config, sha256 },
        root,
    }
}

#[test]
fn stages_run_resume_and_preserve_on_a_fixture_parent() {
    let fixture = fixture_v1("stages");
    let (loaded, root) = (&fixture.loaded, fixture.root.as_path());
    let chain = loaded.config.chains[0].clone();

    // A parent whose digests differ from the note's is refused.
    let mut v3 = loaded.config.clone();
    v3.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V3.into();
    assert!(load_parent_v1(&v3, &chain.source).is_err());

    let collected = collect_chain_stage_v1(loaded, root, &chain, 2).unwrap();
    assert_eq!(collected["collect"], json!("written"));
    assert_eq!(collected["games"], json!(8));
    let collection_bytes = read_v1(&root.join("collect/k1/collection.json")).unwrap();
    assert_eq!(
        collect_chain_stage_v1(loaded, root, &chain, 2).unwrap()["collect"],
        json!("verified-existing")
    );

    // Data, then section 7 once every chain has data.
    assert!(load_chain_data_v1(loaded, root, &chain).is_err());
    assert_eq!(
        convert_chain_stage_v1(loaded, root, &chain).unwrap()["convert"],
        json!("written")
    );
    assert_eq!(
        convert_chain_stage_v1(loaded, root, &chain).unwrap()["convert"],
        json!("verified-existing")
    );
    let data = load_chain_data_v1(loaded, root, &chain).unwrap();
    assert_eq!(
        data.record.attempted.fit.len() + data.record.attempted.holdout.len(),
        8
    );
    assert_eq!(
        data.record.fit_groups,
        data.fit.iter().map(|g| g.groups.len()).sum::<usize>()
    );
    assert_eq!(
        (data.record.steps, data.record.heldout_every_steps),
        step_rule_v1(&loaded.config.fitting, data.record.fit_groups)
    );
    assert!(section7_stage_v1(loaded, root).unwrap().is_some());

    // The fit stage refuses before preservation has passed.
    assert!(fit_chain_stage_v1(loaded, root, &chain, 3, 1).is_err());
    let preserved = preserve_chain_stage_v1(loaded, root, &chain, 2).unwrap();
    assert!(preserved["inputs"].as_u64().unwrap() > 0);
    let preservation: Value =
        serde_json::from_slice(&read_v1(&root.join("preserve/k1.json")).unwrap()).unwrap();
    assert_eq!(preservation["pass"], json!(true));
    assert_eq!(preservation["seeds"][0]["mismatches"], json!(0));

    let fitted = fit_chain_stage_v1(loaded, root, &chain, 3, 1).unwrap();
    assert_eq!(fitted["fits_written"], json!(3));
    let record = |arm: &str| -> Value {
        serde_json::from_slice(&read_v1(&fit_path_v1(root, 1, 1, arm)).unwrap()).unwrap()
    };
    let w64 = record("w64");
    let w128 = record("w128");
    assert_eq!(w64["report"]["initial_adam_step"], json!(0));
    assert_eq!(
        w64["report"]["initial_state_sha256"],
        w64["fresh_adam_state_sha256"]
    );
    assert_eq!(
        w128["transform"]["child_state_sha256"],
        w128["transformed_state_sha256"]
    );
    assert_eq!(
        w128["transform"]["parent_state_sha256"],
        w64["parent_state_sha256"]
    );
    assert_eq!(
        w128["widen_seed"],
        json!(widen_seed_v1(&loaded.config, 1, 1))
    );
    assert_eq!(
        w64["report"]["minibatch_order_sha256"],
        w128["report"]["minibatch_order_sha256"]
    );
    assert_eq!(w64["report"]["heldout"][0], w128["report"]["heldout"][0]);
    assert_eq!(w64["report"]["config"]["steps"], json!(data.record.steps));

    // Resume: a lost report is rerun from the same inputs (at another fit
    // concurrency) to the same bytes, and present reports are kept.
    let w128_bytes = read_v1(&fit_path_v1(root, 1, 1, "w128")).unwrap();
    fs::remove_file(fit_path_v1(root, 1, 1, "w128")).unwrap();
    assert_eq!(
        fit_chain_stage_v1(loaded, root, &chain, 2, 1).unwrap()["fits_written"],
        json!(1)
    );
    assert_eq!(
        read_v1(&fit_path_v1(root, 1, 1, "w128")).unwrap(),
        w128_bytes
    );

    // A rerun collection reproduces the record's bytes.
    let rerun = fixture_v1("stages-rerun");
    let rerun_loaded = LoadedConfigV1 {
        config: loaded.config.clone(),
        sha256: loaded.sha256.clone(),
    };
    collect_chain_stage_v1(&rerun_loaded, &rerun.root, &chain, 1).unwrap();
    assert_eq!(
        read_v1(&rerun.root.join("collect/k1/collection.json")).unwrap(),
        collection_bytes
    );

    // Damage is refused.
    let data_path = root.join("data/k1.json");
    let mut damaged = read_v1(&data_path).unwrap();
    damaged.insert(1, b' ');
    fs::write(&data_path, damaged).unwrap();
    assert!(load_chain_data_v1(loaded, root, &chain).is_err());
    let store = root.join("collect/k1/games.bin");
    let mut bytes = read_v1(&store).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&store, bytes).unwrap();
    assert!(read_collection_v1(loaded, root, &chain).is_err());
}

#[test]
fn a_seed_at_or_above_two_to_the_63_plays() {
    let fixture = fixture_v1("high-seed");
    let config = &fixture.loaded.config;
    let parent = load_parent_v1(config, &config.chains[0].source).unwrap();
    let mut plan = chain_schedule_v1(config, 1)[1].clone();
    plan.seed = (1u64 << 63) | 0x0B51_2027;
    assert!(plan.seed >= 1u64 << 63);
    let games = collect_games_v1(config, &parent, &[plan], 1).unwrap();
    assert_eq!(games.len(), 1);
    assert!(games[0].terminal.policy_step_count > 0);
}

#[test]
fn the_throughput_guard_refuses_other_hosts_binaries_and_counts() {
    let fixture = fixture_v1("guard");
    let loaded = &fixture.loaded;
    let path = fixture.root.join("qualification.json");
    let good = json!({"schema": QUALIFICATION_SCHEMA_V1, "stage": "fit", "host": host_name_v1(),
        "config_sha256": loaded.sha256, "binary_sha256": binary_sha256_v1().unwrap(),
        "selected_workers": 4});
    fs::write(&path, serde_json::to_vec(&good).unwrap()).unwrap();
    require_qualification_v1(loaded, &path, QualifiedStageV1::Fit, 4).unwrap();
    assert!(require_qualification_v1(loaded, &path, QualifiedStageV1::Fit, 8).is_err());
    assert!(require_qualification_v1(loaded, &path, QualifiedStageV1::Collect, 4).is_err());
    for (field, value) in [
        ("host", json!("another-host")),
        ("binary_sha256", json!("0".repeat(64))),
        ("config_sha256", json!("1".repeat(64))),
    ] {
        let mut changed = good.clone();
        changed[field] = value;
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(require_qualification_v1(loaded, &path, QualifiedStageV1::Fit, 4).is_err());
    }
}

#[test]
fn staging_check_reports_missing_and_changed_files() {
    let fixture = fixture_v1("staging");
    let mut loaded = fixture.loaded.clone();
    // The fixture import pins its manifest and parameters by absolute path;
    // a parent checkpoint is required.
    assert!(staging_check_stage_v1(&loaded, &fixture.root).is_err());
    let record: Value =
        serde_json::from_slice(&read_v1(&fixture.root.join("staging-check.json")).unwrap())
            .unwrap();
    assert_eq!(record["pass"], json!(false));
    let checkpoint = fixture.root.join("checkpoint.json");
    let digests = json!({"feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
        "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4});
    let bytes = serde_json::to_vec(&digests).unwrap();
    fs::write(&checkpoint, &bytes).unwrap();
    loaded.config.chains[0].source.checkpoint = Some(PinnedFileV1 {
        path: checkpoint.canonicalize().unwrap(),
        sha256: sha(&bytes),
    });
    let passed = staging_check_stage_v1(&loaded, &fixture.root).unwrap();
    assert_eq!(passed["pass"], json!(true));
    assert!(
        passed["chains"][0]["detail"]["import_pins"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
    fs::write(&checkpoint, b"{}").unwrap();
    assert!(staging_check_stage_v1(&loaded, &fixture.root).is_err());
}

// ---------------------------------------------------------------------------
// HaleysPC launch preflight on a real b512 (note sections 6.5 and 8).
// ---------------------------------------------------------------------------

/// Set `MTG_KERNEL_B_WIDTH_PRESCREEN_CONFIG` to the frozen config with its
/// parents staged; `MTG_KERNEL_B_WIDTH_PRESCREEN_CHAIN` picks the chain
/// (default 1). Plays the config's qualification games (never screen games),
/// then requires w128's outputs on every recorded input to be bit-identical
/// to w64's for every fitting seed, and both arms' step-0 held-out reports to
/// be bit-identical.
#[test]
#[ignore = "HaleysPC launch preflight: needs MTG_KERNEL_B_WIDTH_PRESCREEN_CONFIG with staged b512 parents; plays real games"]
fn real_b512_step0_preflight() {
    let path = std::env::var_os("MTG_KERNEL_B_WIDTH_PRESCREEN_CONFIG")
        .expect("set MTG_KERNEL_B_WIDTH_PRESCREEN_CONFIG");
    let loaded = load_config_v1(Path::new(&path)).unwrap();
    let index = std::env::var("MTG_KERNEL_B_WIDTH_PRESCREEN_CHAIN")
        .map_or(1, |value| value.parse().unwrap());
    let config = &loaded.config;
    let chain = config.chain_v1(index).unwrap();
    let parent = load_parent_v1(config, &chain.source).unwrap();
    let plans = qualification_plans_v1(config, chain.index);
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    let games = collect_games_v1(config, &parent, &plans, workers).unwrap();
    let records: Vec<ScreenGameRecordV1> = plans
        .iter()
        .zip(&games)
        .filter_map(|(plan, game)| {
            game.stored.as_ref().map(|stored| ScreenGameRecordV1 {
                game_id: plan.id.clone(),
                outcome: ScreenGameOutcomeV1::from(&game.terminal),
                groups: decode_game_v1(&stored.bytes).unwrap(),
            })
        })
        .collect();
    assert!(records.len() >= 2, "fewer than two natural games");
    let (fit, holdout) = records.split_at(records.len() / 2);
    let snapshot = parent.state.snapshot_v1().unwrap();
    let (inputs, seeds) = preservation_seeds_v1(
        config,
        chain.index,
        &snapshot,
        [fit, holdout],
        &config.fitting.fitting_seeds,
        workers,
    )
    .unwrap();
    for seed in &seeds {
        assert_eq!(
            seed.mismatches, 0,
            "seed {}: {:?}",
            seed.seed, seed.first_mismatch
        );
    }
    let fit_groups: usize = fit.iter().map(|game| game.groups.len()).sum();
    let mut reports = Vec::new();
    for arm in ["w64", "w128"] {
        let start = arm_start_v1(
            config,
            &snapshot,
            &parent.state_sha256,
            arm,
            widen_seed_v1(config, chain.index, 1),
        )
        .unwrap();
        let mut fitting = config.fitting.clone();
        fitting.batch_groups = fitting.batch_groups.min(fit_groups);
        let fit_config = screen_fit_config_v1(&fitting, 1, 1, 1, workers.min(64));
        let split = [fit, holdout];
        let (_, report) = match &start.start {
            ArmStartV1::Narrow(state) => {
                fit_state_v1(state, config.generation_v1(), split, &fit_config).unwrap()
            }
            ArmStartV1::Wide(state) => {
                fit_state_v1(state, config.generation_v1(), split, &fit_config).unwrap()
            }
        };
        reports.push(serde_json::to_vec(&report.heldout[0].games).unwrap());
    }
    assert_eq!(reports[0], reports[1], "step-0 held-out reports differ");
    eprintln!(
        "chain {index}: w128 matched w64 bit for bit on {inputs} inputs for {} seeds",
        seeds.len()
    );
}
