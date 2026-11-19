//! Library tests of the campaign runner: inventory pins, schedule, seat
//! streams, the Gate 2 rule, game collection on short (truncated) games
//! with one and several workers, the update's treatment of truncated games,
//! the control arm's advantages, the dev score, resume cleaning and the
//! guarded launcher. The full-length game path runs in the bounded smoke.
use super::*;
use crate::current_tree_learner_v2::ModelV2;
use crate::limited_campaign_v1::collect::{play_game_v1, GameSpecV1, RetainedV1};
use crate::limited_campaign_v1::inventory::{
    canonical_pool_bytes_v1, default_repo_root_v1, SAMPLE_DIR_V1, SAMPLE_SHA256_IN_POOL_ORDER_V1,
};
use crate::limited_campaign_v1::schedule::{
    actor_seed_v1, league_snapshot_steps_v1, leg_layout_v1, SlotV1, GATE2_REQUIRED_DECK_V1,
};
use crate::limited_match_v1::{deal_pair_v1, DeckFileV1, SessionKindV1};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "limited-campaign-{name}-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    root
}

fn inventory() -> InventoryV1 {
    read_inventory_v1(&default_repo_root_v1()).unwrap()
}

fn train_decks() -> Vec<DeckV1> {
    load_train_decks_v1(&default_repo_root_v1(), &inventory()).unwrap()
}

#[test]
fn inventory_matches_the_design_pins_with_pool_tsv_in_crlf_form() {
    let inventory = inventory();
    assert_eq!(
        inventory.sample_sha256_in_pool_order,
        SAMPLE_SHA256_IN_POOL_ORDER_V1
    );
    assert_eq!(inventory.rows.len(), 80);
    // The CRLF form is the same from LF, CRLF and mixed checkouts, as the
    // inventory tool's canonical_pool_bytes makes it.
    assert_eq!(canonical_pool_bytes_v1(b"a\nb\r\nc\r"), b"a\r\nb\r\nc\r");
    assert_eq!(
        canonical_pool_bytes_v1(b"a\r\nb\r\n"),
        canonical_pool_bytes_v1(b"a\nb\n")
    );
    // A reordered pool.tsv no longer matches the pin.
    let root = temp_root("pool");
    let sample = root.join(SAMPLE_DIR_V1);
    fs::create_dir_all(sample.join("decks")).unwrap();
    let source = default_repo_root_v1().join(SAMPLE_DIR_V1);
    for entry in fs::read_dir(source.join("decks")).unwrap() {
        let path = entry.unwrap().path();
        fs::copy(&path, sample.join("decks").join(path.file_name().unwrap())).unwrap();
    }
    let pool = fs::read_to_string(source.join("pool.tsv")).unwrap();
    let mut lines = pool.lines().collect::<Vec<_>>();
    lines.swap(1, 2);
    fs::write(sample.join("pool.tsv"), lines.join("\n") + "\n").unwrap();
    let error = read_inventory_v1(&root).unwrap_err();
    assert!(error.contains("sample_sha256_in_pool_order"), "{error}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn train_decks_resolve_at_their_registered_counts_and_never_meet_an_eval_deck() {
    let inventory = inventory();
    let decks = train_decks();
    assert_eq!(decks.len(), 40);
    for (deck, row) in decks
        .iter()
        .zip(inventory.rows.iter().filter(|row| row.split == "train"))
    {
        assert_eq!(deck.name, row.deck);
        assert_eq!(deck.cards.len(), row.cards);
        assert_eq!(deck.source_sha256, row.source_sha256);
    }
    let oversize = decks
        .iter()
        .find(|deck| deck.name == GATE2_REQUIRED_DECK_V1)
        .unwrap();
    assert_eq!(oversize.cards.len(), 41);
    // The eval split, read here only to prove disjointness, shares no name
    // and no card multiset with the train split.
    let train_names = decks
        .iter()
        .map(|d| d.name.clone())
        .collect::<BTreeSet<_>>();
    let train_sets = decks.iter().map(DeckV1::multiset).collect::<BTreeSet<_>>();
    for row in inventory.rows.iter().filter(|row| row.split == "eval") {
        assert!(!train_names.contains(&row.deck));
        let path = default_repo_root_v1()
            .join(SAMPLE_DIR_V1)
            .join("decks")
            .join(format!("{}.json", row.deck));
        let file: DeckFileV1 = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let mut cards = file.resolve_for(SessionKindV1::FullPoolV1).unwrap();
        cards.sort_unstable();
        assert!(!train_sets.contains(&cards), "{}", row.deck);
    }
}

#[test]
fn deals_and_seat_layouts_follow_section_2_2() {
    for pair in 0..64 {
        let deal = deal_v1(TRAINING_SCHEDULE_SEED_V1, pair, 40).unwrap();
        let (decks, env_seed, starting) =
            deal_pair_v1(TRAINING_SCHEDULE_SEED_V1, pair, 40).unwrap();
        assert_eq!(
            (deal.decks, deal.env_seed, deal.starting),
            (decks, env_seed, starting)
        );
        assert_ne!(deal.decks[0], deal.decks[1]);
        let symmetric = [0, 1].map(|leg| leg_layout_v1(true, &deal, leg));
        assert_eq!(symmetric[0], ([SlotV1::A, SlotV1::A], starting));
        assert_eq!(symmetric[1], ([SlotV1::A, SlotV1::A], starting.opponent()));
        let asymmetric = [0, 1].map(|leg| leg_layout_v1(false, &deal, leg));
        assert_eq!(asymmetric[0], ([SlotV1::A, SlotV1::B], starting));
        assert_eq!(asymmetric[1], ([SlotV1::B, SlotV1::A], starting));
    }
}

#[test]
fn batches_league_draws_and_seat_streams_follow_the_schedule() {
    assert_eq!(update_pairs_v1(0), 0..16);
    assert_eq!(update_pairs_v1(3), 48..64);
    let league = update_pairs_v1(5)
        .filter(|&p| is_league_pair_v1(p))
        .collect::<Vec<_>>();
    assert_eq!(league, vec![92, 93, 94, 95]);
    assert_eq!(league_snapshot_steps_v1(0), vec![0]);
    assert_eq!(league_snapshot_steps_v1(1), vec![0]);
    assert_eq!(league_snapshot_steps_v1(10), vec![0]);
    assert_eq!(league_snapshot_steps_v1(11), vec![0, 10]);
    assert_eq!(league_snapshot_steps_v1(30), vec![0, 10, 20]);
    assert_eq!(league_snapshot_steps_v1(31), vec![0, 10, 20, 30]);
    let mut counts = BTreeMap::<u64, u64>::new();
    for pair in 0..4_000 {
        let step = league_opponent_step_v1(TRAINING_SCHEDULE_SEED_V1, 31, pair);
        assert_eq!(
            step,
            league_opponent_step_v1(TRAINING_SCHEDULE_SEED_V1, 31, pair)
        );
        *counts.entry(step).or_default() += 1;
    }
    assert_eq!(
        counts.keys().copied().collect::<Vec<_>>(),
        vec![0, 10, 20, 30]
    );
    assert!(counts.values().all(|&count| (850..1_150).contains(&count)));
    assert_eq!(league_opponent_step_v1(TRAINING_SCHEDULE_SEED_V1, 0, 12), 0);
    // Seat streams never coincide across pairs, legs, seats or seeds.
    let mut seeds = BTreeSet::new();
    for seed in [
        TRAINING_SCHEDULE_SEED_V1,
        DEV_SEED_V1,
        QUALIFICATION_SEED_V1,
    ] {
        for pair in 0..64 {
            for leg in 0..2 {
                for seat in 0..2 {
                    assert!(seeds.insert(actor_seed_v1(seed, pair, leg, seat)));
                }
            }
        }
    }
}

#[test]
fn every_campaign_deal_uses_train_decks_only() {
    let inventory = inventory();
    let train = inventory
        .rows
        .iter()
        .filter(|row| row.split == "train")
        .map(|row| row.deck.clone())
        .collect::<Vec<_>>();
    let eval = inventory
        .rows
        .iter()
        .filter(|row| row.split == "eval")
        .map(|row| row.deck.clone())
        .collect::<BTreeSet<_>>();
    let mut deals = Vec::new();
    for step in 0..200 {
        deals.extend(update_pairs_v1(step).map(|pair| (TRAINING_SCHEDULE_SEED_V1, pair)));
    }
    deals.extend((0..DEV_PAIRS_V1).map(|pair| (DEV_SEED_V1, pair)));
    let gate2 = gate2_pairs_v1(&train, &DRAKE_HATCHER_TRAIN_DECKS_V1).unwrap();
    deals.extend(
        gate2
            .indices
            .iter()
            .map(|&pair| (QUALIFICATION_SEED_V1, pair)),
    );
    for (seed, pair) in deals {
        let deal = deal_v1(seed, pair, train.len()).unwrap();
        for index in deal.decks {
            assert!(!eval.contains(&train[index]));
        }
    }
}

#[test]
fn gate2_rule_keeps_or_replaces_indices_as_ruled() {
    let names = train_decks()
        .iter()
        .map(|deck| deck.name.clone())
        .collect::<Vec<_>>();
    let pairs = gate2_pairs_v1(&names, &DRAKE_HATCHER_TRAIN_DECKS_V1).unwrap();
    assert_eq!(pairs.indices.len(), 8);
    assert_eq!(pairs.seed, QUALIFICATION_SEED_V1);
    let decks_of = |pair: u64| {
        deal_v1(QUALIFICATION_SEED_V1, pair, 40)
            .unwrap()
            .decks
            .map(|index| names[index].clone())
    };
    let all = pairs
        .indices
        .iter()
        .flat_map(|&p| decks_of(p))
        .collect::<Vec<_>>();
    assert!(all.iter().any(|name| name == GATE2_REQUIRED_DECK_V1));
    assert!(all
        .iter()
        .any(|name| DRAKE_HATCHER_TRAIN_DECKS_V1.contains(&name.as_str())));
    // Synthetic names: put the two required decks where pairs 0 to 7 never
    // deal, so the rule must replace index 7 and then 6 by the lowest later
    // pairs that hold them.
    let dealt = (0..8)
        .flat_map(|pair| deal_v1(QUALIFICATION_SEED_V1, pair, 40).unwrap().decks)
        .collect::<BTreeSet<_>>();
    let mut free = (0..40).filter(|index| !dealt.contains(index));
    let (curator, drake) = (free.next().unwrap(), free.next().unwrap());
    let mut synthetic = (0..40).map(|i| format!("deck_{i:02}")).collect::<Vec<_>>();
    synthetic[curator] = GATE2_REQUIRED_DECK_V1.into();
    synthetic[drake] = "drake_deck".into();
    let ruled = gate2_pairs_v1(&synthetic, &["drake_deck"]).unwrap();
    let holds = |pair: u64, index: usize| {
        deal_v1(QUALIFICATION_SEED_V1, pair, 40)
            .unwrap()
            .decks
            .contains(&index)
    };
    let first_curator = (8..).find(|&pair| holds(pair, curator)).unwrap();
    let first_drake = (8..)
        .find(|&pair| pair != first_curator && holds(pair, drake))
        .unwrap();
    let drake_needed = !holds(first_curator, drake);
    let mut expected = (0..7).collect::<Vec<u64>>();
    if drake_needed {
        expected[6] = first_drake;
    }
    expected.push(first_curator);
    expected.sort_unstable();
    assert_eq!(ruled.indices, expected);
    assert_eq!(ruled.replaced[0], (7, first_curator));
    if drake_needed {
        assert_eq!(ruled.replaced[1], (6, first_drake));
    }
}

fn short_limits() -> GameLimitsV1 {
    GameLimitsV1 {
        max_decisions: 24,
        ..CAMPAIGN_LIMITS_V1
    }
}

fn short_specs<'a>(
    decks: &'a [DeckV1],
    a: &SeatPolicyV1<'a>,
    b: &SeatPolicyV1<'a>,
    pairs: std::ops::Range<u64>,
    symmetric: bool,
) -> Vec<GameSpecV1<'a>> {
    let mut specs = Vec::new();
    for pair in pairs {
        let plan = PairPlanV1 {
            deal: deal_v1(TRAINING_SCHEDULE_SEED_V1, pair, decks.len()).unwrap(),
            symmetric,
            stream_seed: TRAINING_SCHEDULE_SEED_V1,
            train: true,
            limits: short_limits(),
        };
        specs.extend(pair_specs_v1(plan, decks, a, b));
    }
    specs
}

#[test]
fn short_games_are_truncated_and_identical_for_any_worker_count_and_diagnostics() {
    let decks = train_decks();
    let model = CheckpointPolicyV1::ActorCriticH32(ModelV2::deterministic_initialization());
    let other = CheckpointPolicyV1::ActorCriticH16(
        crate::current_tree_learner_v2::ModelV2H16::deterministic_initialization(),
    );
    let a = SeatPolicyV1 {
        policy: &model,
        label: "learner:step_0000".into(),
    };
    let b = SeatPolicyV1 {
        policy: &other,
        label: "league:step_0000".into(),
    };
    let mut specs = short_specs(&decks, &a, &b, 0..2, true);
    specs.extend(short_specs(&decks, &a, &b, 12..13, false));
    let one = play_games_v1(&specs, 1, None);
    let root = temp_root("diagnostics");
    fs::create_dir_all(&root).unwrap();
    let diagnostics = DiagnosticsV1::open(&root).unwrap();
    let three = play_games_v1(&specs, 3, Some(&diagnostics));
    assert_eq!(one.len(), 6);
    for (first, second) in one.iter().zip(&three) {
        assert_eq!(first.receipt, second.receipt);
        assert_eq!(
            first.terminal,
            TerminalV1::Truncated("max_decisions".into())
        );
        assert_eq!(first.receipt.decisions, 24);
        assert_eq!(first.retained.len(), second.retained.len());
        for (x, y) in first.retained.iter().zip(&second.retained) {
            assert_eq!((x.seat, x.selected), (y.seat, y.selected));
            assert!(x.input.packet().actions().len() > 1);
        }
        let retained = first.receipt.retained_decisions;
        assert_eq!(retained[0] + retained[1], first.retained.len() as u64);
        assert!(first.receipt.forced_decisions + first.retained.len() as u64 <= 24);
        assert!(first.stages.is_none() && second.stages.is_some());
    }
    // Self-play retains both seats; in a league pair only policy A's seat.
    assert_eq!(one[0].receipt.learner_seats, [true, true]);
    assert_eq!(one[4].receipt.learner_seats, [true, false]);
    assert_eq!(one[5].receipt.learner_seats, [false, true]);
    assert_eq!(one[5].receipt.retained_decisions[0], 0);
    // The two legs of a symmetric pair start from opposite seats.
    assert_ne!(
        one[0].receipt.starting_player,
        one[1].receipt.starting_player
    );
    assert_eq!(
        one[4].receipt.starting_player,
        one[5].receipt.starting_player
    );
    // A different seat stream changes the game.
    let mut shifted = short_specs(&decks, &a, &b, 0..1, true);
    shifted[0].stream_seed += 1;
    assert_ne!(
        play_game_v1(&shifted[0], None).receipt.action_trace_sha256,
        one[0].receipt.action_trace_sha256
    );
    drop(diagnostics);
    let lines = fs::read_to_string(root.join("stage-diagnostics.jsonl")).unwrap();
    assert_eq!(lines.lines().count(), 12);
    fs::remove_dir_all(root).unwrap();
}

/// A real batch of short games with chosen terminals, for the update tests.
fn records_with(terminals: &[TerminalV1]) -> Vec<GameRecordV1> {
    let decks = train_decks();
    let model = CheckpointPolicyV1::ActorCriticH32(ModelV2::deterministic_initialization());
    let a = SeatPolicyV1 {
        policy: &model,
        label: "learner:step_0000".into(),
    };
    let pairs = (terminals.len() as u64).div_ceil(2);
    let specs = short_specs(&decks, &a, &a, 0..pairs, true);
    let mut records = play_games_v1(&specs, 2, None);
    records.truncate(terminals.len());
    for (record, terminal) in records.iter_mut().zip(terminals) {
        record.terminal = terminal.clone();
    }
    records
}

#[test]
fn truncated_games_add_nothing_to_the_actor_critic_update() {
    let natural = TerminalV1::Natural([1, -1]);
    let truncated = TerminalV1::Truncated("max_decisions".into());
    let only_truncated = records_with(&[truncated.clone(), truncated.clone()]);
    let mut learner = initial_learner_v1(LearnerKindV1::ActorCriticH32, 4).unwrap();
    let before = learner.snapshot();
    assert_eq!(
        update_learner_v1(&mut learner, &only_truncated).unwrap(),
        None
    );
    assert_eq!(learner.snapshot(), before);
    let mixed = records_with(&[natural.clone(), truncated.clone()]);
    let natural_decisions = mixed[0].retained.len() as u64;
    let report = update_learner_v1(&mut learner, &mixed).unwrap().unwrap();
    assert_eq!(report["step"], 1);
    assert_eq!(report["decisions"], natural_decisions);
    assert_eq!(report["natural_trajectories"], 2);
    assert_eq!(report["truncated_trajectories"], 2);
    // The same natural game alone gives the same update bits.
    let mut alone = initial_learner_v1(LearnerKindV1::ActorCriticH32, 4).unwrap();
    let single = records_with(&[natural]);
    update_learner_v1(&mut alone, &single).unwrap();
    assert_eq!(alone.snapshot(), learner.snapshot());
    // A failed game is refused.
    let failed = records_with(&[TerminalV1::Failed("halted".into())]);
    assert!(update_learner_v1(&mut alone, &failed).is_err());
}

#[test]
fn control_arm_centers_each_return_on_the_same_deck_in_the_other_leg() {
    // Pair 0: seat 0 wins leg 0 and draws leg 1; pair 1 leg 1 is truncated.
    let records = records_with(&[
        TerminalV1::Natural([1, -1]),
        TerminalV1::Natural([0, 0]),
        TerminalV1::Natural([-1, 1]),
        TerminalV1::Truncated("max_turns".into()),
    ]);
    let count = |game: usize, seat: u8| {
        records[game]
            .retained
            .iter()
            .filter(|decision: &&RetainedV1| decision.seat == seat)
            .count() as u64
    };
    let mut learner = initial_learner_v1(LearnerKindV1::PairCenteredControl, 4).unwrap();
    let report = update_learner_v1(&mut learner, &records).unwrap().unwrap();
    // Seat 0: (1 - 0) / 2 = 1/2 in leg 0 (rewards 1 and 0), (0 - 1) / 2 in
    // leg 1 (rewards -1 and 0); seat 1 the mirror. Pair 1 has no natural
    // other leg, so its natural leg's advantage is 0 (rewards 0 and 0).
    let ones = count(0, 0) + count(1, 1);
    let minus_ones = count(0, 1) + count(1, 0);
    let zeros = ones + minus_ones + 2 * (count(2, 0) + count(2, 1));
    assert_eq!(report["reward_counts"]["1"].as_u64().unwrap_or(0), ones);
    assert_eq!(
        report["reward_counts"]["-1"].as_u64().unwrap_or(0),
        minus_ones
    );
    assert_eq!(report["reward_counts"]["0"].as_u64().unwrap_or(0), zeros);
    assert_eq!(
        report["doubled_examples"],
        2 * (count(0, 0) + count(0, 1) + count(1, 0) + count(1, 1) + count(2, 0) + count(2, 1))
    );
}

#[test]
fn dev_score_counts_wins_draws_and_truncations_for_the_candidate_seat() {
    let records = records_with(&[
        TerminalV1::Natural([1, -1]),
        TerminalV1::Natural([1, -1]),
        TerminalV1::Natural([0, 0]),
        TerminalV1::Truncated("max_turns".into()),
    ]);
    // The candidate plays seat `leg`: win, loss, draw, truncated.
    assert_eq!(dev_half_points_v1(&records).unwrap(), 2 + 1 + 1);
    let failed = records_with(&[TerminalV1::Failed("x".into())]);
    assert!(dev_half_points_v1(&failed).is_err());
}

#[test]
fn resume_cleaning_removes_uncommitted_files_and_refuses_gaps() {
    let root = temp_root("clean");
    fixture_chain(&root, &fixture_config(ModeV1::Smoke, 2), 1);
    let staging = root.join("iterations").join(".iter_0002.staging");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("receipt.json"), b"{}").unwrap();
    fs::write(checkpoint_path_v1(&root, 2), b"{}").unwrap();
    fs::write(
        root.join("checkpoints").join(".step_0003.json.staging"),
        b"",
    )
    .unwrap();
    assert_eq!(committed_head(&root).unwrap(), Some(1));
    clean_uncommitted(&root, 1).unwrap();
    assert!(!staging.exists());
    assert!(!checkpoint_path_v1(&root, 2).exists());
    assert!(checkpoint_path_v1(&root, 1).exists());
    assert_eq!(fs::read_dir(root.join("checkpoints")).unwrap().count(), 2);
    // A committed iteration after a missing one is a gap.
    fs::create_dir_all(iteration_dir_v1(&root, 3)).unwrap();
    fs::write(iteration_dir_v1(&root, 3).join("complete.json"), b"{}").unwrap();
    assert_eq!(committed_head(&root).unwrap(), Some(1));
    assert!(clean_uncommitted(&root, 1).is_err());
    fs::remove_dir_all(root).unwrap();
}

fn identity() -> SourceIdentityV1 {
    SourceIdentityV1 {
        git_head: "0123456789abcdef0123456789abcdef01234567".into(),
        binary_sha256: "11".repeat(32),
        binary_source_sha256: runner_source_sha256_v1(),
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        sample_sha256_in_pool_order: SAMPLE_SHA256_IN_POOL_ORDER_V1.into(),
        actor_profile: full_pool::PROFILE_V1.into(),
        actor_producer_sha256: "44".repeat(32),
        hand_knowledge: HAND_KNOWLEDGE_V1.into(),
        reveal_producer_sha256: "55".repeat(32),
        reveal_reducer_sha256: "66".repeat(32),
    }
}

fn host() -> HostClassV1 {
    HostClassV1 {
        short_name: "linux-9950x".into(),
        cpu_model: "AMD Ryzen 9 9950X 16-Core Processor".into(),
        os_family: "linux".into(),
        target_triple: "x86_64-unknown-linux-gnu".into(),
        trainer_binary_sha256: "11".repeat(32),
    }
}

#[test]
fn receipt_host_classes_follow_the_ruled_names() {
    admit_receipt_host_class_v1(&host(), 32).unwrap();
    let with = |change: &dyn Fn(&mut HostClassV1)| {
        let mut host = host();
        change(&mut host);
        host
    };
    let windows = |name: &str, cpu: &str| {
        with(&|h| {
            h.short_name = name.into();
            h.cpu_model = cpu.into();
            h.os_family = "windows".into();
            h.target_triple = "x86_64-pc-windows-msvc".into();
        })
    };
    admit_receipt_host_class_v1(
        &windows("win11-13700k", "13th Gen Intel(R) Core(TM) i7-13700K"),
        24,
    )
    .unwrap();
    admit_receipt_host_class_v1(
        &windows("win11-5900xt", "AMD Ryzen 9 5900XT 16-Core Processor"),
        32,
    )
    .unwrap();
    assert!(
        admit_receipt_host_class_v1(&windows("win11-13700k", "AMD Ryzen 9 5900XT"), 32).is_err()
    );
    let epyc = with(&|h| {
        h.short_name = "runpod-epyc-16".into();
        h.cpu_model = "AMD EPYC 9554 64-Core Processor".into();
    });
    admit_receipt_host_class_v1(&epyc, 16).unwrap();
    assert!(admit_receipt_host_class_v1(&epyc, 32).is_err());
    let mac = with(&|h| {
        h.short_name = "engineering-unlisted".into();
        h.cpu_model = "Apple M3 Ultra".into();
        h.os_family = "macos".into();
    });
    assert!(admit_receipt_host_class_v1(&mac, 28).is_err());
    assert!(admit_receipt_host_class_v1(&with(&|h| h.os_family = "windows".into()), 32).is_err());
    assert!(admit_receipt_host_class_v1(
        &with(&|h| h.cpu_model = "AMD Ryzen 9 9950X3D".into()),
        32,
    )
    .is_err());
    for model in ["13700KF", "13700KS", "13700KX", "13700K-ES", "13700KФ"] {
        let cpu = format!("13th Gen Intel(R) Core(TM) i7-{model}");
        assert!(admit_receipt_host_class_v1(&windows("win11-13700k", &cpu), 24).is_err());
    }
    for cpu in [
        "OtherCPU 13700K",
        "AMD Ryzen 9 9950XФ",
        "AMD Ryzen 9 9950X-ES",
    ] {
        assert!(admit_receipt_host_class_v1(&with(&|h| h.cpu_model = cpu.into()), 32).is_err());
    }
    assert!(admit_receipt_host_class_v1(&with(&|h| h.target_triple.clear()), 32).is_err());
    assert!(
        admit_receipt_host_class_v1(&with(&|h| h.trainer_binary_sha256 = "wrong".into()), 32,)
            .is_err()
    );
    admit_receipt_host_class_v1(
        &windows("win11-13700k", "Intel Core i7-13700k @ 3.40GHz"),
        24,
    )
    .unwrap();
}

#[test]
fn guarded_launcher_refuses_any_mismatch() {
    let learner = LearnerKindV1::ActorCriticH32;
    let identity = identity();
    assert_eq!(
        guard_launch_v1(None, 1, 4, learner, &identity, &host(), 32).unwrap(),
        None
    );
    assert!(guard_launch_v1(None, 200, 4, learner, &identity, &host(), 32).is_err());
    let empty = QualificationManifestV1 {
        schema: MANIFEST_SCHEMA_V1.into(),
        binary_source_sha256: identity.binary_source_sha256.clone(),
        feature_sha256: learner.feature_sha256(),
        card_db_hash: identity.card_db_hash.clone(),
        learner_contract: learner.contract().into(),
        host_class: host(),
        qualification_pairs: Gate2PairsV1 {
            rule: "rule".into(),
            seed: QUALIFICATION_SEED_V1,
            indices: (0..8).collect(),
            replaced: Vec::new(),
        },
        step_0001_sha256: "aa".repeat(32),
        evaluation_sha256: "bb".repeat(32),
        runs: Vec::new(),
        workers: 4,
        u_max: 200,
    };
    let root = temp_root("manifest");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("qualification_manifest.json");
    let check = |manifest: &QualificationManifestV1, u_max: u64, workers: usize| {
        fs::write(&path, manifest_bytes_v1(manifest).unwrap()).unwrap();
        guard_launch_v1(Some(&path), u_max, workers, learner, &identity, &host(), 32)
    };
    assert!(check(&empty, 200, 4).is_err());
    let config = fixture_config(ModeV1::Qualification, 1);
    let mut roots = Vec::new();
    for workers in [1, 2, 4, 8] {
        for diagnostics in [false, true] {
            let directory = root.join(format!("w{workers}-{diagnostics}"));
            qualification_fixture(&directory, &config, workers, diagnostics);
            roots.push(directory);
        }
    }
    let manifest = lock_manifest_v1(&roots, 4, 200, 32).unwrap();
    let digest = check(&manifest, 200, 4).unwrap().unwrap();
    assert_eq!(digest, sha256_hex(&fs::read(&path).unwrap()));
    assert!(check(&manifest, 199, 4).is_err());
    assert!(check(&manifest, 200, 8).is_err());
    assert!(guard_launch_v1(Some(&path), 1, 4, learner, &identity, &host(), 32).is_err());
    let refused = |change: &dyn Fn(&mut QualificationManifestV1)| {
        let mut changed = manifest.clone();
        change(&mut changed);
        assert!(check(&changed, changed.u_max, changed.workers).is_err());
    };
    refused(&|m| m.binary_source_sha256 = "cc".repeat(32));
    refused(&|m| m.feature_sha256 = "cc".repeat(32));
    refused(&|m| m.card_db_hash = "0000000000000000".into());
    refused(&|m| m.learner_contract = LearnerKindV1::ActorCriticH16.contract().into());
    refused(&|m| m.host_class.cpu_model = "AMD Ryzen 9 9950X3D".into());
    refused(&|m| m.host_class.trainer_binary_sha256 = "12".repeat(32));
    refused(&|m| m.workers = 1);
    refused(&|m| m.u_max = 99);
    refused(&|m| m.schema = "other".into());
    refused(&|m| m.runs.clear());
    refused(&|m| m.runs.retain(|run| run["workers"] != 1));
    refused(&|m| m.runs.retain(|run| run["stage_diagnostics"] != false));
    refused(&|m| m.qualification_pairs.rule = "arbitrary".into());
    refused(&|m| m.qualification_pairs.indices[0] = 999);
    refused(&|m| m.step_0001_sha256 = "ff".repeat(32));
    refused(&|m| m.evaluation_sha256 = "ff".repeat(32));
    refused(&|m| m.runs[0]["training_ns"] = json!(0));
    refused(&|m| m.runs[0]["speedup"] = json!(999.0));
    let timing_path = roots[0].join("timing.json");
    let run_path = roots[0].join("qualification_run.json");
    let timing_bytes = fs::read(&timing_path).unwrap();
    let run_bytes = fs::read(&run_path).unwrap();
    let mut timing = canonical_value(&timing_bytes).unwrap();
    let mut run = canonical_value(&run_bytes).unwrap();
    timing["training_ns"] = json!(0);
    run["timing"] = timing.clone();
    fs::write(&timing_path, to_bytes(&timing).unwrap()).unwrap();
    fs::write(&run_path, to_bytes(&run).unwrap()).unwrap();
    assert!(check(&manifest, 200, 4).is_err());
    fs::write(&timing_path, timing_bytes).unwrap();
    fs::write(&run_path, run_bytes).unwrap();
    // Non-canonical bytes are refused.
    let mut bytes = manifest_bytes_v1(&manifest).unwrap();
    bytes.push(b'\n');
    fs::write(&path, bytes).unwrap();
    assert!(guard_launch_v1(Some(&path), 200, 4, learner, &identity, &host(), 32).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn guarded_launcher_uses_recorded_evidence_under_a_different_cpu_allocation() {
    let root = temp_root("pinned-launch");
    fs::create_dir(&root).unwrap();
    let config = fixture_config(ModeV1::Qualification, 1);
    let mut roots = Vec::new();
    for workers in [1, 2, 4, 8] {
        for diagnostics in [false, true] {
            let directory = root.join(format!("w{workers}-{diagnostics}"));
            qualification_fixture(&directory, &config, workers, diagnostics);
            roots.push(directory.join("."));
        }
    }
    assert!(lock_manifest_v1(&roots, 4, 200, 4).is_err());
    let manifest = lock_manifest_v1(&roots, 4, 200, 32).unwrap();
    for (run, directory) in manifest.runs.iter().zip(&roots) {
        assert_eq!(
            PathBuf::from(run["directory"].as_str().unwrap()),
            fs::canonicalize(directory).unwrap()
        );
    }
    let path = root.join("qualification_manifest.json");
    let bytes = manifest_bytes_v1(&manifest).unwrap();
    fs::write(&path, &bytes).unwrap();
    let check = |available| {
        guard_launch_v1(
            Some(&path),
            200,
            4,
            LearnerKindV1::ActorCriticH32,
            &config.identity,
            &host(),
            available,
        )
    };
    for available in [4, 32] {
        assert_eq!(check(available).unwrap(), Some(sha256_hex(&bytes)));
    }
    assert!(check(3).is_err());

    // Keep the timing internally consistent, but change the recorded evidence.
    let timing_path = roots[0].join("timing.json");
    let run_path = roots[0].join("qualification_run.json");
    let mut timing = canonical_value(&fs::read(&timing_path).unwrap()).unwrap();
    timing["training_ns"] = json!(500_000_000u64);
    timing["completed_natural_games_per_sec"] = json!(32.0);
    timing["decisions_per_sec"] = json!(64.0);
    let mut run = canonical_value(&fs::read(&run_path).unwrap()).unwrap();
    run["timing"] = timing.clone();
    fs::write(&timing_path, to_bytes(&timing).unwrap()).unwrap();
    fs::write(&run_path, to_bytes(&run).unwrap()).unwrap();
    for available in [4, 32] {
        let error = check(available).unwrap_err();
        assert!(
            error.contains("differs from its qualification evidence"),
            "{error}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn runner_contract_states_what_the_rulings_require() {
    let text = RUNNER_CONTRACT_V1.join(" ");
    for phrase in [
        "replayable within one host class only",
        "dropped before a trajectory is built",
        "CRLF form",
        "left out of the policy loss and the value loss",
        "index 7 (then 6)",
        "(CPU model, OS family, target triple, trainer binary SHA-256)",
        "identical for any worker count",
    ] {
        assert!(text.contains(phrase), "{phrase}");
    }
    for line in RUNNER_CONTRACT_V1 {
        assert!(!line.contains('\u{2013}') && !line.contains('\u{2014}'));
    }
}

pub(crate) fn fixture_config(mode: ModeV1, u_max: u64) -> CampaignConfigV1 {
    let inventory = inventory();
    let decks = train_decks();
    let names = decks
        .iter()
        .map(|deck| deck.name.clone())
        .collect::<Vec<_>>();
    campaign_config_v1(ConfigInputs {
        mode,
        learner: LearnerKindV1::ActorCriticH32,
        u_max,
        identity: source_identity_v1(&identity().git_head, &identity().binary_sha256, &inventory),
        inventory: &inventory,
        decks: &decks,
        qualification_manifest_sha256: (mode == ModeV1::Campaign && u_max > 1)
            .then(|| "aa".repeat(32)),
        qualification_pairs: if mode == ModeV1::Qualification {
            Some(gate2_pairs_v1(&names, &DRAKE_HATCHER_TRAIN_DECKS_V1).unwrap())
        } else {
            None
        },
    })
}

fn fixture_games(config: &CampaignConfigV1, update: u64, dev: bool, half_points: u64) -> Value {
    let pairs = if dev {
        (0..config.dev_pairs).map(|pair| (pair, Some(0))).collect()
    } else {
        batch_pairs_v1(config, update - 1).unwrap()
    };
    let mut games = Vec::new();
    for (pair, opponent) in pairs {
        let seed = if dev {
            config.dev_seed
        } else {
            config.schedule_seed
        };
        let deal = deal_v1(seed, pair, config.train_decks.len()).unwrap();
        for leg in 0u8..2 {
            let (_, starting) = leg_layout_v1(opponent.is_none(), &deal, leg);
            let candidate = if dev {
                format!("candidate:step_{update:04}")
            } else {
                format!("learner:step_{:04}", update - 1)
            };
            let other = if dev {
                "step_0000".to_string()
            } else {
                opponent.map_or_else(
                    || candidate.clone(),
                    |step| format!("league:step_{step:04}"),
                )
            };
            let policies = if opponent.is_some() && leg == 1 {
                [other, candidate]
            } else {
                [candidate, other]
            };
            let learner_seats = if dev {
                [false, false]
            } else if opponent.is_none() {
                [true, true]
            } else {
                [leg == 0, leg == 1]
            };
            let candidate_wins = 2 * (games.len() as u64) < half_points;
            let p0_wins = !dev || candidate_wins == (leg == 0);
            let outcome = if p0_wins { "p0_win" } else { "p1_win" };
            games.push(GameReceiptV1 {
                pair,
                leg,
                env_seed: deal.env_seed,
                starting_player: starting.index() as u8,
                decks: deal
                    .decks
                    .map(|index| config.train_decks[index].deck.clone()),
                policies,
                learner_seats,
                terminal: "natural".into(),
                outcome: Some(outcome.into()),
                returns: Some(if p0_wins { [1, -1] } else { [-1, 1] }),
                truncation_reason: None,
                failure: None,
                turns: 1,
                decisions: 2,
                forced_decisions: 0,
                retained_decisions: learner_seats.map(u64::from),
                action_trace_sha256: "aa".repeat(32),
            });
        }
    }
    serde_json::to_value(games).unwrap()
}

fn fixture_receipt(config: &CampaignConfigV1, update: u64, parent: Option<String>) -> Value {
    let campaign = sha256_hex(&to_bytes(config).unwrap());
    if update == 0 {
        return json!({
            "schema": RECEIPT_SCHEMA_V1,
            "campaign_sha256": campaign,
            "update": 0,
            "kind": "initialization",
            "learner_contract": config.learner_contract,
        });
    }
    let games = fixture_games(config, update, false, 0);
    let totals = verify_games(config, &games, update, false).unwrap();
    let pairs = batch_pairs_v1(config, update - 1)
        .unwrap()
        .into_iter()
        .map(|(pair, league)| {
            let deal = deal_v1(config.schedule_seed, pair, config.train_decks.len()).unwrap();
            json!({
                "pair": pair,
                "kind": if league.is_some() { "league" } else { "self_play" },
                "opponent_step": league,
                "decks": deal.decks.map(|index| config.train_decks[index].deck.clone()),
                "env_seed": deal.env_seed,
                "starting_player": deal.starting.index(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": RECEIPT_SCHEMA_V1,
        "campaign_sha256": campaign,
        "update": update,
        "kind": "update",
        "policy_step": update - 1,
        "parent_checkpoint_sha256": parent,
        "pairs": pairs,
        "games": games,
        "natural_games": totals.natural,
        "truncated_games": 0,
        "learner_trajectories": totals.trajectories,
        "retained_decisions": totals.retained,
        "truncation_gate": {
            "limit": config.truncation_gate_games,
            "previous_truncated_games": if update == 1 { None } else { Some(0u64) },
            "exceeded": false,
        },
        "update_report": {
            "step": update,
            "learning_rate": OptimizerConfigV2::campaign_v1(config.u_max)
                .learning_rate(update).unwrap(),
            "loss": 0.0,
            "policy_loss": 0.0,
            "value_loss": 0.0,
            "entropy": 0.0,
            "advantage_sigma": 0.0,
            "gradient_norm": 0.0,
            "clipped": false,
            "decisions": totals.eligible,
            "natural_trajectories": totals.natural_trajectories,
            "truncated_trajectories": 0,
        },
    })
}

pub(crate) fn fixture_commit(
    root: &Path,
    config: &CampaignConfigV1,
    step: u64,
    parent: Option<String>,
    marker: f64,
    half_points: Option<u64>,
) -> String {
    let campaign_sha256 = sha256_hex(&to_bytes(config).unwrap());
    fs::write(root.join("campaign.json"), to_bytes(config).unwrap()).unwrap();
    let receipt = fixture_receipt(config, step, parent.clone());
    let mut snapshot = serde_json::to_value(
        initial_learner_v1(LearnerKindV1::ActorCriticH32, config.u_max)
            .unwrap()
            .snapshot(),
    )
    .unwrap();
    snapshot["step"] = json!(step);
    if step > 0 {
        snapshot["parameter_bits"][0] = json!(marker.to_bits());
    }
    let checkpoint = CheckpointV1::new(
        &campaign_sha256,
        &config.identity,
        parent.clone(),
        &sha256_hex(&to_bytes(&receipt).unwrap()),
        serde_json::from_value(snapshot).unwrap(),
    )
    .unwrap();
    let mut runner = RunnerV1 {
        root: root.to_path_buf(),
        config: config.clone(),
        campaign_sha256,
        decks: Vec::new(),
        learner: checkpoint.restore_learner_v1().unwrap(),
        head: step.saturating_sub(1),
        head_sha256: parent.unwrap_or_default(),
        snapshots: BTreeMap::new(),
        previous_truncated: None,
        dev_scores: BTreeMap::new(),
        workers: 1,
        diagnostics: None,
        stages: RunStagesV1::default(),
        fault: None,
        natural_games: 0,
        decisions: 0,
    };
    let (staging, receipt_sha256, sha256, _) = runner.stage_iteration(step, &receipt).unwrap();
    let dev = half_points.map(|points| {
        to_bytes(&json!({
            "schema": DEV_SCHEMA_V1,
            "campaign_sha256": runner.campaign_sha256,
            "update": step,
            "candidate_checkpoint_sha256": sha256,
            "opponent": "step_0000",
            "seed": config.dev_seed,
            "pairs": config.dev_pairs,
            "games": fixture_games(config, step, true, points),
            "half_points": points,
            "score": points as f64 / (4 * config.dev_pairs) as f64,
        }))
        .unwrap()
    });
    runner
        .finish_iteration(step, (staging, receipt_sha256, sha256.clone()), dev)
        .unwrap();
    // Drop immediately after the directory rename, before the root completion record.
    sha256
}

fn fixture_chain(root: &Path, config: &CampaignConfigV1, head: u64) {
    prepare_fresh_root(root).unwrap();
    let mut parent = None;
    for step in 0..=head {
        let points = (step > 0 && step.is_multiple_of(config.dev_interval)).then_some(48);
        parent = Some(fixture_commit(root, config, step, parent, 0.25, points));
    }
}

fn fixture_launch(root: &Path) -> LaunchV1 {
    LaunchV1 {
        root: root.to_path_buf(),
        repo_root: default_repo_root_v1(),
        workers: 1,
        stage_diagnostics: false,
        git_head: identity().git_head,
        binary_sha256: identity().binary_sha256,
        host_class: host(),
        qualification_manifest: None,
    }
}

#[test]
fn resume_stops_after_the_collapse_milestone_directory_was_committed() {
    let config = fixture_config(ModeV1::Campaign, 100);
    let root = temp_root("collapse");
    fixture_chain(&root, &config, 40);
    assert!(!root.join("complete.json").exists());
    assert_eq!(verify_v1(&root).unwrap(), 40);
    let checkpoint = fs::read(checkpoint_path_v1(&root, 40)).unwrap();
    let mut launch = fixture_launch(&root);
    // No inventory is needed to report a final stop without collecting again.
    launch.repo_root = root.join("no-inventory");
    assert_eq!(resume_v1(&launch).unwrap(), RunOutcomeV1::DevCollapse);
    assert_eq!(fs::read(checkpoint_path_v1(&root, 40)).unwrap(), checkpoint);
    assert!(!checkpoint_path_v1(&root, 41).exists());
    let complete = canonical_value(&fs::read(root.join("complete.json")).unwrap()).unwrap();
    assert_eq!(complete["final_update"], 40);
    assert_eq!(complete["stop_reason"], "dev_collapse");
    let bytes = fs::read(root.join("complete.json")).unwrap();
    assert_eq!(resume_v1(&launch).unwrap(), RunOutcomeV1::DevCollapse);
    assert_eq!(fs::read(root.join("complete.json")).unwrap(), bytes);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resume_refuses_a_failed_iteration_before_cleanup_or_collection() {
    let root = temp_root("failed-resume");
    let config = fixture_config(ModeV1::Smoke, 2);
    fixture_chain(&root, &config, 1);
    let staging = root.join("iterations").join(".iter_0002.staging");
    fs::create_dir(&staging).unwrap();
    fs::write(checkpoint_path_v1(&root, 2), b"keep").unwrap();
    let failed = json!({
        "schema": FAILED_SCHEMA_V1,
        "campaign_sha256": sha256_hex(&to_bytes(&config).unwrap()),
        "update": 2,
        "error": "worker panic",
        "games": [],
    });
    fs::write(
        root.join("failed-iteration-0002.json"),
        to_bytes(&failed).unwrap(),
    )
    .unwrap();
    let mut launch = fixture_launch(&root);
    launch.repo_root = root.join("no-inventory");
    let error = resume_v1(&launch).unwrap_err();
    assert!(error.contains("failed-iteration-0002.json"), "{error}");
    assert!(staging.exists());
    assert_eq!(fs::read(checkpoint_path_v1(&root, 2)).unwrap(), b"keep");
    assert!(!root.join("complete.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resume_finishes_an_interrupted_truncation_stop_without_collection() {
    let root = temp_root("stopped-resume");
    let config = fixture_config(ModeV1::Smoke, 2);
    fixture_chain(&root, &config, 1);
    let mut games = fixture_games(&config, 2, false, 0);
    for game in games.as_array_mut().unwrap() {
        game["terminal"] = json!("truncated");
        game["returns"] = Value::Null;
        game["outcome"] = Value::Null;
        game["truncation_reason"] = json!("max_decisions");
    }
    let stopped = json!({
        "schema": STOPPED_SCHEMA_V1,
        "campaign_sha256": sha256_hex(&to_bytes(&config).unwrap()),
        "update": 2,
        "stop_reason": "stopped_excess_truncation",
        "truncated_games": 2,
        "previous_truncated_games": 0,
        "games": games,
    });
    let path = root.join("stopped-iteration-0002.json");
    let mut launch = fixture_launch(&root);
    launch.repo_root = root.join("no-inventory");
    for field in ["schema", "campaign_sha256", "update", "truncated_games"] {
        let mut changed = stopped.clone();
        changed[field] = json!("wrong");
        fs::write(&path, to_bytes(&changed).unwrap()).unwrap();
        assert!(resume_v1(&launch).is_err(), "{field}");
        assert!(!root.join("complete.json").exists());
    }
    fs::write(&path, to_bytes(&stopped).unwrap()).unwrap();
    let checkpoint = fs::read(checkpoint_path_v1(&root, 1)).unwrap();
    assert_eq!(resume_v1(&launch).unwrap(), RunOutcomeV1::ExcessTruncation);
    let bytes = fs::read(root.join("complete.json")).unwrap();
    let complete = canonical_value(&bytes).unwrap();
    assert_eq!(complete["final_update"], 1);
    assert_eq!(complete["stop_reason"], "stopped_excess_truncation");
    assert_eq!(resume_v1(&launch).unwrap(), RunOutcomeV1::ExcessTruncation);
    assert_eq!(fs::read(root.join("complete.json")).unwrap(), bytes);
    assert_eq!(fs::read(checkpoint_path_v1(&root, 1)).unwrap(), checkpoint);
    assert!(!checkpoint_path_v1(&root, 2).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resume_refuses_a_non_milestone_head_after_dev_collapse() {
    let root = temp_root("past-collapse");
    let config = fixture_config(ModeV1::Campaign, 100);
    fixture_chain(&root, &config, 41);
    let error = verify_v1(&root).unwrap_err();
    assert!(error.contains("passed the dev collapse stop"), "{error}");
    let error = resume_v1(&fixture_launch(&root)).unwrap_err();
    assert!(error.contains("passed the dev collapse stop"), "{error}");
    assert!(!checkpoint_path_v1(&root, 42).exists());
    fs::remove_dir_all(root).unwrap();

    let root = temp_root("remember-collapse");
    let mut runner = start_runner(
        &fixture_launch(&root),
        ModeV1::Smoke,
        LearnerKindV1::ActorCriticH32,
        2,
    )
    .unwrap();
    runner.head = 41;
    runner.dev_scores.insert(20, 0.375);
    runner.dev_scores.insert(40, 0.375);
    assert_eq!(runner.committed_stop(), Some(RunOutcomeV1::DevCollapse));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resume_finishes_the_horizon_without_collection() {
    let root = temp_root("horizon-resume");
    fixture_chain(&root, &fixture_config(ModeV1::Smoke, 2), 2);
    let mut launch = fixture_launch(&root);
    launch.repo_root = root.join("no-inventory");
    assert_eq!(resume_v1(&launch).unwrap(), RunOutcomeV1::Horizon);
    let bytes = fs::read(root.join("complete.json")).unwrap();
    assert_eq!(resume_v1(&launch).unwrap(), RunOutcomeV1::Horizon);
    assert_eq!(fs::read(root.join("complete.json")).unwrap(), bytes);
    assert!(!checkpoint_path_v1(&root, 3).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn all_truncated_batches_stop_without_updating_any_learner() {
    for kind in [
        LearnerKindV1::ActorCriticH32,
        LearnerKindV1::ActorCriticH16,
        LearnerKindV1::PairCenteredControl,
    ] {
        let root = temp_root("no-eligible");
        let mut runner = start_runner(&fixture_launch(&root), ModeV1::Campaign, kind, 1).unwrap();
        runner.config.limits.max_decisions = 0;
        let before = runner.learner.snapshot();
        assert_eq!(
            runner.run_to_horizon().unwrap(),
            RunOutcomeV1::ExcessTruncation
        );
        assert_eq!(runner.learner.snapshot(), before);
        assert_eq!(runner.head, 0);
        assert!(!checkpoint_path_v1(&root, 1).exists());
        assert!(!root.join("failed-iteration-0001.json").exists());
        let stopped =
            canonical_value(&fs::read(root.join("stopped-iteration-0001.json")).unwrap()).unwrap();
        assert_eq!(stopped["truncated_games"], 32);
        assert_eq!(stopped["previous_truncated_games"], Value::Null);
        assert_eq!(stopped["games"].as_array().unwrap().len(), 32);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn cleanup_refuses_invalid_chains_before_removing_anything() {
    let root = temp_root("bad-chain");
    fixture_chain(&root, &fixture_config(ModeV1::Smoke, 2), 1);
    let staging = root.join("iterations").join(".iter_0002.staging");
    fs::create_dir(&staging).unwrap();
    fs::write(checkpoint_path_v1(&root, 2), b"keep").unwrap();
    fs::write(iteration_dir_v1(&root, 1).join("receipt.json"), b"{}").unwrap();
    assert!(clean_uncommitted(&root, 1).is_err());
    assert!(staging.exists());
    assert_eq!(fs::read(checkpoint_path_v1(&root, 2)).unwrap(), b"keep");
    for name in ["..", "../x", "a/b", "a\\b", "C:x", "C:\\x", "/x", ".x..y"] {
        assert!(child_path(&root, name).is_err(), "{name}");
    }
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn cleanup_refuses_redirected_directories_and_nested_links() {
    use std::os::unix::fs::symlink;

    for directory in ["checkpoints", "iterations"] {
        let root = temp_root("redirected");
        let outside = temp_root("outside");
        fixture_chain(&root, &fixture_config(ModeV1::Smoke, 2), 1);
        fs::rename(root.join(directory), &outside).unwrap();
        fs::write(outside.join("step_0002.json"), b"keep").unwrap();
        symlink(&outside, root.join(directory)).unwrap();
        assert!(clean_uncommitted(&root, 1).is_err());
        assert_eq!(fs::read(outside.join("step_0002.json")).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    let root = temp_root("nested-link");
    let outside = temp_root("nested-outside");
    fixture_chain(&root, &fixture_config(ModeV1::Smoke, 2), 1);
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), b"keep").unwrap();
    let staging = root.join("iterations").join(".iter_0002.staging");
    fs::create_dir(&staging).unwrap();
    symlink(&outside, staging.join("linked")).unwrap();
    assert!(clean_uncommitted(&root, 1).is_err());
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

fn rebind_receipt(root: &Path, step: u64, bytes: &[u8]) -> String {
    let directory = iteration_dir_v1(root, step);
    fs::write(directory.join("receipt.json"), bytes).unwrap();
    let (mut checkpoint, _) = CheckpointV1::load_v1(&checkpoint_path_v1(root, step)).unwrap();
    checkpoint.iteration_receipt_sha256 = sha256_hex(bytes);
    let bytes = checkpoint.to_bytes().unwrap();
    fs::write(checkpoint_path_v1(root, step), &bytes).unwrap();
    let digest = sha256_hex(&bytes);
    let complete =
        IterationCompleteV1::new(step, &digest, &checkpoint.iteration_receipt_sha256, None);
    fs::write(
        directory.join("complete.json"),
        complete.to_bytes().unwrap(),
    )
    .unwrap();
    digest
}

#[test]
fn configuration_reader_refuses_each_invalid_invariant() {
    let root = temp_root("invalid-config");
    fs::create_dir(&root).unwrap();
    let config = fixture_config(ModeV1::Smoke, 2);
    let path = root.join("campaign.json");
    fs::write(&path, to_bytes(&config).unwrap()).unwrap();
    read_config_v1(&root).unwrap();
    let refuse = |name: &str, change: &dyn Fn(&mut CampaignConfigV1)| {
        let mut changed = config.clone();
        change(&mut changed);
        fs::write(&path, to_bytes(&changed).unwrap()).unwrap();
        assert!(read_config_v1(&root).is_err(), "{name}");
    };
    refuse("contract", &|c| c.runner_contract.clear());
    refuse("card files", &|c| {
        c.card_file_sha256 = ["x".into(), "y".into()]
    });
    refuse("binary digest", &|c| {
        c.identity.binary_sha256 = "x".repeat(64)
    });
    refuse("source digest", &|c| {
        c.identity.binary_source_sha256.clear()
    });
    refuse("actor digest", &|c| {
        c.identity.actor_producer_sha256.clear()
    });
    refuse("reveal digest", &|c| {
        c.identity.reveal_producer_sha256.clear()
    });
    refuse("reducer digest", &|c| {
        c.identity.reveal_reducer_sha256.clear()
    });
    refuse("git head", &|c| c.identity.git_head.clear());
    refuse("hand knowledge", &|c| {
        c.identity.hand_knowledge = "wrong".into()
    });
    refuse("actor profile", &|c| {
        c.identity.actor_profile = "wrong".into()
    });
    refuse("pool pin", &|c| {
        c.identity.sample_sha256_in_pool_order = "bb".repeat(32)
    });
    refuse("card database pin", &|c| {
        c.identity.card_db_hash = "0000000000000000".into()
    });
    refuse("duplicate deck", &|c| {
        c.train_decks[1] = c.train_decks[0].clone()
    });
    refuse("eval deck", &|c| {
        c.train_decks[0].deck = "FDN_top_00265_UG".into()
    });
    refuse("pool order", &|c| c.train_decks.swap(0, 1));
    refuse("empty deck", &|c| c.train_decks[0].cards = 0);
    refuse("deck bound", &|c| {
        c.train_decks[0].cards = MAX_CUSTOM_DECK_CARDS_V1 + 1
    });
    refuse("registered count", &|c| c.train_decks[0].cards = 41);
    refuse("deck digest", &|c| {
        c.train_decks[0].source_sha256 = "x".repeat(64)
    });
    refuse("oversize digest", &|c| {
        c.train_decks
            .iter_mut()
            .find(|deck| deck.deck == GATE2_REQUIRED_DECK_V1)
            .unwrap()
            .source_sha256 = "bb".repeat(32);
    });
    refuse("smoke horizon", &|c| {
        c.u_max = 3;
        c.optimizer_config = optimizer_config_v1(LearnerKindV1::ActorCriticH32, 3);
    });
    refuse("smoke manifest", &|c| {
        c.qualification_manifest_sha256 = Some("aa".repeat(32))
    });
    let campaign = fixture_config(ModeV1::Campaign, 100);
    for (horizon, manifest) in [
        (50, Some("aa".repeat(32))),
        (100, None),
        (100, Some("x".repeat(64))),
        (1, Some("aa".repeat(32))),
    ] {
        let mut changed = campaign.clone();
        changed.u_max = horizon;
        changed.optimizer_config = optimizer_config_v1(LearnerKindV1::ActorCriticH32, horizon);
        changed.qualification_manifest_sha256 = manifest;
        fs::write(&path, to_bytes(&changed).unwrap()).unwrap();
        assert!(read_config_v1(&root).is_err(), "campaign horizon {horizon}");
    }
    let qualification = fixture_config(ModeV1::Qualification, 1);
    for horizon in [1, 2] {
        let mut changed = qualification.clone();
        changed.u_max = horizon;
        changed.optimizer_config = optimizer_config_v1(LearnerKindV1::ActorCriticH32, horizon);
        if horizon == 1 {
            changed.qualification_manifest_sha256 = Some("aa".repeat(32));
        }
        fs::write(&path, to_bytes(&changed).unwrap()).unwrap();
        assert!(
            read_config_v1(&root).is_err(),
            "qualification horizon {horizon}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn control_update_reports_require_valid_rewards_counts_and_loss() {
    let mut config = fixture_config(ModeV1::Smoke, 2);
    let kind = LearnerKindV1::PairCenteredControl;
    config.learner_contract = kind.contract().into();
    config.objective = kind.objective().into();
    config.feature_sha256 = kind.feature_sha256();
    config.optimizer_config = optimizer_config_v1(kind, 2);
    validate_config_v1(&config).unwrap();
    let totals = GameTotals {
        eligible: 3,
        ..GameTotals::default()
    };
    let report = json!({
        "loss": 0.0,
        "doubled_examples": 6,
        "reward_counts": {"-1": 2, "0": 2, "1": 2},
    });
    verify_update_report(&report, &config, 1, &totals).unwrap();
    for (field, value) in [
        ("loss", Value::Null),
        ("doubled_examples", json!(5)),
        ("reward_counts", json!({"2": 6})),
        ("reward_counts", json!({"0": -1})),
        ("reward_counts", json!({"0": 5})),
        ("reward_counts", json!({"-1": u64::MAX, "0": 1})),
    ] {
        let mut changed = report.clone();
        changed[field] = value;
        assert!(
            verify_update_report(&changed, &config, 1, &totals).is_err(),
            "{field}"
        );
    }
}

#[test]
fn verification_rejects_consecutive_truncation_receipts() {
    let root = temp_root("consecutive-truncation");
    let config = fixture_config(ModeV1::Campaign, 100);
    fixture_chain(&root, &config, 1);
    let path = iteration_dir_v1(&root, 1).join("receipt.json");
    let mut receipt = canonical_value(&fs::read(&path).unwrap()).unwrap();
    let truncate = |receipt: &mut Value| {
        for game in receipt["games"].as_array_mut().unwrap().iter_mut().take(3) {
            game["terminal"] = json!("truncated");
            game["returns"] = Value::Null;
            game["outcome"] = Value::Null;
            game["truncation_reason"] = json!("max_decisions");
        }
        receipt["natural_games"] = json!(29);
        receipt["truncated_games"] = json!(3);
        receipt["truncation_gate"]["exceeded"] = json!(true);
        receipt["update_report"]["decisions"] = json!(50);
        receipt["update_report"]["natural_trajectories"] = json!(50);
        receipt["update_report"]["truncated_trajectories"] = json!(6);
    };
    truncate(&mut receipt);
    let parent = rebind_receipt(&root, 1, &to_bytes(&receipt).unwrap());
    assert_eq!(verify_v1(&root).unwrap(), 1);
    fixture_commit(&root, &config, 2, Some(parent), 0.25, None);
    let path = iteration_dir_v1(&root, 2).join("receipt.json");
    let mut receipt = canonical_value(&fs::read(&path).unwrap()).unwrap();
    receipt["truncation_gate"]["previous_truncated_games"] = json!(3);
    rebind_receipt(&root, 2, &to_bytes(&receipt).unwrap());
    assert_eq!(verify_v1(&root).unwrap(), 2);
    truncate(&mut receipt);
    rebind_receipt(&root, 2, &to_bytes(&receipt).unwrap());
    let error = verify_v1(&root).unwrap_err();
    assert!(error.contains("passed the truncation stop"), "{error}");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn atomic_publication_refuses_dangling_staging_links() {
    use std::os::unix::fs::symlink;

    let root = temp_root("staging-link");
    fs::create_dir(&root).unwrap();
    let target = root.join("outside");
    let staging = root.join(".foo.json.staging");
    symlink(&target, &staging).unwrap();
    assert!(publish_atomic_v1(&root, "foo.json", b"{}").is_err());
    assert!(fs::symlink_metadata(&staging)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!target.exists());
    fs::remove_file(staging).unwrap();
    fs::write(root.join(".foo.json.staging"), b"interrupted").unwrap();
    publish_atomic_v1(&root, "foo.json", b"{}").unwrap();
    assert_eq!(fs::read(root.join("foo.json")).unwrap(), b"{}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn verification_requires_the_bound_campaign_and_canonical_receipts() {
    let root = temp_root("verified-records");
    let config = fixture_config(ModeV1::Smoke, 2);
    fixture_chain(&root, &config, 1);
    assert_eq!(verify_v1(&root).unwrap(), 1);
    let campaign = fs::read(root.join("campaign.json")).unwrap();
    fs::remove_file(root.join("campaign.json")).unwrap();
    assert!(verify_v1(&root).is_err());
    let mut foreign = config.clone();
    foreign.identity.git_head = "another-head".into();
    fs::write(root.join("campaign.json"), to_bytes(&foreign).unwrap()).unwrap();
    assert!(verify_v1(&root).is_err());
    fs::write(root.join("campaign.json"), &campaign).unwrap();
    let receipt = fs::read(iteration_dir_v1(&root, 1).join("receipt.json")).unwrap();
    let value = canonical_value(&receipt).unwrap();
    for field in ["schema", "campaign_sha256", "policy_step", "natural_games"] {
        let mut changed = value.clone();
        changed[field] = json!("wrong");
        rebind_receipt(&root, 1, &to_bytes(&changed).unwrap());
        assert!(verify_v1(&root).is_err(), "{field}");
    }
    let mut changed = value.clone();
    changed["games"][0]["pair"] = json!(999);
    rebind_receipt(&root, 1, &to_bytes(&changed).unwrap());
    assert!(verify_v1(&root).is_err());
    let mut spaced = receipt.clone();
    spaced.push(b'\n');
    rebind_receipt(&root, 1, &spaced);
    assert!(verify_v1(&root).is_err());
    rebind_receipt(&root, 1, &receipt);
    assert_eq!(verify_v1(&root).unwrap(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn verification_requires_milestone_dev_records_and_checks_their_scores() {
    let root = temp_root("dev-records");
    fixture_chain(&root, &fixture_config(ModeV1::Campaign, 100), 20);
    assert_eq!(verify_v1(&root).unwrap(), 20);
    let directory = iteration_dir_v1(&root, 20);
    let dev = fs::read(directory.join("dev.json")).unwrap();
    let complete_bytes = fs::read(directory.join("complete.json")).unwrap();
    let complete = IterationCompleteV1::parse_v1(&complete_bytes).unwrap();
    for field in [
        "schema",
        "campaign_sha256",
        "candidate_checkpoint_sha256",
        "score",
        "update",
    ] {
        let mut value = canonical_value(&dev).unwrap();
        value[field] = if field == "score" {
            json!(0.5)
        } else {
            json!("wrong")
        };
        let bytes = to_bytes(&value).unwrap();
        fs::write(directory.join("dev.json"), &bytes).unwrap();
        let mut changed = complete.clone();
        changed.dev_sha256 = Some(sha256_hex(&bytes));
        fs::write(directory.join("complete.json"), changed.to_bytes().unwrap()).unwrap();
        assert!(verify_v1(&root).is_err(), "{field}");
    }
    fs::remove_file(directory.join("dev.json")).unwrap();
    let mut changed = complete.clone();
    changed.dev_sha256 = None;
    fs::write(directory.join("complete.json"), changed.to_bytes().unwrap()).unwrap();
    assert!(verify_v1(&root).is_err());
    fs::write(directory.join("dev.json"), dev).unwrap();
    fs::write(directory.join("complete.json"), complete_bytes).unwrap();
    assert_eq!(verify_v1(&root).unwrap(), 20);
    fs::remove_dir_all(root).unwrap();
}

fn qualification_fixture(
    root: &Path,
    config: &CampaignConfigV1,
    workers: usize,
    diagnostics: bool,
) {
    fixture_chain(root, config, 1);
    let (_, step) = CheckpointV1::load_v1(&checkpoint_path_v1(root, 1)).unwrap();
    let evaluation = json!({
        "schema": EVALUATION_SCHEMA_V1,
        "campaign_sha256": sha256_hex(&to_bytes(config).unwrap()),
        "candidate_checkpoint_sha256": step,
        "opponent": "step_0000",
        "seed": config.dev_seed,
        "pairs": config.dev_pairs,
        "games": fixture_games(config, 1, true, 4),
        "half_points": 4,
        "score": 0.5,
    });
    let evaluation = to_bytes(&evaluation).unwrap();
    fs::write(root.join("evaluation.json"), &evaluation).unwrap();
    let training_ns = 1_000_000_000u64 / workers as u64;
    let timing = json!({
        "schema": "limited-campaign-qualification-timing/v1",
        "workers": workers,
        "stage_diagnostics": diagnostics,
        "wall_ns": 2 * training_ns,
        "training_ns": training_ns,
        "evaluation_ns": training_ns / 2,
        "training_natural_games": 16,
        "training_decisions": 32,
        "all_natural_games": 20,
        "all_decisions": 40,
        "completed_natural_games_per_sec": 16.0 / (training_ns as f64 / 1e9),
        "decisions_per_sec": 32.0 / (training_ns as f64 / 1e9),
        "peak_rss_bytes": null,
        "peak_rss_note": "process peak; the workers are threads of one process",
        "stages": RunStagesV1::default(),
    });
    fs::write(root.join("timing.json"), to_bytes(&timing).unwrap()).unwrap();
    let run = json!({
        "schema": QUALIFICATION_RUN_SCHEMA_V1,
        "identity": config.identity,
        "learner_contract": config.learner_contract,
        "feature_sha256": config.feature_sha256,
        "host_class": host(),
        "workers": workers,
        "stage_diagnostics": diagnostics,
        "qualification_pairs": config.qualification_pairs,
        "step_0001_sha256": step,
        "evaluation_sha256": sha256_hex(&evaluation),
        "timing": timing,
    });
    fs::write(root.join("qualification_run.json"), to_bytes(&run).unwrap()).unwrap();
}

#[test]
fn qualification_requires_all_sixteen_natural_training_games() {
    assert!(require_qualification_training(15).is_err());
    require_qualification_training(16).unwrap();
    let unfinished = temp_root("unfinished-qualification");
    let launch = fixture_launch(&unfinished);
    let mut runner = start_runner(
        &launch,
        ModeV1::Qualification,
        LearnerKindV1::ActorCriticH32,
        1,
    )
    .unwrap();
    runner.natural_games = 15;
    let error = finish_qualification_v1(&launch, runner, Instant::now(), 1).unwrap_err();
    assert!(error.contains("16 natural training games"), "{error}");
    assert!(!unfinished.join("qualification_run.json").exists());
    assert!(checkpoint_path_v1(&unfinished, 0).exists());
    fs::remove_dir_all(unfinished).unwrap();
    let root = temp_root("partial-qualification");
    let config = fixture_config(ModeV1::Qualification, 1);
    qualification_fixture(&root, &config, 1, false);
    let path = iteration_dir_v1(&root, 1).join("receipt.json");
    let mut receipt = canonical_value(&fs::read(&path).unwrap()).unwrap();
    receipt["games"][0]["terminal"] = json!("truncated");
    receipt["games"][0]["returns"] = Value::Null;
    receipt["games"][0]["outcome"] = Value::Null;
    receipt["games"][0]["truncation_reason"] = json!("max_decisions");
    receipt["natural_games"] = json!(15);
    receipt["truncated_games"] = json!(1);
    receipt["update_report"]["decisions"] = json!(30);
    receipt["update_report"]["natural_trajectories"] = json!(30);
    receipt["update_report"]["truncated_trajectories"] = json!(2);
    let step = rebind_receipt(&root, 1, &to_bytes(&receipt).unwrap());
    assert_eq!(verify_v1(&root).unwrap(), 1);
    let mut evaluation = canonical_value(&fs::read(root.join("evaluation.json")).unwrap()).unwrap();
    evaluation["candidate_checkpoint_sha256"] = json!(step);
    let evaluation = to_bytes(&evaluation).unwrap();
    fs::write(root.join("evaluation.json"), &evaluation).unwrap();
    let mut run = canonical_value(&fs::read(root.join("qualification_run.json")).unwrap()).unwrap();
    run["step_0001_sha256"] = json!(step);
    run["evaluation_sha256"] = json!(sha256_hex(&evaluation));
    fs::write(root.join("qualification_run.json"), to_bytes(&run).unwrap()).unwrap();
    let error = lock_manifest_v1(std::slice::from_ref(&root), 2, 100, 2).unwrap_err();
    assert!(error.contains("16 natural training games"), "{error}");
    assert!(path.exists());
    assert!(checkpoint_path_v1(&root, 1).exists());
    fs::remove_dir_all(root).unwrap();
}
