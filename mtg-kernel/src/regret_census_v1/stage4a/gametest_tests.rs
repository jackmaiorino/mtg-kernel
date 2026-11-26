//! The game test's adapter (collab LANES/speed-lead/gametest-20261125,
//! EXECUTOR-SUPPORT.md revision 2): strict switches, the FlatV7 identity
//! rule, the search-count ceiling, the trainer's London session, the world
//! sampler after London mulligans, rollout limits under action windows,
//! and seat streams across the plain arm, the search arm and the replays.
//! Game-level checks are native qualification (run with --release).

use super::bank::{opening_session, source_session, LondonOpening};
use super::canon2::KeyMode;
use super::refresh::tests::plain_control_fixture;
use super::refresh::{
    budget_case_with_executor, budget_parity_replay, budget_plain_case_opening,
    budget_replay_opening, rule, search_ceiling, Allocation, BudgetExtras, ExecutionMode, Lookup,
    Step, PREGAME_CHOOSER,
};
use super::reuse_witness::Recording;
use super::*;
use crate::ids::PlayerId;
use crate::priority_action_windows_v1::SeatPriorityWindowsV1::{ActionWindowsV1, HarnessV2};
use crate::rl::ActionSemanticV1;
use crate::rl_session::pauper_london_v1::PauperMulliganModeV1::{KeepSeven, Learned};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::Zone;
use std::env::VarError::NotPresent;

const LONDON: LondonOpening = LondonOpening {
    modes: [Learned; 2],
    windows: [ActionWindowsV1; 2],
};

fn v7() -> FrozenPlayPolicyV1 {
    FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap()
}

/// The plain-control fixture with a FlatV7 policy on both seats.
fn london_fixture(
    seat: u8,
    limits: (u64, u64),
) -> (FrozenPlayPolicyV1, bank::BankShared, bank::SourceSpec) {
    let (_, mut sh, mut src) = plain_control_fixture(seat, 0, limits.0, limits.1);
    let policy = v7();
    sh.opponents = vec![policy.fork_for_collection_v3().unwrap()];
    src.id = format!("gametest-s{seat}");
    src.search_root = "gametest-fixture".into();
    (policy, sh, src)
}

fn ok(v: &str) -> Result<String, std::env::VarError> {
    Ok(v.into())
}

#[test]
fn game_test_switches_are_strict_and_budget_only() {
    assert_eq!(
        parse_london(Err(NotPresent), Err(NotPresent), Err(NotPresent)),
        Ok(None)
    );
    assert_eq!(
        parse_london(
            ok("london-v1"),
            ok("learned,learned"),
            ok("action-windows-v1,action-windows-v1")
        ),
        Ok(Some(LONDON))
    );
    assert_eq!(
        parse_london(ok("london-v1"), ok("learned,keep_seven"), Err(NotPresent)),
        Ok(Some(LondonOpening {
            modes: [Learned, KeepSeven],
            windows: [HarnessV2; 2]
        }))
    );
    for (o, m, w) in [
        (ok("london-v2"), ok("learned,learned"), Err(NotPresent)),
        (ok("london-v1"), Err(NotPresent), Err(NotPresent)),
        (ok("london-v1"), ok("learned"), Err(NotPresent)),
        (
            ok("london-v1"),
            ok("learned,learned,learned"),
            Err(NotPresent),
        ),
        (ok("london-v1"), ok("learned, learned"), Err(NotPresent)),
        (
            ok("london-v1"),
            ok("learned,learned"),
            ok("action-windows-v1"),
        ),
        (
            ok("london-v1"),
            ok("learned,learned"),
            ok("action-windows-v2,harness-v2"),
        ),
        (Err(NotPresent), ok("learned,learned"), Err(NotPresent)),
        (
            Err(NotPresent),
            Err(NotPresent),
            ok("action-windows-v1,action-windows-v1"),
        ),
    ] {
        assert!(
            parse_london(o.clone(), m.clone(), w.clone()).is_err(),
            "{o:?} {m:?} {w:?}"
        );
    }
    assert_eq!(parse_max_searches(Err(NotPresent)), Ok(None));
    assert_eq!(parse_max_searches(ok("128")), Ok(Some(128)));
    for bad in ["0", "-1", "+128", "0128", "128 ", "x"] {
        assert!(parse_max_searches(ok(bad)).is_err(), "{bad}");
    }
    for name in GAME_TEST_ENVS {
        let set = |n: &str| n == name;
        assert!(validate_executor_mode("s4a-budget", set, Err(NotPresent)).is_ok());
        for mode in [
            "s4a-budget-replay",
            "s4a-root-rebuild",
            "s4a-run",
            "decisions",
        ] {
            assert_eq!(
                validate_executor_mode(mode, set, Err(NotPresent)),
                Err(format!("{name} is supported only for s4a-budget")),
                "{mode}"
            );
            assert!(validate_executor_mode(mode, |_| false, Err(NotPresent)).is_ok());
        }
    }
    let replays =
        parse_replay_actions("{\"id\":\"a\",\"actions\":[0,2]}\n\n{\"id\":\"b\",\"actions\":[]}\n")
            .unwrap();
    assert_eq!(replays["a"], [0, 2]);
    assert!(
        parse_replay_actions("{\"id\":\"a\",\"actions\":[]}\n{\"id\":\"a\",\"actions\":[1]}")
            .is_err()
    );
    assert!(parse_replay_actions("{\"id\":\"a\",\"actions\":[],\"seed\":1}").is_err());
}

#[test]
fn budget_arms_refuse_incompatible_settings() {
    let arm = |name, total| parse_budget_arm(Ok(name), total, false, None);
    assert_eq!(
        arm("replay-v1", 1),
        Err("BUDGET_ARM=replay-v1 requires BUDGET_TOTAL=0, mean backup and no interchange".into())
    );
    let path = |name, max, replay_path| {
        let (plain, replay) = arm(name, 0).unwrap();
        budget_replay_path(plain, replay, max, replay_path)
    };
    assert_eq!(
        path("plain-v1", Some(128), Err(NotPresent)),
        Err("S4A_MAX_SEARCHES requires BUDGET_ARM=search-v1".into())
    );
    let replay_requirement =
        "BUDGET_REPLAY_ACTIONS is required by, and only by, BUDGET_ARM=replay-v1";
    for arm in ["plain-v1", "search-v1"] {
        assert_eq!(
            path(arm, None, ok("actions.jsonl")),
            Err(replay_requirement.into())
        );
    }
    assert_eq!(
        path("replay-v1", None, Err(NotPresent)),
        Err(replay_requirement.into())
    );
    assert_eq!(
        path("replay-v1", None, ok("actions.jsonl")),
        Ok(Some("actions.jsonl".into()))
    );
    assert_eq!(path("search-v1", Some(128), Err(NotPresent)), Ok(None));
}

#[test]
fn legacy_budget_replay_refuses_london_records() {
    for row in [
        json!({"opening":"london-v1"}),
        json!({"game":{"opening_record":{"seats":[]}}}),
        json!({"opening_record":{"seats":[]}}),
    ] {
        assert_eq!(
            validate_budget_replay_row(&row, None),
            Err("s4a-budget-replay refuses London opening records".into())
        );
    }
    assert!(validate_budget_replay_row(&json!({"game":{"actions":[0]}}), None).is_ok());
    assert!(validate_budget_replay_row(&json!({"opening_record":null}), None).is_ok());
}

/// With London set, a policy without the FlatV7 (or FlatV8) identity is
/// refused on either seat; unset, nothing is checked.
#[test]
fn london_identity_refuses_a_non_flat_v7_model() {
    let (flat_v7, flat_v4, flat_v3) = (
        v7(),
        FrozenPlayPolicyV1::training_fixture_v4(),
        FrozenPlayPolicyV1::training_fixture_v3(),
    );
    let flat_v8 = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v8_v1()
        .unwrap();
    for good in [&flat_v7, &flat_v8] {
        assert!(
            require_london_identity(Some(LONDON), good, std::slice::from_ref(&flat_v7)).is_ok()
        );
    }
    for bad in [&flat_v4, &flat_v3] {
        let fork = || bad.fork_for_collection_v3().unwrap();
        let refusal = require_london_identity(Some(LONDON), bad, &[v7()]).unwrap_err();
        assert!(refusal.contains("FlatV7"), "{refusal}");
        assert!(require_london_identity(Some(LONDON), &flat_v7, &[v7(), fork()]).is_err());
        assert!(require_london_identity(None, bad, &[fork()]).is_ok());
    }
}

/// The count ceiling with B1's settings (S4A_ALLOCATION=T,T, BUDGET_TOTAL =
/// 128 T, S4A_MAX_SEARCHES=128): searches that spend less than T would
/// leave room for a partial 129th search; with the ceiling a game has
/// exactly 128 full searches. A ceiling too small for the allocation never
/// runs a partial search either.
#[test]
fn search_ceiling_allows_128_full_searches_and_no_partial_one() {
    const T: u64 = 16_000;
    let play = |allocation: Allocation, max: Option<u64>, spend: fn(u64) -> u64| {
        let (mut used, mut budgets) = (0, Vec::new());
        for _ in 0..400 {
            let (step, full) = if budgets.is_empty() {
                (
                    Step::Refresh(allocation.anchor.min(allocation.total)),
                    allocation.anchor,
                )
            } else {
                (
                    rule(Lookup::Miss, allocation, used, false),
                    allocation.refresh,
                )
            };
            if let (Step::Refresh(b), _) = search_ceiling(step, max, budgets.len(), full) {
                budgets.push(b);
                used += spend(b);
            }
        }
        budgets
    };
    let b1 = Allocation {
        anchor: T,
        refresh: T,
        total: 128 * T,
    };
    // A flat-root search may stop short of its cap (a sampler stop).
    let short: fn(u64) -> u64 = |b| if b == T { b - 7 } else { b };
    let uncapped = play(b1, None, short);
    assert_eq!(
        uncapped.len(),
        129,
        "the ceiling alone allows a partial search"
    );
    assert!(*uncapped.last().unwrap() < T);
    let capped = play(b1, Some(128), short);
    assert_eq!(capped.len(), 128);
    assert!(capped.iter().all(|&b| b == T));
    assert_eq!(play(b1, Some(128), |b| b).len(), 128);
    let spare = Allocation {
        total: 130 * T,
        ..b1
    };
    assert_eq!(play(spare, None, |b| b), [T; 130]);
    assert_eq!(play(spare, Some(128), |b| b), [T; 128]);
    let tight = Allocation {
        total: 5 * T / 2,
        ..b1
    };
    assert_eq!(play(tight, Some(128), |b| b), [T, T]);
    assert_eq!(
        search_ceiling(Step::Execute(3), Some(0), 9, T),
        (Step::Execute(3), false)
    );
    assert_eq!(
        search_ceiling(Step::Refresh(1), None, 9, T),
        (Step::Refresh(1), false)
    );
}

#[test]
fn search_ceiling_marks_a_decision_at_the_signed_budget_limit() {
    const T: u64 = 2;
    const MAX: u64 = 128;
    let allocation = Allocation {
        anchor: T,
        refresh: T,
        total: MAX * T,
    };
    let before = rule(Lookup::Miss, allocation, (MAX - 1) * T, false);
    assert_eq!(
        search_ceiling(before, Some(MAX), (MAX - 1) as usize, T),
        (Step::Refresh(T), false)
    );
    let exhausted = rule(Lookup::Miss, allocation, MAX * T, false);
    assert_eq!(exhausted, Step::Plain);
    assert_eq!(
        search_ceiling(exhausted, Some(MAX), MAX as usize, T),
        (Step::Plain, true)
    );
    assert_eq!(
        search_ceiling(exhausted, None, MAX as usize, T),
        (Step::Plain, false)
    );
}

/// The adapter's London game is the collector's (`episode_session_v1`) for
/// the same deal, seats, limits, windows and rules, before and along play.
#[test]
fn london_opening_is_the_trainers_episode_session() {
    use crate::expanded_deck_training_v1::*;
    let registered = |id: &str| {
        let deck = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
        let c = deck.registered_configuration();
        ExpandedDeckListV1 {
            label: id.into(),
            mainboard: c.mainboard().to_vec(),
            sideboard: c.sideboard().to_vec(),
        }
    };
    let (_, mut sh, _) = london_fixture(0, (900, 9_000));
    sh.spec.decks = ["Burn", "Elves"]
        .map(|id| bank::DeckSpec {
            label: id.into(),
            mainboard: registered(id).mainboard,
        })
        .to_vec();
    for (seat, starting) in [(0u8, 0u8), (1, 0), (0, 1), (1, 1)] {
        let src = bank::SourceSpec {
            own: 0,
            other: 1,
            seat,
            starting_player: starting,
            seed: 0x6a3e + u64::from(seat),
            ..london_fixture(seat, (1, 1)).2
        };
        let lists = if seat == 0 {
            [registered("Burn"), registered("Elves")]
        } else {
            [registered("Elves"), registered("Burn")]
        };
        let episode: ExpandedEpisodeV1 = serde_json::from_value(json!({
            "id": "gametest", "seed": src.seed, "starting_player": starting, "learner_seat": seat,
            "registered": lists, "selected": lists, "postboard": false,
            "max_physical_decisions": 900, "max_policy_steps": 9_000,
            "runtime_rules": "resolution-boundary-v1", "opening": "london-v1",
            "mulligan_modes": ["learned", "learned"],
            "priority_windows": ["action-windows-v1", "action-windows-v1"],
        }))
        .unwrap();
        for modes in [[Learned, KeepSeven], [KeepSeven, Learned], [KeepSeven; 2]] {
            let asymmetric = ExpandedEpisodeV1 {
                mulligan_modes: Some(modes),
                ..episode.clone()
            };
            assert_eq!(
                opening_session(&sh.spec, &src, Some(LondonOpening { modes, ..LONDON })).err(),
                asymmetric.validate_opening_v1().err(),
                "learner seat {seat}, modes {modes:?}"
            );
        }
        let mut trainer = episode_session_for_test_v1(&episode).unwrap();
        let mut adapter = opening_session(&sh.spec, &src, Some(LONDON)).unwrap();
        assert!(adapter.pauper_london_pending_v1());
        assert_eq!(adapter.priority_windows_for_test_v1(), [ActionWindowsV1; 2]);
        for i in 0..400u32 {
            assert_eq!(
                adapter.current_response(),
                trainer.current_response(),
                "{i}"
            );
            assert_eq!(
                adapter.privileged_core_environment_hash(),
                trainer.privileged_core_environment_hash()
            );
            let Some(d) = decision(&adapter) else { break };
            let a = i.wrapping_mul(2_654_435_761) % d.legal_action_count;
            adapter.step(d.episode_id, d.step, a).unwrap();
            trainer.step(d.episode_id, d.step, a).unwrap();
        }
        assert!(adapter.pauper_opening_record_v1().is_some());
        assert_eq!(
            adapter.pauper_opening_record_v1(),
            trainer.pauper_opening_record_v1()
        );
        // Unset, the keep-seven game is unchanged.
        assert_eq!(
            opening_session(&sh.spec, &src, None)
                .unwrap()
                .privileged_core_environment_hash(),
            source_session(&sh.spec, &src)
                .unwrap()
                .privileged_core_environment_hash()
        );
    }
}

/// A London game where seat 0 takes `mulligans[0]` and seat 1
/// `mulligans[1]` London mulligans; `swap` makes a two-card bottom choose
/// its two cards in the other order. Each mulligan here is followed by its
/// bottoms and then the next announcement, so the kept deal's bottoms are
/// the last ones. Returns the game and each seat's kept-deal bottomed card
/// definitions, first bottomed first.
fn mulligan_game(mulligans: [u8; 2], swap: bool) -> (FastActorSessionV1, [Vec<u16>; 2]) {
    let decks = [&RUNTIME_DECKS[5], &RUNTIME_DECKS[1]];
    let mut s = FastActorSessionV1::reset_pauper_london_windows_v1(
        1,
        0x10_4d0e,
        100_000,
        200_000,
        decks.map(|d| d.id.to_owned()),
        decks.map(|d| d.card_ids.to_vec()),
        PlayerId(0),
        [Learned; 2],
        [ActionWindowsV1; 2],
    )
    .unwrap();
    s.set_runtime_rules_v1(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
        .unwrap();
    let (mut taken, mut bottomed) = ([0u8; 2], [Vec::new(), Vec::new()]);
    let mut plan: [Vec<u16>; 2] = [Vec::new(), Vec::new()];
    let mut trace = Vec::new();
    while s.pauper_london_pending_v1() {
        let d = decision(&s).unwrap();
        let seat = acting(&d).index();
        let sem = s.diagnostic_current_action_semantics().unwrap();
        trace.push(format!(
            "seat {seat} step {} sub {} k {} hand {} first {:?}",
            d.step,
            d.substep_index,
            d.legal_action_count,
            s.game_state().players[seat].hand.len(),
            sem[0]
        ));
        let a = match &sem[0] {
            ActionSemanticV1::ChooseLondonMulligan { .. } => {
                let mulligan = taken[seat] < mulligans[seat];
                taken[seat] += u8::from(mulligan);
                if mulligan {
                    // A mulligan reshuffles the hand, earlier bottoms
                    // included; this deal's bottoms follow it.
                    bottomed[seat].clear();
                    plan[seat].clear();
                }
                sem.iter()
                    .position(|x| matches!(x, ActionSemanticV1::ChooseLondonMulligan { mulligan: m, .. } if *m == mulligan))
                    .unwrap_or_else(|| panic!("seat {seat} mulligan {mulligan}: {sem:?}"))
            }
            ActionSemanticV1::ChooseLondonBottom { .. } => {
                let card = |i: usize| match &sem[i] {
                    ActionSemanticV1::ChooseLondonBottom { card, .. } => card.card_db_id,
                    other => panic!("{other:?}"),
                };
                // Two cards of different definitions: the first offered and
                // the first offered unlike it, in `swap`'s order.
                if plan[seat].is_empty() {
                    let unlike = (1..sem.len())
                        .find(|&i| card(i) != card(0))
                        .unwrap_or_else(|| panic!("seat {seat}: one definition only: {sem:?}"));
                    let two = matches!(
                        sem[0],
                        ActionSemanticV1::ChooseLondonBottom { remaining: 2, .. }
                    );
                    plan[seat] = if swap && two {
                        vec![card(unlike), card(0)]
                    } else {
                        vec![card(0), card(unlike)]
                    };
                }
                let want = plan[seat][bottomed[seat].len()];
                let i = (0..sem.len())
                    .find(|&i| card(i) == want)
                    .unwrap_or_else(|| {
                        panic!(
                            "seat {seat}: {want} not offered: {sem:?}
{}",
                            trace.join(
                                "
"
                            )
                        )
                    });
                bottomed[seat].push(want);
                i
            }
            other => panic!("unexpected pregame decision {other:?}"),
        };
        s.step(d.episode_id, d.step, a as u32).unwrap();
    }
    (s, bottomed)
}

/// Whether the world sampler keeps the actor's own London bottoms at the
/// bottom of its library (EXECUTOR-SUPPORT.md section 3, declaration
/// section 13 check 7), as observed here and pinned.
const OWN_BOTTOMS_KEPT: bool = true;

/// Plays first legal actions to `actor`'s first in-game decision with two or
/// more legal actions.
fn to_root(s: &mut FastActorSessionV1, actor: PlayerId) {
    loop {
        let d = decision(s).expect("game ended before the root");
        if acting(&d) == actor && d.legal_action_count >= 2 {
            return;
        }
        s.step(d.episode_id, d.step, 0).unwrap();
    }
}

fn defs(s: &FastActorSessionV1, owner: PlayerId, zone: Zone) -> Vec<u16> {
    let p = &s.game_state().players[owner.index()];
    let ids = match zone {
        Zone::Library => &p.library,
        Zone::Hand => &p.hand,
        _ => unreachable!(),
    };
    ids.iter()
        .map(|&id| s.game_state().objects.get(id).card_def)
        .collect()
}

fn owned(s: &FastActorSessionV1, owner: PlayerId) -> Vec<u16> {
    let mut all: Vec<u16> = s
        .game_state()
        .objects
        .iter()
        .filter(|(_, o)| o.owner == owner && !o.v4.is_token && o.spell_copy_origin.is_none())
        .map(|(_, o)| o.card_def)
        .collect();
    all.sort_unstable();
    all
}

/// The hidden-world sampler after London mulligans (EXECUTOR-SUPPORT.md
/// 5.4 item 1). Seat 0 takes one mulligan (one bottom), seat 1 two (two
/// bottoms). At each seat's first in-game root, 64 fresh samples are
/// accepted or rejected with a declared reason; every accepted sample keeps
/// each library and hand size and each owner's card multiset (the prior is
/// the true opponent deck alone, so a relabel keeps the multiset); the
/// actor's own bottomed cards stay at the bottom of its library (behaviour
/// recorded: the owner's knowledge of its bottoms is kept); and the
/// opponent's bottom order is never read (two true states that differ only
/// in it give the same samples).
#[test]
fn world_sampler_keeps_london_bottoms_and_never_reads_the_opponents() {
    let (base, bottomed) = mulligan_game([1, 2], false);
    let (swapped, swapped_bottoms) = mulligan_game([1, 2], true);
    assert_eq!((bottomed[0].len(), bottomed[1].len()), (1, 2));
    assert_eq!(swapped_bottoms[1], [bottomed[1][1], bottomed[1][0]]);
    let record = base.pauper_opening_record_v1().unwrap();
    assert_eq!(
        record
            .seats
            .each_ref()
            .map(|r| (r.mulligans_taken, r.kept_hand_size)),
        [(1, 6), (2, 5)]
    );
    for actor in [PlayerId(0), PlayerId(1)] {
        let opp = actor.opponent();
        let (mut s, mut t) = (base.clone(), swapped.clone());
        to_root(&mut s, actor);
        to_root(&mut t, actor);
        let k = bottomed[actor.index()].len();
        let own_library = defs(&s, actor, Zone::Library);
        // Each bottom goes below the last: the library ends first, second.
        assert_eq!(
            &own_library[own_library.len() - k..],
            bottomed[actor.index()],
            "{own_library:?}"
        );
        // Seat 1 bottoms two cards; the swapped game differs only in their
        // order. When seat 1 is the opponent, both true states give the
        // actor the same information, so their samples must be equal.
        let compare = opp == PlayerId(1);
        assert!(defs(&s, PlayerId(1), Zone::Library) != defs(&t, PlayerId(1), Zone::Library));
        assert_eq!(
            defs(&s, PlayerId(0), Zone::Library),
            defs(&t, PlayerId(0), Zone::Library)
        );
        for p in [actor, opp] {
            assert_eq!(defs(&s, p, Zone::Hand), defs(&t, p, Zone::Hand));
        }
        let prior = world::DeckPrior::new(&[if opp == PlayerId(0) { 5 } else { 1 }]);
        let (mut accepted, mut kept) = (0, 0);
        for seed in 0..64u64 {
            let a = world::sample(&s, seed, &prior);
            let b = compare.then(|| world::sample(&t, seed, &prior));
            match (a, b) {
                (Ok(a), b) => {
                    accepted += 1;
                    let w = &a.world;
                    let b = b.map(|b| {
                        b.unwrap_or_else(|e| panic!("seed {seed}: only the swap rejects: {e}"))
                    });
                    for p in [PlayerId(0), PlayerId(1)] {
                        for zone in [Zone::Library, Zone::Hand] {
                            assert_eq!(defs(w, p, zone).len(), defs(&s, p, zone).len());
                            if let Some(b) = &b {
                                assert_eq!(
                                    defs(w, p, zone),
                                    defs(&b.world, p, zone),
                                    "{seed} {p:?}"
                                );
                            }
                        }
                        assert_eq!(owned(w, p), owned(&s, p), "{seed} {p:?}");
                    }
                    let library = defs(w, actor, Zone::Library);
                    kept += usize::from(
                        library[library.len() - k..] == own_library[own_library.len() - k..],
                    );
                    assert_eq!(w.priority_windows_for_test_v1(), [ActionWindowsV1; 2]);
                }
                (Err(a), b) => {
                    assert!(!a.is_empty());
                    if let Some(b) = b {
                        assert_eq!(Some(a), b.err(), "seed {seed}");
                    }
                }
            }
        }
        assert!(accepted >= 32, "{actor:?}: {accepted} of 64 accepted");
        eprintln!("{actor:?}: own bottoms kept in {kept} of {accepted} accepted samples");
        let expected = if OWN_BOTTOMS_KEPT { accepted } else { 0 };
        assert_eq!(kept, expected, "{actor:?}");
    }
}

/// Plays a whole fixture game with `policy` on both seats (seeded) and
/// returns the actions up to the focal in-game root two thirds of the way
/// through the focal roots: a late in-game state.
fn late_prefix(
    sh: &bank::BankShared,
    src: &bank::SourceSpec,
    policy: &FrozenPlayPolicyV1,
) -> Vec<u32> {
    let mut s = opening_session(&sh.spec, src, Some(LONDON)).unwrap();
    let mut p = policy.fork_for_collection_v3().unwrap();
    p.reset_sampling_v1(src.guide_policy_seeds);
    let (mut actions, mut roots) = (Vec::new(), Vec::new());
    while let Some(d) = decision(&s) {
        if !s.pauper_london_pending_v1()
            && acting(&d) == PlayerId(src.seat)
            && d.legal_action_count >= 2
        {
            roots.push(actions.len());
        }
        let a = act(&mut p, &s, &d, &mut Counters::default()).unwrap();
        actions.push(a);
        s.step(d.episode_id, d.step, a).unwrap();
    }
    assert!(roots.len() >= 3, "{} focal roots", roots.len());
    actions.truncate(roots[roots.len() * 2 / 3]);
    actions
}

/// Rollout limits under action-windows-v1 (EXECUTOR-SUPPORT.md 5.4 item 2):
/// from a late in-game state of a windows game every rollout ends naturally
/// at the fixture's large limits; the same state with a deliberately small
/// remaining step budget makes the search report non-natural simulations.
#[test]
#[ignore = "native qualification: a fixture game prefix and two small flat-root searches; run with cargo test --release"]
fn rollout_limits_under_action_windows_are_live() {
    let (policy, sh, src) = london_fixture(0, (100_000, 200_000));
    let actions = late_prefix(&sh, &src, &policy);
    let search = |sh: &bank::BankShared| {
        let s = bank::walk(
            opening_session(&sh.spec, &src, Some(LONDON)).unwrap(),
            &actions,
            |_, _, _| Ok(()),
        )
        .unwrap();
        assert!(!s.pauper_london_pending_v1());
        let mut roles = bank::bank_roles(&policy, &sh.opponents).unwrap();
        let seeds = seeds::RootSeeds {
            model: "fixture".into(),
            root: "rollout-limits".into(),
        };
        let ctx = bank::guide_ctx(
            &s,
            PlayerId(0),
            0,
            &seeds,
            &sh.prior,
            Limits {
                select_cap: 20_000,
                ..sh.limits
            },
        )
        .unwrap();
        let options = root_allocation::SearchOptions {
            flat_root: true,
            ..Default::default()
        };
        roles
            .select_e_options(&ctx, None, 1, None, None, None, &options)
            .0
    };
    let roomy = search(&sh);
    assert!(
        !roomy.incomplete && roomy.faults.is_empty(),
        "{:?}",
        roomy.faults
    );
    assert!(roomy.completed > 0);
    assert_eq!(
        roomy.nonnatural, 0,
        "rollouts end naturally at large limits"
    );
    let mut tight = sh;
    tight.spec.max_policy_steps = actions.len() as u64 + 40;
    let small = search(&tight);
    assert!(
        !small.incomplete && small.faults.is_empty(),
        "{:?}",
        small.faults
    );
    assert!(
        small.nonnatural > 0,
        "a small step budget shows non-natural rollouts"
    );
}

/// Drops wall and CPU fields.
fn scientific(mut row: Value) -> Value {
    fn strip(row: &mut Value) {
        match row {
            Value::Object(object) => {
                for key in ["wall", "cpu", "keying_seconds"] {
                    object.remove(key);
                }
                object.values_mut().for_each(strip);
            }
            Value::Array(array) => array.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut row);
    row
}

fn on_big_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, f)
            .unwrap()
            .join()
            .unwrap()
    })
}

#[test]
fn parity_replay_errors_keep_the_recorded_decisions() {
    on_big_stack(|| {
        let (policy, sh, src) = london_fixture(0, (12, 40));
        let mut session = opening_session(&sh.spec, &src, Some(LONDON)).unwrap();
        let mut actions = Vec::new();
        while let Some(d) = decision(&session) {
            actions.push(0);
            session.step(d.episode_id, d.step, 0).unwrap();
        }
        let replay = |actions: &[u32]| {
            budget_parity_replay(&sh, &policy, &src, actions, "fixture", Some(LONDON)).unwrap()
        };
        let complete = replay(&actions);
        let decisions = complete["decisions"].as_array().unwrap();
        assert!(complete.get("error").is_none());
        assert!(complete["terminal"].is_object());
        assert_eq!(decisions.len(), actions.len());
        assert!(decisions.len() > 2);

        let short = replay(&actions[..actions.len() - 1]);
        assert_eq!(
            short["error"],
            "the replayed actions end before the terminal"
        );
        assert_eq!(short["decisions"], json!(decisions[..decisions.len() - 1]));
        assert!(short.get("terminal").is_none());

        let mut overlong_actions = actions.clone();
        overlong_actions.push(0);
        let overlong = replay(&overlong_actions);
        assert_eq!(
            overlong["error"],
            format!("action {} follows the terminal", actions.len())
        );
        assert_eq!(overlong["decisions"], complete["decisions"]);
        assert_eq!(overlong["terminal"], complete["terminal"]);
        assert_eq!(overlong["end"], complete["end"]);

        let invalid_index = 1;
        let mut invalid_actions = actions;
        invalid_actions[invalid_index] = u32::MAX;
        let invalid = replay(&invalid_actions);
        let message = invalid["error"].as_str().unwrap();
        assert!(
            message.starts_with(&format!("action {invalid_index} ({}) at step ", u32::MAX)),
            "{message}"
        );
        let mut expected = decisions[..=invalid_index].to_vec();
        expected[invalid_index]["action"] = json!(u32::MAX);
        assert_eq!(invalid["decisions"], json!(expected));
        assert!(invalid.get("terminal").is_none());
    });
}

#[test]
fn parity_replay_matches_both_seats_direct_scores_and_terminal() {
    on_big_stack(|| {
        // The focal policy plays physical seat 1. Change both final-layer
        // biases on the opponent so either score detects a seat mix-up.
        let (mut policy, mut sh, src) = london_fixture(1, (12, 40));
        let mut parameters = sh.opponents[0].training_parameters_v3();
        for name in ["scorer.2.bias", "value_head.2.bias"] {
            let parameter = parameters.iter_mut().find(|p| p.name == name).unwrap();
            parameter.values[0] += 2.0;
        }
        sh.opponents[0]
            .replace_training_parameters_v3(&parameters)
            .unwrap();
        let mut opponent = sh.opponents[0].fork_for_collection_v3().unwrap();
        let mut s = opening_session(&sh.spec, &src, Some(LONDON)).unwrap();
        let mut scored = [false; 2];
        let mut actions = Vec::new();
        let mut expected = Vec::new();
        while let Some(d) = decision(&s) {
            let seat = usize::from(acting(&d).0);
            let a = d.legal_action_count - 1;
            let (obs, menu) = decision_replay_key(&s, d).unwrap();
            let mut record = json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep":d.substep_index,"acting":seat,"k":d.legal_action_count,
                "action":a,"pregame":s.pauper_london_pending_v1(),"obs_hash":obs,"menu_hash":menu});
            if d.legal_action_count >= 2 {
                let focal_scores = policy.score_fast_session_v1(&s).unwrap();
                let opponent_scores = opponent.score_fast_session_v1(&s).unwrap();
                assert_ne!(focal_scores.logits, opponent_scores.logits);
                assert_ne!(
                    focal_scores.value.to_bits(),
                    opponent_scores.value.to_bits()
                );
                let scores = if seat == 1 {
                    focal_scores
                } else {
                    opponent_scores
                };
                record["logit_bits"] = json!(scores
                    .logits
                    .iter()
                    .map(|x| x.to_bits())
                    .collect::<Vec<_>>());
                record["value_bits"] = json!(scores.value.to_bits());
                scored[seat] = true;
            }
            expected.push(record);
            actions.push(a);
            s.step(d.episode_id, d.step, a).unwrap();
        }
        assert_eq!(scored, [true, true]);
        let FastActorResponseV1::Terminal(terminal) = s.current_response() else {
            panic!("fixture did not reach terminal");
        };
        let row =
            budget_parity_replay(&sh, &policy, &src, &actions, "fixture", Some(LONDON)).unwrap();
        assert!(row.get("error").is_none(), "{}", row["error"]);
        assert_eq!(row["decisions"], json!(expected));
        assert_eq!(
            row["terminal"],
            json!({
                "winner":terminal.winner,
                "classification":terminal.terminal_classification
            })
        );
        assert_eq!(row["end"], json!(bank::outcome(&s, PlayerId(src.seat))));
    });
}

const T: u64 = 2_000;

fn search_extras(max: u64) -> BudgetExtras {
    BudgetExtras {
        per_decision: Some((T, T)),
        flat_root: true,
        london: Some(LONDON),
        max_searches: Some(max),
        ..Default::default()
    }
}

#[test]
#[ignore = "native qualification: a fixture game with 128 two-transition searches; run with cargo test --release"]
fn signed_budget_game_records_the_search_ceiling_after_spending_all_transitions() {
    const T: u64 = 2;
    const MAX: u64 = 128;
    let (policy, sh, mut src) = london_fixture(0, (1_000, 2_000));
    // An Elves mirror supplies enough action choices to exhaust both limits.
    let elves = sh
        .spec
        .decks
        .iter()
        .position(|d| d.label == "Elves")
        .unwrap();
    src.own = elves;
    src.other = elves;
    let extras = BudgetExtras {
        per_decision: Some((T, T)),
        ..search_extras(MAX)
    };
    let row = on_big_stack(|| {
        budget_case_with_executor(
            &sh,
            &policy,
            &src,
            MAX * T,
            KeyMode::Repaired,
            false,
            None,
            Recording::Disabled,
            "fixture",
            ExecutionMode::Persistent,
            extras,
        )
        .unwrap()
    });
    let game = &row["game"];
    let searches = game["searches"].as_array().unwrap();
    assert_eq!(searches.len() as u64, MAX, "{}", game["invalid_reason"]);
    assert!(searches.iter().all(|s| s["transitions"] == T));
    assert_eq!(game["transitions_used"], MAX * T);
    let exhausted = game["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["remaining_before"] == 0 && d["lookup"] != "supported")
        .expect("a search-eligible focal decision after both budgets run out");
    assert_eq!(exhausted["search_ceiling"], true, "{exhausted}");
}

/// Replays `row`'s actions with fresh live policies seeded from the guide
/// seeds that draw only where the row's chooser drew: every opponent
/// decision and every focal decision played plain. Each reproduced action
/// shows that seat's stream advanced only at its own plain draws.
fn assert_streams(
    sh: &bank::BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &bank::SourceSpec,
    row: &Value,
) {
    let focal = PlayerId(src.seat);
    let chooser: std::collections::HashMap<u64, String> = row["game"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["step"].as_u64().unwrap(),
                d["chooser"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let mut f = policy.fork_for_collection_v3().unwrap();
    let mut o = sh.opponents[0].fork_for_collection_v3().unwrap();
    f.reset_sampling_v1(src.guide_policy_seeds);
    o.reset_sampling_v1(src.guide_policy_seeds);
    let actions: Vec<u32> = serde_json::from_value(row["game"]["actions"].clone()).unwrap();
    let mut s = opening_session(&sh.spec, src, Some(LONDON)).unwrap();
    let tree_executed = row["game"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| matches!(d["chooser"].as_str(), Some("new_root" | "reused")))
        .count();
    let (mut drawn, mut skipped) = (0, 0);
    for &a in &actions {
        let d = decision(&s).unwrap();
        if d.legal_action_count >= 2 {
            if acting(&d) != focal {
                assert_eq!(
                    o.select_fast_session_v1(&s).unwrap(),
                    a,
                    "opponent at {}",
                    d.step
                );
            } else if matches!(
                chooser[&d.step].as_str(),
                "plain" | "raw_instead_of_reuse" | PREGAME_CHOOSER
            ) {
                assert_eq!(
                    f.select_fast_session_v1(&s).unwrap(),
                    a,
                    "focal at {}",
                    d.step
                );
                drawn += 1;
            } else {
                skipped += 1;
            }
        }
        s.step(d.episode_id, d.step, a).unwrap();
    }
    assert!(decision(&s).is_none());
    assert!(drawn > 0);
    assert_eq!(skipped, tree_executed);
    if row["arm"] != "plain-v1" {
        assert!(
            tree_executed > 0,
            "the search arm must execute a tree action"
        );
    }
}

/// London pregame decisions are plain draws, never searched: every pregame
/// focal choice is recorded with the pregame chooser before the first
/// search, and every search root is an in-game decision.
#[test]
#[ignore = "native qualification: one fixture game with two small flat-root searches; run with cargo test --release"]
fn london_pregame_is_played_plain_and_never_searched() {
    for seat in 0..2u8 {
        let (policy, sh, src) = london_fixture(seat, (100_000, 200_000));
        let row = on_big_stack(|| {
            budget_case_with_executor(
                &sh,
                &policy,
                &src,
                2 * T,
                KeyMode::Repaired,
                false,
                None,
                Recording::Disabled,
                "fixture",
                ExecutionMode::Persistent,
                search_extras(2),
            )
            .unwrap()
        });
        assert_eq!(
            row["game"]["valid"], true,
            "{}",
            row["game"]["invalid_reason"]
        );
        let searches = row["game"]["searches"].as_array().unwrap();
        assert_eq!(searches.len(), 2);
        let first = searches[0]["step"].as_u64().unwrap();
        let decisions = row["game"]["decisions"].as_array().unwrap();
        assert_eq!(row["game"]["transitions_used"], 2 * T);
        assert!(decisions
            .iter()
            .any(|d| { d["remaining_before"] == 0 && d["search_ceiling"] == true }));
        let pregame: Vec<_> = decisions
            .iter()
            .filter(|d| d["chooser"] == PREGAME_CHOOSER)
            .collect();
        assert!(!pregame.is_empty());
        assert!(pregame
            .iter()
            .all(|d| d["step"].as_u64().unwrap() < first && d["search"].is_null()));
        let actions: Vec<u32> = serde_json::from_value(row["game"]["actions"].clone()).unwrap();
        let mut roots = Vec::new();
        bank::walk(
            opening_session(&sh.spec, &src, Some(LONDON)).unwrap(),
            &actions,
            |s, d, _| {
                if searches.iter().any(|x| x["step"] == d.step) {
                    roots.push(s.pauper_london_pending_v1());
                }
                if s.pauper_london_pending_v1()
                    && acting(d) == PlayerId(seat)
                    && d.legal_action_count >= 2
                {
                    assert!(pregame.iter().any(|x| x["step"] == d.step));
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(roots, [false, false]);
        let record = &row["game"]["opening_record"];
        assert!(
            record["seats"][usize::from(seat)]["exposed_pregame_decisions"]
                .as_u64()
                .unwrap()
                >= 1
        );
        assert_eq!(row["opening"], "london-v1");
        assert_eq!(row["max_searches"], 2);
        assert!(row["cpu"].as_f64().unwrap() > 0.0);
    }
}

/// Seat streams across plain, search and replay (EXECUTOR-SUPPORT.md 5.4
/// item 3): (a) the two arms make identical pregame draws and reach the
/// first searched decision with the same actions and state; (b) every
/// draw of either live stream is that seat's own plain draw, never taken by
/// a search; (c) both replays reproduce each arm's actions and terminal,
/// and a second live run is byte-identical apart from wall and CPU.
/// Also: a plain London and windows game ends naturally, and the search
/// ceiling stops at three full searches.
#[test]
#[ignore = "native qualification: four fixture games with small flat-root searches; run with cargo test --release"]
fn seat_streams_match_across_plain_search_and_replay() {
    // Give the roots enough work to gather natural backups and execute a
    // tree action, so the focal stream's skipped draws are observable.
    const STREAM_T: u64 = 32_000;
    let (policy, sh, src) = london_fixture(1, (100_000, 200_000));
    let (plain, search, again) = on_big_stack(|| {
        let extras = BudgetExtras {
            per_decision: Some((STREAM_T, STREAM_T)),
            ..search_extras(3)
        };
        let play = || {
            budget_case_with_executor(
                &sh,
                &policy,
                &src,
                10 * STREAM_T,
                KeyMode::Repaired,
                false,
                None,
                Recording::Disabled,
                "fixture",
                ExecutionMode::Persistent,
                extras,
            )
            .unwrap()
        };
        (
            budget_plain_case_opening(&sh, &policy, &src, "fixture", Some(LONDON)).unwrap(),
            play(),
            play(),
        )
    });
    for row in [&plain, &search] {
        assert_eq!(
            row["game"]["valid"], true,
            "{}",
            row["game"]["invalid_reason"]
        );
        assert_eq!(
            row["priority_windows"],
            json!(["action-windows-v1", "action-windows-v1"])
        );
        assert_streams(&sh, &policy, &src, row);
        let actions: Vec<u32> = serde_json::from_value(row["game"]["actions"].clone()).unwrap();
        let replay = budget_replay_opening(&sh, &src, &actions, Some(LONDON)).unwrap();
        assert_eq!(replay["end"], row["game"]["end"]);
        assert_eq!(replay["sequence"], row["game"]["sequence"]);
        let parity =
            budget_parity_replay(&sh, &policy, &src, &actions, "fixture", Some(LONDON)).unwrap();
        assert_eq!(parity["end"], row["game"]["end"]);
        assert_eq!(parity["opening_record"], row["game"]["opening_record"]);
        let decisions = parity["decisions"].as_array().unwrap();
        assert_eq!(decisions.len(), actions.len());
        assert!(decisions
            .iter()
            .any(|d| d["pregame"] == true && d["logit_bits"].is_array()));
        assert!(decisions
            .iter()
            .all(|d| d["logit_bits"].is_array() == (d["k"].as_u64().unwrap() >= 2)));
    }
    assert_eq!(scientific(search.clone()), scientific(again));
    let searches = search["game"]["searches"].as_array().unwrap();
    assert_eq!(searches.len(), 3);
    assert!(searches.iter().all(|x| x["budget"] == STREAM_T));
    assert!(search["game"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["search_ceiling"] == true));
    // (a) The same actions, so the same pregame draws and state, up to the
    // first searched decision.
    let first = searches[0]["step"].as_u64().unwrap();
    let (p, s): (Vec<u32>, Vec<u32>) = (
        serde_json::from_value(plain["game"]["actions"].clone()).unwrap(),
        serde_json::from_value(search["game"]["actions"].clone()).unwrap(),
    );
    let (mut at, mut i) = (None, 0);
    bank::walk(
        opening_session(&sh.spec, &src, Some(LONDON)).unwrap(),
        &s,
        |_, d, _| {
            if d.step == first {
                at = Some(i);
            }
            i += 1;
            Ok(())
        },
    )
    .unwrap();
    let n = at.unwrap();
    assert_eq!(p[..n], s[..n]);
    let reach = |a: &[u32]| {
        bank::walk(
            opening_session(&sh.spec, &src, Some(LONDON)).unwrap(),
            a,
            |_, _, _| Ok(()),
        )
        .unwrap()
        .privileged_core_environment_hash()
    };
    assert_eq!(reach(&p[..n]), reach(&s[..n]));
}
