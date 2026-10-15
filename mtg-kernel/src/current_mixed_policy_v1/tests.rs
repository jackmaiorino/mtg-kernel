use super::*;
use serde_json::json;

#[test]
fn numeric_tree_has_no_scalar_width_loss_and_keeps_every_ordered_member() {
    for value in [0u64, 1, 16777216, 16777217, u32::MAX as u64, u64::MAX] {
        let tree = NumericTree::encode(&json!(value)).unwrap();
        let limbs = &tree.rows()[0][1..5];
        let recovered = limbs
            .iter()
            .enumerate()
            .fold(0u64, |v, (i, p)| v | (u64::from(*p) << (16 * i)));
        assert_eq!(recovered, value);
        assert!(tree.tensors().iter().flatten().all(|v| v.is_finite()));
    }
    let actions = (0..260)
        .map(|i| {
            NumericTree::encode(&json!({"kind":"ChooseDivineTargetV44","selected_count":i}))
                .unwrap()
        })
        .collect();
    let packet = Packet {
        source: Source::RavenousV51,
        state: NumericTree::encode(
            &json!({"roster":(0..12).collect::<Vec<_>>(),"soul":i32::MAX,"damage":u32::MAX}),
        )
        .unwrap(),
        actions,
    };
    let model = NativeModel::deterministic_fixture().unwrap();
    let logits = model.forward(&packet).unwrap();
    assert_eq!(logits.len(), 260);
    assert!(logits.iter().all(|v| v.is_finite()));
    assert_ne!(packet.actions[255], packet.actions[256]);
    assert_ne!(packet.actions[0], packet.actions[259]);
    let mut wrong = ModelConfig::current();
    wrong.contract = "frozen-predecessor".into();
    assert!(NativeModel::from_parameters(wrong, vec![0.; NativeModel::parameter_count()]).is_err());
}

#[test]
fn unknown_printed_program_refuses_instead_of_using_name_or_digest() {
    let id = crate::card_def::card_id_by_name("Counterspell").unwrap();
    assert!(matches!(roots::baseline(id),Err(Error::UnsupportedPrintedRoot(actual))if actual==id));
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod paid {
    use super::*;
    use crate::ids::{ObjectId, PlayerId};
    use crate::policy_observation_v6::tests::put;
    use crate::rl::ActionSemanticV1;
    use crate::state::{GameState, Zone};
    fn state(actor: PlayerId) -> GameState {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Forest", Zone::Hand);
            for _ in 0..12 {
                put(&mut state, player, "Forest", Zone::Library);
            }
        }
        state
    }
    fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        match session.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("expected real current choice:{other:?}"),
        }
    }
    macro_rules! act {
        ($session:expr,$input:ident,$choose:expr) => {{
            let e = expected(&$session.inner);
            let (_, actions, _) = $session.inner.$input(e, e.acting_player).unwrap();
            let index = actions
                .iter()
                .position($choose)
                .unwrap_or_else(|| panic!("actual action absent:{actions:?}"));
            let decision = $session.encode(e).unwrap();
            assert_eq!(decision.packet().actions().len(), actions.len());
            let scores = NativeModel::deterministic_fixture()
                .unwrap()
                .forward(decision.packet())
                .unwrap();
            assert_eq!(scores.len(), actions.len());
            assert!(scores.iter().all(|v| v.is_finite()));
            $session.apply(&decision, index as u32).unwrap();
        }};
    }
    fn mana(action: &ActionSemanticV1, id: ObjectId) -> bool {
        matches!(action,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==id.0)
    }
    #[test]
    fn think_twice_roots_keep_exact_ordered_cost_route_draw_and_departure() {
        let id = crate::card_def::card_id_by_name("Think Twice").unwrap();
        let descriptor = roots::baseline(id).unwrap();
        let roots = &descriptor["primitive_printing"]["ordered_cast_roots"];
        assert_eq!(roots.as_array().unwrap().len(), 2);
        assert_eq!(roots[0]["origin"], json!(Zone::Hand));
        assert_eq!(roots[1]["origin"], json!(Zone::Graveyard));
        assert_eq!(roots[0]["method"], "normal");
        assert_eq!(roots[1]["method"], "flashback");
        assert_eq!(roots[0]["route"], "Hand");
        assert_eq!(roots[1]["route"], "GraveyardFlashback");
        assert_eq!(roots[0]["ordered_costs"][0]["PrintedMana"]["generic"], 1);
        assert_eq!(
            roots[1]["ordered_costs"][0]["FlashbackAlternativeMana"]["generic"],
            2
        );
        for (root, kind) in [(0, "PrintedMana"), (1, "FlashbackAlternativeMana")] {
            assert_eq!(
                roots[root]["ordered_costs"][0][kind]["pips"],
                json!([{"Colored":"U"}])
            );
            assert_eq!(roots[root]["ordered_costs"][0][kind]["x_count"], 0);
            assert_eq!(
                roots[root]["ordered_resolution"],
                json!([{"DrawController":1}])
            );
        }
        assert_eq!(
            roots[0]["physical_stack_departure"],
            "GraveyardAfterResolutionOrCounter"
        );
        assert_eq!(
            roots[1]["physical_stack_departure"],
            "ExileInsteadOfAnyOtherDeparture"
        );
        assert_ne!(
            NumericTree::encode(&roots[0]).unwrap(),
            NumericTree::encode(&roots[1]).unwrap()
        );
    }

    #[test]
    fn genuine_paid_think_twice_then_actual_flashback_keep_both_entry_bindings_and_restore_both_seats(
    ) {
        macro_rules! run {
            ($session:ident,$from:ident,$input:ident,$world:ident) => {
                for actor in [PlayerId::P0, PlayerId::P1] {
                    let mut raw = state(actor);
                    let spell = put(&mut raw, actor, "Think Twice", Zone::Hand);
                    let islands: [ObjectId; 5] = std::array::from_fn(|_| put(&mut raw, actor, "Island", Zone::Battlefield));
                    let mut session = $session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                    let mut cast_packets = Vec::new();
                    let mut cast_actions = Vec::new();
                    for (flashback, lands) in [(false, &islands[..2]), (true, &islands[2..])] {
                        for id in lands {
                            act!(session, $input, |a| mana(a, *id));
                        }
                        let e = expected(&session.inner);
                        assert_eq!(e.acting_player, actor.into());
                        let (_, actions, _) = session.inner.$input(e, e.acting_player).unwrap();
                        let cast = actions.iter().position(|a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==spell.0)).unwrap();
                        let pass = actions.iter().position(|a| matches!(a,ActionSemanticV1::Pass{..})).unwrap();
                        let decision = session.encode(e).unwrap();
                        assert_eq!(decision.packet.actions.len(), actions.len());
                        assert_ne!(decision.packet.actions[cast], decision.packet.actions[pass]);
                        cast_packets.push(decision.packet.clone());
                        cast_actions.push(decision.packet.actions[cast].clone());
                        let origin = if flashback { Zone::Graveyard } else { Zone::Hand };
                        let generation = session.inner.$world().objects.get(spell).zone_change_count;
                        assert_eq!(session.inner.$world().objects.get(spell).zone, origin);
                        session.apply(&decision, cast as u32).unwrap();
                        let raw = session.inner.$world();
                        assert!(raw.engine.pending_cast.is_none());
                        assert_eq!(raw.players[actor.index()].mana_pool, [0;6]);
                        assert!(lands.iter().all(|id| raw.objects.get(*id).tapped));
                        let row = raw.stack.last().unwrap();
                        assert_eq!(raw.stack.len(), 1);
                        assert_eq!(row.source, spell);
                        assert_eq!(row.controller, actor);
                        assert_eq!(row.is_flashback, flashback);
                        assert_eq!(row.v4.target_spec, Some(crate::card_def::TargetSpec::None));
                        assert!(row.targets.is_empty());
                        let contract = row.v4.source_contract.unwrap();
                        assert_eq!(contract.zone_change_count, generation + 1);
                        let provenance = contract.spell_cast_origin.unwrap();
                        assert_eq!(provenance.origin_zone, origin);
                        assert_eq!(provenance.origin_zone_change_count, generation);
                        assert_eq!(provenance.route, if flashback { crate::state::SpellCastRouteV4::GraveyardFlashback } else { crate::state::SpellCastRouteV4::Hand });
                        assert_eq!(row.v4.cast_method, Some(if flashback {crate::state::CastMethodV4::Flashback} else {crate::state::CastMethodV4::Normal}));
                        let before_hand = raw.players[actor.index()].hand.len();
                        let before_library = raw.players[actor.index()].library.len();
                        let before_draws = raw.engine.event_history.iter().filter(|event| matches!(event,crate::event::CommittedEvent::Draw{player,object:Some(_)}if *player==actor)).count();
                        let e = expected(&session.inner);
                        let paid = session.encode(e).unwrap();
                        let saved = session.snapshot();
                        let mut complete = None;
                        for replay in 0..2 {
                            if replay != 0 {
                                session.restore(&saved).unwrap();
                                assert!(expected(&session.inner).environment_revision > e.environment_revision);
                                assert!(matches!(session.apply(&paid, 0), Err(Error::StaleBinding)));
                                assert_eq!(session.encode(expected(&session.inner)).unwrap().packet, paid.packet);
                            }
                            for _ in 0..12 {
                                if session.inner.$world().stack.is_empty() { break; }
                                act!(session, $input, |a| matches!(a,ActionSemanticV1::Pass{..}));
                            }
                            let raw = session.inner.$world();
                            assert!(raw.stack.is_empty());
                            assert_eq!(raw.objects.get(spell).zone, if flashback {Zone::Exile} else {Zone::Graveyard});
                            assert_eq!(raw.objects.get(spell).zone_change_count, generation + 2);
                            assert_eq!(raw.players[actor.index()].hand.len(), before_hand + 1);
                            assert_eq!(raw.players[actor.index()].library.len(), before_library - 1);
                            assert_eq!(raw.engine.event_history.iter().filter(|event| matches!(event,crate::event::CommittedEvent::Draw{player,object:Some(_)}if *player==actor)).count(), before_draws + 1);
                            let outcome = (serde_json::to_vec(raw).unwrap(),session.inner.policy_step_count(),session.inner.physical_decision_count());
                            if let Some(prior) = &complete { assert_eq!(&outcome, prior); } else { complete = Some(outcome); }
                        }
                    }
                    // The same physical card's original ordered cast candidate is
                    // encoded separately for Hand normal and Graveyard flashback.
                    assert_ne!(cast_packets[0], cast_packets[1]);
                    assert_ne!(cast_actions[0], cast_actions[1]);
                    let mut initial = state(actor);
                    put(&mut initial, actor, "Think Twice", Zone::Hand);
                    let hidden = put(&mut initial, actor.opponent(), "Think Twice", Zone::Hand);
                    let mut alternative = initial.clone();
                    alternative.objects.get_mut(hidden).card_def = crate::card_def::card_id_by_name("Counterspell").unwrap();
                    alternative.objects.get_mut(hidden).name = "Counterspell".into();
                    // Independent initial information sets, never edited retained history.
                    let a = $session::from_session(FastActorSessionV1::$from(initial)).unwrap();
                    let b = $session::from_session(FastActorSessionV1::$from(alternative)).unwrap();
                    assert_eq!(a.encode(expected(&a.inner)).unwrap().packet,b.encode(expected(&b.inner)).unwrap().packet);
                }
            };
        }
        run!(
            FakeSession,
            from_fake_v50_fixture_state,
            fake_current_decision_input_v50,
            fake_fixture_state
        );
        run!(
            RavenousSession,
            from_ravenous_v51_fixture_state,
            ravenous_current_decision_input_v51,
            ravenous_fixture_state
        );
    }
    #[test]
    fn real_paid_fake_and_bolt_keep_custody_order_binding_restore_and_private_hand_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut raw = state(actor);
            let spell = put(&mut raw, actor, "Fake Your Own Death", Zone::Hand);
            let creature = put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield);
            let bolt = put(&mut raw, actor.opponent(), "Lightning Bolt", Zone::Hand);
            let mountain = put(&mut raw, actor.opponent(), "Mountain", Zone::Battlefield);

            let swamps: [ObjectId; 2] =
                std::array::from_fn(|_| put(&mut raw, actor, "Swamp", Zone::Battlefield));
            let raw = FastActorSessionV1::from_fake_v50_fixture_state(raw);
            let mut session = FakeSession::from_session(raw).unwrap();
            let e = expected(&session.inner);
            let before = session.encode(e).unwrap();
            let snapshot = session.snapshot();
            let forged_owner = FakeSession::from_session(session.inner.clone()).unwrap();
            assert!(forged_owner.encode(e).is_ok());
            let mut forged = before.clone();
            forged.owner = Arc::new(());
            assert!(matches!(
                session.apply(&forged, 0),
                Err(Error::StaleBinding)
            ));
            let mut forged = before.clone();
            forged.packet.actions.reverse();
            assert!(matches!(
                session.apply(&forged, 0),
                Err(Error::StaleBinding)
            ));
            let old_revision = expected(&session.inner).environment_revision;
            session.restore(&snapshot).unwrap();
            assert!(expected(&session.inner).environment_revision > old_revision);
            assert!(matches!(
                session.apply(&before, 0),
                Err(Error::StaleBinding)
            ));
            for id in swamps {
                act!(session, fake_current_decision_input_v50, |a| mana(a, id));
            }
            act!(
                session,
                fake_current_decision_input_v50,
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==spell.0)
            );
            act!(
                session,
                fake_current_decision_input_v50,
                |a| matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==creature.0)
            );
            for _ in 0..16 {
                if session.inner.fake_fixture_state().stack.is_empty() {
                    break;
                }
                act!(session, fake_current_decision_input_v50, |a| matches!(
                    a,
                    ActionSemanticV1::Pass { .. }
                ));
            }
            assert_eq!(
                session.inner.fake_fixture_state().objects.get(spell).zone,
                Zone::Graveyard
            );
            assert_eq!(
                crate::fake_your_own_death_v50::public_v50::capture(
                    session.inner.fake_fixture_state()
                )
                .unwrap()
                .grants
                .len(),
                1
            );

            if expected(&session.inner).acting_player != actor.opponent().into() {
                act!(session, fake_current_decision_input_v50, |a| matches!(
                    a,
                    ActionSemanticV1::Pass { .. }
                ));
            }
            assert_eq!(
                expected(&session.inner).acting_player,
                actor.opponent().into()
            );
            act!(session, fake_current_decision_input_v50, |a| mana(
                a, mountain
            ));
            act!(
                session,
                fake_current_decision_input_v50,
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==bolt.0)
            );
            act!(
                session,
                fake_current_decision_input_v50,
                |a| matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==creature.0)
            );
            let mut restored_death = false;
            for _ in 0..32 {
                let facts = crate::fake_your_own_death_v50::public_v50::capture(
                    session.inner.fake_fixture_state(),
                )
                .unwrap();
                if !restored_death && facts.deaths.iter().any(|d| {
                    d.phase == crate::fake_your_own_death_v50::public_v50::DeathPhaseV50::Published
                }) {
                    let e = expected(&session.inner);
                    let old = session.encode(e).unwrap();
                    let saved = session.snapshot();
                    session.restore(&saved).unwrap();
                    assert!(matches!(session.apply(&old, 0), Err(Error::StaleBinding)));
                    assert_eq!(
                        session.encode(expected(&session.inner)).unwrap().packet,
                        old.packet
                    );
                    restored_death = true;
                }
                if session.inner.fake_fixture_state().stack.is_empty() {
                    break;
                }
                act!(session, fake_current_decision_input_v50, |a| matches!(
                    a,
                    ActionSemanticV1::Pass { .. }
                ));
            }
            assert!(restored_death);
            let facts = crate::fake_your_own_death_v50::public_v50::capture(
                session.inner.fake_fixture_state(),
            )
            .unwrap();
            assert_eq!(facts.deaths.len(), 1);
            assert_eq!(
                facts.deaths[0].phase,
                crate::fake_your_own_death_v50::public_v50::DeathPhaseV50::Resolved
            );
            assert_eq!(facts.deaths[0].treasures.len(), 1);
            assert_eq!(
                session
                    .inner
                    .fake_fixture_state()
                    .objects
                    .get(creature)
                    .zone,
                Zone::Battlefield
            );
            assert!(
                session
                    .inner
                    .fake_fixture_state()
                    .objects
                    .get(creature)
                    .tapped
            );
            assert_eq!(
                session.inner.fake_fixture_state().objects.get(bolt).zone,
                Zone::Graveyard
            );
            let mut alternative = state(actor);
            let hidden = put(&mut alternative, actor.opponent(), "Island", Zone::Hand);
            let mut variant = alternative.clone();
            variant.objects.get_mut(hidden).card_def =
                crate::card_def::card_id_by_name("Mountain").unwrap();
            variant.objects.get_mut(hidden).name = "Mountain".into();
            // Initial information-set operands have no retained private-world journals to alter.
            // Each owning session rederives its own genuine current observation.
            let a = FakeSession::from_session(FastActorSessionV1::from_fake_v50_fixture_state(
                alternative,
            ))
            .unwrap();
            let b =
                FakeSession::from_session(FastActorSessionV1::from_fake_v50_fixture_state(variant))
                    .unwrap();
            assert_eq!(
                a.encode(expected(&a.inner)).unwrap().packet,
                b.encode(expected(&b.inner)).unwrap().packet
            );
        }
    }
    #[test]
    fn real_paid_amulet_complete_wide_packet_and_original_cost_choice_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut raw = state(actor);
            let amulet = put(&mut raw, actor, "Ravenous Amulet", Zone::Hand);
            put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield);
            put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield);
            let lands: [ObjectId; 3] =
                std::array::from_fn(|_| put(&mut raw, actor, "Forest", Zone::Battlefield));
            let mut session = RavenousSession::from_session(
                FastActorSessionV1::from_ravenous_v51_fixture_state(raw),
            )
            .unwrap();
            for id in &lands[..2] {
                act!(session, ravenous_current_decision_input_v51, |a| mana(
                    a, *id
                ));
            }
            act!(
                session,
                ravenous_current_decision_input_v51,
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==amulet.0)
            );
            for _ in 0..16 {
                if session.inner.ravenous_fixture_state().stack.is_empty() {
                    break;
                }
                act!(session, ravenous_current_decision_input_v51, |a| matches!(
                    a,
                    ActionSemanticV1::Pass { .. }
                ));
            }
            assert_eq!(
                session
                    .inner
                    .ravenous_fixture_state()
                    .objects
                    .get(amulet)
                    .zone,
                Zone::Battlefield
            );
            act!(session, ravenous_current_decision_input_v51, |a| mana(
                a, lands[2]
            ));
            act!(
                session,
                ravenous_current_decision_input_v51,
                |a| matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==amulet.0)
            );
            let e = expected(&session.inner);
            let (_, actions, _) = session
                .inner
                .ravenous_current_decision_input_v51(e, e.acting_player)
                .unwrap();
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, ActionSemanticV1::ChooseCostTarget { .. }))
                    .count(),
                2
            );
            let decision = session.encode(e).unwrap();
            let snapshot = session.snapshot();
            session.restore(&snapshot).unwrap();
            assert!(matches!(
                session.apply(&decision, 0),
                Err(Error::StaleBinding)
            ));
            act!(session, ravenous_current_decision_input_v51, |a| matches!(
                a,
                ActionSemanticV1::ChooseCostTarget { .. }
            ));
            for _ in 0..16 {
                if session.inner.ravenous_fixture_state().stack.is_empty() {
                    break;
                }
                act!(session, ravenous_current_decision_input_v51, |a| matches!(
                    a,
                    ActionSemanticV1::Pass { .. }
                ));
            }
            assert_eq!(
                session
                    .inner
                    .ravenous_fixture_state()
                    .objects
                    .get(amulet)
                    .counters
                    .soul,
                1
            );
        }
    }
    #[test]
    fn actual_v51_public_artifact_operand_preserves_wide_zero_and_adjacent_counts_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut raw = state(actor);
            let amulet = put(&mut raw, actor, "Ravenous Amulet", Zone::Battlefield);
            raw.objects.get_mut(amulet).counters.soul = 16777216;
            raw.objects.get_mut(amulet).counters.plus1_plus1 = i32::MAX;
            raw.objects.get_mut(amulet).damage = u32::MAX;
            let mut changed = raw.clone();
            changed.objects.get_mut(amulet).counters.soul = 16777217;
            let a = RavenousSession::from_session(
                FastActorSessionV1::from_ravenous_v51_fixture_state(raw),
            )
            .unwrap();
            let b = RavenousSession::from_session(
                FastActorSessionV1::from_ravenous_v51_fixture_state(changed),
            )
            .unwrap();
            let a = a.encode(expected(&a.inner)).unwrap();
            let b = b.encode(expected(&b.inner)).unwrap();
            assert_ne!(a.packet, b.packet);
            assert_eq!(a.packet.actions.len(), b.packet.actions.len());
            assert!(NativeModel::deterministic_fixture()
                .unwrap()
                .forward(&a.packet)
                .unwrap()
                .iter()
                .all(|v| v.is_finite()));
        }
    }

    #[test]
    fn genuine_divine_260_targets_restores_after_255_without_numeric_menu_truncation_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut raw = state(actor);
            let spell = put(&mut raw, actor, "Divine Resilience", Zone::Hand);
            let targets = (0..260)
                .map(|_| put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield))
                .collect::<Vec<_>>();
            // Declared floating-mana fixture operand. The real shared cast consumes the exact printed+kicker payment.
            raw.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()] = 2;
            raw.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
            let mut session = RavenousSession::from_session(
                FastActorSessionV1::from_ravenous_v51_fixture_state(raw),
            )
            .unwrap();
            act!(
                session,
                ravenous_current_decision_input_v51,
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==spell.0)
            );
            act!(session, ravenous_current_decision_input_v51, |a| matches!(
                a,
                ActionSemanticV1::ChooseDivineKickerV44 { pay: true, .. }
            ));
            for (index, target) in targets.iter().enumerate() {
                let e = expected(&session.inner);
                let (_, actions, _) = session
                    .inner
                    .ravenous_current_decision_input_v51(e, e.acting_player)
                    .unwrap();
                assert_eq!(
                    actions
                        .iter()
                        .filter(|a| matches!(a, ActionSemanticV1::ChooseDivineTargetV44 { .. }))
                        .count(),
                    260 - index
                );
                if index == 0 || index == 255 {
                    let old = session.encode(e).unwrap();
                    let snapshot = session.snapshot();
                    session.restore(&snapshot).unwrap();
                    assert!(matches!(session.apply(&old, 0), Err(Error::StaleBinding)));
                    assert_eq!(
                        session.encode(expected(&session.inner)).unwrap().packet,
                        old.packet
                    );
                }
                act!(
                    session,
                    ravenous_current_decision_input_v51,
                    |a| matches!(a,ActionSemanticV1::ChooseDivineTargetV44{target:t,..}if t.arena_id==target.0)
                );
            }
            act!(session, ravenous_current_decision_input_v51, |a| matches!(
                a,
                ActionSemanticV1::FinishDivineTargetsV44 { .. }
            ));
            for _ in 0..16 {
                if session.inner.ravenous_fixture_state().stack.is_empty() {
                    break;
                }
                act!(session, ravenous_current_decision_input_v51, |a| matches!(
                    a,
                    ActionSemanticV1::Pass { .. }
                ));
            }
            let raw = session.inner.ravenous_fixture_state();
            assert_eq!(raw.objects.get(spell).zone, Zone::Graveyard);
            for target in targets {
                assert!(crate::engine::has_effective_keyword(
                    raw,
                    target,
                    crate::card_def::Keywords::INDESTRUCTIBLE
                ));
            }
        }
    }
    #[test]
    fn genuine_large_trigger_menu_missing_printed_root_refuses_readonly_instead_of_flattening() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut raw = state(actor);
            for _ in 0..9 {
                put(&mut raw, actor, "Homunculus Horde", Zone::Battlefield);
            }
            raw.players[actor.index()].draws_this_turn = 0;
            for _ in 0..2 {
                crate::event::propose_and_commit(
                    &mut raw,
                    crate::event::ProposedEvent::draw(actor),
                );
            }
            let waiting = crate::trigger::collect_and_process(&mut raw);
            assert_eq!(waiting.len(), 9);
            raw.engine.pending_triggers.extend(waiting);
            let session = RavenousSession::from_session(
                FastActorSessionV1::from_ravenous_v51_fixture_state(raw),
            )
            .unwrap();
            let before = session.inner.ravenous_fixture_state().clone();
            let e = expected(&session.inner);
            let (_, actions, _) = session
                .inner
                .ravenous_current_decision_input_v51(e, e.acting_player)
                .unwrap();
            assert!(actions
                .iter()
                .any(|a| matches!(a, ActionSemanticV1::ChooseTriggerOrderNext { .. })));
            let id = crate::card_def::card_id_by_name("Homunculus Horde").unwrap();
            assert!(
                matches!(session.encode(e),Err(Error::UnsupportedPrintedRoot(actual))if actual==id)
            );
            assert_eq!(session.inner.ravenous_fixture_state(), &before);
        }
    }
    #[test]
    fn native_selection_executes_exact_original_candidate_and_foreign_restore_is_readonly_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut raw = state(actor);
            put(&mut raw, actor, "Forest", Zone::Battlefield);
            put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield);
            let raw = FastActorSessionV1::from_ravenous_v51_fixture_state(raw);
            let mut session = RavenousSession::from_session(raw).unwrap();
            let mut reference = session.inner.clone();
            let e = expected(&session.inner);
            let decision = session.encode(e).unwrap();
            let other = RavenousSession::from_session(reference.clone()).unwrap();
            let foreign = other.snapshot();
            let before = session.inner.privileged_core_environment_hash();
            assert!(matches!(
                session.restore(&foreign),
                Err(Error::ForeignSnapshot)
            ));
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let (logits, selected, actual) = session
                .score_and_apply(&decision, &NativeModel::deterministic_fixture().unwrap())
                .unwrap();
            assert_eq!(logits.len(), decision.packet.actions.len());
            let expected = reference.step(e.episode_id, e.step, selected).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                session.inner.ravenous_fixture_state(),
                reference.ravenous_fixture_state()
            );
            assert_eq!(
                session.inner.policy_step_count(),
                reference.policy_step_count()
            );
            assert_eq!(
                session.inner.physical_decision_count(),
                reference.physical_decision_count()
            );
            assert!(matches!(
                session.apply(&decision, selected),
                Err(Error::StaleBinding)
            ));
        }
    }
}
