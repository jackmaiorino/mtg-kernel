//! Future printed-card acceptance. Requires actual FDN registration and the
//! documented Scout materialization admission; no synthetic catalog or skip.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        110_2026,
        actor,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("requires actual FDN admission");
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: zone == Zone::Hand,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
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

fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) {
    for _ in 0..200 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected resolution: {other:?}"),
        }
    }
    panic!("resolution bound");
}

fn actor_priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..3 {
        match next(state) {
            Decision::CastSpellOrPass { player, .. } if player == actor => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected priority: {other:?}"),
        }
    }
    panic!("actor priority");
}

fn activate(
    state: &mut GameState,
    actor: PlayerId,
    source: ObjectId,
    ability: u8,
    red: u8,
    generic: u8,
) {
    actor_priority(state, actor);
    state.players[actor.index()].mana_pool = [0; 6];
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = red;
    state.players[actor.index()].mana_pool[5] = generic;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. }
        if activatable_abilities.contains(&(source, ability)))
    );
    engine::step(state, Action::ActivateAbility(source, ability)).unwrap();
}

fn blink(state: &mut GameState, source: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Hand));
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
}

/// Traverse real combat, optionally stopping before the engine publishes a
/// combat-produced impulse trigger onto the stack. No fabricated damage event.
fn combat(state: &mut GameState, attacker: ObjectId, stop_at_impulse: bool) {
    let mut declared = false;
    for _ in 0..200 {
        if stop_at_impulse
            && (state
                .engine
                .pending_triggers
                .iter()
                .any(|p| p.granted_by.is_some())
                || state.stack.iter().any(|p| p.v4.granted_by.is_some()))
        {
            return;
        }
        match next(state) {
            Decision::CastSpellOrPass { .. } if declared && state.step == Step::Main2 => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { eligible, .. } => {
                assert!(eligible.contains(&attacker));
                engine::step(state, Action::DeclareAttackers(vec![attacker])).unwrap();
                declared = true;
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected combat: {other:?}"),
        }
    }
    panic!("combat bound");
}

#[test]
fn printed_definitions_require_real_admission_and_scout_universe() {
    assert!(
        Subtype::CREATURE_TYPES.contains(&Subtype::Scout),
        "FDN admission must extend Scout universe"
    );
    let land = &CARD_DEFS[card_id_by_name("Soulstone Sanctuary").unwrap() as usize];
    let kellan = &CARD_DEFS[card_id_by_name("Kellan, Planar Trailblazer").unwrap() as usize];
    assert_eq!(land.capability, CardCapability::Full);
    assert_eq!(kellan.capability, CardCapability::Full);
    let animation = land.animation.unwrap();
    assert!(!animation.until_end_of_turn);
    assert_eq!((animation.power, animation.toughness), (3, 3));
    assert_eq!(animation.subtypes, Subtype::CREATURE_TYPES);
    assert!(
        matches!(land.activated_abilities[0].cost, [CostComponent::Mana(cost)] if cost.generic == 4 && cost.pips.is_empty())
    );
    assert_eq!(kellan.cost.generic, 0);
    assert_eq!(kellan.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(kellan.activated_abilities.len(), 2);
}

#[test]
fn soulstone_real_land_play_animation_restore_sickness_cleanup_and_departure_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let land = put(&mut state, actor, "Soulstone Sanctuary", Zone::Hand);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { land_drops, .. } if land_drops.contains(&land))
        );
        engine::step(&mut state, Action::PlayLand(land)).unwrap();
        activate(&mut state, actor, land, 0, 0, 4);
        state = restore(&state);
        settle(&mut state);
        assert!(engine::object_has_type(&state, land, CardType::Land));
        assert!(engine::object_has_type(&state, land, CardType::Creature));
        assert_eq!(
            (
                engine::effective_power(&state, land),
                engine::effective_toughness(&state, land)
            ),
            (3, 3)
        );
        assert!(engine::has_effective_keyword(
            &state,
            land,
            Keywords::VIGILANCE
        ));
        for &subtype in Subtype::CREATURE_TYPES {
            assert!(engine::has_effective_subtype(&state, land, subtype));
        }
        let mut refused = false;
        for _ in 0..100 {
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareAttackers { eligible, .. } => {
                    assert!(
                        !eligible.contains(&land),
                        "land played this turn is summoning sick"
                    );
                    let before = state.clone();
                    assert!(
                        engine::step(&mut state, Action::DeclareAttackers(vec![land])).is_err()
                    );
                    assert_eq!(state, before);
                    engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap();
                    refused = true;
                    break;
                }
                other => panic!("preattack {other:?}"),
            }
        }
        assert!(refused);
        // Traverse actual cleanup and turn change, preserving permanent animation.
        for _ in 0..100 {
            if state.active_player != actor {
                break;
            }
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("cleanup {other:?}"),
            }
        }
        assert_ne!(state.active_player, actor);
        assert!(engine::object_has_type(&state, land, CardType::Creature));
        blink(&mut state, land);
        assert!(!engine::object_has_type(&state, land, CardType::Creature));
    }
}

#[test]
fn animation_and_upgrade_pending_sources_cannot_cross_incarnations_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Soulstone Sanctuary", "Kellan, Planar Trailblazer"] {
            let mut state = ready(actor);
            let source = put(&mut state, actor, name, Zone::Battlefield);
            let red = u8::from(name.starts_with("Kellan"));
            activate(
                &mut state,
                actor,
                source,
                0,
                red,
                if red == 1 { 1 } else { 4 },
            );
            blink(&mut state, source);
            state = restore(&state);
            settle(&mut state);
            assert!(state.objects.get(source).v4.animation_timestamp.is_none());
            assert!(state.objects.get(source).v4.creature_upgrade.is_none());
            actor_priority(&mut state, actor);
            state.players[actor.index()].mana_pool = [0; 6];
            let before = state.clone();
            assert!(engine::step(&mut state, Action::ActivateAbility(source, 0)).is_err());
            assert_eq!(state, before);
        }
    }
}

#[test]
fn kellan_wrong_condition_pays_and_real_upgrades_persist_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = put(&mut state, actor, "Kellan, Planar Trailblazer", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        settle(&mut state);
        assert_eq!(engine::effective_power(&state, source), 2);
        assert!(engine::has_effective_subtype(
            &state,
            source,
            Subtype::Scout
        ));
        activate(&mut state, actor, source, 1, 1, 2);
        settle(&mut state);
        assert!(state.objects.get(source).v4.creature_upgrade.is_none());
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        activate(&mut state, actor, source, 0, 1, 1);
        settle(&mut state);
        assert!(!engine::has_effective_subtype(
            &state,
            source,
            Subtype::Scout
        ));
        assert!(engine::has_effective_subtype(
            &state,
            source,
            Subtype::Detective
        ));
        activate(&mut state, actor, source, 1, 1, 2);
        state = restore(&state);
        settle(&mut state);
        assert!(!engine::has_effective_subtype(
            &state,
            source,
            Subtype::Detective
        ));
        assert!(engine::has_effective_subtype(
            &state,
            source,
            Subtype::Rogue
        ));
        assert_eq!(
            (
                engine::effective_power(&state, source),
                engine::effective_toughness(&state, source)
            ),
            (3, 2)
        );
        assert!(engine::has_effective_keyword(
            &state,
            source,
            Keywords::DOUBLE_STRIKE
        ));
        assert!(state
            .objects
            .get(source)
            .v4
            .creature_upgrade
            .as_ref()
            .unwrap()
            .combat_impulse
            .is_some());
        state.step = Step::Cleanup;
        next(&mut state);
        assert!(engine::has_effective_subtype(
            &state,
            source,
            Subtype::Rogue
        ));
        blink(&mut state, source);
        assert!(engine::has_effective_subtype(
            &state,
            source,
            Subtype::Scout
        ));
        assert!(!engine::has_effective_keyword(
            &state,
            source,
            Keywords::DOUBLE_STRIKE
        ));
    }
}

#[test]
fn actual_combat_impulse_uses_controller_library_and_survives_source_leave_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for depart in [false, true] {
            let mut state = ready(actor);
            let source = put(
                &mut state,
                actor,
                "Kellan, Planar Trailblazer",
                Zone::Battlefield,
            );
            // Physical setup models a controlled opponent-owned source; this is
            // not claimed as a card effect or a control-change qualification.
            state.objects.get_mut(source).owner = actor.opponent();
            activate(&mut state, actor, source, 0, 1, 1);
            settle(&mut state);
            let top = state.players[actor.index()].library[0];
            let owner_top = state.players[actor.opponent().index()].library[0];
            combat(&mut state, source, true);
            state = restore(&state);
            if let Some(pending) = state
                .engine
                .pending_triggers
                .iter()
                .find(|p| p.granted_by.is_some())
            {
                assert_eq!(pending.controller, actor);
                assert_eq!(pending.source_contract.unwrap().owner, actor.opponent());
            }
            next(&mut state);
            let trigger = state
                .stack
                .iter()
                .find(|item| item.v4.granted_by.is_some())
                .unwrap();
            assert_eq!(trigger.controller, actor);
            let captured = trigger.v4.granted_by.unwrap();
            assert_eq!(captured.source, source);
            assert_eq!(captured.owner, actor.opponent());
            assert_eq!(captured.controller, actor);
            // Restore validation must reject a forged physical owner in the
            // historical grant even when the live source and effect are real.
            let mut forged = restore(&state);
            forged
                .stack
                .iter_mut()
                .find(|item| item.v4.granted_by.is_some())
                .unwrap()
                .v4
                .granted_by
                .as_mut()
                .unwrap()
                .owner = actor;
            let mut halted = false;
            for _ in 0..8 {
                match engine::advance_until_decision(&mut forged) {
                    Decision::Halted { .. } => {
                        halted = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        let _ = engine::step(&mut forged, Action::Pass);
                    }
                    other => panic!("forged grant decision {other:?}"),
                }
            }
            assert!(halted, "forged source owner must fail closed before exile");
            assert_eq!(forged.objects.get(top).zone, Zone::Library);
            if depart {
                blink(&mut state, source);
            }
            let mut restored = restore(&state);
            settle(&mut state);
            settle(&mut restored);
            assert_eq!(state, restored);
            assert_eq!(state.objects.get(top).zone, Zone::Exile);
            assert_eq!(state.objects.get(owner_top).zone, Zone::Library);
            assert_eq!(state.players[actor.opponent().index()].life, 18);
            assert!(state
                .engine
                .exile_play_permissions
                .iter()
                .any(|p| p.object == top && p.holder == actor));
            // Finish this combat to the real postcombat main phase.
            for _ in 0..100 {
                match next(&mut state) {
                    Decision::CastSpellOrPass {
                        player, land_drops, ..
                    } if state.step == Step::Main2 && player == actor => {
                        assert!(land_drops.contains(&top));
                        engine::step(&mut state, Action::PlayLand(top)).unwrap();
                        assert_eq!(state.objects.get(top).zone, Zone::Battlefield);
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    other => panic!("postcombat {other:?}"),
                }
            }
            assert_eq!(state.objects.get(top).zone, Zone::Battlefield);
        }
    }
}

#[test]
fn unupgraded_combat_has_no_impulse_and_rogue_double_strike_exiles_twice_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for upgraded in [false, true] {
            let mut state = ready(actor);
            let source = put(
                &mut state,
                actor,
                "Kellan, Planar Trailblazer",
                Zone::Battlefield,
            );
            if upgraded {
                activate(&mut state, actor, source, 0, 1, 1);
                settle(&mut state);
                activate(&mut state, actor, source, 1, 1, 2);
                settle(&mut state);
            }
            let before = state.players[actor.index()].library.len();
            combat(&mut state, source, false);
            assert_eq!(
                state.players[actor.index()].library.len(),
                before - if upgraded { 2 } else { 0 }
            );
            assert_eq!(
                state.players[actor.opponent().index()].life,
                if upgraded { 14 } else { 18 }
            );
            assert_eq!(
                state.engine.exile_play_permissions.len(),
                if upgraded { 2 } else { 0 }
            );
            // Real cleanup expires play permission, not the persistent upgrade.
            for _ in 0..100 {
                if state.active_player != actor {
                    break;
                }
                match next(&mut state) {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    other => panic!("permission expiry {other:?}"),
                }
            }
            assert_ne!(state.active_player, actor);
            assert!(state.engine.exile_play_permissions.is_empty());
            assert_eq!(
                engine::has_effective_keyword(&state, source, Keywords::DOUBLE_STRIKE),
                upgraded
            );
        }
    }
}
