//! FDN mandatory permanent recovery and optional targeted graveyard exile.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::observe_policy_v6;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 831);
    state.step = Step::Main1;
    enable_foundations_combat_v1(&mut state).unwrap();
    state
}
fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
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
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        _ => panic!("unsupported fixture zone"),
    }
    id
}
fn enter(state: &mut GameState, name: &str) -> ObjectId {
    let id = put(state, PlayerId::P0, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Battlefield));
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
    id
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn pass(state: &mut GameState, d: Decision) {
    match d {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        Decision::OrderTriggers { pending, .. } => {
            engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
        }
        other => panic!("unexpected decision {other:?}"),
    }
}
fn targets(state: &mut GameState) -> Decision {
    for _ in 0..32 {
        let d = next(state);
        if matches!(d, Decision::ChooseTargets { .. }) {
            return d;
        }
        assert!(
            !state.stack.is_empty() || !state.engine.pending_triggers.is_empty(),
            "missing target choice"
        );
        pass(state, d);
    }
    panic!("target choice did not appear")
}
fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        match d {
            Decision::ChooseTargets {
                can_finish: true, ..
            } => {
                engine::step(state, Action::FinishEffectSelection).unwrap();
            }
            other => pass(state, other),
        }
    }
    panic!("trigger did not finish")
}
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn refuse_unchanged(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

#[test]
fn printed_characteristics_costs_subtypes_and_flash_are_exact() {
    for (name, id, generic, green, power, toughness, subtypes) in [
        (
            "Elvish Regrower",
            334,
            2,
            2,
            4,
            3,
            vec![Subtype::Elf, Subtype::Druid],
        ),
        ("Ambush Wolf", 335, 2, 1, 4, 2, vec![Subtype::Wolf]),
    ] {
        let card = card_id_by_name(name).unwrap();
        // The serialized catalog order is qualified after v61-v63 integrate.
        assert_eq!(card, id);
        let def = &CARD_DEFS[card as usize];
        assert_eq!(def.types, &[CardType::Creature]);
        assert_eq!(def.subtypes, subtypes);
        assert_eq!((def.power, def.toughness), (Some(power), Some(toughness)));
        assert_eq!(def.cost.generic, generic);
        assert_eq!(def.cost.pips, vec![Pip::Colored(ManaColor::G); green]);
        assert_eq!(def.mana_value, u16::from(generic) + green as u16);
        assert_eq!(def.colors, &[ManaColor::G]);
        assert_eq!(def.keywords.has(Keywords::FLASH), name == "Ambush Wolf");
        let mut state = ready();
        let source = put(&mut state, PlayerId::P0, name, Zone::Hand);
        state.players[0].mana_pool[5] = generic - 1;
        state.players[0].mana_pool[ManaColor::G.pool_index()] = green as u8;
        refuse_unchanged(&mut state, Action::CastSpell(source));
        state.players[0].mana_pool[5] += 1;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    }
    let mut state = ready();
    state.step = Step::End;
    state.active_player = PlayerId::P1;
    let wolf = put(&mut state, PlayerId::P0, "Ambush Wolf", Zone::Hand);
    let regrower = put(&mut state, PlayerId::P0, "Elvish Regrower", Zone::Hand);
    state.players[0].mana_pool[5] = 2;
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 2;
    let Decision::CastSpellOrPass {
        castable_spells, ..
    } = next(&mut state)
    else {
        panic!("priority")
    };
    assert!(castable_spells.contains(&wolf));
    assert!(!castable_spells.contains(&regrower));
    engine::step(&mut state, Action::CastSpell(wolf)).unwrap();
    settle(&mut state);
    assert_eq!(state.objects.get(wolf).zone, Zone::Battlefield);
}

#[test]
fn regrower_targets_only_own_permanent_cards_including_lands() {
    for name in [
        "Forest",
        "Llanowar Elves",
        "Lembas",
        "Rancor",
        "Ajani, Caller of the Pride",
    ] {
        let mut state = ready();
        let wanted = put(&mut state, PlayerId::P0, name, Zone::Graveyard);
        let instant = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
        let theirs = put(&mut state, PlayerId::P1, "Forest", Zone::Graveyard);
        let token = put(
            &mut state,
            PlayerId::P0,
            "Elf Warrior Token",
            Zone::Graveyard,
        );
        let source = enter(&mut state, "Elvish Regrower");
        let Decision::ChooseTargets {
            legal_targets,
            can_finish,
            remaining,
            ..
        } = targets(&mut state)
        else {
            unreachable!()
        };
        assert_eq!(legal_targets, vec![Target::Object(wanted)]);
        assert_eq!((remaining, can_finish), (1, false));
        refuse_unchanged(&mut state, Action::FinishEffectSelection);
        for illegal in [instant, theirs, token] {
            refuse_unchanged(&mut state, Action::ChooseTarget(Target::Object(illegal)));
        }
        engine::step(&mut state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(wanted).zone, Zone::Hand);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    }
}

#[test]
fn regrower_no_legal_target_drops_trigger_and_foreign_ownership_does_not_widen_it() {
    for with_foreign in [false, true] {
        let mut state = ready();
        put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
        if with_foreign {
            let foreign = put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
            state.objects.get_mut(foreign).owner = PlayerId::P1;
        }
        enter(&mut state, "Elvish Regrower");
        settle(&mut state);
        assert!(state.engine.pending_triggers.is_empty());
    }
}

#[test]
fn wolf_can_decline_or_exile_any_card_from_either_graveyard_and_never_two() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let mut state = ready();
            let wanted = put(&mut state, owner, "Lightning Bolt", Zone::Graveyard);
            let second = put(&mut state, owner, "Forest", Zone::Graveyard);
            enter(&mut state, "Ambush Wolf");
            let Decision::ChooseTargets {
                legal_targets,
                remaining,
                can_finish,
                ..
            } = targets(&mut state)
            else {
                unreachable!()
            };
            assert_eq!((remaining, can_finish), (1, true));
            assert!(legal_targets.contains(&Target::Object(wanted)));
            assert!(legal_targets.contains(&Target::Object(second)));
            if accept {
                engine::step(&mut state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
                refuse_unchanged(&mut state, Action::ChooseTarget(Target::Object(second)));
            } else {
                engine::step(&mut state, Action::FinishEffectSelection).unwrap();
                refuse_unchanged(&mut state, Action::FinishEffectSelection);
            }
            settle(&mut state);
            assert_eq!(
                state.objects.get(wanted).zone,
                if accept { Zone::Exile } else { Zone::Graveyard }
            );
            assert_eq!(state.objects.get(second).zone, Zone::Graveyard);
        }
    }
    let mut state = ready();
    enter(&mut state, "Ambush Wolf");
    let Decision::ChooseTargets {
        legal_targets,
        can_finish,
        ..
    } = targets(&mut state)
    else {
        unreachable!()
    };
    assert!(legal_targets.is_empty() && can_finish);
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settle(&mut state);
}

#[test]
fn pending_optional_choice_and_finished_prefix_restore_without_changing_legacy_bytes() {
    for accept in [false, true] {
        let mut state = ready();
        let wanted = put(&mut state, PlayerId::P1, "Forest", Zone::Graveyard);
        enter(&mut state, "Ambush Wolf");
        let decision = targets(&mut state);
        assert!(!serde_json::to_string(&state.engine.pending_triggers[0])
            .unwrap()
            .contains("target_selection_finished"));
        let mut replay = restored(&state);
        assert_eq!(next(&mut replay), decision);
        let action = if accept {
            Action::ChooseTarget(Target::Object(wanted))
        } else {
            Action::FinishEffectSelection
        };
        engine::step(&mut state, action.clone()).unwrap();
        engine::step(&mut replay, action).unwrap();
        let mut after_answer = restored(&state);
        settle(&mut state);
        settle(&mut replay);
        settle(&mut after_answer);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&after_answer).unwrap()
        );
    }
}

#[test]
fn trigger_finish_cannot_answer_another_pending_effect_selection() {
    let mut state = ready();
    let lembas = put(&mut state, PlayerId::P0, "Lembas", Zone::Hand);
    state.players[0].mana_pool[5] = 2;
    engine::step(&mut state, Action::CastSpell(lembas)).unwrap();
    let mut saw_effect = false;
    for _ in 0..32 {
        let d = next(&mut state);
        if matches!(d, Decision::ChooseEffectTargets { .. }) {
            saw_effect = true;
            break;
        }
        pass(&mut state, d);
    }
    assert!(saw_effect && state.engine.pending_effect.is_some());
    // Fault injection: two otherwise real producers cannot share a finish answer.
    enter(&mut state, "Ambush Wolf");
    state.engine.pending_triggers[0].placement_ordered = true;
    refuse_unchanged(&mut state, Action::FinishEffectSelection);
}

#[test]
fn source_departure_does_not_cancel_and_leave_return_targets_fizzle() {
    for name in ["Elvish Regrower", "Ambush Wolf"] {
        for stale in [false, true] {
            let mut state = ready();
            let wanted = put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
            let source = enter(&mut state, name);
            targets(&mut state);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
            let d = next(&mut state);
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            assert!(!state.stack.is_empty());
            if stale {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(wanted, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(wanted, Zone::Graveyard),
                );
            }
            let mut replay = restored(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            let expected = if stale {
                Zone::Graveyard
            } else if name == "Elvish Regrower" {
                Zone::Hand
            } else {
                Zone::Exile
            };
            assert_eq!(state.objects.get(wanted).zone, expected);
            assert_eq!(state.objects.get(source).zone, Zone::Hand);
        }
    }
}

#[test]
fn target_and_returned_card_stay_public_without_exposing_other_hand_cards() {
    let mut state = ready();
    let wanted = put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
    put(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    enter(&mut state, "Elvish Regrower");
    targets(&mut state);
    let surface = PolicySurfaceV5::new();
    let observation = observe_policy_v6(&state, &surface, PlayerId::P0, 0, 0, 0, 1).unwrap();
    let payload = serde_json::to_string(&observation).unwrap();
    assert!(payload.contains("Forest"));
    assert!(!payload.contains("Counterspell"));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
    settle(&mut state);
    let opponent = observe_policy_v6(&state, &surface, PlayerId::P1, 0, 0, 0, 1).unwrap();
    assert!(opponent.known_hand_cards[0]
        .iter()
        .any(|card| card.stable.arena_id == wanted.0));
    assert_eq!(
        trigger::trigger_target_spec(card_id_by_name("Elvish Regrower").unwrap()),
        TargetSpec::PermanentCardInOwnGraveyard
    );
    assert_eq!(TargetSpec::PermanentCardInOwnGraveyard.stable_id(), 55);
    assert_eq!(TargetSpec::UpToOneCardInGraveyards.stable_id(), 54);
}
