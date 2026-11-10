use super::*;
use mtg_kernel::card_def::{preflight_fully_supported_deck, DeckPreflightError};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::state::{GameState, Step, Target, Zone};

#[test]
fn declared_leylines_require_the_native_opening_profile() {
    let format = EternalFormatV1::Legacy;
    let swamp = id("Swamp");
    for leyline in ["Leyline of the Void", "Leyline of Sanctity"] {
        let card_def = id(leyline);
        for (main, side) in [
            ([vec![swamp; 59], vec![card_def]].concat(), vec![swamp; 15]),
            (vec![swamp; 60], [vec![swamp; 14], vec![card_def]].concat()),
        ] {
            assert!(matches!(
                EternalRegisteredDeckV1::new_v1(format, "ordinary", main.clone(), side.clone(), None),
                Err(EternalBo3ErrorV1::NativeOpeningSetup { reason }) if reason.contains(leyline)
            ));
            EternalRegisteredDeckV1::new_with_native_opening_v1(format, "native", main, side, None)
                .unwrap();
        }
        // Retain the capability diagnostic while these printings are Partial.
        let result = preflight_fully_supported_deck(&[swamp, card_def]);
        if CARD_DEFS[usize::from(card_def)].has_full_support() {
            assert!(matches!(result, Err(DeckPreflightError::RequiresNativeOpening { index: 1, .. })));
        } else {
            assert!(matches!(result, Err(DeckPreflightError::NotFullySupported { index: 1, .. })));
        }
    }
}

fn passive_action(decision: Decision) -> Action {
    match decision {
        Decision::ChooseLondonMulligan { mulligan_count: 0, .. } => {
            Action::ChooseLondonMulligan { mulligan: false }
        }
        Decision::ChooseEternalOpeningV1 { .. } => Action::FinishEffectSelection,
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        unexpected => panic!("unexpected decision in the native Leyline fixture: {unexpected:?}"),
    }
}

/// With no spells or attacks, ordinary draw and cleanup lead to a real decking
/// terminal. When present, Void must redirect the opponent's cleanup discard.
fn play_to_decking(
    state: &mut GameState,
    void: Option<(PlayerId, u16)>,
) -> (PlayerId, Vec<Action>) {
    let mut actions = Vec::new();
    let mut entered = false;
    let mut replaced = false;
    for _ in 0..4096 {
        let decision = engine::advance_until_decision(state);
        if let Decision::GameOver { winner } = decision {
            assert_eq!(entered, void.is_some());
            assert_eq!(replaced, void.is_some());
            return (winner.expect("one player decks first"), actions);
        }
        let mut entering = None;
        let mut discarded = None;
        if let Some((owner, printing)) = void {
            match &decision {
                Decision::ChooseEternalOpeningV1 { player, legal_cards }
                    if *player == owner && !entered =>
                {
                    entering = legal_cards.iter().copied().find(|&object| {
                        state.objects.get(object).card_def == printing
                    });
                    assert!(entering.is_some());
                    assert_eq!(state.step, Step::Untap);
                    assert!(state.stack.is_empty());
                }
                Decision::Discard { player, choices, .. } if *player == owner.opponent() => {
                    discarded = choices.first().copied();
                }
                _ => {}
            }
        }
        let action = entering.map_or_else(
            || passive_action(decision),
            |object| Action::ChooseEffectTarget(Target::Object(object)),
        );
        engine::step(state, action.clone()).unwrap();
        if let Some(object) = entering {
            assert_eq!(state.objects.get(object).zone, Zone::Battlefield);
            entered = true;
        }
        if let Some(object) = discarded {
            assert!(entered);
            assert_eq!(state.objects.get(object).zone, Zone::Exile);
            replaced = true;
        }
        actions.push(action);
    }
    panic!("native Leyline fixture did not reach a natural terminal");
}

#[test]
fn snapshot_void_75_boards_into_native_game_two_and_replays_to_terminal() {
    let register = |archetype| {
        let deck = snapshot_decks().into_iter().find(|deck| deck.archetype == archetype).unwrap();
        let expand = |rows: &[CustomCardCountV1]| {
            rows.iter()
                .flat_map(|row| std::iter::repeat_n(id(&row.name), row.count as usize))
                .collect()
        };
        assert!(deck.deck.companion.is_none());
        EternalRegisteredDeckV1::new_with_native_opening_v1(
            EternalFormatV1::Legacy,
            deck.archetype,
            expand(&deck.deck.cards),
            expand(&deck.sideboard),
            None,
        )
        .unwrap()
    };
    let decks = [register("legacy-mardu-energy"), register("legacy-izzet-delver")];
    let mut configurations = decks.each_ref().map(|deck| deck.registered_configuration().clone());
    let mut match_owner = EternalBestOfThreeMatchV1::new_v1(decks, PlayerId::P0).unwrap();
    let (game_one, mut state) = match_owner.start_game_with_native_opening_v1(
        PlayerId::P0,
        PlayDrawChoiceV1::Play,
        configurations.clone(),
        0x4c45_5900,
    ).unwrap();
    let (winner, _) = play_to_decking(&mut state, None);
    let winner = game_one.player_in_seat(winner.index());
    match_owner.record_game_result_v1(GameOutcomeV1::Win { winner }).unwrap();

    let void = id("Leyline of the Void");
    let pride = id("Ocelot Pride");
    let mut main = configurations[0].mainboard().to_vec();
    let mut side = configurations[0].sideboard().to_vec();
    assert_eq!(main.iter().filter(|&&card| card == pride).count(), 4);
    assert_eq!(side.iter().filter(|&&card| card == void).count(), 4);
    assert!(!main.contains(&void));
    main.retain(|&card| card != pride);
    side.retain(|&card| card != void);
    main.extend([void; 4]);
    side.extend([pride; 4]);
    configurations[0] = EternalConfigurationV1::new_v1(main, side);

    let start_two = |owner: &mut EternalBestOfThreeMatchV1, seed| {
        owner.start_game_with_native_opening_v1(
            winner.opponent(),
            PlayDrawChoiceV1::Play,
            configurations.clone(),
            seed,
        ).unwrap()
    };
    let (seed, mut game_two_owner, mut state, seat) = (0x4c45_5910..0x4c45_5930)
        .find_map(|seed| {
            let mut owner = match_owner.clone();
            let (prepared, state) = start_two(&mut owner, seed);
            assert_eq!(prepared.start().game_index, 2);
            let seat = if prepared.player_in_seat(0) == PlayerId::P0 { PlayerId::P0 } else { PlayerId::P1 };
            state.players[seat.index()].hand.iter().any(|&card| state.objects.get(card).card_def == void)
                .then_some((seed, owner, state, seat))
        })
        .expect("the fixed seed window contains a genuine kept Void");
    let (winner_two, actions) = play_to_decking(&mut state, Some((seat, void)));
    let (prepared, mut replay) = start_two(&mut match_owner, seed);
    for action in actions {
        assert!(!matches!(engine::advance_until_decision(&mut replay), Decision::GameOver { .. } | Decision::Halted { .. }));
        engine::step(&mut replay, action).unwrap();
    }
    assert_eq!(engine::advance_until_decision(&mut replay), Decision::GameOver { winner: Some(winner_two) });
    assert_eq!(replay.state_hash(), state.state_hash());
    let outcome = GameOutcomeV1::Win { winner: prepared.player_in_seat(winner_two.index()) };
    assert_eq!(game_two_owner.record_game_result_v1(outcome).unwrap(), match_owner.record_game_result_v1(outcome).unwrap());
}

#[test]
fn sanctity_native_opening_excludes_the_protected_player_from_first_spell_both_seats() {
    let sanctity = id("Leyline of Sanctity");
    let bolt = id("Lightning Bolt");
    for protected in [PlayerId::P0, PlayerId::P1] {
        let caster = protected.opponent();
        let decks = [PlayerId::P0, PlayerId::P1].map(|seat| {
            let main = if seat == protected {
                [vec![id("Plains"); 56], vec![sanctity; 4]].concat()
            } else {
                [vec![id("Mountain"); 56], vec![bolt; 4]].concat()
            };
            EternalRegisteredDeckV1::new_with_native_opening_v1(
                EternalFormatV1::Legacy, "native", main, Vec::new(), None,
            ).unwrap()
        });
        let configurations = decks.each_ref().map(|deck| deck.registered_configuration().clone());
        let owner = EternalBestOfThreeMatchV1::new_v1(decks, PlayerId::P0).unwrap();
        let state = (0x4c45_5940..0x4c45_5980).find_map(|seed| {
            let (_, state) = owner.clone().start_game_with_native_opening_v1(
                PlayerId::P0, PlayDrawChoiceV1::Play, configurations.clone(), seed,
            ).unwrap();
            let has = |seat: PlayerId, printing| state.players[seat.index()].hand.iter().any(|&card| state.objects.get(card).card_def == printing);
            (has(protected, sanctity) && has(caster, bolt)).then_some(state)
        }).expect("the fixed seed window contains both opening printings");

        for enter in [true, false] {
            let mut state = state.clone();
            let mut used_leyline = false;
            let mut resolved = false;
            for _ in 0..128 {
                let decision = engine::advance_until_decision(&mut state);
                if state.players[caster.index()].life == 17 {
                    resolved = true;
                    break;
                }
                let action = match decision {
                    Decision::ChooseEternalOpeningV1 { player, legal_cards }
                        if player == protected && enter && !used_leyline =>
                    {
                        assert_eq!(state.step, Step::Untap);
                        let card = legal_cards.into_iter().find(|&card| state.objects.get(card).card_def == sanctity).unwrap();
                        used_leyline = true;
                        Action::ChooseEffectTarget(Target::Object(card))
                    }
                    Decision::CastSpellOrPass { player, ref castable_spells, ref land_drops, ref mana_abilities, .. }
                        if player == caster =>
                    {
                        if let Some(&spell) = castable_spells.iter().find(|&&card| state.objects.get(card).card_def == bolt) {
                            Action::CastSpell(spell)
                        } else if let Some(&land) = land_drops.first() {
                            Action::PlayLand(land)
                        } else if let Some(&land) = mana_abilities.first() {
                            Action::ActivateManaAbility(land)
                        } else {
                            Action::Pass
                        }
                    }
                    Decision::ChooseTargets { player, spell, legal_targets, .. } => {
                        assert_eq!(player, caster);
                        assert_eq!(state.objects.get(spell).card_def, bolt);
                        assert!(legal_targets.contains(&Target::Player(caster)));
                        assert_eq!(legal_targets.contains(&Target::Player(protected)), !enter);
                        if enter {
                            let before = state.state_hash();
                            assert!(engine::step(&mut state, Action::ChooseTarget(Target::Player(protected))).is_err());
                            assert_eq!(state.state_hash(), before);
                        }
                        Action::ChooseTarget(Target::Player(caster))
                    }
                    other => passive_action(other),
                };
                engine::step(&mut state, action).unwrap();
            }
            assert_eq!(used_leyline, enter);
            assert!(resolved, "the first paid Bolt resolved");
        }
    }
}
