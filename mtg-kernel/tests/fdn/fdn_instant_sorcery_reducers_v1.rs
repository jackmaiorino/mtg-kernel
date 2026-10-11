//! Controller instant/sorcery reductions across payment, casting and restore boundaries.
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
fn printed_cost_reducer_definitions_match_pinned_constructors() {
    for (name, generic, pips, power, toughness, subtypes, flying) in [
        (
            "Mocking Sprite",
            2,
            1,
            2,
            1,
            &[Subtype::Faerie, Subtype::Rogue][..],
            true,
        ),
        (
            "Archmage of Runes",
            3,
            2,
            3,
            6,
            &[Subtype::Giant, Subtype::Wizard][..],
            false,
        ),
    ] {
        let definition = &CARD_DEFS[card_id_by_name(name).unwrap() as usize];
        assert_eq!(definition.capability, CardCapability::Full);
        assert_eq!(definition.types, &[CardType::Creature]);
        assert_eq!(definition.subtypes, subtypes);
        assert_eq!(definition.colors, &[ManaColor::U]);
        assert_eq!(definition.cost.generic, generic);
        assert_eq!(definition.cost.pips, vec![Pip::Colored(ManaColor::U); pips]);
        assert_eq!(
            (definition.power, definition.toughness),
            (Some(power), Some(toughness))
        );
        assert_eq!(definition.keywords.has(Keywords::FLYING), flying);
    }
}

#[test]
fn discounts_stack_only_for_controller_and_never_remove_colored_pips() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for own in 0..=3 {
            let mut state = ready(player);
            for _ in 0..own {
                put(&mut state, player, "Mocking Sprite", Zone::Battlefield);
            }
            put(
                &mut state,
                player.opponent(),
                "Mocking Sprite",
                Zone::Battlefield,
            );
            let spell = put(&mut state, player, "Think Twice", Zone::Hand);
            state.players[player.index()].mana_pool[1] = 1;
            assert_eq!(offered(&mut state, spell), own != 0);
            if own != 0 {
                let mut no_blue = copy(&state);
                no_blue.players[player.index()].mana_pool = [0, 0, 0, 0, 0, 9];
                assert!(!offered(&mut no_blue, spell));
                engine::step(&mut state, Action::CastSpell(spell)).unwrap();
                next(&mut state);
                assert!(state.engine.pending_cast.is_none());
                assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
                settle(&mut state);
                assert_eq!(state.players[player.index()].hand.len(), 1);
            }
        }
    }
}

#[test]
fn creature_casts_keep_their_printed_generic_and_do_not_trigger_archmage() {
    let mut state = ready(PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Archmage of Runes",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Mocking Sprite",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Mocking Sprite", Zone::Hand);
    state.players[0].mana_pool = [0, 1, 0, 0, 0, 1];
    assert!(!offered(&mut state, spell));
    state.players[0].mana_pool[5] = 2;
    assert!(offered(&mut state, spell));
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    next(&mut state);
    assert_eq!(state.stack.len(), 1);
    assert!(state.engine.pending_triggers.is_empty());
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    settle(&mut state);
    assert!(state.players[0].hand.is_empty());
}

#[test]
fn archmage_draws_for_its_controllers_spell_above_the_spell_resolution() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(caster);
        let archmage = put(
            &mut state,
            PlayerId::P0,
            "Archmage of Runes",
            Zone::Battlefield,
        );
        let spell = put(&mut state, caster, "Think Twice", Zone::Hand);
        state.players[caster.index()].mana_pool = [0, 1, 0, 0, 0, u8::from(caster != PlayerId::P0)];
        assert!(offered(&mut state, spell));
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        next(&mut state);
        assert_eq!(state.stack.len(), 1 + usize::from(caster == PlayerId::P0));
        if caster == PlayerId::P0 {
            assert_eq!(state.stack.last().unwrap().source, archmage);
        }
        settle(&mut state);
        assert_eq!(
            state.players[caster.index()].hand.len(),
            1 + usize::from(caster == PlayerId::P0)
        );
        assert_eq!(state.players[caster.opponent().index()].hand.len(), 0);
    }
}

#[test]
fn witness_protection_removes_discount_until_its_exact_attachment_leaves() {
    let mut state = ready(PlayerId::P0);
    let sprite = put(
        &mut state,
        PlayerId::P0,
        "Mocking Sprite",
        Zone::Battlefield,
    );
    let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
    state.players[0].mana_pool[1] = 1;
    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(sprite))).unwrap();
    settle(&mut state);
    let spell = put(&mut state, PlayerId::P0, "Think Twice", Zone::Hand);
    state.players[0].mana_pool[1] = 1;
    assert!(!offered(&mut state, spell));
    assert!(!engine::has_effective_keyword(
        &state,
        sprite,
        Keywords::FLYING
    ));
    let mut restored = copy(&state);
    for game in [&mut state, &mut restored] {
        event::propose_and_commit(game, ProposedEvent::zone_change(aura, Zone::Graveyard));
        assert!(offered(game, spell));
        assert!(engine::has_effective_keyword(
            game,
            sprite,
            Keywords::FLYING
        ));
    }
    assert_eq!(
        serde_json::to_value(&state).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
}

#[test]
fn restored_kicker_choice_uses_discount_on_the_complete_cost_once() {
    let mut state = ready(PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Mocking Sprite",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    state.players[0].mana_pool = [0, 0, 0, 1, 0, 3];
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseKicker { .. }));
    let mut restored = copy(&state);
    for game in [&mut state, &mut restored] {
        engine::step(game, Action::ChooseKicker(true)).unwrap();
        assert!(matches!(next(game), Decision::ChooseTargets { .. }));
        engine::step(game, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();
        next(game);
        assert!(game.engine.pending_cast.is_none());
        assert!(
            game.stack
                .iter()
                .find(|item| item.source == spell)
                .unwrap()
                .kicked
        );
        assert_eq!(game.players[0].mana_pool, [0; 6]);
        settle(game);
        assert_eq!(game.players[1].life, 16);
    }
    assert_eq!(
        serde_json::to_value(&state).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
}

#[test]
fn restored_x_choice_applies_discount_after_x_and_keeps_black_pips() {
    let mut state = ready(PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Mocking Sprite",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Exsanguinate", Zone::Hand);
    state.players[0].mana_pool = [0, 0, 2, 0, 0, 3];
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 5,
            ..
        }
    ));
    let mut restored = copy(&state);
    for game in [&mut state, &mut restored] {
        engine::step(game, Action::ChooseEffectOption(4)).unwrap();
        next(game);
        assert!(game.engine.pending_cast.is_none());
        assert_eq!(game.players[0].mana_pool, [0; 6]);
        settle(game);
        assert_eq!((game.players[0].life, game.players[1].life), (24, 16));
    }
    assert_eq!(
        serde_json::to_value(&state).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
}
