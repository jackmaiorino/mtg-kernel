use super::*;
use crate::card_def::card_id_by_name;

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "full_pool_search_tests.rs"]
mod full_pool_search;

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
    let v2 = PolicySpecV1::parse("pimc:sims=2:depth=0:closedlist-v2").unwrap();
    assert!(v2.needs_belief() && v2.uses_closed_list_v2());
    assert!(PolicySpecV1::parse("pimc:closedlist:closedlist-v2").is_err());
}

fn rules_only_closed_list_game(decks: [Vec<u16>; 2], starting: PlayerId) -> FastActorSessionV1 {
    let mut session = FastActorSessionV1::reset_rules_only_v1(
        31,
        31,
        (4_000, 1 << 20),
        ["first".into(), "second".into()],
        decks,
        starting,
        |id| {
            SessionKindV1::RulesOnly(RulesAdmissionV1::AllExecutable)
                .refusal(id)
                .is_none()
        },
    )
    .expect("rules-only reset");
    session.enable_hand_knowledge_by_name_v2().unwrap();
    session
}

#[test]
fn closed_list_v2_ignores_hidden_definitions_reveal_copy_and_future_rng() {
    let forest = card_id_by_name("Forest").unwrap();
    let plains = card_id_by_name("Plains").unwrap();
    let swamp = card_id_by_name("Swamp").unwrap();
    let island = card_id_by_name("Island").unwrap();
    let axe = card_id_by_name("Leyline Axe").unwrap();

    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut decks = [vec![plains; 40], vec![plains; 40]];
        decks[starting.index()][..18].fill(forest);
        decks[starting.index()][18..22].fill(axe);
        let session = rules_only_closed_list_game(decks, starting);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("expected a London decision");
        };
        let actor = seat(decision.acting_player);
        assert_eq!(actor, starting);
        assert!(decision.legal_action_count > 1);
        let opponent = actor.opponent();
        let same_named_card_at = |copy: usize, unknown: u16, future_seed: u64| {
            session.census_edited_clone_v1(|state| {
                let hand = state.players[opponent.index()].hand.clone();
                let revealed = hand[copy];
                for id in hand
                    .into_iter()
                    .chain(state.players[opponent.index()].library.clone())
                {
                    if id != revealed {
                        let card = state.objects.get_mut(id);
                        card.card_def = unknown;
                        card.name = CARD_DEFS[usize::from(unknown)].name.into();
                        card.v4 = ObjectStateV4::from_card_def(unknown);
                    }
                }
                state.reveal_hand_card(actor, opponent, revealed).unwrap();
                if copy != 0 {
                    let library = state.players[actor.index()].library.clone();
                    let reversed: Vec<_> = library
                        .iter()
                        .rev()
                        .map(|&id| state.objects.get(id).card_def)
                        .collect();
                    for (id, definition) in library.into_iter().zip(reversed) {
                        let card = state.objects.get_mut(id);
                        card.card_def = definition;
                        card.name = CARD_DEFS[usize::from(definition)].name.into();
                        card.v4 = ObjectStateV4::from_card_def(definition);
                    }
                }
                state.resample_future_randomness_for_search_v3(future_seed);
            })
        };
        let left = same_named_card_at(0, swamp, 8325);
        let right = same_named_card_at(4, island, 9325);
        assert_ne!(left.game_state(), right.game_state());
        assert_eq!(left.current_response(), right.current_response());
        let mut deck = vec![forest; 40];
        deck[0] = plains;
        let belief = LimitedBeliefV1::new([deck]);
        let world = WorldModeV1::ClosedListV2;
        for seed in 0..8 {
            let a = sample_world_v1(&left, world, Some(&belief), seed).unwrap();
            let b = sample_world_v1(&right, world, Some(&belief), seed).unwrap();
            assert_eq!(a.game_state(), b.game_state(), "seat {actor:?} seed {seed}");
            let known = a.game_state().known_hand_cards(actor, opponent);
            assert_eq!(known.len(), 1);
            assert_eq!(a.game_state().objects.get(known[0].object).card_def, plains);
        }
        let search = |session: &FastActorSessionV1| {
            pimc_choose_v1(session, 8, 12, world, Some(&belief), 534).unwrap()
        };
        let result = search(&left);
        assert_eq!(result, search(&right));
        assert_eq!(result.1.rejected, 0);
        assert_eq!(
            result.1.rollout_errors, 0,
            "{:?}",
            result.1.first_rollout_error
        );
        assert_eq!(result.1.visits.iter().sum::<u32>(), 8);
    }
}

#[test]
fn closed_list_v3_uses_present_minima_instead_of_distinct_physical_rows() {
    let forest = card_id_by_name("Forest").unwrap();
    let plains = card_id_by_name("Plains").unwrap();
    let island = card_id_by_name("Island").unwrap();
    let mut belief_deck = vec![forest; 40];
    belief_deck[0] = plains;
    let belief = LimitedBeliefV1::new([belief_deck]);
    for actor in [PlayerId::P0, PlayerId::P1] {
        let opponent = actor.opponent();
        let mut session = rules_only_closed_list_game([vec![plains; 40], vec![plains; 40]], actor);
        session.enable_limited_reveals_v3().unwrap();
        let variant = |other_copy, future_seed| {
            session.census_edited_clone_v1(|state| {
                let hand = state.players[opponent.index()].hand.clone();
                state
                    .reveal_distinct_hand_card(actor, opponent, hand[0])
                    .unwrap();
                state
                    .reveal_distinct_hand_card(actor, opponent, hand[other_copy])
                    .unwrap();
                if other_copy != 0 {
                    let hidden = state.objects.get_mut(hand[2]);
                    hidden.card_def = island;
                    hidden.name = "Island".into();
                    hidden.v4 = ObjectStateV4::from_card_def(island);
                }
                state.resample_future_randomness_for_search_v3(future_seed);
            })
        };
        let same_copy = variant(0, 13);
        let two_copies = variant(1, 29);
        assert_eq!(
            same_copy
                .game_state()
                .known_hand_cards(actor, opponent)
                .len(),
            1
        );
        assert_eq!(
            two_copies
                .game_state()
                .known_hand_cards(actor, opponent)
                .len(),
            2
        );
        let mode = WorldModeV1::ClosedListV3;
        for seed in 0..4 {
            let a = sample_world_v1(&same_copy, mode, Some(&belief), seed).unwrap();
            let b = sample_world_v1(&two_copies, mode, Some(&belief), seed).unwrap();
            assert_eq!(a.game_state(), b.game_state());
            assert_eq!(a.game_state().known_hand_cards(actor, opponent).len(), 1);
            assert_eq!(
                a.diagnostic_current_action_semantics(),
                session.diagnostic_current_action_semantics()
            );
            assert_eq!(
                b.diagnostic_current_action_semantics(),
                a.diagnostic_current_action_semantics()
            );
        }
        let a = pimc_choose_v1(&same_copy, 4, 2, mode, Some(&belief), 552).unwrap();
        let b = pimc_choose_v1(&two_copies, 4, 2, mode, Some(&belief), 552).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.1.rejected, 0);
        assert_eq!(a.1.rollout_errors, 0);
        assert_eq!(a.1.visits.iter().sum::<u32>(), 4);
    }
}

#[test]
fn closed_list_v2_searches_opening_decisions_for_the_frozen_fdn_pool() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../data/limited/fdn_v1/booster_pool_v1.json"
    ))
    .unwrap();
    let printings = manifest["card_names"].as_array().unwrap();
    assert_eq!(printings.len(), 286);
    let forest = card_id_by_name("Forest").unwrap();
    let island = card_id_by_name("Island").unwrap();
    let opponent = vec![island; 40];
    let belief = LimitedBeliefV1::new([opponent.clone()]);
    let mut ids = HashSet::new();
    let mut failures = Vec::new();
    for name in printings {
        let name = name.as_str().unwrap();
        let id = card_id_by_name(name).unwrap_or_else(|| panic!("missing printing {name}"));
        assert!(ids.insert(id), "duplicate definition for {name}");
        let mut deck = vec![forest; 40];
        deck[..4].fill(id);
        let session = rules_only_closed_list_game([deck, opponent.clone()], PlayerId::P0);
        let (_, stats) =
            pimc_choose_v1(&session, 2, 1, WorldModeV1::ClosedListV2, Some(&belief), 99)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        if stats.visits.iter().sum::<u32>() != 2 || stats.rollout_errors != 0 {
            failures.push(format!("{name}: {stats:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn closed_list_v2_counts_unmodeled_face_down_roots_as_fallbacks() {
    let forest = card_id_by_name("Forest").unwrap();
    let deck = vec![forest; 40];
    let session = rules_only_closed_list_game([deck.clone(), deck.clone()], PlayerId::P0);
    let hidden = session.census_edited_clone_v1(|state| {
        let object =
            crate::policy_observation_v6::tests::put(state, PlayerId::P1, "Forest", Zone::Exile);
        state.objects.get_mut(object).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0b10,
            hidden_by: None,
            hidden_by_source: None,
        });
    });
    let (_, stats) = pimc_choose_v1(
        &hidden,
        4,
        1,
        WorldModeV1::ClosedListV2,
        Some(&LimitedBeliefV1::new([deck])),
        99,
    )
    .unwrap();
    assert_eq!(stats.visits.iter().sum::<u32>(), 0);
    assert_eq!(stats.rejected, 4);
    assert_eq!(stats.rejection_reasons["unknown face-down identity"], 4);
    assert_eq!(stats.rollout_errors, 0);
}

#[test]
fn closed_list_v2_refuses_invalid_belief_printings() {
    let forest = card_id_by_name("Forest").unwrap();
    let deck = vec![forest; 40];
    let session = rules_only_closed_list_game([deck.clone(), deck], PlayerId::P0);
    let error = sample_world_v1(
        &session,
        WorldModeV1::ClosedListV2,
        Some(&LimitedBeliefV1::new([vec![u16::MAX; 40]])),
        99,
    )
    .err();
    assert_eq!(
        error.as_deref(),
        Some("belief corpus contains a non-executable printing")
    );
}

#[test]
fn closed_list_v2_searches_legal_partial_cast_and_land_actions() {
    check_partial_cast_and_land_actions(WorldModeV1::ClosedListV2);
}

#[test]
fn closed_list_v3_rebuilds_legal_partial_cast_and_land_actions() {
    check_partial_cast_and_land_actions(WorldModeV1::ClosedListV3);
}

#[test]
fn closed_list_v3_courtyard_answers_in_sampled_world_without_restoring_hidden_cards() {
    use crate::creature_types_v60::CreatureTypeV60;
    use crate::rl::ActionSemanticV1;

    let courtyard = card_id_by_name("Secluded Courtyard").unwrap();
    let forest = card_id_by_name("Forest").unwrap();
    let island = card_id_by_name("Island").unwrap();
    let mountain = card_id_by_name("Mountain").unwrap();
    let belief = LimitedBeliefV1::new([vec![island; 40]]);
    let mode = WorldModeV1::ClosedListV3;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut decks = [vec![mountain; 40], vec![mountain; 40]];
        decks[actor.index()] = vec![forest; 40];
        decks[actor.index()][0] = courtyard;
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &decks[0],
            &decks[1],
            crate::rl::card_name,
            552,
            actor,
        );
        state.step = crate::state::Step::Main1;
        state.reveal_meaning_v3.enable();
        let player = &mut state.players[actor.index()];
        let object = player.library.remove(0);
        assert_eq!(state.objects.get(object).card_def, courtyard);
        state.objects.get_mut(object).zone = Zone::Hand;
        player.hand.push(object);
        let mut live = rules_only_closed_list_game(decks, actor);
        live.install_rules_only_test_state_v1(state);
        let play = live.diagnostic_current_action_semantics().unwrap().iter().position(
            |action| matches!(action, ActionSemanticV1::PlayLand { source, .. } if source.arena_id == object.0),
        ).unwrap() as u32;

        let mut sampled = sample_world_v1(&live, mode, Some(&belief), 552).unwrap();
        step_index(&mut live, play, false).unwrap();
        step_index(&mut sampled, play, false).unwrap();
        let expected: Vec<_> = CreatureTypeV60::ALL
            .iter()
            .map(
                |&creature_type| ActionSemanticV1::ChooseEntryCreatureTypeV60 {
                    actor: actor.into(),
                    entry_ordinal: 0,
                    creature_type,
                },
            )
            .collect();
        assert_eq!(
            live.diagnostic_current_action_semantics().unwrap(),
            expected
        );
        assert_eq!(
            sampled.diagnostic_current_action_semantics().unwrap(),
            expected
        );
        let before = live.game_state().clone();
        let mut sampled_at_entry = sample_world_v1(&live, mode, Some(&belief), 552).unwrap();
        assert_eq!(
            sampled_at_entry.diagnostic_current_action_semantics().unwrap(),
            expected
        );

        let tampered = live.census_edited_clone_v1(|state| {
            let id = state.players[actor.opponent().index()].library[0];
            let hidden = state.objects.get_mut(id);
            hidden.card_def = island;
            hidden.name = CARD_DEFS[island as usize].name.into();
            hidden.v4 = ObjectStateV4::from_card_def(island);
        });
        assert_eq!(
            sample_world_v1(&tampered, mode, Some(&belief), 552)
                .err()
                .as_deref(),
            Some("Kinship checkpoint/source/proposal/scene/stack/answers changed"),
        );

        let chosen = CreatureTypeV60::Goblin;
        for branch in [&mut sampled, &mut sampled_at_entry] {
            step_index(branch, u32::from(chosen.choice_index()), true).unwrap();
            let state = branch.game_state();
            let entered = state.objects.get(object);
            assert_eq!(entered.zone, Zone::Battlefield);
            assert_eq!(entered.v4.kinship_type_v1, Some(chosen));
            let opponent_library = &state.players[actor.opponent().index()].library;
            assert_eq!(opponent_library.len(), 40);
            assert!(opponent_library
                .iter()
                .all(|&id| state.objects.get(id).card_def == island));
        }
        assert_eq!(live.game_state(), &before);
    }
}

fn check_partial_cast_and_land_actions(mode: WorldModeV1) {
    use crate::rl::ActionSemanticV1;
    use crate::state::Step;

    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../data/limited/fdn_v1/booster_pool_v1.json"
    ))
    .unwrap();
    let forest = card_id_by_name("Forest").unwrap();
    let island = card_id_by_name("Island").unwrap();
    let turtle = card_id_by_name("Aegis Turtle").unwrap();
    let mut opponent = vec![island; 40];
    opponent[0] = turtle;
    let belief = LimitedBeliefV1::new([opponent.clone()]);
    let mut failures = Vec::new();
    for name in manifest["card_names"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let id = card_id_by_name(name).unwrap();
        if CARD_DEFS[usize::from(id)].has_full_support() {
            continue;
        }
        let mut deck = vec![forest; 40];
        deck[..2].copy_from_slice(&[id, turtle]);
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &deck,
            &opponent,
            crate::rl::card_name,
            552,
            PlayerId::P0,
        );
        state.step = Step::Main1;
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
        state.enable_hand_knowledge_by_name_v2();
        if mode == WorldModeV1::ClosedListV3 {
            state.reveal_meaning_v3.enable();
        }
        for seat in [PlayerId::P0, PlayerId::P1] {
            let player = &mut state.players[seat.index()];
            let position = player
                .library
                .iter()
                .position(|&obj| state.objects.get(obj).card_def == turtle)
                .unwrap();
            let obj = player.library.remove(position);
            state.objects.get_mut(obj).zone = Zone::Battlefield;
            player.battlefield.push(obj);
        }
        let player = &mut state.players[0];
        let position = player
            .library
            .iter()
            .position(|&obj| state.objects.get(obj).card_def == id)
            .unwrap();
        let obj = player.library.remove(position);
        state.objects.get_mut(obj).zone = Zone::Hand;
        player.hand.push(obj);
        player.mana_pool.fill(20);

        let mut session = rules_only_closed_list_game([deck, opponent.clone()], PlayerId::P0);
        session.install_rules_only_test_state_v1(state);
        let Some(actions) = session.diagnostic_current_action_semantics() else {
            failures.push(format!("{name}: {:?}", session.current_response()));
            continue;
        };
        let Some(index) = actions.iter().position(|action| {
            matches!(action, ActionSemanticV1::CastSpell { source, .. }
                | ActionSemanticV1::PlayLand { source, .. } if source.arena_id == obj.0)
        }) else {
            failures.push(format!("{name}: no cast or play action in {actions:?}"));
            continue;
        };
        let (_, stats) =
            pimc_choose_v1(&session, actions.len() as u32, 0, mode, Some(&belief), 552).unwrap();
        if stats.visits[index] != 1 || stats.rollout_errors != 0 {
            failures.push(format!("{name}: {stats:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
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
