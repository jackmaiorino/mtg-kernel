//! Prepared v73 Aetherize cases, pending serial admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        730,
        player,
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
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("spell did not resolve")
}

fn cast(state: &mut GameState, caster: PlayerId, source: ObjectId) {
    state.players[caster.index()].mana_pool[5] = 3;
    state.players[caster.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.stack.len(), 1);
}

#[test]
fn aetherize_characteristics_and_untargeted_return_recipe_are_exact() {
    let id = card_id_by_name("Aetherize").unwrap();
    assert_eq!(id, 347);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Instant]);
    assert_eq!(def.mana_value, 4);
    assert_eq!(def.cost.generic, 3);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert!(matches!(
        (def.spell_effect)(),
        Some(EffectOp::ReturnAttackingCreaturesToOwnersHands)
    ));
}

#[test]
fn aetherize_returns_real_attackers_to_owners_with_pending_stack_restore() {
    for attacker_seat in [PlayerId::P0, PlayerId::P1] {
        let caster = attacker_seat.opponent();
        let mut state = ready(attacker_seat);
        let attacker = put(
            &mut state,
            attacker_seat,
            "Faerie Miscreant",
            Zone::Battlefield,
        );
        let borrowed = put(&mut state, caster, "Faerie Miscreant", Zone::Battlefield);
        state.objects.get_mut(borrowed).controller = attacker_seat;
        state.players[caster.index()]
            .battlefield
            .retain(|&id| id != borrowed);
        state.players[attacker_seat.index()]
            .battlefield
            .push(borrowed);
        let idle = put(
            &mut state,
            attacker_seat,
            "Faerie Miscreant",
            Zone::Battlefield,
        );
        state.objects.get_mut(idle).tapped = true;
        let spell = put(&mut state, caster, "Aetherize", Zone::Hand);
        state.step = Step::DeclareAttackers;
        assert!(matches!(
            next(&mut state),
            Decision::DeclareAttackers { .. }
        ));
        engine::step(
            &mut state,
            Action::DeclareAttackers(vec![attacker, borrowed]),
        )
        .unwrap();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == attacker_seat)
        );
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == caster)
        );
        cast(&mut state, caster, spell);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            settle(branch);
            assert_eq!(branch.objects.get(attacker).zone, Zone::Hand);
            assert!(branch.players[attacker_seat.index()]
                .hand
                .contains(&attacker));
            assert_eq!(branch.objects.get(borrowed).zone, Zone::Hand);
            assert!(branch.players[caster.index()].hand.contains(&borrowed));
            for seat in [PlayerId::P0, PlayerId::P1] {
                assert!(!branch.players[seat.index()].battlefield.contains(&attacker));
                assert!(!branch.players[seat.index()].battlefield.contains(&borrowed));
            }
            assert_eq!(branch.objects.get(idle).zone, Zone::Battlefield);
            assert!(!branch.engine.combat.attackers.contains(&attacker));
            assert!(!branch.engine.combat.attackers.contains(&borrowed));
            assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn aetherize_samples_current_creature_type_and_combat_at_resolution() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(caster);
        let departing = put(
            &mut state,
            caster.opponent(),
            "Faerie Miscreant",
            Zone::Battlefield,
        );
        let no_longer_creature = put(
            &mut state,
            caster.opponent(),
            "Faerie Miscreant",
            Zone::Battlefield,
        );
        state.engine.combat.attackers = vec![departing, no_longer_creature];
        let spell = put(&mut state, caster, "Aetherize", Zone::Hand);
        next(&mut state);
        cast(&mut state, caster, spell);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(departing, Zone::Hand),
        );
        let forest = card_id_by_name("Forest").unwrap();
        state.objects.get_mut(no_longer_creature).card_def = forest;
        state.objects.get_mut(no_longer_creature).v4 = ObjectStateV4::from_card_def(forest);
        let late = put(
            &mut state,
            caster.opponent(),
            "Faerie Miscreant",
            Zone::Battlefield,
        );
        state.engine.combat.attackers.push(late);
        settle(&mut state);
        assert_eq!(state.objects.get(departing).zone, Zone::Hand);
        assert_eq!(
            state.objects.get(no_longer_creature).zone,
            Zone::Battlefield
        );
        assert_eq!(state.objects.get(late).zone, Zone::Hand);
    }
}
