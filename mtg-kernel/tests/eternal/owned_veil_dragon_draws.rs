use super::*;
use mtg_kernel::effect::{EffectOp, PendingEffectChoice};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::state::StackItemKind;

fn scenario(drawer: PlayerId, count: usize) -> GameState {
    let mut state = game(&vec!["Island"; count]);
    state.active_player = drawer;
    state.priority_player = drawer;
    state
}

fn veil_count(state: &GameState) -> usize {
    state
        .eternal_v1
        .as_ref()
        .and_then(|eternal| eternal.e3.as_ref())
        .map_or(0, |e3| e3.veils.len())
}

fn history_draws(state: &GameState, drawer: PlayerId, start: usize) -> usize {
    state.engine.event_history[start..]
        .iter()
        .filter(|event| matches!(event, CommittedEvent::Draw { player, .. } if *player == drawer))
        .count()
}

fn discard_attempts(state: &GameState, card: ObjectId, start: usize) -> usize {
    state.engine.event_history[start..].iter().filter(|event| {
        matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Hand, .. } if *object == card)
    }).count()
}

fn owned_records(state: &GameState, start: usize) -> usize {
    state.engine.event_history[start..]
        .iter()
        .filter(|event| matches!(event, CommittedEvent::EternalOwnedDrawV1 { .. }))
        .count()
}

#[test]
fn paid_veil_draw_replacements_finish_before_protection_both_seats() {
    for drawer in [P0, P1] {
        let opponent = drawer.opponent();
        let mut base = scenario(drawer, 8);
        put(&mut base, opponent, "Hullbreacher", Zone::Battlefield);
        let thug = put(&mut base, drawer, "Golgari Thug", Zone::Graveyard);
        let veil = put(&mut base, drawer, "Veil of Summer", Zone::Hand);
        let library = base.players[drawer.index()].library.clone();
        let mut no_blue = base.clone();
        pool(&mut no_blue, drawer, &[ManaColor::G], 0);
        cast(&mut no_blue, veil, &[]);
        settled(&mut no_blue);
        assert_eq!(veil_count(&no_blue), 1);
        assert_eq!(no_blue.players[drawer.index()].library, library);
        assert_eq!(treasure_count(&no_blue, opponent), 0);
        assert_eq!(owned_records(&no_blue, 0), 0);

        let bolt = put(&mut base, drawer, "Lightning Bolt", Zone::Hand);
        pool(&mut base, drawer, &[ManaColor::R, ManaColor::G], 0);
        cast(&mut base, bolt, &[Target::Player(opponent)]);
        assert!(matches!(next(&mut base), Decision::CastSpellOrPass { player, .. } if player == drawer));
        act(&mut base, Action::Pass);
        let counter = put(&mut base, opponent, "Counterspell", Zone::Hand);
        pool(&mut base, opponent, &[ManaColor::U, ManaColor::U], 0);
        cast(&mut base, counter, &[Target::Object(bolt)]);
        assert!(matches!(next(&mut base), Decision::CastSpellOrPass { player, .. } if player == opponent));
        act(&mut base, Action::Pass);
        cast(&mut base, veil, &[]);
        let start = base.engine.event_history.len();
        let decision = settle(&mut base).expect("blue spell makes Veil draw");
        assert_draw_replacement_prompt(&decision, drawer, veil, thug);
        assert_eq!(veil_count(&base), 0);
        let saved = serde_json::to_vec(&base).unwrap();
        for dredge in [true, false] {
            let mut branch: GameState = serde_json::from_slice(&saved).unwrap();
            assert_eq!(next(&mut branch), decision);
            let answer = if dredge {
                Action::ChooseEffectTarget(Target::Object(thug))
            } else {
                Action::FinishEffectSelection
            };
            act(&mut branch, answer);
            let mut restored = restore_draw_answer(&branch);
            for state in [&mut branch, &mut restored] {
                settled(state);
                assert_eq!(veil_count(state), 1);
                assert_eq!(state.players[opponent.index()].life, 17, "Veil protects Bolt");
                assert_eq!(treasure_count(state, opponent), usize::from(!dredge));
                assert_eq!(state.players[drawer.index()].library, library[if dredge { 4 } else { 0 }..]);
                assert_eq!(history_draws(state, drawer, start), 0);
                assert_eq!(owned_records(state, start), 2);
            }
            assert_eq!(branch.state_hash(), restored.state_hash());
        }
    }
}

fn paid_dragon_entry(state: &mut GameState, drawer: PlayerId) -> ObjectId {
    let dragon = put(state, drawer, "Phyrexian Dragon Engine", Zone::Graveyard);
    pool(state, drawer, &[ManaColor::R, ManaColor::R], 3);
    activate(state, dragon, 0, &[]);
    pass_until(state, |state| {
        state.stack.last().is_some_and(|item| item.source == dragon
            && item.kind == StackItemKind::TriggeredAbility)
    });
    assert_eq!(state.objects.get(dragon).zone, Zone::Battlefield);
    assert_eq!(state.players[drawer.index()].mana_pool, [0; 6]);
    dragon
}

fn dragon_option(state: &mut GameState, drawer: PlayerId, dragon: ObjectId) -> Decision {
    let decision = settle(state).expect("Dragon Engine offers its original option");
    assert!(matches!(
        decision,
        Decision::ChooseEffectOption { player, source, option_count: 2 }
            if player == drawer && source == dragon
    ));
    decision
}

#[test]
fn paid_dragon_discards_once_then_offers_new_dredger_both_seats() {
    for drawer in [P0, P1] {
        let opponent = drawer.opponent();
        let mut ordinary = scenario(drawer, 8);
        let held = put(&mut ordinary, drawer, "Lightning Bolt", Zone::Hand);
        let dragon = paid_dragon_entry(&mut ordinary, drawer);
        let start = ordinary.engine.event_history.len();
        dragon_option(&mut ordinary, drawer, dragon);
        let mut decline = ordinary.clone();
        act(&mut decline, Action::ChooseEffectOption(1));
        settled(&mut decline);
        assert_eq!(decline.objects.get(held).zone, Zone::Hand);
        assert_eq!(history_draws(&decline, drawer, start), 0);
        act(&mut ordinary, Action::ChooseEffectOption(0));
        settled(&mut ordinary);
        assert_eq!(ordinary.objects.get(held).zone, Zone::Graveyard);
        assert_eq!(history_draws(&ordinary, drawer, start), 3);
        assert_eq!(owned_records(&ordinary, start), 0);

        let mut base = scenario(drawer, 8);
        put(&mut base, opponent, "Hullbreacher", Zone::Battlefield);
        let held = put(&mut base, drawer, "Lightning Bolt", Zone::Hand);
        let thug = put(&mut base, drawer, "Golgari Thug", Zone::Hand);
        let dragon = paid_dragon_entry(&mut base, drawer);
        let generation = base.objects.get(dragon).zone_change_count;
        let library = base.players[drawer.index()].library.clone();
        for departed in [false, true] {
            let mut state = base.clone();
            let start = state.engine.event_history.len();
            if departed {
                if state.priority_player != opponent {
                    act(&mut state, Action::Pass);
                }
                let bounce = put(&mut state, opponent, "Echoing Truth", Zone::Hand);
                pool(&mut state, opponent, &[ManaColor::U], 1);
                cast(&mut state, bounce, &[Target::Object(dragon)]);
                pass_until(&mut state, |state| {
                    state.objects.get(dragon).zone == Zone::Exile
                        && state.stack.last().is_some_and(|item| item.source == dragon)
                });
                assert!(state.objects.get(dragon).zone_change_count > generation);
            }
            let option = dragon_option(&mut state, drawer, dragon);
            assert_eq!(state.objects.get(thug).zone, Zone::Hand);
            let mut forged: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let Some(PendingEffectChoice::ChooseOption { options, .. }) = forged.engine.pending_effect.as_mut().unwrap().choice.as_mut() else {
                panic!("Dragon option missing");
            };
            options[0] = EffectOp::DrawCards { player: mtg_kernel::effect::PlayerRef::Controller, count: 3 };
            let hash = forged.state_hash();
            assert!(engine::step(&mut forged, Action::ChooseEffectOption(0)).is_err());
            assert_eq!(forged.state_hash(), hash);
            state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(next(&mut state), option);
            act(&mut state, Action::ChooseEffectOption(0));
            let decision = settle(&mut state).expect("the discarded Thug can now dredge");
            assert_draw_replacement_prompt(&decision, drawer, dragon, thug);
            assert_eq!(discard_attempts(&state, held, start), 1);
            assert_eq!(discard_attempts(&state, thug, start), 1);
            assert!(state.players[drawer.index()].hand.is_empty());
            let saved = serde_json::to_vec(&state).unwrap();
            for dredge in [true, false] {
                let mut branch: GameState = serde_json::from_slice(&saved).unwrap();
                assert_eq!(next(&mut branch), decision);
                let answer = if dredge {
                    Action::ChooseEffectTarget(Target::Object(thug))
                } else {
                    Action::FinishEffectSelection
                };
                act(&mut branch, answer);
                let mut restored = restore_draw_answer(&branch);
                for state in [&mut branch, &mut restored] {
                    if !dredge {
                        for _ in 0..2 {
                            let draw = settle(state).expect("later attempt can still dredge");
                            assert_draw_replacement_prompt(&draw, drawer, dragon, thug);
                            act(state, Action::FinishEffectSelection);
                        }
                    }
                    settled(state);
                    assert_eq!(discard_attempts(state, held, start), 1);
                    assert_eq!(discard_attempts(state, thug, start), 1);
                    assert_eq!(owned_records(state, start), 4);
                    assert_eq!(treasure_count(state, opponent), if dredge { 2 } else { 3 });
                    assert_eq!(state.players[drawer.index()].library, library[if dredge { 4 } else { 0 }..]);
                    assert_eq!(state.objects.get(thug).zone, if dredge { Zone::Hand } else { Zone::Graveyard });
                }
                assert_eq!(branch.state_hash(), restored.state_hash());
            }
        }
    }
}

fn cast_jace(state: &mut GameState, drawer: PlayerId) -> ObjectId {
    let jace = put(state, drawer, "Jace, Wielder of Mysteries", Zone::Hand);
    pool(state, drawer, &[ManaColor::U; 3], 1);
    cast(state, jace, &[]);
    settled(state);
    jace
}

#[test]
fn paid_veil_and_dragon_stop_on_jace_win_both_seats() {
    for drawer in [P0, P1] {
        let opponent = drawer.opponent();
        let mut veil_game = scenario(drawer, 0);
        let jace = cast_jace(&mut veil_game, drawer);
        act(&mut veil_game, Action::Pass);
        let ritual = put(&mut veil_game, opponent, "Dark Ritual", Zone::Hand);
        pool(&mut veil_game, opponent, &[ManaColor::B], 0);
        cast(&mut veil_game, ritual, &[]);
        assert!(matches!(next(&mut veil_game), Decision::CastSpellOrPass { player, .. } if player == opponent));
        act(&mut veil_game, Action::Pass);
        let veil = put(&mut veil_game, drawer, "Veil of Summer", Zone::Hand);
        for competing in [false, true] {
            let mut branch = veil_game.clone();
            if competing {
                put(&mut branch, opponent, "Hullbreacher", Zone::Battlefield);
            }
            pool(&mut branch, drawer, &[ManaColor::G], 0);
            cast(&mut branch, veil, &[]);
            let first = settle(&mut branch).unwrap();
            if competing {
                assert_draw_replacement_prompt(&first, drawer, veil, jace);
                act(&mut branch, Action::ChooseEffectTarget(Target::Object(jace)));
                branch = restore_draw_answer(&branch);
                assert!(matches!(next(&mut branch), Decision::GameOver { winner: Some(player) } if player == drawer));
                assert_eq!(treasure_count(&branch, opponent), 0);
            } else {
                assert!(matches!(first, Decision::GameOver { winner: Some(player) } if player == drawer));
            }
            assert_eq!(veil_count(&branch), 0);
        }

        let mut dragon_game = scenario(drawer, 4);
        cast_jace(&mut dragon_game, drawer);
        let thug = put(&mut dragon_game, drawer, "Golgari Thug", Zone::Hand);
        let dragon = paid_dragon_entry(&mut dragon_game, drawer);
        let start = dragon_game.engine.event_history.len();
        dragon_option(&mut dragon_game, drawer, dragon);
        act(&mut dragon_game, Action::ChooseEffectOption(0));
        let first = settle(&mut dragon_game).expect("discarded Thug can replace the first draw");
        assert_draw_replacement_prompt(&first, drawer, dragon, thug);
        act(&mut dragon_game, Action::ChooseEffectTarget(Target::Object(thug)));
        let mut restored = restore_draw_answer(&dragon_game);
        for state in [&mut dragon_game, &mut restored] {
            assert!(matches!(next(state), Decision::GameOver { winner: Some(player) } if player == drawer));
            assert_eq!(history_draws(state, drawer, start), 1, "third draw was skipped");
            assert_eq!(owned_records(state, start), 3);
            assert_eq!(discard_attempts(state, thug, start), 1);
            assert_eq!(state.objects.get(thug).zone, Zone::Hand);
        }
        assert_eq!(dragon_game.state_hash(), restored.state_hash());
    }
}
