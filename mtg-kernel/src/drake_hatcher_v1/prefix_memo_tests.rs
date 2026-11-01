//! The checked-prefix memo of Drake custody validation (#375). On recorded
//! DraftZero sample games every validation the engine runs, and every
//! validation of altered copies of those states, gives exactly the verdict
//! of the validation before the memo, and the rows checked per step stop
//! growing with the game.
use super::custody::{self, JournalV1};
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::limited_match_v1::{
    deal_pair_v1, play_game_observed_v1, DeckFileV1, GameLimitsV1, GameOutcomeV1, PolicySpecV1,
    RulesAdmissionV1, SessionKindV1,
};
use crate::state::GameState;

const SAMPLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/limited/draftzero_sample_v1"
);
const RULES_ONLY_ALL: SessionKindV1 = SessionKindV1::RulesOnly(RulesAdmissionV1::AllExecutable);

/// The sample decks of `split` the session plays, in pool order, as
/// `kernel_limited_match_v1 play` lists them.
fn split_decks(split: &str) -> Vec<(String, Vec<u16>)> {
    let pool = std::fs::read_to_string(format!("{SAMPLE}/pool.tsv")).unwrap();
    let mut lines = pool.lines();
    assert!(lines.next().unwrap().starts_with("deck\tsplit"));
    lines
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let deck = columns.next().unwrap_or("");
            if columns.next().unwrap_or("") != split {
                return None;
            }
            let text = std::fs::read_to_string(format!("{SAMPLE}/decks/{deck}.json")).ok()?;
            let file: DeckFileV1 = serde_json::from_str(&text).ok()?;
            Some((deck.to_string(), file.resolve_for(RULES_ONLY_ALL).ok()?))
        })
        .collect()
}

/// A recorded deal: seed 1, the first leg, random against random, as
/// `kernel_limited_match_v1 play -seed 1 -single -a random -b random
/// -session rules-only-all -max-turns 100 -max-decisions 6000` plays it.
struct DealV1 {
    split: &'static str,
    pair: u64,
    decks: [&'static str; 2],
    winner: PlayerId,
    decisions: u64,
}

/// Train pair 29: the issue's trace, Drake Hatcher cast at decision 581.
const PAIR_29: DealV1 = DealV1 {
    split: "train",
    pair: 29,
    decks: ["FDN_top_02819_WU", "FDN_top_21341_WU"],
    winner: PlayerId::P0,
    decisions: 948,
};
/// The four deals that still passed 60 s under 8 workers after #429.
const SLOW: [DealV1; 4] = [
    DealV1 {
        split: "train",
        pair: 241,
        decks: ["FDN_top_15301_BG", "FDN_top_01766_UR"],
        winner: PlayerId::P1,
        decisions: 1344,
    },
    DealV1 {
        split: "train",
        pair: 436,
        decks: ["FDN_top_07760_UBpG", "FDN_top_02819_WU"],
        winner: PlayerId::P1,
        decisions: 1575,
    },
    DealV1 {
        split: "train",
        pair: 597,
        decks: ["FDN_top_04406_BR", "FDN_top_01766_UR"],
        winner: PlayerId::P1,
        decisions: 1657,
    },
    DealV1 {
        split: "eval",
        pair: 458,
        decks: ["FDN_top_04752_WG", "FDN_top_03718_UR"],
        winner: PlayerId::P0,
        decisions: 1322,
    },
];

/// Plays `deal`, showing `observe` the state before each step, and checks
/// it ends as recorded.
fn play(deal: &DealV1, observe: &mut dyn FnMut(&GameState)) {
    let decks = split_decks(deal.split);
    let (indices, env_seed, starting) = deal_pair_v1(1, deal.pair, decks.len()).unwrap();
    assert_eq!(indices.map(|i| decks[i].0.as_str()), deal.decks);
    let limits = GameLimitsV1 {
        max_turns: 100,
        max_decisions: 6000,
        full_validation: false,
        session: RULES_ONLY_ALL,
    };
    let random = PolicySpecV1::Random;
    let result = play_game_observed_v1(
        indices.map(|i| decks[i].1.as_slice()),
        [&random, &random],
        [None, None],
        env_seed,
        starting,
        limits,
        &mut |session| observe(session.game_state()),
    );
    assert_eq!(
        (result.outcome, result.decisions),
        (GameOutcomeV1::Winner(deal.winner), deal.decisions),
        "{} pair {}",
        deal.split,
        deal.pair
    );
}

fn rows(state: &GameState) -> u64 {
    state.engine.drake_ledger_v1.issued
}

/// Every validation the engine runs while playing `deal`, inside enclosing
/// replays and operations included, matches the reference.
fn engine_validations_match_reference(deal: &DealV1) {
    let mut last = 0;
    let ((), compared, differences) = custody::comparing_with_v0_for_test(|| {
        play(deal, &mut |state| last = rows(state));
    });
    assert!(last > 100, "{} journal rows", last);
    assert!(compared > 1000, "{compared} validations compared");
    assert_eq!(differences, Vec::<String>::new());
}

#[test]
fn every_engine_validation_of_recorded_drake_pair_29_matches_the_reference() {
    engine_validations_match_reference(&PAIR_29);
}

/// The four slow deals take minutes with the reference beside every
/// validation: `cargo test --release --features limited-fdn-fixtures --lib
/// prefix_memo_tests -- --ignored`.
#[test]
#[ignore]
fn every_engine_validation_of_the_four_slow_drake_deals_matches_the_reference() {
    for deal in &SLOW {
        engine_validations_match_reference(deal);
    }
}

fn kind_name(value: &JournalV1) -> &'static str {
    match value {
        JournalV1::CombatInput { .. } => "CombatInput",
        JournalV1::CombatFinished { .. } => "CombatFinished",
        JournalV1::Captured { .. } => "Captured",
        JournalV1::Collected { .. } => "Collected",
        JournalV1::Published { .. } => "Published",
        JournalV1::Debit { .. } => "Debit",
        JournalV1::Paid { .. } => "Paid",
        JournalV1::Removed { .. } => "Removed",
        JournalV1::Completed { .. } => "Completed",
        JournalV1::Enclosing { .. } => "Enclosing",
    }
}

/// Copies of `state` changed in ways that keep its checked prefix, and one
/// that drops it, each with the change's name.
fn altered(state: &GameState) -> Vec<(String, GameState)> {
    let history = &state.engine.event_history;
    let journal = history
        .iter()
        .filter_map(custody::journal)
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut add = |name: String, change: &dyn Fn(&mut GameState)| {
        let mut copy = state.clone();
        change(&mut copy);
        out.push((name, copy));
    };
    add("ledger".into(), &|s: &mut GameState| {
        s.engine.drake_ledger_v1.issued += 1
    });
    add("escaped reference".into(), &|s: &mut GameState| {
        s.engine
            .event_history
            .push(CommittedEvent::DrakeHistoryRefV1 { index: 0 })
    });
    add("journal in trigger log".into(), &|s: &mut GameState| {
        let last = s
            .engine
            .event_history
            .iter()
            .rev()
            .find(|e| custody::journal(e).is_some())
            .cloned()
            .unwrap();
        s.engine.event_log.push(last);
    });
    // The last row of each kind again, at its old index and at the end.
    let mut kinds = Vec::new();
    for row in journal.iter().rev() {
        let kind = std::mem::discriminant(&row.value);
        if kinds.contains(&kind) {
            continue;
        }
        kinds.push(kind);
        let name = kind_name(&row.value);
        for reindex in [false, true] {
            let row = (*row).clone();
            add(
                format!("{name} again, reindexed {reindex}"),
                &move |s: &mut GameState| {
                    let mut row = row.clone();
                    if reindex {
                        row.index = s.engine.event_history.len() as u64;
                    }
                    s.engine.event_history.push(CommittedEvent::DrakeJournalV1 {
                        record: Box::new(row),
                    });
                    s.engine.drake_ledger_v1.issued += 1;
                },
            );
        }
        if let JournalV1::Collected { pending, .. } = &row.value {
            let pending = (**pending).clone();
            add(
                "collected capture held again".into(),
                &move |s: &mut GameState| s.engine.pending_triggers.push(pending.clone()),
            );
        }
        if let JournalV1::Published { item, .. } | JournalV1::Paid { item, .. } = &row.value {
            let item = (**item).clone();
            add(
                format!("{name} item on the stack"),
                &move |s: &mut GameState| s.stack.push(item.clone()),
            );
        }
    }
    add("stack popped".into(), &|s: &mut GameState| {
        s.stack.pop();
    });
    add("stack top doubled".into(), &|s: &mut GameState| {
        if let Some(top) = s.stack.last().cloned() {
            s.stack.push(top);
        }
    });
    add("held triggers cleared".into(), &|s: &mut GameState| {
        s.engine.pending_triggers.clear()
    });
    if !crate::linked_aura_operation_v1::locally_owned(state) {
        add("inside an operation".into(), &|s: &mut GameState| {
            crate::linked_aura_operation_v1::tamper_drake_saved_output_runtime_for_test(s)
        });
    }
    // Changing a covered row drops the checked prefix.
    if let Some(at) = history.iter().position(|e| custody::journal(e).is_some()) {
        add("first row changed".into(), &move |s: &mut GameState| {
            if let CommittedEvent::DrakeJournalV1 { record } = &mut s.engine.event_history[at] {
                record.index += 1;
            }
        });
    }
    out
}

#[test]
fn altered_states_along_recorded_drake_pair_29_get_the_reference_verdicts() {
    let mut probes = 0;
    let mut refused = 0;
    let mut decision = 0_u64;
    let mut kept = 0;
    let ((), compared, differences) = custody::comparing_with_v0_for_test(|| {
        play(&PAIR_29, &mut |state| {
            decision += 1;
            if rows(state) == 0 || !decision.is_multiple_of(25) {
                return;
            }
            let base = state.clone();
            custody::validate(&base).unwrap();
            assert!(base.engine.event_history.prefix_memo_v1().is_some());
            for (name, copy) in altered(&base) {
                kept += usize::from(copy.engine.event_history.prefix_memo_v1().is_some());
                probes += 1;
                if custody::validate(&copy).is_err() {
                    refused += 1;
                }
                // The memo a copy leaves never changes the original's verdict.
                assert_eq!(custody::validate(&base), Ok(()), "after {name}");
            }
        });
    });
    assert_eq!(differences, Vec::<String>::new());
    assert!(
        probes > 200 && compared > probes as u64,
        "{probes} probes, {compared} compared"
    );
    assert!(refused > probes / 2, "{refused} of {probes} refused");
    assert!(kept > probes * 3 / 4, "{kept} of {probes} kept the prefix");
}

/// Rows given the checks that read their own prefix, and enclosing replays,
/// per journal row over a stretch of decisions.
#[derive(Default, Clone, Copy)]
struct WorkV1 {
    rows: u64,
    checks: u64,
    replays: u64,
}

#[test]
fn rows_checked_per_step_stay_bounded_after_drake_hatcher_is_cast() {
    // Work per new row in each quarter of the game after the first row.
    let work = |memo: bool| {
        let mut marks = Vec::<WorkV1>::new();
        let mut first = None;
        let mut decision = 0_u64;
        let mut run = || {
            custody::reset_replay_memo_for_test();
            let start = custody::local_row_checks_for_test();
            play(&PAIR_29, &mut |state| {
                decision += 1;
                let rows = rows(state);
                if rows == 0 {
                    return;
                }
                let first = *first.get_or_insert(decision);
                if (decision - first).is_multiple_of((PAIR_29.decisions - first) / 4) {
                    marks.push(WorkV1 {
                        rows,
                        checks: custody::local_row_checks_for_test() - start,
                        replays: custody::replay_memo_stats_for_test().executions,
                    });
                }
            });
        };
        if memo {
            run();
        } else {
            custody::without_prefix_memo_for_test(run);
        }
        marks
            .windows(2)
            .map(|pair| {
                let rows = (pair[1].rows - pair[0].rows).max(1) as f64;
                (
                    (pair[1].checks - pair[0].checks) as f64 / rows,
                    (pair[1].replays - pair[0].replays) as f64 / rows,
                )
            })
            .collect::<Vec<_>>()
    };
    let checked = work(true);
    let every = work(false);
    assert!(checked.len() >= 3, "{checked:?}");
    // With the memo each new row is checked a bounded number of times however
    // long the game, while checking every row grows with the game.
    for (checks, replays) in &checked {
        assert!(*checks < 40.0 && *replays < 4.0, "{checked:?}");
    }
    let (early, late) = (checked[0].0, checked[checked.len() - 1].0);
    assert!(late < early * 2.0 + 4.0, "{checked:?}");
    let (every_early, every_late) = (every[0].0, every[every.len() - 1].0);
    assert!(every_late > every_early * 2.0, "{every:?}");
    assert!(every_late > late * 10.0, "{every:?} against {checked:?}");
}

#[test]
fn a_copy_adopts_the_longer_prefix_another_copy_accepted() {
    // Two public states of one game, the later with more journal rows.
    let (mut earlier, mut later) = (None, None);
    play(&PAIR_29, &mut |state| {
        if earlier.is_none() && rows(state) > 50 {
            earlier = Some(state.clone());
        } else if later.is_none() && rows(state) > 60 {
            later = Some(state.clone());
        }
    });
    let (earlier, later) = (earlier.unwrap(), later.unwrap());
    let covered = |s: &GameState| s.engine.event_history.prefix_memo_v1().map(|(c, _)| c);
    custody::validate(&earlier).unwrap();
    let start = earlier.engine.event_history.len();
    assert_eq!(covered(&earlier), Some(start));
    // Copies of the later state whose histories carry the earlier state's
    // checked prefix, then `added` (the later state's added events).
    let added = later.engine.event_history[start..].to_vec();
    let copy = |added: &[CommittedEvent]| {
        let mut copy = later.clone();
        copy.engine.event_history = earlier.engine.event_history.clone();
        copy.engine.event_history.extend(added.iter().cloned());
        assert_eq!(covered(&copy), Some(start));
        copy
    };
    let (a, b) = (copy(&added), copy(&added));
    assert!(a == later && b == later);
    let checks = custody::local_row_checks_for_test;
    let before = checks();
    assert_eq!(custody::validate(&a), Ok(()));
    let checked = checks() - before;
    assert!(checked >= rows(&later) - rows(&earlier));
    // `b` reuses what validating `a` established about the same events.
    assert_eq!(custody::validate(&b), Ok(()));
    assert_eq!(checks() - before, checked);
    assert_eq!(covered(&b), Some(later.engine.event_history.len()));
    // A copy whose added events differ (here by a reference that escaped
    // into them) must not adopt that prefix, which would skip the escape.
    let mut different = added.clone();
    let swap = different
        .iter()
        .position(|e| custody::journal(e).is_none())
        .unwrap();
    different[swap] = CommittedEvent::DrakeHistoryRefV1 { index: 0 };
    let other = copy(&different);
    let verdict = custody::validate(&other);
    assert!(verdict.is_err());
    assert_eq!(covered(&other), Some(start));
    let ((), _, differences) = custody::comparing_with_v0_for_test(|| {
        assert_eq!(custody::validate(&copy(&different)), verdict);
    });
    assert_eq!(differences, Vec::<String>::new());
}
