use super::*;
use crate::card_def::{card_id_by_name, KERNEL_CARDDB_HASH};
use crate::cube_draft_build_v1::card_index::{CardIndexV1, CubeTableV1};
use crate::cube_draft_build_v1::model::{ModelCardRowsV1, ModelV1};
use crate::cube_draft_v1::{
    basic_land_ids, deck_seed, derive_seed, generate_packs, run_draft, uniform_pickers, CubePool,
    GameCaps, GameEnd, SeedStream, PACK_SIZE, POOL_CARDS, SEATS,
};
use crate::cube_seat_v1::{
    run_seat_draft, AttemptOutcome, AttemptSpec, GamePilot, SeatPicker, UniformSeatPicker,
};
use crate::cube_training_driver_v1::{
    load_checkpoint_read_only_v1, write_marked_test_checkpoint_v1, PilotPin, PilotProvider,
    PilotSelection, RunConfig, RunOutcome, TrainingDriver, TrainingDriverError,
};
use crate::m4_sampler_v1::GRID_SIZE;
use crate::state::SplitMix64;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const FINAL_CUBE_TABLE_SHA256: &str =
    "778c12a127071fdc2170730e4f0e932d296f47b6ac2805b3eee331e00f5c46c0";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_temp_dir(tag: &str) -> PathBuf {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("target")
        .join("d16_test_tmp")
        .join(format!("{tag}_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir on N:");
    dir
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Loads the committed Vintage Cube list and gap record, sorts its registry
/// ids in ascending order, and constructs an ordered `CubePool` for testing.
fn test_ordered_pool() -> (CubePool, String) {
    let root = workspace_root();
    let cube_list = fs::read_to_string(root.join("data/cube_v1/mtgo_vintage_cube_20261007.json"))
        .expect("read cube list");
    let gaps = fs::read_to_string(root.join("data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"))
        .expect("read gaps");
    let raw_pool = CubePool::from_json_texts(&cube_list, &gaps, None).expect("build raw pool");
    let mut ids: Vec<u16> = raw_pool.ids().to_vec();
    ids.sort_unstable();
    ids.dedup();
    cube_pool_from_ordered_ids(&ids).expect("build ascending ordered pool")
}

fn carddb_hex() -> String {
    format!("{KERNEL_CARDDB_HASH:016x}")
}

/// Independent seed oracle: computes `seed(["owner-view", "draft", run, idx])`
/// directly via SHA-256 without calling any production seed helper.
fn oracle_owner_view_seed(run: &str, idx: u64) -> u64 {
    let compact = format!("[\"owner-view\",\"draft\",\"{run}\",{idx}]");
    let digest = Sha256::digest(compact.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(bytes) & ((1u64 << 63) - 1)
}

#[test]
fn seeds_match_python_deal_py_reference_values() {
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
        assert_eq!(oracle_owner_view_seed(run_id, draft_idx), expected_seed);
    }
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
        Segment::BuilderOnly,
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
    assert!(json.contains("\"card_database_sha256\""));
    assert!(!json.contains("\"card_database_hash\""));
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

    // Any key appearing after head_updates must be rejected.
    let reordered_header = format!(
        "{{\"schema\":\"{CHECKPOINT_SCHEMA_V1}\",\"model_identity\":\"m\",\"parameter_layout_sha256\":\"{}\",\"card_index_sha256\":\"{}\",\"cube_table_sha256\":\"{}\",\"card_database_sha256\":\"{}\",\"training_math\":\"t\",\"sampler\":\"softmax-cdf-splitmix64-v2\",\"segment\":\"builder_only\",\"run\":\"b1\",\"update\":32,\"head_updates\":{{\"pick\":0,\"value\":0,\"count\":32,\"pointer\":32,\"type_set\":32,\"split\":32}},\"state_sha256\":\"{}\"}}",
        fixture.parameter_layout_sha256,
        fixture.card_index_sha256,
        fixture.cube_table_sha256,
        fixture.card_database_sha256,
        fixture.state_sha256,
    );
    assert!(
        matches!(
            CheckpointHeader::from_json_str(&reordered_header),
            Err(CheckpointHeaderError::HeadUpdatesNotLast(_))
        ),
        "any key after head_updates must fail"
    );
}

#[test]
fn cli_refuses_real_checkpoint_and_requires_explicit_fixture_flag() {
    let dir = fresh_temp_dir("cli_refusal");
    let (pool, pool_sha) = test_ordered_pool();
    let pool_path = dir.join("ordered_pool.json");
    let mut pool_json = serde_json::to_string(pool.ids()).expect("serialize pool ids");
    pool_json.push('\n');
    fs::write(&pool_path, pool_json.as_bytes()).expect("write ordered pool");

    // 1. Write all 6 fixture checkpoints via the CLI.
    let fixtures_dir = dir.join("fixtures");
    cli::run_cli_args(&[
        "--write-fixture-checkpoints".to_owned(),
        fixtures_dir.to_string_lossy().into_owned(),
    ])
    .expect("write fixture checkpoints");

    // 2. Construct a real-style header JSON lacking fixture_standin.
    let sample_fixture = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::BuilderOnly,
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
    let real_ckpt_header = CheckpointHeader {
        model_identity: "m4-cube-draft-build/v1".to_owned(),
        ..sample_fixture.header()
    };
    let real_ckpt_path = dir.join("real_b1_u0032.checkpoint.json");
    fs::write(
        &real_ckpt_path,
        serde_json::to_string_pretty(&real_ckpt_header).expect("serialize real header"),
    )
    .expect("write real header");

    // 3. Without --fixture and without --cube-table, real path fails clearly.
    let out_missing_table = dir.join("refused_missing_table.html");
    let err = cli::run_cli_args(&[
        "--run".to_owned(),
        "b1-u0032".to_owned(),
        "--checkpoint".to_owned(),
        real_ckpt_path.to_string_lossy().into_owned(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_missing_table.to_string_lossy().into_owned(),
    ])
    .expect_err("real path requires --cube-table");
    assert!(err.contains("--cube-table"), "unexpected error: {err}");
    assert!(!out_missing_table.exists());

    // 4. Real-style checkpoint lacking fixture_standin even WITH --fixture MUST be refused.
    let out_refused_real_with_flag = dir.join("refused_real_with_flag.html");
    let err_with_flag = cli::run_cli_args(&[
        "--run".to_owned(),
        "b1-u0032".to_owned(),
        "--checkpoint".to_owned(),
        real_ckpt_path.to_string_lossy().into_owned(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--fixture".to_owned(),
        "--out".to_owned(),
        out_refused_real_with_flag.to_string_lossy().into_owned(),
    ])
    .expect_err("CLI must refuse real checkpoint without fixture_standin even with --fixture");
    assert!(
        err_with_flag.contains("fixture_standin") || err_with_flag.contains("invalid fixture"),
        "unexpected error: {err_with_flag}"
    );
    assert!(!out_refused_real_with_flag.exists());

    // 5. Fixture checkpoint WITHOUT --fixture MUST be refused (even if all real pins are passed).
    let fixture_b1_32 = fixtures_dir.join("b1-u0032.checkpoint.json");
    let out_no_flag = dir.join("refused_fixture_no_flag.html");
    let root = workspace_root();
    let err_fixture_real = cli::run_cli_args(&[
        "--run".to_owned(),
        "b1-u0032".to_owned(),
        "--checkpoint".to_owned(),
        fixture_b1_32.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        "0".repeat(64),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        root.join("data/card_semantics_v1/cube_table_v1.json")
            .to_string_lossy()
            .into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        root.join("data/cube_v1/mtgo_vintage_cube_20261007.json")
            .to_string_lossy()
            .into_owned(),
        "--out".to_owned(),
        out_no_flag.to_string_lossy().into_owned(),
    ])
    .expect_err("fixture checkpoint must be refused on real path");
    assert!(
        err_fixture_real.contains("failed to load checkpoint"),
        "unexpected error: {err_fixture_real}"
    );
    assert!(!out_no_flag.exists());

    // 6. Removed unpinned --cube / --gaps flags MUST be rejected.
    let err_cube_flag = cli::run_cli_args(&[
        "--run".to_owned(),
        "b1-u0032".to_owned(),
        "--checkpoint".to_owned(),
        fixture_b1_32.to_string_lossy().into_owned(),
        "--cube".to_owned(),
        "data/cube_v1/mtgo_vintage_cube_20261007.json".to_owned(),
        "--gaps".to_owned(),
        "data/cube_v1/mtgo_vintage_cube_20261007_gaps.json".to_owned(),
        "--fixture".to_owned(),
        "--out".to_owned(),
        dir.join("refused_cube.html").to_string_lossy().into_owned(),
    ])
    .expect_err("--cube flag must not be accepted");
    assert!(err_cube_flag.contains("unknown flag"));

    // 7. Mismatched checkpoint (b1-u0064 checkpoint passed to --run b1-u0032) MUST be refused.
    let fixture_b1_64 = fixtures_dir.join("b1-u0064.checkpoint.json");
    let err_ckpt_mismatch = cli::run_cli_args(&[
        "--run".to_owned(),
        "b1-u0032".to_owned(),
        "--checkpoint".to_owned(),
        fixture_b1_64.to_string_lossy().into_owned(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--fixture".to_owned(),
        "--out".to_owned(),
        dir.join("refused_mismatch.html")
            .to_string_lossy()
            .into_owned(),
    ])
    .expect_err("checkpoint 64 must be refused for --run b1-u0032");
    assert!(err_ckpt_mismatch.contains("checkpoint mismatch"));

    // 8. Mismatched --ordered-pool-sha256 MUST be refused.
    let wrong_sha = "0".repeat(64);
    let err_pool_sha = cli::run_cli_args(&[
        "--run".to_owned(),
        "b1-u0032".to_owned(),
        "--checkpoint".to_owned(),
        fixture_b1_32.to_string_lossy().into_owned(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        wrong_sha,
        "--fixture".to_owned(),
        "--out".to_owned(),
        dir.join("refused_pool_sha.html")
            .to_string_lossy()
            .into_owned(),
    ])
    .expect_err("wrong ordered-pool sha256 must be refused");
    assert!(err_pool_sha.contains("sha256 mismatch"));

    // 9. Valid fixture checkpoint + explicit --fixture + pinned --ordered-pool-sha256 SUCCEEDS.
    let out_ok = dir.join("b1-u0032.html");
    cli::run_cli_args(&[
        "--run".to_owned(),
        "b1-u0032".to_owned(),
        "--checkpoint".to_owned(),
        fixture_b1_32.to_string_lossy().into_owned(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha,
        "--fixture".to_owned(),
        "--out".to_owned(),
        out_ok.to_string_lossy().into_owned(),
    ])
    .expect("valid fixture CLI render succeeds");
    let rendered = fs::read_to_string(&out_ok).expect("read rendered HTML");
    assert!(rendered.contains(
        "Shakedown, under random play: the pilot plays uniformly at random. Weights are discarded."
    ));
    assert!(
        rendered.contains("rendered with the uniform fixture: every pick is uniform over the pack")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn page_identity_mismatch_and_pool_mismatch_are_refused() {
    let (pool, pool_sha) = test_ordered_pool();
    let fixture_b1_32 = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::BuilderOnly,
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
    let fixture_b1_64 = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::BuilderOnly,
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
        &carddb_hex(),
    );
    let fixture_b2_32 = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::BuilderOnly,
        "b2",
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
    let fixture_k1_64 = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::PickPhase,
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
    let standin_32 = StandInCheckpoint::from_fixture(&fixture_b1_32);
    let standin_64 = StandInCheckpoint::from_fixture(&fixture_b1_64);
    let standin_b2 = StandInCheckpoint::from_fixture(&fixture_b2_32);
    let standin_k64 = StandInCheckpoint::from_fixture(&fixture_k1_64);

    // 1. Undeclared run ids are refused (no zero/leading-zero/non-shakedown/M5 fallback).
    for undeclared in [
        "b0-u0032",
        "b01-u0032",
        "b1-u0000",
        "b2-u0016",
        "b2-u32",
        "k0-u0064",
        "k1-u0032",
        "k2-u0032",
        "m5-u0064",
        "custom",
    ] {
        assert!(matches!(
            resolve_page_spec(undeclared),
            Err(OwnerViewError::UndeclaredRunId(_))
        ));
        assert!(matches!(
            render_owner_view_page(
                undeclared,
                &pool,
                &pool_sha,
                &fixture_b1_32.header(),
                &standin_32
            ),
            Err(OwnerViewError::UndeclaredRunId(_))
        ));
    }

    // 1b. Restart run ids b<n>-u{0032,0064,0096,0128} and k<n>-u{0064,0128} resolve and bind
    // strictly to their matching training run and segment (D0.13 / D0.18).
    let b2_spec = resolve_page_spec("b2-u0032").expect("b2-u0032 resolves");
    assert_eq!(b2_spec.expected_segment, Segment::BuilderOnly);
    assert_eq!(b2_spec.expected_training_run, "b2");
    assert_eq!(b2_spec.expected_update, 32);
    assert_eq!(b2_spec.picker_mode, PickerMode::UniformFixture);

    let k2_spec = resolve_page_spec("k2-u0128").expect("k2-u0128 resolves");
    assert_eq!(k2_spec.expected_segment, Segment::PickPhase);
    assert_eq!(k2_spec.expected_training_run, "k2");
    assert_eq!(k2_spec.expected_update, 128);
    assert_eq!(k2_spec.picker_mode, PickerMode::LearnedPickHead);

    let (b2_data, b2_html) = render_owner_view_page(
        "b2-u0032",
        &pool,
        &pool_sha,
        &fixture_b2_32.header(),
        &standin_b2,
    )
    .expect("b2-u0032 renders with matching b2 checkpoint");
    assert_eq!(b2_data.run_id, "b2-u0032");
    assert_eq!(b2_data.checkpoint.run, "b2");
    assert!(b2_html.contains("b2-u0032"));
    assert!(matches!(
        render_owner_view_page(
            "b2-u0032",
            &pool,
            &pool_sha,
            &fixture_b1_32.header(),
            &standin_32
        ),
        Err(OwnerViewError::CheckpointMismatch { .. })
    ));

    // 2. Learned picker mode on a b1 page is refused.
    assert!(matches!(
        generate_owner_view_data(
            "b1-u0032",
            PickerMode::LearnedPickHead,
            &pool,
            &pool_sha,
            &fixture_b1_32.header(),
            &standin_32
        ),
        Err(OwnerViewError::PickerModeMismatch {
            expected: PickerMode::UniformFixture,
            got: PickerMode::LearnedPickHead,
            ..
        })
    ));

    // 3. Uniform picker mode on a k1 page is refused.
    assert!(matches!(
        generate_owner_view_data(
            "k1-u0064",
            PickerMode::UniformFixture,
            &pool,
            &pool_sha,
            &fixture_k1_64.header(),
            &standin_k64
        ),
        Err(OwnerViewError::PickerModeMismatch {
            expected: PickerMode::LearnedPickHead,
            got: PickerMode::UniformFixture,
            ..
        })
    ));

    // 4. Checkpoint update mismatch (b1 checkpoint 64 on b1-u0032) is refused.
    assert!(matches!(
        render_owner_view_page(
            "b1-u0032",
            &pool,
            &pool_sha,
            &fixture_b1_64.header(),
            &standin_64
        ),
        Err(OwnerViewError::CheckpointMismatch {
            expected_update: 32,
            got_update: 64,
            ..
        })
    ));

    // 5. Checkpoint training run id mismatch (run "b2" on b1-u0032) is refused.
    assert!(matches!(
        render_owner_view_page(
            "b1-u0032",
            &pool,
            &pool_sha,
            &fixture_b2_32.header(),
            &standin_b2
        ),
        Err(OwnerViewError::CheckpointMismatch { .. })
    ));

    // 6. Checkpoint segment mismatch (k1 checkpoint 64 on b1-u0064) is refused.
    assert!(matches!(
        render_owner_view_page(
            "b1-u0064",
            &pool,
            &pool_sha,
            &fixture_k1_64.header(),
            &standin_k64
        ),
        Err(OwnerViewError::CheckpointMismatch { .. })
    ));

    // 7. Pool sha256 mismatch is refused.
    let bad_sha = "f".repeat(64);
    assert!(matches!(
        render_owner_view_page(
            "b1-u0032",
            &pool,
            &bad_sha,
            &fixture_b1_32.header(),
            &standin_32
        ),
        Err(OwnerViewError::InvalidPool(_))
    ));

    // 8. Unsorted CubePool (cube-list order rather than ascending registry id) is refused.
    let root = workspace_root();
    let cube_list = fs::read_to_string(root.join("data/cube_v1/mtgo_vintage_cube_20261007.json"))
        .expect("read cube list");
    let gaps = fs::read_to_string(root.join("data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"))
        .expect("read gaps");
    let unsorted_pool = CubePool::from_json_texts(&cube_list, &gaps, None).expect("unsorted pool");
    let unsorted_sha = ordered_pool_sha256(unsorted_pool.ids());
    assert!(matches!(
        render_owner_view_page(
            "b1-u0032",
            &unsorted_pool,
            &unsorted_sha,
            &fixture_b1_32.header(),
            &standin_32
        ),
        Err(OwnerViewError::InvalidPool(_))
    ));
}

#[test]
fn independent_oracle_pins_packs_picks_pools_build_seeds_and_histograms() {
    let (pool, pool_sha) = test_ordered_pool();
    let fixture = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::BuilderOnly,
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

    let (data, html) = render_owner_view_page("b1-u0032", &pool, &pool_sha, &header, &standin)
        .expect("render b1-u0032");

    // --- Oracle 1: First-pick pack counts over drafts 0..100 ---
    let mut oracle_pack_counts: BTreeMap<u16, usize> = BTreeMap::new();
    for d in 0u64..100 {
        let seed = oracle_owner_view_seed("b1-u0032", d);
        let opened = generate_packs(&pool, seed).expect("oracle generate_packs");
        for pack in &opened[0] {
            for &id in pack {
                *oracle_pack_counts.entry(id).or_insert(0) += 1;
            }
        }
    }
    let mut rendered_pack_counts: BTreeMap<u16, usize> = BTreeMap::new();
    for row in data
        .first_pick_ranked
        .iter()
        .chain(data.first_pick_low_count.iter())
    {
        rendered_pack_counts.insert(row.card.id, row.pack_count);
    }
    for &id in pool.ids() {
        let expected = oracle_pack_counts.get(&id).copied().unwrap_or(0);
        let got = rendered_pack_counts.get(&id).copied().unwrap_or(usize::MAX);
        assert_eq!(got, expected, "pack count mismatch for card id {id}");
    }
    let top_row = &data.first_pick_ranked[0];
    let top_expected_packs = oracle_pack_counts[&top_row.card.id];
    assert!(html.contains(&format!(
        "<td class=\"num\">1</td><td>{}</td><td class=\"num\">{}</td>",
        top_row.card.name, top_row.card.id
    )));
    assert!(html.contains(&format!("<td class=\"num\">{top_expected_packs}</td></tr>")));
    let mut shifted_pack_counts: BTreeMap<u16, usize> = BTreeMap::new();
    for d in 1u64..101 {
        let seed = oracle_owner_view_seed("b1-u0032", d);
        let opened = generate_packs(&pool, seed).expect("shifted generate_packs");
        for pack in &opened[0] {
            for &id in pack {
                *shifted_pack_counts.entry(id).or_insert(0) += 1;
            }
        }
    }
    assert_ne!(oracle_pack_counts, shifted_pack_counts);

    // --- Oracle 2: 20 sample picks over drafts 100..120 (with L4 on m=8) ---
    for m in 0usize..20 {
        let draft_idx = 100 + m as u64;
        let draft_seed = oracle_owner_view_seed("b1-u0032", draft_idx);
        let chair = m % SEATS;
        let raw_step = (7 * m + 3) % POOL_CARDS;
        let step = if raw_step % PACK_SIZE == PACK_SIZE - 1 {
            raw_step - 1
        } else {
            raw_step
        };
        let round = step / PACK_SIZE;
        let pick = step % PACK_SIZE;
        let mut pickers = uniform_pickers(draft_seed);
        let draft = run_draft(&pool, draft_seed, &mut pickers).expect("oracle run_draft pick");
        let rec = &draft.picks[step * SEATS + chair];

        let got = &data.sample_picks[m];
        assert_eq!(got.draft_seed, draft_seed);
        assert_eq!(got.spec.draft_index, draft_idx);
        assert_eq!(got.spec.chair, chair);
        assert_eq!(got.spec.round, round);
        assert_eq!(got.spec.pick, pick);
        assert_eq!(got.spec.step, step);
        assert_eq!(got.chosen_card.id, rec.chosen);
        let got_pack_ids: BTreeSet<u16> = got.options.iter().map(|o| o.card.id).collect();
        let expected_pack_ids: BTreeSet<u16> = rec.pack.iter().copied().collect();
        assert_eq!(got_pack_ids, expected_pack_ids);
        let got_pool_so_far: Vec<u16> = got.pool_so_far.iter().map(|c| c.id).collect();
        assert_eq!(got_pool_so_far, draft.pools[chair][..step]);
    }
    for m in [0usize, 8, 19] {
        let entry = &data.sample_picks[m];
        assert!(html.contains(&format!(
            "draft_seed = <code>{}</code> | Chosen: <strong>",
            entry.draft_seed
        )));
    }
    assert!(html.contains("(adjusted from forced step 14 to step 13 per D0.21 L4)"));

    // --- Oracle 3: 20 sample pools and builds over drafts 120..140, k = 0 ---
    let mut oracle_builder = standin.make_learned_builder();
    for m in 0usize..20 {
        let draft_idx = 120 + m as u64;
        let draft_seed = oracle_owner_view_seed("b1-u0032", draft_idx);
        let chair = m % SEATS;
        let mut pickers = uniform_pickers(draft_seed);
        let draft = run_draft(&pool, draft_seed, &mut pickers).expect("oracle run_draft pool");
        let expected_build_seed =
            derive_seed(deck_seed(draft_seed, chair), SeedStream::DeckBuild, 0);
        let mut stream = SplitMix64::seed(expected_build_seed);
        let outcome = oracle_builder
            .build(&draft.pools[chair], &mut stream)
            .expect("oracle build");

        let got = &data.sample_pools[m];
        assert_eq!(got.draft_seed, draft_seed);
        assert_eq!(got.build_seed, expected_build_seed);
        let got_pool_ids: Vec<u16> = got.pool_cards.iter().map(|c| c.id).collect();
        assert_eq!(got_pool_ids, draft.pools[chair]);
        let got_deck_ids: Vec<u16> = got.deck_cards.iter().map(|c| c.id).collect();
        assert_eq!(got_deck_ids, outcome.deck.card_ids());
    }
    assert!(html.contains(&format!(
        "draft_seed = <code>{}</code> | build_seed = <code>{}</code>",
        data.sample_pools[0].draft_seed, data.sample_pools[0].build_seed
    )));

    // --- Oracle 4: 3,200 build seeds and histogram bins over drafts 200..300 ---
    let basics = basic_land_ids().expect("basic ids");
    let mut oracle_b_hist = vec![0usize; 41];
    let mut oracle_t_hist = [0usize; 6];
    let mut k0_only_b_hist = vec![0usize; 41];
    let mut shifted_b_hist = vec![0usize; 41];

    for d in 200u64..301 {
        let draft_seed = oracle_owner_view_seed("b1-u0032", d);
        let mut pickers = uniform_pickers(draft_seed);
        let draft = run_draft(&pool, draft_seed, &mut pickers).expect("oracle dist draft");
        for chair in 0..SEATS {
            let seat_pool = &draft.pools[chair];
            let mut k0_basics = 0usize;
            for k in 0u64..4 {
                let bseed = derive_seed(deck_seed(draft_seed, chair), SeedStream::DeckBuild, k);
                let mut stream = SplitMix64::seed(bseed);
                let outcome = oracle_builder
                    .build(seat_pool, &mut stream)
                    .expect("oracle dist build");
                let mut basic_count = 0usize;
                let mut type_mask = 0u8;
                for entry in &outcome.deck.entries {
                    if let Some(pos) = basics.iter().position(|&id| id == entry.id) {
                        basic_count += entry.count as usize;
                        type_mask |= 1u8 << pos;
                    }
                }
                if k == 0 {
                    k0_basics = basic_count;
                }
                if d < 300 {
                    oracle_b_hist[basic_count] += 1;
                    oracle_t_hist[type_mask.count_ones() as usize] += 1;
                }
                if d >= 201 {
                    shifted_b_hist[basic_count] += 1;
                }
            }
            if d < 300 {
                k0_only_b_hist[k0_basics] += 4;
            }
        }
    }

    assert_eq!(data.distributions.basics_count_hist, oracle_b_hist);
    assert_eq!(data.distributions.type_set_size_hist, oracle_t_hist);
    assert_ne!(k0_only_b_hist, oracle_b_hist);
    assert_ne!(shifted_b_hist, oracle_b_hist);
    let peak_b = oracle_b_hist
        .iter()
        .enumerate()
        .max_by_key(|&(_, &c)| c)
        .expect("peak B")
        .0;
    assert!(html.contains(&format!(
        "<tr><td class=\"num\">{peak_b}</td><td class=\"num\">{}</td>",
        oracle_b_hist[peak_b]
    )));
    let peak_t = oracle_t_hist
        .iter()
        .enumerate()
        .max_by_key(|&(_, &c)| c)
        .expect("peak |T|")
        .0;
    assert!(html.contains(&format!(
        "<tr><td class=\"num\">{peak_t}</td><td class=\"num\">{}</td>",
        oracle_t_hist[peak_t]
    )));
}

#[test]
fn b1_page_labels_sample_plan_and_byte_identical_regeneration() {
    let (pool, pool_sha) = test_ordered_pool();
    let fixture = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::BuilderOnly,
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

    let (data_a, html_a) = render_owner_view_page("b1-u0032", &pool, &pool_sha, &header, &standin)
        .expect("render b1-u0032 first pass");
    let (data_b, html_b) = render_owner_view_page("b1-u0032", &pool, &pool_sha, &header, &standin)
        .expect("render b1-u0032 second pass");

    assert_eq!(data_a, data_b);
    assert_eq!(html_a.as_bytes(), html_b.as_bytes());
    assert!(
        !html_a.contains('\u{2014}'),
        "no em-dash characters allowed"
    );
    assert!(!html_a.contains("\r\n"), "LF line endings only");

    assert!(html_a.contains(
        "Shakedown, under random play: the pilot plays uniformly at random. Weights are discarded."
    ));
    assert!(
        html_a.contains("rendered with the uniform fixture: every pick is uniform over the pack")
    );
    assert!(html_a.contains("sampled builds"));
    assert!(html_a.contains("The owner&#39;s reactions are a go, no-go or bug signal only. They never label cards, enter training data, tune a constant or select a checkpoint."));

    assert!(html_a.contains("<tr><td>pick</td><td><code>pick</code></td><td class=\"num\">0</td><td><span class=\"badge badge-untrained\">untrained</span></td></tr>"));
    assert!(html_a.contains("<tr><td>value</td><td><code>value</code></td><td class=\"num\">0</td><td><span class=\"badge badge-untrained\">untrained</span></td></tr>"));
    assert!(html_a.contains("<tr><td>count</td><td><code>count</code></td><td class=\"num\">32</td><td><span class=\"badge badge-trained\">trained for 32 updates under random play</span></td></tr>"));
    assert!(html_a.contains("<tr><td>pointer</td><td><code>pointer</code></td><td class=\"num\">32</td><td><span class=\"badge badge-trained\">trained for 32 updates under random play</span></td></tr>"));
    assert!(html_a.contains("<tr><td>type set</td><td><code>type_set</code></td><td class=\"num\">32</td><td><span class=\"badge badge-trained\">trained for 32 updates under random play</span></td></tr>"));
    assert!(html_a.contains("<tr><td>split</td><td><code>split</code></td><td class=\"num\">32</td><td><span class=\"badge badge-trained\">trained for 32 updates under random play</span></td></tr>"));

    for other_run in ["b1-u0064", "b1-u0096", "b1-u0128", "k1-u0064", "k1-u0128"] {
        assert!(!html_a.contains(other_run));
    }
    assert!(!html_a.contains("dose-d8"));
}

#[test]
fn k1_page_uses_learned_picker_labels_all_heads_and_regenerates_byte_identically() {
    let (pool, pool_sha) = test_ordered_pool();
    let fixture = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::PickPhase,
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

    let (data_a, html_a) = render_owner_view_page("k1-u0064", &pool, &pool_sha, &header, &standin)
        .expect("render k1-u0064 first pass");
    let (_data_b, html_b) = render_owner_view_page("k1-u0064", &pool, &pool_sha, &header, &standin)
        .expect("render k1-u0064 second pass");

    assert_eq!(html_a.as_bytes(), html_b.as_bytes());
    assert!(
        !html_a.contains('\u{2014}'),
        "no em-dash characters allowed"
    );
    assert!(html_a.contains(
        "Shakedown, under random play: the pilot plays uniformly at random. Weights are discarded."
    ));
    assert!(html_a.contains("sampled builds"));
    assert!(!html_a.contains("rendered with the uniform fixture"));
    assert!(!html_a.contains("class=\"badge badge-untrained\""));

    assert!(html_a.contains("<tr><td>pick</td><td><code>pick</code></td><td class=\"num\">64</td><td><span class=\"badge badge-trained\">trained for 64 updates under random play</span></td></tr>"));
    assert!(html_a.contains("<tr><td>value</td><td><code>value</code></td><td class=\"num\">64</td><td><span class=\"badge badge-trained\">trained for 64 updates under random play</span></td></tr>"));
    assert!(html_a.contains("<tr><td>count</td><td><code>count</code></td><td class=\"num\">192</td><td><span class=\"badge badge-trained\">trained for 192 updates under random play</span></td></tr>"));
    assert!(html_a.contains("<tr><td>pointer</td><td><code>pointer</code></td><td class=\"num\">192</td><td><span class=\"badge badge-trained\">trained for 192 updates under random play</span></td></tr>"));
    assert!(html_a.contains("<tr><td>type set</td><td><code>type_set</code></td><td class=\"num\">192</td><td><span class=\"badge badge-trained\">trained for 192 updates under random play</span></td></tr>"));
    assert!(html_a.contains("<tr><td>split</td><td><code>split</code></td><td class=\"num\">192</td><td><span class=\"badge badge-trained\">trained for 192 updates under random play</span></td></tr>"));

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
fn head_zero_rotation_reaches_each_heads_own_html_row() {
    let (pool, pool_sha) = test_ordered_pool();
    let head_meta: [(&str, &str, u64); 6] = [
        ("pick", "pick", 11),
        ("value", "value", 22),
        ("count", "count", 33),
        ("pointer", "pointer", 44),
        ("type set", "type_set", 55),
        ("split", "split", 66),
    ];

    for zero_idx in 0..6 {
        let mut counts = [11u64, 22, 33, 44, 55, 66];
        counts[zero_idx] = 0;
        let hu = HeadUpdates {
            pick: counts[0],
            value: counts[1],
            count: counts[2],
            pointer: counts[3],
            type_set: counts[4],
            split: counts[5],
        };
        let fixture = FixtureCheckpointFile::for_shakedown_fixture(
            Segment::BuilderOnly,
            "b1",
            32,
            hu,
            &carddb_hex(),
        );
        let json = serde_json::to_string_pretty(&fixture).expect("serialize fixture");
        let parsed = FixtureCheckpointFile::from_json_str(&json).expect("reparse fixture");
        let standin = StandInCheckpoint::from_fixture(&parsed);
        let (_data, html) =
            render_owner_view_page("b1-u0032", &pool, &pool_sha, &parsed.header(), &standin)
                .expect("render zero-rotation page");

        for (idx, &(display, key, nonzero_count)) in head_meta.iter().enumerate() {
            let expected_row = if idx == zero_idx {
                format!(
                    "<tr><td>{display}</td><td><code>{key}</code></td><td class=\"num\">0</td><td><span class=\"badge badge-untrained\">untrained</span></td></tr>"
                )
            } else {
                format!(
                    "<tr><td>{display}</td><td><code>{key}</code></td><td class=\"num\">{nonzero_count}</td><td><span class=\"badge badge-trained\">trained for {nonzero_count} updates under random play</span></td></tr>"
                )
            };
            assert!(
                html.contains(&expected_row),
                "rotation zero_idx={zero_idx}: missing exact HTML row {expected_row}"
            );
        }
        assert_eq!(
            html.matches("class=\"badge badge-untrained\"").count(),
            1,
            "rotation zero_idx={zero_idx}: exactly one head must be untrained"
        );
    }
}

#[test]
fn small_positive_probabilities_render_in_scientific_notation_never_zero() {
    let min_positive = 1.0f64 / (GRID_SIZE as f64); // 2^-53
    let formatted_min = format_probability(min_positive);
    assert!(
        formatted_min.contains("e-"),
        "expected scientific notation for 2^-53, got {formatted_min:?}"
    );
    assert!(!formatted_min.contains("0.000000"));
    assert!(!formatted_min.contains("(0.00%)"));

    let formatted_tiny = format_probability(2.5e-7);
    assert_eq!(formatted_tiny, "2.500e-7 (2.500e-5%)");

    assert_eq!(format_probability(0.0), "0.000000 (0.00%)");
    assert_eq!(format_probability(1.0 / 15.0), "0.066667 (6.67%)");

    let (pool, pool_sha) = test_ordered_pool();
    let fixture = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::BuilderOnly,
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
    let (mut data, _html) =
        render_owner_view_page("b1-u0032", &pool, &pool_sha, &fixture.header(), &standin)
            .expect("render page");
    data.sample_picks[0].options[0].probability = min_positive;
    let html = render_owner_view_html(&data);
    assert!(html.contains(&formatted_min));
}

#[test]
fn zero_appearance_eligible_card_is_listed_in_fewer_than_five_table() {
    let (pool, pool_sha) = test_ordered_pool();
    let fixture = FixtureCheckpointFile::for_shakedown_fixture(
        Segment::BuilderOnly,
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
    let (mut data, _html) =
        render_owner_view_page("b1-u0032", &pool, &pool_sha, &fixture.header(), &standin)
            .expect("render page");
    let synthetic_zero_card = summarize_card(pool.ids()[0]);
    data.first_pick_low_count.push(FirstPickRankRow {
        rank: data.first_pick_low_count.len() + 1,
        card: synthetic_zero_card.clone(),
        pack_count: 0,
        mean_probability: None,
        exact_count_sum: None,
    });
    let html = render_owner_view_html(&data);
    assert!(html.contains("<table class=\"low-count-table\">"));
    assert!(html.contains(&format!(
        "<td>{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td><td class=\"num\">N/A</td><td class=\"num\">0</td></tr>",
        synthetic_zero_card.name,
        synthetic_zero_card.id,
        synthetic_zero_card.colors,
        synthetic_zero_card.types
    )));
}

#[test]
fn all_six_shakedown_pages_render_independently_and_satisfy_acceptance_labels() {
    let (pool, pool_sha) = test_ordered_pool();
    let specs: [(&str, Segment, &str, u64, HeadUpdates); 6] = [
        (
            "b1-u0032",
            Segment::BuilderOnly,
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
            Segment::BuilderOnly,
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
            Segment::BuilderOnly,
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
            Segment::BuilderOnly,
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
            Segment::PickPhase,
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
            Segment::PickPhase,
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
    for (run_id, segment, training_run, update, head_updates) in specs {
        let fixture = FixtureCheckpointFile::for_shakedown_fixture(
            segment,
            training_run,
            update,
            head_updates,
            &carddb_hex(),
        );
        let standin = StandInCheckpoint::from_fixture(&fixture);
        let header = fixture.header();
        let (data, html) = render_owner_view_page(run_id, &pool, &pool_sha, &header, &standin)
            .expect("render page");

        assert_eq!(data.namespace, "owner-view");
        assert_eq!(data.run_id, run_id);
        assert_eq!(data.first_pick_pack_count, 800);
        assert_eq!(data.sample_picks.len(), 20);
        assert_eq!(data.sample_pools.len(), 20);
        assert_eq!(data.distributions.total_builds, 3_200);
        assert!(html.contains(
            "Shakedown, under random play: the pilot plays uniformly at random. Weights are discarded."
        ));
        assert!(html.contains("sampled builds"));
        assert!(!html.contains('\u{2014}'));
        assert!(!html.contains("dose-d8"));

        for &other in &[
            "b1-u0032", "b1-u0064", "b1-u0096", "b1-u0128", "k1-u0064", "k1-u0128",
        ] {
            if other != run_id {
                assert!(
                    !html.contains(other),
                    "page {run_id} must not depend on or mention {other}"
                );
            }
        }
        if segment == Segment::BuilderOnly {
            assert!(html.contains(
                "rendered with the uniform fixture: every pick is uniform over the pack"
            ));
            assert!(html.contains("<tr><td>pick</td><td><code>pick</code></td><td class=\"num\">0</td><td><span class=\"badge badge-untrained\">untrained</span></td></tr>"));
            assert!(html.contains("<tr><td>value</td><td><code>value</code></td><td class=\"num\">0</td><td><span class=\"badge badge-untrained\">untrained</span></td></tr>"));
            assert!(html.contains(&format!("<tr><td>count</td><td><code>count</code></td><td class=\"num\">{update}</td><td><span class=\"badge badge-trained\">trained for {update} updates under random play</span></td></tr>")));
        } else {
            assert!(!html.contains("rendered with the uniform fixture"));
            assert!(!html.contains("class=\"badge badge-untrained\""));
            assert!(html.contains(&format!("<tr><td>pick</td><td><code>pick</code></td><td class=\"num\">{update}</td><td><span class=\"badge badge-trained\">trained for {update} updates under random play</span></td></tr>")));
        }

        let digest = Sha256::digest(html.as_bytes());
        assert!(
            page_hashes.insert(digest.to_vec()),
            "distinct run ids must produce distinct pages"
        );
    }
    assert_eq!(page_hashes.len(), 6);
    assert!(card_id_by_name("Plains").is_some());
}

struct FastDeterministicPilot;

impl GamePilot for FastDeterministicPilot {
    fn play(&self, spec: &AttemptSpec<'_>, _caps: &GameCaps) -> AttemptOutcome {
        let deck_key = spec.decks[0]
            .card_ids()
            .into_iter()
            .fold(spec.action_seed, |key, id| {
                key.rotate_left(11)
                    .wrapping_mul(0x9e37_79b9)
                    .wrapping_add(u64::from(id))
            });
        AttemptOutcome {
            decisions: 3,
            physical_decisions: Some(4),
            policy_steps: Some(5),
            end: GameEnd::Natural {
                winner_seat: Some(spec.seats[((deck_key ^ (deck_key >> 32)) % 2) as usize]),
                reason: "fixture".to_owned(),
            },
            last_action_source: None,
            first_player_seat: Some(spec.seats[0]),
            last_acting_seat: Some(spec.seats[0]),
            pilot_sample: None,
        }
    }
}

struct FastPilotProvider;

impl PilotProvider for FastPilotProvider {
    fn load_checkpoint(
        &self,
        _pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        Ok(Arc::new(FastDeterministicPilot))
    }
}

#[test]
fn real_d6_checkpoint_renders_k1_page_end_to_end_via_cli() {
    let dir = fresh_temp_dir("real_d6_k1_e2e");
    let root = workspace_root();
    let cube_list_path = root.join("data/cube_v1/mtgo_vintage_cube_20261007.json");
    let cube_table_path = root.join("data/card_semantics_v1/cube_table_v1.json");
    let cube_list_text = fs::read_to_string(&cube_list_path).expect("read cube list");

    // 1. Load the final 545-row card index and verified D1 cube table on main.
    let card_index = CardIndexV1::full_v1(&cube_list_text).expect("545-row card index");
    assert_eq!(card_index.len(), 545);
    let table = CubeTableV1::load_v1(&cube_table_path, FINAL_CUBE_TABLE_SHA256, &card_index)
        .expect("load final cube table");
    let rows = Arc::new(
        ModelCardRowsV1::with_registry_basics_v1(
            table.index().ids().to_vec(),
            table.vector_width(),
            table.scaled_rows().to_vec(),
            table.dense_columns().len(),
            table.dense_rows().to_vec(),
        )
        .expect("build model card rows"),
    );

    // 2. Write a canonical ascending declared pool JSON file excluding Brain Freeze
    //    (fenced by D0.4 while retaining its table and index row), plus an altered pool
    //    that adds Brain Freeze back in.
    let (full_pool, _) = test_ordered_pool();
    let brain_freeze_id = card_id_by_name("Brain Freeze").expect("Brain Freeze in registry");
    assert!(
        card_index.contains(brain_freeze_id),
        "Brain Freeze keeps its 545-row index entry"
    );
    let declared_ids: Vec<u16> = full_pool
        .ids()
        .iter()
        .copied()
        .filter(|&id| id != brain_freeze_id)
        .collect();
    assert_eq!(declared_ids.len() + 1, full_pool.ids().len());
    let (pool, pool_sha) =
        cube_pool_from_ordered_ids(&declared_ids).expect("build declared fenced pool");
    for &id in pool.ids() {
        assert!(card_index.contains(id));
    }
    let pool_path = dir.join("ordered_pool.json");
    let mut pool_json = serde_json::to_string(pool.ids()).expect("serialize pool ids");
    pool_json.push('\n');
    fs::write(&pool_path, pool_json.as_bytes()).expect("write ordered pool");

    let altered_pool_path = dir.join("altered_pool_with_brain_freeze.json");
    let mut altered_pool_json =
        serde_json::to_string(full_pool.ids()).expect("serialize altered pool ids");
    altered_pool_json.push('\n');
    fs::write(&altered_pool_path, altered_pool_json.as_bytes()).expect("write altered pool");

    // 3. Write a real k1 checkpoint with D6's TrainingDriver via the real b1 -> k1
    //    segment boundary (1 real b1 update, then start_pick_phase + 1 real k1 update).
    let init_model =
        ModelV1::initialise_v1(rows.rows(), rows.table_width(), rows.dense_width(), 4242)
            .expect("init model");
    let layout_pin: String = init_model
        .parameter_layout_sha256()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let make_cfg = |run_dir: PathBuf, run: &str, segment: Segment| -> RunConfig {
        let mut cfg = RunConfig::shakedown(run_dir, run, segment);
        cfg.update_range = [0, 1];
        cfg.label = "test".to_owned();
        cfg.namespace = "test".to_owned();
        cfg.pilot.initial = PilotSelection::Checkpoint {
            pin: PilotPin {
                checkpoint_sha256: "11".repeat(32),
                after_state_sha256: "22".repeat(32),
                card_database_sha256: carddb_hex(),
                table_sha256: FINAL_CUBE_TABLE_SHA256.to_owned(),
                training_math: "portable-math-v1".to_owned(),
                source_binary_sha256: "55".repeat(32),
                decoding: "uniform-index-splitmix64-v1".to_owned(),
            },
        };
        cfg.pins.card_index = Some(table.index_pin().to_owned());
        cfg.pins.cube_table = Some(table.file_sha256().to_owned());
        cfg.metadata.card_database_sha256 = carddb_hex();
        cfg.metadata.parameter_layout_sha256 = layout_pin.clone();
        cfg
    };

    let b1_root = dir.join("d6_b1");
    let mut b1_driver = TrainingDriver::open(
        make_cfg(b1_root, "b1", Segment::BuilderOnly),
        rows.clone(),
        4242,
    )
    .expect("open D6 b1 driver");
    assert_eq!(
        b1_driver
            .run_next_update(&pool, &FastPilotProvider)
            .expect("run 1 real b1 update"),
        RunOutcome::Published { update: 0 }
    );

    let driver_root = dir.join("d6_k1");
    let mut driver = TrainingDriver::start_pick_phase(
        &b1_driver,
        make_cfg(driver_root.clone(), "k1", Segment::PickPhase),
    )
    .expect("start D6 k1 pick phase");
    assert_eq!(
        driver
            .run_next_update(&pool, &FastPilotProvider)
            .expect("run 1 real k1 update"),
        RunOutcome::Published { update: 0 }
    );
    let update0_ckpt_path = driver_root.join("updates/update-0000/checkpoint.json");
    let (u1_ckpt, _, _) =
        load_checkpoint_read_only_v1(&update0_ckpt_path, &table).expect("D6 loader reads update 0");
    assert_eq!(u1_ckpt.segment, Segment::PickPhase);
    assert_eq!(u1_ckpt.run, "k1");
    assert_eq!(u1_ckpt.update, 1);
    assert_eq!(u1_ckpt.head_updates.pick, 1);
    assert_eq!(u1_ckpt.head_updates.value, 1);
    assert_eq!(u1_ckpt.head_updates.count, 2);

    // 4. Passing the update-1 checkpoint to --run k1-u0064 MUST be refused (update mismatch).
    let out_refused_u1 = dir.join("refused_u1.html");
    let err_u1 = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        update0_ckpt_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        u1_ckpt.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--card-index-sha256".to_owned(),
        table.index_pin().to_owned(),
        "--out".to_owned(),
        out_refused_u1.to_string_lossy().into_owned(),
    ])
    .expect_err("update 1 checkpoint must be refused for k1-u0064");
    assert!(err_u1.contains("checkpoint mismatch"), "got: {err_u1}");
    assert!(!out_refused_u1.exists());

    // 5. Use D6's own test-support helper to write a marked k1-u0064 checkpoint
    //    carrying the real D6 trained weights and D6's own canonical state_sha256.
    let k1_64_path = dir.join("k1_u0064.checkpoint.json");
    let k1_64_ckpt = write_marked_test_checkpoint_v1(
        &k1_64_path,
        &u1_ckpt,
        Segment::PickPhase,
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
    )
    .expect("D6 writes marked k1-u0064 test checkpoint");
    // Verify D6's read-only loader accepts it verbatim.
    let (verified_ckpt, _, _) =
        load_checkpoint_read_only_v1(&k1_64_path, &table).expect("D6 loader verifies k1-u0064");
    assert_eq!(verified_ckpt.state_sha256, k1_64_ckpt.state_sha256);

    // 6. Render k1-u0064 TWICE through the real CLI (NO --fixture) using D6 loader + D3 seats.
    let html_1_path = dir.join("k1-u0064-pass1.html");
    let html_2_path = dir.join("k1-u0064-pass2.html");
    let json_1_path = dir.join("k1-u0064-pass1.json");
    let base_args = |out_html: &PathBuf, out_json: Option<&PathBuf>| -> Vec<String> {
        let mut a = vec![
            "--run".to_owned(),
            "k1-u0064".to_owned(),
            "--checkpoint".to_owned(),
            k1_64_path.to_string_lossy().into_owned(),
            "--ordered-pool".to_owned(),
            pool_path.to_string_lossy().into_owned(),
            "--ordered-pool-sha256".to_owned(),
            pool_sha.clone(),
            "--cube-table".to_owned(),
            cube_table_path.to_string_lossy().into_owned(),
            "--cube-table-sha256".to_owned(),
            FINAL_CUBE_TABLE_SHA256.to_owned(),
            "--cube-list".to_owned(),
            cube_list_path.to_string_lossy().into_owned(),
            "--card-index-sha256".to_owned(),
            table.index_pin().to_owned(),
            "--checkpoint-state-sha256".to_owned(),
            k1_64_ckpt.state_sha256.clone(),
            "--out".to_owned(),
            out_html.to_string_lossy().into_owned(),
        ];
        if let Some(jp) = out_json {
            a.push("--json-out".to_owned());
            a.push(jp.to_string_lossy().into_owned());
        }
        a
    };

    cli::run_cli_args(&base_args(&html_1_path, Some(&json_1_path)))
        .expect("real k1-u0064 render pass 1");
    cli::run_cli_args(&base_args(&html_2_path, None)).expect("real k1-u0064 render pass 2");

    let bytes_1 = fs::read(&html_1_path).expect("read pass 1 HTML");
    let bytes_2 = fs::read(&html_2_path).expect("read pass 2 HTML");
    assert_eq!(
        bytes_1, bytes_2,
        "real k1-u0064 page must regenerate byte-identically"
    );

    let html = String::from_utf8(bytes_1).expect("UTF-8 HTML");
    assert!(!html.contains('\u{2014}'), "no em-dash characters");
    assert!(
        !html.contains("Stand-in notice"),
        "real page must not show stand-in banner"
    );
    assert!(!html.contains("rendered with the uniform fixture"));
    assert!(!html.contains("class=\"badge badge-untrained\""));
    assert!(html.contains(
        "Shakedown, under random play: the pilot plays uniformly at random. Weights are discarded."
    ));
    assert!(html.contains("sampled builds"));
    assert!(html.contains("<tr><td>pick</td><td><code>pick</code></td><td class=\"num\">64</td><td><span class=\"badge badge-trained\">trained for 64 updates under random play</span></td></tr>"));
    assert!(html.contains("<tr><td>value</td><td><code>value</code></td><td class=\"num\">64</td><td><span class=\"badge badge-trained\">trained for 64 updates under random play</span></td></tr>"));
    assert!(html.contains("<tr><td>count</td><td><code>count</code></td><td class=\"num\">192</td><td><span class=\"badge badge-trained\">trained for 192 updates under random play</span></td></tr>"));
    assert!(html.contains("<tr><td>pointer</td><td><code>pointer</code></td><td class=\"num\">192</td><td><span class=\"badge badge-trained\">trained for 192 updates under random play</span></td></tr>"));
    assert!(html.contains("<tr><td>type set</td><td><code>type_set</code></td><td class=\"num\">192</td><td><span class=\"badge badge-trained\">trained for 192 updates under random play</span></td></tr>"));
    assert!(html.contains("<tr><td>split</td><td><code>split</code></td><td class=\"num\">192</td><td><span class=\"badge badge-trained\">trained for 192 updates under random play</span></td></tr>"));

    let data: OwnerViewPageData =
        serde_json::from_str(&fs::read_to_string(&json_1_path).expect("read JSON"))
            .expect("parse OwnerViewPageData");
    assert!(!data.used_stand_in_seats);
    assert_eq!(data.first_pick_pack_count, 800);
    assert_eq!(data.sample_picks.len(), 20);
    assert_eq!(data.sample_pools.len(), 20);
    assert_eq!(data.distributions.total_builds, 3_200);

    // 7. Real-path CLI refusals (Findings 1, 2, 3, 4): every refusal checks both
    //    the error message and that no HTML output file was written.

    // 7a. Omitted --ordered-pool-sha256 in real mode MUST be refused before output.
    let out_missing_pool_pin = dir.join("refused_missing_pool_pin.html");
    let err_missing_pool_pin = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        k1_64_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        k1_64_ckpt.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_missing_pool_pin.to_string_lossy().into_owned(),
    ])
    .expect_err("omitted --ordered-pool-sha256 must be refused in real mode");
    assert!(
        err_missing_pool_pin.contains("--ordered-pool-sha256"),
        "got: {err_missing_pool_pin}"
    );
    assert!(!out_missing_pool_pin.exists());

    // 7b. Altered pool with fenced card Brain Freeze added MUST be refused against
    //     the declared pool pin before output (even though Brain Freeze is in the index).
    let out_altered_pool = dir.join("refused_altered_pool.html");
    let err_altered_pool = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        k1_64_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        k1_64_ckpt.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        altered_pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_altered_pool.to_string_lossy().into_owned(),
    ])
    .expect_err("altered pool with fenced card added must be refused");
    assert!(
        err_altered_pool.contains("ordered pool sha256 mismatch"),
        "got: {err_altered_pool}"
    );
    assert!(!out_altered_pool.exists());

    // 7c. Omitted --checkpoint-state-sha256 in real mode MUST be refused before output.
    let out_missing_state_pin = dir.join("refused_missing_state_pin.html");
    let err_missing_state_pin = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        k1_64_path.to_string_lossy().into_owned(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_missing_state_pin.to_string_lossy().into_owned(),
    ])
    .expect_err("omitted --checkpoint-state-sha256 must be refused in real mode");
    assert!(
        err_missing_state_pin.contains("--checkpoint-state-sha256"),
        "got: {err_missing_state_pin}"
    );
    assert!(!out_missing_state_pin.exists());

    // 7d. Wrong --checkpoint-state-sha256 in real mode MUST be refused before output.
    let out_wrong_state_pin = dir.join("refused_wrong_state_pin.html");
    let err_wrong_state_pin = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        k1_64_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        u1_ckpt.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_wrong_state_pin.to_string_lossy().into_owned(),
    ])
    .expect_err("wrong --checkpoint-state-sha256 must be refused");
    assert!(
        err_wrong_state_pin.contains("checkpoint state_sha256 mismatch"),
        "got: {err_wrong_state_pin}"
    );
    assert!(!out_wrong_state_pin.exists());

    // 7e. Tampered checkpoint field keeping the published state_sha256 MUST fail
    //     with D6's state digest error and write no HTML.
    let mut tampered_ckpt = k1_64_ckpt.clone();
    tampered_ckpt.head_updates.pick = 65;
    assert_eq!(tampered_ckpt.state_sha256, k1_64_ckpt.state_sha256);
    let tampered_path = dir.join("tampered_k1_u0064.checkpoint.json");
    fs::write(
        &tampered_path,
        serde_json::to_vec(&tampered_ckpt).expect("serialize tampered checkpoint"),
    )
    .expect("write tampered checkpoint");
    let out_tampered = dir.join("refused_tampered.html");
    let err_tampered = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        tampered_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        k1_64_ckpt.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_tampered.to_string_lossy().into_owned(),
    ])
    .expect_err("tampered checkpoint retaining published digest must be refused");
    assert!(
        err_tampered.contains("checkpoint schema or state digest differs"),
        "got: {err_tampered}"
    );
    assert!(!out_tampered.exists());

    // 7f. Reordering top-level JSON keys to place state_sha256 after head_updates
    //     MUST be refused on the same bytes before output.
    let k1_64_compact =
        serde_json::to_string(&k1_64_ckpt).expect("serialize compact k1-u0064 JSON");
    let state_kv = format!(",\"state_sha256\":\"{}\"", k1_64_ckpt.state_sha256);
    assert!(k1_64_compact.contains(&state_kv));
    let mut reordered_raw = k1_64_compact.replacen(&state_kv, "", 1);
    assert!(reordered_raw.ends_with('}'));
    let insert_at = reordered_raw.len() - 1;
    reordered_raw.insert_str(insert_at, &state_kv);
    let reordered_path = dir.join("reordered_k1_u0064.checkpoint.json");
    fs::write(&reordered_path, reordered_raw.as_bytes()).expect("write reordered checkpoint");
    let out_reordered = dir.join("refused_reordered.html");
    let err_reordered = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        reordered_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        k1_64_ckpt.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_reordered.to_string_lossy().into_owned(),
    ])
    .expect_err("reordered JSON placing state_sha256 after head_updates must be refused");
    assert!(
        err_reordered.contains("head_updates must be the final top-level key"),
        "got: {err_reordered}"
    );
    assert!(!out_reordered.exists());

    // 7g. Wrong run id, wrong segment, wrong table sha256, wrong index sha256.
    let wrong_run_path = dir.join("wrong_run.checkpoint.json");
    let wrong_run = write_marked_test_checkpoint_v1(
        &wrong_run_path,
        &k1_64_ckpt,
        Segment::PickPhase,
        "k2",
        64,
        k1_64_ckpt.head_updates,
    )
    .expect("write wrong-run test checkpoint");
    let out_wrong_run = dir.join("refused_run.html");
    let err_run = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        wrong_run_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        wrong_run.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_wrong_run.to_string_lossy().into_owned(),
    ])
    .expect_err("wrong checkpoint run id must be refused");
    assert!(err_run.contains("checkpoint mismatch"), "got: {err_run}");
    assert!(!out_wrong_run.exists());

    let wrong_seg_path = dir.join("wrong_seg.checkpoint.json");
    let wrong_seg = write_marked_test_checkpoint_v1(
        &wrong_seg_path,
        &k1_64_ckpt,
        Segment::BuilderOnly,
        "k1",
        64,
        k1_64_ckpt.head_updates,
    )
    .expect("write wrong-segment test checkpoint");
    let out_wrong_seg = dir.join("refused_seg.html");
    let err_seg = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        wrong_seg_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        wrong_seg.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_wrong_seg.to_string_lossy().into_owned(),
    ])
    .expect_err("wrong checkpoint segment must be refused");
    assert!(err_seg.contains("checkpoint mismatch"), "got: {err_seg}");
    assert!(!out_wrong_seg.exists());

    let out_wrong_table = dir.join("refused_table.html");
    let err_table_pin = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        k1_64_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        k1_64_ckpt.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha.clone(),
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        "0".repeat(64),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--out".to_owned(),
        out_wrong_table.to_string_lossy().into_owned(),
    ])
    .expect_err("wrong cube-table sha256 must be refused");
    assert!(err_table_pin.contains("cube table"), "got: {err_table_pin}");
    assert!(!out_wrong_table.exists());

    let out_wrong_index = dir.join("refused_index.html");
    let err_index_pin = cli::run_cli_args(&[
        "--run".to_owned(),
        "k1-u0064".to_owned(),
        "--checkpoint".to_owned(),
        k1_64_path.to_string_lossy().into_owned(),
        "--checkpoint-state-sha256".to_owned(),
        k1_64_ckpt.state_sha256.clone(),
        "--ordered-pool".to_owned(),
        pool_path.to_string_lossy().into_owned(),
        "--ordered-pool-sha256".to_owned(),
        pool_sha,
        "--cube-table".to_owned(),
        cube_table_path.to_string_lossy().into_owned(),
        "--cube-table-sha256".to_owned(),
        FINAL_CUBE_TABLE_SHA256.to_owned(),
        "--cube-list".to_owned(),
        cube_list_path.to_string_lossy().into_owned(),
        "--card-index-sha256".to_owned(),
        "0".repeat(64),
        "--out".to_owned(),
        out_wrong_index.to_string_lossy().into_owned(),
    ])
    .expect_err("wrong card-index sha256 must be refused");
    assert!(
        err_index_pin.contains("card index pin sha256 mismatch"),
        "got: {err_index_pin}"
    );
    assert!(!out_wrong_index.exists());

    let _ = fs::remove_dir_all(&dir);
}
