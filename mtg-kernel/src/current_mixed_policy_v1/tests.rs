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
    let id = crate::card_def::card_id_by_name("Brainstorm").unwrap();
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
    fn closed_counter_roots_preserve_target_filter_guard_loot_and_controller_reward_order() {
        let root = |name| {
            let id = crate::card_def::card_id_by_name(name).unwrap();
            roots::baseline(id).unwrap()["primitive_printing"]["ordered_cast_roots"][0].clone()
        };
        let ordinary = root("Counterspell");
        assert_eq!(ordinary["target_filter"], "AnySpell");
        assert_eq!(
            ordinary["ordered_costs"][0]["PrintedMana"],
            json!({"pips":[{"Colored":"U"},{"Colored":"U"}],"generic":0,"x_count":0})
        );
        let guard = &ordinary["ordered_resolution"][0]["CounterBoundSpell"];
        assert_eq!(guard["guard"], "BoundSpellCanBeCountered");
        assert_eq!(
            guard["normal_physical_departure_proposal"],
            json!(Zone::Graveyard)
        );
        assert_eq!(
            guard["flashback_replaces_departure_with"],
            json!(Zone::Exile)
        );
        assert_eq!(guard["other_zone_replacements_still_apply"], true);
        assert_eq!(guard["virtual_copy_ceases"], true);
        assert_eq!(root("Essence Scatter")["target_filter"], "CreatureSpell");
        let refute = root("Refute");
        let instructions = refute["ordered_resolution"].as_array().unwrap();
        assert_eq!(instructions.len(), 3);
        assert_eq!(instructions[0], ordinary["ordered_resolution"][0]);
        assert_eq!(instructions[1], json!({"DrawController":1}));
        assert_eq!(instructions[2]["DiscardController"]["requested_count"], 1);
        assert_eq!(instructions[2]["DiscardController"]["is_payment"], false);
        let offer = root("An Offer You Can't Refuse");
        assert_eq!(offer["target_filter"], "NoncreatureSpell");
        let reward = &offer["ordered_resolution"][0]["CounterThenReward"];
        assert_eq!(offer["ordered_resolution"].as_array().unwrap().len(), 1);
        assert_eq!(reward["token_count"], 2);
        assert_eq!(
            reward["sample_target_spell_controller_before_counter"],
            true
        );
        assert_eq!(reward["reward_independent_of_counter_success"], true);
        assert_eq!(
            reward["token_printing"]["primitive_printing"]["ordered_mana_costs"],
            json!(["TapSelf", "SacrificeSelf"])
        );
        assert_ne!(
            NumericTree::encode(&refute).unwrap(),
            NumericTree::encode(&ordinary).unwrap()
        );
        assert_ne!(
            NumericTree::encode(&offer).unwrap(),
            NumericTree::encode(&ordinary).unwrap()
        );
        // Other filters and payment/mode recipes never enter this closed
        // subset by sharing a printed keyword or a familiar name.
        for name in ["Force Spike", "Dispel", "Annul"] {
            let id = crate::card_def::card_id_by_name(name).unwrap();
            assert!(
                matches!(roots::baseline(id),Err(Error::UnsupportedPrintedRoot(actual))if actual==id)
            );
        }
    }

    #[test]
    fn genuine_paid_counter_recipes_keep_target_binding_controller_order_and_restore_in_both_owners_both_seats(
    ) {
        macro_rules! run {
            ($session:ident,$from:ident,$input:ident,$world:ident) => {
                for actor in [PlayerId::P0,PlayerId::P1] {
                    for own_target in [false,true] {
                        let target_controller=if own_target {actor} else {actor.opponent()};
                        for (name,flashback) in [("Counterspell",false),("Counterspell",true),("Essence Scatter",false),("Refute",false),("An Offer You Can't Refuse",false),("An Offer You Can't Refuse",true)] {
                            let mut raw=state(target_controller);
                            let discard=raw.players[actor.index()].hand[0];
                            let counter=put(&mut raw,actor,name,Zone::Hand);
                            let target=put(&mut raw,target_controller,if name=="Essence Scatter" {"Llanowar Elves"} else {"Think Twice"},if flashback {Zone::Graveyard} else {Zone::Hand});
                            let destination=if flashback {Zone::Exile} else {Zone::Graveyard};
                            let target_lands=if name=="Essence Scatter" {vec![put(&mut raw,target_controller,"Forest",Zone::Battlefield)]} else {(0..if flashback {3} else {2}).map(|_|put(&mut raw,target_controller,"Island",Zone::Battlefield)).collect::<Vec<_>>()};
                            let cost=match name {"Refute"=>3,"An Offer You Can't Refuse"=>1,_=>2};
                            let counter_lands=(0..cost).map(|_|put(&mut raw,actor,"Island",Zone::Battlefield)).collect::<Vec<_>>();
                            let mut session=$session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                            for id in &target_lands {act!(session,$input,|a|mana(a,*id));}
                            act!(session,$input,|a|matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==target.0));
                            assert_eq!(session.inner.$world().stack.len(),1);
                            assert_eq!(session.inner.$world().stack[0].is_flashback,flashback);
                            if !own_target {act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                            assert_eq!(expected(&session.inner).acting_player,actor.into());
                            for id in &counter_lands {act!(session,$input,|a|mana(a,*id));}
                            act!(session,$input,|a|matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==counter.0));
                            let e=expected(&session.inner);
                            let (_,actions,_)=session.inner.$input(e,e.acting_player).unwrap();
                            // Existing spell target semantics name the genuine
                            // physical Stack incarnation, not an invented ID.
                            let target_index=actions.iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{source,target:crate::rl::TargetRefV1::Object{object},remaining:1,..}if source.arena_id==counter.0 && object.arena_id==target.0 && object.zone==Zone::Stack)).unwrap();
                            let decision=session.encode(e).unwrap();
                            assert_eq!(decision.packet.actions.len(),actions.len());
                            let mut forged=decision.clone();
                            forged.packet.actions.push(forged.packet.actions[0].clone());
                            let unchanged=session.inner.privileged_core_environment_hash();
                            assert!(matches!(session.apply(&forged,target_index as u32),Err(Error::StaleBinding)));
                            assert_eq!(session.inner.privileged_core_environment_hash(),unchanged);
                            let saved=session.snapshot();
                            let mut complete=None;
                            for replay in 0..2 {
                                if replay!=0 {
                                    session.restore(&saved).unwrap();
                                    assert!(expected(&session.inner).environment_revision>e.environment_revision);
                                    assert!(matches!(session.apply(&decision,target_index as u32),Err(Error::StaleBinding)));
                                    assert_eq!(session.encode(expected(&session.inner)).unwrap().packet,decision.packet);
                                }
                                let current=session.encode(expected(&session.inner)).unwrap();
                                // Execute the selected original index through
                                // both the bound owner and an actual backend clone.
                                let mut reference=session.inner.clone();
                                let now=expected(&reference);
                                let expected_reply=reference.step(now.episode_id,now.step,target_index as u32).unwrap();
                                assert_eq!(session.apply(&current,target_index as u32).unwrap(),expected_reply);
                                assert_eq!(session.inner.$world(),reference.$world());
                                let raw=session.inner.$world();
                                assert!(raw.engine.pending_cast.is_none());
                                assert_eq!(raw.stack.len(),2);
                                assert_eq!(raw.players[actor.index()].mana_pool,[0;6]);
                                assert!(counter_lands.iter().chain(&target_lands).all(|id|raw.objects.get(*id).tapped));
                                let row=raw.stack.last().unwrap();
                                assert_eq!(row.source,counter);
                                assert_eq!(row.controller,actor);
                                assert_eq!(row.targets,vec![crate::state::Target::Object(target)]);
                                assert_eq!(row.v4.target_contracts.len(),1);
                                let start=raw.engine.event_history.len();
                                let library_before=raw.players[actor.index()].library.len();
                                for _ in 0..24 {
                                    let raw=session.inner.$world();
                                    if raw.stack.is_empty() && raw.engine.pending_discard.is_none() && raw.engine.pending_effect.is_none() {break;}
                                    let now=expected(&session.inner);
                                    let (_,actions,_)=session.inner.$input(now,now.acting_player).unwrap();
                                    if actions.iter().any(|a|matches!(a,ActionSemanticV1::Discard{..})) {
                                        assert_eq!(name,"Refute");
                                        // Draw happened and countered spell left
                                        // before the mandatory private discard.
                                        assert_eq!(raw.objects.get(target).zone,destination);
                                        assert_eq!(raw.players[actor.index()].library.len(),library_before-1);
                                        act!(session,$input,|a|matches!(a,ActionSemanticV1::Discard{cards,..}if cards.len()==1 && cards[0].arena_id==discard.0));
                                    } else {act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                                }
                                let raw=session.inner.$world();
                                assert!(raw.stack.is_empty());
                                assert!(raw.engine.pending_discard.is_none());
                                assert_eq!(raw.objects.get(counter).zone,Zone::Graveyard);
                                assert_eq!(raw.objects.get(target).zone,destination);
                                assert_eq!(raw.players[actor.index()].mana_pool,[0;6]);
                                let events=&raw.engine.event_history[start..];
                                let target_left=events.iter().position(|event|matches!(event,crate::event::CommittedEvent::ZoneChange{object,to,..}if *object==target && *to==destination)).unwrap();
                                let draws=events.iter().filter(|event|matches!(event,crate::event::CommittedEvent::Draw{player,object:Some(_)}if *player==actor)).count();
                                assert_eq!(draws,usize::from(name=="Refute"));
                                if name=="Refute" {
                                    let draw=events.iter().position(|event|matches!(event,crate::event::CommittedEvent::Draw{player,..}if *player==actor)).unwrap();
                                    let discarded=events.iter().position(|event|matches!(event,crate::event::CommittedEvent::ZoneChange{object,to:Zone::Graveyard,..}if *object==discard)).unwrap();
                                    assert!(target_left<draw && draw<discarded);
                                }
                                let tokens=raw.objects.iter().filter(|(_,object)|object.zone==Zone::Battlefield && object.card_def==crate::card_def::card_id_by_name("Treasure Token").unwrap()).map(|(_,object)|object).collect::<Vec<_>>();
                                assert_eq!(tokens.len(),if name=="An Offer You Can't Refuse" {2} else {0});
                                assert!(tokens.iter().all(|token|token.controller==target_controller && token.owner==target_controller));
                                if name=="An Offer You Can't Refuse" {
                                    let first_token=events.iter().position(|event|matches!(event,crate::event::CommittedEvent::CreateToken{..})).unwrap();
                                    assert!(target_left<first_token);
                                }
                                let outcome=(serde_json::to_vec(raw).unwrap(),session.inner.policy_step_count(),session.inner.physical_decision_count());
                                if let Some(prior)=&complete {assert_eq!(&outcome,prior);} else {complete=Some(outcome);}
                            }
                        }
                    }
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
    fn counter_target_restrictions_and_unknown_hand_invariance_are_preserved_by_both_owners() {
        macro_rules! run {
            ($session:ident,$from:ident,$input:ident) => {
                for actor in [PlayerId::P0,PlayerId::P1] {
                    for (counter_name,target_name) in [("Essence Scatter","Think Twice"),("An Offer You Can't Refuse","Llanowar Elves")] {
                        let mut raw=state(actor);
                        let counter=put(&mut raw,actor,counter_name,Zone::Hand);
                        let target=put(&mut raw,actor,target_name,Zone::Hand);
                        let target_lands=if target_name=="Think Twice" {(0..2).map(|_|put(&mut raw,actor,"Island",Zone::Battlefield)).collect::<Vec<_>>()} else {vec![put(&mut raw,actor,"Forest",Zone::Battlefield)]};
                        let counter_lands=(0..2).map(|_|put(&mut raw,actor,"Island",Zone::Battlefield)).collect::<Vec<_>>();
                        let mut session=$session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                        for id in target_lands {act!(session,$input,|a|mana(a,id));}
                        act!(session,$input,|a|matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==target.0));
                        for id in counter_lands {act!(session,$input,|a|mana(a,id));}
                        let e=expected(&session.inner);
                        let (_,actions,_)=session.inner.$input(e,e.acting_player).unwrap();
                        assert!(!actions.iter().any(|a|matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==counter.0)));
                        // An otherwise affordable counter with the wrong
                        // target kind must remain absent from the real menu.
                        assert!(session.encode(e).is_ok());
                    }
                    for name in ["Counterspell","Essence Scatter","Refute","An Offer You Can't Refuse"] {
                        let mut raw=state(actor);
                        put(&mut raw,actor,name,Zone::Hand);
                        let hidden=put(&mut raw,actor.opponent(),"Brainstorm",Zone::Hand);
                        let mut alternative=raw.clone();
                        alternative.objects.get_mut(hidden).card_def=crate::card_def::card_id_by_name("Refute").unwrap();
                        alternative.objects.get_mut(hidden).name="Refute".into();
                        let a=$session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                        let b=$session::from_session(FastActorSessionV1::$from(alternative)).unwrap();
                        assert_eq!(a.encode(expected(&a.inner)).unwrap().packet,b.encode(expected(&b.inner)).unwrap().packet);
                    }
                }
            };
        }
        run!(
            FakeSession,
            from_fake_v50_fixture_state,
            fake_current_decision_input_v50
        );
        run!(
            RavenousSession,
            from_ravenous_v51_fixture_state,
            ravenous_current_decision_input_v51
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

    #[test]
    fn conqueror_and_season_exact_printing_roots_keep_cost_characteristics_and_entry() {
        for (name, color, generic, pips, types, power, keywords) in [
            (
                "Bloodthirsty Conqueror",
                "B",
                3,
                2,
                "Creature",
                Some(5),
                vec!["Flying", "Deathtouch"],
            ),
            ("Doubling Season", "G", 4, 1, "Enchantment", None, vec![]),
        ] {
            let id = crate::card_def::card_id_by_name(name).unwrap();
            assert_eq!(
                crate::card_def::CARD_DEFS[usize::from(id)].capability,
                crate::card_def::CardCapability::Partial
            );
            let descriptor = roots::registered_permanent_printing(id).unwrap().unwrap();
            let printing = &descriptor["registered_permanent_printing"];
            let card = &printing["characteristics"];
            assert_eq!(card["types"], json!([types]));
            assert_eq!(card["colors"], json!([color]));
            assert_eq!(card["mana_value"], 5);
            assert_eq!(card["power"], json!(power));
            assert_eq!(card["toughness"], json!(power));
            assert_eq!(card["ordered_keywords"], json!(keywords));
            assert_eq!(
                card["subtypes"],
                if power.is_some() {
                    json!(["Vampire", "Knight"])
                } else {
                    json!([])
                }
            );
            assert_eq!(card["supertypes"], json!([]));
            for key in ["token", "land", "enters_tapped"] {
                assert_eq!(card[key], false);
            }
            assert_eq!(card["printed_mana_abilities"], 0);
            assert_eq!(card["printed_activated_abilities"], 0);
            assert_eq!(card["minimum_blockers"], 1);
            let cast = &printing["ordered_cast_roots"][0];
            assert_eq!(printing["ordered_cast_roots"].as_array().unwrap().len(), 1);
            assert_eq!(cast["method"], "normal");
            assert_eq!(cast["ordinary_route"], "Hand");
            assert_eq!(cast["ordinary_origin"], json!(Zone::Hand));
            assert_eq!(cast["timing"], "CasterMainPhaseEmptyStackWithPriority");
            assert_eq!(cast["targets"], "None");
            assert_eq!(
                cast["ordered_costs"],
                json!([{"PrintedMana":{"generic":generic,"x_count":0,"pips":(0..pips).map(|_|json!({"Colored":color})).collect::<Vec<_>>()}}])
            );
            assert_eq!(
                cast["ordered_resolution"],
                json!(["MoveOriginalSpellFromStackToBattlefieldUnderCasterControl"])
            );
            assert_eq!(
                cast["physical_departure"],
                "BattlefieldOnResolutionOtherwiseGraveyard"
            );
            assert_eq!(cast["zone_replacements_still_apply"], true);
            assert_eq!(
                cast["other_cast_permissions_require_current_candidate_authority"],
                true
            );
        }
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        assert!(roots::registered_permanent_printing(forest)
            .unwrap()
            .is_none());
        assert!(matches!(
            roots::registered_permanent_printing(u16::MAX),
            Err(Error::UnsupportedPrintedRoot(_))
        ));
        // Family semantics remain independent of the new ordinary cast roots.
        let conq = crate::conqueror_public_v49::meanings().unwrap();
        assert_eq!(conq.len(), 2);
        assert!(conq[1]
            .ordered_facts
            .iter()
            .any(|fact| fact == "gain_saved_actual_amount_for_original_controller"));
        let season = crate::doubling_season_public_v48::meanings().unwrap();
        assert_eq!(season.len(), 3);
        assert!(season[1].positive_effect_token_creation_only);
        assert!(season[2].counters_of_each_kind_doubled);
        assert!(season[2].other_replacement_can_make_cost_or_rule_counter_event_eligible);
    }

    // Inspect the public lossless packet's schema fields, never sealed bindings.
    fn numeric_field_count(tree: &NumericTree, field: &str) -> usize {
        let rows = tree.rows();
        rows.iter()
            .enumerate()
            .filter(|(at, row)| {
                row[0] == 9
                    && rows.get(at + 1..at + 1 + field.len()).is_some_and(|bytes| {
                        bytes
                            .iter()
                            .zip(field.bytes())
                            .all(|(row, byte)| row[0] == 8 && row[1] == u16::from(byte))
                    })
            })
            .count()
    }

    #[test]
    fn genuine_paid_conqueror_season_roots_bind_ordered_cast_and_restore_in_separate_owners_both_seats(
    ) {
        macro_rules! run {
            ($session:ident,$from:ident,$input:ident,$world:ident) => {
                for actor in [PlayerId::P0,PlayerId::P1] {
                    for (name,land) in [("Bloodthirsty Conqueror","Swamp"),("Doubling Season","Forest")] {
                        let mut raw=state(actor);
                        let spell=put(&mut raw,actor,name,Zone::Hand);
                        put(&mut raw,actor,if name=="Doubling Season" {"Bloodthirsty Conqueror"} else {"Doubling Season"},Zone::Hand);
                        let lands=(0..5).map(|_|put(&mut raw,actor,land,Zone::Battlefield)).collect::<Vec<_>>();
                        let mut session=$session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                        for id in lands {act!(session,$input,|a|mana(a,id));}
                        let e=expected(&session.inner);
                        let (public,actions,_)=session.inner.$input(e,e.acting_player).unwrap();
                        // All registered family meanings already exist, so this
                        // specifically exercises the former covered-ID bypass.
                        assert_eq!(public.conqueror.meanings.len(),2);
                        assert_eq!(public.season.meanings.len(),3);
                        let decision=session.encode(e).unwrap();
                        assert_eq!(numeric_field_count(&decision.packet.state,"registered_permanent_printing"),2);
                        assert_eq!(decision.packet.actions.len(),actions.len());
                        let index=actions.iter().position(|action|matches!(action,ActionSemanticV1::CastSpell{source,..}if source.arena_id==spell.0)).unwrap();
                        let saved=session.snapshot();
                        session.restore(&saved).unwrap();
                        assert!(expected(&session.inner).environment_revision>e.environment_revision);
                        assert!(matches!(session.apply(&decision,index as u32),Err(Error::StaleBinding)));
                        let current=session.encode(expected(&session.inner)).unwrap();
                        assert_eq!(current.packet,decision.packet);
                        let mut reference=session.inner.clone();
                        let e=expected(&session.inner);
                        let (_,current_actions,_)=session.inner.$input(e,e.acting_player).unwrap();
                        assert_eq!(current_actions,actions);
                        let response=session.apply(&current,index as u32).unwrap();
                        assert_eq!(response,reference.step(e.episode_id,e.step,index as u32).unwrap());
                        assert_eq!(session.inner.$world(),reference.$world());
                        assert_eq!(session.inner.policy_step_count(),reference.policy_step_count());
                        assert_eq!(session.inner.physical_decision_count(),reference.physical_decision_count());
                        assert_eq!(session.inner.$world().players[actor.index()].mana_pool,[0;6]);
                        assert_eq!(session.inner.$world().objects.get(spell).zone,Zone::Stack);
                        let paid=session.snapshot();
                        for _ in 0..16 {
                            if session.inner.$world().stack.is_empty(){break;}
                            act!(session,$input,|action|matches!(action,ActionSemanticV1::Pass{..}));
                        }
                        assert!(session.inner.$world().stack.is_empty());
                        assert_eq!(session.inner.$world().objects.get(spell).zone,Zone::Battlefield);
                        assert_eq!(session.inner.$world().objects.get(spell).controller,actor);
                        let outcome=(session.inner.$world().clone(),session.inner.policy_step_count(),session.inner.physical_decision_count());
                        session.restore(&paid).unwrap();
                        for _ in 0..16 {
                            if session.inner.$world().stack.is_empty(){break;}
                            act!(session,$input,|action|matches!(action,ActionSemanticV1::Pass{..}));
                        }
                        assert_eq!((session.inner.$world().clone(),session.inner.policy_step_count(),session.inner.physical_decision_count()),outcome);
                    }
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
    fn ordinary_permanent_supplements_preserve_initial_hidden_hand_invariance_both_owners_both_seats(
    ) {
        macro_rules! run {
            ($session:ident,$from:ident) => {
                for actor in [PlayerId::P0, PlayerId::P1] {
                    let mut initial = state(actor);
                    put(&mut initial, actor, "Bloodthirsty Conqueror", Zone::Hand);
                    put(&mut initial, actor, "Doubling Season", Zone::Hand);
                    let hidden = put(
                        &mut initial,
                        actor.opponent(),
                        "Bloodthirsty Conqueror",
                        Zone::Hand,
                    );
                    let mut other = initial.clone();
                    other.objects.get_mut(hidden).card_def =
                        crate::card_def::card_id_by_name("Doubling Season").unwrap();
                    other.objects.get_mut(hidden).name = "Doubling Season".into();
                    // Distinct initial information sets, no retained input or history is edited.
                    let a = $session::from_session(FastActorSessionV1::$from(initial)).unwrap();
                    let b = $session::from_session(FastActorSessionV1::$from(other)).unwrap();
                    assert_eq!(
                        a.encode(expected(&a.inner)).unwrap().packet,
                        b.encode(expected(&b.inner)).unwrap().packet
                    );
                }
            };
        }
        run!(FakeSession, from_fake_v50_fixture_state);
        run!(RavenousSession, from_ravenous_v51_fixture_state);
    }

    // A complete local CardDef fixture. Non-Copy optional programs are absent
    // in both admitted recipes; no live catalog entry is mutated.
    fn ordinary_recipe(id: u16) -> crate::card_def::CardDef {
        let d = &crate::card_def::CARD_DEFS[usize::from(id)];
        crate::card_def::CardDef {
            name: d.name,
            capability: d.capability,
            cost: d.cost,
            generic_cost_reduction: None,
            ward_cost: None,
            spell_cannot_be_countered: d.spell_cannot_be_countered,
            records_mana_spent: d.records_mana_spent,
            equipment: None,
            types: d.types,
            subtypes: d.subtypes,
            supertypes: d.supertypes,
            power: d.power,
            toughness: d.toughness,
            starting_loyalty: None,
            enters_with_plus_one_counters: None,
            controlled_counter_keyword: None,
            game_outcome_protection_v1: None,
            controlled_creature_grant: None,
            is_land: d.is_land,
            produces_mana: d.produces_mana,
            colors: d.colors,
            target_spec: d.target_spec,
            keywords: d.keywords,
            spell_effect: d.spell_effect,
            mana_ability: d.mana_ability,
            alt_cost: None,
            kicker_cost: None,
            additional_cost: None,
            flashback: None,
            activated_abilities: d.activated_abilities,
            plot_cost: None,
            madness_cost: None,
            mode2: None,
            mode3: None,
            additional_modes: d.additional_modes,
            kicked_target_spec: None,
            is_token: d.is_token,
            escape: None,
            mana_ability_choices: d.mana_ability_choices,
            enters_battlefield_tapped: d.enters_battlefield_tapped,
            mana_ability_def: None,
            minimum_blockers: d.minimum_blockers,
            omen: None,
            mana_value: d.mana_value,
            mana_ability_includes_chosen_color: d.mana_ability_includes_chosen_color,
            as_enters_choose_color_other_than: None,
            additional_mana_abilities: d.additional_mana_abilities,
            object_name: d.object_name,
            enters_battlefield_tapped_unless: None,
            attachment: None,
            transform_face: None,
            saga: None,
            optional_additional_cost: None,
            changeling: d.changeling,
            bestow: None,
            delve: d.delve,
            adventure: None,
            cant_be_blocked_by_monarchs_creatures: d.cant_be_blocked_by_monarchs_creatures,
            conditional_tap_yield: None,
            enters_tapped_unless_controller: None,
            additional_mana_ability_conditions: d.additional_mana_ability_conditions,
            restricted_mana_abilities: d.restricted_mana_abilities,
            animation: None,
            activated_ability_generic_reductions: d.activated_ability_generic_reductions,
            combat_static_rules: d.combat_static_rules,
            as_enters_color_choice_v1: d.as_enters_color_choice_v1,
            chosen_color_creature_boost_v1: d.chosen_color_creature_boost_v1,
            damage_replacement_v1: None,
            token_counter_replacement_v48: None,
            starting_life_self_bonuses_v1: d.starting_life_self_bonuses_v1,
            source_type_hexproof_v1: None,
            other_controlled_nontoken_death_v1: None,
        }
    }

    #[test]
    fn frenzied_banner_closed_printings_supply_exact_cast_entry_and_mana_operands() {
        use crate::mana::ManaColor;
        for (name, cost, kind, power, subtypes) in [
            (
                "Frenzied Goblin",
                json!({"generic":0,"pips":[{"Colored":"R"}],"x_count":0}),
                "Creature",
                Some(1),
                json!(["Goblin", "Berserker"]),
            ),
            (
                "Heraldic Banner",
                json!({"generic":3,"pips":[],"x_count":0}),
                "Artifact",
                None,
                json!([]),
            ),
        ] {
            let id = crate::card_def::card_id_by_name(name).unwrap();
            let actual = roots::registered_permanent_printing(id).unwrap().unwrap();
            assert_eq!(
                roots::attack_or_banner_printing(id, &ordinary_recipe(id)).unwrap(),
                Some(actual.clone())
            );
            let printing = &actual["registered_permanent_printing"];
            let card = &printing["characteristics"];
            assert_eq!(card["types"], json!([kind]));
            assert_eq!(card["subtypes"], subtypes);
            assert_eq!(
                card["colors"],
                if name == "Frenzied Goblin" {
                    json!(["R"])
                } else {
                    json!([])
                }
            );
            assert_eq!(card["supertypes"], json!([]));
            assert_eq!(card["power"], json!(power));
            assert_eq!(card["toughness"], json!(power));
            assert_eq!(card["ordered_keywords"], json!([]));
            assert_eq!(
                card["mana_value"],
                if name == "Frenzied Goblin" { 1 } else { 3 }
            );
            assert_eq!(card["minimum_blockers"], 1);
            for key in ["token", "land", "enters_tapped"] {
                assert_eq!(card[key], false);
            }
            assert_eq!(card["printed_activated_abilities"], 0);
            let cast = &printing["ordered_cast_roots"][0];
            assert_eq!(printing["ordered_cast_roots"].as_array().unwrap().len(), 1);
            assert_eq!(cast["ordered_costs"], json!([{"PrintedMana":cost}]));
            assert_eq!(cast["method"], "normal");
            assert_eq!(cast["ordinary_route"], "Hand");
            assert_eq!(cast["ordinary_origin"], json!(Zone::Hand));
            assert_eq!(cast["targets"], "None");
            assert_eq!(cast["timing"], "CasterMainPhaseEmptyStackWithPriority");
            assert_eq!(
                cast["ordered_resolution"],
                json!(["MoveOriginalSpellFromStackToBattlefieldUnderCasterControl"])
            );
            assert_eq!(
                cast["physical_departure"],
                "BattlefieldOnResolutionOtherwiseGraveyard"
            );
            assert_eq!(cast["zone_replacements_still_apply"], true);
            if name == "Heraldic Banner" {
                assert_eq!(card["printed_mana_abilities"], 1);
                let entry = &printing["ordered_entry_replacements"][0];
                assert_eq!(
                    entry["choices"],
                    json!([
                        ManaColor::W,
                        ManaColor::U,
                        ManaColor::B,
                        ManaColor::R,
                        ManaColor::G
                    ])
                );
                assert_eq!(entry["chooser"], "ProposedBattlefieldController");
                assert_eq!(entry["before_original_battlefield_move"], true);
                assert_eq!(entry["all_batch_choices_before_any_entry"], true);
                assert_eq!(entry["fresh_choice_each_entering_incarnation"], true);
                let mana = &printing["ordered_mana_roots"][0];
                assert_eq!(mana["source_zone"], json!(Zone::Battlefield));
                assert_eq!(mana["ordered_costs"], json!(["TapOriginalSource"]));
                assert_eq!(mana["recipient"], "CurrentSourceController");
                assert_eq!(mana["output"], "CurrentIncarnationChosenWubrgColor");
                assert_eq!(mana["spending_restriction"], "Unrestricted");
                assert_eq!(mana["amount"], 1);
                assert_eq!(mana["uses_stack"], false);
            } else {
                assert_eq!(card["printed_mana_abilities"], 0);
                assert_eq!(printing["ordered_entry_replacements"], json!([]));
                assert_eq!(printing["ordered_mana_roots"], json!([]));
            }
            macro_rules! reject {
                ($field:ident,$value:expr)=>{{
                    let mut changed=ordinary_recipe(id); changed.$field=$value;
                    assert!(matches!(roots::attack_or_banner_printing(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));
                }};
            }
            reject!(cost, crate::mana::Cost::zero());
            reject!(types, &[crate::card_def::CardType::Sorcery]);
            reject!(subtypes, &[crate::card_def::Subtype::Human]);
            reject!(supertypes, &[crate::card_def::Supertype::Legendary]);
            reject!(colors, &[ManaColor::G]);
            reject!(power, Some(7));
            reject!(toughness, Some(8));
            reject!(keywords, crate::card_def::Keywords::HASTE);

            reject!(target_spec, crate::card_def::TargetSpec::Creature);
            reject!(
                alt_cost,
                Some(crate::card_def::AltCostDef {
                    components: &[],
                    condition: crate::card_def::AltCostCondition::Always
                })
            );
            reject!(
                mode2,
                Some(crate::card_def::ModeDef {
                    target_spec: crate::card_def::TargetSpec::None,
                    effect: || crate::effect::EffectOp::Sequence(vec![])
                })
            );
            reject!(spell_effect, || None);
            reject!(
                activated_abilities,
                crate::card_def::CARD_DEFS
                    [usize::from(crate::card_def::card_id_by_name("Masked Meower").unwrap())]
                .activated_abilities
            );
            reject!(
                combat_static_rules,
                crate::card_def::CombatStaticRulesDef {
                    must_attack_each_combat: true,
                    cant_be_blocked_by_subtype: None
                }
            );
            reject!(
                source_type_hexproof_v1,
                Some(crate::card_def::SourceTypeHexproofDefV1::Instants)
            );
            reject!(other_controlled_nontoken_death_v1,Some(crate::card_def::OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom));
            reject!(produces_mana, &[ManaColor::G]);
            reject!(
                mana_ability_includes_chosen_color,
                name != "Heraldic Banner"
            );
            reject!(
                as_enters_color_choice_v1,
                if name == "Heraldic Banner" {
                    None
                } else {
                    Some(crate::card_def::AsEntersColorChoiceDefV1::AnyWubrg)
                }
            );
        }
        // Independent family bodies are still complete ordered meanings.
        let f = crate::rules_meaning_frenzied_goblin_v1::meaning(
            &crate::rules_meaning_frenzied_goblin_v1::live_selector().unwrap(),
            crate::card_def::card_id_by_name("Frenzied Goblin").unwrap(),
            0,
        )
        .unwrap();
        assert!(f.payment_optional_at_resolution && f.target_mandatory_at_placement_before_payment);
        assert_eq!(f.payment_colored, vec![ManaColor::R]);
        use crate::rules_meaning_heraldic_banner_v1 as b;
        for root in [
            b::RootV1::PermanentSpell,
            b::RootV1::AsEntersColor,
            b::RootV1::ChosenColorAnthem,
            b::RootV1::ChosenColorMana,
        ] {
            let m = b::meaning(
                &b::live_selector().unwrap(),
                crate::card_def::card_id_by_name("Heraldic Banner").unwrap(),
                root,
            )
            .unwrap();
            assert_eq!((m.rule.power_delta, m.rule.toughness_delta), (1, 0));
            assert!(
                m.rule.choice_before_entry
                    && m.rule.mana_cost_is_tap_self
                    && m.rule.resolves_without_stack
            );
        }
    }

    #[test]
    fn genuine_paid_frenzied_banner_covered_roots_pay_enter_choose_mana_and_restore_v50_v51_both_seats(
    ) {
        macro_rules! run {
            ($session:ident,$from:ident,$input:ident,$world:ident)=>{
                for actor in [PlayerId::P0,PlayerId::P1] {
                    let mut raw=state(actor);
                    let goblin=put(&mut raw,actor,"Frenzied Goblin",Zone::Hand);
                    let next_goblin=put(&mut raw,actor,"Frenzied Goblin",Zone::Hand);
                    let banner=put(&mut raw,actor,"Heraldic Banner",Zone::Hand);
                    let mountain=put(&mut raw,actor,"Mountain",Zone::Battlefield);
                    let forests=(0..3).map(|_|put(&mut raw,actor,"Forest",Zone::Battlefield)).collect::<Vec<_>>();
                    let mut session=$session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                    let e=expected(&session.inner);
                    let (public,_,_)=session.inner.$input(e,e.acting_player).unwrap();
                    assert!(public.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Frenzied(v)if v.roots.len()==1)));
                    assert!(public.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Banner(v)if v.roots.len()==4)));
                    assert_eq!(numeric_field_count(session.encode(e).unwrap().packet.state(),"registered_permanent_printing"),2);
                    act!(session,$input,|a|mana(a,mountain));
                    act!(session,$input,|a|matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==goblin.0));
                    assert_eq!(session.inner.$world().players[actor.index()].mana_pool,[0;6]);
                    assert!(session.inner.$world().objects.get(mountain).tapped);
                    for _ in 0..16 {if session.inner.$world().stack.is_empty(){break;}act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                    assert_eq!(session.inner.$world().objects.get(goblin).zone,Zone::Battlefield);
                    for id in &forests {act!(session,$input,|a|mana(a,*id));}
                    let e=expected(&session.inner);let (_,actions,_)=session.inner.$input(e,e.acting_player).unwrap();
                    let index=actions.iter().position(|a|matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==banner.0)).unwrap();
                    let decision=session.encode(e).unwrap();
                    assert_eq!(numeric_field_count(decision.packet.state(),"registered_permanent_printing"),2);
                    let mut erased=decision.clone();erased.packet.state=NumericTree::encode(&json!({})).unwrap();
                    let before=session.inner.privileged_core_environment_hash();
                    assert!(matches!(session.apply(&erased,index as u32),Err(Error::StaleBinding)));
                    assert_eq!(session.inner.privileged_core_environment_hash(),before);
                    let mut reference=session.inner.clone();let r=reference.step(e.episode_id,e.step,index as u32).unwrap();
                    assert_eq!(session.apply(&decision,index as u32).unwrap(),r);assert_eq!(session.inner.$world(),reference.$world());
                    assert_eq!(session.inner.$world().objects.get(banner).zone,Zone::Stack);
                    assert_eq!(session.inner.$world().players[actor.index()].mana_pool,[0;6]);
                    assert!(forests.iter().all(|id|session.inner.$world().objects.get(*id).tapped));
                    let paid=session.snapshot();let original=session.encode(expected(&session.inner)).unwrap();let mut outcome=None;
                    for replay in 0..2 {
                        if replay!=0 {session.restore(&paid).unwrap();let unchanged=session.inner.privileged_core_environment_hash();assert!(matches!(session.apply(&original,0),Err(Error::StaleBinding)));assert_eq!(session.inner.privileged_core_environment_hash(),unchanged);assert_eq!(session.encode(expected(&session.inner)).unwrap().packet,original.packet);}
                        let mut chosen=0;
                        for _ in 0..32 {
                            let world=session.inner.$world();
                            if world.stack.is_empty() && world.engine.pending_effect.is_none() && !crate::linked_aura_operation_v1::has_banner_pending(world){break;}
                            let e=expected(&session.inner);let (_,actions,_)=session.inner.$input(e,e.acting_player).unwrap();
                            if actions.iter().any(|a|matches!(a,ActionSemanticV1::ChooseEntryColorV36{..})) {
                                assert_eq!(world.objects.get(banner).zone,Zone::Stack);
                                act!(session,$input,|a|matches!(a,ActionSemanticV1::ChooseEntryColorV36{color:crate::mana::ManaColor::R,..}));chosen+=1;
                            }else{act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                        }
                        assert_eq!(chosen,1);
                        assert_eq!(session.inner.$world().objects.get(banner).zone,Zone::Battlefield);
                        assert_eq!(session.inner.$world().objects.get(banner).controller,actor);
                        assert_eq!(session.inner.$world().objects.get(banner).v4.chosen_color,Some(crate::mana::ManaColor::R));
                        assert_eq!(crate::engine::effective_power(session.inner.$world(),goblin),2);
                        let stack=session.inner.$world().stack.clone();
                        act!(session,$input,|a|mana(a,banner));
                        assert_eq!(session.inner.$world().stack,stack);
                        assert!(session.inner.$world().objects.get(banner).tapped);
                        assert_eq!(session.inner.$world().players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()],1);
                        act!(session,$input,|a|matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==next_goblin.0));
                        assert_eq!(session.inner.$world().players[actor.index()].mana_pool,[0;6]);
                        for _ in 0..16 {if session.inner.$world().stack.is_empty(){break;}act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                        assert_eq!(session.inner.$world().objects.get(next_goblin).zone,Zone::Battlefield);
                        let complete=(session.inner.$world().clone(),session.inner.policy_step_count(),session.inner.physical_decision_count());
                        if let Some(previous)=&outcome {assert_eq!(&complete,previous);}else{outcome=Some(complete);}
                    }
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
    fn frenzied_banner_supplements_follow_actor_knowledge_and_ignore_independent_hidden_hands_v50_v51_both_seats(
    ) {
        macro_rules! run {
            ($session:ident,$from:ident) => {
                for actor in [PlayerId::P0, PlayerId::P1] {
                    for known in [false, true] {
                        let mut raw = state(actor);
                        if known {
                            put(&mut raw, actor, "Frenzied Goblin", Zone::Hand);
                            put(&mut raw, actor, "Heraldic Banner", Zone::Hand);
                        }
                        let hidden = put(&mut raw, actor.opponent(), "Frenzied Goblin", Zone::Hand);
                        let mut other = raw.clone();
                        other.objects.get_mut(hidden).card_def =
                            crate::card_def::card_id_by_name("Heraldic Banner").unwrap();
                        other.objects.get_mut(hidden).name = "Heraldic Banner".into();
                        let a = $session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                        let b = $session::from_session(FastActorSessionV1::$from(other)).unwrap();
                        let pa = a.encode(expected(&a.inner)).unwrap();
                        let pb = b.encode(expected(&b.inner)).unwrap();
                        assert_eq!(pa.packet, pb.packet);
                        assert_eq!(
                            numeric_field_count(pa.packet.state(), "registered_permanent_printing"),
                            if known { 2 } else { 0 }
                        );
                    }
                }
            };
        }
        run!(FakeSession, from_fake_v50_fixture_state);
        run!(RavenousSession, from_ravenous_v51_fixture_state);
    }

    #[test]
    fn sower_closed_printing_supplies_normal_cast_and_exact_native_activation_cost() {
        use crate::card_def::{
            ActivationTargetFilter, CardType, CostComponent, Keywords, ModeDef, Subtype, TargetSpec,
        };
        use crate::mana::{Cost, ManaColor, Pip};
        let id = crate::card_def::card_id_by_name("Sower of Chaos").unwrap();
        let descriptor = roots::registered_permanent_printing(id).unwrap().unwrap();
        assert_eq!(
            roots::sower_printing(id, &ordinary_recipe(id)).unwrap(),
            Some(descriptor.clone())
        );
        let printing = &descriptor["registered_permanent_printing"];
        let card = &printing["characteristics"];
        assert_eq!(card["types"], json!(["Creature"]));
        assert_eq!(card["subtypes"], json!(["Devil"]));
        assert_eq!(card["supertypes"], json!([]));
        assert_eq!(card["colors"], json!(["R"]));
        assert_eq!(card["mana_value"], 4);
        assert_eq!(card["power"], 4);
        assert_eq!(card["toughness"], 3);
        assert_eq!(card["ordered_keywords"], json!([]));
        assert_eq!(card["minimum_blockers"], 1);
        assert_eq!(card["printed_mana_abilities"], 0);
        assert_eq!(card["printed_activated_abilities"], 1);
        for field in ["token", "land", "enters_tapped"] {
            assert_eq!(card[field], false);
        }
        let cast = &printing["ordered_cast_roots"][0];
        assert_eq!(
            cast["ordered_costs"],
            json!([{"PrintedMana":{"generic":3,"pips":[{"Colored":"R"}],"x_count":0}}])
        );
        assert_eq!(cast["method"], "normal");
        assert_eq!(cast["ordinary_route"], "Hand");
        assert_eq!(cast["ordinary_origin"], "Hand");
        assert_eq!(cast["timing"], "CasterMainPhaseEmptyStackWithPriority");
        assert_eq!(cast["targets"], "None");
        assert_eq!(
            cast["ordered_resolution"],
            json!(["MoveOriginalSpellFromStackToBattlefieldUnderCasterControl"])
        );
        assert_eq!(
            cast["physical_departure"],
            "BattlefieldOnResolutionOtherwiseGraveyard"
        );
        assert_eq!(cast["zone_replacements_still_apply"], true);
        let activation = &printing["ordered_activation_roots"][0];
        assert_eq!(activation["ability_index"], 0);
        assert_eq!(
            activation["ordered_costs"],
            json!([{"Mana":{"generic":2,"pips":[{"Colored":"R"}],"x_count":0}}])
        );
        assert_eq!(activation["timing"], "WithPriorityAtInstantSpeed");
        assert_eq!(
            activation["original_source_incarnation_bound_before_payment"],
            true
        );
        assert_eq!(activation["payment_precedes_stack_publication"], true);
        assert_eq!(
            activation["countered_activation_never_refunds_payment"],
            true
        );
        let original = crate::rules_meaning_sower_v1::meaning_v1(
            &crate::rules_meaning_sower_v1::live_selector_v1().unwrap(),
            id,
            0,
        )
        .unwrap();
        assert_eq!(
            activation["meaning"],
            serde_json::to_value(&original).unwrap()
        );
        assert!(
            !original.requires_tap
                && !original.sorcery_speed_only
                && original.exact_target_incarnation
                && original.recheck_full_target_legality
        );
        assert_eq!(original.target_spec, TargetSpec::Creature);
        assert!(!original.source_survival_required_after_payment);
        macro_rules! reject {
            ($field:ident,$value:expr)=>{{let mut changed=ordinary_recipe(id);changed.$field=$value;
                assert!(matches!(roots::sower_printing(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));}};
        }
        reject!(
            cost,
            Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::R)],
                x_count: 0
            }
        );
        reject!(types, &[CardType::Enchantment]);
        reject!(subtypes, &[Subtype::Goblin]);
        reject!(power, Some(3));
        reject!(toughness, Some(4));
        reject!(keywords, Keywords::HASTE);
        reject!(target_spec, TargetSpec::Creature);
        reject!(
            alt_cost,
            Some(crate::card_def::AltCostDef {
                components: &[],
                condition: crate::card_def::AltCostCondition::Always
            })
        );
        reject!(
            mode2,
            Some(ModeDef {
                target_spec: TargetSpec::None,
                effect: || crate::effect::EffectOp::Sequence(vec![])
            })
        );
        reject!(spell_effect, || None);
        reject!(activated_abilities, &[]);
        reject!(
            source_type_hexproof_v1,
            Some(crate::card_def::SourceTypeHexproofDefV1::Instants)
        );
        reject!(other_controlled_nontoken_death_v1,Some(crate::card_def::OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom));
        reject!(
            combat_static_rules,
            crate::card_def::CombatStaticRulesDef {
                must_attack_each_combat: true,
                cant_be_blocked_by_subtype: None
            }
        );
        // Each native ability field is independently closed, including the
        // face selector not represented by the historical activation-only body.
        macro_rules! reject_ability {
            ($field:ident,$value:expr)=>{{let mut changed=ordinary_recipe(id);let mut ability=changed.activated_abilities[0];ability.$field=$value;
                changed.activated_abilities=Box::leak(vec![ability].into_boxed_slice());
                assert!(matches!(roots::sower_printing(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));}};
        }
        reject_ability!(cost, &[CostComponent::Mana(Cost::zero())]);
        reject_ability!(target_spec, TargetSpec::AnyTarget);
        reject_ability!(effect, || crate::effect::EffectOp::Sequence(vec![]));
        reject_ability!(activation_zone, Zone::Hand);
        reject_ability!(sorcery_speed_only, true);
        reject_ability!(
            activation_target_filter,
            ActivationTargetFilter::CreatureSubtype(Subtype::Goblin)
        );
        reject_ability!(max_activations_per_turn, Some(1));
        reject_ability!(face, Some(1));
    }

    #[test]
    fn genuine_paid_sower_cast_and_target_bound_activation_restore_v50_v51_both_seats() {
        macro_rules! run {
            ($session:ident,$from:ident,$input:ident,$world:ident)=>{
                for actor in [PlayerId::P0,PlayerId::P1] {
                    let mut raw=state(actor);let source=put(&mut raw,actor,"Sower of Chaos",Zone::Hand);
                    let target=put(&mut raw,actor.opponent(),"Llanowar Elves",Zone::Battlefield);
                    let mountains=(0..2).map(|_|put(&mut raw,actor,"Mountain",Zone::Battlefield)).collect::<Vec<_>>();
                    let forests=(0..5).map(|_|put(&mut raw,actor,"Forest",Zone::Battlefield)).collect::<Vec<_>>();
                    let mut session=$session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                    let e=expected(&session.inner);let (public,_,_)=session.inner.$input(e,e.acting_player).unwrap();
                    assert!(public.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Sower(v)if v.roots.len()==1)));
                    assert_eq!(numeric_field_count(session.encode(e).unwrap().packet.state(),"registered_permanent_printing"),1);
                    for id in std::iter::once(&mountains[0]).chain(forests[..3].iter()) {act!(session,$input,|a|mana(a,*id));}
                    act!(session,$input,|a|matches!(a,ActionSemanticV1::CastSpell{source:card,..}if card.arena_id==source.0));
                    assert_eq!(session.inner.$world().players[actor.index()].mana_pool,[0;6]);
                    assert_eq!(session.inner.$world().objects.get(source).zone,Zone::Stack);
                    let cast_paid=session.snapshot();let mut entry=None;
                    for replay in 0..2 {
                        if replay!=0 {session.restore(&cast_paid).unwrap();}
                        for _ in 0..16 {if session.inner.$world().stack.is_empty(){break;}act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                        assert_eq!(session.inner.$world().objects.get(source).zone,Zone::Battlefield);
                        assert!(session.inner.$world().objects.get(source).summoning_sick);
                        let result=(session.inner.$world().clone(),session.inner.policy_step_count(),session.inner.physical_decision_count());
                        if let Some(previous)=&entry {assert_eq!(&result,previous);}else {entry=Some(result);}
                    }
                    for id in std::iter::once(&mountains[1]).chain(forests[3..].iter()) {act!(session,$input,|a|mana(a,*id));}
                    act!(session,$input,|a|matches!(a,ActionSemanticV1::ActivateAbility{source:card,ability_index:0,granted_ability,..}if card.arena_id==source.0 && granted_ability.is_none()));
                    // Target selection precedes native payment and publication.
                    assert!(session.inner.$world().stack.is_empty());assert!(session.inner.$world().engine.pending_activation.is_some());
                    assert_eq!(session.inner.$world().players[actor.index()].mana_pool.iter().copied().map(u32::from).sum::<u32>(),3);
                    assert!(!session.inner.$world().objects.get(source).tapped);
                    let e=expected(&session.inner);let (_,actions,_)=session.inner.$input(e,e.acting_player).unwrap();
                    let target_index=actions.iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{source:card,target:crate::rl::TargetRefV1::Object{object},remaining:1,..}if card.arena_id==source.0 && object.arena_id==target.0 && object.zone==Zone::Battlefield)).unwrap();
                    let original=session.encode(e).unwrap();assert_eq!(numeric_field_count(original.packet.state(),"registered_permanent_printing"),1);
                    let pending=session.snapshot();let mut outcome=None;
                    for replay in 0..2 {
                        if replay!=0 {session.restore(&pending).unwrap();let before=session.inner.privileged_core_environment_hash();assert!(matches!(session.apply(&original,target_index as u32),Err(Error::StaleBinding)));assert_eq!(session.inner.privileged_core_environment_hash(),before);assert_eq!(session.encode(expected(&session.inner)).unwrap().packet,original.packet);}
                        let e=expected(&session.inner);let decision=session.encode(e).unwrap();
                        let mut erased=decision.clone();erased.packet.state=NumericTree::encode(&json!({})).unwrap();let before=session.inner.privileged_core_environment_hash();
                        assert!(matches!(session.apply(&erased,target_index as u32),Err(Error::StaleBinding)));assert_eq!(session.inner.privileged_core_environment_hash(),before);
                        let mut reference=session.inner.clone();let expected_reply=reference.step(e.episode_id,e.step,target_index as u32).unwrap();
                        assert_eq!(session.apply(&decision,target_index as u32).unwrap(),expected_reply);assert_eq!(session.inner.$world(),reference.$world());
                        let paid=session.inner.$world();assert_eq!(paid.players[actor.index()].mana_pool,[0;6]);assert!(!paid.objects.get(source).tapped);
                        assert!(mountains.iter().chain(forests.iter()).all(|id|paid.objects.get(*id).tapped));
                        assert_eq!(paid.stack.len(),1);let ability=&paid.stack[0];assert_eq!(ability.kind,crate::state::StackItemKind::ActivatedAbility);
                        assert_eq!(ability.source,source);assert_eq!(ability.controller,actor);assert_eq!(ability.targets,vec![crate::state::Target::Object(target)]);
                        let captured=ability.v4.ability_source_contract.unwrap();assert_eq!(captured.source,source);assert_eq!(captured.zone_change_count,paid.objects.get(source).zone_change_count);
                        let paid_snapshot=session.snapshot();
                        for _ in 0..16 {if session.inner.$world().stack.is_empty(){break;}act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                        assert!(crate::sower_of_chaos_v1::cant_block(session.inner.$world(),target));
                        let result=(session.inner.$world().clone(),session.inner.policy_step_count(),session.inner.physical_decision_count());
                        session.restore(&paid_snapshot).unwrap();for _ in 0..16 {if session.inner.$world().stack.is_empty(){break;}act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                        assert_eq!((session.inner.$world().clone(),session.inner.policy_step_count(),session.inner.physical_decision_count()),result);
                        if let Some(previous)=&outcome {assert_eq!(&result,previous);}else {outcome=Some(result);}
                    }
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
    fn sower_supplement_requires_actor_authorized_printing_without_hidden_hand_leak_v50_v51_both_seats(
    ) {
        macro_rules! run {
            ($session:ident,$from:ident) => {
                for actor in [PlayerId::P0, PlayerId::P1] {
                    for known in [false, true] {
                        let mut raw = state(actor);
                        if known {
                            put(&mut raw, actor, "Sower of Chaos", Zone::Hand);
                        }
                        let hidden = put(&mut raw, actor.opponent(), "Sower of Chaos", Zone::Hand);
                        let mut other = raw.clone();
                        other.objects.get_mut(hidden).card_def =
                            crate::card_def::card_id_by_name("Frenzied Goblin").unwrap();
                        other.objects.get_mut(hidden).name = "Frenzied Goblin".into();
                        let a = $session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                        let b = $session::from_session(FastActorSessionV1::$from(other)).unwrap();
                        let pa = a.encode(expected(&a.inner)).unwrap();
                        let pb = b.encode(expected(&b.inner)).unwrap();
                        assert_eq!(pa.packet, pb.packet);
                        assert_eq!(
                            numeric_field_count(pa.packet.state(), "registered_permanent_printing"),
                            usize::from(known)
                        );
                    }
                }
            };
        }
        run!(FakeSession, from_fake_v50_fixture_state);
        run!(RavenousSession, from_ravenous_v51_fixture_state);
    }

    #[test]
    fn wildwood_closed_variable_printing_keeps_native_x_cost_mana_value_and_single_entry_owner() {
        use crate::card_def::{CostComponent, TargetSpec};
        use crate::mana::{Cost, ManaColor, Pip};
        let id = crate::card_def::card_id_by_name("Wildwood Scourge").unwrap();
        let descriptor = roots::registered_permanent_printing(id).unwrap().unwrap();
        assert_eq!(
            roots::wildwood_printing(id, &ordinary_recipe(id)).unwrap(),
            Some(descriptor.clone())
        );
        let printing = &descriptor["registered_permanent_printing"];
        let card = &printing["characteristics"];
        assert_eq!(card["types"], json!(["Creature"]));
        assert_eq!(card["subtypes"], json!(["Hydra"]));
        assert_eq!(card["supertypes"], json!([]));
        assert_eq!(card["colors"], json!(["G"]));
        assert_eq!(card["mana_value"], 1);
        assert_eq!(card["power"], 0);
        assert_eq!(card["toughness"], 0);
        assert_eq!(card["ordered_keywords"], json!([]));
        assert_eq!(card["printed_activated_abilities"], 0);
        assert_eq!(card["printed_mana_abilities"], 0);
        let cast = &printing["ordered_cast_roots"][0];
        assert_eq!(cast["method"], "normal");
        assert_eq!(cast["ordinary_route"], "Hand");
        assert_eq!(cast["ordinary_origin"], "Hand");
        assert_eq!(cast["timing"], "CasterMainPhaseEmptyStackWithPriority");
        assert_eq!(cast["targets"], "None");
        let x = &cast["ordered_announcement"][0];
        assert_eq!(x["minimum"], 0);
        assert_eq!(x["supported_maximum"], 255);
        assert_eq!(x["menu"], "SourceBoundChooseEffectOption");
        assert_eq!(x["option_index_adds_to_minimum"], true);
        assert_eq!(
            x["option_count_is_current_payable_maximum_minus_minimum_plus_one"],
            true
        );
        assert_eq!(x["sole_payable_value_is_selected_automatically"], true);
        assert_eq!(x["maximum_requires_current_native_payment_quote"], true);
        assert_eq!(x["native_option_menu_requires_original_pending_cast"], true);
        assert_eq!(x["native_without_mana_cost_permission_forces_zero_x"], true);
        let payment = &cast["ordered_costs"][0];
        assert_eq!(
            payment["native_printed_cost"],
            json!({"generic":0,"pips":[{"Colored":"G"}],"x_count":1})
        );
        assert_eq!(
            payment["x_source"],
            "NativePendingCastChoiceThenFinalizedStackStateXValue"
        );
        assert_eq!(payment["each_printed_x_adds_chosen_x_generic_mana"], true);
        assert_eq!(
            payment["payment_precedes_finalized_stack_publication"],
            true
        );
        assert_eq!(
            cast["mana_value"],
            json!({"printed_base":1,"stack_x_multiplicity":1,"stack_uses_finalized_captured_x":true,"pending_unfinalized_object_uses_printed_base":true,"off_stack_x_value":0,"maximum_represented_stack_value":256})
        );
        assert_eq!(
            cast["ordered_resolution"],
            json!(["ResolveExistingWildwoodCapturedXEntry"])
        );
        assert_eq!(
            cast["existing_entry_meaning_owns_the_only_counter_placement"],
            true
        );
        assert_eq!(
            cast["physical_departure"],
            "BattlefieldOnResolutionOtherwiseGraveyard"
        );
        let family = crate::rules_meaning_wildwood_v1::meaning_v1(
            &crate::rules_meaning_wildwood_v1::live_selector_v1().unwrap(),
            id,
        )
        .unwrap();
        assert!(
            family.entry.counter_count_equals_captured_x
                && family.entry.counters_committed_during_entry_before_sba
                && family.entry.entry_is_a_replacement_not_a_trigger
                && family.entry.zero_base_toughness_uses_ordinary_sba
        );
        assert!(
            family.counter_trigger.recipient_is_another_object
                && !family
                    .counter_trigger
                    .later_batch_collection_duplicates_capture
        );
        assert_eq!(
            family.counter_trigger.excluded_effective_subtype,
            crate::card_def::Subtype::Hydra
        );
        macro_rules! reject {
            ($field:ident,$value:expr)=>{{let mut d=ordinary_recipe(id);d.$field=$value;assert!(matches!(roots::wildwood_printing(id,&d),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));}};
        }
        reject!(
            cost,
            Cost {
                generic: 0,
                pips: &[Pip::Colored(ManaColor::G)],
                x_count: 0
            }
        );
        reject!(
            cost,
            Cost {
                generic: 0,
                pips: &[Pip::Colored(ManaColor::G)],
                x_count: 2
            }
        );
        reject!(mana_value, 2);
        reject!(power, Some(1));
        reject!(toughness, Some(1));
        reject!(target_spec, TargetSpec::Creature);
        reject!(spell_effect, || Some(crate::effect::EffectOp::MoveObject {
            object: crate::effect::ObjectRef::ThisSource,
            to_zone: Zone::Battlefield
        }));
        reject!(
            alt_cost,
            Some(crate::card_def::AltCostDef {
                components: &[CostComponent::Mana(Cost::zero())],
                condition: crate::card_def::AltCostCondition::Always
            })
        );
        reject!(
            mode2,
            Some(crate::card_def::ModeDef {
                target_spec: TargetSpec::None,
                effect: || crate::effect::EffectOp::Sequence(vec![])
            })
        );
        reject!(
            bestow,
            Some(crate::card_def::BestowDef {
                cost: Cost::zero(),
                target_spec: TargetSpec::Creature
            })
        );
        reject!(
            activated_abilities,
            crate::card_def::CARD_DEFS
                [usize::from(crate::card_def::card_id_by_name("Sower of Chaos").unwrap())]
            .activated_abilities
        );
        reject!(
            source_type_hexproof_v1,
            Some(crate::card_def::SourceTypeHexproofDefV1::Instants)
        );
        reject!(other_controlled_nontoken_death_v1,Some(crate::card_def::OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom));
    }

    #[test]
    fn genuine_paid_wildwood_x_zero_three_255_choice_entry_and_restore_v50_v51_both_seats() {
        macro_rules! run {
            ($session:ident,$from:ident,$input:ident,$world:ident)=>{
                for actor in [PlayerId::P0,PlayerId::P1] {for x in [0u8,3,255] {
                    let mut raw=state(actor);let source=put(&mut raw,actor,"Wildwood Scourge",Zone::Hand);
                    let forest=put(&mut raw,actor,"Forest",Zone::Battlefield);
                    // Declared initial funding before all native journals. The
                    // actual Forest ability supplies G; native finalization
                    // pays every chosen generic X from this bounded pool.
                    raw.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()]=x;
                    let mut session=$session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                    assert_eq!(crate::engine::object_mana_value(session.inner.$world(),source),1);
                    let e=expected(&session.inner);let (public,_,_)=session.inner.$input(e,e.acting_player).unwrap();
                    assert!(public.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Wildwood(v)if v.roots.len()==1)));
                    assert_eq!(numeric_field_count(session.encode(e).unwrap().packet.state(),"registered_permanent_printing"),1);
                    act!(session,$input,|a|mana(a,forest));
                    act!(session,$input,|a|matches!(a,ActionSemanticV1::CastSpell{source:card,..}if card.arena_id==source.0));
                    let original_choice=if x!=0 {
                        let e=expected(&session.inner);let (_,actions,_)=session.inner.$input(e,e.acting_player).unwrap();
                        assert_eq!(actions.len(),usize::from(x)+1);
                        for (i,a) in actions.iter().enumerate() {assert!(matches!(a,ActionSemanticV1::ChooseEffectOption{source:card,option_index,option_count,..}if card.arena_id==source.0 && usize::from(*option_index)==i && *option_count==u16::from(x)+1));}
                        let pending=session.inner.$world().engine.pending_cast.as_ref().unwrap();assert_eq!(pending.x_value,None);assert_eq!(crate::engine::object_mana_value(session.inner.$world(),source),1);
                        let decision=session.encode(e).unwrap();assert_eq!(decision.packet.actions().len(),usize::from(x)+1);
                        Some((session.snapshot(),decision))
                    } else {assert!(session.inner.$world().engine.pending_cast.is_none());None};
                    let mut final_world=None;
                    for replay in 0..if x==0 {1}else{2} {
                        if let Some((snapshot,original))=&original_choice {
                            if replay!=0 {session.restore(snapshot).unwrap();let before=session.inner.privileged_core_environment_hash();assert!(matches!(session.apply(original,u32::from(x)),Err(Error::StaleBinding)));assert_eq!(session.inner.privileged_core_environment_hash(),before);assert_eq!(session.encode(expected(&session.inner)).unwrap().packet,original.packet);}
                            let e=expected(&session.inner);let decision=session.encode(e).unwrap();let mut erased=decision.clone();erased.packet.state=NumericTree::encode(&json!({})).unwrap();
                            let before=session.inner.privileged_core_environment_hash();assert!(matches!(session.apply(&erased,u32::from(x)),Err(Error::StaleBinding)));assert_eq!(session.inner.privileged_core_environment_hash(),before);assert!(matches!(session.apply(&decision,u32::from(x)+1),Err(Error::ActionRange)));assert_eq!(session.inner.privileged_core_environment_hash(),before);
                            let mut reference=session.inner.clone();let reply=reference.step(e.episode_id,e.step,u32::from(x)).unwrap();assert_eq!(session.apply(&decision,u32::from(x)).unwrap(),reply);assert_eq!(session.inner.$world(),reference.$world());
                        }
                        let paid=session.inner.$world();assert!(paid.engine.pending_cast.is_none());assert_eq!(paid.players[actor.index()].mana_pool,[0;6]);assert!(paid.objects.get(forest).tapped);
                        assert_eq!(paid.objects.get(source).zone,Zone::Stack);assert_eq!(paid.stack.len(),1);assert_eq!(paid.stack[0].v4.x_value,u16::from(x));
                        assert_eq!(crate::engine::object_mana_value(paid,source),u16::from(x)+1);
                        let e=expected(&session.inner);let (public,_,_)=session.inner.$input(e,e.acting_player).unwrap();assert_eq!(public.projection.surface.stack[0].x_value,u16::from(x));
                        let paid_snapshot=session.snapshot();let mut entry=None;
                        for repeat in 0..2 {
                            if repeat!=0 {session.restore(&paid_snapshot).unwrap();}
                            for _ in 0..16 {if session.inner.$world().stack.is_empty(){break;}act!(session,$input,|a|matches!(a,ActionSemanticV1::Pass{..}));}
                            let world=session.inner.$world();assert_eq!(world.objects.get(source).zone,if x==0 {Zone::Graveyard}else{Zone::Battlefield});
                            assert_eq!(crate::engine::object_mana_value(world,source),1);assert_eq!(world.objects.get(source).counters.plus1_plus1,i32::from(x));
                            assert_eq!(world.engine.event_history.iter().filter_map(|event|match event {crate::event::CommittedEvent::PlusOneCountersAdded{object,count,..}if *object==source=>Some(*count),_=>None}).sum::<i32>(),i32::from(x));
                            assert_eq!(world.engine.event_history.iter().filter(|event|matches!(event,crate::event::CommittedEvent::PlusOneCountersAdded{object,count,..}if *object==source && *count>0)).count(),usize::from(x!=0));
                            let result=(world.clone(),session.inner.policy_step_count(),session.inner.physical_decision_count());if let Some(previous)=&entry {assert_eq!(&result,previous);}else {entry=Some(result);}
                        }
                        if let Some(previous)=&final_world {assert_eq!(entry.as_ref().unwrap(),previous);}else {final_world=entry;}
                    }
                }}
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
    fn wildwood_variable_supplement_keeps_independent_hidden_hands_private_v50_v51_both_seats() {
        macro_rules! run {
            ($session:ident,$from:ident) => {
                for actor in [PlayerId::P0, PlayerId::P1] {
                    for known in [false, true] {
                        let mut raw = state(actor);
                        if known {
                            put(&mut raw, actor, "Wildwood Scourge", Zone::Hand);
                        }
                        let hidden =
                            put(&mut raw, actor.opponent(), "Wildwood Scourge", Zone::Hand);
                        let mut other = raw.clone();
                        other.objects.get_mut(hidden).card_def =
                            crate::card_def::card_id_by_name("Frenzied Goblin").unwrap();
                        other.objects.get_mut(hidden).name = "Frenzied Goblin".into();
                        let a = $session::from_session(FastActorSessionV1::$from(raw)).unwrap();
                        let b = $session::from_session(FastActorSessionV1::$from(other)).unwrap();
                        let pa = a.encode(expected(&a.inner)).unwrap();
                        let pb = b.encode(expected(&b.inner)).unwrap();
                        assert_eq!(pa.packet, pb.packet);
                        assert_eq!(
                            numeric_field_count(pa.packet.state(), "registered_permanent_printing"),
                            usize::from(known)
                        );
                    }
                }
            };
        }
        run!(FakeSession, from_fake_v50_fixture_state);
        run!(RavenousSession, from_ravenous_v51_fixture_state);
    }
}
