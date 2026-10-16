use super::*;
use crate::{
    card_def::{self, CardCapability, Keywords, Subtype},
    engine::{self, Action, Decision},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Target, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    for player in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..6 {
            for land in ["Swamp", "Mountain", "Island", "Forest", "Plains"] {
                put(&mut state, player, land, Zone::Battlefield);
            }
        }
    }
    state
}

fn pass(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected Valkyrie's Call decision: {other:?}"),
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
        pass(state, decision);
    }
    panic!("Valkyrie's Call fixture did not settle");
}

fn paid_cast(state: &mut GameState, actor: PlayerId, spell: ObjectId, target: Option<ObjectId>) {
    let decision = (0..12)
        .find_map(|_| {
            let decision = engine::advance_until_decision(state);
            if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
                Some(decision)
            } else {
                pass(state, decision);
                None
            }
        })
        .expect("the caster receives priority");
    assert!(
        matches!(decision, Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        let choice = engine::advance_until_decision(state);
        assert!(
            matches!(choice, Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    settle(state);
}

fn cast_until_call_trigger(
    state: &mut GameState,
    actor: PlayerId,
    spell: ObjectId,
    target: ObjectId,
    call: ObjectId,
) {
    let decision = (0..12)
        .find_map(|_| {
            let decision = engine::advance_until_decision(state);
            if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
                Some(decision)
            } else {
                pass(state, decision);
                None
            }
        })
        .expect("the removal spell caster receives priority");
    assert!(
        matches!(decision, Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    let choice = engine::advance_until_decision(state);
    assert!(
        matches!(choice, Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    for _ in 0..40 {
        let decision = engine::advance_until_decision(state);
        assert!(state.engine.halted.is_none());
        if state.stack.iter().any(|item| {
            item.source == call
                && matches!(
                    item.inline_effect.as_ref(),
                    Some(crate::effect::EffectOp::ReturnValkyriesCallCreatureV1 { .. })
                )
        }) {
            return;
        }
        pass(state, decision);
    }
    panic!("Valkyrie's Call trigger was not placed on the stack");
}

fn assert_effect_only_call_tampering_is_rejected(state: &GameState, call: ObjectId) {
    let tamper = |mut state: GameState, edit: &dyn Fn(&mut crate::effect::EffectOp)| {
        let item_index = state
            .stack
            .iter()
            .position(|item| {
                item.source == call
                    && matches!(
                        item.inline_effect.as_ref(),
                        Some(crate::effect::EffectOp::ReturnValkyriesCallCreatureV1 { .. })
                    )
            })
            .expect("the genuine Call trigger remains on the stack");
        {
            let item = &mut state.stack[item_index];
            edit(item.inline_effect.as_mut().unwrap());
        }
        assert!(
            engine::validated_stack_item_target_spec(&state.stack[item_index], &state).is_err()
        );
    };
    tamper(state.clone(), &|effect| {
        if let crate::effect::EffectOp::ReturnValkyriesCallCreatureV1 { graveyard, .. } = effect {
            graveyard.object = call;
        }
    });
    tamper(state.clone(), &|effect| {
        if let crate::effect::EffectOp::ReturnValkyriesCallCreatureV1 { owner, .. } = effect {
            *owner = (*owner).opponent();
        }
    });
    tamper(state.clone(), &|effect| {
        if let crate::effect::EffectOp::ReturnValkyriesCallCreatureV1 {
            capture_history_index,
            ..
        } = effect
        {
            *capture_history_index += 1;
        }
    });
    tamper(state.clone(), &|effect| {
        if let crate::effect::EffectOp::ReturnValkyriesCallCreatureV1 { source, .. } = effect {
            source.controller = source.controller.opponent();
        }
    });
}

#[test]
fn paid_controlled_death_returns_physical_owner_with_entry_replacements_and_characteristics_both_seats(
) {
    assert_eq!(card_def::card_id_by_name("Valkyrie's Call"), Some(459));
    let definition = &card_def::CARD_DEFS[459];
    assert_eq!(definition.capability, CardCapability::Partial);
    assert_eq!(definition.mana_value, 5);
    assert!(definition
        .types
        .contains(&crate::card_def::CardType::Enchantment));

    for actor in [PlayerId::P0, PlayerId::P1] {
        let opponent = actor.opponent();
        let mut state = ready(actor);
        // A creature spell must be cast during its owner's own main phase.
        // Keep the permanent under that opponent's control before the Call
        // controller takes their next turn.
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, opponent);
        let season = put(&mut state, opponent, "Doubling Season", Zone::Battlefield);
        let creature = put(&mut state, opponent, "Llanowar Elves", Zone::Hand);
        paid_cast(&mut state, opponent, creature, None);

        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor);
        let call = put(&mut state, actor, "Valkyrie's Call", Zone::Hand);
        paid_cast(&mut state, actor, call, None);
        assert_eq!(state.objects.get(call).zone, Zone::Battlefield);

        // The replacement belongs to the physical owner, who controls the
        // returned permanent even though the creature dies under the Call
        // controller's temporary control. Employment resolves on the Call
        // controller's own main phase, then the opponent uses Murder as an
        // instant before that turn ends, while the effect still controls it.
        let employment = put(&mut state, actor, "Involuntary Employment", Zone::Hand);
        paid_cast(&mut state, actor, employment, Some(creature));
        assert_eq!(state.objects.get(creature).owner, opponent);
        assert_eq!(state.objects.get(creature).controller, actor);

        let murder = put(&mut state, opponent, "Murder", Zone::Hand);
        cast_until_call_trigger(&mut state, opponent, murder, creature, call);
        assert_effect_only_call_tampering_is_rejected(&state, call);
        assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
        let expected_graveyard_generation = state.objects.get(creature).zone_change_count;
        assert_eq!(state.objects.get(creature).owner, opponent);
        assert_eq!(state.objects.get(creature).controller, opponent);
        assert_eq!(state.objects.get(season).zone, Zone::Battlefield);

        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.state_hash(), state.state_hash());
        assert_effect_only_call_tampering_is_rejected(&restored, call);
        settle(&mut state);
        settle(&mut restored);
        assert_eq!(state.state_hash(), restored.state_hash());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        let returned = state.objects.get(creature);
        assert_eq!(returned.zone, Zone::Battlefield);
        assert_eq!(returned.owner, opponent);
        assert_eq!(returned.controller, opponent);
        assert_eq!(
            returned.zone_change_count,
            expected_graveyard_generation + 1
        );
        assert_eq!(returned.counters.plus1_plus1, 2);
        assert!(crate::engine::has_effective_subtype(
            &state,
            creature,
            Subtype::Elf
        ));
        assert!(crate::engine::has_effective_subtype(
            &state,
            creature,
            Subtype::Angel
        ));
        assert!(crate::engine::has_effective_keyword(
            &state,
            creature,
            Keywords::FLYING
        ));
        assert!(state.engine.halted.is_none());
    }
}
