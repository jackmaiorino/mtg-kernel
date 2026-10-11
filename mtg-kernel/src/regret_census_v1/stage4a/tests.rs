use super::arms::{Limits, Roles, RootCtx};
use super::play::{Meter, PlayErr};
use super::*;

#[test]
fn selection_setting_refuses_invalid_encoding_unknown_rule_and_formal_modes() {
    use std::env::VarError;
    assert_eq!(
        parse_select_rule("s4a-diag", Err(VarError::NotPresent)).unwrap(),
        None
    );
    for alias in ["", "untried-first"] {
        assert_eq!(parse_select_rule("s4a", Ok(alias.into())).unwrap(), None);
    }
    assert_eq!(
        parse_select_rule("s4a-diag", Ok("fpu-1.5".into())).unwrap(),
        Some(1.5)
    );
    for mode in ["s4a", "corpus", "s4a-duel"] {
        assert!(parse_select_rule(mode, Ok("fpu-1.5".into())).is_err());
    }
    assert!(parse_select_rule("s4a-diag", Ok("unknown".into())).is_err());
    assert!(parse_select_rule("s4a-diag", Err(VarError::NotUnicode("opaque".into()))).is_err());
}

#[test]
fn selection_resume_requires_typed_identity_for_both_runtime_profiles() {
    let boundary = Some(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1);
    for runtime in [None, boundary] {
        for selection in [None, Some("fpu-1.5")] {
            let row = json!({"kind":"s4a_diag_root","root_id":"fixture",
                "runtime_rules":runtime.map(|_| "resolution-boundary-v1"),
                "select_rule":selection});
            let bytes = format!("{row}\n");
            let done =
                completed_identity_roots(bytes.as_bytes(), "s4a_diag_root", runtime, selection)
                    .unwrap();
            assert!(done.contains("fixture"));
            let other = if selection.is_none() {
                Some("fpu-1.5")
            } else {
                None
            };
            assert!(
                completed_identity_roots(bytes.as_bytes(), "s4a_diag_root", runtime, other)
                    .is_err()
            );
            for bad in [
                json!(false),
                json!(17),
                json!({}),
                json!([]),
                json!("unknown"),
            ] {
                let mut invalid = row.clone();
                invalid["select_rule"] = bad;
                let bad_bytes = format!("{invalid}\n");
                assert!(completed_identity_roots(
                    bad_bytes.as_bytes(),
                    "s4a_diag_root",
                    runtime,
                    selection
                )
                .is_err());
            }
        }
    }
}

#[test]
fn selection_resume_preserves_legacy_rows_and_ignores_only_incomplete_tail() {
    let bytes = b"{\"kind\":\"s4a_diag_root\",\"root_id\":\"old\"}\n{\"kind\":\"error\",\"select_rule\":true}\n\xff";
    let done = completed_identity_roots(bytes, "s4a_diag_root", None, None).unwrap();
    assert!(done.contains("old"));
    assert!(completed_identity_roots(bytes, "s4a_diag_root", None, Some("fpu-1.5")).is_err());
    let malformed = b"{\"kind\":\"s4a_diag_root\",\"root_id\":\"old\",\"select_rule\":true}\n\xff";
    assert!(completed_identity_roots(malformed, "s4a_diag_root", None, None).is_err());
}
use crate::ids::PlayerId;
use std::collections::BTreeMap;

#[test]
fn runtime_resume_requires_matching_identity_and_preserves_legacy_rows() {
    let kind = "s4a_diag_root";
    let active = Some(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1);
    let legacy = b"{\"kind\":\"s4a_diag_root\",\"root_id\":\"a\"}\n";
    let historical = b"{\"kind\":\"s4a_diag_root\",\"root_id\":\"b\",\"runtime_rules\":null}\n";
    let current = b"{\"kind\":\"s4a_diag_root\",\"root_id\":\"c\",\"runtime_rules\":\"resolution-boundary-v1\"}\n";
    assert!(completed_runtime_roots(legacy, kind, None)
        .unwrap()
        .contains("a"));
    assert!(completed_runtime_roots(historical, kind, None)
        .unwrap()
        .contains("b"));
    assert!(completed_runtime_roots(current, kind, active)
        .unwrap()
        .contains("c"));
    assert!(completed_runtime_roots(legacy, kind, active).is_err());
    assert!(completed_runtime_roots(historical, kind, active).is_err());
    assert!(completed_runtime_roots(current, kind, None).is_err());
    let mixed = [legacy.as_slice(), current.as_slice()].concat();
    assert!(completed_runtime_roots(&mixed, kind, None).is_err());
    assert!(completed_runtime_roots(&mixed, kind, active).is_err());
    for invalid in ["true", "12", "\"future-runtime\""] {
        let row = format!(
            "{{\"kind\":\"s4a_diag_root\",\"root_id\":\"d\",\"runtime_rules\":{invalid}}}\n"
        );
        assert!(completed_runtime_roots(row.as_bytes(), kind, None).is_err());
    }
}

#[test]
fn runtime_resume_ignores_only_partial_tail_and_other_row_kinds() {
    let mut bytes = b"{\"kind\":\"s4a_diag_root\",\"root_id\":\"a\"}\n".to_vec();
    bytes.extend_from_slice(b"{\"runtime_rules\":\"resolution-boundary-v1\",\"unfinished\":\"");
    bytes.push(0xf0);
    assert!(completed_runtime_roots(&bytes, "s4a_diag_root", None)
        .unwrap()
        .contains("a"));
    assert!(completed_runtime_roots(
        &bytes,
        "s4a_diag_root",
        Some(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
    )
    .is_err());
    bytes.push(b'\n');
    assert!(completed_runtime_roots(&bytes, "s4a_diag_root", None).is_err());
    assert!(completed_runtime_roots(b"{broken completed row}\n", "s4a_diag_root", None).is_err());
    let other = b"{\"kind\":\"error\",\"root_id\":\"a\",\"runtime_rules\":true}\n";
    assert!(completed_runtime_roots(other, "s4a_diag_root", None)
        .unwrap()
        .is_empty());
}

#[test]
fn runtime_setting_refuses_unknown_identity_and_unsupported_modes() {
    for setting in [None, Some(""), Some("historical")] {
        for mode in ["s4a-corpus", "s4a-run", "s4a-diag"] {
            assert_eq!(parse_runtime_rules(mode, setting).unwrap(), None);
        }
    }
    assert_eq!(
        parse_runtime_rules("s4a-diag", Some("resolution-boundary-v1")).unwrap(),
        Some(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
    );
    for mode in ["s4a-corpus", "s4a-run"] {
        assert!(parse_runtime_rules(mode, Some("resolution-boundary-v1")).is_err());
    }
    assert!(parse_runtime_rules("s4a-diag", Some("unknown")).is_err());
}

fn test_cfg() -> CensusConfigV1 {
    CensusConfigV1 {
        source: String::new(),
        out: String::new(),
        first_game: 0,
        games: 1,
        base_seed: 2026100941,
        root_prob: 0.0,
        rollouts: 1,
        max_actions: 4,
        workers: 1,
        decks: (0..9).collect(),
        mode: "s4a-corpus".into(),
        pilot_deck: 0,
    }
}

fn fixture() -> FrozenPlayPolicyV1 {
    FrozenPlayPolicyV1::training_fixture_v4()
}

fn spy_index() -> usize {
    RUNTIME_DECKS.iter().position(|d| d.id == SPY_DECK).unwrap()
}

/// Visits every decision of a fixture game (both seats).
fn each_decision(
    game: u64,
    mut f: impl FnMut(&crate::rl_session::FastActorSessionV1, &crate::rl_session::FastActorDecisionV1),
) {
    let cfg = test_cfg();
    let setup = game_setup(&cfg, 1, game);
    let (mut a, mut b) = (fixture(), fixture());
    drive(&setup, &mut a, &mut b, |s, d, _| {
        f(s, d);
        Ok(false)
    })
    .unwrap();
}

fn sorted_defs(st: &crate::state::GameState, ids: &[crate::ids::ObjectId]) -> Vec<u16> {
    let mut v: Vec<u16> = ids.iter().map(|&i| st.objects.get(i).card_def).collect();
    v.sort_unstable();
    v
}

#[test]
fn meter_refuses_the_transition_that_would_exceed_the_cap() {
    let mut m = Meter::new(2);
    assert!(m.charge().is_ok() && m.charge().is_ok());
    assert_eq!(m.charge(), Err(PlayErr::Truncated));
    assert_eq!(m.spent, 2);
}

/// Sampler sweep over fixture games of all nine decks: every sample keeps
/// the actor's canonical decision view, own cards and public zones; the prior
/// leaves the true deck; library-search decisions sample; samples repeat.
#[test]
fn sampler_with_deck_prior_over_nine_deck_games() {
    let prior = world::DeckPrior::new(&(0..9).collect::<Vec<_>>());
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    let (mut ok, mut library_ok, mut library) = (0u64, 0u64, 0u64);
    let mut decks_seen = std::collections::BTreeSet::new();
    for game in [0u64, 10, 20, 30, 40, 50, 60, 70, 80] {
        let mut n = 0u64;
        each_decision(game, |s, d| {
            n += 1;
            if !n.is_multiple_of(3) {
                return;
            }
            let actor = play::acting(d);
            let opp = actor.opponent();
            let is_library = s
                .actor_visible_decision_v4(*d)
                .map(|(o, _)| o.extensions.decision_local_library.is_some())
                .unwrap_or(false);
            library += u64::from(is_library);
            let seed = game * 1000 + n;
            match world::sample(s, seed, &prior) {
                Ok(w) => {
                    ok += 1;
                    library_ok += u64::from(is_library);
                    decks_seen.insert(w.prior_deck);
                    let (a, b) = (s.game_state(), w.world.game_state());
                    let me = actor.index();
                    assert_eq!(
                        sorted_defs(a, &a.players[me].hand),
                        sorted_defs(b, &b.players[me].hand)
                    );
                    let own = |st: &crate::state::GameState| {
                        let mut v = sorted_defs(st, &st.players[me].library);
                        v.extend(sorted_defs(st, &st.players[me].hand));
                        v.sort_unstable();
                        v
                    };
                    assert_eq!(own(a), own(b));
                    for p in [actor, opp] {
                        let i = p.index();
                        assert_eq!(
                            sorted_defs(a, &a.players[i].battlefield),
                            sorted_defs(b, &b.players[i].battlefield)
                        );
                        assert_eq!(
                            sorted_defs(a, &a.players[i].graveyard),
                            sorted_defs(b, &b.players[i].graveyard)
                        );
                        assert_eq!(a.players[i].hand.len(), b.players[i].hand.len());
                        assert_eq!(a.players[i].library.len(), b.players[i].library.len());
                    }
                    let wd = play::decision(&w.world).unwrap();
                    assert_eq!(
                        tree::canon(s, *d).unwrap(),
                        tree::canon(&w.world, wd).unwrap()
                    );
                    let again = world::sample(s, seed, &prior).unwrap();
                    assert_eq!(
                        w.world.diagnostic_state_hash(),
                        again.world.diagnostic_state_hash()
                    );
                }
                Err(e) => {
                    let k: String = e.split(':').next().unwrap_or("").to_owned();
                    *kinds.entry(k).or_default() += 1;
                }
            }
        });
    }
    eprintln!(
        "ok {ok} library {library} library_ok {library_ok} rejections {kinds:?} decks {decks_seen:?}"
    );
    assert!(ok > 100, "{ok}");
    assert_eq!(
        library, library_ok,
        "library-search samples rejected: {kinds:?}"
    );
    assert!(
        decks_seen.len() >= 3,
        "prior never left the true deck: {decks_seen:?}"
    );
}

/// The prior chooses only decks consistent with the opponent's public cards.
#[test]
fn deck_prior_is_consistent_with_public_cards() {
    let prior = world::DeckPrior::new(&(0..9).collect::<Vec<_>>());
    let mut late = None;
    each_decision(4 * 9 + 7, |s, d| {
        let st = s.game_state();
        let opp = play::acting(d).opponent();
        if late.is_none()
            && st.players[opp.index()].graveyard.len() + st.players[opp.index()].battlefield.len()
                >= 6
        {
            late = Some(s.clone());
        }
    });
    let s = late.expect("a late decision");
    let d = play::decision(&s).unwrap();
    let actor = play::acting(&d);
    let opp = actor.opponent();
    let st = s.game_state();
    let public: Vec<u16> = st
        .objects
        .iter()
        .filter(|(_, o)| {
            o.owner == opp
                && !o.v4.is_token
                && matches!(
                    o.zone,
                    crate::state::Zone::Battlefield | crate::state::Zone::Graveyard
                )
        })
        .map(|(_, o)| o.card_def)
        .collect();
    let mut chosen_any = 0;
    for seed in 0..40u64 {
        let mut state = st.clone();
        let chosen = world::apply_deck_prior(&mut state, actor, &prior, seed).unwrap();
        chosen_any += 1;
        let deck = &RUNTIME_DECKS[chosen].card_ids;
        for c in &public {
            assert!(deck.contains(c), "chosen deck lacks a public card");
        }
        for &i in state.players[opp.index()]
            .hand
            .iter()
            .chain(&state.players[opp.index()].library)
        {
            assert!(deck.contains(&state.objects.get(i).card_def));
        }
    }
    assert_eq!(chosen_any, 40);
}

/// E, A and D on a fixture Spy-deck root with small ceilings: one node per
/// completed simulation, ceilings respected, and the whole package repeats.
#[test]
fn small_root_package_is_deterministic_and_within_ceilings() {
    let cfg = test_cfg();
    let spy = spy_index();
    let game = spy as u64 + 9 * 3;
    let setup = game_setup(&cfg, 1, game);
    assert_eq!(setup.decks[setup.focal], spy);
    let focal_id = PlayerId(setup.focal as u8);
    let (mut a, mut b) = (fixture(), fixture());
    let mut root = None;
    drive(&setup, &mut a, &mut b, |s, d, _| {
        if play::acting(d) == focal_id && d.legal_action_count >= 3 && s.game_state().turn >= 3 {
            root = Some(s.clone());
            return Ok(true);
        }
        Ok(false)
    })
    .unwrap();
    let session = root.expect("a fixture root");
    let prior = world::DeckPrior::new(&cfg.decks);
    let run = || {
        let mut roles = Roles {
            focal: fixture(),
            opps: vec![fixture()],
            scorer: fixture(),
            inner_focal: fixture(),
            inner_opps: vec![fixture()],
        };
        let seeds = RootSeeds {
            model: "fixture".into(),
            root: "fixture-root".into(),
        };
        let d = play::decision(&session).unwrap();
        let probs = arms::softmax(&roles.scorer.score_fast_session_v1(&session).unwrap().logits);
        let ctx = RootCtx {
            root: &session,
            d,
            focal: focal_id,
            opp: 0,
            seeds: &seeds,
            prior: &prior,
            defs: spy_defs(),
            cast_root: false,
            probs: probs.clone(),
            limits: Limits {
                select_cap: 6_000,
                eval_worlds: 2,
                eval_cap: 4_000,
            },
            urgency: None,
        };
        let (e, tree) = roles.select_e(&ctx);
        assert!(e.transitions <= 6_000);
        assert!(e.faults.is_empty(), "{:?}", e.faults);
        assert!(e.completed > 0);
        let root_visits: u64 = tree
            .nodes
            .values()
            .filter(|n| n.parent == [0u8; 32])
            .map(|n| n.visits)
            .sum();
        assert_eq!(root_visits, e.completed);
        assert!(tree.nodes.len() as u64 <= e.completed);
        let ac = arms::a_candidates(&probs);
        let asel = roles.select_rounds(&ctx, "A", &ac);
        let dc = arms::d_candidates(&probs, &seeds);
        let dsel = roles.select_rounds(&ctx, "D", &dc);
        assert!(asel.transitions <= 6_000 && dsel.transitions <= 6_000);
        assert!(asel.faults.is_empty() && dsel.faults.is_empty());
        let mut st = arms::SamplerStats::default();
        let w = world::sample(&session, 99, &prior).unwrap();
        let outs: Vec<String> = [
            roles.evaluate(&ctx, "E", &w.world, 0, None, Some(&tree), &mut st),
            roles.evaluate(&ctx, "A", &w.world, 0, asel.choice.as_ref(), None, &mut st),
            roles.evaluate(&ctx, "D", &w.world, 0, dsel.choice.as_ref(), None, &mut st),
        ]
        .iter()
        .map(|o| {
            assert!(o.transitions <= 4_000);
            assert!(o.fault.is_none(), "{:?}", o.fault);
            o.json().to_string()
        })
        .collect();
        (
            tree.hash(),
            e.json().to_string(),
            asel.json().to_string(),
            dsel.json().to_string(),
            outs,
        )
    };
    assert_eq!(run(), run());
}

#[test]
#[ignore = "diagnostic: lists numeric observation fields"]
fn list_numeric_observation_fields() {
    fn walk(v: &serde_json::Value, path: &str, out: &mut std::collections::BTreeMap<String, u64>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    walk(x, &format!("{path}.{k}"), out)
                }
            }
            serde_json::Value::Array(a) => {
                a.iter().for_each(|x| walk(x, &format!("{path}[]"), out))
            }
            serde_json::Value::Number(_) => *out.entry(path.to_owned()).or_default() += 1,
            _ => {}
        }
    }
    let mut out = std::collections::BTreeMap::new();
    for game in 0..81u64 {
        each_decision(game, |s, d| {
            if let Ok((o, sem)) = s.actor_visible_decision_v4(*d) {
                walk(&serde_json::to_value(&o).unwrap(), "obs", &mut out);
                walk(&serde_json::to_value(&sem).unwrap(), "sem", &mut out);
            }
        });
    }
    for (k, n) in out {
        eprintln!("FIELD {k} {n}");
    }
}

/// Node keys follow the focal player's visible history, not engine handles:
/// swapping two hidden library objects of the same card (own and opponent
/// library) changes which handles are drawn but nothing visible, so every
/// focal node key, edge set and executed edge along the same play matches.
#[test]
fn node_keys_ignore_hidden_engine_handles() {
    let mut checked = 0u64;
    let mut swapped_draws = 0u64;
    for game in [3u64, 13, 23, 33, 43] {
        let mut root = None;
        let mut n = 0;
        each_decision(game, |s, d| {
            n += 1;
            if root.is_none() && n > 30 && d.legal_action_count >= 2 {
                root = Some(s.clone());
            }
        });
        let a0 = root.expect("a root");
        let d0 = play::decision(&a0).unwrap();
        let focal = play::acting(&d0);
        let mut pairs = Vec::new();
        {
            let st = a0.game_state();
            for owner in [focal, focal.opponent()] {
                let lib = &st.players[owner.index()].library;
                let known: Vec<u32> = st
                    .known_library_cards(focal, owner)
                    .iter()
                    .map(|k| k.position)
                    .collect();
                let unknown: Vec<usize> = (0..lib.len())
                    .filter(|i| !known.contains(&(*i as u32)))
                    .collect();
                'pair: for (x, &i) in unknown.iter().enumerate() {
                    for &j in &unknown[x + 1..] {
                        if st.objects.get(lib[i]).card_def == st.objects.get(lib[j]).card_def {
                            pairs.push((owner, i, j));
                            break 'pair;
                        }
                    }
                }
            }
        }
        let swaps = pairs.len();
        let b0 = a0.census_edited_clone_v1(|st| {
            for &(owner, i, j) in &pairs {
                st.players[owner.index()].library.swap(i, j);
            }
        });
        if swaps == 0 {
            continue;
        }
        swapped_draws += 1;
        let (mut a, mut b) = (a0.clone(), b0);
        let mut pa = (fixture(), fixture());
        let mut pb = (fixture(), fixture());
        for p in [&mut pa.0, &mut pa.1, &mut pb.0, &mut pb.1] {
            p.reset_sampling_v1([7, 8]);
        }
        let (mut ka, mut kb) = (([0u8; 32], b"root".to_vec()), ([0u8; 32], b"root".to_vec()));
        let mut meter = Meter::new(u64::MAX);
        let defs = spy_defs();
        let mut c = play::Counters::default();
        for _ in 0..400 {
            let (Some(da), Some(db)) = (play::decision(&a), play::decision(&b)) else {
                assert_eq!(play::terminal(&a, focal), play::terminal(&b, focal));
                break;
            };
            assert_eq!(play::acting(&da), play::acting(&db));
            let (ia, ib) = if play::acting(&da) == focal && da.legal_action_count >= 2 {
                let (ca, cb) = (tree::canon(&a, da).unwrap(), tree::canon(&b, db).unwrap());
                let key_a = tree::child_key(&ka.0, &ka.1, &ca);
                let key_b = tree::child_key(&kb.0, &kb.1, &cb);
                if ca.obs != cb.obs {
                    let (x, y) = (
                        String::from_utf8_lossy(&ca.obs),
                        String::from_utf8_lossy(&cb.obs),
                    );
                    let i = x
                        .bytes()
                        .zip(y.bytes())
                        .position(|(p, q)| p != q)
                        .unwrap_or(0);
                    let lo = i.saturating_sub(300);
                    panic!(
                        "observation bytes differ at {i}:
A ...{}
B ...{}",
                        &x[lo..(i + 200).min(x.len())],
                        &y[lo..(i + 200).min(y.len())]
                    );
                }
                assert_eq!(ca.edges, cb.edges, "edge sets differ");
                assert_eq!(key_a, key_b);
                checked += 1;
                let ia = play::act(&mut pa.0, &a, &da, &mut c).unwrap();
                let edge = ca.menu[ia as usize].clone();
                let ib = tree::live_index(&cb, &edge).unwrap();
                ka = (key_a, edge.clone());
                kb = (key_b, edge);
                (ia, ib)
            } else if da.legal_action_count >= 2 {
                // Opponent (or forced-free) choices bind by canonical action
                // from the actor's own view, never by live index.
                let actor_policy = if play::acting(&da) == focal {
                    &mut pa.0
                } else {
                    &mut pa.1
                };
                let ia = play::act(actor_policy, &a, &da, &mut c).unwrap();
                let (ca, cb) = (tree::canon(&a, da).unwrap(), tree::canon(&b, db).unwrap());
                assert_eq!(ca.edges, cb.edges, "actor edge sets differ");
                let ib = tree::live_index(&cb, &ca.menu[ia as usize]).unwrap();
                (ia, ib)
            } else {
                (0, 0)
            };
            play::apply(&mut a, &da, ia, &mut meter, focal, None, &defs).unwrap();
            play::apply(&mut b, &db, ib, &mut meter, focal, None, &defs).unwrap();
        }
    }
    eprintln!("handle-invariance: {checked} focal nodes over {swapped_draws} swapped games");
    assert!(
        checked > 20 && swapped_draws >= 2,
        "{checked} {swapped_draws}"
    );
}

/// Scripted Spy line on the real engine: the suffix labels record each
/// resolution in order; targeting the opponent instead never completes.
fn scripted_spy_line(self_target: bool) -> labels::Suffix {
    use crate::card_def::{CardType, CARD_DEFS};
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::rl::{ActionSemanticV1, PlayerSeatV1, TargetRefV1};
    use crate::state::Zone;
    let defs = spy_defs();
    let spy_deck = &RUNTIME_DECKS[spy_index()];
    let creatures: Vec<&str> = spy_deck
        .card_ids
        .iter()
        .map(|&c| &CARD_DEFS[c as usize])
        .filter(|c| {
            c.types.contains(&CardType::Creature)
                && !matches!(c.name, "Balustrade Spy" | "Lotleth Giant")
        })
        .map(|c| c.name)
        .collect();
    let mut st = ready_state();
    let me = PlayerId::P0;
    put(&mut st, me, "Balustrade Spy", Zone::Hand);
    st.players[0].mana_pool[crate::mana::ManaColor::B.pool_index()] = 4;
    put(&mut st, me, creatures[0], Zone::Battlefield);
    put(&mut st, me, creatures[0], Zone::Battlefield);
    for name in ["Dread Return", "Lotleth Giant", creatures[0], creatures[0]] {
        put(&mut st, me, name, Zone::Library);
    }
    for _ in 0..10 {
        put(&mut st, me.opponent(), "Swamp", Zone::Library);
    }
    let mut s = crate::rl_session::FastActorSessionV1::from_v3_fixture_state(st);
    let mut suffix = labels::Suffix::new(true);
    let mut meter = Meter::new(10_000);
    for _ in 0..300 {
        let Some(d) = play::decision(&s) else { break };
        let sem = s.diagnostic_current_action_semantics().unwrap();
        let pick = |pred: &dyn Fn(&ActionSemanticV1) -> bool| sem.iter().position(pred);
        let is_src = |src: &crate::rl::CardStableRefV1, def: u16| src.card_db_id == def;
        let a = if play::acting(&d) == me {
            pick(&|x| matches!(x, ActionSemanticV1::CastSpell { source, .. } if is_src(source, defs.spy) || is_src(source, defs.dread_return)))
                .or_else(|| {
                    pick(&|x| match x {
                        ActionSemanticV1::ChooseTarget { source, target, .. } if is_src(source, defs.spy) => {
                            matches!(target, TargetRefV1::Player { player } if (*player == PlayerSeatV1::P0) == self_target)
                        }
                        ActionSemanticV1::ChooseTarget { source, target, .. } if is_src(source, defs.dread_return) => {
                            matches!(target, TargetRefV1::Object { object } if object.card_db_id == defs.giant)
                        }
                        _ => false,
                    })
                })
                .or_else(|| pick(&|x| !matches!(x, ActionSemanticV1::Pass { .. } | ActionSemanticV1::CastSpell { .. } | ActionSemanticV1::ActivateAbility { .. } | ActionSemanticV1::PlayLand { .. })))
                .or_else(|| pick(&|x| matches!(x, ActionSemanticV1::Pass { .. })))
                .unwrap_or(0)
        } else {
            pick(&|x| matches!(x, ActionSemanticV1::Pass { .. })).unwrap_or(0)
        };
        if play::acting(&d) == me && d.legal_action_count >= 2 {
            play::observe(Some(&mut suffix), &s, a as u32, me, &defs);
        }
        play::apply(
            &mut s,
            &d,
            a as u32,
            &mut meter,
            me,
            Some(&mut suffix),
            &defs,
        )
        .unwrap();
        if suffix.complete() || s.game_state().turn > 1 {
            break;
        }
    }
    suffix
}

#[test]
fn suffix_labels_follow_a_scripted_spy_line() {
    let done = scripted_spy_line(true);
    assert!(done.spy_resolved, "{done:?}");
    assert!(done.self_target_resolved, "{done:?}");
    assert!(done.dr_giant_resolved, "{done:?}");
    assert!(done.complete(), "{done:?}");
    let other = scripted_spy_line(false);
    assert!(
        other.spy_resolved && !other.self_target_resolved,
        "{other:?}"
    );
    assert!(!other.complete(), "{other:?}");
}

/// The search-only fast forward scores like the ordinary one: logits and
/// values within 1e-4 and the same argmax at every fixture decision.
#[test]
fn fast_search_forward_matches_the_ordinary_forward_closely() {
    let mut plain = FrozenPlayPolicyV1::training_fixture_v3();
    let mut fast = FrozenPlayPolicyV1::training_fixture_v3();
    fast.enable_fast_search_forward_v1();
    let (mut worst, mut decisions) = (0f32, 0u64);
    for game in 0..2 {
        each_decision(game, |s, _| {
            let a = plain.score_fast_session_v1(s).unwrap();
            let b = fast.score_fast_session_v1(s).unwrap();
            assert_eq!(a.logits.len(), b.logits.len());
            for (x, y) in a.logits.iter().zip(&b.logits) {
                worst = worst.max((x - y).abs());
            }
            worst = worst.max((a.value - b.value).abs());
            let argmax = |v: &[f32]| (0..v.len()).max_by(|&i, &j| v[i].total_cmp(&v[j]));
            assert_eq!(argmax(&a.logits), argmax(&b.logits));
            decisions += 1;
        });
    }
    assert!(
        decisions > 100 && worst <= 1e-4,
        "{decisions} decisions, worst {worst:e}"
    );
}

#[test]
fn resume_refuses_mixed_forward_activation_modes() {
    let ordinary = json!({"kind":"s4a_root","root_id":"ordinary","config":{}}).to_string() + "\n";
    let fast = json!({"kind":"s4a_root","root_id":"fast","config":{"fast_search_forward":true}})
        .to_string()
        + "\n";
    assert!(completed_roots(&ordinary, false, "s4a_root")
        .unwrap()
        .contains("ordinary"));
    assert!(completed_roots(&fast, true, "s4a_root")
        .unwrap()
        .contains("fast"));
    assert!(completed_roots(&ordinary, true, "s4a_root").is_err());
    assert!(completed_roots(&fast, false, "s4a_root").is_err());
    assert!(completed_roots(&(ordinary.clone() + &fast), false, "s4a_root").is_err());
    assert!(completed_roots(&(ordinary + &fast), true, "s4a_root").is_err());
    assert!(completed_roots(fast.trim_end(), false, "s4a_root")
        .unwrap()
        .is_empty());
    let mut interrupted = fast.as_bytes().to_vec();
    interrupted.extend_from_slice(&[0xe2, 0x82]);
    assert!(completed_roots_from_bytes(&interrupted, false, "s4a_root", None, None).is_err());
    assert!(
        completed_roots_from_bytes(&interrupted, true, "s4a_root", None, None)
            .unwrap()
            .contains("fast")
    );
    assert!(completed_roots_from_bytes(&[0xff, b'\n'], false, "s4a_root", None, None).is_err());
}

#[test]
fn diagnostic_resume_preserves_row_kind_and_ordinary_activation() {
    let diag =
        json!({"kind":"s4a_diag_root","root_id":"diagnostic","config":{}}).to_string() + "\n";
    assert!(
        completed_roots_from_bytes(diag.as_bytes(), false, "s4a_diag_root", None, None)
            .unwrap()
            .contains("diagnostic")
    );
    assert!(
        completed_roots_from_bytes(diag.as_bytes(), false, "s4a_root", None, None)
            .unwrap()
            .is_empty()
    );
    let mixed =
        json!({"kind":"s4a_diag_root","root_id":"invalid","config":{"fast_search_forward":true}})
            .to_string()
            + "\n";
    assert!(
        completed_roots_from_bytes(mixed.as_bytes(), false, "s4a_diag_root", None, None).is_err()
    );
    assert!(validate_fast_forward_mode("s4a-diag", true).is_err());
    assert!(validate_fast_forward_mode("s4a-diag", false).is_ok());
}

#[test]
fn resume_binds_runtime_selection_and_forward_mode_together() {
    use crate::engine::RuntimeRulesV1;
    let runtime = Some(RuntimeRulesV1::RESOLUTION_BOUNDARY_V1);
    let row = json!({"kind":"s4a_diag_root","root_id":"retained",
        "runtime_rules":"resolution-boundary-v1","select_rule":"fpu-1.5","config":{}});
    let mut bytes = (row.to_string() + "\n").into_bytes();
    // Disposable interrupted UTF8 must not hide any identity mismatch.
    bytes.extend_from_slice(&[0xe2, 0x82]);
    assert!(
        completed_roots_from_bytes(&bytes, false, "s4a_diag_root", runtime, Some("fpu-1.5"))
            .unwrap()
            .contains("retained")
    );
    for (fast, rules, select) in [
        (true, runtime, Some("fpu-1.5")),
        (false, None, Some("fpu-1.5")),
        (false, runtime, None),
    ] {
        assert!(completed_roots_from_bytes(&bytes, fast, "s4a_diag_root", rules, select).is_err());
    }
    let malformed = b"{\"kind\":\"s4a_diag_root\",\"root_id\":\"retained\"}\ninvalid\n";
    assert!(completed_roots_from_bytes(malformed, false, "s4a_diag_root", None, None).is_err());
}

#[test]
fn corpus_cannot_enable_unrecorded_fast_forward() {
    assert!(validate_fast_forward_mode("s4a-corpus", true).is_err());
    assert!(validate_fast_forward_mode("s4a-corpus", false).is_ok());
    assert!(validate_fast_forward_mode("s4a-run", true).is_ok());
}

#[test]
fn replay_roles_restore_ordinary_scoring_after_each_fast_root() {
    let cfg = test_cfg();
    let setup = game_setup(&cfg, 1, 0);
    let session = new_session(&setup).unwrap();
    let mut reference = fixture();
    let expected = reference.score_fast_session_v1(&session).unwrap();
    let mut roles = Roles {
        focal: fixture(),
        opps: vec![fixture()],
        scorer: fixture(),
        inner_focal: fixture(),
        inner_opps: vec![fixture()],
    };
    for _ in 0..2 {
        set_replay_forward(&mut roles, true);
        set_replay_forward(&mut roles, false);
        for policy in std::iter::once(&mut roles.focal).chain(roles.opps.iter_mut()) {
            let actual = policy.score_fast_session_v1(&session).unwrap();
            assert_eq!(
                actual
                    .logits
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .logits
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
            assert_eq!(actual.value.to_bits(), expected.value.to_bits());
        }
    }
}

/// Per-call cost of policy scoring, ordinary vs fast search forward
/// (manual: --ignored --nocapture).
#[test]
#[ignore]
fn profile_policy_scoring_phases() {
    for fast in [false, true] {
        let mut p = FrozenPlayPolicyV1::training_fixture_v3();
        if fast {
            p.enable_fast_search_forward_v1();
        }
        let mut t = [0f64; 8];
        let mut n = 0u64;
        let mut worst = 0f32;
        let mut reference = FrozenPlayPolicyV1::training_fixture_v3();
        for game in 0..6 {
            each_decision(game, |s, _| {
                let x = p.profile_score_phases_v3(s);
                for i in 0..8 {
                    t[i] += x[i];
                }
                let (a, b) = (
                    p.score_fast_session_v1(s).unwrap(),
                    reference.score_fast_session_v1(s).unwrap(),
                );
                for (x, y) in a.logits.iter().zip(&b.logits) {
                    worst = worst.max((x - y).abs());
                }
                n += 1;
            });
        }
        let m = |i: usize| t[i] * 1e3 / n as f64;
        let r = |i: usize| t[i] / n as f64;
        eprintln!(
            "fast={fast} decisions={n} per-call ms: encode={:.4} tensorize={:.4} forward={:.4} forward_hot={:.4} total={:.4}; rows objects={:.1} edges={:.1} action_refs={:.1} actions={:.1}; max logit diff {worst:e}",
            m(0), m(1), m(2), m(3), m(0) + m(1) + m(2), r(4), r(5), r(6), r(7)
        );
    }
}

/// The passive diagnostic trace leaves E's selection and frozen execution
/// unchanged, reconstructs every node's terminal backups from the ordered
/// simulation records, and classifies every focal non-forced execution
/// decision consistently with E's own counters (a first miss ends matching).
#[test]
fn diagnostic_trace_is_passive_and_reconstructs_backups() {
    let cfg = test_cfg();
    let spy = spy_index();
    let game = spy as u64 + 9 * 3;
    let setup = game_setup(&cfg, 1, game);
    let focal_id = PlayerId(setup.focal as u8);
    let (mut a, mut b) = (fixture(), fixture());
    let mut root = None;
    drive(&setup, &mut a, &mut b, |s, d, _| {
        if play::acting(d) == focal_id && d.legal_action_count >= 3 && s.game_state().turn >= 3 {
            root = Some(s.clone());
            return Ok(true);
        }
        Ok(false)
    })
    .unwrap();
    let session = root.expect("a fixture root");
    let prior = world::DeckPrior::new(&cfg.decks);
    let seeds = RootSeeds {
        model: "fixture".into(),
        root: "fixture-root".into(),
    };
    let run = |traced: bool| {
        let mut roles = Roles {
            focal: fixture(),
            opps: vec![fixture()],
            scorer: fixture(),
            inner_focal: fixture(),
            inner_opps: vec![fixture()],
        };
        let d = play::decision(&session).unwrap();
        let probs = arms::softmax(&roles.scorer.score_fast_session_v1(&session).unwrap().logits);
        let ctx = RootCtx {
            root: &session,
            d,
            focal: focal_id,
            opp: 0,
            seeds: &seeds,
            prior: &prior,
            defs: spy_defs(),
            cast_root: false,
            probs,
            limits: Limits {
                select_cap: 6_000,
                eval_worlds: 3,
                eval_cap: 4_000,
            },
            urgency: None,
        };
        let mut trace = traced.then(diag::Trace::default);
        let (e, tree) = roles.select_e_traced(&ctx, trace.as_mut());
        let mut st = arms::SamplerStats::default();
        let mut outs = Vec::new();
        for w in 0..3u64 {
            let world = world::sample(&session, 99 + w, &prior).unwrap();
            if let Some(t) = trace.as_mut() {
                t.world_begin(w);
            }
            let o = roles.evaluate_traced(
                &ctx,
                "E",
                &world.world,
                w,
                None,
                Some(&tree),
                &mut st,
                trace.as_mut(),
            );
            if let Some(t) = trace.as_mut() {
                t.world_end(o.json());
            }
            outs.push(o.json().to_string());
        }
        ((tree.hash(), e.json().to_string(), outs), trace)
    };
    let (plain, none) = run(false);
    let (traced, trace) = run(true);
    assert!(none.is_none());
    assert_eq!(plain, traced);
    let lines: Vec<serde_json::Value> = trace
        .unwrap()
        .lines
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // Backups reconstructed from the ordered simulation records.
    let mut n: BTreeMap<(u64, u64), (u64, u64)> = BTreeMap::new();
    for l in lines.iter().filter(|l| l["r"] == "sel") {
        let win = match l["end"].as_str().unwrap() {
            "win" => 1,
            "loss" => 0,
            _ => continue,
        };
        for p in l["path"].as_array().unwrap() {
            let x = n
                .entry((p[0].as_u64().unwrap(), p[1].as_u64().unwrap()))
                .or_default();
            x.0 += 1;
            x.1 += win;
        }
    }
    let nodes: Vec<&serde_json::Value> = lines.iter().filter(|l| l["r"] == "node").collect();
    assert!(!nodes.is_empty());
    for node in &nodes {
        assert!(node.get("error").is_none(), "{node}");
        let id = node["id"].as_u64().unwrap();
        for (e, (c, w)) in node["n"]
            .as_array()
            .unwrap()
            .iter()
            .zip(node["w"].as_array().unwrap())
            .enumerate()
        {
            let got = n.get(&(id, e as u64)).copied().unwrap_or_default();
            assert_eq!(
                got,
                (c.as_u64().unwrap(), w.as_u64().unwrap()),
                "node {id} edge {e}"
            );
        }
    }
    // Execution sources agree with E's counters; a miss ends matching.
    for world in lines.iter().filter(|l| l["r"] == "eval") {
        let e = &world["out"]["e"];
        let decs = world["dec"].as_array().unwrap();
        let count =
            |f: &dyn Fn(&serde_json::Value) -> bool| decs.iter().filter(|d| f(d)).count() as u64;
        let looked = |d: &serde_json::Value| {
            matches!(
                d["src"].as_str().unwrap(),
                "tree_edge" | "matched_no_qualified_plain" | "first_miss_plain"
            )
        };
        let hit = |d: &serde_json::Value| {
            matches!(
                d["src"].as_str().unwrap(),
                "tree_edge" | "matched_no_qualified_plain"
            )
        };
        assert_eq!(
            count(&|d| d["root"] == false && looked(d)),
            e["nonroot_lookups"].as_u64().unwrap()
        );
        assert_eq!(
            count(&|d| d["root"] == false && hit(d)),
            e["nonroot_hits"].as_u64().unwrap()
        );
        assert_eq!(
            count(&|d| d["src"] == "matched_no_qualified_plain"),
            e["fallbacks"].as_u64().unwrap()
        );
        assert_eq!(
            count(&|d| d["src"] == "tree_edge"),
            e["executed"].as_array().unwrap().len() as u64
        );
        let miss = decs.iter().position(|d| d["src"] == "first_miss_plain");
        for (i, d) in decs.iter().enumerate() {
            assert_eq!(d["mb"] == true, miss.is_none_or(|m| i <= m), "{d}");
            if miss.is_some_and(|m| i > m) {
                assert_eq!(d["src"], "after_miss_plain");
                assert!(d["node"].is_null());
            }
            if hit(d) {
                assert!(d["node"].is_u64() && d["edge"].is_u64(), "{d}");
            }
        }
        assert_eq!(decs[0]["root"], true);
    }
}

/// Resolution-boundary profile (engine::RuntimeRulesV1): plays a scripted
/// cast of `spell` by P0 (self-targeting, choosing the first option at every
/// other P0 decision, passing for P1) until the stack is empty again, and
/// returns every P0 multi-option decision's semantics plus the final state.
fn scripted_cast(
    rules: crate::engine::RuntimeRulesV1,
    spell: &str,
    library: &[&str],
) -> (
    Vec<Vec<crate::rl::ActionSemanticV1>>,
    crate::state::GameState,
) {
    scripted_cast_with(rules, spell, library, &[], false).0
}

/// `scripted_cast` with extra hand cards; with `stop_at_suspension`, also
/// returns the state at the first suspended effect after the cast.
fn scripted_cast_with(
    rules: crate::engine::RuntimeRulesV1,
    spell: &str,
    library: &[&str],
    hand: &[&str],
    stop_at_suspension: bool,
) -> (
    (
        Vec<Vec<crate::rl::ActionSemanticV1>>,
        crate::state::GameState,
    ),
    Option<crate::state::GameState>,
) {
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::rl::{ActionSemanticV1, PlayerSeatV1, TargetRefV1};
    use crate::state::Zone;
    let mut st = ready_state();
    let me = PlayerId::P0;
    put(&mut st, me, spell, Zone::Hand);
    for name in hand {
        put(&mut st, me, name, Zone::Hand);
    }
    for c in [
        crate::mana::ManaColor::B,
        crate::mana::ManaColor::U,
        crate::mana::ManaColor::G,
    ] {
        st.players[0].mana_pool[c.pool_index()] = 4;
    }
    for name in library {
        put(&mut st, me, name, Zone::Library);
    }
    for _ in 0..10 {
        put(&mut st, me.opponent(), "Swamp", Zone::Library);
    }
    let mut s = crate::rl_session::FastActorSessionV1::from_v3_fixture_state(st);
    s.set_runtime_rules_v1(rules).unwrap();
    let spell_id = crate::card_def::card_id_by_name(spell).unwrap();
    let mut menus = Vec::new();
    let mut cast = false;
    let mut suspended = None;
    for _ in 0..400 {
        let Some(d) = play::decision(&s) else { break };
        let g = s.game_state();
        if stop_at_suspension && cast && suspended.is_none() && g.engine.pending_effect.is_some() {
            suspended = Some(g.clone());
        }
        if cast
            && g.stack.is_empty()
            && g.engine.pending_effect.is_none()
            && g.engine.pending_triggers.is_empty()
            && g.engine.pending_cast.is_none()
        {
            break;
        }
        let sem = s.diagnostic_current_action_semantics().unwrap();
        let a = if play::acting(&d) == me {
            if d.legal_action_count >= 2 {
                menus.push(sem.clone());
            }
            let pos = |f: &dyn Fn(&ActionSemanticV1) -> bool| sem.iter().position(f);
            pos(&|x| {
                !cast
                    && matches!(x, ActionSemanticV1::CastSpell { source, .. } if source.card_db_id == spell_id)
            })
            .or_else(|| {
                pos(&|x| {
                    matches!(x, ActionSemanticV1::ChooseTarget { target: TargetRefV1::Player { player }, .. } if *player == PlayerSeatV1::P0)
                })
            })
            .or_else(|| {
                pos(&|x| {
                    cast && !matches!(
                        x,
                        ActionSemanticV1::Pass { .. }
                            | ActionSemanticV1::CastSpell { .. }
                            | ActionSemanticV1::ActivateAbility { .. }
                            | ActionSemanticV1::PlayLand { .. }
                    )
                })
            })
            .or_else(|| pos(&|x| matches!(x, ActionSemanticV1::Pass { .. })))
            .unwrap_or(0)
        } else {
            sem.iter()
                .position(|x| matches!(x, ActionSemanticV1::Pass { .. }))
                .unwrap_or(0)
        };
        if matches!(&sem[a], ActionSemanticV1::CastSpell { source, .. } if source.card_db_id == spell_id)
        {
            cast = true;
        }
        s.step(d.episode_id, d.step, a as u32).unwrap();
    }
    assert!(cast, "{spell} was never cast");
    ((menus, s.game_state().clone()), suspended)
}

fn order_menus(menus: &[Vec<crate::rl::ActionSemanticV1>]) -> usize {
    menus
        .iter()
        .filter(|m| {
            m.iter()
                .all(|x| matches!(x, crate::rl::ActionSemanticV1::ChooseEffectTarget { .. }))
        })
        .count()
}

fn zone_names(
    st: &crate::state::GameState,
    ids: &[crate::ids::ObjectId],
    sorted: bool,
) -> Vec<String> {
    let mut v: Vec<String> = ids
        .iter()
        .map(|&i| st.objects.get(i).name.to_string())
        .collect();
    if sorted {
        v.sort();
    }
    v
}

#[test]
fn resolution_boundary_mills_a_landless_library_without_ordering_choices() {
    use crate::engine::RuntimeRulesV1;
    let lib = [
        "Dread Return",
        "Lotleth Giant",
        "Balustrade Spy",
        "Dread Return",
    ];
    let (old_menus, old) = scripted_cast(RuntimeRulesV1::default(), "Balustrade Spy", &lib);
    let (new_menus, new) = scripted_cast(
        RuntimeRulesV1::RESOLUTION_BOUNDARY_V1,
        "Balustrade Spy",
        &lib,
    );
    // The historical engine asks the owner to order the milled cards.
    assert!(order_menus(&old_menus) >= 3, "{old_menus:?}");
    assert_eq!(order_menus(&new_menus), 0, "{new_menus:?}");
    // Exactly-one-of-many targeting stays a real choice.
    let targets = |m: &[Vec<crate::rl::ActionSemanticV1>]| {
        m.iter().any(|x| {
            x.len() >= 2
                && x.iter()
                    .all(|y| matches!(y, crate::rl::ActionSemanticV1::ChooseTarget { .. }))
        })
    };
    assert!(targets(&old_menus) && targets(&new_menus));
    // Same legal outcome: the whole library is in the graveyard either way;
    // the profile keeps the bound (library) order.
    for st in [&old, &new] {
        assert!(st.players[0].library.is_empty());
    }
    assert_eq!(
        zone_names(&old, &old.players[0].graveyard, true),
        zone_names(&new, &new.players[0].graveyard, true)
    );
    assert_eq!(old.players[1].graveyard, new.players[1].graveyard);
}

#[test]
fn resolution_boundary_stops_at_the_first_land_like_the_historical_engine() {
    use crate::engine::RuntimeRulesV1;
    let lib = [
        "Lotleth Giant",
        "Swamp",
        "Dread Return",
        "Balustrade Spy",
        "Dread Return",
    ];
    let (_, old) = scripted_cast(RuntimeRulesV1::default(), "Balustrade Spy", &lib);
    let (new_menus, new) = scripted_cast(
        RuntimeRulesV1::RESOLUTION_BOUNDARY_V1,
        "Balustrade Spy",
        &lib,
    );
    assert_eq!(order_menus(&new_menus), 0, "{new_menus:?}");
    assert!(
        !new.players[0].library.is_empty(),
        "the land stops the reveal"
    );
    assert!(
        new.players[0].graveyard.len() >= 2,
        "the prefix through the land is milled"
    );
    assert_eq!(
        zone_names(&old, &old.players[0].library, false),
        zone_names(&new, &new.players[0].library, false)
    );
    assert_eq!(
        zone_names(&old, &old.players[0].graveyard, true),
        zone_names(&new, &new.players[0].graveyard, true)
    );
    // The reveal stays public under the profile.
    assert_eq!(old.library_knowledge, new.library_knowledge);
}

#[test]
fn resolution_boundary_keeps_ordered_library_placement() {
    use crate::engine::RuntimeRulesV1;
    let lib = ["Island", "Swamp", "Lotleth Giant", "Dread Return"];
    let (menus, _) = scripted_cast(RuntimeRulesV1::RESOLUTION_BOUNDARY_V1, "Ponder", &lib);
    assert!(
        order_menus(&menus) >= 1,
        "Ponder's top-of-library order must stay a choice: {menus:?}"
    );
}

#[test]
fn resolution_boundary_profile_survives_world_sampling_and_is_hash_neutral() {
    use crate::engine::RuntimeRulesV1;
    let cfg = test_cfg();
    let spy = spy_index();
    let setup = game_setup(&cfg, 1, spy as u64 + 9 * 3);
    let focal_id = PlayerId(setup.focal as u8);
    let (mut a, mut b) = (fixture(), fixture());
    let mut root = None;
    drive(&setup, &mut a, &mut b, |s, d, _| {
        if play::acting(d) == focal_id && d.legal_action_count >= 3 && s.game_state().turn >= 3 {
            root = Some(s.clone());
            return Ok(true);
        }
        Ok(false)
    })
    .unwrap();
    let mut s = root.unwrap();
    let h = |st: &crate::state::GameState| (st.state_hash(), st.diagnostic_state_hash());
    let json = |st: &crate::state::GameState| serde_json::to_string(st).unwrap();
    let before = (h(s.game_state()), json(s.game_state()));
    s.set_runtime_rules_v1(RuntimeRulesV1::default()).unwrap();
    assert_eq!(before, (h(s.game_state()), json(s.game_state())));
    s.set_runtime_rules_v1(RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
        .unwrap();
    // Hash-neutral for the runtime state hash; the audit hash and snapshot
    // record the profile once it is on.
    assert_eq!(before.0 .0, h(s.game_state()).0);
    assert_ne!(before.1, json(s.game_state()));
    let prior = world::DeckPrior::new(&cfg.decks);
    let w = world::sample(&s, 7, &prior).unwrap();
    assert_eq!(
        w.world.game_state().engine.runtime_rules,
        RuntimeRulesV1::RESOLUTION_BOUNDARY_V1
    );
}

/// Zone-change and trigger events of a scripted run, order-free (the
/// profile may only change the order in which a batch is committed).
fn event_multiset(st: &crate::state::GameState) -> Vec<String> {
    let mut v: Vec<String> = st
        .engine
        .event_history
        .iter()
        .map(|e| format!("{e:?}"))
        .collect();
    v.sort();
    v
}

#[test]
fn resolution_boundary_preserves_events_and_bound_order_for_spy() {
    use crate::engine::RuntimeRulesV1;
    let lib = [
        "Dread Return",
        "Lotleth Giant",
        "Balustrade Spy",
        "Dread Return",
    ];
    let (_, old) = scripted_cast(RuntimeRulesV1::default(), "Balustrade Spy", &lib);
    let (_, new) = scripted_cast(
        RuntimeRulesV1::RESOLUTION_BOUNDARY_V1,
        "Balustrade Spy",
        &lib,
    );
    assert_eq!(event_multiset(&old), event_multiset(&new));
    assert_eq!(
        old.engine.pending_triggers.len(),
        new.engine.pending_triggers.len()
    );
    // The profile keeps the order the effect bound: library order, which the
    // fixture pushed bottom to top, so the graveyard reads it either way.
    let gy = zone_names(&new, &new.players[0].graveyard, false);
    let milled: Vec<String> = gy
        .iter()
        .filter(|n| lib.contains(&n.as_str()))
        .cloned()
        .collect();
    let fwd: Vec<String> = lib.iter().map(|s| s.to_string()).collect();
    let rev: Vec<String> = fwd.iter().rev().cloned().collect();
    // Library index 0 is the top; the batch keeps that bound order.
    let _ = rev;
    assert_eq!(milled, fwd, "{gy:?}");
}

#[test]
fn resolution_boundary_covers_plain_mills() {
    use crate::engine::RuntimeRulesV1;
    let lib = ["Lotleth Giant", "Dread Return", "Swamp", "Balustrade Spy"];
    let (old_menus, old) = scripted_cast(RuntimeRulesV1::default(), "Thought Scour", &lib);
    let (new_menus, new) = scripted_cast(
        RuntimeRulesV1::RESOLUTION_BOUNDARY_V1,
        "Thought Scour",
        &lib,
    );
    assert!(order_menus(&old_menus) >= 1, "{old_menus:?}");
    assert_eq!(order_menus(&new_menus), 0, "{new_menus:?}");
    assert_eq!(
        zone_names(&old, &old.players[0].graveyard, true),
        zone_names(&new, &new.players[0].graveyard, true)
    );
    assert_eq!(
        zone_names(&old, &old.players[0].hand, true),
        zone_names(&new, &new.players[0].hand, true)
    );
    assert_eq!(event_multiset(&old), event_multiset(&new));
}

#[test]
fn resolution_boundary_covers_reveal_and_partition() {
    use crate::engine::RuntimeRulesV1;
    let lib = ["Lotleth Giant", "Dread Return", "Masked Vandal", "Swamp"];
    let ((old_menus, old), _) =
        scripted_cast_with(RuntimeRulesV1::default(), "Winding Way", &lib, &[], false);
    let ((new_menus, new), _) = scripted_cast_with(
        RuntimeRulesV1::RESOLUTION_BOUNDARY_V1,
        "Winding Way",
        &lib,
        &[],
        false,
    );
    assert!(order_menus(&old_menus) >= 1, "{old_menus:?}");
    assert_eq!(order_menus(&new_menus), 0, "{new_menus:?}");
    assert_eq!(
        zone_names(&old, &old.players[0].graveyard, true),
        zone_names(&new, &new.players[0].graveyard, true)
    );
    assert_eq!(
        zone_names(&old, &old.players[0].hand, true),
        zone_names(&new, &new.players[0].hand, true)
    );
    assert_eq!(event_multiset(&old), event_multiset(&new));
    assert_eq!(old.library_knowledge, new.library_knowledge);
}

/// A graveyard batch whose binding no longer exists is an error, never a
/// panic, under either profile.
#[test]
fn resolution_boundary_rejects_a_missing_binding_without_panicking() {
    use crate::effect::{EffectFrame, EffectObjectBinding};
    use crate::engine::RuntimeRulesV1;
    use crate::state::Zone;
    let lib = ["Lotleth Giant", "Dread Return", "Masked Vandal", "Swamp"];
    for rules in [
        RuntimeRulesV1::default(),
        RuntimeRulesV1::RESOLUTION_BOUNDARY_V1,
    ] {
        let (_, suspended) = scripted_cast_with(rules, "Winding Way", &lib, &[], true);
        let mut st = suspended.expect("Winding Way suspends on its type choice");
        let mut cont = st.engine.pending_effect.take().unwrap();
        let top = st.players[0].library[0];
        cont.frames.push(EffectFrame::MoveObjectsBatch {
            objects: vec![
                EffectObjectBinding {
                    object: crate::ids::ObjectId(60_000),
                    expected_zone: Zone::Library,
                    expected_zone_change_count: 0,
                },
                EffectObjectBinding {
                    object: top,
                    expected_zone: Zone::Library,
                    expected_zone_change_count: st.objects.get(top).zone_change_count,
                },
            ],
            to_zone: Zone::Graveyard,
            preserve_known_identity: false,
            order_resolved: false,
            path: Vec::new(),
        });
        cont.choice = None;
        cont.answered_choice_guard = None;
        st.engine.pending_effect = Some(cont);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::effect::resume_resumable_resolution(&mut st)
        }));
        assert!(matches!(r, Ok(Err(_))), "{rules:?}");
    }
}

/// The profile is a game-level rule: refused whenever any card in the game,
/// in any zone and for either player, reads graveyard order (Delve), so the
/// ordering stage's presence never depends on hidden cards. Without the
/// profile the Delve owner is still asked.
#[test]
fn resolution_boundary_is_refused_when_any_card_reads_graveyard_order() {
    use crate::engine::RuntimeRulesV1;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::Zone;
    let me = PlayerId::P0;
    for (owner, zone) in [
        (me, Zone::Library),
        (me, Zone::Hand),
        (me.opponent(), Zone::Library),
    ] {
        let mut st = ready_state();
        put(&mut st, me, "Balustrade Spy", Zone::Hand);
        put(&mut st, owner, "Gurmag Angler", zone);
        put(&mut st, me.opponent(), "Swamp", Zone::Library);
        let mut s = crate::rl_session::FastActorSessionV1::from_v3_fixture_state(st);
        assert!(s
            .set_runtime_rules_v1(RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
            .is_err());
        assert!(s.set_runtime_rules_v1(RuntimeRulesV1::default()).is_ok());
    }
    let lib = [
        "Dread Return",
        "Lotleth Giant",
        "Gurmag Angler",
        "Dread Return",
    ];
    let (menus, _) = scripted_cast(RuntimeRulesV1::default(), "Balustrade Spy", &lib);
    assert!(order_menus(&menus) >= 3, "{menus:?}");
}
