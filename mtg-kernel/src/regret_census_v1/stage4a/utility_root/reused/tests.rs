//! Synthetic producer envelopes on REAL replayed sessions/Keyer histories.
//! These test consumer authentication, never genuine search/tree membership.
//! The distinct refresh fixture exercises actual search-produced acquisition.
use super::*;
use crate::regret_census_v1::stage4a::bank::{BankSpec, DeckSpec, SPEC_SCHEMA};
use crate::regret_census_v1::stage4a::world::DeckPrior;
use crate::rl_session::FastActorResponseV1;
use crate::runtime_decks::RUNTIME_DECKS;

const BINDING: &str = "4444444444444444444444444444444444444444444444444444444444444444";

struct ReplayFixture {
    shared: BankShared,
    request: ComparisonRequest,
    row: Value,
    key: tree::Key,
    canon: tree::Canon,
}

/// Select a fixed ordinal of later focal decisions while playing action zero.
/// Budgets/backups/tree hash are explicitly synthetic. Observation identities,
/// prefix transitions, physical/substep boundaries, key and edge mapping are real.
fn fixture(target_ordinal: usize) -> ReplayFixture {
    let case = SourceSpec {
        id: "reused-consumer-replay-fixture".into(),
        own: 5,
        other: 1,
        seat: 0,
        opp: "fixture".into(),
        seed: 11,
        starting_player: 0,
        source_policy_seeds: [101, 103],
        anchor_draw: 0,
        guide_policy_seeds: [107, 109],
        search_root: "synthetic-membership-real-replay-v1".into(),
    };
    let shared = BankShared {
        spec: BankSpec {
            schema: SPEC_SCHEMA.into(),
            namespace: "spy-reset-pilot-v1".into(),
            lineage: "fixture".into(),
            runtime_rules: "resolution-boundary-v1".into(),
            select_rule: "fpu-1.5".into(),
            max_physical_decisions: 128,
            max_policy_steps: 256,
            decks: RUNTIME_DECKS
                .iter()
                .map(|deck| DeckSpec {
                    label: deck.id.into(),
                    mainboard: deck.card_ids.to_vec(),
                })
                .collect(),
            sources: vec![case.clone()],
        },
        labels: vec!["fixture".into()],
        opponents: Vec::new(), // Authentication does not load or call policies.
        prior: DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>()),
        limits: super::super::super::arms::Limits::FORMAL,
    };
    let mut session = bank::source_session(&shared.spec, &case).unwrap();
    let mut keyer = Keyer::new(KeyMode::Repaired, KeyStats::default());
    let (mut actions, mut decisions) = (Vec::new(), Vec::new());
    let mut search = None;
    let mut later = 0;
    let mut selected = None;
    let mut selected_mapping = None;
    for _ in 0..96 {
        let d = decision(&session).expect("fixture ended before selected later focal choice");
        let action = 0;
        if acting(&d) == PlayerId(case.seat) && d.legal_action_count >= 2 {
            let identity = bank::decision_identity(&session).unwrap();
            let initial = search.is_none();
            if initial {
                search = Some(json!({"search":0,"step":d.step,"root":identity,
                    "reason":"initial","budget":128000,"transitions":128000,
                    "incomplete":false,"faults":[],"tree":{"hash":"b".repeat(64)}}));
            }
            let originating = search.as_ref().unwrap();
            let keyed = keyer.step(&session, d).unwrap();
            let (key, _, _, canon) = keyer.storage(&keyed);
            if d.physical_decision_id
                > originating["root"]["physical_decision_id"]
                    .as_u64()
                    .unwrap()
            {
                later += 1;
            }
            let target = later == target_ordinal && !initial;
            let mut recorded = json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep":d.substep_index,"k":d.legal_action_count,"search":0,
                "chooser":if initial { "new_root" } else { "plain" },"edge_backups":null});
            if target {
                let edge = canon
                    .edges
                    .iter()
                    .position(|edge| *edge == canon.menu[action as usize])
                    .unwrap();
                let witness = json!({"schema":reuse_witness::SCHEMA,"decision":identity,
                    "search":0,"search_tree_hash":originating["tree"]["hash"],
                    "node_key":pathlog::key_hex(&key),"storage_edge_index":edge,
                    "storage_edge_sha256":sha(&canon.edges[edge]),"applied_live_index":action,
                    "natural_terminal_backups":8,"focal_wins":6,
                    "mean_focal_win_rate":0.75,"selection_backup":"mean","selection_value":0.75});
                recorded["chooser"] = json!("reused");
                recorded["edge_backups"] = json!(8);
                recorded["reuse_witness"] = witness.clone();
                selected = Some(json!({"action_index":actions.len(),"action":action,
                    "originating_search":0,"originating_search_step":originating["step"],
                    "originating_search_physical_decision_id":originating["root"]["physical_decision_id"],
                    "decision":identity,"witness":witness}));
                selected_mapping = Some((key, canon.clone()));
            }
            keyer.advance(keyed, action);
            decisions.push(recorded);
        }
        session.step(d.episode_id, d.step, action).unwrap();
        actions.push(action);
        if selected.is_some() {
            break;
        }
    }
    let mut selected =
        selected.expect("fixture must expose selected later focal choice within96steps");
    selected["post_action_identity"] = decision(&session)
        .map(|_| bank::decision_identity(&session).unwrap())
        .unwrap_or(Value::Null);
    selected["post_action_terminal"] = match session.current_response() {
        FastActorResponseV1::Terminal(t) => json!(t),
        FastActorResponseV1::Decision(_) => Value::Null,
    };
    selected["post_action_diagnostic_state_hash"] =
        json!(format!("{:016x}", session.diagnostic_state_hash()));
    selected["post_action_core_environment_hash"] = json!(format!(
        "{:016x}",
        session.privileged_core_environment_hash()
    ));
    selected["post_action_policy_steps"] = json!(session.policy_step_count());
    selected["post_action_physical_decisions"] = json!(session.physical_decision_count());
    let request = ComparisonRequest {
        schema: REQUEST_SCHEMA.into(),
        id: "consumer-authentication-fixture".into(),
        bank_spec_sha256: "5".repeat(64),
        recorded_build: RecordedBuild {
            git_head: "1".repeat(40),
            tracked_tree_sha256: "2".repeat(64),
        },
        prefix: RecordPin {
            path: "synthetic-producer-envelope.json".into(),
            sha256: "3".repeat(64),
        },
        case,
        root: RootCoordinates {
            step: selected["decision"]["step"].as_u64().unwrap(),
            physical_decision_id: selected["decision"]["physical_decision_id"]
                .as_u64()
                .unwrap(),
            substep: selected["decision"]["substep"].as_u64().unwrap() as u32,
        },
        mode: Comparison::ReusedVsRaw,
        comparator_policy_seeds: Some([307, 311]),
        continuation_seeds: ContinuationSeeds {
            focal: [211, 223],
            opponent: [227, 229],
        },
        max_transitions: 1,
    };
    let mut game = json!({"actions":actions,"decisions":decisions,"searches":[search.unwrap()],
        "key_mode":"repaired-v2","real_transitions":actions.len(),"transitions_used":128000});
    reuse_witness::Recording::Enabled.annotate(&mut game);
    let row = json!({"kind":"reused_prefix","schema":"s4a-reused-prefix/v1",
        "status":"acquired","complete":true,"lineage":shared.spec.lineage,
        "bank_binding_sha256":BINDING,"git_head":request.recorded_build.git_head,
        "tracked_tree_sha256":request.recorded_build.tracked_tree_sha256,"case":request.case,
        "allocation":{"initial":128000,"refresh":64000,"total":2048000},
        "acquisition_search_cap":320000,"interchange":false,"backup":"mean",
        "error":null,"blocked_search":null,"selected":selected,"game":game});
    let (key, canon) = selected_mapping.unwrap();
    ReplayFixture {
        shared,
        request,
        row,
        key,
        canon,
    }
}

fn authenticated(f: &ReplayFixture, row: &Value) -> Result<ReusedRoot, String> {
    reconstruct_reused(&f.request, &f.shared, row, BINDING, &mut 0)
}

#[test]
fn replayed_prefix_binds_source_build_recipe_and_witness_presence() {
    let f = fixture(1);
    validate_comparison(&f.request, &f.shared).unwrap();
    let root = authenticated(&f, &f.row).unwrap();
    assert_eq!(root.identity, f.row["selected"]["decision"]);
    for (pointer, changed) in [
        ("/case/seed", json!(12)),
        ("/case/own", json!(0)),
        ("/lineage", json!("other")),
        ("/bank_binding_sha256", json!("0".repeat(64))),
        ("/git_head", json!("9".repeat(40))),
        ("/tracked_tree_sha256", json!("9".repeat(64))),
        ("/allocation/total", json!(320000)),
        ("/acquisition_search_cap", json!(319999)),
        ("/interchange", json!(true)),
        ("/backup", json!("bestline-m8")),
        ("/status", json!("acquisition_cap")),
        ("/complete", json!(false)),
        ("/game/reuse_witnesses/enabled", json!(false)),
        ("/selected/action_index", json!(0)),
        ("/selected/witness", Value::Null),
    ] {
        let mut row = f.row.clone();
        *row.pointer_mut(pointer).unwrap() = changed;
        assert!(
            authenticated(&f, &row).is_err(),
            "accepted changed {pointer}"
        );
    }
}

#[test]
fn replayed_prefix_rejects_stale_roots_post_state_and_terminal_metadata() {
    let f = fixture(1);
    assert!(authenticated(&f, &f.row).is_ok());
    for pointer in [
        "/selected/decision/obs_hash",
        "/selected/decision/menu_hash",
        "/selected/decision/state_hash",
        "/selected/post_action_diagnostic_state_hash",
        "/selected/post_action_core_environment_hash",
        "/game/searches/0/root/menu_hash",
    ] {
        let mut row = f.row.clone();
        *row.pointer_mut(pointer).unwrap() = json!("f".repeat(64));
        assert!(
            authenticated(&f, &row).is_err(),
            "accepted changed {pointer}"
        );
    }
    for (pointer, changed) in [
        ("/selected/post_action_policy_steps", json!(999)),
        ("/selected/post_action_physical_decisions", json!(999)),
        (
            "/selected/post_action_identity",
            json!({"fabricated_identity":true}),
        ),
        (
            "/selected/post_action_terminal",
            json!({"fabricated_terminal":true}),
        ),
        ("/selected/originating_search", json!(1)),
        (
            "/selected/originating_search_physical_decision_id",
            json!(999),
        ),
    ] {
        let mut row = f.row.clone();
        *row.pointer_mut(pointer).unwrap() = changed;
        let mut replayed = 0;
        assert!(
            reconstruct_reused(&f.request, &f.shared, &row, BINDING, &mut replayed).is_err(),
            "accepted {pointer}"
        );
        if pointer.contains("post_action") {
            assert!(replayed > 0, "lost authentication progress");
        }
    }
}

#[test]
fn replayed_keyer_authenticates_node_storage_edge_and_ordered_live_menu() {
    let f = fixture(1);
    let recorded = f.row["game"]["decisions"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let witness = &f.row["selected"]["witness"];
    let identity = &f.row["selected"]["decision"];
    let search = &f.row["game"]["searches"][0];
    let action = f.row["selected"]["action"].as_u64().unwrap() as u32;
    validate_witness(
        witness, recorded, identity, search, &f.key, &f.canon, action,
    )
    .unwrap();
    for (key, changed) in [
        ("node_key", json!("0".repeat(64))),
        ("search_tree_hash", json!("0".repeat(64))),
        ("storage_edge_index", json!(f.canon.edges.len())),
        ("storage_edge_sha256", json!("0".repeat(64))),
        (
            "applied_live_index",
            json!((action + 1) % f.canon.menu.len() as u32),
        ),
        ("natural_terminal_backups", json!(7)),
        ("focal_wins", json!(9)),
        ("mean_focal_win_rate", json!(0.5)),
        ("selection_backup", json!("bestline-m8")),
    ] {
        let mut bad = witness.clone();
        bad[key] = changed;
        assert!(
            validate_witness(&bad, recorded, identity, search, &f.key, &f.canon, action).is_err(),
            "accepted {key}"
        );
    }
    let mut reordered = f.canon.clone();
    let other = reordered
        .menu
        .iter()
        .position(|edge| *edge != reordered.menu[action as usize])
        .unwrap();
    reordered.menu.swap(action as usize, other);
    assert_eq!(reordered.menu.len(), f.canon.menu.len());
    assert!(
        validate_witness(witness, recorded, identity, search, &f.key, &reordered, action).is_err()
    );
}

#[test]
fn replayed_prefix_rejects_skipped_earlier_reuse_and_duplicate_decisions() {
    let f = fixture(2);
    assert!(authenticated(&f, &f.row).is_ok());
    let origin = f.row["game"]["searches"][0]["root"]["physical_decision_id"]
        .as_u64()
        .unwrap();
    let mut skipped = f.row.clone();
    let earlier = skipped["game"]["decisions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| {
            r["step"].as_u64().unwrap() < f.request.root.step
                && r["physical_decision_id"].as_u64().unwrap() > origin
        })
        .unwrap();
    earlier["chooser"] = json!("reused");
    let Err(error) = authenticated(&f, &skipped) else {
        panic!("must reject earlier reuse");
    };
    assert!(error.contains("earlier eligible"));
    let mut duplicated = f.row.clone();
    let record = duplicated["game"]["decisions"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    duplicated["game"]["decisions"]
        .as_array_mut()
        .unwrap()
        .push(record);
    assert!(authenticated(&f, &duplicated).is_err());
}

#[test]
fn comparator_modes_and_seed_namespaces_do_not_silently_add_or_skip_draws() {
    let mut f = fixture(1);
    assert!(validate_comparison(&f.request, &f.shared).is_ok());
    f.request.mode = Comparison::SameAction;
    assert!(validate_comparison(&f.request, &f.shared).is_err());
    f.request.comparator_policy_seeds = None;
    assert!(validate_comparison(&f.request, &f.shared).is_ok());
    f.request.continuation_seeds.focal[0] = f.request.case.guide_policy_seeds[0];
    assert!(validate_comparison(&f.request, &f.shared).is_err());
}

/// No search runs here. The synthetic producer envelope is only an adapter
/// fixture. Genuine acquisition is checked separately by refresh's fixture.
#[test]
#[ignore = "native qualification: at most80policy calls,20branch transitions,96prefix steps per authentication; no searches"]
fn isolated_raw_draw_same_action_parity_and_failure_progress_are_preserved() {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(|| {
            let mut f = fixture(1);
            let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
            policy.reset_sampling_v1([401, 409]);
            f.shared
                .opponents
                .push(policy.fork_for_collection_v3().unwrap());
            let root = authenticated(&f, &f.row).unwrap();
            let d = decision(&root.session).unwrap();
            let mut manual = policy.fork_for_collection_v3().unwrap();
            manual.reset_sampling_v1(f.request.comparator_policy_seeds.unwrap());
            let mut calls = Counters::default();
            let expected = act(&mut manual, &root.session, &d, &mut calls)
                .map_err(bank::err)
                .unwrap();
            assert_eq!(calls.inference, 1);
            let mut reference = policy.fork_for_collection_v3().unwrap();
            reference.reset_sampling_v1([401, 409]);
            let result = diagnose_reused(&f.request, &f.shared, &policy, &f.row, BINDING);
            assert_eq!(result["complete"], true, "{result}");
            assert_eq!(result["sampled_raw_action"], expected);
            assert_eq!(result["comparator_inference_calls"], 1);
            assert_eq!(result["comparator_sampler_draws"], 1);
            assert_eq!(result["reused"]["transitions"], 1);
            assert_eq!(result["comparison"]["transitions"], 1);
            assert_eq!(result["reused"]["inference_calls"], 0);
            assert_eq!(result["comparison"]["inference_calls"], 0);
            if expected == root.action {
                assert_eq!(result["equal_action_parity"], true);
            }
            f.request.mode = Comparison::SameAction;
            f.request.comparator_policy_seeds = None;
            let same = diagnose_reused(&f.request, &f.shared, &policy, &f.row, BINDING);
            assert_eq!(same["complete"], true, "{same}");
            assert_eq!(same["comparator_inference_calls"], 0);
            assert_eq!(same["comparator_sampler_draws"], 0);
            assert!(same["sampled_raw_action"].is_null());
            assert_eq!(same["equal_action_parity"], true);
            assert_eq!(same["reused"], same["comparison"]);
            f.request.max_transitions = 8;
            let tail = diagnose_reused(&f.request, &f.shared, &policy, &f.row, BINDING);
            assert_eq!(tail["complete"], true, "{tail}");
            assert_eq!(tail["comparator_inference_calls"], 0);
            assert_eq!(tail["comparator_sampler_draws"], 0);
            assert_eq!(tail["equal_action_parity"], true);
            assert_eq!(tail["reused"], tail["comparison"]);
            assert!(tail["reused"]["transitions"].as_u64().unwrap() <= 8);
            let mut bad = f.row.clone();
            bad["selected"]["post_action_core_environment_hash"] = json!("0".repeat(16));
            let failed = diagnose_reused(&f.request, &f.shared, &policy, &bad, BINDING);
            assert_eq!(failed["complete"], false);
            assert_eq!(failed["failure"]["phase"], "authentication");
            assert_eq!(
                failed["authentication_transitions"],
                f.row["game"]["actions"].as_array().unwrap().len()
            );
            assert_eq!(failed["comparator_inference_calls"], 0);
            assert_eq!(failed["comparator_sampler_draws"], 0);
            assert!(failed["reused"].is_null() && failed["comparison"].is_null());
            assert!(failed["paired_signed_result"].is_null());
            assert_eq!(failed["paired_signed_lower"], -2);
            assert_eq!(failed["paired_signed_upper"], 2);
            // Forked comparator and continuation work must leave the original
            // source policy's actor stream untouched.
            for _ in 0..32 {
                assert_eq!(
                    policy.select_fast_session_v1(&root.session).unwrap(),
                    reference.select_fast_session_v1(&root.session).unwrap()
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
