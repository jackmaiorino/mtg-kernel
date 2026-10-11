//! Unregistered Skyknight Squire fixtures; actual gameplay admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
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
        Zone::Library => state.players[player.index()].library.push(id),
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

fn settle(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let choice = next(state);
        match choice {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return choice
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { ref pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            _ => return choice,
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

fn has_flying(state: &GameState, source: ObjectId) -> bool {
    engine::has_effective_keyword(state, source, Keywords::FLYING)
}

fn has_knight(state: &GameState, source: ObjectId) -> bool {
    let one = engine::has_effective_subtype(state, source, Subtype::Knight);
    let all = engine::effective_subtype_ids(state, source).contains(&Subtype::Knight.stable_id());
    assert_eq!(one, all);
    one
}

fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let id = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Battlefield));
    id
}

#[test]
fn metadata_cost_boundary_and_actual_entry_do_not_trigger_for_self() {
    let def = &CARD_DEFS[card_id_by_name("Skyknight Squire").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Cat, Subtype::Scout]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(1), Some(1), 2)
    );
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert!(!def.keywords.has(Keywords::FLYING));
    for player in [PlayerId::P0, PlayerId::P1] {
        for (white, colorless, legal) in [(0, 2, false), (1, 0, false), (1, 1, true)] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Skyknight Squire", Zone::Hand);
            state.players[player.index()].mana_pool[0] = white;
            state.players[player.index()].mana_pool[5] = colorless;
            priority(&mut state, player);
            let Decision::CastSpellOrPass {
                castable_spells, ..
            } = next(&mut state)
            else {
                panic!("priority");
            };
            assert_eq!(castable_spells.contains(&source), legal);
            if !legal {
                let bytes = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
                continue;
            }
            cast(&mut state, source, None);
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
                assert_eq!(current.objects.get(source).counters.plus1_plus1, 0);
                assert!(!has_flying(current, source));
                assert!(!has_knight(current, source));
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn creature_token_and_nontoken_entries_cross_the_three_counter_threshold() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Skyknight Squire", Zone::Battlefield);
        enter(&mut state, player, "Forest");
        settle(&mut state);
        enter(&mut state, player.opponent(), "Llanowar Elves");
        settle(&mut state);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
        enter(&mut state, player, "Llanowar Elves");
        settle(&mut state);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
        let token = card_id_by_name("Elf Warrior Token").unwrap();
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            for expected in 2..=4 {
                event::propose_and_commit(current, ProposedEvent::create_token(token, player));
                settle(current);
                assert_eq!(current.objects.get(source).counters.plus1_plus1, expected);
                assert_eq!(has_flying(current, source), expected >= 3);
                assert_eq!(has_knight(current, source), expected >= 3);
                assert!(engine::has_effective_subtype(current, source, Subtype::Cat));
                assert!(engine::has_effective_subtype(
                    current,
                    source,
                    Subtype::Scout
                ));
            }
            current.objects.get_mut(source).counters.plus1_plus1 = 2;
            assert!(!has_flying(current, source));
            assert!(!has_knight(current, source));
            current.objects.get_mut(source).counters.stun = 3;
            assert!(!has_flying(current, source));
            assert!(!has_knight(current, source));
        }
        same(&state, &replay);
    }
}

#[test]
fn queued_counter_keeps_exact_source_and_captured_controller_after_responses() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for response in ["leave", "blink", "transfer"] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Skyknight Squire", Zone::Battlefield);
            enter(&mut state, player, "Llanowar Elves");
            next(&mut state);
            assert_eq!(state.stack.len(), 1);
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                if response == "transfer" {
                    current.players[player.index()]
                        .battlefield
                        .retain(|id| *id != source);
                    current.objects.get_mut(source).controller = player.opponent();
                    current.players[player.opponent().index()]
                        .battlefield
                        .push(source);
                } else {
                    event::propose_and_commit(
                        current,
                        ProposedEvent::zone_change(source, Zone::Exile),
                    );
                    if response == "blink" {
                        event::propose_and_commit(
                            current,
                            ProposedEvent::zone_change(source, Zone::Battlefield),
                        );
                    }
                }
                settle(current);
                assert_eq!(
                    current.objects.get(source).counters.plus1_plus1,
                    i32::from(response == "transfer")
                );
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn future_entries_use_current_controller_and_departed_sources_do_not_watch() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Skyknight Squire", Zone::Battlefield);
        state.players[player.index()]
            .battlefield
            .retain(|id| *id != source);
        state.objects.get_mut(source).controller = player.opponent();
        state.players[player.opponent().index()]
            .battlefield
            .push(source);
        enter(&mut state, player, "Llanowar Elves");
        settle(&mut state);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
        enter(&mut state, player.opponent(), "Llanowar Elves");
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 1);
            event::propose_and_commit(current, ProposedEvent::zone_change(source, Zone::Graveyard));
            enter(current, player.opponent(), "Llanowar Elves");
            settle(current);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 0);
        }
        same(&state, &replay);
    }
}

#[test]
fn actual_witness_removes_future_capture_and_statics_but_preserves_queued_counter() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Skyknight Squire", Zone::Battlefield);
        state.objects.get_mut(source).counters.plus1_plus1 = 3;
        enter(&mut state, player, "Llanowar Elves");
        next(&mut state);
        assert_eq!(state.stack.len(), 1);
        let aura = put(&mut state, player, "Witness Protection", Zone::Hand);
        state.players[player.index()].mana_pool[1] = 1;
        priority(&mut state, player);
        cast(&mut state, aura, Some(Target::Object(source)));
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 4);
            assert!(!has_flying(current, source));
            assert!(!has_knight(current, source));
            assert!(engine::has_effective_subtype(
                current,
                source,
                Subtype::Citizen
            ));
            enter(current, player, "Llanowar Elves");
            settle(current);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 4);
            event::propose_and_commit(current, ProposedEvent::zone_change(aura, Zone::Graveyard));
            assert!(has_flying(current, source));
            assert!(has_knight(current, source));
        }
        same(&state, &replay);
    }
}

#[test]
fn public_characteristics_and_real_blocker_offers_track_flying_and_added_knight() {
    use mtg_kernel::surface_v2::{HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode};
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Skyknight Squire", Zone::Battlefield);
        state.objects.get_mut(source).counters.plus1_plus1 = 3;
        let ground = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let reach = put(
            &mut state,
            player.opponent(),
            "Treetop Snarespinner",
            Zone::Battlefield,
        );
        let flying = put(
            &mut state,
            player.opponent(),
            "Healer's Hawk",
            Zone::Battlefield,
        );
        state.step = Step::DeclareAttackers;
        assert!(matches!(
            next(&mut state),
            Decision::DeclareAttackers { .. }
        ));
        engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
        for _ in 0..16 {
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareBlockers { .. } => break,
                other => panic!("unexpected decision {other:?}"),
            }
        }
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            let Decision::DeclareBlockers { legal_blockers, .. } = next(current) else {
                panic!("blocker decision absent");
            };
            let offered = &legal_blockers
                .iter()
                .find(|(attacker, _)| *attacker == source)
                .unwrap()
                .1;
            assert!(!offered.contains(&ground));
            assert!(offered.contains(&reach));
            assert!(offered.contains(&flying));
            let bytes = serde_json::to_vec(&*current).unwrap();
            assert!(
                engine::step(current, Action::DeclareBlockers(vec![(ground, source)])).is_err()
            );
            assert_eq!(serde_json::to_vec(&*current).unwrap(), bytes);
            let surface = HarnessSurfaceV2::new_with_priority_mode_v1(
                PriorityModeV1::EngineWindowsV1,
                SuppressionAuditMode::Full,
            );
            for observer in [player, player.opponent()] {
                let projection = mtg_kernel::rl::observe_v2(current, &surface, observer, 0)
                    .unwrap()
                    .projection;
                let card = projection.battlefield[player.index()]
                    .iter()
                    .find(|card| card.stable.arena_id == source.0)
                    .unwrap();
                assert!(card.characteristics.effective_keywords.flying);
                assert!(card
                    .characteristics
                    .effective_subtype_ids
                    .contains(&Subtype::Knight.stable_id()));
            }
            current.objects.get_mut(source).counters.plus1_plus1 = 2;
            let Decision::DeclareBlockers { legal_blockers, .. } = next(current) else {
                panic!("blocker decision absent");
            };
            assert!(legal_blockers
                .iter()
                .find(|(attacker, _)| *attacker == source)
                .unwrap()
                .1
                .contains(&ground));
            engine::step(current, Action::DeclareBlockers(vec![(ground, source)])).unwrap();
        }
        same(&state, &replay);
    }
}
