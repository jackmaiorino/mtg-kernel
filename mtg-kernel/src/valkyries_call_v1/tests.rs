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
        let call = put(&mut state, actor, "Valkyrie's Call", Zone::Hand);
        paid_cast(&mut state, actor, call, None);
        assert_eq!(state.objects.get(call).zone, Zone::Battlefield);

        // The replacement belongs to the physical owner, who controls the
        // returned permanent even though the creature dies under the Call
        // controller's temporary control.
        let season = put(&mut state, opponent, "Doubling Season", Zone::Battlefield);
        let creature = put(&mut state, opponent, "Llanowar Elves", Zone::Hand);
        paid_cast(&mut state, opponent, creature, None);
        let employment = put(&mut state, actor, "Involuntary Employment", Zone::Hand);
        paid_cast(&mut state, actor, employment, Some(creature));
        assert_eq!(state.objects.get(creature).owner, opponent);
        assert_eq!(state.objects.get(creature).controller, actor);

        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, opponent);
        let murder = put(&mut state, opponent, "Murder", Zone::Hand);
        cast_until_call_trigger(&mut state, opponent, murder, creature, call);
        assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
        let expected_graveyard_generation = state.objects.get(creature).zone_change_count;
        assert_eq!(state.objects.get(creature).owner, opponent);
        assert_eq!(state.objects.get(creature).controller, opponent);
        assert_eq!(state.objects.get(season).zone, Zone::Battlefield);

        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.state_hash(), state.state_hash());
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
