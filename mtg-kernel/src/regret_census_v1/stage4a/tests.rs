use super::arms::{Limits, Roles, RootCtx};
use super::play::{Meter, PlayErr};
use super::*;
use crate::ids::PlayerId;
use std::collections::BTreeMap;

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
