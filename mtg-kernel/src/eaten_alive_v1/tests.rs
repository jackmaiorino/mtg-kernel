use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v6::tests::put;
use crate::state::{GameState, Step, Target, Zone};

fn ready(actor: PlayerId) -> GameState {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 20],
        &[forest; 20],
        |id| {
            crate::card_def::CARD_DEFS[usize::from(id)]
                .object_name
                .into()
        },
        110_450,
        actor,
    );
    state.step = Step::Main1;
    state
}
fn creature(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let id = put(state, actor, "Llanowar Elves", Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Battlefield));
    id
}
fn fund(state: &mut GameState, actor: PlayerId, black: u8, colorless: u8) {
    state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = black;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = colorless;
}
fn bytes(state: &GameState) -> Vec<u8> {
    serde_json::to_vec(state).unwrap()
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&bytes(state)).unwrap()
}
fn refuses(state: &mut GameState, action: Action) {
    let before = bytes(state);
    assert!(engine::step(state, action).is_err());
    assert_eq!(bytes(state), before);
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn resolve(state: &mut GameState, source: ObjectId) {
    for _ in 0..64 {
        if !state.stack.iter().any(|item| item.source == source) {
            return;
        }
        match next(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            decision => panic!("unexpected response {decision:?}"),
        }
    }
    panic!("spell failed to leave stack");
}
fn target(
    state: &mut GameState,
    actor: PlayerId,
    source: ObjectId,
    recipient: ObjectId,
) -> Decision {
    assert_eq!(state.priority_player, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
    next(state)
}

#[test]
fn absent_definition_and_foreign_mandatory_stage_refuse() {
    // Universal registered physical source, not an Eaten Alive stand-in recipe.
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let spell = put(&mut state, actor, "Counterspell", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 4;
        engine::step(&mut state, Action::CastSpell(spell)).unwrap_err();
        assert!(exact_recipe(state.objects.get(spell).card_def).is_err());
        let value =
            crate::rules_meaning_eaten_alive_v1::printed_meaning(state.objects.get(spell).card_def);
        assert!(value.is_err());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_printed_cost_and_complete_root_forgery_refusal() {
    let id = crate::card_def::card_id_by_name("Eaten Alive").unwrap();
    exact_recipe(id).unwrap();
    let meaning = crate::rules_meaning_eaten_alive_v1::printed_meaning(id).unwrap();
    assert_eq!(meaning.printed_cost, "{B}");
    assert_eq!(meaning.additional_mana_cost, "{3}{B}");
    assert_eq!(meaning.unreduced_mana_branch_total, "{3}{B}{B}");
    crate::rules_meaning_eaten_alive_v1::validate(&meaning).unwrap();
    for key in [
        "additional_cost_mandatory",
        "sacrifice_current_controlled_creature",
        "sacrifice_is_target",
        "creature_or_planeswalker",
        "copies_pay_additional_cost",
        "frozen_model_admission",
    ] {
        let mut value = serde_json::to_value(&meaning).unwrap();
        value[key] = serde_json::Value::Bool(!value[key].as_bool().unwrap());
        let forged = serde_json::from_value(value).unwrap();
        assert!(crate::rules_meaning_eaten_alive_v1::validate(&forged).is_err());
    }
    let mut forged = meaning.clone();
    forged.additional_mana_cost = "{4}".into();
    assert!(crate::rules_meaning_eaten_alive_v1::validate(&forged).is_err());
    let mut forged = meaning;
    forged.additional_branches_ordered.reverse();
    assert!(crate::rules_meaning_eaten_alive_v1::validate(&forged).is_err());
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_both_branches_payment_restore_exile_and_receipt_refusal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay_mana in [false, true] {
            let mut state = ready(actor);
            let own = creature(&mut state, actor);
            let recipient = creature(&mut state, actor.opponent());
            // Sacrifice does not require an untapped creature or haste.
            state.objects.get_mut(own).tapped = true;
            state.objects.get_mut(own).summoning_sick = true;
            let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
            fund(&mut state, actor, 2, 3);
            let before_legacy = bytes(&state);
            assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
            assert_eq!(bytes(&state), before_legacy);
            assert!(matches!(
                target(&mut state, actor, spell, recipient),
                Decision::ChooseEffectOption {
                    option_count: 2,
                    ..
                }
            ));
            let mut erased = restore(&state);
            erased
                .engine
                .pending_cast
                .as_mut()
                .unwrap()
                .mandatory_additional_cost = None;
            refuses(&mut erased, Action::ChooseEffectOption(0));
            state = restore(&state);
            engine::step(&mut state, Action::ChooseEffectOption(u16::from(pay_mana))).unwrap();
            assert!(
                matches!(next(&mut state),Decision::CastSpellOrPass{player,..} if player==actor)
            );
            assert!(state.engine.pending_cast.is_none());
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
                if pay_mana { 0 } else { 1 }
            );
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                if pay_mana { 0 } else { 3 }
            );
            assert_eq!(
                state.objects.get(own).zone,
                if pay_mana {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
            assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
            let index = state
                .engine
                .event_history
                .iter()
                .position(|event| {
                    matches!(event, CommittedEvent::MandatoryCastPaymentInputV1 { .. })
                })
                .unwrap();
            let mut missing = restore(&state);
            missing.engine.event_history[index] = CommittedEvent::Tap { object: own };
            refuses(&mut missing, Action::Pass);
            let mut replay = restore(&state);
            let original = replay
                .engine
                .event_history
                .iter()
                .find(|event| matches!(event, CommittedEvent::MandatoryCastPaymentV1 { .. }))
                .unwrap()
                .clone();
            replay.engine.event_log.push(original);
            refuses(&mut replay, Action::Pass);
            state = restore(&state);
            resolve(&mut state, spell);
            assert_eq!(state.objects.get(recipient).zone, Zone::Exile);
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        }
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_mana_branch_needs_two_black_and_cost_reducer_changes_only_generic_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let recipient = creature(&mut state, actor.opponent());
        let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
        fund(&mut state, actor, 1, 4);
        refuses(&mut state, Action::CastSpell(spell));
        state.players[actor.index()].mana_pool = Default::default();
        fund(&mut state, actor, 2, 2);
        refuses(&mut state, Action::CastSpell(spell));
        // Actual registered reducer installed by a real battlefield entry.
        let sprite = put(&mut state, actor, "Mocking Sprite", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(sprite, Zone::Battlefield),
        );
        assert!(matches!(
            target(&mut state, actor, spell, recipient),
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        ));
        engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
        assert!(matches!(next(&mut state),Decision::CastSpellOrPass{player,..} if player==actor));
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
            0
        );
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            0
        );
        resolve(&mut state, spell);
        assert_eq!(state.objects.get(recipient).zone, Zone::Exile);
        assert_eq!(
            crate::rules_meaning_eaten_alive_v1::printed_meaning(state.objects.get(spell).card_def)
                .unwrap()
                .printed_cost,
            "{B}"
        );
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_target_sacrificed_as_cost_or_reentered_fizzles_without_refund_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for self_target in [false, true] {
            let mut state = ready(actor);
            let own = creature(&mut state, actor);
            let other = creature(&mut state, actor.opponent());
            let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
            fund(&mut state, actor, 2, 3);
            let recipient = if self_target { own } else { other };
            assert!(matches!(
                target(&mut state, actor, spell, recipient),
                Decision::ChooseEffectOption { .. }
            ));
            engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
            next(&mut state);
            assert_eq!(state.objects.get(own).zone, Zone::Graveyard);
            if !self_target {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(other, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(other, Zone::Battlefield),
                );
            }
            let before_pool = state.players[actor.index()].mana_pool;
            state = restore(&state);
            resolve(&mut state, spell);
            assert_eq!(state.players[actor.index()].mana_pool, before_pool);
            assert_eq!(
                state.objects.get(recipient).zone,
                if self_target {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
        }
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_planeswalker_target_and_an_offer_response_keep_paid_costs_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for counter in [false, true] {
            let mut state = ready(actor);
            let id = put(
                &mut state,
                actor.opponent(),
                "Ajani, Caller of the Pride",
                Zone::Hand,
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(id, Zone::Battlefield),
            );
            let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
            fund(&mut state, actor, 2, 3);
            assert!(
                matches!(target(&mut state,actor,spell,id),Decision::CastSpellOrPass{player,..} if player==actor)
            );
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
                0
            );
            if counter {
                let offer = put(
                    &mut state,
                    actor.opponent(),
                    "An Offer You Can't Refuse",
                    Zone::Hand,
                );
                state.players[actor.opponent().index()].mana_pool[ManaColor::U.pool_index()] = 1;
                engine::step(&mut state, Action::Pass).unwrap();
                assert!(
                    matches!(next(&mut state),Decision::CastSpellOrPass{player,..} if player==actor.opponent())
                );
                engine::step(&mut state, Action::CastSpell(offer)).unwrap();
                assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(spell))).unwrap();
                next(&mut state);
            }
            state = restore(&state);
            resolve(&mut state, spell);
            assert_eq!(
                state.objects.get(id).zone,
                if counter {
                    Zone::Battlefield
                } else {
                    Zone::Exile
                }
            );
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
                0
            );
        }
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_sacrifice_current_control_keeps_physical_owner_and_accepted_copy_pays_nothing_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let stolen = creature(&mut state, actor.opponent());
        let target = creature(&mut state, actor.opponent());
        // Accepted control primitive, not an additional printed control spell claim.
        crate::standard_cards_v1::gain_control(&mut state, stolen, actor);
        let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
        fund(&mut state, actor, 2, 3);
        assert!(matches!(
            self::target(&mut state, actor, spell, target),
            Decision::ChooseEffectOption { .. }
        ));
        engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
        next(&mut state);
        assert_eq!(state.objects.get(stolen).owner, actor.opponent());
        assert_eq!(state.objects.get(stolen).zone, Zone::Graveyard);
        assert!(state.players[actor.opponent().index()]
            .graveyard
            .contains(&stolen));
        let before_pool = state.players[actor.index()].mana_pool;
        // Accepted actual copy allocator from the genuinely paid original spell.
        // No additional arbitrary printed copy card is asserted.
        let (copy, copy_id) = engine::copy_spell_for(&mut state, spell, actor.opponent()).unwrap();
        assert!(state.stack.iter().any(|item| item.source == copy
            && item.v4.stack_item_id == copy_id
            && item.controller == actor.opponent()));
        state = restore(&state);
        resolve(&mut state, copy);
        assert_eq!(state.objects.get(target).zone, Zone::Exile);
        assert_eq!(state.players[actor.index()].mana_pool, before_pool);
        resolve(&mut state, spell);
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(
            state
                .engine
                .event_history
                .iter()
                .filter(|event| matches!(event, CommittedEvent::MandatoryCastPaymentInputV1 { .. }))
                .count(),
            1
        );
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_snakeskin_response_protects_target_without_refunding_mana_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let recipient = creature(&mut state, actor.opponent());
        let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
        fund(&mut state, actor, 2, 3);
        assert!(matches!(
            target(&mut state, actor, spell, recipient),
            Decision::CastSpellOrPass { .. }
        ));
        let veil = put(&mut state, actor.opponent(), "Snakeskin Veil", Zone::Hand);
        state.players[actor.opponent().index()].mana_pool[4] = 1;
        engine::step(&mut state, Action::Pass).unwrap();
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(veil)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
        next(&mut state);
        state = restore(&state);
        resolve(&mut state, veil);
        assert!(engine::has_effective_keyword(
            &state,
            recipient,
            Keywords::HEXPROOF
        ));
        resolve(&mut state, spell);
        assert_eq!(state.objects.get(recipient).zone, Zone::Battlefield);
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
            0
        );
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            0
        );
    }
}

#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_unpayable_selected_branch_refuses_without_rewinding_cast_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let first = creature(&mut state, actor);
        let second = creature(&mut state, actor);
        let recipient = creature(&mut state, actor.opponent());
        let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
        fund(&mut state, actor, 1, 0);
        assert!(
            matches!(target(&mut state,actor,spell,recipient),Decision::ChooseCostTargets{remaining:1,candidates,..} if candidates == [first,second])
        );
        assert_eq!(
            state
                .engine
                .pending_cast
                .as_ref()
                .unwrap()
                .mandatory_additional_cost
                .unwrap()
                .pay_mana,
            Some(false)
        );
        let original_history = state.engine.event_history.clone();
        for _ in 0..3 {
            let mut forged = restore(&state);
            forged
                .engine
                .pending_cast
                .as_mut()
                .unwrap()
                .mandatory_additional_cost
                .as_mut()
                .unwrap()
                .pay_mana = Some(true);
            assert_eq!(forged.engine.event_history, original_history);
            refuses(&mut forged, Action::Pass);
            let mut halted_only = forged.clone();
            assert!(matches!(
                engine::advance_until_decision(&mut forged),
                Decision::Halted {
                    mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation,
                    source: ObjectId(0)
                }
            ));
            halted_only.engine.halted = forged.engine.halted;
            assert_eq!(bytes(&forged), bytes(&halted_only));
            assert_eq!(forged.objects.get(spell).zone, Zone::Stack);
            assert!(forged.engine.pending_cast.is_some());
        }
        engine::step(&mut state, Action::ChooseCostTarget(first)).unwrap();
        assert!(matches!(next(&mut state),Decision::CastSpellOrPass{player,..} if player==actor));
        assert_eq!(state.objects.get(first).zone, Zone::Graveyard);
        resolve(&mut state, spell);
        assert_eq!(state.objects.get(recipient).zone, Zone::Exile);
    }
}

#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_finalized_publication_requires_unique_original_row_until_real_departure_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay_mana in [false, true] {
            let mut state = ready(actor);
            let own = creature(&mut state, actor);
            let recipient = creature(&mut state, actor.opponent());
            state.objects.get_mut(own).tapped = true;
            let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
            fund(&mut state, actor, 2, 3);
            assert!(matches!(
                target(&mut state, actor, spell, recipient),
                Decision::ChooseEffectOption {
                    option_count: 2,
                    ..
                }
            ));
            engine::step(&mut state, Action::ChooseEffectOption(u16::from(pay_mana))).unwrap();
            next(&mut state);
            let published = state.stack.last().unwrap().clone();
            let original_history = state.engine.event_history.clone();
            for _ in 0..3 {
                let mut forged = restore(&state);
                forged
                    .stack
                    .retain(|item| item.v4.stack_item_id != published.v4.stack_item_id);
                assert_eq!(forged.engine.event_history, original_history);
                assert_eq!(forged.objects.get(spell).zone, Zone::Stack);
                refuses(&mut forged, Action::Pass);
                let mut halted_only = forged.clone();
                assert!(matches!(
                    engine::advance_until_decision(&mut forged),
                    Decision::Halted { .. }
                ));
                halted_only.engine.halted = forged.engine.halted;
                assert_eq!(bytes(&forged), bytes(&halted_only));
                assert_eq!(forged.objects.get(recipient).zone, Zone::Battlefield);
            }
            let mut countered = restore(&state);
            // Accepted exact-ID counter primitive acquires the genuine row before
            // removal and commits its actual physical source departure.
            engine::counter_stack_item_by_id(&mut countered, published.v4.stack_item_id).unwrap();
            assert_eq!(countered.objects.get(spell).zone, Zone::Graveyard);
            crate::engine::mandatory_cast_cost_v1::validate_state(&restore(&countered)).unwrap();
            let mut replay = restore(&countered);
            replay.stack.push(published.clone());
            refuses(&mut replay, Action::Pass);
            state = restore(&state);
            resolve(&mut state, spell);
            assert_eq!(state.objects.get(recipient).zone, Zone::Exile);
            crate::engine::mandatory_cast_cost_v1::validate_state(&restore(&state)).unwrap();
        }
    }
}
