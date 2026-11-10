//! Native pre-turn Eternal opening actions. Real deal and London entry points.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::{
    card_def::card_id_by_name,
    engine::{self, Action, Decision},
    eternal_opening_v1::{enable_eternal_opening_v1, RecordV1},
    event::{self, CommittedEvent, ProposedEvent},
    ids::{ObjectId, PlayerId},
    state::{GameState, Step, Target, Zone},
};

const SEATS: [PlayerId; 2] = [PlayerId::P0, PlayerId::P1];
const VOID: &str = "Leyline of the Void";
const SANCTITY: &str = "Leyline of Sanctity";
const CHANCELLOR: &str = "Chancellor of the Annex";

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn deck(top: &[&str], fill: &str) -> Vec<u16> {
    let mut cards = vec![id(fill); 60];
    for (card, name) in cards.iter_mut().zip(top) {
        *card = id(name);
    }
    cards
}

fn deal(start: PlayerId, own: &[u16], other: &[u16]) -> GameState {
    let (zero, one) = if start == PlayerId::P0 {
        (own, other)
    } else {
        (other, own)
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        zero,
        one,
        mtg_kernel::rl::card_name,
        196730,
        start,
    );
    for _ in 0..7 {
        for player in SEATS {
            event::propose_and_commit(&mut state, ProposedEvent::draw(player));
        }
    }
    enable_eternal_opening_v1(&mut state).unwrap();
    state
}

fn keep(state: &mut GameState, player: PlayerId, mulligan: bool) {
    assert!(matches!(engine::advance_until_decision(state),
        Decision::ChooseLondonMulligan { player: actual, .. } if actual == player));
    engine::step(state, Action::ChooseLondonMulligan { mulligan }).unwrap();
}

fn choices(state: &mut GameState, player: PlayerId) -> Vec<ObjectId> {
    let decision = engine::advance_until_decision(state);
    let Decision::ChooseEternalOpeningV1 {
        player: actual,
        legal_cards,
    } = decision
    else {
        panic!("expected Eternal pregame menu, got {decision:?}");
    };
    assert_eq!(actual, player);
    assert_eq!(state.turn, 1);
    assert_eq!(state.step, Step::Untap);
    assert!(state.stack.is_empty());
    legal_cards
}

fn pick(state: &mut GameState, card: ObjectId) {
    engine::step(state, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
}

fn finish(state: &mut GameState) {
    engine::step(state, Action::FinishEffectSelection).unwrap();
}

fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn refuse_restored_first_spell(state: &GameState) {
    let mut restored = restore(state);
    let before = serde_json::to_value(&restored).unwrap();
    assert!(engine::step(&mut restored, Action::Pass).is_err());
    assert_eq!(serde_json::to_value(&restored).unwrap(), before);
    assert!(matches!(
        engine::advance_until_decision(&mut restored),
        Decision::Halted { .. }
    ));
}

#[test]
fn native_leyline_and_chancellor_order_restore_and_first_spell_both_seats() {
    for start in SEATS {
        let other = start.opponent();
        let mut state = deal(
            start,
            &deck(&[VOID, SANCTITY, "Mox Pearl"], "Island"),
            &deck(&[SANCTITY, CHANCELLOR, VOID], "Island"),
        );
        keep(&mut state, start, false);
        keep(&mut state, other, false);
        let own = choices(&mut state, start);
        assert_eq!(own.len(), 2);
        let other_hand = state.players[other.index()].hand.clone();
        let other_sanctity = other_hand[0];
        let chancellor = other_hand[1];
        let other_void = other_hand[2];
        let mox = state.players[start.index()].hand[2];
        let encoded = serde_json::to_value(&state).unwrap();
        assert!(engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(chancellor)),
        )
        .is_err());
        assert_eq!(serde_json::to_value(&state).unwrap(), encoded);
        state = restore(&state);
        assert_eq!(choices(&mut state, start), own);
        pick(&mut state, own[1]);
        assert_eq!(choices(&mut state, start), vec![own[0]]);
        pick(&mut state, own[0]);
        assert!(choices(&mut state, start).is_empty());
        assert_eq!(
            state.players[start.index()].battlefield,
            vec![own[1], own[0]]
        );
        finish(&mut state);
        assert_eq!(
            choices(&mut state, other),
            vec![other_sanctity, chancellor, other_void]
        );
        pick(&mut state, chancellor);
        assert!(state
            .known_hand_cards(start, other)
            .iter()
            .any(|entry| entry.object == chancellor));
        pick(&mut state, other_void);
        finish(&mut state);
        assert_eq!(state.objects.get(other_sanctity).zone, Zone::Hand);
        assert!(!state
            .known_hand_cards(start, other)
            .iter()
            .any(|entry| entry.object == other_sanctity));
        for (player, objects) in [(start, vec![own[1], own[0]]), (other, vec![other_void])] {
            for object in objects {
                let live = state.objects.get(object);
                assert_eq!(
                    (
                        live.zone,
                        live.owner,
                        live.controller,
                        live.zone_change_count
                    ),
                    (Zone::Battlefield, player, player, 2)
                );
            }
        }
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event, CommittedEvent::SpellCast { .. })));
        let mut suppressed = state.clone();
        let vintage = &mut suppressed.eternal_v1.as_mut().unwrap().vintage;
        let original_reveals = vintage.chancellor_reveals.clone();
        assert_eq!(original_reveals.len(), 1);
        vintage.cast_a_spell[start.index()] = true;
        vintage.chancellor_reveals.clear();
        refuse_restored_first_spell(&suppressed);
        state = restore(&state);
        let mut cast = false;
        let mut countered = false;
        for _ in 0..28 {
            match engine::advance_until_decision(&mut state) {
                Decision::CastSpellOrPass {
                    player,
                    castable_spells,
                    ..
                } => {
                    if !cast
                        && player == start
                        && castable_spells.contains(&mox)
                        && state.step == Step::Main1
                    {
                        state.players[start.index()].mana_pool[5] = 1;
                        engine::step(&mut state, Action::CastSpell(mox)).unwrap();
                        assert!(matches!(
                            engine::advance_until_decision(&mut state),
                            Decision::CastSpellOrPass { .. }
                        ));
                        let mut rearmed = state.clone();
                        let vintage = &mut rearmed.eternal_v1.as_mut().unwrap().vintage;
                        assert!(vintage.cast_a_spell[start.index()]);
                        assert!(vintage.chancellor_reveals.is_empty());
                        vintage.cast_a_spell[start.index()] = false;
                        vintage.chancellor_reveals.clone_from(&original_reveals);
                        refuse_restored_first_spell(&rearmed);
                        state = restore(&state);
                        cast = true;
                    } else {
                        if countered && state.stack.is_empty() {
                            break;
                        }
                        engine::step(&mut state, Action::Pass).unwrap();
                    }
                }
                Decision::ChooseEffectOption { player, .. } if cast && !countered => {
                    assert_eq!(player, start);
                    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
                    countered = true;
                }
                other => panic!("unexpected first-turn decision: {other:?}"),
            }
        }
        assert!(cast && countered);
        // The opposing Leyline of the Void applies to the actual countered spell.
        assert_eq!(state.objects.get(mox).zone, Zone::Exile);
        assert!(state.engine.event_history.iter().any(|event| {
            matches!(event, CommittedEvent::EternalOpeningV1 { record: RecordV1::Revealed(source) }
                if source.source == chancellor && source.owner == other)
        }));
    }
}

#[test]
fn native_opening_only_uses_kept_cards_and_refuses_tampered_owner_both_seats() {
    for start in SEATS {
        let other = start.opponent();
        let mut state = deal(start, &deck(&[], VOID), &deck(&[], "Island"));
        let mut premature = serde_json::to_value(&state).unwrap();
        premature["london_mulligans_v1"]["stage"] = serde_json::json!("Complete");
        let mut premature: GameState = serde_json::from_value(premature).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut premature),
            Decision::Halted { .. }
        ));
        keep(&mut state, start, true);
        keep(&mut state, other, false);
        let Decision::ChooseLondonBottom {
            player,
            remaining,
            candidates,
        } = engine::advance_until_decision(&mut state)
        else {
            panic!("expected the native London bottom");
        };
        assert_eq!((player, remaining), (start, 1));
        let bottom = candidates[1];
        engine::step(&mut state, Action::ChooseLondonBottom(bottom)).unwrap();
        keep(&mut state, start, false);
        let legal = choices(&mut state, start);
        assert_eq!(legal.len(), 6);
        assert!(!legal.contains(&bottom));
        assert!(engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(bottom))
        )
        .is_err());
        pick(&mut state, legal[3]);
        let mut altered = serde_json::to_value(&state).unwrap();
        altered
            .as_object_mut()
            .unwrap()
            .remove("eternal_opening_v1")
            .unwrap();
        let mut altered: GameState = serde_json::from_value(altered).unwrap();
        let before = serde_json::to_value(&altered).unwrap();
        assert!(engine::step(&mut altered, Action::FinishEffectSelection).is_err());
        assert_eq!(serde_json::to_value(&altered).unwrap(), before);
        assert!(matches!(
            engine::advance_until_decision(&mut altered),
            Decision::Halted { .. }
        ));
        finish(&mut state);
        assert!(choices(&mut state, other).is_empty());
        finish(&mut state);
    }
}

#[test]
fn native_opening_has_no_nonactor_menu_or_declined_public_identity_both_seats() {
    for start in SEATS {
        let other = start.opponent();
        let own = deck(&[VOID], "Island");
        let mut has_leyline = deal(start, &own, &deck(&[SANCTITY], "Island"));
        let mut has_land = deal(start, &own, &deck(&[], "Island"));
        for state in [&mut has_leyline, &mut has_land] {
            keep(state, start, false);
            keep(state, other, false);
        }
        assert_eq!(
            choices(&mut has_leyline, start),
            choices(&mut has_land, start)
        );
        for player in SEATS {
            let refusal = |state| {
                mtg_kernel::rl::observe_v1(state, player, 0)
                    .unwrap_err()
                    .to_string()
            };
            assert_eq!(refusal(&has_leyline), refusal(&has_land));
        }
        finish(&mut has_leyline);
        finish(&mut has_land);
        assert_eq!(choices(&mut has_leyline, other).len(), 1);
        assert!(choices(&mut has_land, other).is_empty());
        finish(&mut has_leyline);
        finish(&mut has_land);
        for state in [&mut has_leyline, &mut has_land] {
            assert!(state.known_hand_cards(start, other).is_empty());
            assert!(state
                .players
                .iter()
                .all(|player| player.battlefield.is_empty()));
            let decision = engine::advance_until_decision(state);
            assert!(
                matches!(decision, Decision::CastSpellOrPass { player, .. } if player == start)
            );
        }
        let disclosed = |state: &GameState| {
            state
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::EternalOpeningV1 {
                        record:
                            record @ (RecordV1::Entered { .. }
                            | RecordV1::Revealed(_)
                            | RecordV1::Finished(_)),
                    } => Some(record.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(disclosed(&has_leyline), disclosed(&has_land));
    }
}
