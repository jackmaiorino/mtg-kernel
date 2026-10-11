//! Unregistered Ghalta and Archdruid fixtures; actual admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, EffectDuration, Layers, UntilEndOfTurnEffect};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        957,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(object),
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        _ => panic!("fixture zone"),
    }
    object
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn offered(state: &mut GameState, spell: ObjectId) -> bool {
    matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => panic!("unexpected resolution decision {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("resolution did not settle");
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn printed_creature_power_and_elf_definitions_match_pinned_constructors() {
    let ghalta = &CARD_DEFS[card_id_by_name("Ghalta, Primal Hunger").unwrap() as usize];
    assert_eq!(ghalta.capability, CardCapability::Full);
    assert_eq!(ghalta.types, &[CardType::Creature]);
    assert_eq!(ghalta.subtypes, &[Subtype::Elder, Subtype::Dinosaur]);
    assert_eq!(ghalta.cost.generic, 10);
    assert_eq!(ghalta.cost.pips, &[Pip::Colored(ManaColor::G); 2]);
    assert_eq!(ghalta.colors, &[ManaColor::G]);
    assert_eq!((ghalta.power, ghalta.toughness), (Some(12), Some(12)));
    assert!(ghalta.supertypes.contains(&Supertype::Legendary));
    assert!(ghalta.keywords.has(Keywords::TRAMPLE));
    let druid = &CARD_DEFS[card_id_by_name("Elvish Archdruid").unwrap() as usize];
    assert_eq!(druid.capability, CardCapability::Full);
    assert_eq!(druid.types, &[CardType::Creature]);
    assert_eq!(druid.subtypes, &[Subtype::Elf, Subtype::Druid]);
    assert_eq!(druid.cost.generic, 1);
    assert_eq!(druid.cost.pips, &[Pip::Colored(ManaColor::G); 2]);
    assert_eq!(druid.colors, &[ManaColor::G]);
    assert_eq!((druid.power, druid.toughness), (Some(2), Some(2)));
}

#[test]
fn ghalta_signed_power_reduction_preserves_green_payment_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (bonus, minus, generic) in [(8, 0, 0), (4, 3, 7), (0, 4, 10)] {
            let mut state = ready(player);
            let first = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let second = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(first).counters.plus1_plus1 = bonus;
            state.objects.get_mut(second).counters.minus1_minus1 = minus;
            // Preserve the negative-power creature through state-based actions.
            state
                .engine
                .until_end_of_turn
                .push(UntilEndOfTurnEffect::ResolvedObjectEffect {
                    object_id: second,
                    object_zone_change_count: state.objects.get(second).zone_change_count,
                    layer: Layers::POWER_TOUGHNESS,
                    timestamp: 1,
                    duration: EffectDuration::EndOfTurn,
                    power: 0,
                    toughness: 5,
                    grant_haste: false,
                });
            for source in [first, second] {
                state.objects.get_mut(source).tapped = true;
            }
            let foreign = put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.objects.get_mut(foreign).counters.plus1_plus1 = 100;
            let spell = put(&mut state, player, "Ghalta, Primal Hunger", Zone::Hand);
            state.players[player.index()].mana_pool = [0, 0, 0, 0, 1, generic];
            assert!(!offered(&mut state, spell));
            state.players[player.index()].mana_pool[4] = 2;
            if generic > 0 {
                state.players[player.index()].mana_pool[5] = generic - 1;
                assert!(!offered(&mut state, spell));
                state.players[player.index()].mana_pool[5] = generic;
            }
            let mut resumed = copy(&state);
            for game in [&mut state, &mut resumed] {
                assert!(offered(game, spell));
                engine::step(game, Action::CastSpell(spell)).unwrap();
                settle(game);
                assert_eq!(game.objects.get(spell).zone, Zone::Battlefield);
                assert_eq!(game.players[player.index()].mana_pool, [0; 6]);
                assert_eq!(engine::effective_power(game, spell), 12);
            }
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::to_value(resumed).unwrap()
            );
        }
    }
}

#[test]
fn archdruid_lord_excludes_itself_stacks_and_tracks_current_control() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let first = put(&mut state, player, "Elvish Archdruid", Zone::Battlefield);
        let elf = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let foreign = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        assert_eq!(engine::effective_power(&state, first), 2);
        assert_eq!(engine::effective_power(&state, elf), 2);
        assert_eq!(engine::effective_power(&state, foreign), 1);
        let second = put(&mut state, player, "Elvish Archdruid", Zone::Battlefield);
        assert_eq!(engine::effective_power(&state, first), 3);
        assert_eq!(engine::effective_power(&state, second), 3);
        assert_eq!(engine::effective_power(&state, elf), 3);
        state.objects.get_mut(second).controller = player.opponent();
        state.players[player.index()]
            .battlefield
            .retain(|&id| id != second);
        state.players[player.opponent().index()]
            .battlefield
            .push(second);
        let resumed = copy(&state);
        assert_eq!(engine::effective_power(&resumed, first), 2);
        assert_eq!(engine::effective_power(&resumed, elf), 2);
        assert_eq!(engine::effective_power(&resumed, foreign), 2);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Exile));
        assert_eq!(engine::effective_power(&state, elf), 1);
    }
}

#[test]
fn archdruid_produces_nine_green_and_refuses_pool_overflow_before_mutation() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for initial in [0, 246, 247] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Elvish Archdruid", Zone::Battlefield);
            for _ in 0..8 {
                put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            }
            put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.players[player.index()].mana_pool[4] = initial;
            let mut resumed = copy(&state);
            for game in [&mut state, &mut resumed] {
                let Decision::CastSpellOrPass { mana_abilities, .. } = next(game) else {
                    panic!("priority decision absent");
                };
                assert_eq!(mana_abilities.contains(&source), initial != 247);
                let before = serde_json::to_value(&*game).unwrap();
                let result = engine::step(game, Action::ActivateManaAbility(source));
                if initial == 247 {
                    assert!(result.is_err());
                    assert_eq!(serde_json::to_value(&*game).unwrap(), before);
                } else {
                    result.unwrap();
                    assert!(game.objects.get(source).tapped);
                    assert_eq!(game.players[player.index()].mana_pool[4], initial + 9);
                }
            }
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::to_value(resumed).unwrap()
            );
        }
    }
}

#[test]
fn archdruid_mana_counts_controlled_noncreature_elves_and_effective_subtypes() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Elvish Archdruid", Zone::Battlefield);
        let land = put(&mut state, player, "Forest", Zone::Battlefield);
        state.objects.get_mut(land).v4.effective_subtype_ids = vec![Subtype::Elf.stable_id()];
        let elf = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(elf).v4.effective_subtype_ids = vec![Subtype::Frog.stable_id()];
        let mut resumed = copy(&state);
        engine::step(&mut resumed, Action::ActivateManaAbility(source)).unwrap();
        assert_eq!(resumed.players[player.index()].mana_pool[4], 2);
        assert_eq!(engine::effective_power(&resumed, land), 0);
        assert_eq!(engine::effective_power(&resumed, elf), 1);
    }
}

#[test]
fn witness_protection_suppresses_archdruid_lord_and_mana_then_restores_both() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Elvish Archdruid", Zone::Battlefield);
        let elf = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let aura = put(&mut state, player, "Witness Protection", Zone::Hand);
        state.players[player.index()].mana_pool[1] = 1;
        assert!(offered(&mut state, aura));
        engine::step(&mut state, Action::CastSpell(aura)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        let mut resumed = copy(&state);
        for game in [&mut state, &mut resumed] {
            engine::step(game, Action::ChooseTarget(Target::Object(source))).unwrap();
            settle(game);
            assert_eq!(engine::effective_power(game, source), 1);
            assert_eq!(engine::effective_power(game, elf), 1);
            assert!(engine::step(game, Action::ActivateManaAbility(source)).is_err());
            event::propose_and_commit(game, ProposedEvent::zone_change(aura, Zone::Graveyard));
            assert_eq!(engine::effective_power(game, elf), 2);
            engine::step(game, Action::ActivateManaAbility(source)).unwrap();
            assert_eq!(game.players[player.index()].mana_pool[4], 2);
        }
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::to_value(resumed).unwrap()
        );
    }
}
