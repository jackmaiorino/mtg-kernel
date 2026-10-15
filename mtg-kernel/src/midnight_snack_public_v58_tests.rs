use super::*;
use crate::{
    card_def::{CardCapability, TargetSpec, CARD_DEFS},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    midnight_snack_v1::tests::{activate_paid, definition_fixture, drain, foods, mana, paid, walk},
    policy_observation_v6::tests::put,
    state::{Step, Target, Zone},
};
fn roundtrip(s: &GameState, viewer: PlayerId) -> FactsV58 {
    let facts = capture(s, viewer).unwrap();
    let value: FactsV58 = serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap();
    validate(s, viewer, &value).unwrap();
    let mut unknown = serde_json::to_value(&facts).unwrap();
    unknown.as_object_mut().unwrap().insert(
        "executable_program".into(),
        serde_json::json!({"forged":true}),
    );
    assert!(serde_json::from_value::<FactsV58>(unknown).is_err());
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(capture(&restored, viewer).unwrap(), facts);
    let json = serde_json::to_string(&facts).unwrap();
    for forbidden in [
        "event_history",
        "event_log",
        "resolving_item",
        "paid_cost_refs",
        "inline_effect",
        "source_contract",
        "history_index",
        "original_hand",
        "randomness",
        "original_world",
    ] {
        assert!(
            !json.contains(forbidden),
            "private executable field {forbidden}"
        );
    }
    facts
}
#[test]
fn exact_ordered_meaning_selector_characteristics_native_costs_food_and_tamper_refuse() {
    let id = crate::card_def::card_id_by_name("Midnight Snack").unwrap();
    let m = meaning::printed_meaning(id).unwrap();
    assert_eq!(id, 452);
    assert_eq!(m.capability, CardCapability::Partial);
    assert!(!m.model_admitted);
    assert_eq!(m.printed.mana.generic, 2);
    assert_eq!(m.printed.mana.colored_pips, [crate::mana::ManaColor::B]);
    assert_eq!(m.printed.types, [crate::card_def::CardType::Enchantment]);
    assert_eq!((m.printed.power, m.printed.toughness), (None, None));
    assert_eq!(m.ordered_rules.len(), 3);
    let meaning::RuleV58::PrintedActivation { operation, .. } = &m.ordered_rules[2] else {
        panic!()
    };
    assert_eq!(
        operation.ordered_costs,
        [
            meaning::CostV58::PayNativeMana,
            meaning::CostV58::SacrificeOriginalSource
        ]
    );
    assert_eq!(
        operation.targets,
        meaning::TargetV58::ExactlyOriginalControllerOpponent
    );
    assert_eq!(m.food.printed.subtypes, [crate::card_def::Subtype::Food]);
    assert_eq!(
        m.food.activation.ordered_costs,
        [
            meaning::CostV58::PayNativeMana,
            meaning::CostV58::TapOriginalSource,
            meaning::CostV58::SacrificeOriginalSource
        ]
    );
    let restored = serde_json::from_slice(&serde_json::to_vec(&m).unwrap()).unwrap();
    meaning::validate(&restored).unwrap();
    for case in 0..6 {
        let mut bad = m.clone();
        match case {
            0 => bad.ordered_rules.reverse(),
            1 => bad.root_sha256.clear(),
            2 => bad.printed.mana.generic = 0,
            3 => bad.food.activation.ordered_costs.reverse(),
            4 => bad.selector.producer_sha256.clear(),
            _ => bad.model_admitted = true,
        };
        assert!(meaning::validate(&bad).is_err());
    }
    let d = &CARD_DEFS[id as usize];
    for case in 0..4 {
        let mut bad = definition_fixture(d);
        match case {
            0 => bad.target_spec = TargetSpec::Creature,
            1 => bad.flashback = Some(crate::card_def::FlashbackDef { cost: &[] }),
            2 => bad.activated_abilities = &[],
            _ => {
                bad.controlled_creature_grant = Some(crate::controlled_creature_grants_v1::MEMORIAL)
            }
        };
        assert!(!meaning::selected_definition(id, &bad));
    }
}
#[test]
fn real_paid_cast_pending_and_finalized_stack_facts_follow_native_payment_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = crate::extra_turn_v1::tests::ready(actor);
        let source = put(&mut s, actor, "Midnight Snack", Zone::Hand);
        assert_eq!(roundtrip(&s, actor).authorized_printings.len(), 1);
        assert!(roundtrip(&s, actor.opponent())
            .authorized_printings
            .is_empty());
        mana(&mut s, actor, "Swamp", 3);
        engine::step(&mut s, Action::CastSpell(source)).unwrap();
        assert!(s.engine.pending_cast.is_some());
        let pending = roundtrip(&s, actor.opponent());
        assert_eq!(pending.casts.len(), 1);
        assert!(pending.casts[0].payment_pending);
        assert!(
            matches!(engine::advance_until_decision(&mut s),Decision::CastSpellOrPass{player,..}if player==actor)
        );
        assert!(s.engine.pending_cast.is_none());
        assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);
        let finalized = roundtrip(&s, actor);
        assert_eq!(finalized.casts.len(), 1);
        assert!(!finalized.casts[0].payment_pending);
        drain(&mut s);
        assert_eq!(roundtrip(&s, actor).current.len(), 1);
    }
}
#[test]
fn paid_raid_food_response_and_sacrificed_original_activation_dynamic_amount_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        // Accepted attacker setup primitive, then genuine native declaration,
        // unblocked lifelink, natural own end step and printed Raid publication.
        let hawk = put(&mut s, actor, "Healer's Hawk", Zone::Battlefield);
        s.objects.get_mut(hawk).summoning_sick = false;
        walk(&mut s, actor, Step::End, Some(hawk));
        let published = roundtrip(&s, actor);
        assert_eq!(published.published_raid.len(), 1);
        assert!(published.attacked_this_turn[actor.index()]);
        let original = s.objects.get(source).zone_change_count;
        drain(&mut s);
        assert_eq!(foods(&s, actor).len(), 1);
        assert_eq!(roundtrip(&s, actor).gained_this_turn[actor.index()], 1);
        mana(&mut s, actor, "Swamp", 3);
        engine::step(&mut s, Action::ActivateAbility(source, 0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut s),
            Decision::ChooseTargets { .. }
        ));
        let prompt = roundtrip(&s, actor.opponent());
        assert_eq!(
            prompt.activations[0].stage,
            ActivationStageV58::ChoosingOpponentBeforeCosts
        );
        let bytes = serde_json::to_vec(&s).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut s, &mut restored] {
            engine::step(game, Action::ChooseTarget(Target::Player(actor.opponent()))).unwrap();
            engine::advance_until_decision(game);
            assert!(game.engine.pending_activation.is_none());
            assert_eq!(game.players[actor.index()].mana_pool, [0; 6]);
            assert_eq!(game.objects.get(source).zone, Zone::Graveyard);
            let paid = roundtrip(game, actor.opponent());
            let a = &paid.activations[0];
            assert_eq!(a.stage, ActivationStageV58::PaidAndSacrificedOnStack);
            assert_eq!(a.root.original_source.generation, original);
            assert_eq!(a.root.captured_controller, actor.into());
            assert!(a.root.current_original_source.is_none());
            assert_eq!(a.original_controller_gained_this_turn_now, 1);
            let food = foods(game, actor)[0];
            mana(game, actor, "Swamp", 2);
            engine::step(game, Action::ActivateAbility(food, 0)).unwrap();
            engine::advance_until_decision(game);
            // Snack stays below the genuine Food response, retaining its original
            // actor and a dynamic total rather than freezing the one-life input.
            assert_eq!(
                roundtrip(game, actor).activations[0].original_controller_gained_this_turn_now,
                1
            );
            engine::step(game, Action::Pass).unwrap();
            engine::advance_until_decision(game);
            engine::step(game, Action::Pass).unwrap();
            engine::advance_until_decision(game);
            let after_food = roundtrip(game, actor);
            assert_eq!(after_food.gained_this_turn[actor.index()], 4);
            assert_eq!(after_food.activations.len(), 1);
            assert_eq!(
                after_food.activations[0].original_controller_gained_this_turn_now,
                4
            );
            drain(game);
            assert!(roundtrip(game, actor).activations.is_empty());
            assert_eq!(game.players[actor.opponent().index()].life, 15);
        }
        assert_eq!(s, restored);
    }
}
#[test]
fn source_control_copy_and_reentry_do_not_rebind_original_paid_actor_or_reveal_hidden_new_incarnation(
) {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(owner);
        let actor = owner.opponent();
        let copy = crate::standard_cards_v1::create_token_copy(&mut s, source, actor).unwrap();
        crate::standard_cards_v1::gain_control(&mut s, source, actor);
        event::propose_and_commit(&mut s, ProposedEvent::life_gain(actor, 2));
        activate_paid(&mut s, actor, source);
        let original = roundtrip(&s, actor).activations[0].clone();
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(source, Zone::Hand));
        // Returned physical card is new and private to its owner. The public
        // ability retains only its original public source, with no new handle.
        let current = roundtrip(&s, actor);
        assert_eq!(current.activations[0].root, original.root);
        assert!(current.activations[0]
            .root
            .current_original_source
            .is_none());
        assert!(!current
            .authorized_printings
            .iter()
            .any(|r| r.object == source.0));
        assert!(current
            .current
            .iter()
            .any(|r| r.source.object == copy.0 && r.is_token));
        drain(&mut s);
        assert_eq!(s.players[owner.index()].life, 18);
    }
}
#[test]
fn independent_blind_hand_face_down_worlds_are_identical_and_current_counters_remain_full_width() {
    for viewer in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let build = |secret: &str| {
                let mut s = crate::extra_turn_v1::tests::ready(viewer);
                if known {
                    put(&mut s, viewer, "Midnight Snack", Zone::Hand);
                }
                put(&mut s, viewer.opponent(), secret, Zone::Hand);
                let hidden = put(&mut s, viewer.opponent(), secret, Zone::Battlefield);
                s.objects.get_mut(hidden).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                    disguised: true,
                    lookers: 0,
                    hidden_by: None,
                    hidden_by_source: None,
                });
                s
            };
            let a = build("Midnight Snack");
            let b = build("Llanowar Elves");
            let before_a = serde_json::to_vec(&a).unwrap();
            let before_b = serde_json::to_vec(&b).unwrap();
            assert_eq!(roundtrip(&a, viewer), roundtrip(&b, viewer));
            assert_eq!(serde_json::to_vec(&a).unwrap(), before_a);
            assert_eq!(serde_json::to_vec(&b).unwrap(), before_b);
        }
        let (mut s, source) = paid(viewer);
        s.objects.get_mut(source).counters.charge = i32::MAX;
        assert_eq!(roundtrip(&s, viewer).current[0].counters.charge, i32::MAX);
        let facts = roundtrip(&s, viewer);
        let mut forged = facts.clone();
        forged.gained_this_turn[viewer.index()] += 1;
        let before = serde_json::to_vec(&s).unwrap();
        assert!(validate(&s, viewer, &forged).is_err());
        assert_eq!(serde_json::to_vec(&s).unwrap(), before);
        let mut erased = s.clone();
        erased
            .engine
            .event_history
            .retain(|e| !matches!(e, event::CommittedEvent::LifeLossTurnBeganV1 { .. }));
        let before = serde_json::to_vec(&erased).unwrap();
        assert!(capture(&erased, viewer).is_err());
        assert_eq!(serde_json::to_vec(&erased).unwrap(), before);
    }
}

#[test]
fn genuine_two_paid_sources_held_apnap_order_and_no_raid_without_attack_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut no_attack, _) = paid(actor);
        walk(&mut no_attack, actor, Step::End, None);
        let no_raid = roundtrip(&no_attack, actor);
        assert!(no_raid.published_raid.is_empty());
        assert!(no_raid.held_raid.is_empty());
        assert!(!no_raid.attacked_this_turn[actor.index()]);
        let (mut s, source) = paid(actor);
        let other = put(&mut s, actor, "Midnight Snack", Zone::Hand);
        mana(&mut s, actor, "Swamp", 3);
        engine::step(&mut s, Action::CastSpell(other)).unwrap();
        engine::advance_until_decision(&mut s);
        assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);
        drain(&mut s);
        let hawk = put(&mut s, actor, "Healer's Hawk", Zone::Battlefield);
        s.objects.get_mut(hawk).summoning_sick = false;
        let mut held = false;
        for _ in 0..160 {
            let d = engine::advance_until_decision(&mut s);
            let action = match d {
                Decision::OrderTriggers { player, pending } if s.step == Step::End => {
                    assert_eq!(player, actor);
                    assert_eq!(pending.len(), 2);
                    let facts = roundtrip(&s, actor.opponent());
                    assert_eq!(facts.held_raid.len(), 2);
                    assert!(facts.published_raid.is_empty());
                    assert!(facts
                        .held_raid
                        .iter()
                        .all(|r| r.root.captured_controller == actor.into()));
                    assert!(facts
                        .held_raid
                        .iter()
                        .any(|r| r.root.original_source.object == source.0));
                    assert!(facts
                        .held_raid
                        .iter()
                        .any(|r| r.root.original_source.object == other.0));
                    let bytes = serde_json::to_vec(&s).unwrap();
                    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
                    for world in [&mut s, &mut restored] {
                        engine::step(world, Action::OrderTriggers(vec![1, 0])).unwrap();
                        engine::advance_until_decision(world);
                        assert_eq!(roundtrip(world, actor).published_raid.len(), 2);
                        drain(world);
                        assert_eq!(foods(world, actor).len(), 2);
                    }
                    assert_eq!(s, restored);
                    held = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } => Action::Pass,
                Decision::DeclareAttackers { player, .. } => {
                    Action::DeclareAttackers(if player == actor { vec![hawk] } else { vec![] })
                }
                Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
                Decision::OrderTriggers { pending, .. } => {
                    Action::OrderTriggers((0..pending.len()).collect())
                }
                other => panic!("actual Snack held window {other:?}"),
            };
            engine::step(&mut s, action).unwrap();
        }
        assert!(held);
    }
}

#[test]
fn paid_witness_effective_suppression_uses_public_form_and_actual_gain_totals_reset_without_loss_subtraction(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        s.objects.get_mut(source).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        assert!(roundtrip(&s, actor).current.is_empty());
        assert!(roundtrip(&s, actor.opponent()).current.is_empty());
        // Same accepted concealed-creature setup as the engine fixture. The
        // Witness spell/target/payment is real; no Aura relation is injected.
        let witness = put(&mut s, actor, "Witness Protection", Zone::Hand);
        mana(&mut s, actor, "Island", 1);
        engine::step(&mut s, Action::CastSpell(witness)).unwrap();
        engine::advance_until_decision(&mut s);
        engine::step(&mut s, Action::ChooseTarget(Target::Object(source))).unwrap();
        drain(&mut s);
        s.objects.get_mut(source).v4.face_down_v1 = None;
        let suppressed = roundtrip(&s, actor.opponent());
        assert_eq!(suppressed.current.len(), 1);
        assert!(!suppressed.current[0].form.printed_abilities_active);
        assert!(suppressed.current[0].form.abilities_removed);
        assert!(suppressed.current[0]
            .form
            .types
            .contains(&crate::card_def::CardType::Creature));
        assert_eq!(suppressed.current[0].form.power, Some(1));
        assert!(suppressed.published_raid.is_empty());
        let (mut s, source) = paid(actor);
        // Explicit committed inputs isolate the current public total from a net
        // life delta. The actual paid activation below remains native.
        for e in [
            ProposedEvent::life_gain(actor, 2),
            ProposedEvent::life_loss(actor, 1),
            ProposedEvent::life_gain(actor.opponent(), 5),
            ProposedEvent::life_gain(actor, 0),
        ] {
            event::propose_and_commit(&mut s, e);
        }
        assert_eq!(roundtrip(&s, actor).gained_this_turn[actor.index()], 2);
        activate_paid(&mut s, actor, source);
        assert_eq!(
            roundtrip(&s, actor).activations[0].original_controller_gained_this_turn_now,
            2
        );
        drain(&mut s);
        assert_eq!(s.players[actor.opponent().index()].life, 23);
        walk(&mut s, actor.opponent(), Step::Main1, None);
        assert_eq!(roundtrip(&s, actor).gained_this_turn, [0; 2]);
    }
}
