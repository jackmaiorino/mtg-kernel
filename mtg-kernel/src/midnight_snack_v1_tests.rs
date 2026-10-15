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

#[test]
fn exact_selector_and_unowned_execution_refuse_without_life_mutation() {
    let (mut s, source) = paid(PlayerId::P0);
    let d = CARD_DEFS[452].clone();
    let mut full = d.clone();
    full.capability = CardCapability::Full;
    assert!(exact_definition(&full));
    let mut changed = d.clone();
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
        assert!(acquire(&s, item).is_err());
        let before = s.players[actor.opponent().index()].life;
        let journal = s.engine.event_history.clone();
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(acquire(&restored, restored.stack.last().unwrap()).is_err());
        assert_eq!(restored.players[actor.opponent().index()].life, before);
        assert_eq!(restored.engine.event_history, journal);
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
