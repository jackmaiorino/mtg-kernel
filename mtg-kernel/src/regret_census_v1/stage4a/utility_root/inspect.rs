//! Bounded, authenticated public semantics of saved first-divergence roots.
//! `UTILITY_ROOT_SPEC` and `UTILITY_ROOT_SPEC_SHA256` pin one exact JSON batch:
//! {schema: "shared-root-utility-v1/inspect-request", id, lineage,
//!  bank_spec_sha256, recorded_build: {git_head, tracked_tree_sha256},
//!  cases: [{search: {path, sha256}, plain: {path, sha256}, case: SourceSpec,
//!           first_divergence_step}]}
//! There are 1..=324 unique case IDs and at most 25 prefix transitions per case.
//! The loaded policies bind identity only. No inspection function accepts one.

use super::*;
use crate::policy_observation_v6::ObservationV6;
use crate::rl::ActionSemanticV1;
use std::collections::HashSet;

const SCHEMA: &str = "shared-root-utility-v1/inspect-request";
const MAX_CASES: usize = 324;
const MAX_PREFIX: u64 = 25;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InspectCase {
    search: RecordPin,
    plain: RecordPin,
    case: SourceSpec,
    first_divergence_step: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InspectRequest {
    schema: String,
    id: String,
    lineage: String,
    bank_spec_sha256: String,
    recorded_build: RecordedBuild,
    cases: Vec<InspectCase>,
}

fn validate_batch(req: &InspectRequest, lineage: &str) -> Result<(), String> {
    if req.schema != SCHEMA || req.id.is_empty() || req.lineage != lineage {
        return Err("utility inspection schema, id or lineage differs".into());
    }
    hex_pin(&req.bank_spec_sha256, 64)?;
    hex_pin(&req.recorded_build.git_head, 40)?;
    hex_pin(&req.recorded_build.tracked_tree_sha256, 64)?;
    if req.cases.is_empty() || req.cases.len() > MAX_CASES {
        return Err(format!("utility inspection needs 1..={MAX_CASES} cases"));
    }
    let mut ids = HashSet::new();
    for item in &req.cases {
        if item.case.id.is_empty() || !ids.insert(&item.case.id) {
            return Err("utility inspection case ids must be nonempty and unique".into());
        }
        hex_pin(&item.search.sha256, 64)?;
        hex_pin(&item.plain.sha256, 64)?;
        if item.first_divergence_step > MAX_PREFIX {
            return Err(format!("utility inspection prefix exceeds {MAX_PREFIX}"));
        }
    }
    Ok(())
}

/// Keep only the discriminant and the safe visible label, in executable order.
/// No serialized semantic object or observation escapes this boundary.
fn menu(observation: &ObservationV6, actions: &[ActionSemanticV1]) -> Result<Vec<Value>, String> {
    let labels = crate::human_bo3_v1::gameplay_trace_labels_v1(observation, actions)
        .map_err(|_| "utility inspection actor-visible labels unavailable")?;
    if labels.len() != actions.len() {
        return Err("utility inspection label count differs from executable menu".into());
    }
    actions
        .iter()
        .zip(labels)
        .enumerate()
        .map(|(index, (action, label))| {
            let semantic = serde_json::to_value(action).map_err(|e| e.to_string())?;
            let kind = semantic["action_kind"]
                .as_str()
                .ok_or("utility inspection action kind unavailable")?;
            Ok(json!({"executable_index":index,"action_kind":kind,"label":label}))
        })
        .collect()
}

/// Immutable root access and no policy argument make the inspection stop exact.
fn inspect_root(root: &AuthenticatedRoot) -> Result<Value, String> {
    let session = &root.session;
    let d = decision(session).ok_or("utility inspection root is terminal")?;
    // This is the same frozen pending-trigger source projection as tree::raw_decision.
    let (observation, actions) = session
        .actor_visible_decision_v4(d)
        .map_err(|_| "utility inspection actor-visible V4 projection unavailable")?;
    if actions.len() != d.legal_action_count as usize {
        return Err("utility inspection executable menu width differs".into());
    }
    let ordered_menu = menu(&observation, &actions)?;
    let public = &observation.projection.surface;
    Ok(json!({
        "root":root.identity,
        "root_core_environment_hash":format!("{:016x}",session.privileged_core_environment_hash()),
        "root_policy_steps":session.policy_step_count(),
        "root_physical_decisions":session.physical_decision_count(),
        "shared_prefix_transitions":root.prefix_len,
        "shared_prefix_actions_sha256":root.prefix_sha256,
        "step":d.step,"physical_decision_id":d.physical_decision_id,
        "substep_index":d.substep_index,"substep_count":d.substep_count,
        "actor":d.acting_player,"k":d.legal_action_count,
        "turn":public.turn,"phase":public.phase,
        "active_player":public.active_player,"priority_player":public.priority_player,
        "engine_stage":public.engine_context.current_stage,
        "surface_stage":public.surface_context.current_stage,
        "policy_stage":observation.projection.policy_surface_context.current_stage,
        "saved_search_action":ordered_menu.get(root.search_action as usize)
            .ok_or("utility inspection saved search index outside menu")?,
        "saved_plain_action":ordered_menu.get(root.plain_action as usize)
            .ok_or("utility inspection saved plain index outside menu")?,
        "ordered_menu":ordered_menu,
        "inspection_inference_calls":0,"inspection_search_calls":0,
        "selected_root_transitions":0
    }))
}

/// Every requested case is represented once, including failures and panics.
fn collect_cases(
    cases: &[InspectCase],
    mut inspect: impl FnMut(&InspectCase, &mut u64) -> Result<Value, String>,
) -> Value {
    let mut succeeded = 0;
    let mut prefix_total = 0;
    let rows: Vec<_> = cases
        .iter()
        .map(|item| {
            let mut prefix_transitions = 0;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                inspect(item, &mut prefix_transitions)
            }))
            .unwrap_or_else(|_| Err("utility inspection case panicked".into()));
            prefix_total += prefix_transitions;
            let mut row = json!({"case":item.case,
                "search_row_sha256":item.search.sha256,"plain_row_sha256":item.plain.sha256,
                "first_divergence_step":item.first_divergence_step,
                "prefix_transitions":prefix_transitions});
            match result {
                Ok(diagnostic) => {
                    succeeded += 1;
                    row["status"] = json!("ok");
                    row["diagnostic"] = diagnostic;
                }
                Err(error) => {
                    row["status"] = json!("error");
                    row["error"] = json!(error);
                }
            }
            row
        })
        .collect();
    json!({"requested":cases.len(),"succeeded":succeeded,
        "failed":cases.len()-succeeded,"complete":succeeded==cases.len(),
        "prefix_transitions":prefix_total,
        "inspection_inference_calls":0,"inspection_search_calls":0,
        "selected_root_transitions":0,"cases":rows})
}

pub(super) fn run(
    cfg: &CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: &BankShared,
    bank_spec_bytes: &[u8],
) -> Result<(), String> {
    if cfg.games != 1 || cfg.first_game != 0 || cfg.workers != 1 {
        return Err("s4a-utility-root-inspect requires games=1, first_game=0, workers=1".into());
    }
    let path = std::env::var("UTILITY_ROOT_SPEC").map_err(|_| "UTILITY_ROOT_SPEC is required")?;
    let pin = std::env::var("UTILITY_ROOT_SPEC_SHA256")
        .map_err(|_| "UTILITY_ROOT_SPEC_SHA256 is required")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let req: InspectRequest = serde_json::from_slice(pinned_bytes(&bytes, &pin)?)
        .map_err(|e| format!("utility inspection request: {e}"))?;
    validate_batch(&req, &sh.spec.lineage)?;
    sh.spec.validate()?;
    pinned_bytes(bank_spec_bytes, &req.bank_spec_sha256)?;
    // Actual policy loading is retained upstream to authenticate historical identity.
    // Neither reconstruction nor inspection receives a policy or a policy fork.
    let historical_binding = bank::bank_binding_with_build(
        policy,
        sh,
        &req.recorded_build.git_head,
        &req.recorded_build.tracked_tree_sha256,
    )?;
    let current_binding = bank::bank_binding(policy, sh)?;
    let batch = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, || {
                collect_cases(&req.cases, |item, prefix_transitions| {
                    validate_case(&item.case, sh)?;
                    let search = read_record(&item.search)?;
                    let plain = read_record(&item.plain)?;
                    let root = reconstruct(
                        &item.case,
                        Root::Step {
                            step: item.first_divergence_step,
                        },
                        &sh.spec,
                        &search,
                        &plain,
                        &historical_binding,
                        prefix_transitions,
                    )?;
                    inspect_root(&root)
                })
            })
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "utility inspection batch worker panicked".to_owned())
    })?;
    let row = json!({"kind":"utility_root_inspection",
        "schema":"shared-root-utility-v1/inspect-receipt",
        "id":req.id,"lineage":req.lineage,"request_sha256":pin,
        "bank_spec_sha256":req.bank_spec_sha256,"recorded_build":req.recorded_build,
        "historical_bank_binding_sha256":historical_binding,
        "current_bank_binding_sha256":current_binding,
        "runtime":crate::gameplay_trace_v1::runtime_identity()?,
        "carddb_hash":format!("{:016x}",crate::card_def::KERNEL_CARDDB_HASH),
        "runtime_rules":sh.spec.runtime_rules,"batch":batch});
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&cfg.out)
        .map_err(|e| format!("fresh utility inspection output {}: {e}", cfg.out))?;
    writeln!(output, "{row}").map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())?;
    if batch["complete"] != true {
        return Err(format!(
            "utility inspection incomplete: {} failed cases",
            batch["failed"]
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_decks::RUNTIME_DECKS;

    fn replay_stack(test: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(test)
            .unwrap()
            .join()
            .unwrap();
    }

    fn spec() -> bank::BankSpec {
        bank::BankSpec {
            schema: bank::SPEC_SCHEMA.into(),
            namespace: "spy-reset-pilot-v1".into(),
            lineage: "fixture".into(),
            runtime_rules: "resolution-boundary-v1".into(),
            select_rule: "fpu-1.5".into(),
            max_physical_decisions: 128,
            max_policy_steps: 256,
            decks: RUNTIME_DECKS
                .iter()
                .map(|d| bank::DeckSpec {
                    label: d.id.into(),
                    mainboard: d.card_ids.to_vec(),
                })
                .collect(),
            sources: vec![],
        }
    }

    fn item(seat: u8) -> InspectCase {
        InspectCase {
            search: RecordPin {
                path: "search.json".into(),
                sha256: "a".repeat(64),
            },
            plain: RecordPin {
                path: "plain.json".into(),
                sha256: "b".repeat(64),
            },
            case: SourceSpec {
                id: format!("case-{seat}"),
                own: 0,
                other: 1,
                seat,
                opp: "fixture".into(),
                seed: 101,
                starting_player: seat,
                source_policy_seeds: [2, 3],
                anchor_draw: 5,
                guide_policy_seeds: [7, 11],
                search_root: "search-root".into(),
            },
            first_divergence_step: 0,
        }
    }

    /// Build a historical witness from a real opening, without constructing a policy.
    fn historical_rows(item: &mut InspectCase, spec: &bank::BankSpec) -> (Value, Value) {
        let src = &item.case;
        let mut session = bank::source_session(spec, src).unwrap();
        let mut prefix = Vec::new();
        let d = loop {
            let d = decision(&session).unwrap();
            if acting(&d) == PlayerId(src.seat) && d.legal_action_count >= 2 {
                break d;
            }
            assert!(prefix.len() < MAX_PREFIX as usize);
            prefix.push(0u32);
            session.step(d.episode_id, d.step, 0).unwrap();
        };
        item.first_divergence_step = d.step;
        let identity = bank::decision_identity(&session).unwrap();
        let mut search = json!({"kind":"budget_game","schema":"search-total-budget-v1/game",
            "lineage":spec.lineage,"bank_binding_sha256":"binding","id":src.id,
            "own":src.own,"other":src.other,"seat":src.seat,"opp":src.opp,
            "seed":src.seed,"starting_player":src.starting_player,
            "live_policy_seeds":src.guide_policy_seeds,"search_root":src.search_root,
            "game":{"actions":prefix,"decisions":[{"step":d.step,
                "physical_decision_id":d.physical_decision_id,"substep":d.substep_index,
                "k":d.legal_action_count,"chooser":"new_root","search":0}],
                "searches":[{"search":0,"step":d.step,"root":identity}]}});
        let mut plain = search.clone();
        plain["arm"] = json!("plain-v1");
        plain["game"]["decisions"][0]["chooser"] = json!("plain");
        search["game"]["actions"]
            .as_array_mut()
            .unwrap()
            .push(json!(0));
        plain["game"]["actions"]
            .as_array_mut()
            .unwrap()
            .push(json!(1));
        (search, plain)
    }

    fn replay(
        item: &InspectCase,
        spec: &bank::BankSpec,
        search: &Value,
        plain: &Value,
        transitions: &mut u64,
    ) -> Result<Value, String> {
        let root = reconstruct(
            &item.case,
            Root::Step {
                step: item.first_divergence_step,
            },
            spec,
            search,
            plain,
            "binding",
            transitions,
        )?;
        inspect_root(&root)
    }

    #[test]
    fn inspection_authenticates_both_seats_without_root_mutation_or_policy() {
        replay_stack(|| {
            let spec = spec();
            for seat in [0, 1] {
                let mut item = item(seat);
                let (search, plain) = historical_rows(&mut item, &spec);
                let mut transitions = 0;
                let root = reconstruct(
                    &item.case,
                    Root::Step {
                        step: item.first_divergence_step,
                    },
                    &spec,
                    &search,
                    &plain,
                    "binding",
                    &mut transitions,
                )
                .unwrap();
                let session = &root.session;
                let before = (
                    session.diagnostic_state_hash(),
                    session.privileged_core_environment_hash(),
                    session.policy_step_count(),
                    session.physical_decision_count(),
                    decision(session),
                );
                let raw_before =
                    super::super::super::tree::raw_decision(session, decision(session).unwrap())
                        .unwrap();
                let result = inspect_root(&root).unwrap();
                assert_eq!(result, inspect_root(&root).unwrap());
                assert_eq!(
                    before,
                    (
                        session.diagnostic_state_hash(),
                        session.privileged_core_environment_hash(),
                        session.policy_step_count(),
                        session.physical_decision_count(),
                        decision(session)
                    )
                );
                assert_eq!(
                    raw_before,
                    super::super::super::tree::raw_decision(session, decision(session).unwrap())
                        .unwrap()
                );
                assert_eq!(transitions, item.first_divergence_step);
                assert_eq!(result["inspection_inference_calls"], 0);
                assert_eq!(result["inspection_search_calls"], 0);
                assert_eq!(result["selected_root_transitions"], 0);
                assert_eq!(result["saved_search_action"], result["ordered_menu"][0]);
                assert_eq!(result["saved_plain_action"], result["ordered_menu"][1]);
                assert_eq!(result["root"], search["game"]["searches"][0]["root"]);
                assert!(!result.to_string().contains("arena_id"));
                assert!(result.get("observation").is_none());
            }
        });
    }

    #[test]
    fn ordered_labels_keep_duplicates_visible_names_and_both_seats() {
        replay_stack(|| {
            let spec = spec();
            for seat in [0, 1] {
                let mut item = item(seat);
                let (search, plain) = historical_rows(&mut item, &spec);
                let root = reconstruct(
                    &item.case,
                    Root::Step {
                        step: item.first_divergence_step,
                    },
                    &spec,
                    &search,
                    &plain,
                    "binding",
                    &mut 0,
                )
                .unwrap();
                let (obs, actions) = root
                    .session
                    .actor_visible_decision_v4(decision(&root.session).unwrap())
                    .unwrap();
                let a = actions[0].clone();
                let b = actions[1].clone();
                let repeated = [b.clone(), a.clone(), b];
                let original = menu(&obs, &actions).unwrap();
                let rows = menu(&obs, &repeated).unwrap();
                assert_eq!(rows.len(), 3);
                for (index, expected) in [1usize, 0, 1].into_iter().enumerate() {
                    assert_eq!(rows[index]["executable_index"], index);
                    assert_eq!(rows[index]["label"], original[expected]["label"]);
                    assert_eq!(
                        rows[index]["action_kind"],
                        original[expected]["action_kind"]
                    );
                    assert_eq!(rows[index].as_object().unwrap().len(), 3);
                }
                assert!(!rows[0]["label"].as_str().unwrap().is_empty());
                assert!(!serde_json::to_string(&rows).unwrap().contains("arena_id"));
            }
        });
    }

    #[test]
    fn batch_keeps_every_failure_and_historical_mismatch_in_order() {
        replay_stack(|| {
            let spec = spec();
            let mut good = item(0);
            let (search, plain) = historical_rows(&mut good, &spec);
            let mut bad = item(1);
            let (mut wrong_search, wrong_plain) = historical_rows(&mut bad, &spec);
            wrong_search["game"]["searches"][0]["root"]["menu_hash"] = json!("wrong-menu");
            let mut missing = item(0);
            missing.case.id = "missing-record".into();
            missing.search.path = format!("nonexistent-utility-root-{}.json", std::process::id());
            let batch = collect_cases(&[good, bad, missing], |item, count| {
                match item.case.id.as_str() {
                    "case-0" => replay(item, &spec, &search, &plain, count),
                    "case-1" => replay(item, &spec, &wrong_search, &wrong_plain, count),
                    _ => {
                        read_record(&item.search)?;
                        unreachable!()
                    }
                }
            });
            assert_eq!(batch["requested"], 3);
            assert_eq!(batch["succeeded"], 1);
            assert_eq!(batch["failed"], 2);
            assert_eq!(batch["complete"], false);
            assert_eq!(batch["cases"][0]["status"], "ok");
            assert_eq!(batch["cases"][1]["case"]["id"], "case-1");
            assert!(batch["cases"][1]["error"]
                .as_str()
                .unwrap()
                .contains("menu_hash"));
            assert_eq!(batch["cases"][2]["case"]["id"], "missing-record");
            assert_eq!(batch["cases"][2]["status"], "error");
            assert_eq!(batch["cases"][2]["prefix_transitions"], 0);
        });
    }

    #[test]
    fn batch_refuses_duplicates_wrong_lineage_and_unbounded_prefix_before_replay() {
        let mut request = InspectRequest {
            schema: SCHEMA.into(),
            id: "batch".into(),
            lineage: "fixture".into(),
            bank_spec_sha256: "a".repeat(64),
            recorded_build: RecordedBuild {
                git_head: "b".repeat(40),
                tracked_tree_sha256: "c".repeat(64),
            },
            cases: vec![item(0), item(1)],
        };
        assert!(validate_batch(&request, "fixture").is_ok());
        request.cases[1].case.id = request.cases[0].case.id.clone();
        assert!(validate_batch(&request, "fixture").is_err());
        request.cases.pop();
        assert!(validate_batch(&request, "other").is_err());
        request.cases[0].first_divergence_step = MAX_PREFIX + 1;
        assert!(validate_batch(&request, "fixture").is_err());
        request.cases.clear();
        assert!(validate_batch(&request, "fixture").is_err());
        request.cases = (0..MAX_CASES + 1)
            .map(|n| {
                let mut item = item(0);
                item.case.id = format!("case-{n}");
                item
            })
            .collect();
        assert!(validate_batch(&request, "fixture").is_err());
    }

    #[test]
    #[ignore = "native qualification: fixture policy forks, no inference"]
    fn continuation_modes_keep_their_saved_action_and_cap_semantics() {
        replay_stack(|| {
            let spec = spec();
            let mut item = item(0);
            let (search, plain) = historical_rows(&mut item, &spec);
            let policy = FrozenPlayPolicyV1::training_fixture_v3();
            let sh = BankShared {
                prior: super::super::super::world::DeckPrior::new(
                    &(0..spec.decks.len()).collect::<Vec<_>>(),
                ),
                spec,
                labels: vec!["fixture".into()],
                opponents: vec![policy.fork_for_collection_v3().unwrap()],
                limits: super::super::super::arms::Limits::FORMAL,
            };
            let mut req = Request {
                schema: REQUEST_SCHEMA.into(),
                id: "continuation".into(),
                bank_spec_sha256: "a".repeat(64),
                recorded_build: RecordedBuild {
                    git_head: "b".repeat(40),
                    tracked_tree_sha256: "c".repeat(64),
                },
                search: item.search,
                plain: item.plain,
                case: item.case,
                root: Root::Step {
                    step: item.first_divergence_step,
                },
                continuation_seeds: ContinuationSeeds {
                    focal: [13, 17],
                    opponent: [19, 23],
                },
                mode: Mode::SameAction,
                max_transitions: 1,
            };
            let same = diagnose(&req, &sh, &policy, &search, &plain, "binding").unwrap();
            assert_eq!(same["search"], same["comparison"]);
            assert_eq!(same["search"]["first_action"], 0);
            assert_eq!(same["same_action_parity"], true);
            req.mode = Mode::SearchVsPlain;
            let paired = diagnose(&req, &sh, &policy, &search, &plain, "binding").unwrap();
            assert_eq!(same["root"], paired["root"]);
            assert_eq!(same["search"], paired["search"]);
            assert_eq!(paired["comparison"]["first_action"], 1);
            assert_eq!(paired["total_inference_calls"], 0);
            assert_eq!(paired["total_transitions"], item.first_divergence_step + 2);
            assert_eq!(paired["search"]["transitions"], 1);
            assert_eq!(paired["comparison"]["transitions"], 1);
        });
    }
}
