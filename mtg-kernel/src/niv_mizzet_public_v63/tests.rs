use super::*;
use crate::{
    effect::EffectOp,
    engine::{self, Action, Decision},
    event::CommittedEvent,
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    policy_observation_v6::tests::{put, ready_state_for},
    state::{GameState, Target, Zone},
};
// Genuine paid fixture helpers from the existing native Niv source. These use
// the actual announced spell, selected target and native Advance boundaries.
fn finish(state: &mut GameState) {
    for _ in 0..96 {
        match engine::advance_until_decision(state) {
            Decision::OrderTriggers { pending, .. } => engine::step(
                state,
                Action::OrderTriggers((0..pending.len()).rev().collect()),
            )
            .unwrap(),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("Niv fixture unexpected decision while finishing: {other:?}"),
        }
    }
    panic!("Niv fixture stack did not finish");
}

fn cast_niv(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let responder = actor.opponent();
    let mut state = ready_state_for(actor);
    for player in [actor, responder] {
        for _ in 0..20 {
            put(&mut state, player, "Island", Zone::Library);
        }
    }
    for _ in 0..8 {
        put(&mut state, actor, "Island", Zone::Hand);
    }
    // These are ordinary battlefield setup permanents. The card under test,
    // the damage spell, and the control response below use real cast routes.
    let archer = put(&mut state, actor, "Firebrand Archer", Zone::Battlefield);
    let fae = put(
        &mut state,
        responder,
        "High Fae Trickster",
        Zone::Battlefield,
    );
    let niv = put(&mut state, actor, "Niv-Mizzet, Visionary", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[responder.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    state.players[responder.index()].mana_pool[ManaColor::B.pool_index()] = 2;
    engine::step(&mut state, Action::CastSpell(niv)).unwrap();
    finish(&mut state);
    assert_eq!(state.objects.get(niv).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(niv).controller, actor);
    assert_eq!(state.objects.get(archer).controller, actor);
    assert_eq!(state.objects.get(fae).controller, responder);
    (state, niv, archer, fae)
}

fn top_niv_damage_amount(state: &GameState) -> Option<i32> {
    let item = state.stack.last()?;
    let Some(EffectOp::NivMizzetVisionaryDrawV1 {
        damage_history_index,
    }) = item.inline_effect.as_ref()
    else {
        return None;
    };
    match state
        .engine
        .event_history
        .get(usize::try_from(*damage_history_index).ok()?)?
    {
        CommittedEvent::Damage {
            target: Target::Player(_),
            amount,
            ..
        } => Some(*amount),
        _ => None,
    }
}

fn cast_bolt(actor: PlayerId, state: &mut GameState) {
    let bolt = put(state, actor, "Lightning Bolt", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(state, Action::CastSpell(bolt)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseTargets { spell, .. } if spell == bolt
    ));
    engine::step(
        state,
        Action::ChooseTarget(Target::Player(actor.opponent())),
    )
    .unwrap();
}

fn archer_trigger_on_stack(
    actor: PlayerId,
    archer: ObjectId,
    state: &mut GameState,
) -> crate::state::StackItem {
    let bolt = put(state, actor, "Lightning Bolt", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(state, Action::CastSpell(bolt)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseTargets { spell, .. } if spell == bolt
    ));
    engine::step(
        state,
        Action::ChooseTarget(Target::Player(actor.opponent())),
    )
    .unwrap();
    let pending = match engine::advance_until_decision(state) {
        Decision::OrderTriggers { pending, .. } => pending,
        decision => panic!("expected Archer cast trigger ordering, got {decision:?}"),
    };
    let archer_index = pending
        .iter()
        .position(|trigger| trigger.source == archer)
        .expect("Firebrand Archer cast trigger");
    let mut order: Vec<_> = (0..pending.len())
        .filter(|index| *index != archer_index)
        .collect();
    order.push(archer_index);
    engine::step(state, Action::OrderTriggers(order)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { player, .. } if player == actor
    ));
    let archer_trigger = state.stack.last().expect("Archer trigger on stack").clone();
    assert_eq!(archer_trigger.source, archer);
    assert_eq!(archer_trigger.controller, actor);
    archer_trigger
}

fn pass_until_archer_trigger_returns(
    state: &mut GameState,
    archer_trigger: &crate::state::StackItem,
) {
    for _ in 0..48 {
        if state
            .stack
            .last()
            .is_some_and(|item| item.v4.stack_item_id == archer_trigger.v4.stack_item_id)
        {
            return;
        }
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => engine::step(
                state,
                Action::OrderTriggers((0..pending.len()).rev().collect()),
            )
            .unwrap(),
            other => panic!("unexpected decision while response resolves: {other:?}"),
        }
    }
    panic!("response did not leave the original Archer trigger on top");
}

fn remove_archer_before_its_trigger_damage(
    actor: PlayerId,
    archer: ObjectId,
    state: &mut GameState,
) {
    let responder = actor.opponent();
    let archer_trigger = archer_trigger_on_stack(actor, archer, state);
    engine::step(state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { player, .. } if player == responder
    ));
    let removal = put(state, responder, "Hero's Downfall", Zone::Hand);
    engine::step(state, Action::CastSpell(removal)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseTargets { spell, .. } if spell == removal
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(archer))).unwrap();
    pass_until_archer_trigger_returns(state, &archer_trigger);
    assert_eq!(state.objects.get(archer).zone, Zone::Graveyard);
    let departure = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .find_map(|(at, event)| match event {
            CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 {
                source,
                departure_history_index,
            } if source.source == archer && source.controller == actor => {
                Some((at, *source, *departure_history_index))
            }
            _ => None,
        })
        .expect("actual departure LKI receipt");
    assert_eq!(
        state
            .engine
            .event_history
            .iter()
            .filter(|event| matches!(
                event,
                CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 { source, .. }
                    if source.source == archer
            ))
            .count(),
        1,
    );
    assert_eq!(departure.0 as u64, departure.2 + 1);
    assert!(matches!(
        state.engine.event_history.get(departure.2 as usize),
        Some(CommittedEvent::ZoneChange {
            object,
            from: Zone::Battlefield,
            to: Zone::Graveyard,
            controller_before,
        }) if *object == archer
            && *controller_before == actor
            && departure.1.zone_change_count + 1 == state.objects.get(archer).zone_change_count
    ));
    finish(state);
    assert_eq!(
        state.players[actor.index()].library.len(),
        16,
        "the Archer damage uses its pre-departure controller and draws one; Bolt then draws three"
    );
    assert_eq!(state.players[responder.index()].life, 16);
}

fn advance_to_archer_damage_trigger(actor: PlayerId, archer: ObjectId, state: &mut GameState) {
    for _ in 0..96 {
        match engine::advance_until_decision(state) {
            Decision::OrderTriggers { pending, .. } => {
                let archer_index = pending
                    .iter()
                    .position(|trigger| trigger.source == archer)
                    .expect("actual Firebrand Archer trigger in ordered batch");
                let mut order: Vec<_> = (0..pending.len())
                    .filter(|index| *index != archer_index)
                    .collect();
                order.push(archer_index);
                engine::step(state, Action::OrderTriggers(order)).unwrap();
            }
            Decision::CastSpellOrPass { player, .. }
                if player == actor && top_niv_damage_amount(state) == Some(1) =>
            {
                return;
            }
            Decision::CastSpellOrPass { .. } => {
                engine::step(state, Action::Pass).unwrap();
            }
            other => {
                panic!("Niv fixture unexpected decision before Archer damage trigger: {other:?}")
            }
        }
    }
    panic!("paid Firebrand Archer damage did not produce Niv's one-card draw trigger");
}

fn cast_flash_employment_and_finish(actor: PlayerId, target: ObjectId, state: &mut GameState) {
    let responder = actor.opponent();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { player, .. } if player == actor
    ));
    engine::step(state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { player, .. } if player == responder
    ));
    let employment = put(state, responder, "Involuntary Employment", Zone::Hand);
    state.players[responder.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    state.players[responder.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(state, Action::CastSpell(employment)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseTargets { spell, .. } if spell == employment
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    finish(state);
    assert_eq!(state.objects.get(target).controller, responder);
}
fn bytes(state: &GameState) -> Vec<u8> {
    serde_json::to_vec(state).unwrap()
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&bytes(state)).unwrap()
}
fn published(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let (mut state, niv, archer, _) = cast_niv(actor);
    cast_bolt(actor, &mut state);
    advance_to_archer_damage_trigger(actor, archer, &mut state);
    (state, niv, archer)
}
#[test]
fn closed_printed_program_and_complete_definition_keep_partial_and_refuse_forged_meaning() {
    let value = meaning::printed_meaning().unwrap();
    assert_eq!(value.card_id, 461);
    assert_eq!(value.printed.colored_mana, [ManaColor::U, ManaColor::R]);
    assert_eq!(
        (
            value.printed.generic_mana,
            value.printed.power,
            value.printed.toughness
        ),
        (4, 5, 5)
    );
    assert_eq!(value.ordered_rules.len(), 5);
    meaning::validate(&value).unwrap();
    assert_eq!(
        crate::card_def::CARD_DEFS[461].capability,
        crate::card_def::CardCapability::Partial
    );
    assert!(
        !value.full_engine_admitted
            && !value.owning_consumer_admitted
            && !value.model_admitted
            && !value.sampler_admitted
    );
    let mut forged = value.clone();
    forged.ordered_rules.swap(2, 3);
    assert!(meaning::validate(&forged).is_err());
    let mut json = serde_json::to_value(&value).unwrap();
    json["unrepresented_native_rule"] = true.into();
    assert!(serde_json::from_value::<meaning::MeaningV63>(json).is_err());
}
#[test]
fn genuine_paid_cast_and_native_publication_roundtrip_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let niv = put(&mut state, actor, meaning::NAME, Zone::Hand);
        let before = capture(&state, actor).unwrap();
        assert_eq!(before.authorized.len(), 1);
        assert!(capture(&state, actor.opponent())
            .unwrap()
            .authorized
            .is_empty());
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(niv)).unwrap();
        let waiting = capture(&state, actor).unwrap();
        assert_eq!(waiting.casts.len(), 1);
        assert!(waiting.casts[0].payment_pending);
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::CastSpellOrPass{player,..}if player==actor)
        );
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        let paid = capture(&state, actor).unwrap();
        assert!(
            paid.casts[0].genuine_cast_finalized
                && !paid.casts[0].payment_pending
                && !paid.casts[0].is_uncast_copy
        );
        assert_eq!(capture(&restore(&state), actor).unwrap(), paid);
        finish(&mut state);
        let facts = capture(&state, actor).unwrap();
        assert_eq!(facts.current.len(), 1);
        assert_eq!(facts.current[0].card.object, niv.0);
        assert_eq!(facts.current[0].form.power, Some(5));
        assert!(facts.no_maximum_hand_size[actor.index()]);
        validate(&state, actor, &facts).unwrap();
        assert!(!facts.complete_ordinary_observation && !facts.owning_consumer_admitted);
    }
}
#[test]
fn genuine_paid_draw_receipt_response_controller_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for steal_observer in [false, true] {
            let (mut state, niv, archer) = published(actor);
            let facts = capture(&state, actor).unwrap();
            assert_eq!(facts.damage_draws.len(), 1);
            let row = &facts.damage_draws[0];
            assert_eq!(
                (row.stage, row.committed_damage, row.draw_count),
                (StageV63::Published, 1, 1)
            );
            assert_eq!(row.observer.captured_controller, actor.into());
            assert_eq!(row.damage_source.object, archer.0);
            assert_eq!(
                row.damage_source.controller_basis,
                ControllerBasisV63::AtDamageEvent
            );
            let snapshot = restore(&state);
            assert_eq!(capture(&snapshot, actor).unwrap(), facts);
            let before = state.players[actor.index()].library.len();
            cast_flash_employment_and_finish(
                actor,
                if steal_observer { niv } else { archer },
                &mut state,
            );
            assert_eq!(
                before - state.players[actor.index()].library.len(),
                if steal_observer { 1 } else { 4 }
            );
            let after = capture(&state, actor).unwrap();
            assert_eq!(
                after.damage_draws[0].observer.captured_controller,
                actor.into()
            );
            assert_eq!(
                after.damage_draws[0].stage,
                StageV63::NoCurrentExecutableRow
            );
            assert_eq!(
                after.no_maximum_hand_size[actor.opponent().index()],
                steal_observer
            );
            assert_eq!(capture(&restore(&state), actor).unwrap(), after);
        }
    }
}
#[test]
fn genuine_paid_departed_damage_source_uses_exact_lki_without_later_identity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, archer, _) = cast_niv(actor);
        let generation = state.objects.get(archer).zone_change_count;
        remove_archer_before_its_trigger_damage(actor, archer, &mut state);
        let facts = capture(&state, actor).unwrap();
        let row = facts
            .damage_draws
            .iter()
            .find(|r| r.damage_source.object == archer.0)
            .unwrap();
        assert_eq!(row.damage_source.generation, generation);
        assert_eq!(row.damage_source.damage_controller, actor.into());
        assert_eq!(
            row.damage_source.controller_basis,
            ControllerBasisV63::ExactDepartureLki
        );
        assert!(row.damage_source.current_original_source.is_none());
        assert_eq!((row.committed_damage, row.draw_count), (1, 1));
        assert_eq!(state.objects.get(archer).zone, Zone::Graveyard);
        assert_eq!(capture(&restore(&state), actor).unwrap(), facts);
    }
}
#[test]
fn genuine_published_draw_rejects_changed_receipts_rows_and_facts_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = published(actor);
        let marker = state
            .engine
            .event_history
            .iter()
            .position(|e| matches!(e, E::NivMizzetVisionaryDamageV1 { .. }))
            .unwrap();
        let publication = state
            .engine
            .event_history
            .iter()
            .position(|e| matches!(e, E::NivMizzetVisionaryTriggerStackedV1 { .. }))
            .unwrap();
        for mutation in 0..10 {
            let mut forged = restore(&state);
            match mutation {
                0 => {
                    let E::NivMizzetVisionaryDamageV1 { observer, .. } =
                        &mut forged.engine.event_history[marker]
                    else {
                        unreachable!()
                    };
                    observer.controller = observer.controller.opponent();
                }
                1 => {
                    let row = forged.engine.event_history[marker].clone();
                    forged.engine.event_history.push(row);
                }
                2 => {
                    forged.engine.event_history[marker] = E::Tap {
                        object: ObjectId(0),
                    }
                }
                3 => {
                    let E::NivMizzetVisionaryTriggerStackedV1 { stack_item_id, .. } =
                        &mut forged.engine.event_history[publication]
                    else {
                        unreachable!()
                    };
                    stack_item_id.0 += 1;
                }
                4 => {
                    let E::NivMizzetVisionaryTriggerStackedV1 {
                        damage_history_index,
                        ..
                    } = &mut forged.engine.event_history[publication]
                    else {
                        unreachable!()
                    };
                    *damage_history_index += 1;
                }
                5 => {
                    let row = forged.stack.last_mut().unwrap();
                    row.controller = row.controller.opponent();
                }
                6 => {
                    let E::NivMizzetVisionaryDamageV1 { damage_source, .. } =
                        &mut forged.engine.event_history[marker]
                    else {
                        unreachable!()
                    };
                    damage_source.zone_change_count += 1;
                }
                7 => {
                    let row = forged.engine.event_history[marker].clone();
                    forged.engine.event_log.push(row);
                }
                8 => {
                    let row = forged.stack.last().unwrap().clone();
                    forged.stack.push(row);
                }
                9 => {
                    forged.engine.event_history[publication] = E::Tap {
                        object: ObjectId(0),
                    }
                }
                _ => unreachable!(),
            };
            let before = bytes(&forged);
            assert!(
                capture(&forged, actor).is_err(),
                "native receipt mutation {mutation}"
            );
            assert_eq!(bytes(&forged), before);
        }
        let mut forged = capture(&state, actor).unwrap();
        forged.damage_draws[0].draw_count += 1;
        let before = bytes(&state);
        assert!(validate(&state, actor, &forged).is_err());
        assert_eq!(bytes(&state), before);
    }
}
#[test]
fn genuine_paid_current_width_copy_suppression_and_facedown_appendix_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, niv, _, _) = cast_niv(actor);
        let mut wide = restore(&state);
        {
            let o = wide.objects.get_mut(niv);
            o.counters.charge = i32::MAX;
            o.counters.soul = i32::MAX;
            o.counters.incubation = i32::MAX;
            o.counters.revival = i32::MAX;
            o.counters.fellowship = i32::MAX;
            o.v4.time_counters_v1 = u8::MAX;
        }
        let ajani = put(
            &mut wide,
            actor,
            "Ajani, Caller of the Pride",
            Zone::Battlefield,
        );
        let existing = crate::planeswalker_v1::loyalty(&wide, ajani).unwrap_or(0);
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(
            &mut wide,
            ajani,
            u32::MAX - existing,
        )
        .unwrap();
        let facts = capture(&wide, actor).unwrap();
        let counters = &facts.battlefield_counters[actor.index()]
            .iter()
            .find(|r| r.card.object == niv.0)
            .unwrap()
            .counters;
        assert_eq!(
            (
                counters.charge,
                counters.soul,
                counters.incubation,
                counters.revival,
                counters.fellowship
            ),
            (i32::MAX, i32::MAX, i32::MAX, i32::MAX, i32::MAX)
        );
        assert_eq!(counters.time, u8::MAX);
        assert_eq!(
            facts.battlefield_counters[actor.index()]
                .iter()
                .find(|r| r.card.object == ajani.0)
                .unwrap()
                .counters
                .loyalty,
            u32::MAX
        );
        assert_eq!(capture(&restore(&wide), actor).unwrap(), facts);
        let mut negative = restore(&wide);
        negative.objects.get_mut(niv).counters.charge = -1;
        let before = bytes(&negative);
        assert!(capture(&negative, actor).is_err());
        assert_eq!(bytes(&negative), before);
        let mut overflow = restore(&state);
        overflow.objects.get_mut(niv).counters.plus1_plus1 = i32::MAX;
        let before = bytes(&overflow);
        assert!(capture(&overflow, actor).is_err());
        assert_eq!(bytes(&overflow), before);
        // Explicit native copy and form-state operands, not a paid copy/suppression
        // printing claim. The card under test above was genuinely cast and paid.
        let mut copied = restore(&state);
        let copy = crate::standard_cards_v1::create_token_copy(&mut copied, niv, actor.opponent())
            .unwrap();
        let copied_facts = capture(&copied, actor).unwrap();
        assert_eq!(copied_facts.current.len(), 2);
        assert!(copied_facts
            .current
            .iter()
            .any(|r| r.card.object == copy.0 && r.is_token));
        assert!(copied_facts.no_maximum_hand_size.iter().all(|v| *v));
        let mut hidden = restore(&state);
        hidden.objects.get_mut(niv).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        let hidden_facts = capture(&hidden, actor.opponent()).unwrap();
        assert!(hidden_facts.current.is_empty());
        assert!(!hidden_facts.no_maximum_hand_size[actor.index()]);
        assert!(matches!(
            hidden_facts.battlefield_counters[actor.index()]
                .iter()
                .find(|r| r.card.object == niv.0)
                .unwrap()
                .card
                .identity,
            public::IdentityV1::FaceDown { .. }
        ));
        let mut suppressed = restore(&state);
        let suppressor = put(
            &mut suppressed,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let link = crate::state::ObjectLinkV4 {
            object: suppressor,
            zone_change_count: suppressed.objects.get(suppressor).zone_change_count,
        };
        suppressed
            .objects
            .get_mut(niv)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .suppressed_by
            .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                source: link,
                timestamp: 7,
            });
        let suppressed_facts = capture(&suppressed, actor).unwrap();
        assert!(!suppressed_facts.current[0].form.printed_abilities_active);
        assert!(!suppressed_facts.no_maximum_hand_size[actor.index()]);
    }
}
#[test]
fn independent_unknown_hand_library_and_facedown_underlying_worlds_keep_public_facts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |unknown: &str, opponent_library: &str| {
            let mut state = ready_state_for(actor);
            for player in [actor, actor.opponent()] {
                for _ in 0..20 {
                    put(
                        &mut state,
                        player,
                        if player == actor {
                            "Island"
                        } else {
                            opponent_library
                        },
                        Zone::Library,
                    );
                }
            }
            for _ in 0..8 {
                put(&mut state, actor, "Island", Zone::Hand);
            }
            put(&mut state, actor, "Firebrand Archer", Zone::Battlefield);
            put(
                &mut state,
                actor.opponent(),
                "High Fae Trickster",
                Zone::Battlefield,
            );
            let niv = put(&mut state, actor, meaning::NAME, Zone::Hand);
            let secret = put(&mut state, actor.opponent(), unknown, Zone::Hand);
            let hidden = put(&mut state, actor.opponent(), unknown, Zone::Battlefield);
            // Explicit initial face-down native primitive. Both worlds are
            // initialized independently before the genuine paid cast history.
            state.objects.get_mut(hidden).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(niv)).unwrap();
            finish(&mut state);
            assert_eq!(state.objects.get(niv).zone, Zone::Battlefield);
            assert!(state.players[actor.index()]
                .mana_pool
                .iter()
                .all(|n| *n == 0));
            (state, secret, hidden)
        };
        let (a, secret, hidden) = build(meaning::NAME, "Island");
        let (b, other_secret, other_hidden) = build("Island", "Forest");
        assert_eq!((secret, hidden), (other_secret, other_hidden));
        assert_eq!(capture(&a, actor).unwrap(), capture(&b, actor).unwrap());
        assert_eq!(
            capture(&restore(&a), actor).unwrap(),
            capture(&restore(&b), actor).unwrap()
        );
        let own = capture(&a, actor.opponent()).unwrap();
        assert!(own.authorized.iter().any(|r| r.object == secret.0));
        assert!(!own.authorized.iter().any(|r| r.object == hidden.0));
    }
}

#[test]
fn paid_niv_native_damage_primitive_keeps_full_width_and_real_collection_before_any_draw_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, niv, _, _) = cast_niv(actor);
        // Explicit native damage operand, not a printed Niv damage ability or
        // a claim that Bolt deals this amount. No enormous draw loop is run.
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::damage(niv, Target::Player(actor.opponent()), i32::MAX),
        );
        let before = bytes(&state);
        let uncollected = capture(&state, actor).unwrap();
        assert_eq!(bytes(&state), before);
        let row = uncollected.damage_draws.last().unwrap();
        assert_eq!((row.committed_damage, row.draw_count), (i32::MAX, i32::MAX));
        assert_eq!(row.stage, StageV63::Uncollected);
        let receipt = state
            .engine
            .event_log
            .iter()
            .find(|event| matches!(event, CommittedEvent::NivMizzetVisionaryDamageV1 { .. }))
            .unwrap()
            .clone();
        for edit in 0..3 {
            let mut forged = restore(&state);
            let mut extra = receipt.clone();
            let CommittedEvent::NivMizzetVisionaryDamageV1 {
                damage_source,
                damage_history_index,
                ..
            } = &mut extra
            else {
                unreachable!()
            };
            match edit {
                0 => damage_source.controller = damage_source.controller.opponent(),
                1 => *damage_history_index = u64::MAX,
                2 => {}
                _ => unreachable!(),
            }
            forged.engine.event_log.push(extra);
            assert_eq!(forged.engine.event_history, state.engine.event_history);
            let before = bytes(&forged);
            assert!(crate::event::niv_mizzet_visionary_triggers_v1(
                &forged,
                &forged.engine.event_log,
            )
            .is_err());
            assert!(capture(&forged, actor).is_err());
            assert!(capture(&forged, actor.opponent()).is_err());
            assert_eq!(bytes(&forged), before);
        }
        let actual = crate::trigger::collect_and_process(&mut state);
        assert_eq!(actual.len(), 1);
        state.engine.pending_triggers.extend(actual);
        let held = capture(&state, actor).unwrap();
        assert_eq!(held.damage_draws.last().unwrap().stage, StageV63::Held);
        assert_eq!(capture(&restore(&state), actor).unwrap(), held);
        let wire = serde_json::to_value(&held).unwrap();
        assert_eq!(
            wire["damage_draws"].as_array().unwrap().last().unwrap()["draw_count"],
            i32::MAX
        );
    }
}
