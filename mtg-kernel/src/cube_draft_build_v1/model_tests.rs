//! Unit tests of the D2 model on synthetic card rows. The tests that need
//! D1's real provisional index and table belong in
//! `tests/eternal/cube_draft_build_v1.rs`, which D1 creates.

use std::collections::HashMap;

use super::*;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROVISIONAL_ROWS: usize = 509;
const COMMITTED_WIDTH: usize = 947;
const COMMITTED_DENSE: usize = 56;

/// Small dims for the fast tests. d = 12 is not 56, so every width written
/// from d is exercised.
const SMALL_ROWS: usize = 80;
const SMALL_WIDTH: usize = 60;
const SMALL_DENSE: usize = 12;

const INIT_SEED: u64 = 0x4d34_4432_0000_0001;
const FILL_SEED: u64 = 0x4d34_4432_0000_0002;

/// Index rows of the basics, W, U, B, R, G: not in ascending row order, so
/// type-set means must sort them.
const BASIC_ROW_CHOICE: [usize; 5] = [7, 2, 11, 4, 9];

fn synthetic_id(row: usize) -> u16 {
    u16::try_from(100 + 3 * row).unwrap()
}

/// Card rows shaped like D1's: strictly ascending ids with gaps, a sparse
/// scaled table with values in [-1, 1] on a power-of-two grid, and dense rows
/// that are a fixed selection of the table's columns.
fn synthetic_rows(rows: usize, width: usize, dense_width: usize, seed: u64) -> ModelCardRowsV1 {
    let ids: Vec<u16> = (0..rows).map(synthetic_id).collect();
    let mut stream = SplitMix64::seed(seed);
    let scaled: Vec<f32> = (0..rows * width)
        .map(|_| {
            let x = stream.next_u64();
            if x.is_multiple_of(8) {
                (((x >> 8) % 9) as f32 - 4.0) / 4.0
            } else {
                0.0
            }
        })
        .collect();
    let mut dense = Vec::with_capacity(rows * dense_width);
    for row in 0..rows {
        let scaled_row = &scaled[row * width..(row + 1) * width];
        dense.extend((0..dense_width).map(|k| scaled_row[(k * 7) % width]));
    }
    let basics = BASIC_ROW_CHOICE.map(synthetic_id);
    ModelCardRowsV1::new_v1(ids, width, scaled, dense_width, dense, basics).unwrap()
}

fn small_rows() -> ModelCardRowsV1 {
    synthetic_rows(SMALL_ROWS, SMALL_WIDTH, SMALL_DENSE, 11)
}

fn small_model(seed: u64) -> ModelV1 {
    ModelV1::initialise_v1(SMALL_ROWS, SMALL_WIDTH, SMALL_DENSE, seed).unwrap()
}

fn provisional_rows() -> ModelCardRowsV1 {
    synthetic_rows(PROVISIONAL_ROWS, COMMITTED_WIDTH, COMMITTED_DENSE, 13)
}

fn provisional_model(seed: u64) -> ModelV1 {
    ModelV1::initialise_v1(PROVISIONAL_ROWS, COMMITTED_WIDTH, COMMITTED_DENSE, seed).unwrap()
}

/// The last layer of every head.
const LAST_LAYERS: [&str; 6] = [
    "pick.scorer.2.weight",
    "build.count_head.weight",
    "build.pointer_scorer.2.weight",
    "build.typeset_scorer.2.weight",
    "build.split_scorer.2.weight",
    "value.out.weight",
];

fn fill_draws(count: usize, stream: &mut SplitMix64, bound: f32) -> Vec<f32> {
    (0..count)
        .map(|_| uniform_draw_v1(stream.next_u64(), bound))
        .collect()
}

/// Fills the named tensors from a second stream.
fn fill_tensors(model: &mut ModelV1, names: &[&str], seed: u64, bound: f32) {
    let mut stream = SplitMix64::seed(seed);
    for name in names {
        let values = model.tensor_mut_v1(name).unwrap();
        let drawn = fill_draws(values.len(), &mut stream, bound);
        values.copy_from_slice(&drawn);
    }
}

/// Fills every value the initialiser sets to +0.0 (both P, every bias, every
/// last layer and the history columns) from a second stream, so every path of
/// the forward carries non-zero arithmetic.
fn fill_zero_initialised(model: &mut ModelV1, seed: u64, bound: f32) {
    let mut stream = SplitMix64::seed(seed);
    let specs = model.layout().to_vec();
    for spec in specs {
        let values = model.tensor_mut_v1(&spec.name).unwrap();
        match spec.init {
            InitRuleV1::Zero => {
                let drawn = fill_draws(values.len(), &mut stream, bound);
                values.copy_from_slice(&drawn);
            }
            InitRuleV1::Uniform {
                zero_columns: Some(columns),
                ..
            } => {
                let in_width = spec.shape[1];
                for row in values.chunks_exact_mut(in_width) {
                    let drawn = fill_draws(columns.len(), &mut stream, bound);
                    row[columns.clone()].copy_from_slice(&drawn);
                }
            }
            InitRuleV1::Uniform { .. } => {}
        }
    }
}

fn fill_history_columns(model: &mut ModelV1, seed: u64, bound: f32) {
    let mut stream = SplitMix64::seed(seed);
    for name in ["pick.state_encoder.0.weight", "value.0.weight"] {
        let values = model.tensor_mut_v1(name).unwrap();
        for row in values.chunks_exact_mut(STATE_INPUT_WIDTH_V1) {
            let drawn = fill_draws(HISTORY_COLUMNS_V1.len(), &mut stream, bound);
            row[HISTORY_COLUMNS_V1].copy_from_slice(&drawn);
        }
    }
}

/// A valid pick state at `step` from consecutive index ids starting at
/// `offset`: the pool, then the pack, then `history_len` history cards.
fn pick_state(step: usize, history_len: usize, offset: usize) -> PickStateV1 {
    let pick = step % PACK_SIZE;
    let round = step / PACK_SIZE;
    let pack_len = PACK_SIZE - pick;
    let id = |k: usize| synthetic_id(offset + k);
    PickStateV1 {
        pool: (0..step).map(id).collect(),
        pack: (step..step + pack_len).map(id).collect(),
        history: (step + pack_len..step + pack_len + history_len)
            .map(id)
            .collect(),
        step: u8::try_from(step).unwrap(),
        direction: if round == 1 { -1 } else { 1 },
    }
}

/// A 45-card pool from index rows `offset..offset + 45`, listed in a scrambled
/// order (the input order the forward must not depend on).
fn build_pool(offset: usize) -> Vec<u16> {
    let mut pool: Vec<u16> = (0..BUILD_POOL_CARDS_V1)
        .map(|k| synthetic_id(offset + (k * 17) % BUILD_POOL_CARDS_V1))
        .collect();
    pool.rotate_left(5);
    pool
}

fn split_at_step(order: &[u16], step: usize) -> (Vec<u16>, Vec<u16>) {
    (order[..step].to_vec(), order[step..].to_vec())
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn assert_all_positive_zero(values: &[f32], what: &str) {
    assert!(
        values.iter().all(|value| value.to_bits() == 0),
        "{what} is not +0.0 bit for bit: {values:?}"
    );
}

/// Digest of every tensor whose name does not start with `skip_prefix`.
fn digest_excluding(model: &ModelV1, skip_prefix: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    model.visit_tensors_v1(|tensor| {
        if tensor.name.starts_with(skip_prefix) {
            return;
        }
        digest.update(tensor.name.as_bytes());
        for value in tensor.values {
            digest.update(value.to_bits().to_le_bytes());
        }
    });
    digest.finalize().into()
}

// ---------------------------------------------------------------------------
// 1. Parameter counts
// ---------------------------------------------------------------------------

fn layout_count(rows: usize, width: usize, dense: usize) -> usize {
    parameter_layout_v1(rows, width, dense)
        .unwrap()
        .iter()
        .map(TensorSpecV1::element_count)
        .sum()
}

#[test]
fn parameter_counts_match_the_design() {
    assert_eq!(layout_count(545, 947, 56), 165_418);
    assert_eq!(layout_count(509, 947, 56), 164_266);
    assert_eq!(layout_count(545, 947, 61), 166_058);

    let model = ModelV1::initialise_v1(545, 947, 56, INIT_SEED).unwrap();
    assert_eq!(model.parameter_count(), 165_418);
    assert_eq!(model.parameter_count_in_set(ParameterSetV1::Build), 85_417);
    assert_eq!(model.parameter_count_in_set(ParameterSetV1::Pick), 60_480);
    assert_eq!(model.parameter_count_in_set(ParameterSetV1::Value), 19_521);

    for (width, dense) in [(1, 0), (100, 10), (2_000, 99)] {
        assert_eq!(
            layout_count(545, width, dense),
            127_946 + 32 * width + 128 * dense,
            "F = {width}, d = {dense}"
        );
    }

    // card_mlp.0's in-width follows d: 77 on the frozen table.
    let frozen = parameter_layout_v1(545, 947, 61).unwrap();
    for name in [
        "build_trunk.card_mlp.0.weight",
        "pick_trunk.card_mlp.0.weight",
    ] {
        let spec = frozen.iter().find(|spec| spec.name == name).unwrap();
        assert_eq!(spec.shape, vec![64, 77]);
    }
}

#[test]
fn layout_names_and_order_are_the_tables() {
    let names: Vec<String> = parameter_layout_v1(509, 947, 56)
        .unwrap()
        .into_iter()
        .map(|spec| spec.name)
        .collect();
    let expected = [
        "build_trunk.card_embedding.weight",
        "build_trunk.feature_projection.weight",
        "build_trunk.card_mlp.0.weight",
        "build_trunk.card_mlp.0.bias",
        "build_trunk.card_mlp.2.weight",
        "build_trunk.card_mlp.2.bias",
        "build.pool_encoder.0.weight",
        "build.pool_encoder.0.bias",
        "build.pool_encoder.2.weight",
        "build.pool_encoder.2.bias",
        "build.count_head.weight",
        "build.count_head.bias",
        "build.pointer_context.weight",
        "build.pointer_context.bias",
        "build.pointer_scorer.0.weight",
        "build.pointer_scorer.0.bias",
        "build.pointer_scorer.2.weight",
        "build.typeset_context.weight",
        "build.typeset_context.bias",
        "build.typeset_scorer.0.weight",
        "build.typeset_scorer.0.bias",
        "build.typeset_scorer.2.weight",
        "build.split_scorer.0.weight",
        "build.split_scorer.0.bias",
        "build.split_scorer.2.weight",
        "pick_trunk.card_embedding.weight",
        "pick_trunk.feature_projection.weight",
        "pick_trunk.card_mlp.0.weight",
        "pick_trunk.card_mlp.0.bias",
        "pick_trunk.card_mlp.2.weight",
        "pick_trunk.card_mlp.2.bias",
        "pick.state_encoder.0.weight",
        "pick.state_encoder.0.bias",
        "pick.state_encoder.2.weight",
        "pick.state_encoder.2.bias",
        "pick.scorer.0.weight",
        "pick.scorer.0.bias",
        "pick.scorer.2.weight",
        "value.0.weight",
        "value.0.bias",
        "value.2.weight",
        "value.2.bias",
        "value.out.weight",
        "value.out.bias",
    ];
    assert_eq!(names, expected);
}

// ---------------------------------------------------------------------------
// 2. Layout hash and parameter digest
// ---------------------------------------------------------------------------

#[test]
fn layout_hash_depends_on_names_and_shapes_only() {
    let first = small_model(INIT_SEED);
    let second = small_model(INIT_SEED + 1);
    assert_eq!(
        first.parameter_layout_sha256(),
        second.parameter_layout_sha256()
    );
    assert_ne!(first.parameter_sha256(), second.parameter_sha256());

    let specs = parameter_layout_v1(SMALL_ROWS, SMALL_WIDTH, SMALL_DENSE).unwrap();
    assert_eq!(layout_sha256_v1(&specs), first.parameter_layout_sha256());

    let mut renamed = specs.clone();
    renamed[5].name.push('x');
    assert_ne!(layout_sha256_v1(&renamed), layout_sha256_v1(&specs));

    let mut resized = specs.clone();
    resized[0].shape[0] += 1;
    assert_ne!(layout_sha256_v1(&resized), layout_sha256_v1(&specs));

    assert_ne!(
        layout_sha256_v1(&parameter_layout_v1(509, 947, 56).unwrap()),
        layout_sha256_v1(&parameter_layout_v1(545, 947, 56).unwrap())
    );

    let mut changed = first.clone();
    changed.tensor_mut_v1("value.2.bias").unwrap()[3] = 0.5;
    assert_eq!(
        changed.parameter_layout_sha256(),
        first.parameter_layout_sha256()
    );
    assert_ne!(changed.parameter_sha256(), first.parameter_sha256());
}

#[test]
fn layout_and_parameter_digest_bytes_are_the_stated_format() {
    let specs = vec![TensorSpecV1 {
        name: "ab".to_owned(),
        set: ParameterSetV1::Build,
        shape: vec![2, 3],
        init: InitRuleV1::Zero,
    }];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(b"ab");
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&3u64.to_be_bytes());
    bytes.extend_from_slice(&6u64.to_be_bytes());
    let expected: [u8; 32] = Sha256::digest(&bytes).into();
    assert_eq!(layout_sha256_v1(&specs), expected);

    // The parameter digest interleaves each tensor's header and its values.
    let model = small_model(INIT_SEED);
    let mut digest = Sha256::new();
    model.visit_tensors_v1(|tensor| {
        digest.update((tensor.name.len() as u32).to_be_bytes());
        digest.update(tensor.name.as_bytes());
        digest.update((tensor.shape.len() as u32).to_be_bytes());
        for dimension in tensor.shape {
            digest.update((*dimension as u64).to_be_bytes());
        }
        digest.update((tensor.values.len() as u64).to_be_bytes());
        for value in tensor.values {
            digest.update(value.to_bits().to_le_bytes());
        }
    });
    let expected: [u8; 32] = digest.finalize().into();
    assert_eq!(model.parameter_sha256(), expected);
}

// ---------------------------------------------------------------------------
// 3. Zero rules
// ---------------------------------------------------------------------------

#[test]
fn initialiser_zero_rules_and_bounds() {
    let model = provisional_model(INIT_SEED);
    let mut zero_tensors = Vec::new();
    for (spec, values) in model.layout().iter().zip(&model.values) {
        match &spec.init {
            InitRuleV1::Zero => {
                assert_all_positive_zero(values, &spec.name);
                zero_tensors.push(spec.name.clone());
            }
            InitRuleV1::Uniform {
                fan_in,
                zero_columns,
            } => {
                let bound = uniform_bound_v1(*fan_in);
                let in_width = spec.shape[1];
                let mut drawn_non_zero = false;
                for row in values.chunks_exact(in_width) {
                    for (column, value) in row.iter().enumerate() {
                        let zeroed = zero_columns
                            .as_ref()
                            .is_some_and(|columns| columns.contains(&column));
                        if zeroed {
                            assert_eq!(value.to_bits(), 0, "{} column {column}", spec.name);
                        } else {
                            assert!(value.abs() <= bound, "{}: {value} > {bound}", spec.name);
                            drawn_non_zero |= *value != 0.0;
                        }
                    }
                }
                assert!(drawn_non_zero, "{} drew only zeros", spec.name);
            }
        }
    }
    let mut expected_zero: Vec<String> = model
        .layout()
        .iter()
        .filter(|spec| spec.name.ends_with(".bias"))
        .map(|spec| spec.name.clone())
        .collect();
    expected_zero.extend(
        [
            "build_trunk.feature_projection.weight",
            "pick_trunk.feature_projection.weight",
        ]
        .map(str::to_owned),
    );
    expected_zero.extend(LAST_LAYERS.map(str::to_owned));
    expected_zero.sort();
    zero_tensors.sort();
    assert_eq!(zero_tensors, expected_zero);

    // The history columns are zero in exactly the two state encoders, and
    // nowhere else is a column zeroed.
    let zeroed_columns: Vec<(&str, Range<usize>)> = model
        .layout()
        .iter()
        .filter_map(|spec| match &spec.init {
            InitRuleV1::Uniform {
                zero_columns: Some(columns),
                ..
            } => Some((spec.name.as_str(), columns.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        zeroed_columns,
        vec![
            ("pick.state_encoder.0.weight", 128..192),
            ("value.0.weight", 128..192)
        ]
    );

    // Fan-ins: E uses the named constant (bound 0.25); every other drawn
    // weight its input width.
    for spec in model.layout() {
        if let InitRuleV1::Uniform { fan_in, .. } = spec.init {
            let expected = if spec.name.ends_with("card_embedding.weight") {
                EMBEDDING_INIT_FAN_IN_V1
            } else {
                spec.shape[1]
            };
            assert_eq!(fan_in, expected, "{}", spec.name);
        }
    }
    assert_eq!(uniform_bound_v1(EMBEDDING_INIT_FAN_IN_V1), 0.25);

    assert_eq!(
        provisional_model(INIT_SEED).parameter_sha256(),
        model.parameter_sha256()
    );
    assert_ne!(
        provisional_model(INIT_SEED + 1).parameter_sha256(),
        model.parameter_sha256()
    );
}

// ---------------------------------------------------------------------------
// 4. The exact draw rule
// ---------------------------------------------------------------------------

/// The stated rule, derived independently in f64: u = (x >> 40) / 2^24 and
/// t = 2u - 1 are exact in f64, and t times the f32 bound is exact in f64
/// (24 by 24 bits), so one rounding to f32 equals the one f32 multiply.
fn expected_draw(x: u64, fan_in: usize) -> f32 {
    let u = (x >> 40) as f64 / 16_777_216.0;
    let t = 2.0 * u - 1.0;
    let bound = (1.0f64 / (fan_in as f64).sqrt()) as f32;
    (t * f64::from(bound)) as f32
}

fn drawn_spec(name: &str, zero_columns: Option<Range<usize>>) -> TensorSpecV1 {
    TensorSpecV1 {
        name: name.to_owned(),
        set: ParameterSetV1::Build,
        shape: vec![2, 3],
        init: InitRuleV1::Uniform {
            fan_in: 3,
            zero_columns,
        },
    }
}

#[test]
fn draw_rule_is_exact_and_zero_tensors_draw_nothing() {
    let seed = 0x1234_5678_9abc_def0;
    let values = initial_values_v1(&[drawn_spec("t", None)], seed);
    let mut stream = SplitMix64::seed(seed);
    let expected: Vec<f32> = (0..6)
        .map(|_| expected_draw(stream.next_u64(), 3))
        .collect();
    assert_eq!(bits(&values[0]), bits(&expected));

    // Zeroed columns still draw: the other columns keep their values.
    let zeroed = initial_values_v1(&[drawn_spec("t", Some(1..2))], seed);
    for (index, (got, want)) in zeroed[0].iter().zip(&expected).enumerate() {
        if index % 3 == 1 {
            assert_eq!(got.to_bits(), 0);
        } else {
            assert_eq!(got.to_bits(), want.to_bits());
        }
    }

    // A zero tensor before a drawn one consumes no draw.
    let zero = TensorSpecV1 {
        name: "z".to_owned(),
        set: ParameterSetV1::Build,
        shape: vec![4, 5],
        init: InitRuleV1::Zero,
    };
    let with_zero = initial_values_v1(&[zero, drawn_spec("t", None)], seed);
    assert_all_positive_zero(&with_zero[0], "the zero tensor");
    assert_eq!(bits(&with_zero[1]), bits(&expected));

    // A second drawn tensor continues the same stream.
    let two = initial_values_v1(&[drawn_spec("a", None), drawn_spec("b", None)], seed);
    let next: Vec<f32> = (0..6)
        .map(|_| expected_draw(stream.next_u64(), 3))
        .collect();
    assert_eq!(bits(&two[1]), bits(&next));
}

#[test]
fn whole_layout_follows_the_draw_rule_in_layout_order() {
    let model = small_model(INIT_SEED);
    let mut stream = SplitMix64::seed(INIT_SEED);
    for (spec, values) in model.layout().iter().zip(&model.values) {
        let InitRuleV1::Uniform {
            fan_in,
            zero_columns,
        } = &spec.init
        else {
            continue;
        };
        let in_width = spec.shape[1];
        for (index, value) in values.iter().enumerate() {
            let drawn = expected_draw(stream.next_u64(), *fan_in);
            let zeroed = zero_columns
                .as_ref()
                .is_some_and(|columns| columns.contains(&(index % in_width)));
            let expected = if zeroed { 0.0f32 } else { drawn };
            assert_eq!(
                value.to_bits(),
                expected.to_bits(),
                "{} element {index}",
                spec.name
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The registry's basics
// ---------------------------------------------------------------------------

#[test]
fn registry_basics_bind_in_type_bit_order() {
    let basics = crate::cube_draft_v1::basic_land_ids().unwrap();
    let mut ids: Vec<u16> = basics.to_vec();
    ids.extend([1u16, 2, 3]);
    ids.sort_unstable();
    ids.dedup();
    let rows = ids.len();
    let card_rows = ModelCardRowsV1::with_registry_basics_v1(
        ids,
        2,
        vec![0.0f32; rows * 2],
        1,
        vec![0.0f32; rows],
    )
    .unwrap();
    // Type bit 1 << i (W, U, B, R, G) is the i-th of Plains, Island, Swamp,
    // Mountain, Forest.
    for (bit, row) in card_rows.basic_rows().iter().enumerate() {
        assert_eq!(card_rows.ids()[*row], basics[bit], "type bit {}", 1 << bit);
    }
    assert_eq!(
        crate::cube_draft_v1::BASIC_LAND_NAMES,
        ["Plains", "Island", "Swamp", "Mountain", "Forest"]
    );
}

// ---------------------------------------------------------------------------
// 5. Zero at initialisation
// ---------------------------------------------------------------------------

#[test]
fn every_output_is_positive_zero_at_initialisation() {
    let rows = provisional_rows();
    let model = provisional_model(INIT_SEED);

    let pick = model.pick_forward_v1(&rows, 4).unwrap();
    let states = [
        pick_state(0, 0, 20),   // first pick, empty pool and history
        pick_state(20, 37, 0),  // round 1, passing right
        pick_state(14, 0, 60),  // a 1-card pack, empty history
        pick_state(44, 100, 0), // the last pick, a 1-card pack
    ];
    for state in &states {
        let logits = pick.pick_logits_v1(state).unwrap();
        assert_eq!(logits.logits.len(), state.pack.len());
        assert_all_positive_zero(&logits.logits, "pick logits");
        assert_eq!(pick.value_v1(state).unwrap().to_bits(), 0, "value");
    }

    let build = model.build_forward_v1(&rows, 4).unwrap();
    let pool = build_pool(30);
    let context = build.build_context_v1(&pool).unwrap();
    let counts = context.count_logits_v1();
    assert_eq!(counts.len(), 41);
    assert_all_positive_zero(&counts, "count logits");

    for basics_count in [0usize, 17, 40] {
        let steps = DECK_CARDS_V1 - basics_count;
        for step in 0..steps {
            let (chosen, remaining) = split_at_step(&pool, step);
            let logits = context
                .pointer_logits_v1(basics_count, &chosen, &remaining)
                .unwrap();
            assert_eq!(logits.logits.len(), BUILD_POOL_CARDS_V1 - step);
            assert_all_positive_zero(&logits.logits, "pointer logits");
        }
        if basics_count == 0 {
            continue;
        }
        let chosen = &pool[..steps];
        let types = context.type_context_v1(basics_count, chosen).unwrap();
        let sets = types.typeset_logits_v1();
        assert_eq!(sets.len(), 31);
        assert_all_positive_zero(&sets, "type-set logits");
        for mask in 1u8..=31 {
            let split = types.split_logits_v1(mask).unwrap();
            assert_eq!(split.logits.len(), mask.count_ones() as usize);
            assert_all_positive_zero(&split.logits, "split logits");
        }
    }
}

// ---------------------------------------------------------------------------
// 6. The history changes nothing at initialisation
// ---------------------------------------------------------------------------

#[test]
fn history_is_inert_until_its_columns_move() {
    let rows = small_rows();
    let mut model = small_model(INIT_SEED);
    fill_tensors(
        &mut model,
        &["pick.scorer.2.weight", "value.out.weight", "value.out.bias"],
        FILL_SEED,
        0.5,
    );
    let state = pick_state(20, 10, 0);
    let mut other = state.clone();
    other.history = (60..75).map(synthetic_id).collect();
    assert_ne!(state.history, other.history);

    let forward = model.pick_forward_v1(&rows, 1).unwrap();
    let logits = forward.pick_logits_v1(&state).unwrap();
    assert!(logits.logits.iter().any(|logit| *logit != 0.0));
    assert_eq!(
        bits(&logits.logits),
        bits(&forward.pick_logits_v1(&other).unwrap().logits)
    );
    assert_eq!(
        forward.value_v1(&state).unwrap().to_bits(),
        forward.value_v1(&other).unwrap().to_bits()
    );
    // The history mean itself does reach x.
    assert_ne!(
        bits(&forward.state_input_v1(&state).unwrap()),
        bits(&forward.state_input_v1(&other).unwrap())
    );

    let mut moved = model.clone();
    fill_history_columns(&mut moved, FILL_SEED + 1, 0.5);
    let forward = moved.pick_forward_v1(&rows, 1).unwrap();
    assert_ne!(
        bits(&forward.pick_logits_v1(&state).unwrap().logits),
        bits(&forward.pick_logits_v1(&other).unwrap().logits)
    );
    assert_ne!(
        forward.value_v1(&state).unwrap().to_bits(),
        forward.value_v1(&other).unwrap().to_bits()
    );
}

// ---------------------------------------------------------------------------
// 7. Set and order invariance
// ---------------------------------------------------------------------------

fn filled_small_model() -> ModelV1 {
    let mut model = small_model(INIT_SEED);
    fill_zero_initialised(&mut model, FILL_SEED, 0.5);
    model
}

fn assert_candidates_match_input(logits: &CandidateLogitsV1, input: &[u16]) {
    assert!(logits.ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(logits.ids.len(), input.len());
    assert_eq!(logits.logits.len(), input.len());
    for (id, position) in logits.ids.iter().zip(&logits.input_positions) {
        assert_eq!(input[*position], *id);
    }
}

#[test]
fn outputs_depend_on_sets_not_input_order() {
    let rows = small_rows();
    let model = filled_small_model();

    let pick = model.pick_forward_v1(&rows, 2).unwrap();
    let state = pick_state(20, 25, 3);
    let mut permuted = state.clone();
    permuted.pack.reverse();
    permuted.pool.rotate_left(7);
    permuted.history.reverse();
    permuted.history.rotate_left(3);
    let original = pick.pick_logits_v1(&state).unwrap();
    let shuffled = pick.pick_logits_v1(&permuted).unwrap();
    assert_candidates_match_input(&original, &state.pack);
    assert_candidates_match_input(&shuffled, &permuted.pack);
    assert_eq!(original.ids, shuffled.ids);
    assert_eq!(bits(&original.logits), bits(&shuffled.logits));
    assert_ne!(original.input_positions, shuffled.input_positions);
    assert!(original.logits.iter().all(|logit| *logit != 0.0));
    assert_eq!(
        pick.value_v1(&state).unwrap().to_bits(),
        pick.value_v1(&permuted).unwrap().to_bits()
    );

    let build = model.build_forward_v1(&rows, 2).unwrap();
    let pool = build_pool(10);
    let mut pool_permuted = pool.clone();
    pool_permuted.reverse();
    let context = build.build_context_v1(&pool).unwrap();
    let context_permuted = build.build_context_v1(&pool_permuted).unwrap();
    assert_eq!(
        bits(context.pool_context()),
        bits(context_permuted.pool_context())
    );
    assert_eq!(
        bits(&context.count_logits_v1()),
        bits(&context_permuted.count_logits_v1())
    );

    let basics_count = 17;
    let (chosen, remaining) = split_at_step(&pool, 9);
    let mut chosen_permuted = chosen.clone();
    chosen_permuted.reverse();
    let mut remaining_permuted = remaining.clone();
    remaining_permuted.rotate_left(4);
    let pointer = context
        .pointer_logits_v1(basics_count, &chosen, &remaining)
        .unwrap();
    let pointer_permuted = context_permuted
        .pointer_logits_v1(basics_count, &chosen_permuted, &remaining_permuted)
        .unwrap();
    assert_candidates_match_input(&pointer, &remaining);
    assert_candidates_match_input(&pointer_permuted, &remaining_permuted);
    assert_eq!(pointer.ids, pointer_permuted.ids);
    assert_eq!(bits(&pointer.logits), bits(&pointer_permuted.logits));

    let chosen = &pool[..DECK_CARDS_V1 - basics_count];
    let mut chosen_permuted = chosen.to_vec();
    chosen_permuted.reverse();
    let types = context.type_context_v1(basics_count, chosen).unwrap();
    let types_permuted = context_permuted
        .type_context_v1(basics_count, &chosen_permuted)
        .unwrap();
    assert_eq!(
        bits(&types.typeset_logits_v1()),
        bits(&types_permuted.typeset_logits_v1())
    );
    let split = types.split_logits_v1(0b10110).unwrap();
    assert_eq!(split.type_bits, vec![2, 4, 16]);
    assert_eq!(split, types_permuted.split_logits_v1(0b10110).unwrap());
}

// ---------------------------------------------------------------------------
// 8. Reference forward in f64
// ---------------------------------------------------------------------------

/// A plain f64 reimplementation of the stated model, reading the same
/// tensors by name.
struct ReferenceV1 {
    tensors: HashMap<String, Vec<f64>>,
    ids: Vec<u16>,
    basics: [u16; 5],
    build_hidden: Vec<Vec<f64>>,
    pick_hidden: Vec<Vec<f64>>,
}

impl ReferenceV1 {
    fn new(model: &ModelV1, rows: &ModelCardRowsV1) -> Self {
        let mut tensors = HashMap::new();
        model.visit_tensors_v1(|tensor| {
            tensors.insert(
                tensor.name.to_owned(),
                tensor
                    .values
                    .iter()
                    .map(|value| f64::from(*value))
                    .collect(),
            );
        });
        let mut reference = Self {
            tensors,
            ids: rows.ids().to_vec(),
            basics: rows.basic_rows().map(|row| rows.ids()[row]),
            build_hidden: Vec::new(),
            pick_hidden: Vec::new(),
        };
        reference.build_hidden = (0..rows.rows())
            .map(|row| reference.card_hidden("build_trunk", rows, row))
            .collect();
        reference.pick_hidden = (0..rows.rows())
            .map(|row| reference.card_hidden("pick_trunk", rows, row))
            .collect();
        reference
    }

    fn tensor(&self, name: &str) -> &[f64] {
        &self.tensors[name]
    }

    fn linear(&self, layer: &str, bias: bool, input: &[f64]) -> Vec<f64> {
        let weight = self.tensor(&format!("{layer}.weight"));
        let out_width = weight.len() / input.len();
        (0..out_width)
            .map(|i| {
                let start = if bias {
                    self.tensor(&format!("{layer}.bias"))[i]
                } else {
                    0.0
                };
                start
                    + weight[i * input.len()..(i + 1) * input.len()]
                        .iter()
                        .zip(input)
                        .map(|(w, x)| w * x)
                        .sum::<f64>()
            })
            .collect()
    }

    fn tanh_layer(&self, layer: &str, input: &[f64]) -> Vec<f64> {
        self.linear(layer, true, input)
            .into_iter()
            .map(f64::tanh)
            .collect()
    }

    fn card_hidden(&self, trunk: &str, rows: &ModelCardRowsV1, row: usize) -> Vec<f64> {
        let embedding = self.tensor(&format!("{trunk}.card_embedding.weight"));
        let projection = self.tensor(&format!("{trunk}.feature_projection.weight"));
        let width = rows.table_width();
        let scaled = rows.scaled_row(row);
        let mut vector: Vec<f64> = (0..16)
            .map(|k| {
                embedding[row * 16 + k]
                    + (0..width)
                        .map(|j| projection[k * width + j] * f64::from(scaled[j]))
                        .sum::<f64>()
            })
            .collect();
        vector.extend(rows.dense_row(row).iter().map(|value| f64::from(*value)));
        let first = self.tanh_layer(&format!("{trunk}.card_mlp.0"), &vector);
        self.tanh_layer(&format!("{trunk}.card_mlp.2"), &first)
    }

    fn hidden(&self, trunk: TrunkV1, id: u16) -> &[f64] {
        let row = self
            .ids
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap();
        match trunk {
            TrunkV1::Build => &self.build_hidden[row],
            TrunkV1::Pick => &self.pick_hidden[row],
        }
    }

    fn mean(&self, trunk: TrunkV1, ids: &[u16]) -> Vec<f64> {
        let mut mean = vec![0.0; 64];
        for id in ids {
            for (slot, value) in mean.iter_mut().zip(self.hidden(trunk, *id)) {
                *slot += value;
            }
        }
        if !ids.is_empty() {
            for slot in &mut mean {
                *slot /= ids.len() as f64;
            }
        }
        mean
    }

    fn state_input(&self, state: &PickStateV1) -> Vec<f64> {
        let mut input = self.mean(TrunkV1::Pick, &state.pack);
        input.extend(self.mean(TrunkV1::Pick, &state.pool));
        input.extend(self.mean(TrunkV1::Pick, &state.history));
        let mut step = vec![0.0; 45];
        step[usize::from(state.step)] = 1.0;
        input.extend(step);
        input.push(f64::from(state.direction));
        input
    }

    fn score(&self, scorer: &str, context: &[f64], item: &[f64]) -> f64 {
        let mut input = context.to_vec();
        input.extend_from_slice(item);
        let hidden = self.tanh_layer(&format!("{scorer}.0"), &input);
        self.linear(&format!("{scorer}.2"), false, &hidden)[0]
    }

    fn pick_logits(&self, state: &PickStateV1) -> Vec<(u16, f64)> {
        let input = self.state_input(state);
        let first = self.tanh_layer("pick.state_encoder.0", &input);
        let state_hidden = self.tanh_layer("pick.state_encoder.2", &first);
        let mut pack = state.pack.clone();
        pack.sort_unstable();
        pack.iter()
            .map(|id| {
                let logit = self.score(
                    "pick.scorer",
                    &state_hidden,
                    self.hidden(TrunkV1::Pick, *id),
                );
                (*id, logit)
            })
            .collect()
    }

    fn value(&self, state: &PickStateV1) -> f64 {
        let input = self.state_input(state);
        let first = self.tanh_layer("value.0", &input);
        let second = self.tanh_layer("value.2", &first);
        self.linear("value.out", true, &second)[0]
    }

    fn pool_context(&self, pool: &[u16]) -> Vec<f64> {
        let first = self.tanh_layer("build.pool_encoder.0", &self.mean(TrunkV1::Build, pool));
        self.tanh_layer("build.pool_encoder.2", &first)
    }

    fn pointer_logits(
        &self,
        pool: &[u16],
        basics_count: usize,
        chosen: &[u16],
        remaining: &[u16],
    ) -> Vec<(u16, f64)> {
        let mut input = self.pool_context(pool);
        input.extend(self.mean(TrunkV1::Build, chosen));
        input.push(basics_count as f64 / 40.0);
        input.push(chosen.len() as f64 / 40.0);
        let query = self.tanh_layer("build.pointer_context", &input);
        let mut remaining = remaining.to_vec();
        remaining.sort_unstable();
        remaining
            .iter()
            .map(|id| {
                let logit = self.score(
                    "build.pointer_scorer",
                    &query,
                    self.hidden(TrunkV1::Build, *id),
                );
                (*id, logit)
            })
            .collect()
    }

    fn type_context(&self, pool: &[u16], basics_count: usize, chosen: &[u16]) -> Vec<f64> {
        let mut input = self.pool_context(pool);
        input.extend(self.mean(TrunkV1::Build, chosen));
        input.push(basics_count as f64 / 40.0);
        self.tanh_layer("build.typeset_context", &input)
    }

    fn typeset_logits(&self, context: &[f64]) -> Vec<f64> {
        (1..32usize)
            .map(|mask| {
                let members: Vec<u16> = (0..5)
                    .filter(|bit| mask & (1 << bit) != 0)
                    .map(|bit| self.basics[bit])
                    .collect();
                let mut item = self.mean(TrunkV1::Build, &members);
                item.push(members.len() as f64 / 5.0);
                self.score("build.typeset_scorer", context, &item)
            })
            .collect()
    }

    fn split_logits(&self, context: &[f64], mask: u8) -> Vec<f64> {
        (0..5)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| {
                self.score(
                    "build.split_scorer",
                    context,
                    self.hidden(TrunkV1::Build, self.basics[bit]),
                )
            })
            .collect()
    }
}

fn assert_close(got: f32, want: f64, what: &str) {
    let error = (f64::from(got) - want).abs();
    assert!(
        error <= 1e-5 * want.abs().max(1.0),
        "{what}: model {got}, reference {want}, error {error}"
    );
}

fn assert_candidates_close(got: &CandidateLogitsV1, want: &[(u16, f64)], what: &str) {
    assert_eq!(
        got.ids,
        want.iter().map(|entry| entry.0).collect::<Vec<_>>()
    );
    for (logit, (id, expected)) in got.logits.iter().zip(want) {
        assert_close(*logit, *expected, &format!("{what} card {id}"));
    }
}

#[test]
fn forward_agrees_with_an_f64_reference() {
    let rows = small_rows();
    let model = filled_small_model();
    let reference = ReferenceV1::new(&model, &rows);

    let pick = model.pick_forward_v1(&rows, 1).unwrap();
    for (row, h) in pick.hidden_table().values().chunks_exact(64).enumerate() {
        for (got, want) in h.iter().zip(&reference.pick_hidden[row]) {
            assert_close(*got, *want, &format!("pick H row {row}"));
        }
    }
    let mut magnitude: f64 = 0.0;
    for state in [
        pick_state(0, 0, 0),
        pick_state(20, 25, 3),
        pick_state(37, 30, 0),
    ] {
        let want = reference.pick_logits(&state);
        assert_candidates_close(&pick.pick_logits_v1(&state).unwrap(), &want, "pick");
        assert_close(
            pick.value_v1(&state).unwrap(),
            reference.value(&state),
            "value",
        );
        magnitude = want
            .iter()
            .fold(magnitude, |acc, entry| acc.max(entry.1.abs()));
    }
    assert!(
        magnitude > 1e-2,
        "the reference outputs are too small to test"
    );

    let build = model.build_forward_v1(&rows, 1).unwrap();
    let pool = build_pool(20);
    let context = build.build_context_v1(&pool).unwrap();
    let want_context = reference.pool_context(&pool);
    for (got, want) in context.pool_context().iter().zip(&want_context) {
        assert_close(*got, *want, "g");
    }
    let want_counts = reference.linear("build.count_head", true, &want_context);
    for (b, (got, want)) in context
        .count_logits_v1()
        .iter()
        .zip(&want_counts)
        .enumerate()
    {
        assert_close(*got, *want, &format!("count B = {b}"));
    }
    for basics_count in [0usize, 17, 40] {
        let steps = DECK_CARDS_V1 - basics_count;
        for step in [0, steps / 2, steps.saturating_sub(1)] {
            if step >= steps {
                continue;
            }
            let (chosen, remaining) = split_at_step(&pool, step);
            assert_candidates_close(
                &context
                    .pointer_logits_v1(basics_count, &chosen, &remaining)
                    .unwrap(),
                &reference.pointer_logits(&pool, basics_count, &chosen, &remaining),
                &format!("pointer B = {basics_count} j = {step}"),
            );
        }
        if basics_count == 0 {
            continue;
        }
        let chosen = &pool[..steps];
        let types = context.type_context_v1(basics_count, chosen).unwrap();
        let want_types = reference.type_context(&pool, basics_count, chosen);
        for (mask, (got, want)) in types
            .typeset_logits_v1()
            .iter()
            .zip(reference.typeset_logits(&want_types))
            .enumerate()
        {
            assert_close(*got, want, &format!("type set {}", mask + 1));
        }
        for mask in [1u8, 0b00110, 0b11001, 31] {
            let got = types.split_logits_v1(mask).unwrap();
            for (logit, want) in got
                .logits
                .iter()
                .zip(reference.split_logits(&want_types, mask))
            {
                assert_close(*logit, want, &format!("split mask {mask}"));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 9. Golden bits
// ---------------------------------------------------------------------------

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/cube_draft_build_v1/model_forward_goldens_v1.txt"
);

fn golden_model() -> ModelV1 {
    let mut model = provisional_model(INIT_SEED);
    fill_zero_initialised(&mut model, FILL_SEED, 0.5);
    model
}

fn push_candidates(lines: &mut Vec<String>, prefix: &str, logits: &CandidateLogitsV1) {
    for (id, logit) in logits.ids.iter().zip(&logits.logits) {
        lines.push(format!("{prefix} card {id} {:08x}", logit.to_bits()));
    }
}

/// Every golden output, one line each, as f32 bits.
fn golden_lines(model: &ModelV1, rows: &ModelCardRowsV1, threads: usize) -> Vec<String> {
    let mut lines = vec![
        format!("layout_sha256 {}", hex(&model.parameter_layout_sha256())),
        format!("parameter_sha256 {}", hex(&model.parameter_sha256())),
    ];
    let pick = model.pick_forward_v1(rows, threads).unwrap();
    lines.push(format!(
        "pick_hidden_sha256 {}",
        hex(&Sha256::digest(
            bits(pick.hidden_table().values())
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<u8>>()
        ))
    ));
    for (index, state) in [pick_state(0, 0, 40), pick_state(23, 61, 100)]
        .iter()
        .enumerate()
    {
        push_candidates(
            &mut lines,
            &format!("pick state {index}"),
            &pick.pick_logits_v1(state).unwrap(),
        );
        lines.push(format!(
            "value state {index} {:08x}",
            pick.value_v1(state).unwrap().to_bits()
        ));
    }

    let build = model.build_forward_v1(rows, threads).unwrap();
    lines.push(format!(
        "build_hidden_sha256 {}",
        hex(&Sha256::digest(
            bits(build.hidden_table().values())
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<u8>>()
        ))
    ));
    let pool = build_pool(200);
    let context = build.build_context_v1(&pool).unwrap();
    for (b, logit) in context.count_logits_v1().iter().enumerate() {
        lines.push(format!("count B {b} {:08x}", logit.to_bits()));
    }
    let basics_count = 17;
    let steps = DECK_CARDS_V1 - basics_count;
    for step in 0..steps {
        let (chosen, remaining) = split_at_step(&pool, step);
        push_candidates(
            &mut lines,
            &format!("pointer B {basics_count} step {step}"),
            &context
                .pointer_logits_v1(basics_count, &chosen, &remaining)
                .unwrap(),
        );
    }
    let types = context
        .type_context_v1(basics_count, &pool[..steps])
        .unwrap();
    for (mask, logit) in types.typeset_logits_v1().iter().enumerate() {
        lines.push(format!("typeset mask {} {:08x}", mask + 1, logit.to_bits()));
    }
    let split = types.split_logits_v1(0b10101).unwrap();
    for (bit, logit) in split.type_bits.iter().zip(&split.logits) {
        lines.push(format!("split mask 21 type {bit} {:08x}", logit.to_bits()));
    }
    lines
}

#[test]
fn forward_matches_the_recorded_golden_bits() {
    let rows = provisional_rows();
    let model = golden_model();
    let computed = golden_lines(&model, &rows, 4).join("\n") + "\n";
    let recorded = match std::fs::read_to_string(GOLDEN_PATH) {
        Ok(text) => text.replace("\r\n", "\n"),
        Err(error) => {
            eprintln!("--- computed goldens ---\n{computed}--- end ---");
            panic!(
                "golden file {GOLDEN_PATH} is missing ({error}); record it from the computed \
                 goldens printed above on an authorized x86-64 run, then check it on a second host"
            );
        }
    };
    if recorded != computed {
        let first_difference = recorded
            .lines()
            .zip(computed.lines())
            .position(|(a, b)| a != b);
        eprintln!("--- computed goldens ---\n{computed}--- end ---");
        panic!(
            "forward bits differ from {GOLDEN_PATH}: first differing line {first_difference:?}, \
             recorded {} lines, computed {} lines",
            recorded.lines().count(),
            computed.lines().count()
        );
    }
}

// ---------------------------------------------------------------------------
// 10. Threads
// ---------------------------------------------------------------------------

#[test]
fn outputs_do_not_depend_on_the_thread_count() {
    let rows = provisional_rows();
    let model = golden_model();
    for trunk in [TrunkV1::Build, TrunkV1::Pick] {
        let one = model.card_hidden_table(&rows, trunk, 1).unwrap();
        for threads in [2, 3, 8, PROVISIONAL_ROWS + 5] {
            let many = model.card_hidden_table(&rows, trunk, threads).unwrap();
            assert_eq!(
                bits(one.values()),
                bits(many.values()),
                "{trunk:?} {threads}"
            );
        }
    }
    assert_eq!(
        golden_lines(&model, &rows, 1),
        golden_lines(&model, &rows, 8)
    );
}

// ---------------------------------------------------------------------------
// 11. Trunk copy
// ---------------------------------------------------------------------------

#[test]
fn trunk_copy_copies_the_build_trunk_and_nothing_else() {
    let mut model = filled_small_model();
    let trunk_names = [
        "card_embedding.weight",
        "feature_projection.weight",
        "card_mlp.0.weight",
        "card_mlp.0.bias",
        "card_mlp.2.weight",
        "card_mlp.2.bias",
    ];
    for name in trunk_names {
        assert_ne!(
            bits(model.tensor_v1(&format!("build_trunk.{name}")).unwrap()),
            bits(model.tensor_v1(&format!("pick_trunk.{name}")).unwrap()),
            "{name} already equal before the copy"
        );
    }
    let others = digest_excluding(&model, "pick_trunk.");
    let before = model.clone();
    model.copy_build_trunk_to_pick_trunk();
    for name in trunk_names {
        assert_eq!(
            bits(model.tensor_v1(&format!("build_trunk.{name}")).unwrap()),
            bits(model.tensor_v1(&format!("pick_trunk.{name}")).unwrap()),
            "{name}"
        );
    }
    assert_eq!(digest_excluding(&model, "pick_trunk."), others);
    assert_eq!(
        model.parameter_layout_sha256(),
        before.parameter_layout_sha256()
    );

    // The pick trunk's H now equals the build trunk's.
    let rows = small_rows();
    assert_eq!(
        bits(
            model
                .card_hidden_table(&rows, TrunkV1::Build, 1)
                .unwrap()
                .values()
        ),
        bits(
            model
                .card_hidden_table(&rows, TrunkV1::Pick, 1)
                .unwrap()
                .values()
        )
    );
}

// ---------------------------------------------------------------------------
// 12. Input validation
// ---------------------------------------------------------------------------

fn assert_state_refused(forward: &PickForwardV1<'_>, state: &PickStateV1, what: &str) {
    assert!(
        matches!(
            forward.pick_logits_v1(state),
            Err(ModelErrorV1::PickState(_))
        ),
        "{what}: pick logits accepted"
    );
    assert!(
        matches!(forward.value_v1(state), Err(ModelErrorV1::PickState(_))),
        "{what}: value accepted"
    );
}

#[test]
fn invalid_pick_states_are_refused() {
    let rows = provisional_rows();
    let model = provisional_model(INIT_SEED);
    let forward = model.pick_forward_v1(&rows, 2).unwrap();

    let valid = pick_state(20, 10, 0);
    assert!(forward.pick_logits_v1(&valid).is_ok());

    let mut sixteen = pick_state(0, 0, 0);
    sixteen.pack.push(synthetic_id(300));
    assert_state_refused(&forward, &sixteen, "a 16-card pack");

    let mut late = pick_state(44, 0, 0);
    late.step = 45;
    assert_state_refused(&forward, &late, "step 45");

    let mut repeated = valid.clone();
    repeated.pool[3] = repeated.pool[4];
    assert_state_refused(&forward, &repeated, "a repeated pool card");

    let mut outside = valid.clone();
    outside.pack[0] = 101; // between synthetic ids 100 and 103
    assert_state_refused(&forward, &outside, "an id outside the index");

    let mut repeated_pack = valid.clone();
    repeated_pack.pack[1] = repeated_pack.pack[0];
    assert_state_refused(&forward, &repeated_pack, "a repeated pack card");

    let mut repeated_history = valid.clone();
    repeated_history.history[1] = repeated_history.history[0];
    assert_state_refused(&forward, &repeated_history, "a repeated history card");

    let mut empty = pick_state(14, 0, 0);
    empty.pack.clear();
    assert_state_refused(&forward, &empty, "an empty pack");

    let mut direction = valid.clone();
    direction.direction = 0;
    assert_state_refused(&forward, &direction, "direction 0");
    direction.direction = 1; // round 1 passes right
    assert_state_refused(&forward, &direction, "round 1 passing left");

    let mut short_pool = valid.clone();
    short_pool.pool.pop();
    assert_state_refused(&forward, &short_pool, "a pool shorter than the step");

    let mut wrong_pack = valid.clone();
    wrong_pack.pack.pop();
    assert_state_refused(&forward, &wrong_pack, "a pack of the wrong size");

    let mut overlap = valid.clone();
    overlap.history[0] = overlap.pool[0];
    assert_state_refused(&forward, &overlap, "a pool card in the history");

    let mut taken = valid.clone();
    taken.pack[0] = taken.pool[0];
    assert_state_refused(&forward, &taken, "a pool card in the pack");

    let long_history = pick_state(26, MAX_HISTORY_CARDS_V1 + 1, 0);
    assert_state_refused(&forward, &long_history, "a history over 238 cards");
    let full_history = pick_state(26, MAX_HISTORY_CARDS_V1, 0);
    assert!(forward.pick_logits_v1(&full_history).is_ok());

    // A card that wheels is in both the pack and the history.
    let mut wheel = valid.clone();
    wheel.history[0] = wheel.pack[0];
    assert!(forward.pick_logits_v1(&wheel).is_ok());
}

#[test]
fn invalid_build_steps_are_refused() {
    let rows = small_rows();
    let model = small_model(INIT_SEED);
    let build = model.build_forward_v1(&rows, 1).unwrap();
    let pool = build_pool(0);
    let refused = |result: Result<CandidateLogitsV1, ModelErrorV1>| {
        matches!(result, Err(ModelErrorV1::Build(_)))
    };

    assert!(build.build_context_v1(&pool[..44]).is_err());
    let mut repeated = pool.clone();
    repeated[1] = repeated[0];
    assert!(matches!(
        build.build_context_v1(&repeated),
        Err(ModelErrorV1::Build(_))
    ));
    let mut outside = pool.clone();
    outside[0] = 101;
    assert!(build.build_context_v1(&outside).is_err());

    let context = build.build_context_v1(&pool).unwrap();
    let (chosen, remaining) = split_at_step(&pool, 5);
    assert!(context.pointer_logits_v1(17, &chosen, &remaining).is_ok());
    assert!(refused(context.pointer_logits_v1(41, &chosen, &remaining)));
    // Step j = 5 is not under 40 - 36 = 4.
    assert!(refused(context.pointer_logits_v1(36, &chosen, &remaining)));
    // Lists that do not partition the pool.
    assert!(refused(context.pointer_logits_v1(
        17,
        &chosen,
        &remaining[1..]
    )));
    let mut both = remaining.clone();
    both[0] = chosen[0];
    assert!(refused(context.pointer_logits_v1(17, &chosen, &both)));
    let mut stranger = remaining.clone();
    stranger[0] = synthetic_id(70); // in the index, not in the pool
    assert!(refused(context.pointer_logits_v1(17, &chosen, &stranger)));

    assert!(context.type_context_v1(0, &pool[..40]).is_err());
    assert!(context.type_context_v1(17, &pool[..22]).is_err());
    assert!(context.type_context_v1(41, &[]).is_err());
    let types = context.type_context_v1(2, &pool[..38]).unwrap();
    assert!(types.split_logits_v1(0b00011).is_ok());
    assert!(types.split_logits_v1(0b00111).is_err());
    assert!(types.split_logits_v1(0).is_err());
    assert!(types.split_logits_v1(32).is_err());
    assert_eq!(types.typeset_logits_v1().len(), 31);
}

#[test]
fn shapes_threads_and_card_rows_are_checked() {
    let rows = small_rows();
    let model = small_model(INIT_SEED);
    assert!(model.card_hidden_table(&rows, TrunkV1::Build, 0).is_err());
    let other = ModelV1::initialise_v1(SMALL_ROWS, SMALL_WIDTH, SMALL_DENSE + 1, 1).unwrap();
    assert!(matches!(
        other.pick_forward_v1(&rows, 1),
        Err(ModelErrorV1::Shape(_))
    ));
    assert!(ModelV1::initialise_v1(0, 10, 2, 1).is_err());
    assert!(ModelV1::initialise_v1(10, 4, 5, 1).is_err());

    let ids: Vec<u16> = (0..8).map(synthetic_id).collect();
    let basics = [0, 1, 2, 3, 4].map(synthetic_id);
    let table = vec![0.0f32; 8 * 3];
    let dense = vec![0.0f32; 8 * 2];
    assert!(
        ModelCardRowsV1::new_v1(ids.clone(), 3, table.clone(), 2, dense.clone(), basics).is_ok()
    );
    let mut descending = ids.clone();
    descending.swap(0, 1);
    assert!(
        ModelCardRowsV1::new_v1(descending, 3, table.clone(), 2, dense.clone(), basics).is_err()
    );
    assert!(ModelCardRowsV1::new_v1(
        ids.clone(),
        3,
        table[1..].to_vec(),
        2,
        dense.clone(),
        basics
    )
    .is_err());
    let mut nan = table.clone();
    nan[4] = f32::NAN;
    assert!(ModelCardRowsV1::new_v1(ids.clone(), 3, nan, 2, dense.clone(), basics).is_err());
    let mut repeated_basic = basics;
    repeated_basic[4] = repeated_basic[0];
    assert!(ModelCardRowsV1::new_v1(
        ids.clone(),
        3,
        table.clone(),
        2,
        dense.clone(),
        repeated_basic
    )
    .is_err());
    let mut missing_basic = basics;
    missing_basic[2] = 101;
    assert!(ModelCardRowsV1::new_v1(ids, 3, table, 2, dense, missing_basic).is_err());
}
