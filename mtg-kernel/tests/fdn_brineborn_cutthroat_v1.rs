//! Source preparation. Registration and actual card qualification remain pending.
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
use mtg_kernel::state::{GameObject, GameState, ObjectLinkV4, ObjectStateV4, Step, Target, Zone};

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

#[test]
fn printed_definition_and_opponent_turn_flash_do_not_observe_own_cast() {
    let def = &CARD_DEFS[card_id_by_name("Brineborn Cutthroat").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Merfolk, Subtype::Pirate]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(2), Some(1), 2)
    );
    assert_eq!(def.colors, &[ManaColor::U]);
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert!(def.keywords.has(Keywords::FLASH));
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player.opponent());
        let source = put(&mut state, player, "Brineborn Cutthroat", Zone::Hand);
        priority(&mut state, player);
        state.players[player.index()].mana_pool[5] = 2;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if !castable_spells.contains(&source))
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state.players[player.index()].mana_pool[5] = 1;
        state.players[player.index()].mana_pool[1] = 1;
        cast(&mut state, source, None);
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 0);
        }
        same(&state, &replay);
    }
}

#[test]
fn only_controller_spells_on_opponent_turn_count_for_both_seats_and_spell_types() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        for caster in [owner, owner.opponent()] {
            for active in [owner, owner.opponent()] {
                for name in ["Lightning Bolt", "Brineborn Cutthroat"] {
                    let mut state = ready(active);
                    let source = put(&mut state, owner, "Brineborn Cutthroat", Zone::Battlefield);
                    let spell = put(&mut state, caster, name, Zone::Hand);
                    priority(&mut state, caster);
                    let target = if name == "Lightning Bolt" {
                        state.players[caster.index()].mana_pool[3] = 1;
                        Some(Target::Player(caster.opponent()))
                    } else {
                        state.players[caster.index()].mana_pool[1] = 1;
                        state.players[caster.index()].mana_pool[5] = 1;
                        None
                    };
                    cast(&mut state, spell, target);
                    let mut replay = restored(&state);
                    for current in [&mut state, &mut replay] {
                        settle(current);
                        assert_eq!(
                            current.objects.get(source).counters.plus1_plus1,
                            i32::from(caster == owner && active != owner)
                        );
                        if name == "Brineborn Cutthroat" {
                            assert_eq!(current.objects.get(spell).zone, Zone::Battlefield);
                            assert_eq!(current.objects.get(spell).counters.plus1_plus1, 0);
                        } else {
                            assert_eq!(current.players[caster.opponent().index()].life, 17);
                        }
                    }
                    same(&state, &replay);
                }
            }
        }
    }
}

#[test]
fn pending_counter_skips_departed_and_returned_source_after_restore() {
    let mut state = ready(PlayerId::P1);
    let source = put(
        &mut state,
        PlayerId::P0,
        "Brineborn Cutthroat",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    priority(&mut state, PlayerId::P0);
    state.players[0].mana_pool[3] = 1;
    cast(&mut state, spell, Some(Target::Player(PlayerId::P1)));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Battlefield),
    );
    let mut replay = restored(&state);
    for current in [&mut state, &mut replay] {
        settle(current);
        assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(current.objects.get(source).counters.plus1_plus1, 0);
        assert_eq!(current.players[1].life, 17);
    }
    same(&state, &replay);
}

#[test]
fn removing_abilities_preserves_pending_counter_and_stops_subsequent_triggers() {
    let mut state = ready(PlayerId::P1);
    let source = put(
        &mut state,
        PlayerId::P0,
        "Brineborn Cutthroat",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    priority(&mut state, PlayerId::P0);
    state.players[0].mana_pool[3] = 1;
    cast(&mut state, spell, Some(Target::Player(PlayerId::P1)));
    // Establish the real registered Aura relationship after the trigger exists.
    // The fixture isolates removal at this boundary without bypassing casting
    // timing by attempting to cast an Aura during its controller's opponent turn.
    let aura = put(
        &mut state,
        PlayerId::P1,
        "Witness Protection",
        Zone::Battlefield,
    );
    state.objects.get_mut(aura).v4.attached_to = Some(ObjectLinkV4 {
        object: source,
        zone_change_count: state.objects.get(source).zone_change_count,
    });
    state.objects.get_mut(source).attachments.push(aura);
    assert!(!engine::has_effective_keyword(
        &state,
        source,
        Keywords::FLASH
    ));
    let mut replay = restored(&state);
    for current in [&mut state, &mut replay] {
        settle(current);
        assert_eq!(current.objects.get(source).counters.plus1_plus1, 1);
        priority(current, PlayerId::P0);
        let second = put(current, PlayerId::P0, "Lightning Bolt", Zone::Hand);
        current.players[0].mana_pool[3] = 1;
        cast(current, second, Some(Target::Player(PlayerId::P1)));
        settle(current);
        assert_eq!(current.objects.get(source).counters.plus1_plus1, 1);
        assert_eq!(current.players[1].life, 14);
    }
    same(&state, &replay);
}
