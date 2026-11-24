//! Acceptance tests for D5 (`cube_seat_v1` and `cube_pod_runner_v1`):
//! 1. Uniform draft equality (`run_seat_draft` vs `run_draft` over 20 seeds).
//! 2. Attempt 0 equals `play_pod_game` with stated builds of a uniform draft.
//! 3. Bit-identical pod replay on 1 and 12 worker threads.
//! 4. Sibling invariance (with/without sibling; with/without sibling halts).
//! 5. Coupling (shared seats, env/action seeds, opponent library order, row
//!    position permutation).
//! 6. Synthetic halt (replays only the halted game with next deal seed).
//! 7. Exhausted retries (4 halts -> `status: "stopped"`, stop report, error).
//! 8. Seed parity with `tools/cube_v1/deal.py` (20 pinned constants).
//! 9. Training pod layout (70 games, 28 groups, 6 sibling pairs of 7 games,
//!    group sizes 3/2/1 with non-adjacent older chairs), read plans and reports.
//! 10. Frozen JSON key order schema check, rejecting missing/extra/reordered
//!     fields.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::cube_draft_v1::{
    derive_seed, game_seed, play_pod_game, pod_pairings, run_draft, uniform_pickers, CubePool,
    Deck, GameCaps, GameEnd, SeedStream, PACK_SIZE, POD_GAMES, POOL_CARDS, ROUNDS, SEATS,
};
use mtg_kernel::cube_pod_runner_v1::{
    attempt_seeds, pod_draft_seed, pod_league_seed, pod_play_seed, run_read_plan,
    run_training_pod, run_uniform_training_batch, seed, sibling_correlation,
    stated_training_chairs, training_roles_with_older, uniform_training_chairs,
    validate_pod_record_json, BatchReport, ChairRole, DealtOrigin, DeckSource, GameRole,
    PairedBuildKind, PilotInfo, PodCaps, PodPins, PodRecord, PodReport, PodRunnerError, PodStatus,
    PodUse, ReadDeckSpec, ReadGameSpec, ReadPlan, SeatRule, SeedPart, TrainingPodConfig,
    UniformBatchConfig, A2_GAME_CAPS, POD_RECORD_SCHEMA, TRAINING_POD_GAMES,
};
use mtg_kernel::cube_seat_v1::{
    canonical_deck, run_seat_draft, AttemptOutcome, AttemptSpec, DecisionKind, GamePilot,
    PassDirection, PickOutcome, SeatBuilder, SeatError, SeatPickView, SeatPicker,
    StatedSeatBuilder, UniformFourStepBuilder, UniformPilot, UniformSeatPicker,
    M4_INTERFACES_VERSION, UNIFORM_SAMPLER_ID,
};
use mtg_kernel::rl::build_deck_pair_state;
use mtg_kernel::state::SplitMix64;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const CUBE_LIST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007.json"
);
const GAP_RECORD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"
);

fn pool() -> CubePool {
    let cube = std::fs::read_to_string(CUBE_LIST).expect("cube list");
    let gaps = std::fs::read_to_string(GAP_RECORD).expect("gap record");
    CubePool::from_json_texts(&cube, &gaps, Some(r#"{"cards": []}"#)).expect("draftable pool")
}

/// Runs `work` on a thread with a 64 MiB stack, as the cube tests do.
fn on_big_stack(work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name("cube-pod-runner-v1-test".to_owned())
        .stack_size(64 << 20)
        .spawn(work)
        .expect("spawn")
        .join()
        .expect("test thread");
}

/// Deterministic fast pilot for structural, replay, halt and invariance tests.
/// Computes a natural outcome purely from `(decks, seats, episode, env_seed,
/// action_seed)`, with optional scripted halts matched by `(episode,
/// p0_first_card, p1_first_card, env_seed)`.
struct ScriptedPilot {
    /// Halts triggered by exact `(episode, env_seed)`.
    halt_by_episode_env: BTreeMap<(u64, u64), (GameEnd, Option<u16>)>,
    /// Halts triggered for any game whose p0 or p1 deck matches `deck_cards`
    /// on any of the listed `env_seed`s (or all attempts when empty).
    halt_by_deck: Vec<(Vec<u16>, BTreeSet<u64>, GameEnd, Option<u16>)>,
}

impl ScriptedPilot {
    fn natural_only() -> Self {
        ScriptedPilot {
            halt_by_episode_env: BTreeMap::new(),
            halt_by_deck: Vec::new(),
        }
    }
}

impl GamePilot for ScriptedPilot {
    fn play(&self, spec: &AttemptSpec<'_>, _caps: &GameCaps) -> AttemptOutcome {
        if let Some((end, src)) = self.halt_by_episode_env.get(&(spec.episode, spec.env_seed)) {
            return AttemptOutcome {
                decisions: 17,
                physical_decisions: Some(19),
                policy_steps: Some(23),
                end: end.clone(),
                last_action_source: *src,
            };
        }
        let d0 = spec.decks[0].card_ids();
        let d1 = spec.decks[1].card_ids();
        for (target_deck, env_seeds, end, src) in &self.halt_by_deck {
            let deck_matches = &d0 == target_deck || &d1 == target_deck;
            let env_matches = env_seeds.is_empty() || env_seeds.contains(&spec.env_seed);
            if deck_matches && env_matches {
                return AttemptOutcome {
                    decisions: 11,
                    physical_decisions: None,
                    policy_steps: None,
                    end: end.clone(),
                    last_action_source: *src,
                };
            }
        }

        // Deterministic mix of p0 win, p1 win and natural draw depending on
        // the two decks' canonical card lists and seeds.
        let mut h = SplitMix64::seed(spec.env_seed ^ spec.action_seed ^ spec.episode);
        for &id in d0.iter().chain(d1.iter()) {
            h = SplitMix64::seed(h.next_u64() ^ u64::from(id));
        }
        let roll = h.next_u64() % 11;
        let winner_seat = match roll {
            0 => None,
            1..=5 => Some(spec.seats[0]),
            _ => Some(spec.seats[1]),
        };
        let decisions = 20 + (h.next_u64() % 80);
        AttemptOutcome {
            decisions,
            physical_decisions: Some(decisions + 3),
            policy_steps: Some(decisions * 2),
            end: GameEnd::Natural {
                winner_seat,
                reason: if winner_seat.is_some() {
                    "life_zero".to_owned()
                } else {
                    "simultaneous_loss".to_owned()
                },
            },
            last_action_source: Some(d0[0]),
        }
    }
}

fn unwrap_complete_or_stopped(result: Result<PodRecord, PodRunnerError>) -> PodRecord {
    match result {
        Ok(record) => record,
        Err(PodRunnerError::Stopped(record)) => *record,
        Err(other) => panic!("unexpected pod runner error: {other}"),
    }
}

// ---------------------------------------------------------------------------
// Acceptance test 1: Uniform draft equality + SeatPickView invariants
// ---------------------------------------------------------------------------

struct CheckingUniformPicker {
    observed_steps: usize,
    seen_history: BTreeSet<u16>,
    pool_history: Vec<u16>,
}

impl CheckingUniformPicker {
    fn new() -> Self {
        CheckingUniformPicker {
            observed_steps: 0,
            seen_history: BTreeSet::new(),
            pool_history: Vec::new(),
        }
    }
}

impl SeatPicker for CheckingUniformPicker {
    fn pick(
        &mut self,
        view: &SeatPickView<'_>,
        stream: &mut SplitMix64,
    ) -> Result<PickOutcome, SeatError> {
        let expected_round = (self.observed_steps / PACK_SIZE) as u8;
        let expected_pick = (self.observed_steps % PACK_SIZE) as u8;
        assert_eq!(view.round, expected_round);
        assert_eq!(view.pick, expected_pick);
        let expected_dir = if expected_round % 2 == 1 {
            PassDirection::Right
        } else {
            PassDirection::Left
        };
        assert_eq!(view.direction, expected_dir);
        assert_eq!(view.pack.len(), PACK_SIZE - usize::from(expected_pick));
        assert_eq!(view.pool, self.pool_history.as_slice());

        // `seen_not_taken`: every card seen at an earlier pick minus own pool,
        // ascending registry id, at most 238 cards.
        let pool_set: BTreeSet<u16> = self.pool_history.iter().copied().collect();
        let expected_seen: Vec<u16> = self
            .seen_history
            .difference(&pool_set)
            .copied()
            .collect();
        assert_eq!(view.seen_not_taken, expected_seen.as_slice());
        assert!(view.seen_not_taken.windows(2).all(|w| w[0] < w[1]));
        assert!(view.seen_not_taken.len() <= 238);

        let outcome = UniformSeatPicker.pick(view, stream)?;
        let chosen = view.pack[outcome.index];
        for &card in view.pack {
            self.seen_history.insert(card);
        }
        self.pool_history.push(chosen);
        self.observed_steps += 1;
        Ok(outcome)
    }
}

#[test]
fn acceptance_1_uniform_draft_equality() {
    let pool = pool();
    for offset in 0u64..20 {
        let draft_seed = 20_261_124 + offset * 97;
        let expected = run_draft(&pool, draft_seed, &mut uniform_pickers(draft_seed)).unwrap();
        let mut seat_pickers: [Box<dyn SeatPicker>; SEATS] =
            std::array::from_fn(|_| Box::new(UniformSeatPicker) as Box<dyn SeatPicker>);
        let actual = run_seat_draft(&pool, draft_seed, &mut seat_pickers).unwrap();
        assert_eq!(actual.record, expected, "seed {draft_seed}");
        assert!(actual.pick_rows.iter().all(Vec::is_empty));
    }

    // Verify `SeatPickView` history, direction and bounds on every pick.
    let draft_seed = 20_261_124;
    let mut checkers: [Box<dyn SeatPicker>; SEATS] =
        std::array::from_fn(|_| Box::new(CheckingUniformPicker::new()) as Box<dyn SeatPicker>);
    let checked = run_seat_draft(&pool, draft_seed, &mut checkers).unwrap();
    let expected = run_draft(&pool, draft_seed, &mut uniform_pickers(draft_seed)).unwrap();
    assert_eq!(checked.record, expected);
    assert_eq!(checked.record.pools.len(), SEATS);
    assert!(checked
        .record
        .pools
        .iter()
        .all(|p| p.len() == POOL_CARDS));
    assert_eq!(checked.record.packs.len(), ROUNDS);
}

// ---------------------------------------------------------------------------
// Acceptance test 2: Attempt 0 is today's game (`play_pod_game`)
// ---------------------------------------------------------------------------

#[test]
fn acceptance_2_attempt_0_is_todays_game() {
    on_big_stack(|| {
        let pool = pool();
        let roles = training_roles_with_older([2, 6]).unwrap();
        let namespace = "test";
        let run = "attempt0-parity";
        let pod_index = 0u64;

        // Small decision cap across all 28 pairings keeps the test fast while
        // still exercising the real Limited session on every reference game.
        let tiny_caps = PodCaps {
            max_decisions: 120,
            max_physical_decisions: A2_GAME_CAPS.max_physical_decisions,
            max_policy_steps: A2_GAME_CAPS.max_policy_steps,
            watchdog_seconds: 600,
            max_replays: 3,
        };
        let config = TrainingPodConfig {
            pod_use: PodUse::Test,
            namespace: namespace.to_owned(),
            run: run.to_owned(),
            pod_index,
            pins: PodPins::default(),
            pilot_info: PilotInfo::uniform(),
            caps: tiny_caps,
            workers: 4,
        };
        let mut chairs = stated_training_chairs(roles);
        let record = unwrap_complete_or_stopped(run_training_pod(
            &pool,
            &config,
            &mut chairs,
            &UniformPilot,
        ));

        let d_seed = pod_draft_seed(namespace, run, pod_index).unwrap();
        let p_seed = pod_play_seed(namespace, run, pod_index).unwrap();
        let draft = run_draft(&pool, d_seed, &mut uniform_pickers(d_seed)).unwrap();
        assert_eq!(record.draft.as_ref().unwrap(), &draft);

        // Verify StatedSeatBuilder draws nothing from its RNG stream and
        // produces canonical 40-row decks.
        let stated_decks: [Deck; SEATS] = std::array::from_fn(|chair| {
            let before = SplitMix64::seed(12_345 + chair as u64);
            let mut stream = before.clone();
            let outcome = StatedSeatBuilder
                .build(&draft.pools[chair], &mut stream)
                .unwrap();
            assert_eq!(stream, before, "StatedSeatBuilder must draw nothing");
            assert!(outcome.rows.is_none());
            assert_eq!(outcome.deck.entries.len(), 40);
            outcome.deck
        });

        let mut checked_reference = 0usize;
        for game in &record.games {
            if game.role != GameRole::Reference {
                continue;
            }
            let pairing = game.pairing.expect("reference game has pairing") as usize;
            let expected =
                play_pod_game(&stated_decks, p_seed, pairing, &tiny_caps.game_caps()).unwrap();
            let a0 = &game.attempts[0];
            let game_seats = [
                usize::from(game.chairs[0].unwrap()),
                usize::from(game.chairs[1].unwrap()),
            ];
            assert_eq!(game_seats, expected.seats, "pairing {pairing} seats");
            assert_eq!(a0.env_seed, expected.env_seed, "pairing {pairing} env_seed");
            assert_eq!(
                a0.decisions, expected.decisions,
                "pairing {pairing} decisions"
            );
            assert_eq!(a0.end, expected.end, "pairing {pairing} end");
            checked_reference += 1;
        }
        assert_eq!(checked_reference, POD_GAMES);

        // Also check pairing 0 under full A2.1 caps through UniformPilot.
        let g0_seed = game_seed(p_seed, 0);
        let expected_a2 = play_pod_game(&stated_decks, p_seed, 0, &A2_GAME_CAPS).unwrap();
        let (_, env_seed, action_seed) = attempt_seeds(g0_seed, 0);
        let outcome_a2 = UniformPilot.play(
            &AttemptSpec {
                decks: [
                    &stated_decks[expected_a2.seats[0]],
                    &stated_decks[expected_a2.seats[1]],
                ],
                seats: expected_a2.seats,
                episode: 0,
                env_seed,
                action_seed,
            },
            &A2_GAME_CAPS,
        );
        assert_eq!(env_seed, expected_a2.env_seed);
        assert_eq!(outcome_a2.decisions, expected_a2.decisions);
        assert_eq!(outcome_a2.end, expected_a2.end);
    });
}

// ---------------------------------------------------------------------------
// Acceptance test 3: Bit-identical replay on 1 and 12 workers
// ---------------------------------------------------------------------------

#[test]
fn acceptance_3_pod_replays_bit_for_bit_on_1_and_12_workers() {
    on_big_stack(|| {
        let pool = pool();
        let roles = training_roles_with_older([1, 5]).unwrap();

        // Part A: full 70-game complete pod with synthetic halts and replays
        // on 1 worker vs 12 workers.
        let p_seed = pod_play_seed("test", "det-workers", 3).unwrap();
        let (_, env5_a0, _) = attempt_seeds(game_seed(p_seed, 5), 0);
        let (_, env19_a0, _) = attempt_seeds(game_seed(p_seed, 19), 0);
        let mut pilot = ScriptedPilot::natural_only();
        pilot.halt_by_episode_env.insert(
            (5, env5_a0),
            (
                GameEnd::Terminal {
                    code: "engine_halted".to_owned(),
                    classification: "engine_halted".to_owned(),
                    reason: "synthetic".to_owned(),
                },
                Some(12),
            ),
        );
        pilot
            .halt_by_episode_env
            .insert((19, env19_a0), (GameEnd::DecisionCap, None));

        let run_with_workers = |workers: usize| -> (PodRecord, String) {
            let cfg = TrainingPodConfig::uniform_a2(
                PodUse::Test,
                "test",
                "det-workers",
                3,
                workers,
            );
            let mut chairs = uniform_training_chairs(roles);
            let record = run_training_pod(&pool, &cfg, &mut chairs, &pilot).unwrap();
            let line = record.to_json_line();
            (record, line)
        };

        let (rec1, bytes1) = run_with_workers(1);
        let (rec12, bytes12) = run_with_workers(12);
        assert_eq!(rec1.status, PodStatus::Complete);
        assert_eq!(rec1, rec12);
        assert_eq!(bytes1, bytes12);
        assert!(!rec1.halts.is_empty());

        // Part B: real UniformPilot through the Limited session on 1 vs 12 workers.
        let caps = PodCaps {
            max_decisions: 40,
            max_physical_decisions: A2_GAME_CAPS.max_physical_decisions,
            max_policy_steps: A2_GAME_CAPS.max_policy_steps,
            watchdog_seconds: 600,
            max_replays: 3,
        };
        let run_uniform = |workers: usize| -> String {
            let cfg = TrainingPodConfig {
                pod_use: PodUse::Test,
                namespace: "test".to_owned(),
                run: "det-uniform".to_owned(),
                pod_index: 0,
                pins: PodPins::default(),
                pilot_info: PilotInfo::uniform(),
                caps,
                workers,
            };
            let mut chairs = uniform_training_chairs(roles);
            let record = unwrap_complete_or_stopped(run_training_pod(
                &pool,
                &cfg,
                &mut chairs,
                &UniformPilot,
            ));
            record.to_json_line()
        };

        let u1 = run_uniform(1);
        let u12 = run_uniform(12);
        assert_eq!(u1, u12);
    });
}

// ---------------------------------------------------------------------------
// Acceptance test 4: Sibling invariance
// ---------------------------------------------------------------------------

#[test]
fn acceptance_4_sibling_invariance() {
    let pool = pool();
    let pilot = ScriptedPilot::natural_only();

    // Run A: chair 0 is Learning (has sibling a and sibling b). Older = [2, 6].
    let roles_with_sib = training_roles_with_older([2, 6]).unwrap();
    let cfg = TrainingPodConfig::uniform_a2(PodUse::Test, "test", "sib-inv", 4, 4);
    let mut chairs_a = uniform_training_chairs(roles_with_sib);
    let pod_with_sib = run_training_pod(&pool, &cfg, &mut chairs_a, &pilot).unwrap();

    // Run B: chair 0 is Older (no sibling b exists). Older = [0, 6].
    let roles_no_sib = training_roles_with_older([0, 6]).unwrap();
    let mut chairs_b = uniform_training_chairs(roles_no_sib);
    let pod_without_sib = run_training_pod(&pool, &cfg, &mut chairs_b, &pilot).unwrap();

    // Every chair's reference build (k = 0) and every reference game across all
    // 28 groups must be bit-identical whether chair 0's sibling exists or not.
    let ref_games_json = |pod: &PodRecord| -> Vec<String> {
        pod.games
            .iter()
            .filter(|g| g.role == GameRole::Reference)
            .map(|g| {
                serde_json::to_string(&json!({
                    "group": g.group,
                    "pairing": g.pairing,
                    "chairs": g.chairs,
                    "seat_rule": g.seat_rule,
                    "game_seed": g.game_seed,
                    "episode": g.episode,
                    "attempts": g.attempts,
                    "scores": g.scores,
                }))
                .unwrap()
            })
            .collect()
    };
    assert_eq!(
        ref_games_json(&pod_with_sib),
        ref_games_json(&pod_without_sib)
    );

    // Also run chair 0's 7 reference games in a ReadPlan where no sibling deck
    // exists anywhere in the pod.
    let chair0_ref_games: Vec<_> = pod_with_sib
        .games
        .iter()
        .filter(|g| g.role == GameRole::Reference && g.chairs.contains(&Some(0)))
        .collect();
    assert_eq!(chair0_ref_games.len(), 7);
    let ref_deck_specs: Vec<ReadDeckSpec> = (0..SEATS)
        .map(|chair| {
            let deck = pod_with_sib
                .decks
                .iter()
                .find(|d| d.chair == Some(chair as u8) && d.build == Some(0))
                .unwrap();
            ReadDeckSpec {
                source: DeckSource::Build,
                chair: deck.chair,
                build: deck.build,
                builder: deck.builder.clone(),
                build_seed: deck.build_seed,
                cards: deck.cards.clone(),
                build_rows: None,
                dealt: None,
            }
        })
        .collect();
    let read_games: Vec<ReadGameSpec> = chair0_ref_games
        .iter()
        .map(|g| {
            let c0 = usize::from(g.chairs[0].unwrap());
            let c1 = usize::from(g.chairs[1].unwrap());
            ReadGameSpec {
                group: g.group,
                role: GameRole::Reference,
                pairing: g.pairing,
                decks: [c0 as u32, c1 as u32],
                chairs: g.chairs,
                seats: [c0, c1],
                game_seed: g.game_seed,
                episode: g.episode,
            }
        })
        .collect();
    let read_plan = ReadPlan {
        pod_use: PodUse::Test,
        namespace: "test".to_owned(),
        run: "sib-inv".to_owned(),
        pod_index: 4,
        seeds: pod_with_sib.seeds,
        pins: PodPins::default(),
        pilot: PilotInfo::uniform(),
        caps: PodCaps::read_a2(),
        chairs: Vec::new(),
        decks: ref_deck_specs,
        games: read_games,
        paired_builds: Vec::new(),
        stop_on_halt: false,
    };
    let read_pod = run_read_plan(&read_plan, &pilot, 2).unwrap();
    for (ref_g, read_g) in chair0_ref_games.iter().zip(&read_pod.games) {
        assert_eq!(
            serde_json::to_string(&ref_g.attempts).unwrap(),
            serde_json::to_string(&read_g.attempts).unwrap()
        );
        assert_eq!(ref_g.scores, read_g.scores);
    }

    // Part B: halt chair 0's sibling b games on attempts 0..=2 (completing on
    // attempt 3) AND also test halting all 4 attempts. Chair 0's sibling a
    // (reference) games must remain bit-identical in both cases.
    let chair0_sib_deck = pod_with_sib
        .decks
        .iter()
        .find(|d| d.chair == Some(0) && d.build == Some(1))
        .unwrap()
        .cards
        .clone();

    let mut halt_3_envs = BTreeSet::new();
    for g in pod_with_sib
        .games
        .iter()
        .filter(|g| g.role == GameRole::Sibling && g.chairs.contains(&Some(0)))
    {
        for attempt in 0u8..3 {
            halt_3_envs.insert(attempt_seeds(g.game_seed, attempt).1);
        }
    }
    let mut pilot_sib_halts_3 = ScriptedPilot::natural_only();
    pilot_sib_halts_3.halt_by_deck.push((
        chair0_sib_deck.clone(),
        halt_3_envs,
        GameEnd::DecisionCap,
        Some(5),
    ));
    let mut chairs_c = uniform_training_chairs(roles_with_sib);
    let pod_sib_halted_3 =
        run_training_pod(&pool, &cfg, &mut chairs_c, &pilot_sib_halts_3).unwrap();
    assert_eq!(pod_sib_halted_3.status, PodStatus::Complete);
    assert_eq!(pod_sib_halted_3.halts.len(), 21); // 7 sibling games * 3 halts
    assert_eq!(
        ref_games_json(&pod_with_sib),
        ref_games_json(&pod_sib_halted_3)
    );

    let mut pilot_sib_halts_all = ScriptedPilot::natural_only();
    pilot_sib_halts_all.halt_by_deck.push((
        chair0_sib_deck,
        BTreeSet::new(), // halt every attempt
        GameEnd::Panic {
            message: "synthetic sibling halt".to_owned(),
        },
        None,
    ));
    let mut chairs_d = uniform_training_chairs(roles_with_sib);
    let pod_sib_stopped = unwrap_complete_or_stopped(run_training_pod(
        &pool,
        &cfg,
        &mut chairs_d,
        &pilot_sib_halts_all,
    ));
    assert_eq!(pod_sib_stopped.status, PodStatus::Stopped);
    assert_eq!(
        ref_games_json(&pod_with_sib),
        ref_games_json(&pod_sib_stopped)
    );
}

// ---------------------------------------------------------------------------
// Acceptance test 5: Coupling
// ---------------------------------------------------------------------------

fn shuffled_indices(len: usize, rng: &mut SplitMix64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..len).collect();
    for i in (1..v.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

fn dealt_order_cards(state: &mtg_kernel::state::GameState, player_idx: usize) -> Vec<u16> {
    let p = &state.players[player_idx];
    // Opening 7 cards are drawn from the top of the library into hand in order,
    // followed by the remaining 33 cards in library top-to-bottom order.
    p.hand
        .iter()
        .chain(p.library.iter())
        .map(|&oid| state.objects.get(oid).card_def)
        .collect()
}

#[test]
fn acceptance_5_coupling() {
    let pool = pool();
    let roles = training_roles_with_older([1, 6]).unwrap();
    let cfg = TrainingPodConfig::uniform_a2(PodUse::Test, "test", "coupling", 2, 4);
    let mut chairs = uniform_training_chairs(roles);
    let pilot = ScriptedPilot::natural_only();
    let pod = run_training_pod(&pool, &cfg, &mut chairs, &pilot).unwrap();

    // Every deck has 40 canonical rows in ascending card id order.
    for deck_rec in &pod.decks {
        assert_eq!(deck_rec.cards.len(), 40);
        assert!(deck_rec.cards.windows(2).all(|w| w[0] <= w[1]));
        let canonical = canonical_deck(&deck_rec.cards);
        assert_eq!(canonical.entries.len(), 40);
        assert!(canonical.entries.iter().all(|e| e.count == 1));
    }

    // Group games by `group` and verify every coupled group shares seats,
    // game_seed, episode, and attempt-0 deal/env/action seeds.
    let mut by_group: BTreeMap<u32, Vec<_>> = BTreeMap::new();
    for game in &pod.games {
        by_group.entry(game.group).or_default().push(game);
    }
    assert_eq!(by_group.len(), POD_GAMES);

    let mut verified_shuffle_groups = 0usize;
    for games in by_group.values() {
        let reference = games[0];
        assert_eq!(reference.role, GameRole::Reference);
        let a0_ref = &reference.attempts[0];
        for sibling in &games[1..] {
            assert_eq!(sibling.role, GameRole::Sibling);
            assert_eq!(sibling.chairs, reference.chairs);
            assert_eq!(sibling.pairing, reference.pairing);
            assert_eq!(sibling.game_seed, reference.game_seed);
            assert_eq!(sibling.episode, reference.episode);
            let a0_sib = &sibling.attempts[0];
            assert_eq!(a0_sib.deal_seed, a0_ref.deal_seed);
            assert_eq!(a0_sib.env_seed, a0_ref.env_seed);
            assert_eq!(a0_sib.action_seed, a0_ref.action_seed);

            // Build the real initial GameStates under `env_seed` and verify
            // both the opponent's library order and the exact row-position
            // permutation are identical.
            let ref_p0 = &pod.decks[reference.decks[0] as usize].cards;
            let ref_p1 = &pod.decks[reference.decks[1] as usize].cards;
            let sib_p0 = &pod.decks[sibling.decks[0] as usize].cards;
            let sib_p1 = &pod.decks[sibling.decks[1] as usize].cards;

            let state_ref = build_deck_pair_state(a0_ref.env_seed, ref_p0, ref_p1).unwrap();
            let state_sib = build_deck_pair_state(a0_sib.env_seed, sib_p0, sib_p1).unwrap();

            // Exactly one seat changed its deck (the learning chair's sibling b);
            // the opponent seat holds the identical deck and must receive the
            // identical library/hand order.
            let opponent_seat = if reference.decks[0] == sibling.decks[0] {
                0usize
            } else {
                assert_eq!(reference.decks[1], sibling.decks[1]);
                1usize
            };
            assert_eq!(
                dealt_order_cards(&state_ref, opponent_seat),
                dealt_order_cards(&state_sib, opponent_seat),
                "opponent library order must match across coupled group"
            );

            // Verify both seats follow the exact same row-position permutation
            // `perm0`, `perm1` of 0..40 derived from `env_seed`.
            let mut rng = SplitMix64::seed(a0_ref.env_seed);
            let perm0 = shuffled_indices(40, &mut rng);
            let perm1 = shuffled_indices(40, &mut rng);
            let apply = |deck: &[u16], perm: &[usize]| -> Vec<u16> {
                perm.iter().map(|&idx| deck[idx]).collect()
            };
            assert_eq!(dealt_order_cards(&state_ref, 0), apply(ref_p0, &perm0));
            assert_eq!(dealt_order_cards(&state_ref, 1), apply(ref_p1, &perm1));
            assert_eq!(dealt_order_cards(&state_sib, 0), apply(sib_p0, &perm0));
            assert_eq!(dealt_order_cards(&state_sib, 1), apply(sib_p1, &perm1));
            verified_shuffle_groups += 1;
        }
    }
    assert_eq!(verified_shuffle_groups, TRAINING_POD_GAMES - POD_GAMES);
}

// ---------------------------------------------------------------------------
// Acceptance test 6: Synthetic halt
// ---------------------------------------------------------------------------

#[test]
fn acceptance_6_synthetic_halt() {
    let pool = pool();
    let roles = training_roles_with_older([0, 4]).unwrap();
    let cfg = TrainingPodConfig::uniform_a2(PodUse::Test, "test", "synth-halt", 7, 4);

    let baseline_pilot = ScriptedPilot::natural_only();
    let mut chairs_base = uniform_training_chairs(roles);
    let baseline = run_training_pod(&pool, &cfg, &mut chairs_base, &baseline_pilot).unwrap();

    // Target a single chosen game (game 17, a sibling game in the middle of the pod)
    // and halt ONLY its attempt 0.
    let target_idx = 17usize;
    let target = &baseline.games[target_idx];
    let target_deck_p0 = baseline.decks[target.decks[0] as usize].cards.clone();
    let target_deck_p1 = baseline.decks[target.decks[1] as usize].cards.clone();

    struct SingleAttemptHaltPilot {
        inner: ScriptedPilot,
        target_p0: Vec<u16>,
        target_p1: Vec<u16>,
        target_env_seed: u64,
    }

    impl GamePilot for SingleAttemptHaltPilot {
        fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
            if spec.env_seed == self.target_env_seed
                && spec.decks[0].card_ids() == self.target_p0
                && spec.decks[1].card_ids() == self.target_p1
            {
                return AttemptOutcome {
                    decisions: 33,
                    physical_decisions: Some(35),
                    policy_steps: Some(40),
                    end: GameEnd::Terminal {
                        code: "engine_halted".to_owned(),
                        classification: "engine_halted".to_owned(),
                        reason: "synthetic test halt".to_owned(),
                    },
                    last_action_source: Some(77),
                };
            }
            self.inner.play(spec, caps)
        }
    }

    let halt_pilot = SingleAttemptHaltPilot {
        inner: ScriptedPilot::natural_only(),
        target_p0: target_deck_p0,
        target_p1: target_deck_p1,
        target_env_seed: target.attempts[0].env_seed,
    };
    let mut chairs_halt = uniform_training_chairs(roles);
    let faulted = run_training_pod(&pool, &cfg, &mut chairs_halt, &halt_pilot).unwrap();

    assert_eq!(faulted.status, PodStatus::Complete);
    assert!(faulted.stop.is_none());

    // Only the targeted game replayed; every other game is unchanged.
    for idx in 0..TRAINING_POD_GAMES {
        if idx == target_idx {
            let g = &faulted.games[idx];
            assert_eq!(g.chairs, target.chairs);
            assert_eq!(g.decks, target.decks);
            assert_eq!(g.attempts.len(), 2);
            assert_eq!(g.attempts[0].attempt, 0);
            assert!(g.attempts[0].halted);
            assert_eq!(
                g.attempts[0].stop_kind.as_deref(),
                Some("terminal:engine_halted")
            );
            assert_eq!(g.attempts[0].last_action_source, Some(77));
            assert_eq!(g.attempts[0].deal_seed, g.game_seed);

            let (d1, env1, act1) = attempt_seeds(g.game_seed, 1);
            assert_eq!(d1, derive_seed(g.game_seed, SeedStream::GameRetry, 1));
            assert_eq!(g.attempts[1].attempt, 1);
            assert!(!g.attempts[1].halted);
            assert_eq!(g.attempts[1].stop_kind, None);
            assert_eq!(g.attempts[1].deal_seed, d1);
            assert_eq!(g.attempts[1].env_seed, env1);
            assert_eq!(g.attempts[1].action_seed, act1);
            assert!(g.scores.is_some());
        } else {
            assert_eq!(faulted.games[idx], baseline.games[idx], "game {idx}");
        }
    }

    assert_eq!(faulted.halts.len(), 1);
    let h = &faulted.halts[0];
    assert_eq!(h.game, target_idx as u32);
    assert_eq!(h.attempt, 0);
    assert_eq!(h.stop_kind, "terminal:engine_halted");
    assert_eq!(h.last_action_source, Some(77));
    assert_eq!(h.deal_seed, target.game_seed);

    let report = PodReport::from_record(&faulted);
    assert_eq!(report.replays, 1);
    assert_eq!(report.total_halts, 1);
    assert_eq!(
        report
            .halts_by_stop_kind_and_attempt
            .get("terminal:engine_halted")
            .and_then(|m| m.get(&0)),
        Some(&1)
    );
    assert_eq!(report.games_by_end_kind.get("natural"), Some(&70));
}

// ---------------------------------------------------------------------------
// Acceptance test 7: Exhausted retries
// ---------------------------------------------------------------------------

#[test]
fn acceptance_7_exhausted_retries() {
    let pool = pool();
    let roles = training_roles_with_older([2, 5]).unwrap();
    let cfg = TrainingPodConfig::uniform_a2(PodUse::Test, "test", "exhausted", 9, 4);

    // Get target game identity from a clean baseline run.
    let mut chairs_base = uniform_training_chairs(roles);
    let baseline =
        run_training_pod(&pool, &cfg, &mut chairs_base, &ScriptedPilot::natural_only()).unwrap();
    let target_idx = 23usize;
    let target = &baseline.games[target_idx];
    let target_p0 = baseline.decks[target.decks[0] as usize].cards.clone();
    let target_p1 = baseline.decks[target.decks[1] as usize].cards.clone();

    let mut per_attempt = BTreeMap::new();
    per_attempt.insert(
        attempt_seeds(target.game_seed, 0).1,
        (
            GameEnd::Terminal {
                code: "engine_halted".to_owned(),
                classification: "engine_halted".to_owned(),
                reason: "halt 0".to_owned(),
            },
            Some(10),
        ),
    );
    per_attempt.insert(
        attempt_seeds(target.game_seed, 1).1,
        (
            GameEnd::SessionError {
                code: "bad_step".to_owned(),
                message: "halt 1".to_owned(),
            },
            None,
        ),
    );
    per_attempt.insert(
        attempt_seeds(target.game_seed, 2).1,
        (GameEnd::DecisionCap, Some(20)),
    );
    per_attempt.insert(
        attempt_seeds(target.game_seed, 3).1,
        (
            GameEnd::Panic {
                message: "halt 3".to_owned(),
            },
            Some(30),
        ),
    );

    struct ExhaustGamePilot {
        inner: ScriptedPilot,
        target_p0: Vec<u16>,
        target_p1: Vec<u16>,
        per_attempt: BTreeMap<u64, (GameEnd, Option<u16>)>,
    }

    impl GamePilot for ExhaustGamePilot {
        fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
            if spec.decks[0].card_ids() == self.target_p0
                && spec.decks[1].card_ids() == self.target_p1
            {
                if let Some((end, src)) = self.per_attempt.get(&spec.env_seed) {
                    return AttemptOutcome {
                        decisions: 5,
                        physical_decisions: None,
                        policy_steps: None,
                        end: end.clone(),
                        last_action_source: *src,
                    };
                }
            }
            self.inner.play(spec, caps)
        }
    }

    let exhaust_pilot = ExhaustGamePilot {
        inner: ScriptedPilot::natural_only(),
        target_p0,
        target_p1,
        per_attempt,
    };

    let mut chairs = uniform_training_chairs(roles);
    let err = run_training_pod(&pool, &cfg, &mut chairs, &exhaust_pilot)
        .expect_err("exhausted retries must return an error");
    let PodRunnerError::Stopped(record) = err else {
        panic!("expected PodRunnerError::Stopped, got {err:?}");
    };

    assert_eq!(record.status, PodStatus::Stopped);
    let stop = record.stop.as_ref().expect("stop report present");
    assert_eq!(stop.game, target_idx as u32);
    assert_eq!(stop.pairing, target.pairing);
    assert_eq!(stop.decks, target.decks);
    assert_eq!(stop.attempts.len(), 4);

    let expected_stop_kinds = [
        ("terminal:engine_halted", Some(10u16)),
        ("session_error:bad_step", None),
        ("decision_cap", Some(20u16)),
        ("panic", Some(30u16)),
    ];
    for (k, (expected_kind, expected_src)) in expected_stop_kinds.iter().enumerate() {
        let expected_deal = attempt_seeds(target.game_seed, k as u8).0;
        assert_eq!(stop.attempts[k].deal_seed, expected_deal);
        assert_eq!(stop.attempts[k].stop_kind, *expected_kind);
        assert_eq!(stop.attempts[k].last_action_source, *expected_src);
        assert_eq!(record.halts[k].game, target_idx as u32);
        assert_eq!(record.halts[k].attempt, k as u8);
        assert_eq!(record.halts[k].stop_kind, *expected_kind);
        assert_eq!(record.halts[k].last_action_source, *expected_src);
        assert_eq!(record.halts[k].deal_seed, expected_deal);
    }
    assert_eq!(record.games[target_idx].scores, None);
    assert_eq!(record.halts.len(), 4);

    // Batch runner must fail and return no completed batch when any pod stops.
    let batch_cfg = UniformBatchConfig {
        pod_use: PodUse::Test,
        namespace: "test".to_owned(),
        run: "exhausted".to_owned(),
        pod_indices: vec![8, 9, 10],
        roles,
        pins: PodPins::default(),
        pilot_info: PilotInfo::uniform(),
        caps: PodCaps::training_a2(),
        workers: 2,
    };
    let batch_err = run_uniform_training_batch(&pool, &batch_cfg, &exhaust_pilot)
        .expect_err("stopped pod must fail the batch");
    assert!(matches!(batch_err, PodRunnerError::Stopped(_)));
}

// ---------------------------------------------------------------------------
// Acceptance test 8: Seed parity with `tools/cube_v1/deal.py` (20 constants)
// ---------------------------------------------------------------------------

#[test]
fn acceptance_8_seed_parity() {
    use SeedPart::{Int, Str};
    // Exact values computed by `tools/cube_v1/deal.py`'s `seed(parts)`.
    let cases: [(&[SeedPart<'_>], u64); 20] = [
        (&[], 912_589_757_505_950_543),
        (&[Str("test")], 5_237_881_001_800_564_204),
        (&[Int(0)], 3_999_586_761_668_934_864),
        (
            &[Str("test"), Str("draft"), Str("r0"), Int(0)],
            8_324_081_982_152_888_707,
        ),
        (
            &[Str("test"), Str("pod"), Str("r0"), Int(0)],
            6_948_102_938_367_031_956,
        ),
        (
            &[Str("test"), Str("league"), Str("r0"), Int(0)],
            7_362_183_919_503_689_233,
        ),
        (
            &[Str("shakedown-a"), Str("draft"), Str("run-1"), Int(0)],
            8_628_876_327_108_644_078,
        ),
        (
            &[Str("shakedown-a"), Str("pod"), Str("run-1"), Int(0)],
            1_518_168_743_933_313_580,
        ),
        (
            &[Str("shakedown-a"), Str("league"), Str("run-1"), Int(0)],
            7_527_987_057_591_521_262,
        ),
        (
            &[Str("m4-train"), Str("draft"), Str("exp-01"), Int(42)],
            2_471_827_610_120_804_323,
        ),
        (
            &[Str("m4-train"), Str("pod"), Str("exp-01"), Int(42)],
            5_152_403_941_398_396_836,
        ),
        (
            &[Str("m4-train"), Str("league"), Str("exp-01"), Int(42)],
            6_452_114_269_503_053_024,
        ),
        (
            &[Str("m5-train"), Str("draft"), Str("s-99"), Int(1_000_000)],
            2_029_264_369_621_416_193,
        ),
        (
            &[
                Str("m5-calibration"),
                Str("pod"),
                Str("cal-0"),
                Int(999_999_999_999),
            ],
            3_618_433_986_077_592_463,
        ),
        (
            &[
                Str("owner-view"),
                Str("league"),
                Str("v2-final"),
                Int(u64::MAX),
            ],
            1_683_882_959_090_064_038,
        ),
        (
            &[Str("a"), Str("b"), Str("c"), Int(1), Int(2), Int(3)],
            3_244_092_357_988_662_097,
        ),
        (
            &[Str("0"), Str("9"), Str("-"), Str("0-9-a-z")],
            6_351_648_686_945_933_338,
        ),
        (
            &[Str("read-b"), Str("run-0"), Int(7), Int(3)],
            6_200_712_038_204_246_618,
        ),
        (
            &[Str("sizing"), Str("draft"), Str("sz-12"), Int(255)],
            7_842_682_840_268_051_753,
        ),
        (
            &[Str("test"), Str("pod"), Str("p-0"), Int(65_535)],
            762_798_492_432_344_307,
        ),
    ];

    for (index, (parts, expected)) in cases.iter().enumerate() {
        assert_eq!(seed(parts).unwrap(), *expected, "case {index}");
    }
    assert_eq!(
        pod_draft_seed("m4-train", "exp-01", 42).unwrap(),
        2_471_827_610_120_804_323
    );
    assert_eq!(
        pod_play_seed("m4-train", "exp-01", 42).unwrap(),
        5_152_403_941_398_396_836
    );
    assert_eq!(
        pod_league_seed("m4-train", "exp-01", 42).unwrap(),
        6_452_114_269_503_053_024
    );

    // Reject strings outside `[a-z0-9-]+`.
    for bad in ["", "Upper", "has_underscore", "has space", "a/b", "a\"b"] {
        assert!(
            matches!(
                seed(&[Str(bad)]),
                Err(PodRunnerError::InvalidSeedPart(_))
            ),
            "should reject {bad:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Acceptance test 9: Training pod layout, UniformFourStepBuilder, read plans
// and reports
// ---------------------------------------------------------------------------

#[test]
fn acceptance_9_layout_read_plans_and_reports() {
    let pool = pool();
    let pilot = ScriptedPilot::natural_only();

    // Test two non-adjacent older chair placements: [1, 5] and [0, 7].
    for older in [[1, 5], [0, 7]] {
        let roles = training_roles_with_older(older).unwrap();
        let cfg = TrainingPodConfig::uniform_a2(PodUse::Training, "test", "layout", 0, 4);
        let mut chairs = uniform_training_chairs(roles);
        let pod = run_training_pod(&pool, &cfg, &mut chairs, &pilot).unwrap();

        assert_eq!(pod.schema, POD_RECORD_SCHEMA);
        assert_eq!(pod.interfaces, M4_INTERFACES_VERSION);
        assert_eq!(pod.chairs.len(), SEATS);
        assert_eq!(pod.decks.len(), 14);
        assert_eq!(pod.games.len(), TRAINING_POD_GAMES);
        assert_eq!(pod.paired_builds.len(), 6);

        // Verify deck build_rows from UniformFourStepBuilder.
        for deck_rec in &pod.decks {
            assert_eq!(deck_rec.cards.len(), 40);
            let rows = deck_rec.build_rows.as_ref().expect("uniform build_rows");
            assert_eq!(rows.count.kind, DecisionKind::Count);
            assert_eq!(rows.count.sampler, UNIFORM_SAMPLER_ID);
            assert_eq!(rows.count.options.len(), 41);
            assert!(!rows.count.stream_values.is_empty());
            let b = usize::from(rows.count.chosen);
            assert_eq!(rows.pointer.len(), 40 - b);
            for ptr in &rows.pointer {
                assert_eq!(ptr.kind, DecisionKind::Pointer);
                assert!(ptr.options.windows(2).all(|w| w[0] < w[1]));
                assert!(ptr.options.contains(&ptr.chosen));
            }
            if b == 0 {
                assert!(rows.type_set.is_none());
                assert!(rows.split.is_empty());
            } else {
                let ts = rows.type_set.as_ref().expect("type_set when B > 0");
                assert_eq!(ts.kind, DecisionKind::TypeSet);
                let t_size = ts.chosen.count_ones() as usize;
                assert!((1..=b).contains(&t_size));
                if t_size == 1 {
                    assert!(rows.split.is_empty());
                } else {
                    assert_eq!(rows.split.len(), b - t_size);
                    for sp in &rows.split {
                        assert_eq!(sp.kind, DecisionKind::Split);
                        assert_eq!(sp.options.len(), t_size);
                        assert!(sp.options.windows(2).all(|w| w[0] < w[1]));
                        assert!(sp.options.contains(&sp.chosen));
                    }
                }
            }
        }

        // Group counts: 15 of 3, 12 of 2, 1 of 1; exact role & deck rules.
        let pairings = pod_pairings();
        let mut group_counts = BTreeMap::new();
        let mut size_histogram = BTreeMap::new();
        for game in &pod.games {
            group_counts
                .entry(game.group)
                .or_default()
                .push(game);
        }
        assert_eq!(group_counts.len(), POD_GAMES);
        for (g, &(low, high)) in pairings.iter().enumerate() {
            let group_games = &group_counts[&(g as u32)];
            let expected_len = 1
                + usize::from(roles[low] == ChairRole::Learning)
                + usize::from(roles[high] == ChairRole::Learning);
            assert_eq!(group_games.len(), expected_len, "group {g} ({low}, {high})");
            *size_histogram.entry(expected_len).or_insert(0usize) += 1;
            assert_eq!(group_games[0].role, GameRole::Reference);
            for sib in &group_games[1..] {
                assert_eq!(sib.role, GameRole::Sibling);
                assert_eq!(sib.chairs, group_games[0].chairs);
            }
        }
        assert_eq!(size_histogram.get(&3), Some(&15));
        assert_eq!(size_histogram.get(&2), Some(&12));
        assert_eq!(size_histogram.get(&1), Some(&1));

        // 6 sibling pairs of 7 opponents / games each, matched by position.
        let learning_chairs: Vec<u8> = (0..SEATS)
            .filter(|&c| roles[c] == ChairRole::Learning)
            .map(|c| c as u8)
            .collect();
        let pb_chairs: Vec<u8> = pod.paired_builds.iter().map(|pb| pb.chair).collect();
        assert_eq!(pb_chairs, learning_chairs);
        for pb in &pod.paired_builds {
            assert_eq!(pb.kind, PairedBuildKind::Sibling);
            assert_eq!(pb.opponents.len(), 7);
            assert_eq!(pb.games_x.len(), 7);
            assert_eq!(pb.games_y.len(), 7);
            for i in 0..7 {
                let gx = &pod.games[pb.games_x[i] as usize];
                let gy = &pod.games[pb.games_y[i] as usize];
                assert_eq!(gx.group, gy.group);
                assert_eq!(gx.role, GameRole::Reference);
                assert_eq!(gy.role, GameRole::Sibling);
                assert!(gx.decks.contains(&pb.deck_x));
                assert!(gx.decks.contains(&pb.opponents[i]));
                assert!(gy.decks.contains(&pb.deck_y));
                assert!(gy.decks.contains(&pb.opponents[i]));
            }
        }
    }

    // Verify ReadPlan (`seat_rule: "schedule"`, `max_replays: 0`):
    // halts are recorded without replaying, and `max_replays > 0` is refused.
    let draft = run_draft(&pool, 99, &mut uniform_pickers(99)).unwrap();
    let d0 = UniformFourStepBuilder
        .build(&draft.pools[0], &mut SplitMix64::seed(1))
        .unwrap()
        .deck
        .card_ids();
    let d1 = UniformFourStepBuilder
        .build(&draft.pools[1], &mut SplitMix64::seed(2))
        .unwrap()
        .deck
        .card_ids();
    let mut read_pilot = ScriptedPilot::natural_only();
    let (_, env0, _) = attempt_seeds(777, 0);
    read_pilot
        .halt_by_episode_env
        .insert((0, env0), (GameEnd::DecisionCap, None));

    let plan = ReadPlan {
        pod_use: PodUse::ReadB,
        namespace: "read-b".to_owned(),
        run: "r0".to_owned(),
        pod_index: 0,
        seeds: Default::default(),
        pins: PodPins::default(),
        pilot: PilotInfo::uniform(),
        caps: PodCaps::read_a2(),
        chairs: Vec::new(),
        decks: vec![
            ReadDeckSpec {
                source: DeckSource::Build,
                chair: Some(0),
                build: Some(0),
                builder: None,
                build_seed: None,
                cards: d0,
                build_rows: None,
                dealt: None,
            },
            ReadDeckSpec {
                source: DeckSource::Dealt,
                chair: None,
                build: None,
                builder: None,
                build_seed: None,
                cards: d1,
                build_rows: None,
                dealt: Some(DealtOrigin {
                    function: "deal_read_b_opponent".to_owned(),
                    key: json!(["read-b", "r0", 0]),
                }),
            },
        ],
        games: vec![
            ReadGameSpec {
                group: 0,
                role: GameRole::ReadB,
                pairing: None,
                decks: [0, 1],
                chairs: [Some(0), None],
                seats: [0, 1],
                game_seed: 777,
                episode: 0,
            },
            ReadGameSpec {
                group: 0,
                role: GameRole::ReadB,
                pairing: None,
                decks: [1, 0],
                chairs: [None, Some(0)],
                seats: [1, 0],
                game_seed: 778,
                episode: 1,
            },
        ],
        paired_builds: Vec::new(),
        stop_on_halt: false,
    };
    let read_rec = run_read_plan(&plan, &read_pilot, 2).unwrap();
    assert!(read_rec.draft.is_none());
    assert_eq!(read_rec.status, PodStatus::Complete);
    assert_eq!(read_rec.games[0].seat_rule, SeatRule::Schedule);
    assert_eq!(read_rec.games[0].attempts.len(), 1);
    assert!(read_rec.games[0].attempts[0].halted);
    assert_eq!(read_rec.games[0].scores, None);
    assert_eq!(read_rec.halts.len(), 1);
    assert!(read_rec.games[1].scores.is_some());

    let mut bad_plan = plan.clone();
    bad_plan.caps.max_replays = 1;
    assert!(matches!(
        run_read_plan(&bad_plan, &read_pilot, 1),
        Err(PodRunnerError::InvalidCaps(_))
    ));

    // Verify BatchReport and within-pair sibling correlation across 2 pods.
    let roles = training_roles_with_older([1, 5]).unwrap();
    let batch_cfg = UniformBatchConfig {
        pod_use: PodUse::Test,
        namespace: "test".to_owned(),
        run: "report".to_owned(),
        pod_indices: vec![0, 1],
        roles,
        pins: PodPins::default(),
        pilot_info: PilotInfo::uniform(),
        caps: PodCaps::training_a2(),
        workers: 4,
    };
    let records = run_uniform_training_batch(&pool, &batch_cfg, &pilot).unwrap();
    let batch_report = BatchReport::from_records(&records);
    assert_eq!(batch_report.pods.len(), 2);
    assert_eq!(batch_report.games_by_end_kind.get("natural"), Some(&140));
    assert_eq!(batch_report.total_halts, 0);
    assert_eq!(batch_report.total_replays, 0);
    let corr = sibling_correlation(&records);
    assert_eq!(corr.game_pairs, 84); // 2 pods * 6 learning chairs * 7 opponents
    assert_eq!(corr.build_pairs, 12);
    assert!(corr.game_correlation.is_some());
    assert!(corr.build_mean_correlation.is_some());
}

// ---------------------------------------------------------------------------
// Acceptance test 10: Frozen JSON schema and key order
// ---------------------------------------------------------------------------

#[test]
fn acceptance_10_schema() {
    let pool = pool();
    let roles = training_roles_with_older([1, 5]).unwrap();
    let cfg = TrainingPodConfig::uniform_a2(PodUse::Training, "test", "schema", 0, 4);

    // 1. Complete training pod record validates cleanly.
    let mut chairs = uniform_training_chairs(roles);
    let complete =
        run_training_pod(&pool, &cfg, &mut chairs, &ScriptedPilot::natural_only()).unwrap();
    let complete_json = complete.to_json_line();
    validate_pod_record_json(&complete_json).expect("complete record matches frozen schema");

    // 2. Stopped pod record (non-null `stop` and non-empty `halts`) validates cleanly.
    let target_seed = complete.games[0].game_seed;
    let mut stop_pilot = ScriptedPilot::natural_only();
    for attempt in 0u8..4 {
        stop_pilot.halt_by_episode_env.insert(
            (0, attempt_seeds(target_seed, attempt).1),
            (GameEnd::DecisionCap, Some(1)),
        );
    }
    let mut chairs_stop = uniform_training_chairs(roles);
    let stopped = unwrap_complete_or_stopped(run_training_pod(
        &pool,
        &cfg,
        &mut chairs_stop,
        &stop_pilot,
    ));
    let stopped_json = stopped.to_json_line();
    validate_pod_record_json(&stopped_json).expect("stopped record matches frozen schema");

    // 3. Reject missing top-level field (`"stop":null`).
    let missing_stop = complete_json.replace(",\"stop\":null}", "}");
    assert_ne!(missing_stop, complete_json);
    assert!(
        validate_pod_record_json(&missing_stop).is_err(),
        "missing top-level field must fail"
    );

    // 4. Reject extra top-level field.
    let extra_top = complete_json.replace(",\"stop\":null}", ",\"stop\":null,\"extra\":1}");
    assert_ne!(extra_top, complete_json);
    assert!(
        validate_pod_record_json(&extra_top).is_err(),
        "extra top-level field must fail"
    );

    // 5. Reject out-of-order top-level keys (`halts` and `status` swapped).
    let swapped_top = complete_json.replace(
        "\"halts\":[],\"status\":\"complete\"",
        "\"status\":\"complete\",\"halts\":[]",
    );
    assert_ne!(swapped_top, complete_json);
    assert!(
        validate_pod_record_json(&swapped_top).is_err(),
        "swapped top-level keys must fail"
    );

    // 6. Reject missing nested field (`caps.max_replays`).
    let missing_nested = complete_json.replace(",\"max_replays\":3}", "}");
    assert_ne!(missing_nested, complete_json);
    assert!(
        validate_pod_record_json(&missing_nested).is_err(),
        "missing nested field must fail"
    );

    // 7. Reject extra nested field (`pilot.extra`).
    let extra_nested = complete_json.replace(
        "\"decoding\":\"uniform-index-splitmix64-v1\"}",
        "\"decoding\":\"uniform-index-splitmix64-v1\",\"extra\":true}",
    );
    assert_ne!(extra_nested, complete_json);
    assert!(
        validate_pod_record_json(&extra_nested).is_err(),
        "extra nested field must fail"
    );

    // 8. Reject out-of-order nested keys (`seeds.pod_seed` before `seeds.draft_seed`).
    let draft_seed = complete.seeds.draft_seed.unwrap();
    let pod_seed = complete.seeds.pod_seed.unwrap();
    let swapped_nested = complete_json.replace(
        &format!("\"draft_seed\":{draft_seed},\"pod_seed\":{pod_seed}"),
        &format!("\"pod_seed\":{pod_seed},\"draft_seed\":{draft_seed}"),
    );
    assert_ne!(swapped_nested, complete_json);
    assert!(
        validate_pod_record_json(&swapped_nested).is_err(),
        "swapped nested keys must fail"
    );

    // 9. Reject missing field inside `stop` report.
    let missing_stop_field = stopped_json.replace(
        ",\"attempts\":[{\"deal_seed\"",
        ",\"bad\":[{\"deal_seed\"",
    );
    assert_ne!(missing_stop_field, stopped_json);
    assert!(
        validate_pod_record_json(&missing_stop_field).is_err(),
        "invalid stop report key must fail"
    );
}

#[test]
fn watchdog_expiry_voids_pod_without_classifying_games() {
    struct SlowPilot;

    impl GamePilot for SlowPilot {
        fn play(&self, spec: &AttemptSpec<'_>, _caps: &GameCaps) -> AttemptOutcome {
            std::thread::sleep(std::time::Duration::from_millis(1_150));
            AttemptOutcome {
                decisions: 1,
                physical_decisions: Some(1),
                policy_steps: Some(1),
                end: GameEnd::Natural {
                    winner_seat: Some(spec.seats[0]),
                    reason: "life_zero".to_owned(),
                },
                last_action_source: None,
            }
        }
    }

    let pool = pool();
    let roles = training_roles_with_older([1, 5]).unwrap();
    let mut cfg = TrainingPodConfig::uniform_a2(PodUse::Test, "test", "watchdog", 0, 1);
    cfg.caps.watchdog_seconds = 1;
    let mut chairs = uniform_training_chairs(roles);
    let err = run_training_pod(&pool, &cfg, &mut chairs, &SlowPilot)
        .expect_err("watchdog expiry must void the pod run");
    assert!(
        matches!(
            err,
            PodRunnerError::WatchdogExpired {
                pod_index: 0,
                watchdog_seconds: 1,
                ..
            }
        ),
        "expected WatchdogExpired, got {err:?}"
    );
}
