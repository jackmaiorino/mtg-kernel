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
/// replays and operations included, matches the reference. Many of them use
/// a proof kept with the history, and each of those also runs the full
/// check and asserts the same verdict.
fn engine_validations_match_reference(deal: &DealV1) {
    use crate::event_history_v1::proof_tests_v1;
    let mut last = 0;
    let uses = proof_tests_v1::uses();
    let ((), compared, differences) = custody::comparing_with_v0_for_test(|| {
        play(deal, &mut |state| last = rows(state));
    });
    let proved = proof_tests_v1::uses() - uses;
    assert!(last > 100, "{} journal rows", last);
    assert!(compared > 1000, "{compared} validations compared");
    assert!(proved > 1000, "{proved} validations used a proof");
    assert_eq!(differences, Vec::<String>::new());
}

#[test]
fn every_engine_validation_of_recorded_drake_pair_29_matches_the_reference() {
    engine_validations_match_reference(&PAIR_29);
}

#[test]
fn owned_enclosing_replays_match_borrowed_states_and_serialized_receipts() {
    let mut final_state = None;
    let mut decision = 0;
    play(&PAIR_29, &mut |state| {
        // The observer is called once at reset and after every decision.
        // Copy only the final state; the helper bypasses the replay cache.
        if decision == PAIR_29.decisions {
            final_state = Some(state.clone());
        }
        decision += 1;
    });
    let compared = custody::compare_owned_enclosing_replays_for_test(&final_state.unwrap());
    assert!(compared > 10, "{compared} enclosing records compared");
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

/// Bounded performance diagnostics for #474. Each invocation plays the exact
/// deal twice with cold thread-local replay caches, once per staging path.
/// It compares every byte of the final state, including the entire event
/// history, and reports decision windows without observer or printing time.
/// Timing is evidence to inspect, not a platform-dependent unit-test gate.
fn compare_slow_deal_staging(deal: &DealV1, borrowed_first: bool) {
    use sha2::{Digest, Sha256};
    use std::time::{Duration, Instant};

    let mut outputs = Vec::new();
    let mut reports = Vec::new();
    for borrowed in [borrowed_first, !borrowed_first] {
        custody::reset_replay_memo_for_test();
        custody::clear_replay_prefix_for_test();
        let mut run = || {
            let mut previous = None::<Instant>;
            let mut elapsed = Duration::ZERO;
            let mut decision = 0;
            let mut window_start = 0;
            let mut windows = Vec::new();
            let mut final_bytes = None;
            play(deal, &mut |state| {
                if let Some(started) = previous {
                    elapsed += started.elapsed();
                }
                if decision > 0 && (decision % 100 == 0 || decision == deal.decisions) {
                    windows.push(serde_json::json!({
                        "first_decision": window_start + 1,
                        "last_decision": decision,
                        "elapsed_ns": u64::try_from(elapsed.as_nanos()).unwrap(),
                        "history_len": state.engine.event_history.len(),
                        "drake_rows": rows(state),
                    }));
                    window_start = decision;
                    elapsed = Duration::ZERO;
                }
                if decision == deal.decisions {
                    final_bytes = Some(serde_json::to_vec(state).unwrap());
                }
                decision += 1;
                previous = Some(Instant::now());
            });
            assert_eq!(decision, deal.decisions + 1);
            let bytes = final_bytes.unwrap();
            let replays = custody::replay_memo_stats_for_test();
            assert!(replays.executions > 0, "the staging path must actually run");
            reports.push(serde_json::json!({
                "schema": "drake-staging-timing/v1",
                "split": deal.split,
                "pair": deal.pair,
                "seed": 1,
                "leg": 0,
                "decks": deal.decks,
                "winner": format!("{:?}", deal.winner),
                "staging": if borrowed { "borrowed" } else { "owned" },
                "decisions": deal.decisions,
                "final_state_sha256": format!("{:x}", Sha256::digest(&bytes)),
                "replay_executions": replays.executions,
                "windows": windows,
            }));
            outputs.push(bytes);
        };
        if borrowed {
            crate::linked_aura_operation_v1::with_borrowed_drake_replays_for_test(run);
        } else {
            run();
        }
    }
    assert!(
        outputs[0] == outputs[1],
        "complete final state and receipts differ ({} versus {} bytes)",
        outputs[0].len(),
        outputs[1].len()
    );
    for report in reports {
        println!("drake_staging_timing_v1 {report}");
    }
}

#[test]
#[ignore = "paired recorded-game timing and full receipt comparison"]
fn slow_drake_staging_train_241() {
    compare_slow_deal_staging(&SLOW[0], true);
}

#[test]
#[ignore = "paired recorded-game timing and full receipt comparison"]
fn slow_drake_staging_train_436() {
    compare_slow_deal_staging(&SLOW[1], false);
}

#[test]
#[ignore = "paired recorded-game timing and full receipt comparison"]
fn slow_drake_staging_train_597() {
    compare_slow_deal_staging(&SLOW[2], true);
}

#[test]
#[ignore = "paired recorded-game timing and full receipt comparison"]
fn slow_drake_staging_eval_458() {
    compare_slow_deal_staging(&SLOW[3], false);
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
                        record: std::sync::Arc::new(row),
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
                std::sync::Arc::make_mut(record).index += 1;
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
        probes > 100 && compared > probes as u64,
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
        let mut final_bytes = None;
        let mut run = || {
            custody::reset_replay_memo_for_test();
            let start = custody::local_row_checks_for_test();
            play(&PAIR_29, &mut |state| {
                decision += 1;
                if decision == PAIR_29.decisions + 1 {
                    final_bytes = Some(serde_json::to_vec(state).unwrap());
                }
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
        let windows = marks
            .windows(2)
            .map(|pair| {
                let rows = (pair[1].rows - pair[0].rows).max(1) as f64;
                (
                    (pair[1].checks - pair[0].checks) as f64 / rows,
                    (pair[1].replays - pair[0].replays) as f64 / rows,
                )
            })
            .collect::<Vec<_>>();
        (windows, final_bytes.unwrap())
    };
    let (checked, with_memo) = work(true);
    let (every, full_scan) = work(false);
    assert!(
        with_memo == full_scan,
        "collection changed native final state"
    );
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
fn collection_tail_matches_full_scan_on_append_divergence_and_tamper() {
    use super::tests::{cast, damage_three, finish};
    use std::sync::Arc;

    let actor = PlayerId::P0;
    let (mut base, drake) = cast(actor);
    damage_three(&mut base, drake, actor);
    finish(&mut base);
    custody::validate(&base).unwrap();
    let history = &base.engine.event_history;
    let covered = history.len();
    assert_eq!(history.prefix_memo_v1().unwrap().0, covered);
    let (collected, capture_index) = history
        .iter()
        .filter_map(custody::journal)
        .find_map(|row| match &row.value {
            JournalV1::Collected { capture, .. } => Some((row, *capture)),
            _ => None,
        })
        .expect("native combat trigger was collected");
    let capture = history
        .get(capture_index as usize)
        .and_then(custody::journal)
        .expect("collected capture record")
        .clone();
    let JournalV1::Captured { marker, .. } = &capture.value else {
        panic!("collected record must point to a capture");
    };
    let events = [history.get(*marker as usize).unwrap().clone()];
    let compare = |state: &GameState, expected: Result<&[u64], &str>| {
        let result = custody::captures_for_collection(state, &events);
        let reference = custody::without_prefix_memo_for_test(|| {
            custody::captures_for_collection(state, &events)
        });
        assert_eq!(result, reference);
        assert_eq!(result.as_deref().map_err(String::as_str), expected);
    };
    let append = |record| {
        let mut branch = base.clone();
        branch
            .engine
            .event_history
            .push(CommittedEvent::DrakeJournalV1 {
                record: Arc::new(record),
            });
        branch.engine.drake_ledger_v1.issued += 1;
        assert_eq!(
            branch.engine.event_history.prefix_memo_v1().unwrap().0,
            covered
        );
        branch
    };

    compare(&base, Ok(&[]));
    let mut new_capture = capture.clone();
    new_capture.index = covered as u64;
    let mut branch = append(new_capture.clone());
    compare(&branch, Ok(&[new_capture.index]));
    let mut collection = collected.clone();
    collection.index = (covered + 1) as u64;
    let JournalV1::Collected { capture: index, .. } = &mut collection.value else {
        unreachable!()
    };
    *index = new_capture.index;
    branch
        .engine
        .event_history
        .push(CommittedEvent::DrakeJournalV1 {
            record: Arc::new(collection),
        });
    branch.engine.drake_ledger_v1.issued += 1;
    compare(&branch, Ok(&[]));

    let JournalV1::Captured { marker, .. } = &mut new_capture.value else {
        unreachable!()
    };
    *marker = u64::MAX;
    compare(&append(new_capture), Err("Drake marker erased"));
    // A duplicate using its old index must still see the old collection.
    compare(&append(capture.clone()), Ok(&[]));

    let collected = collected.index as usize;
    let mut tampered = base;
    *tampered.engine.event_history.get_mut(collected).unwrap() = events[0].clone();
    assert!(tampered.engine.event_history.prefix_memo_v1().is_none());
    compare(&tampered, Ok(&[capture.index]));
}

#[test]
fn checked_relations_skip_enclosing_rows_and_reject_backward_tamper() {
    use super::tests::{cast, damage_three, finish};
    use std::sync::Arc;

    let actor = PlayerId::P0;
    let (mut base, drake) = cast(actor);
    damage_three(&mut base, drake, actor);
    finish(&mut base);
    custody::validate(&base).unwrap();
    let history = &base.engine.event_history;
    let covered = history.len();
    assert_eq!(history.prefix_memo_v1().unwrap().0, covered);
    let records = || history.iter().filter_map(custody::journal);
    let total = records().count();
    let non_enclosing = records()
        .filter(|row| !matches!(row.value, JournalV1::Enclosing { .. }))
        .count();
    assert!(non_enclosing > 0 && non_enclosing < total);

    let visits = custody::relation_row_visits_for_test;
    let before = visits();
    assert_eq!(custody::validate(&base), Ok(()));
    assert_eq!(visits() - before, non_enclosing);
    let before = visits();
    assert_eq!(
        custody::without_prefix_memo_for_test(|| custody::validate(&base)),
        Ok(())
    );
    assert!(visits() - before >= total);

    let mut editable = base.clone();
    let _ = editable.engine.event_history.get_mut(0).unwrap();
    custody::validate(&editable).unwrap();
    assert!(editable.engine.event_history.prefix_memo_v1().is_some());
    let before = visits();
    assert_eq!(custody::validate(&editable), Ok(()));
    assert_eq!(visits() - before, total);

    let compare = |state: &GameState, error| {
        assert_eq!(
            state.engine.event_history.prefix_memo_v1().unwrap().0,
            covered
        );
        let sparse = custody::validate(state);
        let full = custody::without_prefix_memo_for_test(|| custody::validate(state));
        assert_eq!(sparse, full);
        assert_eq!(sparse.as_ref().map_err(String::as_str), Err(error));
    };
    let append = |mut row: custody::RecordV1| {
        let mut branch = base.clone();
        row.index = covered as u64;
        branch
            .engine
            .event_history
            .push(CommittedEvent::DrakeJournalV1 {
                record: Arc::new(row),
            });
        branch.engine.drake_ledger_v1.issued += 1;
        branch
    };

    let (publication, item) = records()
        .find_map(|row| match &row.value {
            JournalV1::Published { item, .. } => Some((row, item.as_ref())),
            _ => None,
        })
        .expect("native trigger publication");
    let enclosure = records()
        .find(|row| {
            matches!(row.value, JournalV1::Enclosing { start, .. }
                if start <= publication.index && publication.index < row.index)
        })
        .expect("enclosing operation for publication");
    compare(
        &append(enclosure.clone()),
        "Drake record lost its unique original enclosing operation",
    );
    compare(
        &append(publication.clone()),
        "Drake APNAP occurrence disappeared/replayed",
    );

    let mut live_again = base.clone();
    live_again.stack.push(item.clone());
    compare(
        &live_again,
        "Drake issued stack row lost/duplicated/replayed",
    );

    let pending = records()
        .find_map(|row| match &row.value {
            JournalV1::Collected { pending, .. } => Some(pending.as_ref()),
            _ => None,
        })
        .expect("native trigger collection");
    let mut held_again = base.clone();
    held_again.engine.pending_triggers.push(pending.clone());
    compare(&held_again, "Drake APNAP occurrence disappeared/replayed");
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

#[test]
fn prefixes_expire_with_their_histories_when_added_worlds_hold_older_prefixes() {
    let mut found = None;
    play(&PAIR_29, &mut |state| {
        if found.is_none() && rows(state) > 50 {
            found = Some(state.clone());
        }
    });
    let earlier = found.unwrap();
    custody::validate(&earlier).unwrap();
    let memo = |s: &GameState| {
        let (_, memo) = s.engine.event_history.prefix_memo_v1().unwrap();
        std::sync::Arc::downgrade(&memo)
    };
    let shorter = memo(&earlier);
    // An added event whose saved world holds the shorter prefix, as a Panda
    // input capturing a linked runner's whole original world can.
    let row = earlier
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|e| {
            custody::journal(e).filter(|r| matches!(r.value, JournalV1::Enclosing { .. }))
        })
        .unwrap();
    let mut record = row.clone();
    if let JournalV1::Enclosing { original, .. } = &mut record.value {
        *original = std::sync::Arc::new(earlier.clone());
    }
    let mut longer = earlier.clone();
    longer
        .engine
        .event_history
        .push(CommittedEvent::DrakeJournalV1 {
            record: std::sync::Arc::new(record),
        });
    custody::extend_prefix_for_test(&longer.engine.event_history);
    let extended = memo(&longer);
    assert!(!extended.ptr_eq(&shorter));
    drop(earlier);
    drop(longer);
    assert!(
        shorter.upgrade().is_none(),
        "the shorter prefix outlived its histories"
    );
    assert!(
        extended.upgrade().is_none(),
        "the longer prefix outlived its histories"
    );
}
