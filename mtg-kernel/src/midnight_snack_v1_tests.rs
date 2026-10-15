//! Real paid printing paths; explicitly labelled support primitives below.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{Step, Target},
};

fn priority(s: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        let d = engine::advance_until_decision(s);
        if matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(s, Action::Pass).unwrap();
    }
    panic!("no priority");
}
fn drain(s: &mut GameState) {
    for _ in 0..40 {
        let d = engine::advance_until_decision(s);
        match d {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(s, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { .. }
                if s.stack.is_empty() && s.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(s, Action::Pass).unwrap(),
            other => panic!("Snack stack {other:?}"),
        }
    }
    panic!("stack did not drain");
}
fn mana(s: &mut GameState, actor: PlayerId, name: &str, n: usize) {
    for _ in 0..n {
        let land = put(s, actor, name, Zone::Battlefield);
        priority(s, actor);
        engine::step(s, Action::ActivateManaAbility(land)).unwrap();
        assert!(s.objects.get(land).tapped);
    }
}
fn paid(actor: PlayerId) -> (GameState, ObjectId) {
    let mut s = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut s, actor, "Midnight Snack", Zone::Hand);
    assert_eq!(s.objects.get(source).card_def, 452);
    assert!(definition(452));
    assert!(crate::card_def::preflight_fully_supported_deck(&[452]).is_err());
    mana(&mut s, actor, "Swamp", 3);
    engine::step(&mut s, Action::CastSpell(source)).unwrap();
    engine::advance_until_decision(&mut s);
    assert!(s
        .stack
        .iter()
        .any(|i| i.source == source && i.kind == StackItemKind::Spell));
    assert_eq!(
        s.players[actor.index()]
            .mana_pool
            .iter()
            .map(|n| u32::from(*n))
            .sum::<u32>(),
        0
    );
    drain(&mut s);
    assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
    (s, source)
}
fn activate_paid(s: &mut GameState, actor: PlayerId, source: ObjectId) {
    mana(s, actor, "Swamp", 3);
    engine::step(s, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(s),
        Decision::ChooseTargets { .. }
    ));
    engine::step(s, Action::ChooseTarget(Target::Player(actor.opponent()))).unwrap();
    engine::advance_until_decision(s);
    assert_ne!(s.objects.get(source).zone, Zone::Battlefield);
    assert!(s
        .stack
        .iter()
        .any(|i| i.source == source && i.kind == StackItemKind::ActivatedAbility));
    assert_eq!(
        s.players[actor.index()]
            .mana_pool
            .iter()
            .map(|n| u32::from(*n))
            .sum::<u32>(),
        0
    );
}
fn walk(s: &mut GameState, actor: PlayerId, step: Step, attacker: Option<ObjectId>) {
    for _ in 0..160 {
        let d = engine::advance_until_decision(s);
        if s.active_player == actor
            && s.step == step
            && matches!(d, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        let a = match d {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { player, .. } => {
                Action::DeclareAttackers(if player == actor {
                    attacker.into_iter().collect()
                } else {
                    vec![]
                })
            }
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => panic!("turn walker {other:?}"),
        };
        engine::step(s, a).unwrap();
    }
    panic!("turn walker exhausted");
}
fn foods(s: &GameState, actor: PlayerId) -> Vec<ObjectId> {
    s.players[actor.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| CARD_DEFS[usize::from(s.objects.get(*id).card_def)].name == "Food Token")
        .collect()
}
#[test]
fn paid_raid_food_and_response_gain_use_final_resolution_total_with_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        // Supporting attacker primitive. Raid itself is a real legal declaration,
        // unblocked combat/lifelink, natural end-step event and printed trigger.
        let hawk = put(&mut s, actor, "Healer's Hawk", Zone::Battlefield);
        s.objects.get_mut(hawk).summoning_sick = false;
        walk(&mut s, actor, Step::End, Some(hawk));
        drain(&mut s);
        assert!(s.player_attacked_this_turn_v1(actor));
        assert_eq!(gained_this_turn(&s, actor).unwrap(), 1);
        let food = foods(&s, actor);
        assert_eq!(food.len(), 1);
        activate_paid(&mut s, actor, source);
        // Pay a genuine Food activation above the pending paid Snack ability.
        mana(&mut s, actor, "Swamp", 2);
        engine::step(&mut s, Action::ActivateAbility(food[0], 0)).unwrap();
        engine::advance_until_decision(&mut s);
        assert_ne!(s.objects.get(food[0]).zone, Zone::Battlefield);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        for game in [&mut s, &mut restored] {
            drain(game);
            assert_eq!(gained_this_turn(game, actor).unwrap(), 4);
            assert_eq!(game.players[actor.opponent().index()].life, 15);
            assert_eq!(game.players[actor.index()].life, 24);
            assert!(game.engine.halted.is_none());
            assert!(game.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::LifeLoss{player,amount:4} if *player==actor.opponent())));
        }
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        walk(&mut s, actor.opponent(), Step::Main1, None);
        assert_eq!(gained_this_turn(&s, actor).unwrap(), 0);
        assert!(!s.player_attacked_this_turn_v1(actor));
    }
}

#[test]
fn paid_zero_and_multiple_positive_gains_ignore_losses_opponent_and_stale_turn_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for gains in [false, true] {
            let (mut s, source) = paid(actor);
            // Committed-event input primitive verifies the dynamic aggregation
            // independently of the paid Food/lifelink integration above.
            if gains {
                for e in [
                    ProposedEvent::life_gain(actor, 2),
                    ProposedEvent::life_gain(actor, 5),
                    ProposedEvent::life_loss(actor, 3),
                    ProposedEvent::life_gain(actor.opponent(), 9),
                    ProposedEvent::life_gain(actor, 0),
                ] {
                    event::propose_and_commit(&mut s, e);
                }
            }
            activate_paid(&mut s, actor, source);
            let mut altered = s.clone();
            altered.life_loss_turn_v1.as_mut().unwrap().turn += 1;
            assert!(gained_this_turn(&altered, actor).is_err());
            let before = altered.players[actor.opponent().index()].life;
            engine::step(&mut altered, Action::Pass).ok();
            engine::step(&mut altered, Action::Pass).ok();
            assert_eq!(altered.players[actor.opponent().index()].life, before);
            drain(&mut s);
            assert_eq!(
                s.players[actor.opponent().index()].life,
                if gains { 22 } else { 20 }
            );
            assert_eq!(
                gained_this_turn(&s, actor).unwrap(),
                if gains { 7 } else { 0 }
            );
        }
    }
}

#[test]
fn raid_requires_attack_and_rechecks_original_controller_after_source_leaves_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        walk(&mut s, actor, Step::End, None);
        drain(&mut s);
        assert!(foods(&s, actor).is_empty());
        walk(&mut s, actor, Step::Main1, None);
        let hawk = put(&mut s, actor, "Healer's Hawk", Zone::Battlefield);
        s.objects.get_mut(hawk).summoning_sick = false;
        walk(&mut s, actor, Step::End, Some(hawk));
        assert!(s
            .stack
            .iter()
            .any(|i| i.source == source && i.kind == StackItemKind::TriggeredAbility));
        // Sacrificing the genuine enchantment while its printed trigger waits
        // cannot erase the original controller's Raid history.
        activate_paid(&mut s, actor, source);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        for game in [&mut s, &mut restored] {
            drain(game);
            assert_eq!(foods(game, actor).len(), 1);
        }
        assert_eq!(s.state_hash(), restored.state_hash());
    }
}

// An explicit complete definition fixture keeps foreign-field selector tests
// independent of any production Clone implementation. New fields must be
// supplied here rather than silently inherited by struct update syntax.
fn definition_fixture(d: &crate::card_def::CardDef) -> crate::card_def::CardDef {
    crate::card_def::CardDef {
        name: d.name,
        capability: d.capability,
        cost: d.cost,
        generic_cost_reduction: d.generic_cost_reduction,
        ward_cost: d.ward_cost,
        spell_cannot_be_countered: d.spell_cannot_be_countered,
        records_mana_spent: d.records_mana_spent,
        equipment: d.equipment,
        types: d.types,
        subtypes: d.subtypes,
        supertypes: d.supertypes,
        power: d.power,
        toughness: d.toughness,
        starting_loyalty: d.starting_loyalty,
        enters_with_plus_one_counters: d.enters_with_plus_one_counters,
        controlled_counter_keyword: d.controlled_counter_keyword,
        game_outcome_protection_v1: d.game_outcome_protection_v1,
        controlled_creature_grant: d.controlled_creature_grant,
        is_land: d.is_land,
        produces_mana: d.produces_mana,
        colors: d.colors,
        target_spec: d.target_spec,
        keywords: d.keywords,
        spell_effect: d.spell_effect,
        mana_ability: d.mana_ability,
        alt_cost: d.alt_cost,
        kicker_cost: d.kicker_cost,
        additional_cost: d.additional_cost,
        flashback: d
            .flashback
            .as_ref()
            .map(|v| crate::card_def::FlashbackDef { cost: v.cost }),
        activated_abilities: d.activated_abilities,
        plot_cost: d.plot_cost,
        madness_cost: d.madness_cost,
        mode2: d.mode2.as_ref().map(|v| crate::card_def::ModeDef {
            target_spec: v.target_spec,
            effect: v.effect,
        }),
        mode3: d.mode3.as_ref().map(|v| crate::card_def::ModeDef {
            target_spec: v.target_spec,
            effect: v.effect,
        }),
        additional_modes: d.additional_modes,
        kicked_target_spec: d.kicked_target_spec,
        is_token: d.is_token,
        escape: d
            .escape
            .as_ref()
            .map(|v| crate::card_def::EscapeDef { cost: v.cost }),
        mana_ability_choices: d.mana_ability_choices,
        enters_battlefield_tapped: d.enters_battlefield_tapped,
        mana_ability_def: d.mana_ability_def,
        minimum_blockers: d.minimum_blockers,
        omen: d.omen.as_ref().map(|v| crate::card_def::OmenDef {
            cost: v.cost,
            types: v.types,
            target_spec: v.target_spec,
            effect: v.effect,
        }),
        mana_value: d.mana_value,
        mana_ability_includes_chosen_color: d.mana_ability_includes_chosen_color,
        as_enters_choose_color_other_than: d.as_enters_choose_color_other_than,
        additional_mana_abilities: d.additional_mana_abilities,
        object_name: d.object_name,
        enters_battlefield_tapped_unless: d.enters_battlefield_tapped_unless,
        attachment: d.attachment,
        transform_face: d
            .transform_face
            .as_ref()
            .map(|v| crate::card_def::TransformFaceDef {
                name: v.name,
                types: v.types,
                subtypes: v.subtypes,
                colors: v.colors,
                power: v.power,
                toughness: v.toughness,
                keywords: v.keywords,
            }),
        saga: d.saga.as_ref().map(|v| crate::card_def::SagaDef {
            chapter_effects: v.chapter_effects,
        }),
        optional_additional_cost: d.optional_additional_cost,
        changeling: d.changeling,
        bestow: d.bestow.as_ref().map(|v| crate::card_def::BestowDef {
            cost: v.cost,
            target_spec: v.target_spec,
        }),
        delve: d.delve,
        adventure: d.adventure.as_ref().map(|v| crate::card_def::AdventureDef {
            name: v.name,
            cost: v.cost,
            types: v.types,
            target_spec: v.target_spec,
            effect: v.effect,
        }),
        cant_be_blocked_by_monarchs_creatures: d.cant_be_blocked_by_monarchs_creatures,
        conditional_tap_yield: d.conditional_tap_yield,
        enters_tapped_unless_controller: d.enters_tapped_unless_controller,
        additional_mana_ability_conditions: d.additional_mana_ability_conditions,
        restricted_mana_abilities: d.restricted_mana_abilities,
        animation: d.animation,
        activated_ability_generic_reductions: d.activated_ability_generic_reductions,
        combat_static_rules: d.combat_static_rules,
        as_enters_color_choice_v1: d.as_enters_color_choice_v1,
        chosen_color_creature_boost_v1: d.chosen_color_creature_boost_v1,
        damage_replacement_v1: d.damage_replacement_v1,
        token_counter_replacement_v48: d.token_counter_replacement_v48,
        starting_life_self_bonuses_v1: d.starting_life_self_bonuses_v1,
        source_type_hexproof_v1: d.source_type_hexproof_v1,
        other_controlled_nontoken_death_v1: d.other_controlled_nontoken_death_v1,
    }
}

#[test]
fn exact_selector_and_unowned_execution_refuse_without_life_mutation() {
    let (mut s, source) = paid(PlayerId::P0);
    let d = &CARD_DEFS[452];
    let mut full = definition_fixture(d);
    full.capability = CardCapability::Full;
    assert!(exact_definition(&full));
    let mut changed = definition_fixture(d);
    changed.flashback = Some(crate::card_def::FlashbackDef { cost: &[] });
    assert!(!exact_definition(&changed));
    let before = s.players[1].life;
    crate::effect::execute(
        &activation(),
        &ExecCtx::no_targets(source, PlayerId::P0),
        &mut s,
    );
    assert!(s.engine.halted.is_some());
    assert_eq!(s.players[1].life, before);
}

#[test]
fn genuine_activation_requires_payment_and_original_opponent_target_without_mutation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        let before = serde_json::to_vec(&s).unwrap();
        assert!(engine::step(&mut s, Action::ActivateAbility(source, 0)).is_err());
        assert_eq!(serde_json::to_vec(&s).unwrap(), before);
        mana(&mut s, actor, "Swamp", 3);
        engine::step(&mut s, Action::ActivateAbility(source, 0)).unwrap();
        engine::advance_until_decision(&mut s);
        let before = serde_json::to_vec(&s).unwrap();
        assert!(engine::step(&mut s, Action::ChooseTarget(Target::Player(actor))).is_err());
        assert_eq!(serde_json::to_vec(&s).unwrap(), before);
        engine::step(
            &mut s,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        engine::advance_until_decision(&mut s);
        let original = s.clone();
        // Preserve the actual event journals while corrupting only the pending
        // original controller. Generic source/row validation must refuse it.
        s.stack.last_mut().unwrap().controller = actor.opponent();
        assert!(engine::validated_stack_item_target_spec(s.stack.last().unwrap(), &s).is_err());
        let source_life = s.players[actor.index()].life;
        engine::step(&mut s, Action::Pass).ok();
        engine::step(&mut s, Action::Pass).ok();
        assert_eq!(s.players[actor.index()].life, source_life);
        let mut authentic = original;
        drain(&mut authentic);
        assert_eq!(authentic.players[actor.opponent().index()].life, 20);
    }
}

#[test]
fn committed_gain_sum_overflow_refuses_before_resolved_loss_with_original_history() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        // Every individual life total and committed event remains representable;
        // the turn's aggregate alone exceeds the dynamic amount's i32 domain.
        for n in 0..3 {
            event::propose_and_commit(&mut s, ProposedEvent::life_gain(actor, 1_000_000_000));
            if n < 2 {
                event::propose_and_commit(&mut s, ProposedEvent::life_loss(actor, 1_000_000_000));
            }
        }
        assert!(gained_this_turn(&s, actor).is_err());
        activate_paid(&mut s, actor, source);
        let item = s.stack.last().unwrap();
        assert!(acquire(&s, item, true).is_err());
        let before = s.players[actor.opponent().index()].life;
        let journal = s.engine.event_history.clone();
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(acquire(&restored, restored.stack.last().unwrap(), true).is_err());
        let original = s.stack.last().unwrap().clone();
        for game in [&mut s, &mut restored] {
            for _ in 0..4 {
                let decision = engine::advance_until_decision(game);
                if game.engine.halted.is_some() {
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(game, Action::Pass).unwrap();
            }
            assert!(game.engine.halted.is_some());
            assert_eq!(game.stack.last(), Some(&original));
            assert_eq!(game.players[actor.opponent().index()].life, before);
            assert_eq!(game.engine.event_history, journal);
        }
    }
}

#[test]
fn paid_crystal_response_fizzles_without_evaluating_overflowed_turn_total_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        let opponent = actor.opponent();
        // Pay the genuine flash creature first. It grants the actual timing
        // permission needed to cast Crystal in response to the Snack ability.
        let trickster = put(&mut s, opponent, "High Fae Trickster", Zone::Hand);
        mana(&mut s, opponent, "Island", 4);
        engine::step(&mut s, Action::CastSpell(trickster)).unwrap();
        engine::advance_until_decision(&mut s);
        drain(&mut s);
        assert_eq!(s.objects.get(trickster).zone, Zone::Battlefield);
        // Legitimate committed gain/loss input primitive: individual amounts
        // and life totals fit while their positive turn aggregate exceeds i32.
        for n in 0..3 {
            event::propose_and_commit(&mut s, ProposedEvent::life_gain(actor, 1_000_000_000));
            if n < 2 {
                event::propose_and_commit(&mut s, ProposedEvent::life_loss(actor, 1_000_000_000));
            }
        }
        activate_paid(&mut s, actor, source);
        let original_ability = s.stack.last().unwrap().v4.stack_item_id;
        let mut countered = s.clone();
        let mut countered_restore: GameState =
            serde_json::from_slice(&serde_json::to_vec(&countered).unwrap()).unwrap();
        // Existing generic counter operation on the actual finalized paid row;
        // no fictitious counterspell, activation or removal loan is supplied.
        for game in [&mut countered, &mut countered_restore] {
            assert!(engine::counter_stack_item_by_id(game, original_ability)
                .unwrap()
                .is_some());
            drain(game);
            assert!(game.engine.halted.is_none());
            assert!(gained_this_turn(game, actor).is_err());
            assert_eq!(game.players[opponent.index()].life, 20);
        }
        assert_eq!(countered.state_hash(), countered_restore.state_hash());
        let crystal = put(&mut s, opponent, "Crystal Barricade", Zone::Hand);
        mana(&mut s, opponent, "Plains", 2);
        engine::step(&mut s, Action::CastSpell(crystal)).unwrap();
        engine::advance_until_decision(&mut s);
        assert_eq!(s.objects.get(crystal).zone, Zone::Stack);
        assert!(s
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == original_ability));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        for game in [&mut s, &mut restored] {
            drain(game);
            assert_eq!(game.objects.get(crystal).zone, Zone::Battlefield);
            assert!(crate::crystal_barricade_v1::controller_has_hexproof(
                game, opponent
            ));
            assert!(gained_this_turn(game, actor).is_err());
            assert!(game.engine.halted.is_none());
            assert!(game.stack.is_empty());
            assert_eq!(game.players[opponent.index()].life, 20);
            assert!(!game
                .engine
                .event_history
                .iter()
                .any(|e| matches!(e,CommittedEvent::LifeLoss{player,..} if *player==opponent)));
        }
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn paid_herald_negative_life_boundary_preflights_before_snack_removal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for underflow in [false, true] {
            let (mut s, source) = paid(actor);
            let target = actor.opponent();
            let herald = put(&mut s, target, "Herald of Eternal Dawn", Zone::Hand);
            assert_eq!(s.objects.get(herald).card_def, 432);
            mana(&mut s, target, "Plains", 7);
            engine::step(&mut s, Action::CastSpell(herald)).unwrap();
            engine::advance_until_decision(&mut s);
            drain(&mut s);
            assert_eq!(s.objects.get(herald).zone, Zone::Battlefield);
            assert!(crate::game_outcome_protection_v1::cannot_lose(&s, target));
            // Genuine committed life input primitive. Herald protects the
            // negative life state; neither subtraction overflows here.
            event::propose_and_commit(&mut s, ProposedEvent::life_loss(target, i32::MAX));
            event::propose_and_commit(
                &mut s,
                ProposedEvent::life_loss(target, if underflow { 18 } else { 17 }),
            );
            event::propose_and_commit(&mut s, ProposedEvent::life_gain(actor, 4));
            let target_life = i32::MIN + if underflow { 3 } else { 4 };
            assert_eq!(s.players[target.index()].life, target_life);
            assert_eq!(gained_this_turn(&s, actor).unwrap(), 4);
            activate_paid(&mut s, actor, source);
            let stack = s.stack.clone();
            let history = s.engine.event_history.clone();
            let public_log = s.engine.event_log.clone();
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
            for game in [&mut s, &mut restored] {
                if underflow {
                    for _ in 0..4 {
                        let decision = engine::advance_until_decision(game);
                        if game.engine.halted.is_some() {
                            break;
                        }
                        assert!(
                            matches!(decision, Decision::CastSpellOrPass { .. }),
                            "{decision:?}"
                        );
                        engine::step(game, Action::Pass).unwrap();
                    }
                    assert!(game.engine.halted.is_some());
                    assert_eq!(game.stack, stack);
                    assert_eq!(game.players[target.index()].life, target_life);
                    assert_eq!(game.engine.event_history, history);
                    assert_eq!(game.engine.event_log, public_log);
                } else {
                    drain(game);
                    assert!(game.engine.halted.is_none());
                    assert!(game.stack.is_empty());
                    assert_eq!(game.players[target.index()].life, i32::MIN);
                    assert!(game.engine.event_history.iter().skip(history.len()).any(
                        |e| matches!(e,CommittedEvent::LifeLoss{player,amount:4} if *player==target)
                    ));
                    assert!(!game.players[target.index()].has_lost);
                }
                assert!(crate::game_outcome_protection_v1::cannot_lose(game, target));
            }
            assert_eq!(
                serde_json::to_vec(&s).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn genuine_copy_and_control_primitives_keep_paid_controller_and_new_incarnation_separate() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(owner);
        let actor = owner.opponent();
        // Existing generic 707.2 copy/control primitives supply this input.
        // No invented copying spell or control-change printing is registered.
        let copy = crate::standard_cards_v1::create_token_copy(&mut s, source, actor).unwrap();
        assert!(s.objects.get(copy).v4.is_token);
        assert!(definition(s.objects.get(copy).card_def));
        crate::standard_cards_v1::gain_control(&mut s, source, actor);
        for e in [
            ProposedEvent::life_gain(owner, 9),
            ProposedEvent::life_gain(actor, 2),
        ] {
            event::propose_and_commit(&mut s, e);
        }
        activate_paid(&mut s, actor, source);
        // Return/change control of the paid source while its old ability waits.
        event::propose_and_commit(
            &mut s,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        crate::standard_cards_v1::gain_control(&mut s, source, owner);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        for game in [&mut s, &mut restored] {
            drain(game);
            assert_eq!(game.players[owner.index()].life, 27);
            activate_paid(game, actor, copy);
            drain(game);
            assert_eq!(game.players[owner.index()].life, 25);
            assert!(game.engine.halted.is_none());
        }
        assert_eq!(s.state_hash(), restored.state_hash());
    }
}

#[test]
fn face_down_and_paid_witness_suppression_remove_both_printed_branches() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        // A concealed creature form is an explicit generic fixture input.
        // Witness is then genuinely cast/paid against that legal creature.
        s.objects.get_mut(source).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        mana(&mut s, actor, "Swamp", 3);
        let before = serde_json::to_vec(&s).unwrap();
        assert!(engine::step(&mut s, Action::ActivateAbility(source, 0)).is_err());
        assert_eq!(serde_json::to_vec(&s).unwrap(), before);
        let witness = put(&mut s, actor, "Witness Protection", Zone::Hand);
        mana(&mut s, actor, "Island", 1);
        engine::step(&mut s, Action::CastSpell(witness)).unwrap();
        engine::advance_until_decision(&mut s);
        engine::step(&mut s, Action::ChooseTarget(Target::Object(source))).unwrap();
        drain(&mut s);
        // Clear only the primitive concealed form; the paid Aura now remains
        // the actual effective ability-removal authority on the printed source.
        s.objects.get_mut(source).v4.face_down_v1 = None;
        assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&s, source));
        priority(&mut s, actor);
        assert!(engine::step(&mut s, Action::ActivateAbility(source, 0)).is_err());
        walk(&mut s, actor.opponent(), Step::Main1, None);
        walk(&mut s, actor, Step::Main1, None);
        walk(&mut s, actor, Step::End, Some(source));
        drain(&mut s);
        assert!(foods(&s, actor).is_empty());
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&restored, source));
    }
}
