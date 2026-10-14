use super::*;
use crate::effect::{self, EffectOp, ExecCtx, ObjectRef};
use crate::engine::{self, Action, Decision};
use crate::event::{self, ProposedEvent};
use crate::ids::ObjectId;
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::{Step, Target};

pub(crate) fn primitive(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    primitive_with_face(actor, None)
}
fn primitive_with_face(actor: PlayerId, face: Option<bool>) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready_state_for(actor);
    let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    let recipient = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.objects.get_mut(recipient).tapped = true;
    if let Some(disguised) = face {
        state.objects.get_mut(recipient).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
    }
    put(&mut state, actor, "Forest", Zone::Hand);
    for owner in [actor, actor.opponent()] {
        for _ in 0..8 {
            put(&mut state, owner, "Forest", Zone::Library);
        }
    }
    let mut ctx = ExecCtx::no_targets(source, actor);
    ctx.targets.push(Target::Object(recipient));
    ctx.target_contracts.push(StackTargetContractV4::capture(
        &state,
        Target::Object(recipient),
    ));
    effect::execute(
        &EffectOp::Sequence(vec![
            EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
            EffectOp::UntapObject {
                object: ObjectRef::Target(0),
            },
            EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: crate::card_def::Keywords::HASTE,
            },
        ]),
        &ctx,
        &mut state,
    );
    authority::validate(&state).unwrap();
    (state, source, recipient)
}
pub(crate) fn cleanup(state: &mut GameState) {
    state.step = Step::End;
    state.engine.priority_passes = [false; 2];
    for _ in 0..24 {
        let decision = engine::advance_until_decision(state);
        if !state.engine.until_end_of_turn.iter().any(|row| {
            matches!(
                row,
                engine::UntilEndOfTurnEffect::ResolvedObjectControlEffect { .. }
            )
        }) {
            authority::validate(state).unwrap();
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("control cleanup did not occur");
}
#[test]
fn absent_successor_matches_all_real_predecessor_fields_and_hidden_substitution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        let hidden = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let surface = PolicySurfaceV5::new();
        let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        public.validate_artifact().unwrap();
        assert_eq!(public.projection, old.projection);
        assert_eq!(public.extensions, old.extensions);
        assert_eq!(public.known_face_down_cards, old.known_face_down_cards);
        assert!(public.installations.is_empty());
        let definition = crate::card_def::card_id_by_name("Island").unwrap();
        let object = state.objects.get_mut(hidden);
        object.card_def = definition;
        object.name = "Island".into();
        object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
        assert_eq!(
            observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
        for alteration in 0..4 {
            let mut forged = public.clone();
            match alteration {
                0 => forged.schema.push('x'),
                1 => forged.producer_sha256.clear(),
                2 => forged.card_db_hash ^= 1,
                _ => forged.roots.push(RootV1 {
                    card_id: crate::card_def::card_id_by_name("Forest").unwrap(),
                    root_sha256: String::new(),
                }),
            };
            forged.projection_sha256 = commitment(&forged).unwrap();
            assert!(forged.validate_artifact().is_err());
        }
    }
}
#[test]
fn registered_primitive_control_current_haste_departure_restore_expiry_and_private_source() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = primitive(actor);
        let surface = PolicySurfaceV5::new();
        for viewer in [actor, actor.opponent()] {
            let public = observe_v1(&state, &surface, viewer, 0, 0, 0, 1).unwrap();
            let row = &public.installations[0];
            assert_eq!(row.origin, OriginV1::RegisteredPrimitive);
            assert_eq!(row.source.is_some(), viewer == actor);
            assert_eq!(row.original_controller, actor.opponent().into());
            assert_eq!(row.captured_controller, actor.into());
            assert_eq!(
                row.current_recipient.as_ref().unwrap().controller,
                actor.into()
            );
            assert_eq!(row.current_tapped, Some(false));
            assert_eq!(row.current_has_haste, Some(true));
            assert!(!row.expired);
            assert!(crate::rl::observe_policy_v8(&state, &surface, viewer, 0, 0, 0, 1).is_err());
            assert!(crate::policy_observation_v9::observe_policy_v9(
                &state, &surface, viewer, 0, 0, 0, 1
            )
            .is_err());
            let text = serde_json::to_string(&public).unwrap();
            assert!(!text.contains("recipient_snapshot"));
            assert!(!text.contains("resolution_history_index"));
        }
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        cleanup(&mut state);
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let row = &public.installations[0];
        assert!(row.expired);
        assert!(row.cleanup_turn.is_some());
        assert_eq!(
            row.current_recipient.as_ref().unwrap().controller,
            actor.opponent().into()
        );
        assert_eq!(row.current_has_haste, Some(false));
        assert_eq!(state.objects.get(recipient).owner, actor.opponent());
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(recipient, Zone::Exile),
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(recipient, Zone::Battlefield),
        );
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert!(public.installations[0].current_recipient.is_none());
        assert_eq!(public.installations[0].current_has_haste, None);
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            observe_v1(&restored, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
    }
}
#[test]
fn registered_primitive_forgery_deletion_replay_and_snapshot_mask_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = primitive(actor);
        let surface = PolicySurfaceV5::new();
        for alteration in 0..5 {
            let mut forged = state.clone();
            match alteration {
                0 => forged.engine.until_end_of_turn.clear(),
                1 => forged
                    .engine
                    .event_history
                    .retain(|e| !matches!(e, CommittedEvent::TemporaryControlInstalledV1 { .. })),
                2 => {
                    let row = forged
                        .engine
                        .event_history
                        .iter()
                        .find(|e| matches!(e, CommittedEvent::TemporaryControlInstalledV1 { .. }))
                        .unwrap()
                        .clone();
                    forged.engine.event_log.push(row);
                }
                _ => {
                    let CommittedEvent::TemporaryControlInstalledV1 { installation } = forged
                        .engine
                        .event_history
                        .iter_mut()
                        .find(|e| matches!(e, CommittedEvent::TemporaryControlInstalledV1 { .. }))
                        .unwrap()
                    else {
                        unreachable!()
                    };
                    let mut snapshot: GameObject =
                        serde_json::from_str(&installation.recipient_snapshot).unwrap();
                    if alteration == 3 {
                        snapshot.owner = actor;
                    } else {
                        snapshot.zone_change_count += 1;
                    }
                    installation.recipient_snapshot = serde_json::to_string(&snapshot).unwrap();
                }
            }
            let before = serde_json::to_vec(&forged).unwrap();
            for viewer in [actor, actor.opponent()] {
                assert!(observe_v1(&forged, &surface, viewer, 0, 0, 0, 1).is_err());
                let restored: GameState = serde_json::from_slice(&before).unwrap();
                assert!(observe_v1(&restored, &surface, viewer, 0, 0, 0, 1).is_err());
            }
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
    }
}
pub(crate) fn ward_state(actor: PlayerId) -> GameState {
    let mut state = ready_state_for(actor);
    let recipient = put(
        &mut state,
        actor.opponent(),
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let spells =
        ["Lightning Bolt", "Lightning Bolt"].map(|name| put(&mut state, actor, name, Zone::Hand));
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 8;
    for spell in spells {
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
    }
    crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
    state
}
#[test]
fn registered_real_ward_successor_preserves_extensions() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = ward_state(actor);
        let surface = PolicySurfaceV5::new();
        let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(old.extensions, public.extensions);
        assert_eq!(old.projection, public.projection);
        assert!(public.extensions.pending_ward_payment.is_some());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn printed(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready_state_for(actor);
    let source = put(&mut state, actor, "Involuntary Employment", Zone::Hand);
    let recipient = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.objects.get_mut(recipient).tapped = true;
    for owner in [actor, actor.opponent()] {
        put(&mut state, owner, "Lightning Bolt", Zone::Hand);
        for _ in 0..8 {
            put(&mut state, owner, "Forest", Zone::Library);
        }
    }
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 2;
    state.players[actor.opponent().index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 3;
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
    for _ in 0..12 {
        let decision = engine::advance_until_decision(&mut state);
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        if state.stack.is_empty() && state.objects.get(source).zone == Zone::Graveyard {
            return (state, source, recipient);
        }
        engine::step(&mut state, Action::Pass).unwrap();
    }
    panic!("Employment did not resolve");
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn actual_future_printed_original_stack_target_controller_snapshot_and_cleanup() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = printed(actor);
        let surface = PolicySurfaceV5::new();
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let row = &public.installations[0];
        assert_eq!(row.origin, OriginV1::PrintedSpell);
        assert_eq!(row.source.as_ref().unwrap().arena_id, source.0);
        assert_eq!(row.source.as_ref().unwrap().zone, Zone::Stack);
        assert_eq!(row.recipient.arena_id, recipient.0);
        assert_eq!(row.current_has_haste, Some(true));
        assert_eq!(row.current_tapped, Some(false));
        assert_eq!(row.original_controller, actor.opponent().into());
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            observe_v1(&restored, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
        let targeted: Vec<_> = state
            .engine
            .event_history
            .iter()
            .filter(|e| matches!(e, CommittedEvent::Targeted { .. }))
            .cloned()
            .collect();
        let mut forged = state.clone();
        for event in &mut forged.engine.event_history {
            match event {
                CommittedEvent::TemporaryControlInstalledV1 { installation } => {
                    installation.controller = actor.opponent();
                    installation.source.controller = actor.opponent();
                }
                CommittedEvent::TemporaryControlResolutionV1 { item, .. } => {
                    item.controller = actor.opponent();
                    item.v4.source_contract.as_mut().unwrap().controller = actor.opponent();
                }
                _ => {}
            }
        }
        for effect in &mut forged.engine.until_end_of_turn {
            if let engine::UntilEndOfTurnEffect::ResolvedObjectControlEffect {
                controller, ..
            } = effect
            {
                *controller = actor.opponent();
            }
        }
        forged.players[actor.index()]
            .battlefield
            .retain(|id| *id != recipient);
        forged.players[actor.opponent().index()]
            .battlefield
            .push(recipient);
        forged.objects.get_mut(recipient).controller = actor.opponent();
        assert_eq!(
            forged
                .engine
                .event_history
                .iter()
                .filter(|e| matches!(e, CommittedEvent::Targeted { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            targeted
        );
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(observe_v1(&forged, &surface, actor, 0, 0, 0, 1).is_err());
        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        cleanup(&mut state);
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert!(public.installations[0].expired);
        assert_eq!(
            public.installations[0]
                .current_recipient
                .as_ref()
                .unwrap()
                .controller,
            actor.opponent().into()
        );
        assert_eq!(public.installations[0].current_has_haste, Some(false));
    }
}

#[test]
fn historical_registered_face_down_mask_uses_installation_snapshot_after_departure() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for disguised in [false, true] {
            let (mut state, _, recipient) = primitive_with_face(actor, Some(disguised));
            // The generic face-down card has no creature type. The
            // typed control primitive correctly has no installation for that
            // illegal target; the disguised 2/2 remains a legal creature.
            if !disguised {
                assert!(!crate::engine::effect_target_is_legal(
                    &state,
                    recipient,
                    actor,
                    crate::card_def::TargetSpec::Creature,
                    &[Target::Object(recipient)],
                    0,
                ));
                assert_eq!(state.objects.get(recipient).controller, actor.opponent());
                assert!(installations(&state, actor).unwrap().is_empty());
                let restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                assert!(installations(&restored, actor).unwrap().is_empty());
                continue;
            }
            let surface = PolicySurfaceV5::new();
            let name = if disguised {
                "Face-down creature"
            } else {
                "Face-down card"
            };
            if let Some(sentinel) = crate::card_def::card_id_by_name(name) {
                let rows = installations(&state, actor).unwrap();
                assert_eq!(rows[0].recipient.card_db_id, sentinel);
                assert_eq!(
                    rows[0].current_recipient.as_ref().unwrap().card_db_id,
                    sentinel
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(recipient, Zone::Exile),
                );
                let rows = installations(&state, actor).unwrap();
                assert_eq!(rows[0].recipient.card_db_id, sentinel);
                assert!(rows[0].current_recipient.is_none());
                let restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                assert_eq!(installations(&restored, actor).unwrap(), rows);
            } else {
                let before = serde_json::to_vec(&state).unwrap();
                assert!(installations(&state, actor).is_err());
                assert!(observe_v1(&state, &surface, actor, 0, 0, 0, 1).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            }
        }
    }
}
