use super::*;
use crate::effect::{self, ExecCtx};

pub(crate) fn ready(actor: PlayerId) -> GameState {
    let forest = card_def::card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_423,
        actor,
    );
    state.step = Step::Main1;
    state
}

#[test]
fn absent_scheduler_preserves_wire_key_and_restore_hash_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = ready(actor);
        assert_eq!(state.turn_key_v1(), state.turn);
        let json = serde_json::to_value(&state).unwrap();
        assert!(json.get("extra_turn_v1").is_none());
        let restored: GameState = serde_json::from_value(json).unwrap();
        assert_eq!(state.state_hash(), restored.state_hash());
        validate(&restored).unwrap();
    }
}

#[test]
fn direct_unowned_leaf_cannot_schedule_from_registered_physical_source() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = state.players[actor.index()].library[0];
        effect::execute(&recipe(), &ExecCtx::no_targets(source, actor), &mut state);
        assert!(state.engine.halted.is_some());
        assert!(state.extra_turn_v1.is_none());
        assert!(state.engine.event_history.iter().all(|event| !matches!(
            event,
            CommittedEvent::ExtraTurnScheduledV1 { .. } | CommittedEvent::ExtraTurnStartedV1 { .. }
        )));
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod future_printed {
    use super::*;
    use crate::card_def::{CardType, Keywords};
    use crate::engine::{self, Action, Decision};
    use crate::ids::ObjectId;
    use crate::mana::{ManaColor, Pip};
    use crate::state::{GameObject, ObjectStateV4, Target, Zone};

    pub(crate) fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
        let id = card_def::card_id_by_name(name).unwrap();
        let object = state.objects.push(GameObject {
            card_def: id,
            name: name.into(),
            owner,
            controller: owner,
            zone,
            tapped: false,
            summoning_sick: zone == Zone::Battlefield,
            damage: 0,
            counters: Default::default(),
            attachments: vec![],
            v4: ObjectStateV4::from_card_def(id),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        match zone {
            Zone::Hand => state.players[owner.index()].hand.push(object),
            Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
            _ => panic!("fixture zone"),
        }
        object
    }
    fn next(state: &mut GameState) -> Decision {
        let decision = engine::advance_until_decision(state);
        assert!(!matches!(decision, Decision::Halted { .. }));
        decision
    }
    pub(crate) fn cast(state: &mut GameState, actor: PlayerId) -> StackItem {
        let source = put(state, actor, "Temporal Manipulation", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        // A resolved prior spell may leave real opponent priority. Return it
        // through ordinary passes without changing the physical actor ledger.
        for _ in 0..2 {
            let decision = next(state);
            if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor.opponent()),
                "actual Temporal priority {decision:?}"
            );
            engine::step(state, Action::Pass).unwrap();
        }
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == actor && castable_spells.contains(&source))
        );
        engine::step(state, Action::CastSpell(source)).unwrap();
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        assert!(state.engine.pending_cast.is_none());
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        state.stack.last().unwrap().clone()
    }
    fn pass(state: &mut GameState, decision: Decision) {
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
            other => panic!("unexpected actual traversal {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    pub(crate) fn resolve(state: &mut GameState) {
        for _ in 0..20 {
            if state.stack.is_empty() {
                validate(state).unwrap();
                return;
            }
            let decision = next(state);
            // Advancing may resolve the last item and return the active
            // player's fresh priority window. Do not pass that new window:
            // a following genuine sorcery cast still needs this main phase.
            if state.stack.is_empty() {
                validate(state).unwrap();
                return;
            }
            pass(state, decision);
        }
        panic!("stack did not resolve");
    }
    pub(crate) fn next_turn(state: &mut GameState) {
        let key = state.turn_key_v1();
        for _ in 0..100 {
            let decision = next(state);
            if state.turn_key_v1() != key {
                assert_eq!(state.step, Step::Upkeep);
                return;
            }
            pass(state, decision);
        }
        panic!("actual Cleanup never reached next Untap");
    }
    pub(crate) fn main(state: &mut GameState) {
        for _ in 0..20 {
            let decision = next(state);
            if state.step == Step::Main1 {
                return;
            }
            pass(state, decision);
        }
        panic!("next main phase absent");
    }
    fn restore(state: &GameState) -> GameState {
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
        assert_eq!(state.state_hash(), restored.state_hash());
        restored
    }

    #[test]
    fn genuine_printed_cast_first_extra_draw_untap_stun_skip_and_next_regular_turn_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let definition = &card_def::CARD_DEFS
                [usize::from(card_def::card_id_by_name("Temporal Manipulation").unwrap())];
            assert_eq!(definition.cost.generic, 3);
            assert_eq!(
                definition.cost.pips,
                &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)]
            );
            assert_eq!(definition.cost.x_count, 0);
            assert_eq!(definition.types, &[CardType::Sorcery]);
            assert_eq!(definition.target_spec, TargetSpec::None);
            assert_eq!((definition.spell_effect)(), Some(recipe()));
            let land = put(&mut state, actor, "Forest", Zone::Battlefield);
            let skipped = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(land).tapped = true;
            state.objects.get_mut(land).counters.stun = 1;
            state.objects.get_mut(skipped).tapped = true;
            state.objects.get_mut(skipped).v4.skip_next_untap = true;
            state.objects.get_mut(skipped).counters.plus1_plus1 = 1;
            state.objects.get_mut(skipped).damage = 1;
            let item = cast(&mut state, actor);
            resolve(&mut state);
            assert_eq!(state.objects.get(item.source).zone, Zone::Graveyard);
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                next_turn(current);
                assert_eq!(current.active_player, actor);
                assert_eq!(current.turn, 1);
                assert_eq!(
                    current.extra_turn_v1.as_ref().unwrap().current_extra,
                    Some(item.v4.stack_item_id)
                );
                assert!(current.objects.get(land).tapped);
                assert_eq!(current.objects.get(land).counters.stun, 0);
                assert!(current.objects.get(skipped).tapped);
                assert!(!current.objects.get(skipped).v4.skip_next_untap);
                assert!(!current.objects.get(skipped).summoning_sick);
                assert_eq!(current.objects.get(skipped).damage, 0);
                main(current);
                assert_eq!(current.players[actor.index()].hand.len(), 1);
                next_turn(current);
                assert_eq!(current.active_player, actor.opponent());
                assert_eq!(current.turn, 1);
                assert_eq!(current.extra_turn_v1.as_ref().unwrap().current_extra, None);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }

    #[test]
    fn actual_virtual_copy_opposing_controller_queues_last_created_first_and_preserves_regular_order(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let original = cast(&mut state, actor);
            // Accepted physical spell-copy primitive, not a fabricated printed producer.
            let (copy_source, copy_id) =
                engine::copy_spell_for(&mut state, original.source, actor.opponent()).unwrap();
            resolve(&mut state);
            validate(&state).unwrap();
            for mutation in 0..2 {
                let mut corrupt = restore(&state);
                match mutation {
                    0 => corrupt.engine.event_history.retain(|event| !matches!(event,
                        CommittedEvent::ExtraTurnVirtualCeasedV1 {stack_item_id,..} if *stack_item_id==copy_id)),
                    1 => {
                        let cease = corrupt.engine.event_history.iter().find(|event|matches!(event,
                            CommittedEvent::ExtraTurnVirtualCeasedV1 {stack_item_id,..} if *stack_item_id==copy_id)).unwrap().clone();
                        corrupt.engine.event_history.push(cease);
                    }
                    _ => unreachable!(),
                }
                let mut restored = restore(&corrupt);
                let before = serde_json::to_vec(&restored).unwrap();
                assert!(engine::step(&mut restored, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&restored).unwrap(), before);
            }
            let queued = &state.extra_turn_v1.as_ref().unwrap().queued;
            assert_eq!(
                queued
                    .iter()
                    .map(|turn| turn.item.v4.stack_item_id)
                    .collect::<Vec<_>>(),
                vec![copy_id, original.v4.stack_item_id]
            );
            assert!(state.objects.get(copy_source).spell_copy_origin.is_some());
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                next_turn(current);
                assert_eq!(current.active_player, actor);
                assert_eq!(
                    current.extra_turn_v1.as_ref().unwrap().current_extra,
                    Some(original.v4.stack_item_id)
                );
                next_turn(current);
                assert_eq!(current.active_player, actor.opponent());
                assert_eq!(
                    current.extra_turn_v1.as_ref().unwrap().current_extra,
                    Some(copy_id)
                );
                next_turn(current);
                assert_eq!(current.active_player, actor.opponent());
                assert!(current
                    .extra_turn_v1
                    .as_ref()
                    .unwrap()
                    .current_extra
                    .is_none());
                assert_eq!(current.turn, 1);
                next_turn(current);
                assert_eq!(current.active_player, actor);
                assert_eq!(current.turn, 2);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }

    #[test]
    fn real_counterspell_short_payment_and_finalized_source_metadata_forgeries_refuse_readonly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = put(&mut state, actor, "Temporal Manipulation", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 2;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
            let original = cast(&mut state, actor);
            for mutation in 0..3 {
                let mut corrupt = restore(&state);
                match mutation {
                    0 => {
                        corrupt.stack.last_mut().unwrap().inline_effect =
                            Some(EffectOp::Sequence(vec![]))
                    }
                    1 => corrupt.stack.last_mut().unwrap().kind = StackItemKind::ActivatedAbility,
                    2 => {
                        corrupt.stack.last_mut().unwrap().controller = actor.opponent();
                        corrupt
                            .stack
                            .last_mut()
                            .unwrap()
                            .v4
                            .source_contract
                            .as_mut()
                            .unwrap()
                            .controller = actor.opponent();
                        corrupt.objects.get_mut(original.source).controller = actor.opponent();
                    }
                    _ => unreachable!(),
                }
                let before = serde_json::to_vec(&corrupt).unwrap();
                assert!(engine::step(&mut corrupt, Action::Pass).is_err());
                assert_eq!(before, serde_json::to_vec(&corrupt).unwrap());
            }
            let counter = put(&mut state, actor.opponent(), "Counterspell", Zone::Hand);
            state.players[actor.opponent().index()].mana_pool[ManaColor::U.pool_index()] = 2;
            engine::step(&mut state, Action::Pass).unwrap();
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
            );
            engine::step(&mut state, Action::CastSpell(counter)).unwrap();
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets == vec![Target::Object(original.source)])
            );
            engine::step(
                &mut state,
                Action::ChooseTarget(Target::Object(original.source)),
            )
            .unwrap();
            next(&mut state);
            let mut replay = restore(&state);
            resolve(&mut state);
            resolve(&mut replay);
            assert!(state.extra_turn_v1.is_none());
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }

    #[test]
    fn real_paid_publication_rejects_dropped_owner_and_manufactured_counter_closure_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let item = cast(&mut state, actor);
            assert!(state.engine.event_history.iter().any(|event|matches!(event,
                CommittedEvent::ExtraTurnOwnerPublishedV1 {item:original} if original.as_ref()==&item)));
            let mut dropped = restore(&state);
            dropped.stack.clear();
            let before = dropped.state_hash();
            assert!(engine::step(&mut dropped, Action::Pass).is_err());
            assert_eq!(dropped.state_hash(), before);
            dropped
                .engine
                .event_history
                .push(CommittedEvent::ExtraTurnOwnerRemovedV1 {
                    item: Box::new(item.clone()),
                    outcome: RemovalV1::Countered,
                });
            let mut forged = restore(&dropped);
            let before = forged.state_hash();
            assert!(engine::step(&mut forged, Action::Pass).is_err());
            assert_eq!(forged.state_hash(), before);
            let mut erased = restore(&state);
            erased
                .engine
                .event_history
                .retain(|event| !matches!(event, CommittedEvent::ExtraTurnOwnerPublishedV1 { .. }));
            let before = erased.state_hash();
            assert!(engine::step(&mut erased, Action::Pass).is_err());
            assert_eq!(erased.state_hash(), before);
        }
    }

    #[test]
    fn manufactured_resolved_schedule_without_actual_stack_departure_refuses_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut paid = ready(actor);
            let original = cast(&mut paid, actor);
            let mut resolved = restore(&paid);
            resolve(&mut resolved);
            validate(&resolved).unwrap();
            // Keep the original genuine publication. Copy only the mutable
            // claimed resolution/scheduler records, omitting the actual
            // physical Stack departure performed by genuine resolution.
            let mut forged = restore(&paid);
            forged.stack.clear();
            forged.extra_turn_v1 = resolved.extra_turn_v1.clone();
            forged.engine.event_history.extend(
                resolved
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| {
                        matches!(event,
                            CommittedEvent::ExtraTurnOwnerRemovedV1 {item,..}
                                | CommittedEvent::ExtraTurnScheduledV1 {item}
                                if item.v4.stack_item_id==original.v4.stack_item_id
                        ) || matches!(event, CommittedEvent::ExtraTurnStartedV1 { .. })
                    })
                    .cloned(),
            );
            for _ in 0..2 {
                let mut restored = restore(&forged);
                let before = serde_json::to_vec(&restored).unwrap();
                assert!(validate(&restored).is_err());
                assert!(engine::step(&mut restored, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&restored).unwrap(), before);
            }
            let mut erased = restore(&resolved);
            erased.engine.event_history.retain(|event| {
                !matches!(event,
                CommittedEvent::ZoneChange {object,from:Zone::Stack,..} if *object==original.source)
            });
            let before = erased.state_hash();
            assert!(engine::step(&mut erased, Action::Pass).is_err());
            assert_eq!(erased.state_hash(), before);
        }
    }

    #[test]
    fn genuine_virtual_counter_removal_has_independent_cease_authority_and_no_extra_turn() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let original = cast(&mut state, actor);
            let (_, copy_id) =
                engine::copy_spell_for(&mut state, original.source, actor.opponent()).unwrap();
            // Actual accepted counter primitive; the printed counter case is separate.
            engine::counter_stack_item_by_id(&mut state, copy_id).unwrap();
            validate(&state).unwrap();
            assert!(state.engine.event_history.iter().any(|event|matches!(event,
                CommittedEvent::ExtraTurnVirtualCeasedV1 {stack_item_id,..} if *stack_item_id==copy_id)));
            let mut erased = restore(&state);
            erased
                .engine
                .event_history
                .retain(|event| !matches!(event, CommittedEvent::ExtraTurnVirtualCeasedV1 { .. }));
            let before = erased.state_hash();
            assert!(engine::step(&mut erased, Action::Pass).is_err());
            assert_eq!(erased.state_hash(), before);
            resolve(&mut state);
            assert_eq!(state.extra_turn_v1.as_ref().unwrap().queued.len(), 1);
            assert_eq!(
                state.extra_turn_v1.as_ref().unwrap().queued[0]
                    .item
                    .v4
                    .stack_item_id,
                original.v4.stack_item_id
            );
        }
    }

    #[test]
    fn another_actual_cast_during_extra_turn_has_distinct_turn_uses_and_draw_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            // Select the genuine first-life-gain consumer through the pool,
            // leaving it below the Forest draw prefix and off the battlefield.
            let forest = card_def::card_id_by_name("Forest").unwrap();
            let seraph = card_def::card_id_by_name("Vanguard Seraph").unwrap();
            let mut library = vec![forest; 39];
            library.push(seraph);
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &library,
                &library,
                crate::rl::card_name,
                110_423,
                actor,
            );
            state.step = Step::Main1;
            assert!(state.life_gain_turn_v1.is_some());
            cast(&mut state, actor);
            resolve(&mut state);
            assert_eq!(state.players[actor.index()].spells_cast_this_turn, 1);
            next_turn(&mut state);
            main(&mut state);
            assert_eq!(state.players[actor.index()].spells_cast_this_turn, 0);
            let life_before_loss = state.players[actor.index()].life;
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::life_loss(actor, 1),
            );
            assert_eq!(state.players[actor.index()].life, life_before_loss - 1);
            let gain = CommittedEvent::LifeGain {
                player: actor.opponent(),
                amount: 1,
            };
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::life_gain(actor.opponent(), 1),
            );
            assert_eq!(
                crate::life_gain_turn_v1::event_ordinals(&state, std::slice::from_ref(&gain))
                    .unwrap(),
                vec![Some(1)]
            );
            let gain_anchor = state.life_gain_turn_v1.as_ref().unwrap().history_index;
            let second = cast(&mut state, actor);
            resolve(&mut state);
            let key = state.turn_key_v1();
            next_turn(&mut state);
            assert_eq!(state.active_player, actor);
            assert_eq!(state.turn, 1);
            assert!(state.turn_key_v1() > key);
            assert_eq!(state.own_turn_count_v1(actor), 3);
            assert_eq!(state.players[actor.index()].life, life_before_loss - 1);
            let history = state.life_gain_turn_v1.as_ref().unwrap();
            assert_eq!(history.turn, state.turn_key_v1());
            assert_eq!(history.active_player, actor);
            assert!(history.history_index > gain_anchor);
            assert!(matches!(
                state.engine.event_history.get(history.history_index),
                Some(CommittedEvent::LifeGainTurnBeganV1 { turn, active_player })
                    if *turn == state.turn_key_v1() && *active_player == actor
            ));
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::life_gain(actor.opponent(), 1),
            );
            assert_eq!(
                crate::life_gain_turn_v1::event_ordinals(&state, std::slice::from_ref(&gain))
                    .unwrap(),
                vec![Some(1)],
                "the actual extra-turn Untap resets the selected gain consumer"
            );
            assert_eq!(state.players[actor.index()].spells_cast_this_turn, 0);
            assert_eq!(
                state.extra_turn_v1.as_ref().unwrap().current_extra,
                Some(second.v4.stack_item_id)
            );
            main(&mut state);
            assert_eq!(state.players[actor.index()].hand.len(), 2);
            next_turn(&mut state);
            assert_eq!(state.active_player, actor.opponent());
            assert_eq!(state.turn, 1);
            assert_eq!(state.own_turn_count_v1(actor.opponent()), 1);
        }
    }

    #[test]
    fn queue_erasure_reorder_replay_and_original_source_reentry_refuse_after_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let item = cast(&mut state, actor);
            resolve(&mut state);
            for mutation in 0..4 {
                let mut corrupt = restore(&state);
                match mutation {
                    0 => corrupt.extra_turn_v1.as_mut().unwrap().queued.clear(),
                    1 => corrupt.extra_turn_v1 = None,
                    2 => {
                        corrupt.extra_turn_v1.as_mut().unwrap().queued[0]
                            .item
                            .controller = actor.opponent()
                    }
                    3 => corrupt.stack.push(item.clone()),
                    _ => unreachable!(),
                }
                for _ in 0..2 {
                    let mut restored = restore(&corrupt);
                    let before = serde_json::to_vec(&restored).unwrap();
                    assert!(engine::step(&mut restored, Action::Pass).is_err());
                    assert_eq!(before, serde_json::to_vec(&restored).unwrap());
                }
            }
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(item.source, Zone::Hand),
            );
            validate(&state).unwrap();
            next_turn(&mut state);
            assert_eq!(state.active_player, actor);
            assert_eq!(
                state.extra_turn_v1.as_ref().unwrap().current_extra,
                Some(item.v4.stack_item_id)
            );
        }
    }

    #[test]
    fn holder_next_turn_effects_and_until_next_turn_impulse_expire_on_actual_extra_boundaries() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let creature = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let source = put(&mut state, actor, "Forest", Zone::Battlefield);
            let mut ctx = ExecCtx::no_targets(source, actor);
            ctx.targets = vec![Target::Object(creature)];
            ctx.target_contracts = vec![crate::state::StackTargetContractV4::capture(
                &state,
                ctx.targets[0],
            )];
            effect::execute(
                &EffectOp::GoadTargetUntilSourcesNextTurn {
                    object: effect::ObjectRef::Target(0),
                },
                &ctx,
                &mut state,
            );
            // Exact physical keyword primitive, not an admitted printed source.
            state
                .engine
                .until_next_turn_keywords
                .push(engine::UntilNextTurnKeywordEffectV1 {
                    object_id: creature,
                    object_zone_change_count: state.objects.get(creature).zone_change_count,
                    holder: actor,
                    expires_at_turn: 2,
                    keywords: Keywords::HEXPROOF,
                    timestamp: None,
                });
            effect::execute(
                &EffectOp::ImpulseDraw {
                    count: 1,
                    duration: effect::ImpulseDuration::UntilOwnersNextTurn,
                },
                &ctx,
                &mut state,
            );
            let exiled = state.engine.exile_play_permissions[0].object;
            cast(&mut state, actor);
            resolve(&mut state);
            next_turn(&mut state);
            assert!(state.objects.get(creature).v4.goaded_by.is_empty());
            assert!(state.engine.until_next_turn_keywords.is_empty());
            assert!(state
                .engine
                .exile_play_permissions
                .iter()
                .any(|permission| permission.object == exiled));
            next_turn(&mut state);
            assert!(!state
                .engine
                .exile_play_permissions
                .iter()
                .any(|permission| permission.object == exiled));
        }
    }
}
