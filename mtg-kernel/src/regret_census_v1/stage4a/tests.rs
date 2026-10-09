use super::arms::{Limits, RootCtx, Roles};
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
            if n % 3 != 0 {
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
                    assert_eq!(tree::canon(s, *d).unwrap(), tree::canon(&w.world, wd).unwrap());
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
    assert_eq!(library, library_ok, "library-search samples rejected: {kinds:?}");
    assert!(decks_seen.len() >= 3, "prior never left the true deck: {decks_seen:?}");
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
            && st.players[opp.index()].graveyard.len() + st.players[opp.index()].battlefield.len() >= 6
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
