use super::*;
use crate::cube_draft_v1::{deck_seed, derive_seed, CubePool, SeedStream, SEATS};
use crate::cube_seat_v1::{run_seat_draft, SeatDraft};
use std::collections::BTreeSet;

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
    CubePool::from_json_texts(
        include_str!("../../../data/cube_v1/mtgo_vintage_cube_20261007.json"),
        include_str!("../../../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"),
        None,
    )
    .unwrap()
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

#[test]
fn eight_seats_replay_all_picks_and_siblings() {
    let params = Arc::new(Probe(0.75));
    let seed = 731;
    let draft = draft(&cube(), seed, params.clone());
    let mut builder = LearnedSeatBuilder::new(params.clone());
    for chair in 0..SEATS {
        assert_eq!(draft.pick_rows[chair].len(), 42);
        let mut pool = Vec::new();
        let mut history = BTreeSet::new();
        let mut rows = draft.pick_rows[chair].iter();
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
                replay_pick(row, params.as_ref(), &view).unwrap();
                if pick.pick == 0 {
                    let mut corrupt = row.clone();
                    corrupt.logits_bits.as_mut().unwrap()[0] ^= 1;
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

fn privacy_draft(cube: &CubePool) -> SeatDraft {
    let mut seats: [Box<dyn SeatPicker>; SEATS] = std::array::from_fn(|chair| {
        if chair == 0 {
            Box::new(LearnedSeatPicker::new(Arc::new(Probe(1.0)))) as Box<dyn SeatPicker>
        } else {
            Box::new(FirstCard) as Box<dyn SeatPicker>
        }
    });
    run_seat_draft(cube, 981, &mut seats).unwrap()
}

#[test]
fn changing_an_unseen_card_in_another_pool_preserves_seat_rows() {
    let cube = cube();
    let first = privacy_draft(&cube);
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
    let cards: Vec<_> = names
        .iter()
        .map(|name| serde_json::json!({"name": name}))
        .collect();
    let changed =
        CubePool::from_json_texts(&serde_json::json!({"cards": cards}).to_string(), "{}", None)
            .unwrap();
    let second = privacy_draft(&changed);
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
        .log_probabilities[0];
    for (j, row) in no_basics.rows.pointer.iter().enumerate() {
        assert_eq!(row.options.len(), 45 - j);
        expected_log_p += sample_v1(&vec![0.0; 45 - j], &vec![true; 45 - j], 0)
            .unwrap()
            .log_probabilities[0];
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
    let mut expected = logs(&changed.count(&ordered).unwrap())[40]
        + logs(&changed.type_set(&input, &masks).unwrap())[30];
    let split = logs(&changed.split(&input, &bits).unwrap());
    for (n, log_q) in copies.into_iter().zip(split) {
        if n > 1 {
            expected += (n - 1) as f64 * log_q;
        }
    }
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
