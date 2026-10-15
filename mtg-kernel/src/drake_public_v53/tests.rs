use super::*;
use crate::drake_hatcher_v1::custody::JournalV1 as J;
use crate::engine::{self, Action, Decision};
use crate::event::{self, ActiveReplacement, ProposedEvent, ReplacementEffectKind};
use crate::ids::PlayerId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::{FaceDownV1, ObjectLinkV4, Step, Target};

fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn finish(state: &mut GameState) {
    for _ in 0..64 {
        match engine::advance_until_decision(state) {
            Decision::OrderTriggers { pending, .. } => engine::step(
                state,
                Action::OrderTriggers((0..pending.len()).rev().collect()),
            )
            .unwrap(),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("genuine V53 stack: {other:?}"),
        }
    }
    panic!("V53 stack did not finish")
}
fn paid(actor: PlayerId, hidden: bool) -> (GameState, ObjectId) {
    let mut state = ready_state_for(actor);
    for player in [actor, actor.opponent()] {
        for _ in 0..8 {
            put(
                &mut state,
                player,
                if hidden { "Swamp" } else { "Island" },
                Zone::Library,
            );
        }
        put(
            &mut state,
            player,
            if hidden {
                "Llanowar Elves"
            } else {
                "Think Twice"
            },
            Zone::Hand,
        );
    }
    let drake = put(&mut state, actor, "Drake Hatcher", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(drake)).unwrap();
    finish(&mut state);
    assert_eq!(state.objects.get(drake).zone, Zone::Battlefield);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    (state, drake)
}
fn damage(state: &mut GameState, source: ObjectId, actor: PlayerId, amount: i32) {
    engine::commit_combat_damage_events(
        state,
        vec![ProposedEvent::damage(
            source,
            Target::Player(actor.opponent()),
            amount,
        )],
    );
}
fn checked(state: &GameState) -> FactsV53 {
    let facts = capture(state).unwrap();
    validate(state, &facts).unwrap();
    let restored: FactsV53 = serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap();
    assert_eq!(facts, restored);
    validate(&restore(state), &restored).unwrap();
    fn fields(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, child) in object {
                    assert!(
                        ![
                            "world",
                            "original",
                            "event_history",
                            "event_log",
                            "record",
                            "journal",
                            "proposals",
                            "operation",
                            "answers",
                            "source_contract",
                            "ability_source_contract",
                            "inline_effect",
                            "effect",
                            "capture",
                            "marker",
                            "debit",
                            "stack_item_id",
                            "targets"
                        ]
                        .contains(&key.as_str()),
                        "private field {key}"
                    );
                    fields(child);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    fields(child)
                }
            }
            serde_json::Value::String(value) => assert!(![
                "DrakeJournalV1",
                "DrakeHistoryRefV1",
                "DrakeIncubationFromCaptureV1",
                "DrakeCreateTokenFromPaidV1"
            ]
            .iter()
            .any(|name| value.contains(name))),
            _ => {}
        }
    }
    fields(&serde_json::to_value(&facts).unwrap());
    facts
}
fn card<'a>(facts: &'a FactsV53, id: ObjectId) -> &'a CardV53 {
    facts
        .battlefield
        .iter()
        .flatten()
        .find(|card| card.reference.incarnation.object == id.0)
        .unwrap()
}
#[test]
fn typed_printed_roots_cost_order_token_blueprint_and_tamper() {
    let id = crate::card_def::card_id_by_name("Drake Hatcher").unwrap();
    for root in [
        RootV53::Prowess,
        RootV53::CombatIncubation,
        RootV53::RemoveIncubationForDrake,
    ] {
        let value = meaning::printed_meaning(id, root).unwrap();
        meaning::validate(&value).unwrap();
        assert_eq!(value.capability, crate::card_def::CardCapability::Full);
        assert!(
            !value.full_engine_admitted
                && !value.model_admitted
                && !value.sampler_admitted
                && !value.fair_search_admitted
        );
        let mut bad = value.clone();
        bad.root_sha256.clear();
        assert!(meaning::validate(&bad).is_err());
        let mut bad = value.clone();
        bad.model_admitted = true;
        assert!(meaning::validate(&bad).is_err());
        let mut bad = value.clone();
        bad.printed.power = 2;
        assert!(meaning::validate(&bad).is_err());
        let mut bad = value.clone();
        match &mut bad.program {
            meaning::ProgramV53::Triggered {
                ordered_instructions,
                ..
            } => ordered_instructions.clear(),
            meaning::ProgramV53::Activated {
                ordered_costs,
                costs_paid_before_stack,
                ..
            } => {
                *costs_paid_before_stack = false;
                let meaning::CostV53::RemoveCounters { count, .. } = &mut ordered_costs[0];
                *count = 2;
            }
        }
        // An unchanged digest cannot bless altered typed meaning.
        assert_eq!(bad.root_sha256, value.root_sha256);
        assert!(meaning::validate(&bad).is_err());
        match value.program {
            meaning::ProgramV53::Triggered {
                event,
                ordered_instructions,
                ..
            } => match root {
                RootV53::Prowess => {
                    assert_eq!(event, meaning::EventV53::ControllerCastsNoncreatureSpell);
                    assert!(matches!(
                        ordered_instructions.as_slice(),
                        [meaning::InstructionV53::BoostUntilEndOfTurn {
                            power: 1,
                            toughness: 1,
                            ..
                        }]
                    ));
                }
                RootV53::CombatIncubation => {
                    assert_eq!(
                        event,
                        meaning::EventV53::SourceDealsFinalPositiveCombatDamageToPlayer
                    );
                    assert!(matches!(
                        ordered_instructions.as_slice(),
                        [meaning::InstructionV53::PlaceCounters {
                            amount: meaning::AmountV53::FinalPositiveCommittedPlayerCombatDamage,
                            replacements_from_resolution_world: true,
                            ..
                        }]
                    ));
                }
                _ => panic!("wrong typed root"),
            },
            meaning::ProgramV53::Activated {
                ordered_costs,
                ordered_instructions,
                costs_paid_before_stack,
                countering_refunds_costs,
                ..
            } => {
                assert!(costs_paid_before_stack && !countering_refunds_costs);
                assert!(matches!(
                    ordered_costs.as_slice(),
                    [meaning::CostV53::RemoveCounters {
                        kind: meaning::CounterV53::Incubation,
                        count: 3,
                        ..
                    }]
                ));
                let [meaning::InstructionV53::CreateToken {
                    token,
                    count: 1,
                    controller: meaning::ActorV53::OriginalActivatingController,
                    independent_of_source_survival: true,
                }] = ordered_instructions.as_slice()
                else {
                    panic!("actual Drake token instruction")
                };
                assert_eq!((token.power, token.toughness), (2, 2));
                assert_eq!(token.colors, [ManaColor::U]);
                assert_eq!(token.subtypes, [crate::card_def::Subtype::Drake]);
                assert_eq!(token.keyword_bits, Keywords::FLYING.0);
            }
        }
    }
    assert!(meaning::printed_meaning(
        crate::card_def::card_id_by_name("Forest").unwrap(),
        RootV53::Prowess
    )
    .is_err());
}
#[test]
fn actual_paid_prowess_current_temporary_form_final_damage_activation_restore_and_counter_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, drake) = paid(actor, false);
        let spell = put(&mut state, actor, "Think Twice", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        state.priority_player = actor;
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        finish(&mut state);
        let facts = checked(&state);
        assert_eq!(
            (
                card(&facts, drake).form.power,
                card(&facts, drake).form.toughness
            ),
            (Some(2), Some(4))
        );
        assert!(card(&facts, drake)
            .temporary_boosts
            .iter()
            .any(
                |boost| boost.binding == TemporaryBindingV53::OriginalIncarnation
                    && boost.power == 1
                    && boost.toughness == 1
                    && boost.until_end_of_turn
            ));
        state.engine.active_replacements.push(ActiveReplacement {
            id: 1,
            source: drake,
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Player(actor.opponent()),
                remaining: 2,
            },
        });
        damage(&mut state, drake, actor, 5);
        let held = checked(&state);
        assert_eq!(held.combat.len(), 1);
        assert_eq!(held.combat[0].final_positive_damage, 3);
        assert_eq!(held.combat[0].original_controller, actor.into());
        assert_eq!(held.combat[0].outcome, CombatOutcomeV53::Held);
        assert_eq!(card(&held, drake).counters.incubation, 0);
        let _ = engine::advance_until_decision(&mut state);
        assert_eq!(
            checked(&state).combat[0].outcome,
            CombatOutcomeV53::Published
        );
        let mut countered = restore(&state);
        let id = countered.stack.last().unwrap().v4.stack_item_id;
        engine::counter_stack_item_by_id(&mut countered, id).unwrap();
        let facts = checked(&countered);
        assert_eq!(facts.combat[0].outcome, CombatOutcomeV53::Countered);
        assert_eq!(card(&facts, drake).counters.incubation, 0);
        finish(&mut state);
        let facts = checked(&state);
        assert_eq!(
            facts.combat[0].outcome,
            CombatOutcomeV53::Resolved {
                placement_on_original_incarnation: Some(PlacementV53 {
                    before: 0,
                    added: 3,
                    after: 3
                })
            }
        );
        state.priority_player = actor;
        engine::step(&mut state, Action::ActivateAbility(drake, 0)).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        let paid = checked(&state);
        assert_eq!(paid.activations.len(), 1);
        assert_eq!(
            paid.activations[0].payment,
            PaymentV53 {
                kind: meaning::CounterV53::Incubation,
                removed: 3,
                before: 3,
                after: 0
            }
        );
        assert_eq!(paid.activations[0].outcome, ActivationOutcomeV53::Published);
        for mode in 0..3 {
            let mut changed = paid.clone();
            match mode {
                0 => changed.activations[0].payment.removed = 2,
                1 => changed.activations[0].payment.after = 1,
                _ => changed.activations[0].original_controller = actor.opponent().into(),
            }
            assert!(validate(&state, &changed).is_err());
        }
        for mode in 0..2 {
            let mut changed = restore(&state);
            let row = changed
                .engine
                .event_history
                .iter_mut()
                .rev()
                .find_map(|event| match event {
                    CommittedEvent::DrakeJournalV1 { record }
                        if matches!(
                            (&record.value, mode),
                            (J::Debit { .. }, 0) | (J::Paid { .. }, 1)
                        ) =>
                    {
                        Some(record)
                    }
                    _ => None,
                })
                .unwrap();
            match &mut row.value {
                J::Debit { count, .. } => *count = 2,
                J::Paid { original, .. } => {
                    original.objects.get_mut(drake).counters.incubation += 1
                }
                _ => unreachable!(),
            }
            let before = serde_json::to_vec(&changed).unwrap();
            assert!(capture(&changed).is_err());
            assert!(validate(&changed, &paid).is_err());
            assert_eq!(serde_json::to_vec(&changed).unwrap(), before);
        }
        let mut countered = restore(&state);
        let id = countered.stack.last().unwrap().v4.stack_item_id;
        engine::counter_stack_item_by_id(&mut countered, id).unwrap();
        let facts = checked(&countered);
        assert_eq!(
            facts.activations[0].outcome,
            ActivationOutcomeV53::Countered
        );
        assert_eq!(card(&facts, drake).counters.incubation, 0);
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            event::propose_and_commit(branch, ProposedEvent::zone_change(drake, Zone::Hand));
            finish(branch);
            let facts = checked(branch);
            assert!(facts.activations[0].current_original_source.is_none());
            let ActivationOutcomeV53::Resolved { tokens } = &facts.activations[0].outcome else {
                panic!("actual original paid token")
            };
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].original_controller, actor.into());
            assert_eq!(tokens[0].created.controller, actor.into());
            assert!(tokens[0].current_original_token.is_some());
            let mut bad = facts.clone();
            let ActivationOutcomeV53::Resolved { tokens } = &mut bad.activations[0].outcome else {
                unreachable!()
            };
            tokens[0].created.identity = IdentityV53::Known {
                card_id: crate::card_def::card_id_by_name("Forest").unwrap(),
            };
            assert!(validate(branch, &bad).is_err());
        }
        assert_eq!(state, restored);
    }
}
#[test]
fn original_source_controller_and_new_incarnation_are_distinct_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, drake) = paid(actor, false);
        damage(&mut state, drake, actor, 3);
        let mut reentered = restore(&state);
        event::propose_and_commit(
            &mut reentered,
            ProposedEvent::zone_change(drake, Zone::Hand),
        );
        event::propose_and_commit(
            &mut reentered,
            ProposedEvent::zone_change(drake, Zone::Battlefield),
        );
        finish(&mut reentered);
        let facts = checked(&reentered);
        assert_eq!(facts.combat[0].final_positive_damage, 3);
        assert!(facts.combat[0].current_original_source.is_none());
        assert_eq!(
            facts.combat[0].outcome,
            CombatOutcomeV53::Resolved {
                placement_on_original_incarnation: None
            }
        );
        assert_eq!(card(&facts, drake).counters.incubation, 0);
        crate::standard_cards_v1::gain_control(&mut state, drake, actor.opponent());
        finish(&mut state);
        let facts = checked(&state);
        assert_eq!(facts.combat[0].original_controller, actor.into());
        assert_eq!(
            facts.combat[0]
                .current_original_source
                .as_ref()
                .unwrap()
                .controller,
            actor.opponent().into()
        );
        assert_eq!(card(&facts, drake).counters.incubation, 3);
        state.priority_player = actor.opponent();
        engine::step(&mut state, Action::ActivateAbility(drake, 0)).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        crate::standard_cards_v1::gain_control(&mut state, drake, actor);
        finish(&mut state);
        let facts = checked(&state);
        assert_eq!(
            facts.activations[0].original_controller,
            actor.opponent().into()
        );
        assert_eq!(
            facts.activations[0]
                .current_original_source
                .as_ref()
                .unwrap()
                .controller,
            actor.into()
        );
        let ActivationOutcomeV53::Resolved { tokens } = &facts.activations[0].outcome else {
            panic!("actual paid actor outcome")
        };
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].original_controller, actor.opponent().into());
        assert_eq!(tokens[0].created.controller, actor.opponent().into());
    }
}
#[test]
fn public_tamper_and_erased_private_originals_refuse_before_recapture_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, drake) = paid(actor, false);
        damage(&mut state, drake, actor, 3);
        finish(&mut state);
        let facts = checked(&state);
        let before = serde_json::to_vec(&state).unwrap();
        for mode in 0..5 {
            let mut bad = facts.clone();
            match mode {
                0 => bad.combat[0].final_positive_damage = 1,
                1 => bad.combat[0].original_controller = actor.opponent().into(),
                2 => bad.combat[0].original_source.incarnation.generation += 1,
                3 => {
                    bad.battlefield[actor.index()]
                        .iter_mut()
                        .find(|card| card.reference.incarnation.object == drake.0)
                        .unwrap()
                        .counters
                        .incubation = 2
                }
                _ => bad.meanings[0].model_admitted = true,
            }
            assert!(validate(&state, &bad).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        let mut private_payload = serde_json::to_value(&facts).unwrap();
        private_payload
            .as_object_mut()
            .unwrap()
            .insert("original".into(), serde_json::to_value(&state).unwrap());
        assert!(serde_json::from_value::<FactsV53>(private_payload).is_err());
        let mut encoded = serde_json::to_value(&facts).unwrap();
        let array = encoded["battlefield"][actor.index()]
            .as_array_mut()
            .unwrap();
        let card = array
            .iter_mut()
            .find(|card| card["reference"]["incarnation"]["object"] == drake.0)
            .unwrap();
        card["counters"]
            .as_object_mut()
            .unwrap()
            .remove("incubation");
        assert!(serde_json::from_value::<FactsV53>(encoded).is_err());
        for mode in 0..4 {
            let mut bad = restore(&state);
            if mode == 0 {
                bad.engine.event_history.retain(|event| {
                    custody::journal(event)
                        .is_none_or(|row| !matches!(row.value, J::Captured { .. }))
                });
            } else {
                let row = bad
                    .engine
                    .event_history
                    .iter_mut()
                    .find_map(|event| match event {
                        CommittedEvent::DrakeJournalV1 { record }
                            if matches!(
                                (&record.value, mode),
                                (J::Captured { .. }, 1)
                                    | (J::CombatInput { .. }, 2)
                                    | (J::Enclosing { .. }, 3)
                            ) =>
                        {
                            Some(record)
                        }
                        _ => None,
                    })
                    .unwrap();
                match &mut row.value {
                    J::Captured { amount, .. } => *amount = 1,
                    J::CombatInput { world, .. } => {
                        world.objects.get_mut(drake).counters.incubation = 99
                    }
                    J::Enclosing { original, .. } => original.players[actor.index()].life -= 1,
                    _ => unreachable!(),
                }
            }
            let original = serde_json::to_vec(&bad).unwrap();
            assert!(capture(&bad).is_err());
            assert!(validate(&bad, &facts).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), original);
        }
    }
}
#[test]
fn independent_hidden_worlds_and_face_down_identities_have_equal_public_facts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut outputs = Vec::new();
        for hidden in [false, true] {
            let (mut state, drake) = paid(actor, hidden);
            let concealed = put(
                &mut state,
                actor.opponent(),
                if hidden {
                    "Drake Hatcher"
                } else {
                    "Llanowar Elves"
                },
                Zone::Battlefield,
            );
            state.objects.get_mut(concealed).v4.face_down_v1 = Some(FaceDownV1 {
                disguised: true,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            damage(&mut state, drake, actor, 3);
            finish(&mut state);
            let facts = checked(&state);
            let face = card(&facts, concealed);
            assert_eq!(
                face.reference.identity,
                IdentityV53::FaceDown { disguised: true }
            );
            assert_eq!(face.face_index, None);
            assert!(face.form.names.is_empty());
            assert!(face.form.subtype_ids.is_empty());
            assert_eq!(face.form.color_mask, 0);
            assert_eq!((face.form.power, face.form.toughness), (Some(2), Some(2)));
            assert!(!face.form.printed_abilities_active);
            outputs.push(serde_json::to_vec(&facts).unwrap());
        }
        assert_eq!(outputs[0], outputs[1]);
    }
}
#[test]
fn exact_storage_widths_are_mandatory_and_current_counter_values_are_not_historical_amounts() {
    let (mut state, drake) = paid(PlayerId::P0, false);
    damage(&mut state, drake, PlayerId::P0, 3);
    finish(&mut state);
    // Width primitive after genuine historical custody. It deliberately differs
    // from the old placement and cannot replace that placement's private input.
    let live = state.objects.get_mut(drake);
    live.counters.incubation = i32::MAX;
    live.counters.plus1_plus1 = 40_000;
    live.counters.charge = i32::MAX;
    live.counters.net = i32::MAX;
    live.counters.bait = i32::MAX;
    live.counters.soul = i32::MAX;
    live.damage = u32::MAX;
    let facts = checked(&state);
    let exact = card(&facts, drake);
    assert_eq!(exact.counters.incubation, i32::MAX);
    assert_eq!(exact.counters.plus_one, 40_000);
    assert_eq!(exact.marked_damage, u32::MAX);
    assert_eq!(facts.combat[0].final_positive_damage, 3);
    assert_eq!(
        facts.combat[0].outcome,
        CombatOutcomeV53::Resolved {
            placement_on_original_incarnation: Some(PlacementV53 {
                before: 0,
                added: 3,
                after: 3
            })
        }
    );
    let encoded = serde_json::to_value(exact).unwrap();
    for name in [
        "minus_one",
        "minus_toughness",
        "stun",
        "lore",
        "oil",
        "lifelink",
        "time",
        "finality",
    ] {
        assert!(encoded["counters"].get(name).is_some(), "mandatory {name}");
    }
}
#[test]
fn actual_combat_zero_prevention_block_suppression_and_strike_waves_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..6 {
            let (mut state, drake) = paid(actor, false);
            damage(&mut state, drake, actor, 0);
            assert!(checked(&state).combat.is_empty());
            crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor);
            crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
            let blocker = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            if mode == 2 {
                state.engine.active_replacements.push(ActiveReplacement {
                    id: 1,
                    source: drake,
                    kind: ReplacementEffectKind::PreventNextDamage {
                        target: Target::Player(actor.opponent()),
                        remaining: 10,
                    },
                });
            }
            if mode == 3 {
                let source = ObjectLinkV4 {
                    object: blocker,
                    zone_change_count: state.objects.get(blocker).zone_change_count,
                };
                state
                    .objects
                    .get_mut(drake)
                    .v4
                    .creature_upgrade
                    .get_or_insert_with(Default::default)
                    .suppressed_by
                    .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                        source,
                        timestamp: 1,
                    });
                let facts = checked(&state);
                assert!(card(&facts, drake).form.abilities_removed);
                assert!(!card(&facts, drake).form.printed_abilities_active);
                assert_eq!(
                    card(&facts, drake).form.keyword_bits & Keywords::VIGILANCE.0,
                    0
                );
            }
            if mode >= 4 {
                state
                    .objects
                    .get_mut(drake)
                    .v4
                    .creature_upgrade
                    .get_or_insert_with(Default::default)
                    .keyword_grants
                    .push((
                        if mode == 4 {
                            Keywords::FIRST_STRIKE
                        } else {
                            Keywords::DOUBLE_STRIKE
                        },
                        1,
                    ));
            }
            let mut attacked = false;
            let mut ended = false;
            for _ in 0..96 {
                let decision = engine::advance_until_decision(&mut state);
                if attacked && state.step == Step::EndCombat && state.stack.is_empty() {
                    ended = true;
                    break;
                }
                match decision {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    Decision::DeclareAttackers { .. } => {
                        engine::step(&mut state, Action::DeclareAttackers(vec![drake])).unwrap();
                        attacked = true;
                    }
                    Decision::DeclareBlockers { .. } => engine::step(
                        &mut state,
                        Action::DeclareBlockers(if mode == 1 {
                            vec![(blocker, drake)]
                        } else {
                            vec![]
                        }),
                    )
                    .unwrap(),
                    other => panic!("actual V53 combat mode {mode}: {other:?}"),
                }
            }
            assert!(ended);
            let facts = checked(&state);
            let expected = if matches!(mode, 1 | 2 | 3) {
                0
            } else if mode == 5 {
                2
            } else {
                1
            };
            assert_eq!(facts.combat.len(), expected);
            assert!(facts
                .combat
                .iter()
                .all(|hit| hit.final_positive_damage == 1));
            assert_eq!(card(&facts, drake).counters.incubation, expected as i32);
        }
    }
}

#[test]
fn previously_public_original_refs_redact_current_face_down_identity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, drake) = paid(actor, false);
        damage(&mut state, drake, actor, 3);
        finish(&mut state);
        state.objects.get_mut(drake).v4.face_down_v1 = Some(FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        let facts = checked(&state);
        assert_eq!(
            facts.combat[0].original_source.identity,
            IdentityV53::FaceDown { disguised: true }
        );
        assert_eq!(
            facts.combat[0]
                .current_original_source
                .as_ref()
                .unwrap()
                .identity,
            IdentityV53::FaceDown { disguised: true }
        );
        assert_eq!(
            card(&facts, drake).reference.identity,
            IdentityV53::FaceDown { disguised: true }
        );
        assert_eq!(card(&facts, drake).counters.incubation, 3);
        assert_eq!(facts.combat[0].final_positive_damage, 3);
    }
}

#[test]
fn native_pt_prefix_refuses_overflow_and_uses_public_face_down_form_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, drake) = paid(actor, false);
        state.objects.get_mut(drake).counters.plus1_plus1 = i32::MAX;
        let before = serde_json::to_vec(&state).unwrap();
        assert!(capture(&state)
            .unwrap_err()
            .0
            .contains("native engine range"));
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let mut views = Vec::new();
        for name in ["Drake Hatcher", "Llanowar Elves"] {
            let mut primitive = ready_state_for(actor);
            let concealed = put(&mut primitive, actor, name, Zone::Battlefield);
            primitive.objects.get_mut(concealed).v4.face_down_v1 = Some(FaceDownV1 {
                disguised: true,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            // Both have public base 2/2. Using concealed printed 1/3 vs 1/1
            // would incorrectly reject only Hatcher near the native limit.
            primitive.objects.get_mut(concealed).counters.plus1_plus1 = i32::MAX - 2;
            let facts = checked(&primitive);
            assert_eq!(
                (
                    card(&facts, concealed).form.power,
                    card(&facts, concealed).form.toughness
                ),
                (Some(i32::MAX), Some(i32::MAX))
            );
            views.push(serde_json::to_vec(&facts).unwrap());
        }
        assert_eq!(views[0], views[1]);
    }
}
