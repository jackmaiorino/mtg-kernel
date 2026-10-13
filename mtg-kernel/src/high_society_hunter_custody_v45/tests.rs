use super::*;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::engine::{self, Action, Decision};
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;

#[test]
fn predecessor_wrapper_empty_bytes_hash_and_foreign_owned_recipe_refusal() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let forest = put(&mut state, actor, "Forest", Zone::Battlefield);
        let source = AbilitySourceContractV4::capture(&state, forest);
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        validate(&state).unwrap();
        assert_eq!(
            normalize_root(
                &state,
                &crate::high_society_hunter_v1::attack_program(),
                Some(source)
            )
            .unwrap(),
            None
        );
        assert!(normalize_root(&state, &bound(0, true), Some(source)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.state_hash(), hash);
        let wrapper = crate::state::HighSocietyHunterStartedV45::default();
        let mut hasher = std::collections::hash_map::DefaultHasher::default();
        let before = std::hash::Hasher::finish(&hasher);
        std::hash::Hash::hash(&wrapper, &mut hasher);
        assert_eq!(std::hash::Hasher::finish(&hasher), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn attack_state(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    state.step = crate::state::Step::DeclareAttackers;
    // True-definition physical setup; the integration suite separately pays 3BB.
    let hunter = put(&mut state, actor, "High-Society Hunter", Zone::Battlefield);
    let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, actor, "Forest", Zone::Hand);
    engine::step(&mut state, Action::DeclareAttackers(vec![hunter])).unwrap();
    (state, hunter, other)
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn menu(state: &mut GameState) {
    for _ in 0..12 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::ChooseEffectTargets { .. }) {
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("missing actual captured Hunter menu")
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn readonly_refusal(state: &GameState, action: Action) {
    let bytes = serde_json::to_vec(state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert!(engine::step(&mut restored, action).is_err());
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    assert!(matches!(
        engine::advance_until_decision(&mut restored),
        Decision::Halted { .. }
    ));
    let halt = restored.engine.halted.take();
    assert!(halt.is_some());
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "unregistered actual Hunter; parent owns registration and native"]
fn genuine_capture_waiting_publication_menu_answer_and_cost_draw_completion_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let (mut state, hunter, other) = attack_state(actor);
            validate(&state).unwrap();
            menu(&mut state);
            validate(&state).unwrap();
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
            let hand = state.players[actor.index()].hand.len();
            for current in [&mut state, &mut replay] {
                if pay {
                    engine::step(
                        current,
                        Action::ChooseEffectTarget(crate::state::Target::Object(other)),
                    )
                    .unwrap();
                } else {
                    engine::step(current, Action::FinishEffectSelection).unwrap();
                }
                validate(current).unwrap();
                for _ in 0..24 {
                    let decision = engine::advance_until_decision(current);
                    validate(current).unwrap();
                    if current.stack.is_empty() && current.engine.pending_triggers.is_empty() {
                        break;
                    }
                    assert!(
                        matches!(decision, Decision::CastSpellOrPass { .. }),
                        "{decision:?}"
                    );
                    engine::step(current, Action::Pass).unwrap();
                }
                assert_eq!(
                    current.objects.get(hunter).counters.plus1_plus1,
                    i32::from(pay)
                );
                assert_eq!(
                    current.players[actor.index()].hand.len(),
                    hand + usize::from(pay)
                );
                assert!(current.engine.pending_effect.is_none());
                assert!(current.stack.is_empty());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "unregistered actual Hunter; parent owns registration and native"]
fn original_journals_preserved_fresh_published_suspended_and_answer_erasure_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (fresh, _, _) = attack_state(actor);
        let mut erased = fresh.clone();
        erased.engine.event_log.clear();
        readonly_refusal(&erased, Action::Pass);
        let mut published = fresh.clone();
        engine::advance_until_decision(&mut published);
        let mut erased = published.clone();
        erased.stack.clear();
        readonly_refusal(&erased, Action::Pass);
        let mut suspended = fresh;
        menu(&mut suspended);
        for case in 0..7 {
            let mut bad = suspended.clone();
            match case {
                0=>bad.engine.pending_effect=None,
                1=>bad.engine.event_history.retain(|e|!matches!(e,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::Menu{..}))),
                2=>{bad.engine.pending_effect=None;bad.engine.event_history.retain(|e|!matches!(e,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::Menu{..}|JournalV45::SelectionInput{..})));},
                3=>{
                    let removal=bad.engine.event_history.iter().enumerate().find_map(|(at,e)|matches!(e,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::RemovalInput{..})).then_some(at as u64)).unwrap();
                    bad.stack.clear();bad.engine.pending_effect=None;
                    append(&mut bad,JournalV45::Closed{removal,operation:None}).unwrap();
                },
                4=>bad.stack.last_mut().unwrap().controller=actor.opponent(),
                5=>{let publication=bad.engine.event_history.iter().find(|e|matches!(e,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::Published{..}))).unwrap().clone();bad.engine.event_history.push(publication);},
                _=>bad.stack.last_mut().unwrap().v4.high_society_hunter_started_v45.0=None,
            }
            readonly_refusal(&bad, Action::FinishEffectSelection);
        }
        engine::step(&mut suspended, Action::FinishEffectSelection).unwrap();
        let mut erased = suspended;
        erased.engine.event_history.retain(|e|!matches!(e,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::SelectionInput{..})));
        readonly_refusal(&erased, Action::Pass);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "unregistered actual Hunter; parent owns registration and native"]
fn genuine_counter_retains_full_removal_input_and_manufactured_marker_only_closure_refuses() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, _) = attack_state(actor);
        engine::advance_until_decision(&mut state);
        let original = state.stack.last().unwrap().clone();
        let published = publication(&state, &original).unwrap().0;
        let mut fake = state.clone();
        fake.stack.clear();
        append(
            &mut fake,
            JournalV45::Closed {
                removal: published,
                operation: None,
            },
        )
        .unwrap();
        readonly_refusal(&fake, Action::Pass);
        engine::counter_stack_item_by_id(&mut state, original.v4.stack_item_id).unwrap();
        validate(&state).unwrap();
        let bytes = serde_json::to_vec(&state).unwrap();
        let replay: GameState = serde_json::from_slice(&bytes).unwrap();
        validate(&replay).unwrap();
        let mut reintroduced = replay.clone();
        reintroduced.stack.push(original);
        readonly_refusal(&reintroduced, Action::Pass);
        let mut erased = replay;
        erased.engine.event_history.retain(|e|!matches!(e,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::RemovalInput{..})));
        readonly_refusal(&erased, Action::Pass);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "unregistered actual Hunter; parent owns registration and native"]
fn completed_actual_payment_and_draw_inputs_cannot_omit_real_outputs_or_fake_source_absence() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, hunter, other) = attack_state(actor);
        menu(&mut state);
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(crate::state::Target::Object(other)),
        )
        .unwrap();
        for _ in 0..24 {
            let decision = engine::advance_until_decision(&mut state);
            if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        validate(&state).unwrap();
        assert_eq!(state.objects.get(hunter).counters.plus1_plus1, 1);
        let original = serde_json::to_vec(&state).unwrap();
        for case in 0..5 {
            let mut bad: GameState = serde_json::from_slice(&original).unwrap();
            let mut changed = false;
            for event in &mut bad.engine.event_history {
                let CommittedEvent::HighSocietyHunterJournalV45 { journal } = event else {
                    continue;
                };
                match (case, journal.as_mut()) {
                    (
                        0,
                        JournalV45::CostOperation {
                            departure, counter, ..
                        },
                    ) => {
                        *departure = None;
                        *counter = None;
                        changed = true;
                        break;
                    }
                    (1, JournalV45::CostInput { source_before, .. }) => {
                        source_before.zone = Zone::Graveyard;
                        changed = true;
                        break;
                    }
                    (2, JournalV45::CostOperation { sacrifice, .. }) => {
                        *sacrifice = None;
                        changed = true;
                        break;
                    }
                    (3, JournalV45::DrawOperation { draw, .. }) => {
                        *draw = 0;
                        changed = true;
                        break;
                    }
                    (4, JournalV45::CostInput { source_before, .. }) => {
                        source_before.zone_change_count += 1;
                        changed = true;
                        break;
                    }
                    _ => {}
                }
            }
            assert!(changed, "missing genuine journal phase {case}");
            readonly_refusal(&bad, Action::Pass);
        }
        let mut duplicate: GameState = serde_json::from_slice(&original).unwrap();
        let phase=duplicate.engine.event_history.iter().find(|event|matches!(event,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::SelectionInput{..}))).unwrap().clone();
        duplicate.engine.event_history.push(phase);
        readonly_refusal(&duplicate, Action::Pass);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "unregistered actual Hunter; parent owns registration and native"]
fn genuine_sacrifice_of_typed_copied_banishing_source_keeps_linked_attachment_owner_until_completion(
) {
    use crate::effect::ExecCtx;
    use crate::state::ObjectLinkV4;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, banishing, aura, host) =
            crate::banishing_light_v1::tests::linked_aura(actor);
        // Genuine typed Aegis copy setup, with physical registered operands.
        // This is not a claim of a printed mixed-catalog Longbow copy spell.
        let attachment = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
        let donor = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(donor, Zone::Exile),
        );
        let generation = state.objects.get(banishing).zone_change_count;
        state
            .attach_object_exact(attachment, 0, banishing, generation)
            .unwrap();
        let attachment_source = AbilitySourceContractV4::capture(&state, attachment);
        state.objects.get_mut(donor).v4.exiled_by = Some(ObjectLinkV4 {
            object: attachment,
            zone_change_count: attachment_source.zone_change_count,
        });
        let mut copy_ctx = ExecCtx::no_targets(attachment, actor);
        copy_ctx.ability_source_contract = Some(attachment_source);
        let host_binding = ObjectLinkV4 {
            object: banishing,
            zone_change_count: generation,
        };
        let timestamp = state.objects.get(attachment).v4.layer_timestamp.unwrap();
        let chosen = EffectObjectBinding {
            object: donor,
            expected_zone: Zone::Exile,
            expected_zone_change_count: state.objects.get(donor).zone_change_count,
        };
        assert!(crate::standard_cards_v1::aegis_copy_candidates(
            &state,
            &copy_ctx,
            host_binding,
            timestamp
        )
        .contains(&chosen));
        crate::standard_cards_v1::apply_aegis_copy(
            &mut state,
            &copy_ctx,
            host_binding,
            timestamp,
            chosen,
        );
        assert!(crate::engine::object_has_type(
            &state,
            banishing,
            crate::card_def::CardType::Creature
        ));
        assert!(crate::standard_cards_v1::had_card_def(
            &state,
            banishing,
            generation,
            crate::card_def::card_id_by_name("Banishing Light").unwrap()
        ));
        let hunter = put(&mut state, actor, "High-Society Hunter", Zone::Battlefield);
        put(&mut state, actor, "Forest", Zone::Hand);
        state.step = crate::state::Step::DeclareAttackers;
        engine::step(&mut state, Action::DeclareAttackers(vec![hunter])).unwrap();
        menu(&mut state);
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(crate::state::Target::Object(banishing)),
        )
        .unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::ChooseEffectTargets{player,source,ref legal_targets,..}if player==actor.opponent() && source==aura && legal_targets.contains(&crate::state::Target::Object(host)))
        );
        validate(&state).unwrap();
        assert!(crate::linked_aura_operation_v1::has_pending(&state));
        assert!(state.engine.event_history.iter().any(|event|matches!(event,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::CostInput{..}))));
        assert!(!state.engine.event_history.iter().any(|event|matches!(event,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::CostOperation{..}))));
        let checkpoint = serde_json::to_vec(&state).unwrap();
        let mut bad: GameState = serde_json::from_slice(&checkpoint).unwrap();
        bad.engine.linked_aura_operation_v1 = Default::default();
        readonly_refusal(
            &bad,
            Action::ChooseEffectTarget(crate::state::Target::Object(host)),
        );
        let mut replay: GameState = serde_json::from_slice(&checkpoint).unwrap();
        for current in [&mut state, &mut replay] {
            engine::step(
                current,
                Action::ChooseEffectTarget(crate::state::Target::Object(host)),
            )
            .unwrap();
            for _ in 0..30 {
                let decision = engine::advance_until_decision(current);
                validate(current).unwrap();
                if current.stack.is_empty() && current.engine.pending_triggers.is_empty() {
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(current, Action::Pass).unwrap();
            }
            assert_eq!(current.objects.get(banishing).zone, Zone::Graveyard);
            assert_eq!(current.objects.get(aura).zone, Zone::Battlefield);
            assert_eq!(
                current.objects.get(aura).v4.attached_to,
                Some(ObjectLinkV4 {
                    object: host,
                    zone_change_count: current.objects.get(host).zone_change_count
                })
            );
            assert_eq!(current.objects.get(hunter).counters.plus1_plus1, 1);
            assert!(!crate::linked_aura_operation_v1::has_pending(current));
            assert!(current.stack.is_empty());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
