//! Regression tests for `effective_keywords` vs `has_effective_keyword` parity
//! under `eternal-fixtures` (Dash haste, Dragon's Rage Channeler delirium
//! flying, and Shadowspear hexproof/indestructible stripping).
use super::*;
use crate::card_def::{card_id_by_name, Keywords, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

fn def_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("missing card {name}"))
}

fn ready_game() -> GameState {
    let ids = vec![def_id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x454B_5041_5231,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = def_id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Graveyard => seat.graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Library => seat.library.insert(0, object),
        other => panic!("unexpected helper zone {other:?}"),
    }
    object
}

fn add_mana(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn settle_with<F>(state: &mut GameState, mut choose: F)
where
    F: FnMut(&GameState, &Decision) -> Action,
{
    for _ in 0..64 {
        let decision = advance_until_decision(state);
        assert!(
            !matches!(decision, Decision::Halted { .. }),
            "unexpected halt: {decision:?}"
        );
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_triggers.is_empty()
            && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == P0)
        {
            return;
        }
        let action = choose(state, &decision);
        step(state, action).unwrap_or_else(|err| panic!("step failed: {err:?}"));
    }
    panic!("state did not settle within 64 decisions");
}

#[test]
fn dashed_ragavan_effective_keywords_matches_has_effective_keyword() {
    let mut state = ready_game();
    let ragavan = put(&mut state, P0, "Ragavan, Nimble Pilferer", Zone::Hand);
    add_mana(&mut state, P0, &[ManaColor::R], 1);
    assert!(matches!(
        advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    step(&mut state, Action::CastSpell(ragavan)).unwrap();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::CastSpellOrPass { .. } => Action::Pass,
        other => panic!("unexpected decision while dashing Ragavan: {other:?}"),
    });
    assert_eq!(state.objects.get(ragavan).zone, Zone::Battlefield);
    assert_eq!(
        effective_keywords(&state, ragavan, Keywords::HASTE),
        Keywords::HASTE
    );
    assert!(effective_keywords(&state, ragavan, Keywords(u32::MAX)).has(Keywords::HASTE));

    // Normal cast (non-dashed) Ragavan must not have haste and must also pass
    // the cross-check.
    let mut normal_state = ready_game();
    let normal_ragavan = put(
        &mut normal_state,
        P0,
        "Ragavan, Nimble Pilferer",
        Zone::Hand,
    );
    add_mana(&mut normal_state, P0, &[ManaColor::R], 0);
    assert!(matches!(
        advance_until_decision(&mut normal_state),
        Decision::CastSpellOrPass { .. }
    ));
    step(&mut normal_state, Action::CastSpell(normal_ragavan)).unwrap();
    settle_with(&mut normal_state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Normal),
        Decision::CastSpellOrPass { .. } => Action::Pass,
        other => panic!("unexpected decision while casting Ragavan: {other:?}"),
    });
    assert_eq!(
        normal_state.objects.get(normal_ragavan).zone,
        Zone::Battlefield
    );
    assert_eq!(
        effective_keywords(&normal_state, normal_ragavan, Keywords::HASTE),
        Keywords::NONE
    );
    assert!(
        !effective_keywords(&normal_state, normal_ragavan, Keywords(u32::MAX)).has(Keywords::HASTE)
    );
}

#[test]
fn dragons_rage_channeler_delirium_effective_keywords_matches_has_effective_keyword() {
    let mut state = ready_game();
    let channeler = put(&mut state, P0, "Dragon's Rage Channeler", Zone::Battlefield);
    assert_eq!(
        effective_keywords(&state, channeler, Keywords::FLYING),
        Keywords::NONE
    );
    assert!(!effective_keywords(&state, channeler, Keywords(u32::MAX)).has(Keywords::FLYING));

    // Populate four distinct card types in P0's graveyard for delirium.
    put(&mut state, P0, "Island", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    put(&mut state, P0, "Chromatic Star", Zone::Graveyard);

    assert_eq!(
        effective_keywords(&state, channeler, Keywords::FLYING),
        Keywords::FLYING
    );
    assert!(effective_keywords(&state, channeler, Keywords(u32::MAX)).has(Keywords::FLYING));
}

#[test]
fn shadowspear_strip_effective_keywords_matches_has_effective_keyword() {
    let mut state = ready_game();
    let spear = put(&mut state, P0, "Shadowspear", Zone::Battlefield);
    let bridge = put(&mut state, P1, "Razortide Bridge", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let elves_zcc = state.objects.get(elves).zone_change_count;
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: elves,
            object_zone_change_count: elves_zcc,
            layer: Layers::ABILITY_ADDING,
            timestamp: 1,
            duration: EffectDuration::EndOfTurn,
            keywords: Keywords::HEXPROOF,
        });

    assert_eq!(
        effective_keywords(&state, bridge, Keywords::INDESTRUCTIBLE),
        Keywords::INDESTRUCTIBLE
    );
    assert!(effective_keywords(&state, bridge, Keywords(u32::MAX)).has(Keywords::INDESTRUCTIBLE));
    assert_eq!(
        effective_keywords(&state, elves, Keywords::HEXPROOF),
        Keywords::HEXPROOF
    );
    assert!(effective_keywords(&state, elves, Keywords(u32::MAX)).has(Keywords::HEXPROOF));

    add_mana(&mut state, P0, &[], 1);
    assert!(matches!(
        advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    step(&mut state, Action::ActivateAbility(spear, 0)).unwrap();
    settle_with(&mut state, |_, decision| match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        other => panic!("unexpected decision while activating Shadowspear: {other:?}"),
    });

    assert_eq!(
        effective_keywords(&state, bridge, Keywords::INDESTRUCTIBLE),
        Keywords::NONE
    );
    assert!(!effective_keywords(&state, bridge, Keywords(u32::MAX)).has(Keywords::INDESTRUCTIBLE));
    assert_eq!(
        effective_keywords(&state, elves, Keywords::HEXPROOF),
        Keywords::NONE
    );
    assert!(!effective_keywords(&state, elves, Keywords(u32::MAX)).has(Keywords::HEXPROOF));
}
