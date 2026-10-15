//! Genuine paid baseline Fake/Elf native-envelope fixtures. No new-family dependency.
use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v6::tests::put;
use crate::state::{GameState, StackItemKind, Target, Zone};

fn paid_on(
    mut state: GameState,
    actor: PlayerId,
    target: ObjectId,
) -> (GameState, ObjectId, ObjectId) {
    let spell = put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
    let definition = state.objects.get(spell).card_def;
    assert_eq!(
        CARD_DEFS[usize::from(definition)].capability,
        crate::card_def::CardCapability::Full
    );
    assert!(spell_custody::selected_definition(definition));
    assert!(crate::card_def::preflight_fully_supported_deck(&[definition]).is_ok());
    for root in [
        crate::rules_meaning_fake_your_own_death_v50::RootV50::InstantSpell,
        crate::rules_meaning_fake_your_own_death_v50::RootV50::GainedDiesAbility,
    ] {
        let meaning =
            crate::rules_meaning_fake_your_own_death_v50::printed_meaning(definition, root)
                .unwrap();
        crate::rules_meaning_fake_your_own_death_v50::validate(&meaning).unwrap();
        assert_eq!(meaning.capability, crate::card_def::CardCapability::Full);
        assert!(!meaning.model_admitted);
    }
    for _ in 0..2 {
        let swamp = put(&mut state, actor, "Swamp", Zone::Battlefield);
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, mana_abilities, .. }
            if player == actor && mana_abilities.contains(&swamp))
        );
        engine::step(&mut state, Action::ActivateManaAbility(swamp)).unwrap();
        assert!(state.objects.get(swamp).tapped);
    }
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
        2
    );
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { player, legal_targets, .. }
        if player == actor && legal_targets.contains(&Target::Object(target)))
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
        0
    );
    assert_eq!(state.stack.last().unwrap().source, spell);
    assert!(state.engine.event_history.iter().any(|event|
        matches!(event, CommittedEvent::FakeJournalV50 { journal } if matches!(journal.as_ref(), spell_custody::JournalV50::Published { item } if item.source == spell))));
    public_facts(&state);
    (state, spell, target)
}

fn public_facts(state: &GameState) -> public_v50::FactsV50 {
    let facts = public_v50::capture(state).unwrap();
    public_v50::validate(state, &facts).unwrap();
    let decoded: public_v50::FactsV50 =
        serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap();
    assert_eq!(facts, decoded);
    facts
}

fn drain(state: &mut GameState) {
    for _ in 0..24 {
        if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
            public_facts(state);
            return;
        }
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
            public_facts(state);
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual paid Fake stack did not finish")
}

fn death(state: &mut GameState, target: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Graveyard));
    assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        state.stack.last().unwrap().kind,
        StackItemKind::TriggeredAbility
    );
    assert!(matches!(
        state.stack.last().unwrap().inline_effect,
        Some(EffectOp::FakeDeathFromCaptureV50 { .. })
    ));
}

fn treasures(state: &GameState, player: PlayerId) -> usize {
    let definition = treasure_definition().unwrap();
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|object| {
            state.objects.get(**object).card_def == definition
                && state.objects.get(**object).v4.is_token
        })
        .count()
}

#[test]
fn actual_paid_fake_native_power_boundary_keeps_grant_death_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for owner in [actor, actor.opponent()] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let target = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            // Explicit shared counter-placement operand, not a claim of an
            // additional printed spell producing this many counters.
            event::add_plus_one_counters(&mut state, target, actor, i32::MAX - 3).unwrap();
            let (mut state, spell, target) = paid_on(state, actor, target);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                drain(branch);
                spell_custody::validate_retained_grant_history(branch).unwrap();
                let grants = spell_custody::installed_grants(branch).unwrap();
                assert_eq!(grants.len(), 1);
                assert!(grants[0].active(branch).unwrap());
                assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(
                    engine::checked_effective_pt(branch, target).unwrap(),
                    (i32::MAX, i32::MAX - 2)
                );
                public_facts(branch);
                death(branch, target);
                drain(branch);
                spell_custody::validate_retained_grant_history(branch).unwrap();
                assert_eq!(branch.objects.get(target).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(target).controller, owner);
                assert_eq!(branch.objects.get(target).counters.plus1_plus1, 0);
                assert_eq!(
                    engine::checked_effective_pt(branch, target).unwrap(),
                    (1, 1)
                );
                assert_eq!(treasures(branch, owner), 1);
                public_facts(branch);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn actual_paid_fake_overflow_refuses_before_pop_or_grant_and_restores_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for owner in [actor, actor.opponent()] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let target = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            event::add_plus_one_counters(&mut state, target, actor, i32::MAX - 2).unwrap();
            let (mut state, spell, target) = paid_on(state, actor, target);
            let binding = crate::effect::EffectObjectBinding {
                object: target,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: state.objects.get(target).zone_change_count,
            };
            let before = serde_json::to_vec(&state).unwrap();
            let mut primitive = state.clone();
            assert!(
                super::grant_primitive::primitive_install_grant_v50(&mut primitive, binding)
                    .is_err()
            );
            assert_eq!(serde_json::to_vec(&primitive).unwrap(), before);
            let original_stack = state.stack.clone();
            let history = state.engine.event_history.clone();
            let modifiers = state.engine.until_end_of_turn.clone();
            let timestamp = state.engine.next_effect_timestamp;
            let mut restored: GameState = serde_json::from_slice(&before).unwrap();
            for branch in [&mut state, &mut restored] {
                let mut halted = false;
                for _ in 0..8 {
                    match engine::advance_until_decision(branch) {
                        Decision::CastSpellOrPass { .. } => {
                            engine::step(branch, Action::Pass).unwrap()
                        }
                        Decision::Halted {
                            mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation,
                            source,
                        } if source == spell => {
                            halted = true;
                            break;
                        }
                        other => panic!("actual paid Fake overflow: {other:?}"),
                    }
                }
                assert!(halted, "real paid Fake must reach its resolving preflight");
                assert_eq!(branch.stack, original_stack);
                assert_eq!(branch.engine.event_history, history);
                assert_eq!(branch.engine.until_end_of_turn, modifiers);
                assert_eq!(branch.engine.next_effect_timestamp, timestamp);
                assert_eq!(branch.objects.get(spell).zone, Zone::Stack);
                assert_eq!(branch.objects.get(target).zone, Zone::Battlefield);
                assert_eq!(
                    engine::checked_effective_pt(branch, target).unwrap(),
                    (i32::MAX - 1, i32::MAX - 1)
                );
                assert!(spell_custody::installed_grants(branch).unwrap().is_empty());
                spell_custody::validate_retained_grant_history(branch).unwrap();
                public_facts(branch);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
