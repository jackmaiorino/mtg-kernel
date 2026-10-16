use super::*;
use crate::{
    card_def::{self, CardCapability},
    engine::{self, Action, Decision},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Target, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    for player in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..5 {
            put(&mut state, player, "Swamp", Zone::Battlefield);
            put(&mut state, player, "Mountain", Zone::Battlefield);
            put(&mut state, player, "Island", Zone::Battlefield);
        }
    }
    state
}

fn quiet(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected choice while settling Alesha fixture: {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none());
}

fn settle(state: &mut GameState) {
    for _ in 0..100 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(state, decision);
    }
    panic!("Alesha fixture did not settle");
}

fn announce_paid_cast(
    state: &mut GameState,
    actor: PlayerId,
    spell: ObjectId,
    target: Option<ObjectId>,
) {
    let cast_decision = (0..8)
        .find_map(|_| {
            let decision = engine::advance_until_decision(state);
            if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
                Some(decision)
            } else {
                quiet(state, decision);
                None
            }
        })
        .expect("actor receives a native casting priority window");
    let before = state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count();
    let mana_value = card_def::CARD_DEFS[state.objects.get(spell).card_def as usize].mana_value;
    assert!(
        matches!(cast_decision,Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        let decision = engine::advance_until_decision(state);
        assert!(
            matches!(decision,Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    let decision = engine::advance_until_decision(state);
    assert!(matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor));
    assert!(state.engine.pending_cast.is_none());
    assert!(state.engine.halted.is_none());
    let after = state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count();
    assert_eq!(after - before, usize::from(mana_value));
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
}

fn paid_cast(state: &mut GameState, actor: PlayerId, spell: ObjectId, target: Option<ObjectId>) {
    announce_paid_cast(state, actor, spell, target);
    settle(state);
}

fn resolve_top_spell(state: &mut GameState, first_priority: PlayerId, active_player: PlayerId) {
    let first = engine::advance_until_decision(state);
    assert!(matches!(first, Decision::CastSpellOrPass { player, .. } if player == first_priority));
    engine::step(state, Action::Pass).unwrap();
    let second = engine::advance_until_decision(state);
    assert!(
        matches!(second, Decision::CastSpellOrPass { player, .. } if player == first_priority.opponent())
    );
    engine::step(state, Action::Pass).unwrap();
    let next = engine::advance_until_decision(state);
    assert!(matches!(next, Decision::CastSpellOrPass { player, .. } if player == active_player));
    assert!(state.engine.halted.is_none());
}

fn reach_next_actor_attack(state: &mut GameState, actor: PlayerId, previous_turn: u32) {
    for _ in 0..240 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::DeclareAttackers { player, .. } if player == actor && state.turn > previous_turn)
        {
            return;
        }
        quiet(state, decision);
    }
    panic!("did not reach Alesha controller's next attack step");
}

#[test]
fn paid_alesha_attack_counter_enables_mv_three_raid_return_after_restore_both_seats() {
    let id = card_def::card_id_by_name("Alesha, Who Laughs at Fate").unwrap();
    assert_eq!(id, 457);
    let definition = &card_def::CARD_DEFS[usize::from(id)];
    assert_eq!(definition.capability, CardCapability::Partial);
    assert_eq!((definition.power, definition.toughness), (Some(2), Some(2)));
    assert!(definition
        .keywords
        .has(crate::card_def::Keywords::FIRST_STRIKE));

    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let initial_turn = state.turn;
        let alesha = put(&mut state, actor, "Alesha, Who Laughs at Fate", Zone::Hand);
        let nighthawk = put(&mut state, actor, "Vampire Nighthawk", Zone::Hand);
        paid_cast(&mut state, actor, alesha, None);
        let alesha_generation = state.objects.get(alesha).zone_change_count;
        paid_cast(&mut state, actor, nighthawk, None);
        assert_eq!(state.objects.get(alesha).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(nighthawk).zone, Zone::Battlefield);

        let abrade = put(&mut state, actor, "Abrade", Zone::Hand);
        paid_cast(&mut state, actor, abrade, Some(nighthawk));
        assert_eq!(state.objects.get(nighthawk).zone, Zone::Graveyard);
        let nighthawk_generation = state.objects.get(nighthawk).zone_change_count;

        reach_next_actor_attack(&mut state, actor, initial_turn);
        engine::step(&mut state, Action::DeclareAttackers(vec![alesha])).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(alesha).counters.plus1_plus1, 1);
        assert_eq!(engine::effective_power(&state, alesha), 3);

        let _raid_prompt = (0..240)
            .find_map(|_| {
                let decision = engine::advance_until_decision(&mut state);
                if matches!(&decision, Decision::ChooseTargets { player, legal_targets, .. }
                    if *player == actor && legal_targets.contains(&Target::Object(nighthawk)))
                {
                    Some(decision)
                } else {
                    quiet(&mut state, decision);
                    None
                }
            })
            .expect(
                "Raid chooses the actual MV-3 graveyard creature after Alesha's power rose to 3",
            );
        let frozen_prompt = serde_json::to_vec(&state).unwrap();
        let mut replay: GameState = serde_json::from_slice(&frozen_prompt).unwrap();
        let action = Action::ChooseTarget(Target::Object(nighthawk));
        engine::step(&mut state, action.clone()).unwrap();
        engine::step(&mut replay, action).unwrap();
        let mut shrink_state = state.clone();
        let mut reentry_state = state.clone();

        // A paid response shrinks the live original from 3 power to 2. The
        // already-selected MV-3 creature is illegal when Raid resolves.
        let distraction = put(
            &mut shrink_state,
            actor.opponent(),
            "Fleeting Distraction",
            Zone::Hand,
        );
        paid_cast(
            &mut shrink_state,
            actor.opponent(),
            distraction,
            Some(alesha),
        );
        settle(&mut shrink_state);
        assert_eq!(engine::effective_power(&shrink_state, alesha), 2);
        assert_eq!(shrink_state.objects.get(nighthawk).zone, Zone::Graveyard);

        // A genuine Fake Your Own Death cast and paid Abrade response remove
        // and return Alesha. Raid remains bound to the departing generation.
        let fake_death = put(&mut reentry_state, actor, "Fake Your Own Death", Zone::Hand);
        announce_paid_cast(&mut reentry_state, actor, fake_death, Some(alesha));
        resolve_top_spell(&mut reentry_state, actor, actor);
        let reentry_abrade = put(&mut reentry_state, actor.opponent(), "Abrade", Zone::Hand);
        paid_cast(
            &mut reentry_state,
            actor.opponent(),
            reentry_abrade,
            Some(alesha),
        );
        assert_eq!(reentry_state.objects.get(alesha).zone, Zone::Battlefield);
        assert_eq!(
            reentry_state.objects.get(alesha).zone_change_count,
            alesha_generation + 2
        );
        assert_eq!(engine::effective_power(&reentry_state, alesha), 2);
        assert_eq!(reentry_state.objects.get(alesha).counters.plus1_plus1, 0);
        assert_eq!(reentry_state.objects.get(nighthawk).zone, Zone::Battlefield);

        // A paid Abrade response removes Alesha without a return effect. The
        // Raid target remains legal under original 3-power LKI.
        let bolt = put(&mut state, actor.opponent(), "Abrade", Zone::Hand);
        let replay_bolt = put(&mut replay, actor.opponent(), "Abrade", Zone::Hand);
        paid_cast(&mut state, actor.opponent(), bolt, Some(alesha));
        paid_cast(&mut replay, actor.opponent(), replay_bolt, Some(alesha));
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.state_hash(), replay.state_hash());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(state.objects.get(nighthawk).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(nighthawk).owner, actor);
        assert_eq!(state.objects.get(nighthawk).controller, actor);
        assert_eq!(
            state.objects.get(nighthawk).zone_change_count,
            nighthawk_generation + 1
        );
        assert_eq!(state.objects.get(alesha).counters.plus1_plus1, 0);
        assert_eq!(state.objects.get(alesha).zone, Zone::Graveyard);
    }
}

#[test]
fn paid_raid_uses_original_source_lki_after_its_own_death_return_both_seats() {
    let id = card_def::card_id_by_name("Alesha, Who Laughs at Fate").unwrap();
    let target_def = card_def::card_id_by_name("Dragon Trainer").unwrap();
    assert_eq!(card_def::CARD_DEFS[usize::from(target_def)].mana_value, 5);

    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let initial_turn = state.turn;
        let alesha = put(&mut state, actor, "Alesha, Who Laughs at Fate", Zone::Hand);
        let trainer = put(&mut state, actor, "Dragon Trainer", Zone::Hand);
        paid_cast(&mut state, actor, alesha, None);
        paid_cast(&mut state, actor, trainer, None);

        let trainer_abrade = put(&mut state, actor.opponent(), "Abrade", Zone::Hand);
        paid_cast(&mut state, actor.opponent(), trainer_abrade, Some(trainer));
        assert_eq!(state.objects.get(trainer).zone, Zone::Graveyard);

        let mut previous_turn = initial_turn;
        for expected_counters in 1..=3 {
            reach_next_actor_attack(&mut state, actor, previous_turn);
            previous_turn = state.turn;
            engine::step(&mut state, Action::DeclareAttackers(vec![alesha])).unwrap();
            settle(&mut state);
            assert_eq!(
                state.objects.get(alesha).counters.plus1_plus1,
                expected_counters
            );
        }
        assert_eq!(engine::effective_power(&state, alesha), 5);

        let mut chose_raid_target = false;
        for _ in 0..240 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(&decision, Decision::ChooseTargets { player, legal_targets, .. }
                if *player == actor && legal_targets.contains(&Target::Object(trainer)))
            {
                engine::step(&mut state, Action::ChooseTarget(Target::Object(trainer))).unwrap();
                chose_raid_target = true;
                break;
            }
            quiet(&mut state, decision);
        }
        assert!(
            chose_raid_target,
            "Raid selects the actual MV-5 card at power 5"
        );
        assert!(state.stack.last().is_some_and(|item| {
            item.inline_effect
                .as_ref()
                .is_some_and(|effect| matches!(effect, EffectOp::AleshaRaidReturn { .. }))
        }));
        let raid_index = state
            .stack
            .iter()
            .position(|item| {
                item.inline_effect
                    .as_ref()
                    .is_some_and(|effect| matches!(effect, EffectOp::AleshaRaidReturn { .. }))
            })
            .unwrap();
        let original_effect = state.stack[raid_index].inline_effect.clone().unwrap();
        let captured_power = match original_effect {
            EffectOp::AleshaRaidReturn { source_power_lki } => source_power_lki,
            _ => unreachable!("the selected trigger is Alesha's Raid"),
        };
        assert_eq!(captured_power, 5);
        let original_generation = state.objects.get(alesha).zone_change_count;

        // Grant the real death-return ability. Its +2/+0 and four genuine
        // paid Distractions leave the original incarnation at power 3.
        let fake_death = put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
        announce_paid_cast(&mut state, actor, fake_death, Some(alesha));
        resolve_top_spell(&mut state, actor, actor);
        for _ in 0..4 {
            let distraction = put(
                &mut state,
                actor.opponent(),
                "Fleeting Distraction",
                Zone::Hand,
            );
            announce_paid_cast(&mut state, actor.opponent(), distraction, Some(alesha));
            resolve_top_spell(&mut state, actor.opponent(), actor);
        }
        assert_eq!(engine::effective_power(&state, alesha), 3);

        // Murder kills independently of Alesha's current toughness. The
        // committed Alesha departure receipt freezes the actual power 3.
        let murder = put(&mut state, actor.opponent(), "Murder", Zone::Hand);
        announce_paid_cast(&mut state, actor.opponent(), murder, Some(alesha));
        resolve_top_spell(&mut state, actor.opponent(), actor);
        assert_eq!(state.objects.get(alesha).zone, Zone::Graveyard);
        assert!(state.engine.event_history.iter().any(|event| {
            matches!(event, crate::event::CommittedEvent::AleshaPowerLkiV1 {
                source, card_def, zone_change_count, power, ..
            } if *source == alesha
                && *card_def == id
                && *zone_change_count == original_generation
                && *power == 3)
        }));

        // An effect-only edit cannot replace the engine-produced receipt.
        let source_contract = state.stack[raid_index].v4.ability_source_contract.unwrap();
        assert_eq!(
            engine::alesha_raid_source_power(&state, source_contract),
            Some(3)
        );
        state.stack[raid_index].inline_effect = Some(EffectOp::AleshaRaidReturn {
            source_power_lki: 4,
        });
        let forged_item = state.stack[raid_index].clone();
        assert!(engine::validated_stack_item_target_spec(&forged_item, &state).is_err());
        state.stack[raid_index].inline_effect = Some(EffectOp::AleshaRaidReturn {
            source_power_lki: 3,
        });

        // Resolve the genuine granted death trigger, which returns Alesha
        // tapped and creates a Treasure. It does not preserve old counters or
        // the old incarnation's temporary pump.
        resolve_top_spell(&mut state, actor, actor);
        assert_eq!(state.objects.get(alesha).zone, Zone::Battlefield);
        assert_eq!(
            state.objects.get(alesha).zone_change_count,
            original_generation + 2
        );
        assert!(state.objects.get(alesha).tapped);
        assert_eq!(state.objects.get(alesha).counters.plus1_plus1, 0);
        assert_eq!(engine::effective_power(&state, alesha), 2);
        let treasure = card_def::card_id_by_name("Treasure Token").unwrap();
        assert!(state.players[actor.index()]
            .battlefield
            .iter()
            .any(|&object| { state.objects.get(object).card_def == treasure }));

        // A real post-return Giant Growth makes the new incarnation power 5,
        // but the waiting Raid still uses the departed source's power 3.
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let growth = put(&mut state, actor, "Giant Growth", Zone::Hand);
        announce_paid_cast(&mut state, actor, growth, Some(alesha));
        resolve_top_spell(&mut state, actor, actor);
        assert_eq!(engine::effective_power(&state, alesha), 5);
        settle(&mut state);
        // The waiting Raid's MV-5 target is illegal at LKI power 3.
        assert_eq!(state.objects.get(trainer).zone, Zone::Graveyard);
    }
}
