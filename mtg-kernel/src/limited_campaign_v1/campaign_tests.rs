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
    assert!(update_learner_v1(&mut learner, &only_truncated).is_err());
    assert_eq!(learner.snapshot(), before);
    let mixed = records_with(&[natural.clone(), truncated.clone()]);
    let natural_decisions = mixed[0].retained.len() as u64;
    let report = update_learner_v1(&mut learner, &mixed).unwrap();
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
    let report = update_learner_v1(&mut learner, &records).unwrap();
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
    for directory in ["checkpoints", "iterations"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    for update in 0..=1 {
        let directory = iteration_dir_v1(&root, update);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("complete.json"), b"{}").unwrap();
        fs::write(checkpoint_path_v1(&root, update), b"{}").unwrap();
    }
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
    let manifest = QualificationManifestV1 {
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
    // Non-canonical bytes are refused.
    let mut bytes = manifest_bytes_v1(&manifest).unwrap();
    bytes.push(b'\n');
    fs::write(&path, bytes).unwrap();
    assert!(guard_launch_v1(Some(&path), 200, 4, learner, &identity, &host(), 32).is_err());
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
