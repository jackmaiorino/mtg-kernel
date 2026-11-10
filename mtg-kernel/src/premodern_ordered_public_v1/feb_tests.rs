use super::*;
use crate::{engine::Action, rl::ActionSemanticV1, surface::SurfaceAction};
use crate::{
    mana::ManaColor,
    state::{Step, Zone},
};

fn issued(
    owner: &FebSessionV1,
    viewer: PlayerSeatV1,
) -> (Box<FebNumericViewV1>, Option<FebDecisionV1>) {
    let FebCurrentV1::Decision { numeric, decision } = owner.current(viewer).unwrap() else {
        panic!(
            "FEB did not issue a decision: {:?}",
            owner.inner.current.as_ref().err()
        );
    };
    (numeric, decision)
}

#[test]
fn frozen_feb_opening_step_and_same_owner_restore_match_native_both_seats() {
    let deck = resolve_frozen_mainboard_v1().unwrap();
    let rows = frozen_mainboard_rows_v1().unwrap();
    assert_eq!(rows.len(), 27);
    assert_eq!(rows[0].name, "Brushland");
    assert_eq!(rows.last().unwrap().name, "Survival of the Fittest");
    assert_eq!(
        deck.as_slice(),
        rows.iter()
            .flat_map(|row| {
                std::iter::repeat_n(card_id_by_name(&row.name).unwrap(), row.count as usize)
            })
            .collect::<Vec<_>>()
    );
    let volrath = card_id_by_name("Volrath's Shapeshifter").unwrap();
    assert_eq!(deck.iter().filter(|&&id| id == volrath).count(), 2);
    assert!(crate::limited_session_v1::CustomDeckV1 {
        cards: rows.to_vec(),
        companion: None
    }
    .resolve()
    .is_err());

    for first in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let first_player = native_seat(first);
        let other = first_player.opponent().into();
        let mut owner = FebSessionV1::reset(460_615, first).unwrap();
        let foreign = FebSessionV1::reset(460_615, first).unwrap();
        let mut native = owner.inner.clone();
        assert_eq!(owner.inner.game.starting_player, first_player);
        assert!(owner
            .inner
            .game
            .players
            .iter()
            .all(|p| p.hand.len() == 7 && p.library.len() == 53));
        let (initial, token) = issued(&owner, first);
        let token = token.unwrap();
        assert_eq!(initial.actor, first);
        assert_eq!(initial.actions.len(), 2);
        let (nonactor, authority) = issued(&owner, other);
        assert!(authority.is_none() && nonactor.actions.is_empty());

        let saved = owner.snapshot();
        let (_, foreign_token) = issued(&foreign, first);
        assert!(owner.step(&foreign_token.unwrap(), 0).is_err());
        assert!(owner.restore(&foreign.snapshot()).is_err());
        assert!(owner.step(&token, usize::MAX).is_err());
        assert_eq!(owner.inner.game.state_hash(), saved.inner.game.state_hash());

        let native_opening = native.surface.next_decision(&mut native.game);
        let menu = rl::legal_action_candidates_v1(&native_opening, &native.game).unwrap();
        let keep = menu
            .iter()
            .position(|row| {
                matches!(
                    row.record.semantic,
                    ActionSemanticV1::ChooseLondonMulligan {
                        mulligan: false,
                        ..
                    }
                )
            })
            .unwrap();
        assert_eq!(
            menu[keep].surface_action,
            SurfaceAction::Action(Action::ChooseLondonMulligan { mulligan: false })
        );
        owner.step(&token, keep).unwrap();
        native
            .surface
            .apply(&mut native.game, menu[keep].surface_action.clone())
            .unwrap();
        let next = native.surface.next_decision(&mut native.game);
        assert_eq!(
            rl::acting_player_for_surface_decision(&next, &native.game),
            Some(first_player.opponent())
        );
        assert_eq!(owner.inner.game.state_hash(), native.game.state_hash());

        owner.restore(&saved).unwrap();
        assert!(owner.step(&token, keep).is_err());
        let (restored, fresh) = issued(&owner, first);
        assert_eq!(restored.observation, initial.observation);
        assert_eq!(restored.actions, initial.actions);
        owner.step(&fresh.unwrap(), keep).unwrap();
        assert_eq!(owner.inner.game.state_hash(), native.game.state_hash());

        let mut played_land = false;
        for _ in 0..32 {
            let native_decision = native.surface.next_decision(&mut native.game);
            let chooser =
                rl::acting_player_for_surface_decision(&native_decision, &native.game).unwrap();
            let menu = rl::legal_action_candidates_v1(&native_decision, &native.game).unwrap();
            let land = menu
                .iter()
                .position(|row| matches!(row.record.semantic, ActionSemanticV1::PlayLand { .. }));
            let ordinal = land
                .or_else(|| {
                    menu.iter().position(|row| {
                        matches!(
                            row.record.semantic,
                            ActionSemanticV1::Pass { .. }
                                | ActionSemanticV1::ChooseLondonMulligan {
                                    mulligan: false,
                                    ..
                                }
                        )
                    })
                })
                .unwrap_or_else(|| panic!("unhandled native opening menu {native_decision:?}"));
            let (numeric, token) = issued(&owner, chooser.into());
            assert_eq!(numeric.actions.len(), menu.len());
            owner.step(&token.unwrap(), ordinal).unwrap();
            native
                .surface
                .apply(&mut native.game, menu[ordinal].surface_action.clone())
                .unwrap();
            let _ = native.surface.next_decision(&mut native.game);
            assert_eq!(owner.inner.game.state_hash(), native.game.state_hash());
            if land.is_some() {
                played_land = true;
                break;
            }
        }
        assert!(played_land);
    }
}

#[test]
fn frozen_feb_opening_packet_is_inert_to_opponent_hidden_copy_assignment() {
    let owner = FebSessionV1::reset(460_616, PlayerSeatV1::P0).unwrap();
    let mut paired = FebSessionV1::reset(460_616, PlayerSeatV1::P0).unwrap();
    let state = &mut paired.inner.game;
    let hand = state.players[1].hand[0];
    let library = state.players[1]
        .library
        .iter()
        .copied()
        .find(|&id| state.objects.get(id).card_def != state.objects.get(hand).card_def)
        .unwrap();
    let a = state.objects.get(hand).card_def;
    let b = state.objects.get(library).card_def;
    for (id, definition) in [(hand, b), (library, a)] {
        let object = state.objects.get_mut(id);
        object.card_def = definition;
        object.name = rl::card_name(definition);
        object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
    }
    let (actual, _) = issued(&owner, PlayerSeatV1::P0);
    let (other, _) = issued(&paired, PlayerSeatV1::P0);
    assert_eq!(actual.observation, other.observation);
    assert_eq!(actual.actions, other.actions);
    assert_eq!(
        owner.human_current(PlayerSeatV1::P0).unwrap(),
        paired.human_current(PlayerSeatV1::P0).unwrap()
    );
}

fn choose_label(owner: &mut FebSessionV1, viewer: PlayerSeatV1, starts_with: &str) {
    let human = owner.human_current(viewer).unwrap().unwrap();
    let ordinal = human
        .actions
        .iter()
        .find_map(|row| match row {
            FebHumanActionV1::Supported {
                native_ordinal,
                label,
            } if label.starts_with(starts_with) => Some(*native_ordinal),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "missing Human action {starts_with:?} in {:?}",
                human.actions
            )
        });
    let (_, token) = issued(owner, viewer);
    owner.step_human(&token.unwrap(), ordinal).unwrap();
}

fn acting(owner: &FebSessionV1) -> PlayerSeatV1 {
    let Ok(BoundBoundaryV1::Decision(binding)) = &owner.inner.current else {
        panic!(
            "FEB fixture boundary absent: {:?}",
            owner.inner.current.as_ref().err()
        );
    };
    binding.actor.into()
}

fn stage_deck_card(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let id = card_id_by_name(name).unwrap();
    let objects = &state.players[player.index()];
    let object = objects
        .hand
        .iter()
        .chain(&objects.library)
        .copied()
        .find(|&object| state.objects.get(object).card_def == id)
        .unwrap();
    if state.objects.get(object).zone != zone {
        propose_and_commit(state, ProposedEvent::zone_change(object, zone));
    }
    object
}

fn main_phase_owner(seed: u64, seat: PlayerSeatV1) -> FebSessionV1 {
    let mut owner = FebSessionV1::reset(seed, seat).unwrap();
    choose_label(&mut owner, seat, "Keep this hand");
    choose_label(
        &mut owner,
        native_seat(seat).opponent().into(),
        "Keep this hand",
    );
    for _ in 0..16 {
        if owner.inner.game.step == Step::Main1 {
            break;
        }
        let viewer = acting(&owner);
        choose_label(&mut owner, viewer, "Pass priority");
    }
    assert_eq!(owner.inner.game.step, Step::Main1);
    owner
}

#[test]
fn frozen_feb_human_distinguishes_paid_krosan_reclamation_graveyard_targets() {
    for seat in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let p = native_seat(seat);
        let opponent = p.opponent();
        let mut owner = main_phase_owner(460_618, seat);
        let state = &mut owner.inner.game;
        stage_deck_card(state, p, "Krosan Reclamation", Zone::Hand);
        let lower = stage_deck_card(state, opponent, "Wall of Roots", Zone::Graveyard);
        let middle = stage_deck_card(state, opponent, "Palinchron", Zone::Graveyard);
        let upper = stage_deck_card(state, opponent, "Wall of Roots", Zone::Graveyard);
        assert_eq!(
            state.players[opponent.index()].graveyard,
            [lower, middle, upper]
        );
        state.players[p.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        state.players[p.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        owner.inner.current = bind_next(state, &mut owner.inner.surface, owner.inner.step);
        choose_label(&mut owner, seat, "Begin casting Krosan Reclamation");
        choose_label(
            &mut owner,
            seat,
            "Choose your opponent for Krosan Reclamation",
        );

        let human = owner.human_current(seat).unwrap().unwrap();
        for (native_ordinal, position) in [(0, 1), (2, 3)] {
            assert_eq!(
                human.actions[native_ordinal],
                FebHumanActionV1::Supported {
                    native_ordinal,
                    label: format!(
                        "Choose Wall of Roots ({position} from bottom of your opponent's graveyard) for Krosan Reclamation"
                    ),
                }
            );
        }
        let (_, token) = issued(&owner, seat);
        owner.step_human(&token.unwrap(), 2).unwrap();
        choose_label(&mut owner, seat, "Finish selecting");
        for _ in 0..8 {
            if owner.inner.game.stack.is_empty() {
                break;
            }
            let viewer = acting(&owner);
            choose_label(&mut owner, viewer, "Pass priority");
        }
        let state = &owner.inner.game;
        assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
        assert_eq!(state.players[opponent.index()].graveyard, [lower, middle]);
        assert_eq!(state.objects.get(upper).zone, Zone::Library);
    }
}

#[test]
fn frozen_feb_human_order_exposes_only_presentation_and_sealed_ordinals() {
    for seat in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let p = native_seat(seat);
        let mut owner = main_phase_owner(460_619, seat);
        let state = &mut owner.inner.game;
        let first = stage_deck_card(state, p, "Palinchron", Zone::Hand);
        let second = stage_deck_card(state, p, "Triskelion", Zone::Hand);
        let third = stage_deck_card(state, p, "Wall of Roots", Zone::Hand);
        crate::event::propose_and_commit_batch(
            state,
            vec![
                ProposedEvent::zone_change(first, Zone::Graveyard),
                ProposedEvent::zone_change(second, Zone::Graveyard),
                ProposedEvent::zone_change(third, Zone::Graveyard),
            ],
        );
        owner.inner.current = bind_next(state, &mut owner.inner.surface, owner.inner.step);

        let actor_view = owner.human_current(seat).unwrap().unwrap();
        let order = actor_view.ordered.order_choice.as_ref().unwrap();
        assert_eq!(
            order.action_labels,
            [
                "Place Palinchron next (choice 1)",
                "Place Triskelion next (choice 2)",
                "Place Wall of Roots next (choice 3)",
            ]
        );
        for rendered in [
            serde_json::to_string(&actor_view).unwrap(),
            format!("{actor_view:?}"),
        ] {
            assert!(!rendered.contains("arena_id"));
            assert!(!rendered.contains("zone_change_count"));
        }
        let other = owner.human_current(p.opponent().into()).unwrap().unwrap();
        assert!(other.ordered.order_choice.is_none() && other.actions.is_empty());

        choose_label(&mut owner, seat, "Place Triskelion next");
        let next = owner.human_current(seat).unwrap().unwrap();
        assert_eq!(
            next.ordered
                .order_choice
                .as_ref()
                .unwrap()
                .chosen_bottom_first,
            ["Triskelion"]
        );
        choose_label(&mut owner, seat, "Place Wall of Roots next");
        assert_eq!(
            owner.inner.game.players[p.index()].graveyard,
            [second, third, first]
        );
    }
}

#[test]
fn frozen_feb_human_paid_volrath_discard_and_wall_mana_both_seats() {
    for seat in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let p = native_seat(seat);
        for with_donor in [false, true] {
            let mut owner = main_phase_owner(460_617, seat);
            let source = stage_deck_card(
                &mut owner.inner.game,
                p,
                "Volrath's Shapeshifter",
                Zone::Hand,
            );
            let donor = with_donor.then(|| {
                stage_deck_card(&mut owner.inner.game, p, "Wall of Roots", Zone::Graveyard)
            });
            let mana = &mut owner.inner.game.players[p.index()].mana_pool;
            mana[ManaColor::U.pool_index()] = 2;
            mana[ManaColor::C.pool_index()] = if with_donor { 1 } else { 3 };
            owner.inner.current = bind_next(
                &mut owner.inner.game,
                &mut owner.inner.surface,
                owner.inner.step,
            );
            choose_label(&mut owner, seat, "Begin casting Volrath's Shapeshifter");
            for _ in 0..8 {
                if owner.inner.game.stack.is_empty() {
                    break;
                }
                let viewer = acting(&owner);
                choose_label(&mut owner, viewer, "Pass priority");
            }
            assert_eq!(owner.inner.game.objects.get(source).zone, Zone::Battlefield);
            let human = owner.human_current(seat).unwrap().unwrap();
            assert!(!serde_json::to_string(&human).unwrap().contains("arena_id"));
            assert!(owner
                .human_current(p.opponent().into())
                .unwrap()
                .unwrap()
                .actions
                .is_empty());
            let snapshot = owner.snapshot();
            let (_, stale) = issued(&owner, seat);
            owner.restore(&snapshot).unwrap();
            assert!(owner.step_human(&stale.unwrap(), 0).is_err());
            assert_eq!(owner.human_current(seat).unwrap().unwrap(), human);

            if let Some(donor) = donor {
                assert_eq!(
                    full_text_v1::active_text(&owner.inner.game, source)
                        .unwrap()
                        .donor()
                        .object,
                    donor
                );
                choose_label(
                    &mut owner,
                    seat,
                    "Put a -0/-1 counter on Volrath's Shapeshifter",
                );
                let state = &owner.inner.game;
                assert_eq!(
                    state.players[p.index()].mana_pool[ManaColor::G.pool_index()],
                    1
                );
                assert_eq!(state.objects.get(source).counters.minus0_minus1, 1);
                assert_eq!(state.objects.get(donor).counters.minus0_minus1, 0);
            } else {
                assert!(full_text_v1::active_text(&owner.inner.game, source).is_none());
                let before = owner.inner.game.players[p.index()].hand.len();
                choose_label(
                    &mut owner,
                    seat,
                    "Volrath's Shapeshifter: Pay {2} to discard a card",
                );
                for _ in 0..8 {
                    if owner.inner.game.engine.pending_discard.is_some() {
                        break;
                    }
                    let viewer = acting(&owner);
                    choose_label(&mut owner, viewer, "Pass priority");
                }
                choose_label(&mut owner, seat, "Discard ");
                assert_eq!(owner.inner.game.players[p.index()].hand.len(), before - 1);
                assert_eq!(owner.inner.game.players[p.index()].mana_pool, [0; 6]);
            }
        }
    }
}
