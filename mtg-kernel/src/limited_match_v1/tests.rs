use super::*;
use crate::card_def::card_id_by_name;

const UG: &str = include_str!("../../../data/limited/fdn_v1/FDN_top_04956_UG.dck");
const WG: &str = include_str!("../../../data/limited/fdn_v1/FDN_top_20626_WG.dck");

/// Mainboard ids of an XMage .dck fixture, in row and copy order.
fn dck(text: &str) -> Vec<u16> {
    let rows: Vec<CustomCardCountV1> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("NAME:") && !l.starts_with("SB:"))
        .map(|line| {
            let (count, rest) = line.split_once(' ').expect("count and name");
            let name = match rest.strip_prefix('[') {
                Some(r) => r.split_once("] ").expect("set annotation").1,
                None => rest,
            };
            CustomCardCountV1 {
                name: name.to_string(),
                count: count.parse().expect("count"),
            }
        })
        .collect();
    DeckFileV1 {
        name: None,
        cards: rows,
    }
    .resolve()
    .expect("fixture decks are Full-only")
}

fn sorted(mut cards: Vec<u16>) -> Vec<u16> {
    cards.sort_unstable();
    cards
}

fn new_game(seed: u64) -> FastActorSessionV1 {
    let (ug, wg) = (dck(UG), dck(WG));
    FastActorSessionV1::reset_prepared_v57(
        seed,
        seed,
        (4_000, 1 << 20),
        ["ug".to_string(), "wg".to_string()],
        [ug, wg],
        PlayerId::P0,
    )
    .expect("fixture reset")
}

/// Random play until P0 acts while P1 has an unknown hand card and at least
/// one card on the battlefield (an observed card).
fn midgame(seed: u64) -> FastActorSessionV1 {
    let mut session = new_game(seed);
    let mut rng = SplitMix64::seed(seed ^ 0x5eed);
    for _ in 0..4_000 {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("fixture game ended before a midgame decision");
        };
        let state = session.game_state();
        if seat(d.acting_player) == PlayerId::P0
            && !unknown_slots_v1(state, PlayerId::P0, PlayerId::P1, &[]).is_empty()
            && !state.players[1].hand.is_empty()
            && !state.players[1].battlefield.is_empty()
            && d.legal_action_count > 1
        {
            return session;
        }
        let pick = random_index(&mut rng, d.legal_action_count);
        session
            .step(d.episode_id, d.step, pick)
            .expect("random step");
    }
    panic!("no midgame decision within the step bound");
}

/// The same session with P1's unknown hand/library identities replaced: the
/// public evidence is identical, the hidden composition is not.
fn hidden_variant(session: &FastActorSessionV1) -> FastActorSessionV1 {
    let island = card_id_by_name("Island").expect("Island");
    let slots = unknown_slots_v1(session.game_state(), PlayerId::P0, PlayerId::P1, &[]);
    let variant = session.census_edited_clone_v1(|state| {
        for &id in &slots {
            let object = state.objects.get_mut(id);
            object.card_def = island;
            object.name = CARD_DEFS[island as usize].name.to_string();
            object.v4 = ObjectStateV4::from_card_def(island);
        }
    });
    let defs = |s: &FastActorSessionV1| -> Vec<u16> {
        slots
            .iter()
            .map(|&id| s.game_state().objects.get(id).card_def)
            .collect()
    };
    assert_ne!(
        defs(session),
        defs(&variant),
        "the variant must change hidden cards"
    );
    variant
}

fn slot_defs(world: &FastActorSessionV1, actor: PlayerId, owner: PlayerId) -> Vec<u16> {
    let slots = unknown_slots_v1(world.game_state(), actor, owner, &[]);
    slots
        .iter()
        .map(|&id| world.game_state().objects.get(id).card_def)
        .collect()
}

#[test]
fn policy_specs_parse_and_refuse() {
    assert_eq!(PolicySpecV1::parse("random"), Ok(PolicySpecV1::Random));
    assert_eq!(
        PolicySpecV1::parse("pimc:sims=8:depth=12:closedlist"),
        Ok(PolicySpecV1::Pimc {
            sims: 8,
            depth: 12,
            world: WorldModeV1::ClosedList
        })
    );
    assert!(
        PolicySpecV1::parse("pimc:sims=8").is_err(),
        "a world mode is required"
    );
    assert!(PolicySpecV1::parse("pimc:closedlist:peek").is_err());
    assert!(PolicySpecV1::parse("pimc:sims=0:peek").is_err());
    assert!(PolicySpecV1::parse("random:x").is_err());
    assert!(PolicySpecV1::parse("az:sims=100").is_err());
}

#[test]
fn deals_are_deterministic_distinct_and_shared_by_both_legs() {
    for pair in 0..50 {
        let deal = deal_pair_v1(7, pair, 5).unwrap();
        assert_eq!(deal, deal_pair_v1(7, pair, 5).unwrap());
        assert_ne!(deal.0[0], deal.0[1]);
        assert!(deal.0.iter().all(|&d| d < 5));
    }
    assert_ne!(
        deal_pair_v1(7, 0, 50).unwrap(),
        deal_pair_v1(8, 0, 50).unwrap()
    );
    assert!(deal_pair_v1(7, 0, 1).is_err());
}

#[test]
fn multiset_helpers() {
    assert!(is_submultiset(&[1, 2, 2], &[1, 2, 2, 3]));
    assert!(!is_submultiset(&[2, 2, 2], &[1, 2, 2, 3]));
    assert_eq!(remainder(&[1, 2, 2, 3], &[2, 3, 4]), (vec![1, 2], 2));
}

/// Milestone 6 acceptance: with identical public evidence, a different hidden
/// opponent composition cannot change the fair searcher's sampled worlds or
/// its decision, while the decklist-leaking modes do see the difference.
#[test]
fn closedlist_worlds_and_decisions_ignore_hidden_opponent_composition() {
    let session = midgame(31);
    let variant = hidden_variant(&session);
    let belief = LimitedBeliefV1::new([dck(UG), dck(WG)]);
    for seed in 0..8 {
        let a = sample_world_v1(&session, WorldModeV1::ClosedList, Some(&belief), seed).unwrap();
        let b = sample_world_v1(&variant, WorldModeV1::ClosedList, Some(&belief), seed).unwrap();
        assert_eq!(
            slot_defs(&a, PlayerId::P0, PlayerId::P1),
            slot_defs(&b, PlayerId::P0, PlayerId::P1),
            "seed {seed}: opponent world"
        );
        assert_eq!(
            slot_defs(&a, PlayerId::P0, PlayerId::P0),
            slot_defs(&b, PlayerId::P0, PlayerId::P0),
            "seed {seed}: own library world"
        );
        assert_eq!(
            a.game_state().players[1].hand.len(),
            b.game_state().players[1].hand.len()
        );
        // Every other field too: nothing of the true hidden cards survives.
        assert_eq!(a.game_state(), b.game_state(), "seed {seed}: whole world");
        assert_eq!(a.diagnostic_state_hash(), b.diagnostic_state_hash());
    }
    let leaks = (0..8).any(|seed| {
        let a = sample_world_v1(&session, WorldModeV1::TrueList, None, seed).unwrap();
        let b = sample_world_v1(&variant, WorldModeV1::TrueList, None, seed).unwrap();
        slot_defs(&a, PlayerId::P0, PlayerId::P1) != slot_defs(&b, PlayerId::P0, PlayerId::P1)
    });
    assert!(
        leaks,
        "truelist must see the hidden difference, or the test is blind"
    );

    let fair = |s: &FastActorSessionV1| {
        pimc_choose_v1(s, 12, 16, WorldModeV1::ClosedList, Some(&belief), 99).unwrap()
    };
    let (choice, stats) = fair(&session);
    assert_eq!((choice, stats.clone()), fair(&variant));
    assert_eq!((choice, stats.clone()), fair(&session), "deterministic");
    assert_eq!(stats.rollout_errors, 0, "{:?}", stats.first_rollout_error);
    assert_eq!(stats.visits.iter().sum::<u32>() + stats.rejected, 12);
}

/// A frozen trigger source shuffled into the actor's own library: the census
/// clone rejects only when the redraw happens to change that card, which leaks
/// the hidden identity. The Limited world rejects for every seed, whatever the
/// hidden card is.
#[test]
fn hidden_source_rejection_does_not_depend_on_hidden_identity() {
    let (mut state, source, _, _) =
        crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
    crate::rl_session::shuffle_trigger_source_into_library_v1(&mut state, source, PlayerId::P0);
    for name in ["Forest", "Island", "Mountain"] {
        crate::policy_observation_v6::tests::put(&mut state, PlayerId::P0, name, Zone::Library);
    }
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut census_outcomes = HashSet::new();
    for seed in 1..=32 {
        census_outcomes.insert(session.census_redeterminized_clone_v1(seed).is_ok());
        for mode in [WorldModeV1::TrueList, WorldModeV1::ClosedList] {
            let belief = LimitedBeliefV1::new([vec![0u16; 1]]);
            assert_eq!(
                sample_world_v1(&session, mode, Some(&belief), seed)
                    .err()
                    .as_deref(),
                Some("hidden-source provenance conflict"),
                "seed {seed} {mode:?}"
            );
        }
    }
    assert_eq!(
        census_outcomes.len(),
        2,
        "the census contrast must vary with the hidden draw"
    );
}

/// Known-card conditioning: a corpus deck lacking an observed card is never
/// drawn while a consistent deck exists, and a consistent draw reproduces that
/// deck's whole multiset across observed and unknown cards.
#[test]
fn belief_conditions_on_observed_opponent_cards() {
    let session = midgame(31);
    let (ug, wg) = (dck(UG), dck(WG));
    let plains = card_id_by_name("Plains").unwrap();
    assert!(!ug.contains(&plains) && wg.contains(&plains));
    let opponent_has_plains = session.game_state().players[1]
        .battlefield
        .iter()
        .any(|&id| session.game_state().objects.get(id).card_def == plains);
    assert!(opponent_has_plains, "fixture midgame shows a WG Plains");

    let both = LimitedBeliefV1::new([ug.clone(), wg.clone()]);
    for seed in 0..8 {
        let mut state = session.game_state().clone();
        let kind = apply_belief_v1(&mut state, PlayerId::P0, &[], &both, seed).unwrap();
        assert_eq!(kind, BeliefDrawV1::Consistent);
        let unknown: HashSet<_> = unknown_slots_v1(&state, PlayerId::P0, PlayerId::P1, &[])
            .into_iter()
            .collect();
        let mut whole = observed_cards_v1(&state, PlayerId::P1, &unknown);
        whole.extend(unknown.iter().map(|&id| state.objects.get(id).card_def));
        assert_eq!(sorted(whole), sorted(wg.clone()), "seed {seed}");
    }

    // Only the inconsistent deck: the nearest-deck fallback still fills every slot.
    let ug_only = LimitedBeliefV1::new([ug]);
    let mut state = session.game_state().clone();
    let kind = apply_belief_v1(&mut state, PlayerId::P0, &[], &ug_only, 3).unwrap();
    assert_eq!(kind, BeliefDrawV1::NearestDeck);
    assert!(LimitedBeliefV1::default().is_empty());
}

#[test]
fn random_games_finish_and_replay_exactly() {
    let (ug, wg) = (dck(UG), dck(WG));
    let limits = GameLimitsV1 {
        max_turns: 100,
        max_decisions: 20_000,
        full_validation: false,
        session: SessionKindV1::PreparedV57,
    };
    for seed in [1, 2, 3] {
        let run = || {
            play_game_v1(
                [&ug, &wg],
                [&PolicySpecV1::Random, &PolicySpecV1::Random],
                [None, None],
                seed,
                PlayerId::P1,
                limits,
            )
        };
        let (first, second) = (run(), run());
        assert!(
            !matches!(first.outcome, GameOutcomeV1::Error(_)),
            "seed {seed}: {:?}",
            first.outcome
        );
        assert_eq!(first.outcome, second.outcome);
        assert_eq!(
            (first.turns, first.decisions),
            (second.turns, second.decisions)
        );
    }
}

#[test]
fn fair_search_plays_a_bounded_game_without_errors() {
    let (ug, wg) = (dck(UG), dck(WG));
    let belief = LimitedBeliefV1::new([ug.clone(), wg.clone()]);
    let fair = PolicySpecV1::parse("pimc:sims=4:depth=8:closedlist").unwrap();
    let result = play_game_v1(
        [&ug, &wg],
        [&fair, &PolicySpecV1::Random],
        [Some(&belief), None],
        5,
        PlayerId::P0,
        GameLimitsV1 {
            max_turns: 100,
            max_decisions: 400,
            full_validation: false,
            session: SessionKindV1::PreparedV57,
        },
    );
    assert!(
        !matches!(result.outcome, GameOutcomeV1::Error(_)),
        "{:?}",
        result.outcome
    );
    assert!(result.decisions > 0);
    assert_eq!(result.rollout_errors, 0, "{:?}", result.first_rollout_error);
}

/// Issue #364: the trusted step skips only a re-check of an unchanged state, so
/// it must reproduce the full step exactly. Two copies of each game step in
/// lockstep with the same random choices; every state, response and terminal
/// must match.
#[test]
fn trusted_step_plays_exactly_like_the_full_step() {
    for seed in [7u64, 8, 9] {
        let mut full = new_game(seed);
        let mut trusted = full.clone();
        let mut rng = SplitMix64::seed(seed ^ 0x7357);
        let mut steps = 0;
        while let FastActorResponseV1::Decision(d) = full.current_response() {
            assert!(steps < 20_000, "seed {seed}: no terminal");
            let pick = random_index(&mut rng, d.legal_action_count);
            let a = full.step(d.episode_id, d.step, pick);
            let b = trusted.limited_trusted_step_v1(d.episode_id, d.step, pick);
            assert_eq!(a, b, "seed {seed} step {steps}");
            assert_eq!(
                full.game_state(),
                trusted.game_state(),
                "seed {seed} step {steps}"
            );
            if a.is_err() {
                break;
            }
            steps += 1;
        }
        assert_eq!(full.current_response(), trusted.current_response());
        assert!(
            steps > 100,
            "seed {seed}: {steps} steps is too short to compare"
        );
    }
}

fn rules_only(admission: RulesAdmissionV1) -> GameLimitsV1 {
    GameLimitsV1 {
        max_turns: 100,
        max_decisions: 20_000,
        full_validation: false,
        session: SessionKindV1::RulesOnly(admission),
    }
}

/// The rules-only session (no consumer contract) plays the Full fixture decks
/// to natural ends and replays exactly.
#[test]
fn rules_only_fixture_games_finish_and_replay() {
    let (ug, wg) = (dck(UG), dck(WG));
    for seed in [1, 2] {
        let run = || {
            play_game_v1(
                [&ug, &wg],
                [&PolicySpecV1::Random, &PolicySpecV1::Random],
                [None, None],
                seed,
                PlayerId::P0,
                rules_only(RulesAdmissionV1::Admitted),
            )
        };
        let (first, second) = (run(), run());
        assert!(
            matches!(
                first.outcome,
                GameOutcomeV1::Winner(_) | GameOutcomeV1::Draw
            ),
            "seed {seed}: {:?}",
            first.outcome
        );
        assert_eq!(first.outcome, second.outcome);
        assert_eq!(
            (first.turns, first.decisions),
            (second.turns, second.decisions)
        );
    }
}

/// Admission is explicit: a Partial printing plays only when admitted.
#[test]
fn rules_only_admission_refuses_unlisted_partial_printings() {
    let id = card_id_by_name("Curator of Destinies").unwrap();
    assert!(!CARD_DEFS[id as usize].has_full_support());
    let admitted = crate::limited_rules_only_v1::admitted_v1(id);
    assert_eq!(
        SessionKindV1::RulesOnly(RulesAdmissionV1::Admitted)
            .refusal(id)
            .is_none(),
        admitted
    );
    assert_eq!(SessionKindV1::PreparedV57.refusal(id), Some("not full"));
    assert_eq!(
        SessionKindV1::RulesOnly(RulesAdmissionV1::AllExecutable).refusal(id),
        None
    );
}

/// Diagnostic for the admission list: each top DraftZero blocker's reference
/// deck (four copies plus lands) against the WG fixture, every executable card
/// admitted, random play.
#[test]
fn rules_only_blocker_reference_decks_play_natural_games() {
    let wg = dck(WG);
    let decks = [
        include_str!("../../../data/limited/fdn_v1/FDN_reference_curator_of_destinies_partial.dck"),
        include_str!("../../../data/limited/fdn_v1/FDN_reference_imprisoned_moon_partial.dck"),
        include_str!("../../../data/limited/fdn_v1/FDN_reference_blasphemous_edict_partial.dck"),
        include_str!("../../../data/limited/fdn_v1/FDN_reference_lathril_partial.dck"),
        include_str!("../../../data/limited/fdn_v1/FDN_reference_leyline_axe_partial.dck"),
    ];
    let mut failures = Vec::new();
    for (i, text) in decks.iter().enumerate() {
        let deck = rules_only_dck(text);
        for seed in [11u64, 12, 13] {
            let result = play_game_v1(
                [&deck, &wg],
                [&PolicySpecV1::Random, &PolicySpecV1::Random],
                [None, None],
                seed,
                PlayerId::P0,
                rules_only(RulesAdmissionV1::AllExecutable),
            );
            if let GameOutcomeV1::Error(error) = &result.outcome {
                failures.push(format!("deck {i} seed {seed}: {error}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A .dck fixture resolved for rules-only play with every executable card.
fn rules_only_dck(text: &str) -> Vec<u16> {
    let rows: Vec<CustomCardCountV1> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("NAME:") && !l.starts_with("SB:"))
        .map(|line| {
            let (count, rest) = line.split_once(' ').expect("count and name");
            let name = match rest.strip_prefix('[') {
                Some(r) => r.split_once("] ").expect("set annotation").1,
                None => rest,
            };
            CustomCardCountV1 {
                name: name.to_string(),
                count: count.parse().expect("count"),
            }
        })
        .collect();
    DeckFileV1 {
        name: None,
        cards: rows,
    }
    .resolve_for(SessionKindV1::RulesOnly(RulesAdmissionV1::AllExecutable))
    .expect("reference decks are executable")
}

/// A rules-only combat scan step offers only the answers the policy surface
/// accepts (#376). Flamewake Phoenix attacks each combat if able, so its
/// decline was offered and then refused ("selected action no longer matches
/// the active policy environment") in random DraftZero sample games.
#[test]
fn rules_only_scan_steps_offer_only_accepted_answers() {
    let phoenix: DeckFileV1 = serde_json::from_str(include_str!(
        "../../../data/limited/draftzero_sample_v1/decks/FDN_top_15157_BR.json"
    ))
    .unwrap();
    let phoenix = phoenix
        .resolve_for(SessionKindV1::RulesOnly(RulesAdmissionV1::AllExecutable))
        .expect("sample deck is executable");
    let wg = dck(WG);
    let mut refused = Vec::new();
    for seed in 1..=12u64 {
        let result = play_game_v1(
            [&phoenix, &wg],
            [&PolicySpecV1::Random, &PolicySpecV1::Random],
            [None, None],
            seed,
            PlayerId::P0,
            rules_only(RulesAdmissionV1::AllExecutable),
        );
        if let GameOutcomeV1::Error(error) = &result.outcome {
            if error.contains("no longer matches") {
                refused.push(format!("seed {seed}: {error}"));
            }
        }
    }
    assert!(refused.is_empty(), "{refused:#?}");
}
