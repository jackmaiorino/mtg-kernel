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
fn frozen_feb_human_names_and_resolves_paid_cabal_therapy_both_seats() {
    use crate::effect::PendingEffectChoice;

    for seat in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let caster = native_seat(seat);
        let opponent = caster.opponent();
        let other_viewer = opponent.into();
        let mut owner = main_phase_owner(460_620, seat);
        let state = &mut owner.inner.game;
        let therapy = stage_deck_card(state, caster, "Cabal Therapy", Zone::Hand);
        let nonmatch = stage_deck_card(state, opponent, "Palinchron", Zone::Hand);
        let wall = card_id_by_name("Wall of Roots").unwrap();
        let target = &state.players[opponent.index()];
        let matching = target
            .hand
            .iter()
            .chain(&target.library)
            .copied()
            .filter(|&card| state.objects.get(card).card_def == wall)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 3);
        for &card in &matching {
            if state.objects.get(card).zone != Zone::Hand {
                propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
            }
        }
        state.players[caster.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        owner.inner.current = bind_next(state, &mut owner.inner.surface, owner.inner.step);
        choose_label(&mut owner, seat, "Begin casting Cabal Therapy");
        choose_label(&mut owner, seat, "Choose your opponent for Cabal Therapy");
        for _ in 0..8 {
            if owner.inner.game.engine.pending_effect.is_some() {
                break;
            }
            let viewer = acting(&owner);
            choose_label(&mut owner, viewer, "Pass priority");
        }
        let state = &owner.inner.game;
        assert_eq!(state.objects.get(therapy).zone, Zone::Stack);
        assert_eq!(state.players[caster.index()].mana_pool, [0; 6]);
        assert!(state.known_hand_cards(caster, opponent).is_empty());

        for expected in [
            "Choose names starting with W for Cabal Therapy",
            "Name Wall of Roots for Cabal Therapy",
        ] {
            let human = owner.human_current(seat).unwrap().unwrap();
            let (numeric, token) = issued(&owner, seat);
            let token = token.unwrap();
            assert_eq!(human.actions.len(), numeric.actions.len());
            assert!(human.actions.len() > 1);
            for (ordinal, row) in human.actions.iter().enumerate() {
                assert!(
                    matches!(row, FebHumanActionV1::Supported { native_ordinal, .. }
                    if *native_ordinal == ordinal)
                );
            }
            for rendered in [serde_json::to_string(&human).unwrap(), format!("{human:?}")] {
                assert!(!rendered.contains("arena_id"));
                assert!(!rendered.contains("zone_change_count"));
                assert!(!rendered.contains("TherapyV1"));
            }
            let other = owner.human_current(other_viewer).unwrap().unwrap();
            assert!(other.actions.is_empty());
            let (other, other_token) = issued(&owner, other_viewer);
            assert!(other.actions.is_empty() && other_token.is_none());

            let snapshot = owner.snapshot();
            let Some(PendingEffectChoice::ChooseOption { options, .. }) = owner
                .inner
                .game
                .engine
                .pending_effect
                .as_mut()
                .and_then(|pending| pending.choice.as_mut())
            else {
                panic!("Cabal Therapy did not reach a native naming stage");
            };
            options.swap(0, 1);
            let reordered = owner.human_current(seat).unwrap().unwrap();
            assert_eq!(reordered.actions.len(), human.actions.len());
            assert!(reordered.actions.iter().all(|row| matches!(
                row,
                FebHumanActionV1::Unsupported { action_kind, .. }
                    if action_kind == "choose_effect_option"
            )));
            assert!(owner.step_human(&token, 0).is_err());

            owner.restore(&snapshot).unwrap();
            assert!(owner.step_human(&token, 0).is_err());
            assert_eq!(owner.human_current(seat).unwrap().unwrap(), human);
            assert_eq!(issued(&owner, seat).0.actions, numeric.actions);
            choose_label(&mut owner, seat, expected);
        }

        let order = crate::graveyard_order_v1::public_boundary(&owner.inner.game, opponent)
            .unwrap()
            .unwrap()
            .choice
            .unwrap();
        assert_eq!(order.native_candidates.len(), matching.len());
        let expected_order =
            [1, 2, 0].map(|index| ObjectId(order.native_candidates[index].arena_id));
        // Choose the second available card twice, leaving the first card forced last.
        for _ in 0..2 {
            assert_eq!(acting(&owner), other_viewer);
            let (_, token) = issued(&owner, other_viewer);
            owner.step_human(&token.unwrap(), 1).unwrap();
        }
        let state = &owner.inner.game;
        assert!(state.stack.is_empty());
        assert_eq!(state.objects.get(therapy).zone, Zone::Graveyard);
        let target = &state.players[opponent.index()];
        assert_eq!(target.graveyard, expected_order);
        for card in matching {
            assert!(state.engine.event_history.iter().any(|event| matches!(
                event,
                crate::event::CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Hand,
                    to: Zone::Graveyard,
                    ..
                } if *object == card
            )));
        }
        assert!(target.hand.contains(&nonmatch));
        assert!(state
            .known_hand_cards(caster, opponent)
            .iter()
            .any(|card| card.object == nonmatch));
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

#[derive(Debug, PartialEq, Eq)]
struct FebReplayStep {
    step: u64,
    actor: PlayerSeatV1,
    menu_digest: [u8; 32],
    native_ordinal: usize,
}

fn feb_menu_digest(numeric: &FebNumericViewV1, human: &FebHumanViewV1) -> [u8; 32] {
    let numeric_menu: Vec<_> = numeric
        .actions
        .iter()
        .map(|tree| (tree.rows(), tree.topology()))
        .collect();
    let payload = serde_json::to_vec(&(&human.actions, numeric_menu)).unwrap();
    Sha256::digest(payload).into()
}

fn feb_action_rank(label: &str) -> u8 {
    if label == "Keep this hand"
        || label.starts_with("Play ")
        || (label.starts_with("Attack with ") && label != "Attack with none")
        || (label.starts_with("Block ") && !label.ends_with(" with none"))
    {
        0
    } else if label.starts_with("Begin casting ") {
        1
    } else if label == "Pass priority" {
        3
    } else if label == "Take a mulligan"
        || label.starts_with("Activate the mana ability")
        || label.contains(": ")
        || label.starts_with("Tap ")
        || label.starts_with("Put a -0/-1 counter")
    {
        4
    } else {
        2
    }
}

fn play_frozen_feb_mirror(
    starting: PlayerSeatV1,
    expected: Option<&[FebReplayStep]>,
) -> (FebSessionV1, Option<PlayerSeatV1>, Vec<FebReplayStep>) {
    const SEED: u64 = 20_261_108;
    // Match the Eternal catalog harness's physical-decision cap.
    const MAX_ACTIONS: u64 = 65_536;
    let mut owner = FebSessionV1::reset(SEED, starting).unwrap();
    let mut trace = Vec::new();
    let mut restored = false;
    for step in 0..=MAX_ACTIONS {
        let at = || format!("FEB seed {SEED} starting {starting:?} step {step}");
        let (numeric, decision) = match owner.current(PlayerSeatV1::P0) {
            Ok(FebCurrentV1::Decision { numeric, decision }) => (numeric, decision),
            Ok(FebCurrentV1::GameOver { winner }) => {
                assert!(matches!(
                    owner.current(PlayerSeatV1::P1),
                    Ok(FebCurrentV1::GameOver { winner: other }) if winner == other
                ));
                assert!(
                    restored || expected.is_some(),
                    "{} reached terminal without restoring a cast boundary",
                    at()
                );
                if let Some(expected) = expected {
                    assert_eq!(trace.len(), expected.len(), "{}", at());
                }
                return (owner, winner, trace);
            }
            Ok(FebCurrentV1::NativeHalt) => {
                panic!("{} native halt: {:?}", at(), owner.inner.game.engine.halted);
            }
            Ok(FebCurrentV1::UnsupportedDecision) => {
                panic!(
                    "{} unsupported decision: {:?}",
                    at(),
                    owner.inner.current.as_ref().err()
                );
            }
            Err(error) => panic!("{} numeric view: {error}", at()),
        };
        assert!(step < MAX_ACTIONS, "{} exceeded action cap", at());
        let actor = numeric.actor;
        let (numeric, token) = if let Some(token) = decision {
            assert_eq!(actor, PlayerSeatV1::P0, "{}", at());
            (numeric, token)
        } else {
            assert_eq!(actor, PlayerSeatV1::P1, "{}", at());
            assert!(numeric.actions.is_empty(), "{}", at());
            let (actor_view, token) = issued(&owner, actor);
            (
                actor_view,
                token.expect("actor receives sealed FEB decision"),
            )
        };
        let human = owner
            .human_current(actor)
            .unwrap_or_else(|error| panic!("{} Human view: {error}", at()))
            .expect("actor has a FEB Human decision");
        assert_eq!((human.step, human.actor), (step, actor), "{}", at());
        assert_eq!((numeric.step, numeric.actor), (step, actor), "{}", at());
        assert_eq!(numeric.actions.len(), human.actions.len(), "{}", at());

        if step == 0 {
            assert_eq!(actor, starting, "{}", at());
        }
        let labels: Vec<_> = human
            .actions
            .iter()
            .enumerate()
            .map(|(ordinal, row)| {
                let FebHumanActionV1::Supported {
                    native_ordinal,
                    label,
                } = row
                else {
                    panic!("{} unsupported Human row: {row:?}", at());
                };
                assert_eq!(*native_ordinal, ordinal, "{}", at());
                label.as_str()
            })
            .collect();
        let recorded = expected.map(|steps| {
            steps
                .get(step as usize)
                .unwrap_or_else(|| panic!("{} replay continued past recorded terminal", at()))
        });
        let ordinal = if let Some(recorded) = recorded {
            recorded.native_ordinal
        } else {
            labels
                .iter()
                .enumerate()
                .min_by_key(|&(ordinal, label)| {
                    // Native attack subsets are ordered from smaller masks to
                    // larger masks. Prefer the last legal nonempty group so a
                    // first zero-power attacker cannot starve later attackers.
                    let tie = if label.starts_with("Attack with ") && *label != "Attack with none" {
                        usize::MAX - ordinal
                    } else {
                        ordinal
                    };
                    (feb_action_rank(label), tie)
                })
                .expect("FEB decision has a legal action")
                .0
        };
        let label = *labels
            .get(ordinal)
            .unwrap_or_else(|| panic!("{} recorded ordinal {ordinal} is outside the menu", at()));
        let receipt = FebReplayStep {
            step,
            actor,
            menu_digest: feb_menu_digest(&numeric, &human),
            native_ordinal: ordinal,
        };
        if let Some(recorded) = recorded {
            assert_eq!(&receipt, recorded, "{}", at());
        }
        let checkpoint = (recorded.is_none() && !restored && label.starts_with("Begin casting "))
            .then(|| owner.snapshot());
        let result = if recorded.is_some() {
            owner.step(&token, ordinal)
        } else {
            owner.step_human(&token, ordinal)
        };
        result.unwrap_or_else(|error| panic!("{} action {ordinal} {label:?}: {error}", at()));

        if let Some(checkpoint) = checkpoint {
            let next_hash = owner.inner.game.state_hash();
            let next_human = owner.human_current(PlayerSeatV1::P0).unwrap();
            owner.restore(&checkpoint).unwrap();
            assert!(owner.step(&token, ordinal).is_err());
            let (reset_view, fresh) = issued(&owner, actor);
            assert_eq!(reset_view.observation, numeric.observation, "{}", at());
            assert_eq!(reset_view.actions, numeric.actions, "{}", at());
            assert_eq!(owner.human_current(actor).unwrap().as_ref(), Some(&human));
            owner.step(&fresh.unwrap(), ordinal).unwrap();
            assert_eq!(owner.inner.game.state_hash(), next_hash, "{}", at());
            assert_eq!(owner.human_current(PlayerSeatV1::P0).unwrap(), next_human);
            restored = true;
        }
        trace.push(receipt);
    }
    unreachable!("FEB terminal or action cap checked on every iteration")
}

#[test]
fn frozen_feb_mirrors_reach_native_terminal_and_replay_both_starting_seats() {
    use crate::event::CommittedEvent;

    for starting in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let (actual, winner, trace) = play_frozen_feb_mirror(starting, None);
        let history = &actual.inner.game.engine.event_history;
        for player in [PlayerId::P0, PlayerId::P1] {
            assert!(
                history.iter().any(|event| matches!(
                    event,
                    CommittedEvent::SpellCast { controller, .. } if *controller == player
                )),
                "FEB starting {starting:?} had no native cast for {player:?}"
            );
        }
        assert!(
            history.iter().any(|event| matches!(
                event,
                CommittedEvent::CombatDamageToPlayer { amount, .. } if *amount > 0
            )),
            "FEB starting {starting:?} had no actual combat damage to a player"
        );

        let (replay, replay_winner, replay_trace) = play_frozen_feb_mirror(starting, Some(&trace));
        assert_eq!(winner, replay_winner, "FEB starting {starting:?}");
        assert_eq!(trace, replay_trace, "FEB starting {starting:?}");
        assert_eq!(
            actual.inner.game.state_hash(),
            replay.inner.game.state_hash()
        );
        assert!(
            history == &replay.inner.game.engine.event_history,
            "FEB starting {starting:?} replay changed native event receipts"
        );
    }
}
