use super::*;
use crate::policy_observation_v6::tests::{put, ready_state, ready_state_for};

#[test]
fn ordinary_public_parts_and_hidden_identity_parity_keep_real_predecessor_fields() {
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
        assert!(public.restrictions.is_empty());
        let definition = crate::card_def::card_id_by_name("Island").unwrap();
        let object = state.objects.get_mut(hidden);
        object.card_def = definition;
        object.name = "Island".into();
        object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
        assert_eq!(
            observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
        let mut bad = public.clone();
        bad.schema = "mtg-kernel-policy-observation/v9".into();
        assert!(bad.validate_artifact().is_err());
        let mut bad = public;
        bad.producer_sha256.clear();
        assert!(bad.validate_artifact().is_err());
        let mut bad = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        bad.roots.push(RootV1 {
            card_id: crate::card_def::card_id_by_name("Forest").unwrap(),
            trigger_index: 0,
            root_sha256: String::new(),
        });
        bad.projection_sha256 = commitment(&bad).unwrap();
        assert!(bad.validate_artifact().is_err());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn actual_attack(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    use crate::engine::{self, Action, Decision};
    let mut state = ready_state_for(actor);
    let source = put(&mut state, actor, "Frenzied Goblin", Zone::Battlefield);
    let recipient = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
        put(&mut state, player, "Mountain", Zone::Battlefield);
        for _ in 0..4 {
            put(&mut state, player, "Forest", Zone::Library);
        }
    }
    for _ in 0..20 {
        let decision = engine::advance_until_decision(&mut state);
        if let Decision::DeclareAttackers { eligible, .. } = decision {
            assert!(eligible.contains(&source));
            engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
            break;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(&mut state, Action::Pass).unwrap();
    }
    assert!(matches!(engine::advance_until_decision(&mut state),
        Decision::ChooseTargets {spell,..} if spell==source));
    (state, source, recipient)
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn actual_targeted_attack(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    use crate::engine::{self, Action, Decision};
    use crate::state::Target;
    let (mut state, source, recipient) = actual_attack(actor);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    (state, source, recipient)
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn actual_payment(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    let (mut state, source, recipient) = actual_targeted_attack(actor);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    for _ in 0..10 {
        let decision = engine::advance_until_decision(&mut state);
        if matches!(decision,Decision::ChooseEffectBoolean {source:s,..} if s==source) {
            return (state, source, recipient);
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(&mut state, Action::Pass).unwrap();
    }
    panic!("actual attack payment not reached");
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn printed_public_payment_closed_restriction_restore_and_receipt_forgeries() {
    use crate::engine::{self, Action, Decision};
    use crate::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = actual_payment(actor);
        let surface = PolicySurfaceV5::new();
        let before = serde_json::to_vec(&state).unwrap();
        let pending = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(pending.resolutions.len(), 1);
        assert_eq!(pending.resolutions[0].phase, "optional_red_payment");
        assert_eq!(pending.resolutions[0].captured_payer, actor.into());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        for case in 0..6 {
            let mut corrupt: GameState = serde_json::from_slice(&before).unwrap();
            match case {
                0 => corrupt.engine.pending_effect = None,
                1 => corrupt.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::FrenziedGoblinPublishedV1 { .. })
                }),
                2 => corrupt.stack.clear(),
                3 => {
                    for event in &mut corrupt.engine.event_history {
                        if let CommittedEvent::FrenziedGoblinPublishedV1 { publication } = event {
                            publication.target_at_placement.zone_change_count += 1;
                        }
                    }
                }
                4 => {
                    let contract = crate::state::StackTargetContractV4::capture(
                        &corrupt,
                        crate::state::Target::Object(source),
                    );
                    let snapshot = serde_json::to_string(corrupt.objects.get(source)).unwrap();
                    let public_ref = crate::sower_restriction_capture_v1::masked_ref(
                        source,
                        corrupt.objects.get(source),
                    )
                    .unwrap();
                    for event in &mut corrupt.engine.event_history {
                        match event {
                            CommittedEvent::FrenziedGoblinPublishedV1 { publication } => {
                                publication.item.targets =
                                    vec![crate::state::Target::Object(source)];
                                publication.item.v4.target_contracts = vec![contract];
                                publication.target_snapshot = snapshot.clone();
                                publication.target_at_placement = public_ref.clone();
                            }
                            CommittedEvent::FrenziedGoblinResolutionBoundaryV1 { receipt } => {
                                if let Some(pending) = receipt.pending.as_mut() {
                                    pending.resolving_item.targets =
                                        vec![crate::state::Target::Object(source)];
                                    pending.resolving_item.v4.target_contracts = vec![contract];
                                    pending.ctx.targets =
                                        vec![crate::state::Target::Object(source)];
                                    pending.ctx.target_contracts = vec![contract];
                                }
                            }
                            _ => {}
                        }
                    }
                    for item in &mut corrupt.stack {
                        item.targets = vec![crate::state::Target::Object(source)];
                        item.v4.target_contracts = vec![contract];
                    }
                    let pending = corrupt.engine.pending_effect.as_mut().unwrap();
                    pending.resolving_item.targets = vec![crate::state::Target::Object(source)];
                    pending.resolving_item.v4.target_contracts = vec![contract];
                    pending.ctx.targets = vec![crate::state::Target::Object(source)];
                    pending.ctx.target_contracts = vec![contract];
                    // Genuine committed Targeted remains the independent original recipient.
                }
                _ => {
                    for event in &mut corrupt.engine.event_history {
                        if let CommittedEvent::FrenziedGoblinPublishedV1 { publication } = event {
                            let mut snapshot: crate::state::GameObject =
                                serde_json::from_str(&publication.target_snapshot).unwrap();
                            snapshot.card_def = crate::card_def::card_id_by_name("Forest").unwrap();
                            snapshot.name = "Forest".into();
                            snapshot.v4 =
                                crate::state::ObjectStateV4::from_card_def(snapshot.card_def);
                            publication.target_snapshot = serde_json::to_string(&snapshot).unwrap();
                            publication.target_at_placement =
                                crate::sower_restriction_capture_v1::masked_ref(
                                    recipient, &snapshot,
                                )
                                .unwrap();
                            // The actual placement contract still records Llanowar Elves.
                        }
                    }
                }
            }
            let bytes = serde_json::to_vec(&corrupt).unwrap();
            assert!(observe_v1(&corrupt, &surface, actor, 0, 0, 0, 1).is_err());
            assert_eq!(serde_json::to_vec(&corrupt).unwrap(), bytes);
            assert!(engine::step(&mut corrupt, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&corrupt).unwrap(), bytes);
        }
        engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
        let decision = engine::advance_until_decision(&mut state);
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(public.restrictions.len(), 1);
        assert_eq!(public.restrictions[0].recipient.arena_id, recipient.0);
        assert!(public.restrictions[0].effective);
        assert_eq!(
            public.resolutions[0].outcome,
            Some(crate::effect::FrenziedGoblinOutcomeV1::Paid)
        );
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            observe_v1(&restored, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
    }
}

#[test]
fn actual_registered_generic_restriction_is_explicitly_unrepresentable_without_its_owner_receipt() {
    use crate::effect::{self, EffectOp, ExecCtx};
    use crate::state::{AbilitySourceContractV4, StackTargetContractV4, Target};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mut context = ExecCtx::no_targets(source, actor);
        context.targets = vec![Target::Object(recipient)];
        context.target_contracts = vec![StackTargetContractV4::capture(
            &state,
            Target::Object(recipient),
        )];
        context.ability_source_contract = Some(AbilitySourceContractV4::capture(&state, source));
        // Registered operands exercise the existing typed leaf, not a fake printed Elf root.
        effect::execute(
            &EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 },
            &context,
            &mut state,
        );
        assert!(crate::sower_of_chaos_v1::cant_block(&state, recipient));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(observe_v1(&state, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn genuine_published_owner_cannot_be_replaced_by_an_unrelated_registered_activation() {
    use crate::event::CommittedEvent;
    use crate::state::{AbilitySourceContractV4, StackItemKind, StackStateV4};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut corrupt, _, _) = actual_payment(actor);
        let dragon = put(&mut corrupt, actor, "Shivan Dragon", Zone::Battlefield);
        let card = corrupt.objects.get(dragon).card_def;
        let root = (crate::card_def::CARD_DEFS[usize::from(card)].activated_abilities[0].effect)();
        corrupt.engine.pending_effect = None;
        corrupt.engine.event_history.retain(|event| {
            !matches!(
                event,
                CommittedEvent::FrenziedGoblinResolutionBoundaryV1 { .. }
            )
        });
        let contract = AbilitySourceContractV4::capture(&corrupt, dragon);
        let live = corrupt.stack.last_mut().unwrap();
        let stack_id = live.v4.stack_item_id;
        live.kind = StackItemKind::ActivatedAbility;
        live.source = dragon;
        live.controller = actor;
        live.targets.clear();
        live.inline_effect = Some(root);
        live.v4 = StackStateV4 {
            stack_item_id: stack_id,
            ability_source_contract: Some(contract),
            activated_ability_index: Some(0),
            target_spec: Some(crate::card_def::TargetSpec::None),
            ..Default::default()
        };
        // The unrelated printed registered activation is structurally authentic
        // under the ordinary production source validator. The surviving Frenzied
        // publication still requires the original complete row, not merely its ID.
        crate::engine::validated_stack_item_target_spec(corrupt.stack.last().unwrap(), &corrupt)
            .unwrap();
        let before = serde_json::to_vec(&corrupt).unwrap();
        assert!(crate::engine::step(&mut corrupt, crate::engine::Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&corrupt).unwrap(), before);
        assert!(observe_v1(&corrupt, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1).is_err());
        assert_eq!(serde_json::to_vec(&corrupt).unwrap(), before);
    }
}
