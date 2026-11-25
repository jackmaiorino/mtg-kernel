use super::*;
use crate::cube_draft_build_v1::model::{HISTORY_COLUMNS_V1, STATE_INPUT_WIDTH_V1};
use crate::cube_draft_v1::{deck_seed, derive_seed, picker_seed, CubePool, SeedStream, SEATS};
use crate::cube_seat_v1::{run_seat_draft, SeatDraft};
use std::collections::BTreeSet;

use crate::m4_sampler_v1::golden_cases;

const CUBE_LIST: &str = include_str!("../../../data/cube_v1/mtgo_vintage_cube_20261007.json");
const CUBE_GAPS: &str = include_str!("../../../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json");

// Test forwards only. Nonzero logits make padding, option order, lost inputs
// and replay against the wrong weights observable.
struct Probe(f32);

fn sorted(input: &[u16]) {
    assert!(input.windows(2).all(|w| w[0] < w[1]));
}

impl Probe {
    fn scores(&self, ids: &[u16], context: f32) -> Vec<f32> {
        sorted(ids);
        ids.iter()
            .map(|&id| self.0 * (f32::from(id % 19) / 17.0 + context))
            .collect()
    }
}

impl SeatForward for Probe {
    fn pick_and_value(&self, input: &PickInputs<'_>) -> Result<(Vec<f32>, f32), SeatError> {
        for ids in [input.pack, input.pool, input.history] {
            sorted(ids);
        }
        let step = input.step.iter().position(|&x| x == 1.0).unwrap();
        assert_eq!(input.step.iter().sum::<f32>(), 1.0);
        assert_eq!(step, input.pool.len());
        assert_eq!(
            input.direction,
            if step / PACK_SIZE == 1 { -1.0 } else { 1.0 }
        );
        let context = step as f32
            + input.direction
            + f32::from(input.pool.first().copied().unwrap_or(0)) / 256.0
            + f32::from(input.history.first().copied().unwrap_or(0)) / 512.0;
        Ok((self.scores(input.pack, context), self.0 * context))
    }

    fn count(&self, pool: &[u16]) -> Result<[f32; 41], SeatError> {
        sorted(pool);
        Ok(std::array::from_fn(|i| self.0 * i as f32 / 41.0))
    }

    fn pointer(&self, input: &BuildInputs<'_>, options: &[u16]) -> Result<Vec<f32>, SeatError> {
        sorted(input.pool);
        sorted(input.chosen);
        assert!(options
            .iter()
            .all(|id| input.chosen.binary_search(id).is_err()));
        Ok(self.scores(options, input.chosen.len() as f32 + f32::from(input.basics)))
    }

    fn type_set(&self, input: &BuildInputs<'_>, masks: &[u16]) -> Result<Vec<f32>, SeatError> {
        sorted(input.chosen);
        assert_eq!(input.chosen.len() + usize::from(input.basics), 40);
        assert!(masks
            .iter()
            .all(|mask| mask.count_ones() <= u32::from(input.basics)));
        Ok(self.scores(masks, f32::from(input.basics)))
    }

    fn split(&self, input: &BuildInputs<'_>, bits: &[u16]) -> Result<Vec<f32>, SeatError> {
        Ok(self.scores(bits, f32::from(input.basics)))
    }
}

fn cube() -> CubePool {
    CubePool::from_json_texts(CUBE_LIST, CUBE_GAPS, None).unwrap()
}

fn cube_from_names(names: impl IntoIterator<Item = impl AsRef<str>>) -> CubePool {
    let cards: Vec<_> = names
        .into_iter()
        .map(|name| serde_json::json!({"name": name.as_ref()}))
        .collect();
    CubePool::from_json_texts(&serde_json::json!({"cards": cards}).to_string(), "{}", None).unwrap()
}

fn model_fixture(nonzero: bool) -> (CubePool, Arc<ModelSeatForward>) {
    let pool = cube();
    let mut ids = pool.ids().to_vec();
    ids.extend(basic_land_ids().unwrap());
    ids.sort_unstable();
    ids.dedup();
    // Exercise the concrete D2 forward independently of D1's provisional
    // table pin. Vary the inputs so the card scorers can distinguish cards.
    let features: Vec<_> = ids.iter().map(|&id| f32::from(id % 83) / 83.0).collect();
    let rows =
        ModelCardRowsV1::with_registry_basics_v1(ids, 1, features.clone(), 1, features).unwrap();
    let mut model =
        ModelV1::initialise_v1(rows.rows(), rows.table_width(), rows.dense_width(), 13).unwrap();
    if nonzero {
        for name in [
            "pick.scorer.2.weight",
            "value.out.weight",
            "value.out.bias",
            "build.count_head.weight",
            "build.count_head.bias",
            "build.pointer_scorer.2.weight",
            "build.typeset_scorer.2.weight",
            "build.split_scorer.2.weight",
        ] {
            for (index, value) in model.tensor_mut_v1(name).unwrap().iter_mut().enumerate() {
                *value = (1 + index % 29) as f32 / 512.0;
            }
        }
        for name in ["pick.state_encoder.0.weight", "value.0.weight"] {
            for (index, row) in model
                .tensor_mut_v1(name)
                .unwrap()
                .chunks_exact_mut(STATE_INPUT_WIDTH_V1)
                .enumerate()
            {
                for (column, value) in row[HISTORY_COLUMNS_V1].iter_mut().enumerate() {
                    *value = (1 + (7 * index + column) % 29) as f32 / 512.0;
                }
            }
        }
    }
    let forward = ModelSeatForward::new(Arc::new(model), Arc::new(rows), 1).unwrap();
    (pool, Arc::new(forward))
}

fn sibling(seed: u64, seat: usize, k: u64) -> SplitMix64 {
    SplitMix64::seed(derive_seed(deck_seed(seed, seat), SeedStream::DeckBuild, k))
}

fn draft(pool: &CubePool, seed: u64, params: Arc<dyn SeatForward>) -> SeatDraft {
    let mut seats: [Box<dyn SeatPicker>; SEATS] = std::array::from_fn(|_| {
        Box::new(LearnedSeatPicker::new(params.clone())) as Box<dyn SeatPicker>
    });
    run_seat_draft(pool, seed, &mut seats).unwrap()
}

fn assert_sampler_row(row: &DecisionRow) {
    let logits: Vec<_> = row
        .logits_bits
        .as_ref()
        .unwrap()
        .iter()
        .map(|&bits| f32::from_bits(bits))
        .collect();
    let word = recorded_word(row).unwrap();
    let sample = sample_v1(&logits, &vec![true; logits.len()], word).unwrap();
    assert_eq!(row.chosen, row.options[sample.selected_index]);
    assert_eq!(
        row.p_chosen_bits,
        Some((sample.counts[sample.selected_index] as f64 / GRID_SIZE as f64).to_bits())
    );
}

#[test]
fn shared_sampler_goldens_pass_through_decision_rows() {
    // The D2a fixture tests pin these records. D3 removes masked entries
    // before drawing, so retain their original option ids for comparison.
    let golden_draws = [
        0,
        GRID_SIZE / 32,
        GRID_SIZE / 7,
        GRID_SIZE / 2 - 1,
        GRID_SIZE / 2,
        3 * GRID_SIZE / 4,
        GRID_SIZE - 2,
        GRID_SIZE - 1,
    ];
    for case in golden_cases::cases() {
        let original_record = case.record();
        let (options, logits): (Vec<_>, Vec<_>) = case
            .logits
            .iter()
            .zip(&case.mask)
            .enumerate()
            .filter(|(_, (_, enabled))| **enabled)
            .map(|(index, (&logit, _))| (index as u16, logit))
            .unzip();
        let draws = golden_draws.map(|k| {
            let word = k << 11;
            let sample = sample_v1(&case.logits, &case.mask, word).unwrap();
            let decision =
                decide(DecisionKind::Pointer, &options, &logits, None, word, None).unwrap();
            assert_eq!(
                decision.row.p_chosen_bits,
                Some((sample.counts[sample.selected_index] as f64 / GRID_SIZE as f64).to_bits())
            );
            assert_eq!(
                decision.log_prob.to_bits(),
                sample.selected_log_probability.to_bits()
            );
            decision.row.chosen
        });
        let (_, record_after_hash) = original_record.split_once(' ').unwrap();
        let (recorded_draws, _) = record_after_hash.rsplit_once(' ').unwrap();
        assert_eq!(format!("{draws:?}"), recorded_draws, "{}", case.name);
    }
}

#[test]
fn forced_pick_consumes_nothing_and_skips_forward() {
    // An inconsistent step would fail in the probe forward.
    let view = SeatPickView {
        round: 0,
        pick: 14,
        direction: PassDirection::Left,
        pack: &[27],
        pool: &[],
        seen_not_taken: &[],
    };
    let mut rng = SplitMix64::seed(13);
    let result = LearnedSeatPicker::new(Arc::new(Probe(1.0)))
        .pick(&view, &mut rng)
        .unwrap();
    assert_eq!(result.index, 0);
    assert!(result.row.is_none());
    assert_eq!(rng.next_u64(), SplitMix64::seed(13).next_u64());
}

#[test]
fn malformed_forced_pick_is_rejected_without_drawing() {
    let mut picker = LearnedSeatPicker::new(Arc::new(Probe(1.0)));
    let mut rng = SplitMix64::seed(13);
    for (round, pick) in [(0, 13), (ROUNDS as u8, 14), (0, PACK_SIZE as u8)] {
        let view = SeatPickView {
            round,
            pick,
            direction: PassDirection::Left,
            pack: &[27],
            pool: &[],
            seen_not_taken: &[],
        };
        assert!(matches!(
            picker.pick(&view, &mut rng),
            Err(SeatError::Seat(_))
        ));
    }
    assert_eq!(rng.next_u64(), SplitMix64::seed(13).next_u64());
}

#[test]
fn mapping_and_permuted_sets_preserve_all_bits() {
    let params = Probe(1.0);
    let pool: Vec<_> = (0..28).rev().collect();
    let view = SeatPickView {
        round: 1,
        pick: 13,
        direction: PassDirection::Right,
        pack: &[90, 40],
        pool: &pool,
        seen_not_taken: &[71, 54, 63],
    };
    let first = pick_with(&params, &view, || Ok(0)).unwrap();
    assert_eq!(first.index, 1);
    let first_row = first.row.unwrap();
    assert_eq!(first_row.options, [40, 90]);
    assert_eq!(first_row.chosen, 40);
    let context = 28.0 - 1.0 + 54.0 / 512.0;
    let logits = params.scores(&[40, 90], context);
    let sample = sample_v1(&logits, &[true, true], 0).unwrap();
    assert_eq!(first_row.kind, DecisionKind::Pick);
    assert_eq!(first_row.sampler, "softmax-cdf-splitmix64-v2");
    assert_eq!(first_row.stream_values, [0]);
    assert_eq!(first_row.value_bits, Some(context.to_bits()));
    assert_eq!(
        first_row.logits_bits,
        Some(logits.into_iter().map(f32::to_bits).collect())
    );
    assert_eq!(
        first_row.p_chosen_bits,
        Some((sample.counts[0] as f64 / GRID_SIZE as f64).to_bits())
    );
    let pool = sorted_cards(&pool);
    let reordered = SeatPickView {
        pack: &[40, 90],
        pool: &pool,
        seen_not_taken: &[54, 63, 71],
        ..view
    };
    let second = pick_with(&params, &reordered, || Ok(0)).unwrap();
    assert_eq!(second.index, 0);
    assert_eq!(second.row.as_ref(), Some(&first_row));
    let json = serde_json::to_string(&first_row).unwrap();
    let keys = [
        "kind",
        "sampler",
        "options",
        "chosen",
        "stream_values",
        "logits_bits",
        "p_chosen_bits",
        "value_bits",
    ];
    let positions = keys.map(|key| json.find(&format!("\"{key}\":")).unwrap());
    assert!(positions.windows(2).all(|w| w[0] < w[1]));
}

fn assert_forward_logits(row: &DecisionRow, logits: &[f32]) {
    let bits: Vec<_> = logits.iter().map(|logit| logit.to_bits()).collect();
    assert_eq!(
        row.logits_bits.as_deref(),
        Some(bits.as_slice()),
        "{:?} logits differ from D2",
        row.kind
    );
}

#[test]
fn learned_pick_matches_direct_d2_with_history() {
    let (cube, params) = model_fixture(true);
    let forward = params.model.pick_forward_v1(&params.rows, 1).unwrap();
    let mut picker = LearnedSeatPicker::new(params.clone());
    let ids = cube.ids();
    let mut evaluate = |history: &[u16]| {
        let view = SeatPickView {
            round: 0,
            pick: 1,
            direction: PassDirection::Left,
            pack: &ids[1..PACK_SIZE],
            pool: &ids[..1],
            seen_not_taken: history,
        };
        let row = picker
            .pick(&view, &mut SplitMix64::seed(17))
            .unwrap()
            .row
            .unwrap();
        let state = PickStateV1 {
            pack: view.pack.to_vec(),
            pool: view.pool.to_vec(),
            history: history.to_vec(),
            step: 1,
            direction: 1,
        };
        let logits = forward.pick_logits_v1(&state).unwrap();
        assert_eq!(row.options, logits.ids);
        assert_forward_logits(&row, &logits.logits);
        assert_eq!(
            row.value_bits,
            Some(forward.value_v1(&state).unwrap().to_bits()),
            "pick value differs from D2"
        );
        row
    };
    let empty = evaluate(&[]);
    let remembered = evaluate(&ids[PACK_SIZE..PACK_SIZE + 1]);
    assert_ne!(empty.logits_bits, remembered.logits_bits);
    assert_ne!(empty.value_bits, remembered.value_bits);
}

#[test]
fn learned_build_matches_direct_d2_with_filtered_masks_and_split() {
    let (cube, params) = model_fixture(true);
    let pool = &cube.ids()[..POOL_CARDS];
    let forward = params.model.build_forward_v1(&params.rows, 1).unwrap();
    let context = forward.build_context_v1(pool).unwrap();
    let count_logits = context.count_logits_v1();
    let mut builder = LearnedSeatBuilder::new(params.clone());
    let basics = 3usize;
    let streams = (0..512).map(SplitMix64::seed).filter(|stream| {
        let mut preview = *stream;
        let sample = sample_v1(&count_logits, &[true; 41], preview.next_u64()).unwrap();
        sample.selected_index == basics
    });
    for mut stream in streams {
        let rows = builder.build(pool, &mut stream).unwrap().rows.unwrap();
        assert_eq!(usize::from(rows.count.chosen), basics);
        assert_forward_logits(&rows.count, &count_logits);
        assert_eq!(rows.pointer.len(), DECK_CARDS as usize - basics);

        let mut chosen = Vec::new();
        let mut remaining = sorted_cards(pool);
        for row in &rows.pointer {
            let logits = context
                .pointer_logits_v1(basics, &chosen, &remaining)
                .unwrap();
            assert_eq!(row.options, logits.ids);
            assert_forward_logits(row, &logits.logits);
            chosen.push(remaining.remove(remaining.binary_search(&row.chosen).unwrap()));
        }

        let types = context.type_context_v1(basics, &chosen).unwrap();
        let row = rows.type_set.unwrap();
        let feasible: Vec<u16> = (1u16..=31)
            .filter(|mask| mask.count_ones() <= basics as u32)
            .collect();
        assert!(feasible.windows(2).any(|pair| pair[1] > pair[0] + 1));
        assert_eq!(row.options, feasible);
        let logits = types.typeset_logits_v1();
        let selected_logits: Vec<_> = feasible
            .iter()
            .map(|&mask| logits[usize::from(mask) - 1])
            .collect();
        assert_forward_logits(&row, &selected_logits);

        if !rows.split.is_empty() {
            let split = types.split_logits_v1(row.chosen as u8).unwrap();
            assert_eq!(split.type_bits.len(), 2);
            assert!(split.type_bits[0] < split.type_bits[1]);
            assert!(split.logits.iter().all(|&logit| logit != 0.0));
            assert_ne!(split.logits[0].to_bits(), split.logits[1].to_bits());
            let bits: Vec<_> = split.type_bits.into_iter().map(u16::from).collect();
            assert_eq!(rows.split.len(), basics - bits.len());
            for row in &rows.split {
                assert_eq!(row.options, bits);
                assert_forward_logits(row, &split.logits);
            }
            return;
        }
    }
    panic!("fixture streams produced no split with three basics and two basic types");
}

#[test]
fn concrete_forward_rejects_invalid_pick_inputs() {
    let (cube, params) = model_fixture(false);
    let mut pack = sorted_cards(&cube.ids()[..PACK_SIZE]);
    let mut step = [0.0; POOL_CARDS];
    step[0] = 1.0;
    let valid = PickInputs {
        pack: &pack,
        pool: &[],
        history: &[],
        step,
        direction: 1.0,
    };
    assert!(params.pick_and_value(&valid).is_ok());
    for value in [1.0, 0.5, f32::NAN] {
        step[1] = value;
        let invalid = PickInputs { step, ..valid };
        assert!(matches!(
            params.pick_and_value(&invalid),
            Err(SeatError::Seat(_))
        ));
    }
    let invalid = PickInputs {
        step: [0.0; POOL_CARDS],
        ..valid
    };
    assert!(matches!(
        params.pick_and_value(&invalid),
        Err(SeatError::Seat(_))
    ));
    for direction in [0.0, 1.5, -1.5, f32::NAN] {
        let invalid = PickInputs { direction, ..valid };
        assert!(matches!(
            params.pick_and_value(&invalid),
            Err(SeatError::Seat(_))
        ));
    }

    pack.swap(0, 1);
    let invalid = PickInputs {
        pack: &pack,
        pool: &[],
        history: &[],
        step: std::array::from_fn(|i| if i == 0 { 1.0 } else { 0.0 }),
        direction: 1.0,
    };
    assert!(matches!(
        params.pick_and_value(&invalid),
        Err(SeatError::Seat(_))
    ));
}

#[test]
fn concrete_forward_rejects_invalid_build_options() {
    let (cube, params) = model_fixture(false);
    let pool = sorted_cards(&cube.ids()[..POOL_CARDS]);
    let input = BuildInputs {
        pool: &pool,
        chosen: &[],
        basics: 2,
    };
    let mut remaining = pool.clone();
    assert!(params.pointer(&input, &remaining).is_ok());
    remaining.swap(0, 1);
    assert!(matches!(
        params.pointer(&input, &remaining),
        Err(SeatError::Seat(_))
    ));

    let input = BuildInputs {
        chosen: &pool[..38],
        ..input
    };
    assert!(params.type_set(&input, &[1, 3, 8]).is_ok());
    for mask in [0, 32, 7] {
        assert!(matches!(
            params.type_set(&input, &[mask]),
            Err(SeatError::Seat(_))
        ));
    }
    assert!(params.split(&input, &[1, 4]).is_ok());
    for bits in [[4, 1], [1, 1], [1, 3], [0, 1], [1, 32], [1, 256]] {
        assert!(matches!(
            params.split(&input, &bits),
            Err(SeatError::Seat(_))
        ));
    }
}

#[test]
fn concrete_initializer_scores_every_menu_at_zero() {
    let (cube, params) = model_fixture(false);
    let view = SeatPickView {
        round: 0,
        pick: 0,
        direction: PassDirection::Left,
        pack: &cube.ids()[..PACK_SIZE],
        pool: &[],
        seen_not_taken: &[],
    };
    let pick = pick_with(params.as_ref(), &view, || Ok(0)).unwrap();
    assert_eq!(pick.row.as_ref().unwrap().value_bits, Some(0));
    let built = walk_build(
        params.as_ref(),
        &cube.ids()[..POOL_CARDS],
        |step, options, logits| {
            let word = match step {
                Step::Count => 1 << 63,
                Step::TypeSet => u64::MAX,
                _ => 0,
            };
            decide(step.kind(), options, logits, None, word, None)
        },
    )
    .unwrap();
    assert_eq!(built.rows.count.chosen, 20);
    assert_eq!(built.rows.pointer.len(), 20);
    assert_eq!(built.rows.type_set.as_ref().unwrap().chosen, 31);
    assert_eq!(built.rows.split.len(), 15);
    for row in pick
        .row
        .iter()
        .chain(std::iter::once(&built.rows.count))
        .chain(&built.rows.pointer)
        .chain(&built.rows.type_set)
        .chain(&built.rows.split)
    {
        assert!(row
            .logits_bits
            .as_ref()
            .unwrap()
            .iter()
            .all(|&logit| logit == 0));
        assert_sampler_row(row);
    }
}

#[test]
fn eight_seats_replay_all_picks_and_siblings() {
    let (cube, params) = model_fixture(true);
    let seed = 731;
    let draft = draft(&cube, seed, params.clone());
    let mut builder = LearnedSeatBuilder::new(params.clone());
    for chair in 0..SEATS {
        assert_eq!(draft.pick_rows[chair].len(), 42);
        let mut pool = Vec::new();
        let mut history = BTreeSet::new();
        let mut rows = draft.pick_rows[chair].iter();
        let mut shadow = SplitMix64::seed(picker_seed(seed, chair));
        for pick in draft.record.picks.iter().filter(|p| p.seat == chair) {
            if pick.pack.len() > 1 {
                let history: Vec<u16> = history.iter().copied().collect();
                let view = SeatPickView {
                    round: pick.round as u8,
                    pick: pick.pick as u8,
                    direction: if pick.round == 1 {
                        PassDirection::Right
                    } else {
                        PassDirection::Left
                    },
                    pool: &pool,
                    pack: &pick.pack,
                    seen_not_taken: &history,
                };
                let row = rows.next().unwrap();
                assert_eq!(row.chosen, pick.chosen);
                assert_eq!(row.stream_values, [shadow.next_u64()]);
                assert_sampler_row(row);
                replay_pick(row, params.as_ref(), &view).unwrap();
                if pick.pick == 0 {
                    let mut corrupt = row.clone();
                    corrupt.p_chosen_bits = row.p_chosen_bits.map(|bits| bits ^ 1);
                    assert!(replay_pick(&corrupt, params.as_ref(), &view).is_err());
                    let mut corrupt = row.clone();
                    corrupt.logits_bits.as_mut().unwrap()[0] ^= 1;
                    assert!(replay_pick(&corrupt, params.as_ref(), &view).is_err());
                    let mut corrupt = row.clone();
                    corrupt.value_bits = row.value_bits.map(|bits| bits ^ 1);
                    assert!(replay_pick(&corrupt, params.as_ref(), &view).is_err());
                }
            }
            history.extend(pick.pack.iter().copied());
            history.remove(&pick.chosen);
            pool.push(pick.chosen);
        }
        assert!(rows.next().is_none());
        assert_eq!(pool, draft.record.pools[chair]);
        let alone = builder.build(&pool, &mut sibling(seed, chair, 0)).unwrap();
        for k in [1, 0] {
            let outcome = builder.build(&pool, &mut sibling(seed, chair, k)).unwrap();
            outcome.deck.validate(&pool).unwrap();
            let rows = outcome.rows.unwrap();
            for row in std::iter::once(&rows.count)
                .chain(&rows.pointer)
                .chain(&rows.type_set)
                .chain(&rows.split)
            {
                assert_sampler_row(row);
            }
            replay_build(&rows, params.as_ref(), &pool).unwrap();
            let mut reverse = pool.clone();
            reverse.reverse();
            replay_build(&rows, params.as_ref(), &reverse).unwrap();
            if k == 0 {
                assert_eq!(outcome.deck, alone.deck);
                assert_eq!(Some(&rows), alone.rows.as_ref());
            }
            let mut corrupt = rows.clone();
            corrupt.count.logits_bits.as_mut().unwrap()[0] ^= 1;
            assert!(replay_build(&corrupt, params.as_ref(), &pool).is_err());
        }
    }
}

struct FirstCard;

impl SeatPicker for FirstCard {
    fn pick(&mut self, _: &SeatPickView<'_>, _: &mut SplitMix64) -> Result<PickOutcome, SeatError> {
        Ok(PickOutcome {
            index: 0,
            row: None,
        })
    }
}

fn privacy_draft(cube: &CubePool, params: Arc<dyn SeatForward>) -> SeatDraft {
    let mut seats: [Box<dyn SeatPicker>; SEATS] = std::array::from_fn(|chair| {
        if chair == 0 {
            Box::new(LearnedSeatPicker::new(params.clone())) as Box<dyn SeatPicker>
        } else {
            Box::new(FirstCard) as Box<dyn SeatPicker>
        }
    });
    run_seat_draft(cube, 981, &mut seats).unwrap()
}

#[test]
fn changing_an_unseen_card_in_another_pool_preserves_seat_rows() {
    let (cube, params) = model_fixture(true);
    let first = privacy_draft(&cube, params.clone());
    let seen: BTreeSet<_> = first
        .record
        .picks
        .iter()
        .filter(|p| p.seat == 0)
        .flat_map(|p| p.pack.iter().copied())
        .collect();
    let drafted: BTreeSet<_> = first.record.pools.iter().flatten().copied().collect();
    let hidden = drafted.difference(&seen).next().unwrap();
    let unused = cube.ids().iter().find(|id| !drafted.contains(*id)).unwrap();
    let mut names = cube.names().to_vec();
    names.swap(
        cube.ids().iter().position(|id| id == hidden).unwrap(),
        cube.ids().iter().position(|id| id == unused).unwrap(),
    );
    let changed = cube_from_names(names);
    let second = privacy_draft(&changed, params);
    assert_ne!(first.record.packs, second.record.packs);
    assert_ne!(first.record.pools[1..], second.record.pools[1..]);
    assert!(first.record.picks.iter().filter(|p| p.seat == 0).eq(second
        .record
        .picks
        .iter()
        .filter(|p| p.seat == 0)));
    assert_eq!(first.pick_rows[0], second.pick_rows[0]);
}

#[test]
fn one_and_eight_workers_produce_identical_rows() {
    let pool = cube();
    let params = Arc::new(Probe(0.5));
    let collect = |seed: u64| {
        let draft = draft(&pool, seed, params.clone());
        let mut builder = LearnedSeatBuilder::new(params.clone());
        let builds: Vec<_> = draft
            .record
            .pools
            .iter()
            .enumerate()
            .map(|(chair, pool)| {
                let build = builder.build(pool, &mut sibling(seed, chair, 0)).unwrap();
                (build.deck, build.rows)
            })
            .collect();
        (draft.pick_rows, builds)
    };
    let serial: Vec<_> = (0..8).map(&collect).collect();
    let parallel: Vec<_> = std::thread::scope(|scope| {
        let collect = &collect;
        let handles: Vec<_> = (0..8).map(|i| scope.spawn(move || collect(i))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(serial, parallel);
}

fn targeted(pool: &[u16], count: u64, type_set: u64) -> BuildPath {
    walk_build(&Probe(0.0), pool, |step, options, logits| {
        let word = match step {
            Step::Count => count,
            Step::TypeSet => type_set,
            Step::Split(i) if i % 3 == 0 => u64::MAX,
            _ => 0,
        };
        decide(step.kind(), options, logits, None, word, None)
    })
    .unwrap()
}

#[test]
fn count_boundaries_single_type_and_no_extras() {
    let cube = cube();
    let pool = &cube.ids()[..POOL_CARDS];
    let no_basics = targeted(pool, 0, u64::MAX);
    assert_eq!(no_basics.rows.count.chosen, 0);
    assert_eq!(no_basics.rows.pointer.len(), 40);
    assert!(no_basics.rows.type_set.is_none() && no_basics.rows.split.is_empty());
    let mut expected_log_p = sample_v1(&[0.0; 41], &[true; 41], 0)
        .unwrap()
        .selected_log_probability;
    for (j, row) in no_basics.rows.pointer.iter().enumerate() {
        assert_eq!(row.options.len(), 45 - j);
        expected_log_p += sample_v1(&vec![0.0; 45 - j], &vec![true; 45 - j], 0)
            .unwrap()
            .selected_log_probability;
        assert!(row.options.iter().all(|id| pool.contains(id)));
        assert!(no_basics.rows.pointer[..j]
            .iter()
            .all(|p| !row.options.contains(&p.chosen)));
    }
    assert_eq!(
        build_path_log_prob(&no_basics.rows, &Probe(0.0), pool).to_bits(),
        expected_log_p.to_bits()
    );
    let singleton = targeted(pool, u64::MAX, 0);
    assert_eq!(singleton.rows.count.chosen, 40);
    assert!(singleton.rows.pointer.is_empty() && singleton.rows.split.is_empty());
    assert_eq!(singleton.rows.type_set.unwrap().chosen, 1);
    assert_eq!(
        singleton.deck.card_ids(),
        vec![basic_land_ids().unwrap()[0]; 40]
    );

    let one = targeted(pool, 1 << 59, u64::MAX);
    assert_eq!(one.rows.count.chosen, 1);
    assert_eq!(one.rows.type_set.unwrap().options, [1, 2, 4, 8, 16]);
    assert!(one.rows.split.is_empty());
    let two = targeted(pool, 1 << 60, u64::MAX);
    assert_eq!(two.rows.count.chosen, 2);
    assert_eq!(two.rows.type_set.unwrap().chosen, 24);
    assert!(two.rows.split.is_empty());
}

#[test]
fn likelihood_uses_new_parameters_and_order_symmetric_split_counts() {
    let cube = cube();
    let pool = &cube.ids()[..POOL_CARDS];
    let mut path = targeted(pool, u64::MAX, u64::MAX);
    let rows = &mut path.rows;
    assert_eq!(rows.count.chosen, 40);
    assert_eq!(rows.type_set.as_ref().unwrap().chosen, 31);
    assert_eq!(rows.split.len(), 35);
    let bits = [1u16, 2, 4, 8, 16];
    let mut copies = [1; 5];
    for row in &rows.split {
        assert_eq!(row.options, bits);
        copies[bits.iter().position(|&bit| bit == row.chosen).unwrap()] += 1;
    }
    let expected_cards: Vec<_> = basic_land_ids()
        .unwrap()
        .into_iter()
        .zip(copies)
        .flat_map(|(id, n)| std::iter::repeat_n(id, n))
        .collect();
    assert_eq!(path.deck, canonical_deck(&expected_cards));
    assert_eq!(
        build_path_log_prob(rows, &Probe(0.0), pool).to_bits(),
        path.log_prob.to_bits()
    );

    let changed = Probe(0.75);
    let ordered = sorted_cards(pool);
    let input = BuildInputs {
        pool: &ordered,
        chosen: &[],
        basics: 40,
    };
    let masks = (1..32).collect::<Vec<_>>();
    let logs = |logits: &[f32]| {
        sample_v1(logits, &vec![true; logits.len()], 0)
            .unwrap()
            .log_probabilities
    };
    let expected = logs(&changed.count(&ordered).unwrap())[40]
        + logs(&changed.type_set(&input, &masks).unwrap())[30];
    let split = logs(&changed.split(&input, &bits).unwrap());
    let split_term = copies
        .into_iter()
        .zip(split)
        .fold(0.0, |sum, (n, log_q)| sum + (n - 1) as f64 * log_q);
    let expected = expected + split_term;
    rows.split.reverse();
    // The observed count remains 40 even though this word now draws count 0.
    rows.count.stream_values[0] = 0;
    assert_eq!(
        build_path_log_prob(rows, &changed, pool).to_bits(),
        expected.to_bits()
    );
    assert!(replay_build(rows, &changed, pool).is_err());
    rows.split.pop();
    assert!(try_build_path_log_prob(rows, &changed, pool).is_err());
}

#[test]
fn one_thousand_seeded_decks_validate_and_use_one_word_per_row() {
    let cube = cube();
    let pool = &cube.ids()[..POOL_CARDS];
    let mut builder = LearnedSeatBuilder::new(Arc::new(Probe(0.0)));
    for seed in 0..1_000 {
        let mut rng = sibling(seed, 0, 0);
        let mut expected = sibling(seed, 0, 0);
        let outcome = builder.build(pool, &mut rng).unwrap();
        outcome.deck.validate(pool).unwrap();
        assert_eq!(outcome.deck.entries.len(), 40);
        assert!(outcome.deck.entries.iter().all(|e| e.count == 1));
        assert!(outcome.deck.entries.windows(2).all(|w| w[0].id <= w[1].id));
        let rows = outcome.rows.unwrap();
        let decisions = std::iter::once(&rows.count)
            .chain(&rows.pointer)
            .chain(&rows.type_set)
            .chain(&rows.split);
        let mut draws = 0;
        for row in decisions {
            draws += 1;
            assert_eq!(row.stream_values, [expected.next_u64()]);
            assert_eq!(row.sampler, SAMPLER_ID_V1);
            assert!(row.value_bits.is_none());
        }
        assert!(draws <= 41);
        assert_eq!(rng.next_u64(), expected.next_u64());
    }
}

#[test]
fn twenty_thousand_zero_head_builds_have_the_declared_marginals() {
    let cube = cube();
    let pool = &cube.ids()[..POOL_CARDS];
    let mut builder = LearnedSeatBuilder::new(Arc::new(Probe(0.0)));
    let mut count_bins = [0u32; 41];
    let mut sizes = [0u32; 5];
    for seed in 0..20_000 {
        let rows = builder
            .build(pool, &mut sibling(seed, 0, 0))
            .unwrap()
            .rows
            .unwrap();
        count_bins[usize::from(rows.count.chosen)] += 1;
        if rows.count.chosen >= 5 {
            sizes[rows.type_set.unwrap().chosen.count_ones() as usize - 1] += 1;
        }
    }
    let within_four_se = |observed: u32, trials: u32, prob: f64| {
        let expected = f64::from(trials) * prob;
        assert!(
            (f64::from(observed) - expected).abs() <= 4.0 * (expected * (1.0 - prob)).sqrt(),
            "observed {observed}, expected {expected} in {trials} builds"
        );
    };
    for observed in count_bins {
        within_four_se(observed, 20_000, 1.0 / 41.0);
    }
    let eligible = sizes.iter().sum();
    assert_eq!(eligible, count_bins[5..].iter().sum::<u32>());
    for (observed, masks) in sizes.into_iter().zip([5, 10, 10, 5, 1]) {
        within_four_se(observed, eligible, f64::from(masks) / 31.0);
    }
}
