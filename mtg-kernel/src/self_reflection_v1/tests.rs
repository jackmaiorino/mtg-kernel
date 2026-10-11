//! Registered physical primitive fixtures, not printed Self-Reflection admission.
use super::*;
use crate::card_def::{card_id_by_name, DynamicValueDef, Keywords};
use crate::effect::{ObjectRef, TargetRef};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::Zone;

fn fixture(actor: PlayerId) -> (GameState, ObjectId, ExecCtx) {
    let mut state = ready_state();
    let source = put(&mut state, actor, "Counterspell", Zone::Hand);
    let creature = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change_to_battlefield_under_controller(creature, actor),
    );
    let mut ctx = ExecCtx::no_targets(source, actor);
    ctx.targets = vec![Target::Object(creature)];
    ctx.target_contracts = vec![StackTargetContractV4::capture(&state, ctx.targets[0])];
    (state, creature, ctx)
}
#[test]
fn controlled_copy_uses_current_values_and_new_token_owner_without_copying_incidental_state() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, creature, ctx) = fixture(actor);
        state.objects.get_mut(creature).tapped = true;
        state.objects.get_mut(creature).damage = 1;
        state.objects.get_mut(creature).counters.plus1_plus1 = 2;
        crate::effect::execute(
            &EffectOp::PumpTargetUntilEndOfTurnDynamic {
                target: TargetRef::Target(0),
                power: DynamicValueDef::Fixed(3),
                toughness: DynamicValueDef::Fixed(3),
            },
            &ctx,
            &mut state,
        );
        crate::effect::execute(
            &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: Keywords::HASTE,
            },
            &ctx,
            &mut state,
        );
        let saved = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
        for current in [&mut state, &mut restored] {
            let before = current.players[actor.index()].battlefield.len();
            execute(current, &ctx);
            assert_eq!(current.players[actor.index()].battlefield.len(), before + 1);
            let token = *current.players[actor.index()].battlefield.last().unwrap();
            let live = current.objects.get(token);
            assert_eq!(live.card_def, card_id_by_name("Llanowar Elves").unwrap());
            assert_eq!((live.owner, live.controller), (actor, actor));
            assert!(live.v4.is_token && live.summoning_sick);
            assert!(!live.tapped);
            assert_eq!(live.damage, 0);
            assert_eq!(live.counters.plus1_plus1, 0);
            assert!(live.attachments.is_empty());
            assert_eq!(crate::engine::effective_power(current, token), 1);
            assert!(!crate::engine::has_effective_keyword(
                current,
                token,
                Keywords::HASTE
            ));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
#[test]
fn anonymous_copy_values_contain_no_hidden_identity_before_any_entry_event() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, first, _) = fixture(actor);
        let second = put(&mut state, actor, "Tolarian Terror", Zone::Battlefield);
        for object in [first, second] {
            // Genuine registered hidden-object primitive. This does not claim
            // a printed FDN face-down producer or register the new spell.
            state.objects.get_mut(object).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: true,
                lookers: 1 << actor.index(),
                hidden_by: None,
                hidden_by_source: None,
            });
        }
        let before = serde_json::to_vec(&state).unwrap();
        assert_ne!(
            state.objects.get(first).card_def,
            state.objects.get(second).card_def
        );
        assert_eq!(
            copiable_values(&state, first),
            CopyTokenValuesV1::AnonymousFaceDown
        );
        assert_eq!(
            copiable_values(&state, first),
            copiable_values(&state, second)
        );
        assert_eq!(
            format!("{:?}", copiable_values(&state, first)),
            "AnonymousFaceDown"
        );
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        for name in ["Llanowar Elves", "Tolarian Terror", "Counterspell"] {
            assert!(!anonymous_blueprint_is_exact(
                card_id_by_name(name).unwrap()
            ));
        }
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn registered_zonechange_and_copy_token_etb_capture_exact_source_then_survive_departure() {
    use crate::engine::{self, Action, Decision};
    use crate::state::StackItemKind;
    fn settle(state: &mut GameState) {
        for _ in 0..40 {
            match engine::advance_until_decision(state) {
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("unexpected actual ETB decision: {other:?}"),
            }
        }
        panic!("ETB did not finish");
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        for token_copy in [false, true] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            let original = put(&mut state, actor, "Healer of the Glade", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(original, Zone::Battlefield),
            );
            let entrant = if token_copy {
                settle(&mut state);
                crate::standard_cards_v1::create_token_copy(&mut state, original, actor).unwrap()
            } else {
                original
            };
            let generation = state.objects.get(entrant).zone_change_count;
            let life = state.players[actor.index()].life;
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            let item = state.stack.last().expect("real entry must publish its ETB");
            assert_eq!(item.kind, StackItemKind::TriggeredAbility);
            assert_eq!(item.source, entrant);
            assert_eq!(item.controller, actor);
            let captured = item.v4.ability_source_contract.unwrap();
            assert_eq!(captured.source, entrant);
            assert_eq!(captured.zone, Zone::Battlefield);
            assert_eq!(captured.zone_change_count, generation);
            assert_eq!(captured.controller, actor);
            assert_eq!(state.players[actor.index()].life, life);
            assert!(state.engine.event_history.iter().any(|entry| match entry {
                crate::event::CommittedEvent::CreateToken {
                    object, controller, ..
                } => token_copy && *object == entrant && *controller == actor,
                crate::event::CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                } => !token_copy && *object == entrant,
                _ => false,
            }));
            crate::standard_cards_v1::gain_control(&mut state, entrant, actor.opponent());
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(entrant, Zone::Hand));
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for current in [&mut state, &mut restored] {
                settle(current);
                assert_eq!(current.players[actor.index()].life, life + 3);
                assert_eq!(current.players[actor.opponent().index()].life, 20);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn registered_hidden_reference_refuses_missing_sentinel_without_underlying_identity() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, creature, _) = fixture(actor);
        for disguised in [false, true] {
            state.objects.get_mut(creature).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised,
                lookers: 1 << actor.index(),
                hidden_by: None,
                hidden_by_source: None,
            });
            let before = serde_json::to_vec(&state).unwrap();
            if card_id_by_name(if disguised {
                "Face-down creature"
            } else {
                "Face-down card"
            })
            .is_none()
            {
                for viewer in [actor, actor.opponent()] {
                    let error = crate::rl::observe_v1(&state, viewer, 0).unwrap_err();
                    assert!(error.to_string().contains("neutral sentinel"));
                    assert!(!error.to_string().contains("Llanowar Elves"));
                }
            } else {
                for viewer in [actor, actor.opponent()] {
                    let obs = crate::rl::observe_v1(&state, viewer, 0).unwrap();
                    let card = obs.projection.battlefield[actor.index()]
                        .iter()
                        .find(|c| c.stable.arena_id == creature.0)
                        .unwrap();
                    assert_ne!(card.stable.card_db_id, state.objects.get(creature).card_def);
                    assert!(!serde_json::to_string(card)
                        .unwrap()
                        .contains("Llanowar Elves"));
                }
            }
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}
#[test]
fn exact_target_departure_reentry_control_change_and_monocolored_protection_refuse_before_copy() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for failure in 0..4 {
            let (mut state, creature, ctx) = fixture(actor);
            match failure {
                0 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(creature, Zone::Hand),
                    );
                }
                1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(creature, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change_to_battlefield_under_controller(creature, actor),
                    );
                }
                2 => crate::standard_cards_v1::gain_control(&mut state, creature, actor.opponent()),
                _ => crate::effect::execute(
                    &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                        object: ObjectRef::Target(0),
                        keyword: Keywords::PROTECTION_FROM_MONOCOLORED,
                    },
                    &ctx,
                    &mut state,
                ),
            }
            let before = serde_json::to_vec(&state).unwrap();
            execute(&mut state, &ctx);
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}

#[cfg(feature = "standard-magezero-fixtures")]
#[test]
fn accepted_aegis_copy_changes_values_without_losing_physical_target_incarnation() {
    use crate::effect::EffectObjectBinding;
    use crate::state::{AbilitySourceContractV4, ObjectLinkV4};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, ctx) = fixture(actor);
        let aegis = put(&mut state, actor, "Assimilation Aegis", Zone::Battlefield);
        let exiled = put(&mut state, actor.opponent(), "Tolarian Terror", Zone::Hand);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(exiled, Zone::Exile));
        // Genuine registered Aegis physical attachment/copy primitives. The
        // original creature spell/root is not replaced by the prepared spell.
        state
            .attach_object_exact(aegis, 0, host, state.objects.get(host).zone_change_count)
            .unwrap();
        let source = AbilitySourceContractV4::capture(&state, aegis);
        state.objects.get_mut(exiled).v4.exiled_by = Some(ObjectLinkV4 {
            object: aegis,
            zone_change_count: source.zone_change_count,
        });
        let mut copy_ctx = ExecCtx::no_targets(aegis, actor);
        copy_ctx.ability_source_contract = Some(source);
        let host_link = ObjectLinkV4 {
            object: host,
            zone_change_count: state.objects.get(host).zone_change_count,
        };
        let timestamp = state.objects.get(aegis).v4.layer_timestamp.unwrap();
        let exiled_generation = state.objects.get(exiled).zone_change_count;
        crate::standard_cards_v1::apply_aegis_copy(
            &mut state,
            &copy_ctx,
            host_link,
            timestamp,
            EffectObjectBinding {
                object: exiled,
                expected_zone: Zone::Exile,
                expected_zone_change_count: exiled_generation,
            },
        );
        assert_eq!(
            state.objects.get(host).card_def,
            card_id_by_name("Tolarian Terror").unwrap()
        );
        assert!(!crate::engine::target_contract_matches_live(
            &state,
            ctx.targets[0],
            ctx.target_contracts[0]
        ));
        assert!(target_matches_live(
            &state,
            ctx.targets[0],
            ctx.target_contracts[0]
        ));
        let saved = serde_json::to_vec(&state).unwrap();
        let mut state: GameState = serde_json::from_slice(&saved).unwrap();
        execute(&mut state, &ctx);
        let token = *state.players[actor.index()].battlefield.last().unwrap();
        assert_eq!(
            state.objects.get(token).card_def,
            card_id_by_name("Tolarian Terror").unwrap()
        );
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(aegis, Zone::Hand));
        assert_eq!(
            state.objects.get(host).card_def,
            card_id_by_name("Llanowar Elves").unwrap()
        );
        assert_eq!(
            state.objects.get(token).card_def,
            card_id_by_name("Tolarian Terror").unwrap()
        );
        let mut forged = ctx.target_contracts[0];
        if let StackTargetContractV4::Object { card_def, .. } = &mut forged {
            *card_def = card_id_by_name("Forest").unwrap();
        }
        assert!(!target_matches_live(&state, ctx.targets[0], forged));
    }
}
