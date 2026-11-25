use super::*;
use crate::card_def::{card_id_by_name, KERNEL_CARDDB_HASH};
use crate::cube_draft_v1::{run_draft, uniform_pickers, CubePool, PACK_SIZE, POOL_CARDS, SEATS};
use crate::cube_owner_view_v1::if2::{run_seat_draft, SeatPicker, UniformSeatPicker};
use sha2::Digest;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Loads the committed Vintage Cube list and gap record, sorts its registry
/// ids in ascending order, and constructs an ordered `CubePool` for testing.
fn test_ordered_pool() -> (CubePool, String) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let cube_list = fs::read_to_string(root.join("data/cube_v1/mtgo_vintage_cube_20261007.json"))
        .expect("read cube list");
    let gaps = fs::read_to_string(root.join("data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"))
        .expect("read gaps");
    let raw_pool = CubePool::from_json_texts(&cube_list, &gaps, None).expect("build raw pool");
    let mut ids: Vec<u16> = raw_pool.ids().to_vec();
    ids.sort_unstable();
    ids.dedup();
    // Keep at least 484 ascending cards (or all valid cube cards).
    cube_pool_from_ordered_ids(&ids).expect("build ascending ordered pool")
}

fn carddb_hex() -> String {
    format!("{KERNEL_CARDDB_HASH:016x}")
}

#[test]
fn seeds_match_python_deal_py_reference_values() {
    // Reference outputs computed directly from tools/cube_v1/deal.py::seed.
    let expected: [(&str, u64, u64); 24] = [
        ("b1-u0032", 0, 5_905_443_338_339_785_416),
        ("b1-u0032", 100, 1_145_529_432_473_527_761),
        ("b1-u0032", 120, 3_494_327_161_932_615_146),
        ("b1-u0032", 200, 8_929_407_378_827_673_326),
        ("b1-u0064", 0, 2_426_405_767_764_934_103),
        ("b1-u0064", 100, 4_775_911_627_448_565_525),
        ("b1-u0064", 120, 8_281_419_498_411_807_265),
        ("b1-u0064", 200, 5_064_358_112_961_863_380),
        ("b1-u0096", 0, 171_512_452_470_252_828),
        ("b1-u0096", 100, 7_942_344_022_793_431_746),
        ("b1-u0096", 120, 3_092_980_578_249_044_225),
        ("b1-u0096", 200, 3_875_576_173_396_283_369),
        ("b1-u0128", 0, 7_511_191_187_837_053_771),
        ("b1-u0128", 100, 802_286_136_390_078_060),
        ("b1-u0128", 120, 6_916_429_260_029_137_164),
        ("b1-u0128", 200, 4_793_359_211_352_512_794),
        ("k1-u0064", 0, 1_942_675_477_333_453_122),
        ("k1-u0064", 100, 2_185_358_464_858_774_477),
        ("k1-u0064", 120, 4_779_247_130_783_831_905),
        ("k1-u0064", 200, 9_076_490_310_046_638_890),
        ("k1-u0128", 0, 2_959_709_852_624_785_587),
        ("k1-u0128", 100, 4_329_737_740_626_135_387),
        ("k1-u0128", 120, 5_103_103_361_400_751_020),
        ("k1-u0128", 200, 710_088_427_059_818_498),
    ];
    for (run_id, draft_idx, expected_seed) in expected {
        let got = owner_view_draft_seed(run_id, draft_idx).expect("valid seed");
        assert_eq!(
            got, expected_seed,
            "seed mismatch for run={run_id} draft={draft_idx}"
        );
    }
    // Reject invalid run ids.
    assert!(owner_view_draft_seed("B1-U0032", 0).is_err());
    assert!(owner_view_draft_seed("", 0).is_err());
    assert!(owner_view_draft_seed("b1_u0032", 0).is_err());
}

#[test]
fn if2_uniform_seat_draft_equals_existing_run_draft() {
    let (pool, _sha) = test_ordered_pool();
    let draft_seed = owner_view_draft_seed("b1-u0032", 100).expect("seed");
    let mut legacy_pickers = uniform_pickers(draft_seed);
    let legacy = run_draft(&pool, draft_seed, &mut legacy_pickers).expect("legacy run_draft");

    let mut seat_pickers: [Box<dyn SeatPicker>; SEATS] =
        std::array::from_fn(|_| Box::new(UniformSeatPicker) as Box<dyn SeatPicker>);
    let seat_draft = run_seat_draft(&pool, draft_seed, &mut seat_pickers).expect("run_seat_draft");

    assert_eq!(seat_draft.record, legacy);
    for rows in &seat_draft.pick_rows {
        assert!(rows.is_empty(), "UniformSeatPicker returns row: None");
    }
}

#[test]
fn sample_pick_schedule_matches_d021_and_l4() {
    let mut adjusted_indices = Vec::new();
    for m in 0..SAMPLE_PICK_COUNT {
        let spec = sample_pick_spec(m);
        assert_eq!(spec.draft_index, 100 + m as u64);
        assert_eq!(spec.chair, m % SEATS);
        let expected_raw = (7 * m + 3) % POOL_CARDS;
        assert_eq!(spec.raw_step, expected_raw);
        assert!(
            spec.pick < PACK_SIZE - 1,
            "m={m} must not land on forced pick 14"
        );
        assert_eq!(spec.round * PACK_SIZE + spec.pick, spec.step);
        if spec.adjusted_from_forced {
            adjusted_indices.push(m);
        }
    }
    // D0.21 [L4]: only m = 8 changes, from step 14 to step 13.
    assert_eq!(adjusted_indices, vec![8]);
    let m8 = sample_pick_spec(8);
    assert_eq!(m8.raw_step, 14);
    assert_eq!(m8.step, 13);
    assert_eq!(m8.round, 0);
    assert_eq!(m8.pick, 13);
}

#[test]
fn checkpoint_header_enforces_if2_change2_head_updates() {
    let fixture = FixtureCheckpointFile::for_shakedown_fixture(
        "b1",
        32,
        HeadUpdates {
            pick: 0,
            value: 0,
            count: 32,
            pointer: 32,
            type_set: 32,
            split: 32,
        },
        &carddb_hex(),
    );
    let json = serde_json::to_string_pretty(&fixture).expect("serialize fixture");
    let parsed = CheckpointHeader::from_json_str(&json).expect("valid fixture header");
    assert_eq!(parsed.head_updates, fixture.head_updates);

    // Wrong key order inside head_updates must be rejected.
    let bad_order = json.replace(
        "\"pick\": 0,\n    \"value\": 0,",
        "\"value\": 0,\n    \"pick\": 0,",
    );
    assert!(
        CheckpointHeader::from_json_str(&bad_order).is_err(),
        "out-of-order head_updates keys must fail"
    );

    // Quoted integer inside head_updates must be rejected.
    let quoted_int = json.replace("\"pick\": 0,", "\"pick\": \"0\",");
    assert!(
        CheckpointHeader::from_json_str(&quoted_int).is_err(),
        "quoted head_updates integer must fail"
    );

    // Header key appearing after head_updates must be rejected.
    let reordered_header = format!(
        "{{\"schema\":\"{CHECKPOINT_SCHEMA_V1}\",\"model_identity\":\"m\",\"parameter_layout_sha256\":\"{}\",\"card_index_sha256\":\"{}\",\"cube_table_sha256\":\"{}\",\"card_database_hash\":\"{}\",\"training_math\":\"t\",\"segment\":\"b1\",\"update\":32,\"head_updates\":{{\"pick\":0,\"value\":0,\"count\":32,\"pointer\":32,\"type_set\":32,\"split\":32}},\"state_sha256\":\"{}\"}}",
        fixture.parameter_layout_sha256,
        fixture.card_index_sha256,
        fixture.cube_table_sha256,
        fixture.card_database_hash,
        fixture.state_sha256,
    );
    assert!(
        matches!(
            CheckpointHeader::from_json_str(&reordered_header),
            Err(CheckpointHeaderError::HeadUpdatesNotLast(_))
        ),
        "header key after head_updates must fail"
    );
}

#[test]
fn b1_page_labels_sample_plan_and_byte_identical_regeneration() {
    let (pool, pool_sha) = test_ordered_pool();
    let fixture = FixtureCheckpointFile::for_shakedown_fixture(
        "b1",
        32,
        HeadUpdates {
            pick: 0,
            value: 0,
            count: 32,
            pointer: 32,
            type_set: 32,
            split: 32,
        },
        &carddb_hex(),
    );
    let standin = StandInCheckpoint::from_fixture(&fixture);
    let header = fixture.header();

    let (data_a, html_a) = render_owner_view_page(
        "b1-u0032",
        PickerMode::UniformFixture,
        &pool,
        &pool_sha,
        &header,
        &standin,
    )
    .expect("render b1-u0032 first pass");
    let (data_b, html_b) = render_owner_view_page(
        "b1-u0032",
        PickerMode::UniformFixture,
        &pool,
        &pool_sha,
        &header,
        &standin,
    )
    .expect("render b1-u0032 second pass");

    // Acceptance 1: byte-identical regeneration from checkpoint and seed.
    assert_eq!(data_a, data_b);
    assert_eq!(html_a.as_bytes(), html_b.as_bytes());
    assert!(
        !html_a.contains('\u{2014}'),
        "no em-dash characters allowed"
    );
    assert!(!html_a.contains("\r\n"), "LF line endings only");

    // Exact D0.21 sample counts.
    assert_eq!(data_a.first_pick_pack_count, 800);
    let total_pack_appearances: usize = data_a
        .first_pick_ranked
        .iter()
        .chain(data_a.first_pick_low_count.iter())
        .map(|r| r.pack_count)
        .sum();
    assert_eq!(total_pack_appearances, 800 * PACK_SIZE);
    assert!(data_a
        .first_pick_ranked
        .iter()
        .all(|r| r.pack_count >= FIRST_PICK_MIN_PACKS));
    assert!(data_a
        .first_pick_low_count
        .iter()
        .all(|r| r.pack_count < FIRST_PICK_MIN_PACKS));

    assert_eq!(data_a.sample_picks.len(), 20);
    assert_eq!(data_a.sample_picks[8].spec.step, 13);
    assert_eq!(data_a.sample_picks[8].options.len(), 2);

    assert_eq!(data_a.sample_pools.len(), 20);
    for entry in &data_a.sample_pools {
        assert_eq!(entry.pool_cards.len(), POOL_CARDS);
        assert_eq!(entry.deck_cards.len(), 40);
        assert_eq!(
            entry.included_pool_cards.len() + usize::from(entry.basics_count),
            40
        );
    }

    assert_eq!(data_a.distributions.total_builds, 3_200);
    assert_eq!(data_a.distributions.basics_count_hist.len(), 41);
    assert_eq!(
        data_a.distributions.basics_count_hist.iter().sum::<usize>(),
        3_200
    );
    assert_eq!(
        data_a
            .distributions
            .type_set_size_hist
            .iter()
            .sum::<usize>(),
        3_200
    );

    // Acceptance 3: every head labelled from checkpoint.head_updates.
    assert_eq!(data_a.head_labels.len(), 6);
    assert_eq!(data_a.head_labels[0].label, "untrained"); // pick = 0
    assert_eq!(data_a.head_labels[1].label, "untrained"); // value = 0
    assert_eq!(
        data_a.head_labels[2].label,
        "trained for 32 updates under random play"
    ); // count = 32
    assert_eq!(
        data_a.head_labels[3].label,
        "trained for 32 updates under random play"
    ); // pointer = 32
    assert_eq!(
        data_a.head_labels[4].label,
        "trained for 32 updates under random play"
    ); // type_set = 32
    assert_eq!(
        data_a.head_labels[5].label,
        "trained for 32 updates under random play"
    ); // split = 32

    // Acceptance 4: b1 pages label picks "rendered with the uniform fixture".
    assert!(html_a.contains(UNIFORM_PICKS_LABEL));
    assert!(html_a.contains("rendered with the uniform fixture"));
    // Acceptance 5: every page carries the shakedown label.
    assert!(html_a.contains(SHAKEDOWN_LABEL));
    assert!(html_a.contains(SAMPLED_BUILDS_LABEL));
    assert!(html_a.contains(&LAW_BOUNDARY_NOTICE.replace('\'', "&#39;")));
    // Acceptance 2: no cross-page or dose-read reference.
    for other_run in SHAKEDOWN_OWNER_VIEW_RUNS {
        if other_run != "b1-u0032" {
            assert!(
                !html_a.contains(other_run),
                "b1-u0032 page must not reference another page {other_run}"
            );
        }
    }
    assert!(!html_a.contains("dose-d8"));
}

#[test]
fn k1_page_uses_learned_picker_labels_all_heads_and_regenerates_byte_identically() {
    let (pool, pool_sha) = test_ordered_pool();
    let fixture = FixtureCheckpointFile::for_shakedown_fixture(
        "k1",
        64,
        HeadUpdates {
            pick: 64,
            value: 64,
            count: 192,
            pointer: 192,
            type_set: 192,
            split: 192,
        },
        &carddb_hex(),
    );
    let standin = StandInCheckpoint::from_fixture(&fixture);
    let header = fixture.header();

    let (data_a, html_a) = render_owner_view_page(
        "k1-u0064",
        PickerMode::LearnedPickHead,
        &pool,
        &pool_sha,
        &header,
        &standin,
    )
    .expect("render k1-u0064 first pass");
    let (_data_b, html_b) = render_owner_view_page(
        "k1-u0064",
        PickerMode::LearnedPickHead,
        &pool,
        &pool_sha,
        &header,
        &standin,
    )
    .expect("render k1-u0064 second pass");

    assert_eq!(html_a.as_bytes(), html_b.as_bytes());
    assert!(
        !html_a.contains('\u{2014}'),
        "no em-dash characters allowed"
    );
    assert!(html_a.contains(SHAKEDOWN_LABEL));
    assert!(html_a.contains(SAMPLED_BUILDS_LABEL));
    // k1 page must NOT claim picks were rendered with the uniform fixture.
    assert!(!html_a.contains("rendered with the uniform fixture"));
    // Every head has count > 0, so no head is labelled untrained.
    assert!(data_a.head_labels.iter().all(|h| h.label != "untrained"));
    assert_eq!(
        data_a.head_labels[0].label,
        "trained for 64 updates under random play"
    );
    assert_eq!(
        data_a.head_labels[1].label,
        "trained for 64 updates under random play"
    );
    assert_eq!(
        data_a.head_labels[2].label,
        "trained for 192 updates under random play"
    );
    assert!(!html_a.contains("class=\"badge badge-untrained\""));

    // Verify first-pick ranking ordering: strictly non-increasing mean probability,
    // ties broken by registry id ascending.
    for pair in data_a.first_pick_ranked.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let lhs = a.exact_count_sum.unwrap() * (b.pack_count as u128);
        let rhs = b.exact_count_sum.unwrap() * (a.pack_count as u128);
        assert!(
            lhs > rhs || (lhs == rhs && a.card.id < b.card.id),
            "ranking out of order between {} and {}",
            a.card.id,
            b.card.id
        );
    }

    // Verify all 20 sample picks have non-uniform probabilities summing to ~1.0
    // and valid value_before_pick.
    assert_eq!(data_a.sample_picks.len(), 20);
    for entry in &data_a.sample_picks {
        assert!(entry.value_before_pick.is_some());
        let p_sum: f64 = entry.options.iter().map(|o| o.probability).sum();
        assert!((p_sum - 1.0).abs() < 1e-12, "probabilities sum to {p_sum}");
        assert_eq!(entry.options.iter().filter(|o| o.chosen).count(), 1);
    }
    assert_eq!(data_a.distributions.total_builds, 3_200);
}

#[test]
fn head_untrained_label_tracks_each_individual_head_zero_exactly() {
    // Test every single head independently set to 0 while the other 5 are non-zero.
    for zero_idx in 0..6 {
        let mut counts = [10u64; 6];
        counts[zero_idx] = 0;
        let hu = HeadUpdates {
            pick: counts[0],
            value: counts[1],
            count: counts[2],
            pointer: counts[3],
            type_set: counts[4],
            split: counts[5],
        };
        for (idx, (_name, _key, c)) in hu.entries().into_iter().enumerate() {
            let label = format_head_training_label(c);
            if idx == zero_idx {
                assert_eq!(label, "untrained");
            } else {
                assert_eq!(label, "trained for 10 updates under random play");
            }
        }
    }
}

#[test]
fn all_six_shakedown_run_ids_have_disjoint_seeds_and_correct_picker_modes() {
    let mut seen_seeds = BTreeSet::new();
    for &run_id in &SHAKEDOWN_OWNER_VIEW_RUNS {
        let mode = PickerMode::for_run_id(run_id).expect("valid shakedown run id");
        if run_id.starts_with("b1-") {
            assert_eq!(mode, PickerMode::UniformFixture);
        } else {
            assert_eq!(mode, PickerMode::LearnedPickHead);
        }
        for draft_idx in [0u64, 99, 100, 119, 120, 139, 200, 299] {
            let s = owner_view_draft_seed(run_id, draft_idx).expect("seed");
            assert!(
                seen_seeds.insert(s),
                "seed collision at {run_id} draft {draft_idx}"
            );
        }
    }
    // Sanity check basic land lookup still works in eternal-fixtures test build.
    assert!(card_id_by_name("Plains").is_some());
}

#[test]
fn all_six_shakedown_pages_render_independently_and_satisfy_acceptance_labels() {
    let (pool, pool_sha) = test_ordered_pool();
    let specs: [(&str, &str, u64, HeadUpdates); 6] = [
        (
            "b1-u0032",
            "b1",
            32,
            HeadUpdates {
                pick: 0,
                value: 0,
                count: 32,
                pointer: 32,
                type_set: 32,
                split: 32,
            },
        ),
        (
            "b1-u0064",
            "b1",
            64,
            HeadUpdates {
                pick: 0,
                value: 0,
                count: 64,
                pointer: 64,
                type_set: 64,
                split: 64,
            },
        ),
        (
            "b1-u0096",
            "b1",
            96,
            HeadUpdates {
                pick: 0,
                value: 0,
                count: 96,
                pointer: 96,
                type_set: 96,
                split: 96,
            },
        ),
        (
            "b1-u0128",
            "b1",
            128,
            HeadUpdates {
                pick: 0,
                value: 0,
                count: 128,
                pointer: 128,
                type_set: 128,
                split: 128,
            },
        ),
        (
            "k1-u0064",
            "k1",
            64,
            HeadUpdates {
                pick: 64,
                value: 64,
                count: 192,
                pointer: 192,
                type_set: 192,
                split: 192,
            },
        ),
        (
            "k1-u0128",
            "k1",
            128,
            HeadUpdates {
                pick: 128,
                value: 128,
                count: 256,
                pointer: 256,
                type_set: 256,
                split: 256,
            },
        ),
    ];

    let mut page_hashes = BTreeSet::new();
    for (run_id, segment, update, head_updates) in specs {
        let fixture = FixtureCheckpointFile::for_shakedown_fixture(
            segment,
            update,
            head_updates,
            &carddb_hex(),
        );
        let standin = StandInCheckpoint::from_fixture(&fixture);
        let header = fixture.header();
        let mode = PickerMode::for_run_id(run_id).expect("picker mode");
        let (data, html) =
            render_owner_view_page(run_id, mode, &pool, &pool_sha, &header, &standin)
                .expect("render page");

        assert_eq!(data.namespace, OWNER_VIEW_NAMESPACE);
        assert_eq!(data.run_id, run_id);
        assert_eq!(data.first_pick_pack_count, 800);
        assert_eq!(data.sample_picks.len(), 20);
        assert_eq!(data.sample_pools.len(), 20);
        assert_eq!(data.distributions.total_builds, 3_200);
        assert!(html.contains(SHAKEDOWN_LABEL));
        assert!(html.contains(SAMPLED_BUILDS_LABEL));
        assert!(!html.contains('\u{2014}'));
        assert!(!html.contains("dose-d8"));

        for &other in &SHAKEDOWN_OWNER_VIEW_RUNS {
            if other != run_id {
                assert!(
                    !html.contains(other),
                    "page {run_id} must not depend on or mention {other}"
                );
            }
        }
        if segment == "b1" {
            assert!(html.contains(UNIFORM_PICKS_LABEL));
            assert_eq!(data.head_labels[0].label, "untrained");
            assert_eq!(data.head_labels[1].label, "untrained");
            assert_eq!(
                data.head_labels[2].label,
                format!("trained for {update} updates under random play")
            );
        } else {
            assert!(!html.contains("rendered with the uniform fixture"));
            assert!(!html.contains("class=\"badge badge-untrained\""));
            assert!(data.head_labels.iter().all(|h| h.label != "untrained"));
        }

        let digest = sha2::Sha256::digest(html.as_bytes());
        assert!(
            page_hashes.insert(digest.to_vec()),
            "distinct run ids must produce distinct pages"
        );
    }
    assert_eq!(page_hashes.len(), 6);
}
