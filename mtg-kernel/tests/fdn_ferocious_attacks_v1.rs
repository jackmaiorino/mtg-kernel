//! Source preparation. Registration and actual card qualification remain pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{self, ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(active: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1109,
        active,
    );
    state.step = Step::Main1;
    enable_foundations_combat_v1(&mut state).unwrap();
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
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
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let choice = engine::advance_until_decision(state);
    assert!(!matches!(choice, Decision::Halted { .. }), "{choice:?}");
    choice
}

fn priority(state: &mut GameState, player: PlayerId) {
    let Decision::CastSpellOrPass {
        player: chooser, ..
    } = next(state)
    else {
        panic!("priority decision absent");
    };
    if chooser != player {
        engine::step(state, Action::Pass).unwrap();
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player: chooser, .. } if chooser == player)
        );
    }
}

fn cast(state: &mut GameState, spell: ObjectId, target: Option<Target>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for _ in 0..16 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = target.expect("fixture target");
                assert!(legal_targets.contains(&target));
                engine::step(state, Action::ChooseTarget(target)).unwrap();
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            Decision::CastSpellOrPass { .. } if state.engine.pending_cast.is_none() => return,
            choice => panic!("unexpected cast decision {choice:?}"),
        }
    }
    panic!("cast did not finish");
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            choice => panic!("unexpected resolution decision {choice:?}"),
        }
    }
    panic!("resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn same(state: &GameState, replay: &GameState) {
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
}

fn attack(state: &mut GameState, source: ObjectId) {
    state.step = Step::DeclareAttackers;
    assert!(
        matches!(next(state), Decision::DeclareAttackers { eligible, .. } if eligible.contains(&source))
    );
    engine::step(state, Action::DeclareAttackers(vec![source])).unwrap();
}

fn cleanup(state: &mut GameState) {
    assert!(state.stack.is_empty());
    let active = state.active_player;
    state.step = Step::End;
    state.priority_player = active;
    state.engine.priority_passes = [false, false];
    for _ in 0..8 {
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        if state.active_player != active {
            return;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("cleanup transition absent");
}

#[test]
fn exact_definitions_colored_payment_and_ruby_haste_mana_choices() {
    for (name, generic, colors, power, toughness, subtypes) in [
        (
            "Ruby, Daring Tracker",
            0,
            vec![ManaColor::R, ManaColor::G],
            1,
            2,
            vec![Subtype::Human, Subtype::Scout],
        ),
        (
            "Courageous Goblin",
            1,
            vec![ManaColor::R],
            2,
            2,
            vec![Subtype::Goblin],
        ),
    ] {
        let def = &CARD_DEFS[card_id_by_name(name).unwrap() as usize];
        assert_eq!(def.capability, CardCapability::Full);
        assert_eq!(def.types, &[CardType::Creature]);
        assert_eq!(def.subtypes, subtypes);
        assert_eq!((def.power, def.toughness), (Some(power), Some(toughness)));
        assert_eq!(def.cost.generic, generic);
        assert_eq!(
            def.cost.pips,
            colors.iter().copied().map(Pip::Colored).collect::<Vec<_>>()
        );
        assert_eq!(def.colors, colors);
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(player);
            let source = put(&mut state, player, name, Zone::Hand);
            state.players[player.index()].mana_pool[5] = 2;
            next(&mut state);
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            state.players[player.index()].mana_pool = [0; 6];
            state.players[player.index()].mana_pool[5] = generic;
            for color in &colors {
                state.players[player.index()].mana_pool[color.pool_index()] += 1;
            }
            cast(&mut state, source, None);
            settle(&mut state);
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            assert!(state.objects.get(source).summoning_sick);
            assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
            if name == "Ruby, Daring Tracker" {
                assert!(engine::has_effective_keyword(
                    &state,
                    source,
                    Keywords::HASTE
                ));
                assert!(mana::gather_sources(player, &state)
                    .iter()
                    .any(|candidate| candidate.id == source
                        && candidate.choices == vec![ManaColor::R, ManaColor::G]));
                let before = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(
                    &mut state,
                    Action::ActivateManaAbilityChoice(source, ManaColor::U)
                )
                .is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                for color in [ManaColor::R, ManaColor::G] {
                    let mut current = restored(&state);
                    assert!(
                        matches!(next(&mut current), Decision::CastSpellOrPass { mana_abilities, .. } if mana_abilities.contains(&source))
                    );
                    engine::step(
                        &mut current,
                        Action::ActivateManaAbilityChoice(source, color),
                    )
                    .unwrap();
                    assert!(current.objects.get(source).tapped);
                    let mut expected = [0; 6];
                    expected[color.pool_index()] = 1;
                    assert_eq!(current.players[player.index()].mana_pool, expected);
                }
                attack(&mut state, source);
                assert!(state.objects.get(source).tapped);
                assert!(state.engine.pending_triggers.is_empty());
            }
        }
    }
}

#[test]
fn actual_attack_threshold_uses_friendly_power_and_source_and_replays_cleanup() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for name in ["Ruby, Daring Tracker", "Courageous Goblin"] {
            for (support_counters, own_counters, expected) in
                [(0, 0, false), (2, 0, false), (3, 0, true), (0, 3, true)]
            {
                let mut state = ready(player);
                let source = put(&mut state, player, name, Zone::Battlefield);
                let support = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
                state.objects.get_mut(support).counters.plus1_plus1 = support_counters;
                state.objects.get_mut(source).counters.plus1_plus1 = own_counters;
                put(
                    &mut state,
                    player.opponent(),
                    "Tolarian Terror",
                    Zone::Battlefield,
                );
                attack(&mut state, source);
                assert_eq!(state.engine.pending_triggers.len(), usize::from(expected));
                let mut replay = restored(&state);
                for current in [&mut state, &mut replay] {
                    settle(current);
                    let delta = if expected {
                        if name == "Ruby, Daring Tracker" {
                            2
                        } else {
                            1
                        }
                    } else {
                        0
                    };
                    let base = if name == "Ruby, Daring Tracker" { 1 } else { 2 };
                    assert_eq!(
                        engine::effective_power(current, source),
                        base + i32::from(own_counters) + delta
                    );
                    assert_eq!(
                        engine::has_effective_keyword(current, source, Keywords::MENACE),
                        expected && name == "Courageous Goblin"
                    );
                    cleanup(current);
                    assert_eq!(
                        engine::effective_power(current, source),
                        base + i32::from(own_counters)
                    );
                    assert!(!engine::has_effective_keyword(
                        current,
                        source,
                        Keywords::MENACE
                    ));
                }
                same(&state, &replay);
            }
        }
    }
}

#[test]
fn pending_attack_survives_qualifier_departure_and_control_change_but_not_source_reentry() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for name in ["Ruby, Daring Tracker", "Courageous Goblin"] {
            for reenter in [false, true] {
                let mut state = ready(player);
                let source = put(&mut state, player, name, Zone::Battlefield);
                let support = put(&mut state, player, "Tolarian Terror", Zone::Battlefield);
                attack(&mut state, source);
                assert_eq!(state.engine.pending_triggers.len(), 1);
                // Materialize the actual printed trigger on the stack before changing zones.
                assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
                assert_eq!(state.stack.len(), 1);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(support, Zone::Graveyard),
                );
                if reenter {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                } else {
                    state.players[player.index()]
                        .battlefield
                        .retain(|id| *id != source);
                    state.players[player.opponent().index()]
                        .battlefield
                        .push(source);
                    state.objects.get_mut(source).controller = player.opponent();
                }
                let mut replay = restored(&state);
                for current in [&mut state, &mut replay] {
                    settle(current);
                    let base = if name == "Ruby, Daring Tracker" { 1 } else { 2 };
                    let delta = if reenter {
                        0
                    } else if name == "Ruby, Daring Tracker" {
                        2
                    } else {
                        1
                    };
                    assert_eq!(engine::effective_power(current, source), base + delta);
                    assert_eq!(
                        engine::has_effective_keyword(current, source, Keywords::MENACE),
                        !reenter && name == "Courageous Goblin"
                    );
                }
                same(&state, &replay);
            }
        }
    }
}

#[test]
fn courageous_menace_refuses_one_blocker_and_accepts_two_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Courageous Goblin", Zone::Battlefield);
        put(&mut state, player, "Tolarian Terror", Zone::Battlefield);
        let a = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let b = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        attack(&mut state, source);
        settle(&mut state);
        assert!(engine::has_effective_keyword(
            &state,
            source,
            Keywords::MENACE
        ));
        for _ in 0..8 {
            match next(&mut state) {
                Decision::DeclareBlockers { .. } => break,
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("unexpected combat decision {other:?}"),
            }
        }
        assert!(matches!(next(&mut state), Decision::DeclareBlockers { .. }));
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            let before = serde_json::to_vec(&*current).unwrap();
            assert!(engine::step(current, Action::DeclareBlockers(vec![(a, source)])).is_err());
            assert_eq!(serde_json::to_vec(&*current).unwrap(), before);
            engine::step(
                current,
                Action::DeclareBlockers(vec![(a, source), (b, source)]),
            )
            .unwrap();
        }
        same(&state, &replay);
    }
}
